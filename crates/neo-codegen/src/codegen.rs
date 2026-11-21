use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, DataSection, ElementSection, Elements, EntityType,
    ExportKind, ExportSection, Function, FunctionSection, GlobalSection, GlobalType, ImportSection,
    Instruction as I, MemArg, MemArgTarget, MemorySection, MemoryType, Module, RefType,
    TableSection, TableType, TypeSection, ValType,
};
use wit_component::{ComponentEncoder, StringEncoding, embed_component_metadata};
use wit_parser::{Resolve, UnresolvedPackageGroup, WorldId};

use crate::core::{PrimLit, PrimOp, PrimTy, RcTm, Tm};
use crate::env::Globals;
use crate::eval::{apply_pi, eval, eval_closure, force};
use crate::value::{V, Val, vvar};
use neo_common::error::{Error, Result};

/// The tag stored in the first word of a closure block. Constructor indices are
/// small, so this cannot collide with an enum tag.
const TAG_CLOSURE: i32 = 0x7fff_ffffu32 as i32;

/// Reserved low-memory scratch space for WASI return areas. The decimal
/// conversion buffer ends at 96, so these areas start above it.
const RETPTR: i32 = 0;
const ARGV_RET: i32 = 256;
const DIRS_RET: i32 = 272;
const OPEN_RET: i32 = 288;
const READ_RET: i32 = 304;
const WRITE_RET: i32 = 320;
const DIGIT_END: i32 = 96;

/// Static strings are placed here, above the scratch area.
const DATA_BASE: u32 = 1024;

/// The number of imported WASI functions. Function indices start after these.
const NUM_IMPORTS: u32 = 14;

const HEAP_START: u32 = 65_536;

const IMPORT_GET_STDOUT: u32 = 0;
const IMPORT_GET_STDERR: u32 = 1;
const IMPORT_WRITE_FLUSH: u32 = 2;
const IMPORT_DROP_ERROR: u32 = 3;
const IMPORT_DROP_OUTPUT_STREAM: u32 = 4;
const IMPORT_GET_ARGUMENTS: u32 = 5;
const IMPORT_GET_STDIN: u32 = 6;
const IMPORT_INPUT_READ: u32 = 7;
const IMPORT_DROP_INPUT_STREAM: u32 = 8;
const IMPORT_GET_DIRECTORIES: u32 = 9;
const IMPORT_OPEN_AT: u32 = 10;
const IMPORT_DESCRIPTOR_READ: u32 = 11;
const IMPORT_DESCRIPTOR_WRITE: u32 = 12;
const IMPORT_DROP_DESCRIPTOR: u32 = 13;

const NEO_WIT: &str = include_str!("../wit/neo.wit");
const WIT_IO: &str = include_str!("../wit/deps/io.wit");
const WIT_CLOCKS: &str = include_str!("../wit/deps/clocks.wit");
const WIT_RANDOM: &str = include_str!("../wit/deps/random.wit");
const WIT_FILESYSTEM: &str = include_str!("../wit/deps/filesystem.wit");
const WIT_HTTP: &str = include_str!("../wit/deps/http.wit");
const WIT_SOCKETS: &str = include_str!("../wit/deps/sockets.wit");
const WIT_CLI: &str = include_str!("../wit/deps/cli.wit");

/// A local or a field loaded from a pointer-holding local.
#[derive(Clone, Copy)]
struct Place {
    local: u32,
    offset: Option<u32>,
}

struct FuncSlot {
    ty: u32,
    body: Option<Function>,
}

#[derive(Clone, Copy)]
enum FnRef {
    /// A first-class function closure; the value is the table slot.
    Closure(u32),
    /// A zero-argument function; the value is the function id.
    Zero(usize),
}

fn mem(offset: u32) -> MemArg {
    MemArg {
        offset: offset as u64,
        align: 2,
        target: MemArgTarget::Memory(0),
        ordering: None,
    }
}

fn mem8(offset: u32) -> MemArg {
    MemArg {
        offset: offset as u64,
        align: 0,
        target: MemArgTarget::Memory(0),
        ordering: None,
    }
}

fn defined(id: usize) -> u32 {
    NUM_IMPORTS + id as u32
}

/// The constructor index of `ctor_name` in `enum_name`, or an internal error
/// when the prelude is missing it.
fn ctor_index(globals: &Globals, enum_name: &str, ctor_name: &str) -> Result<usize> {
    globals
        .get_enum(enum_name)
        .and_then(|info| info.ctors.iter().position(|c| c.name == ctor_name))
        .ok_or_else(|| {
            Error::new(format!(
                "internal: the prelude is missing `{}` constructor `{}`",
                enum_name, ctor_name
            ))
        })
}

struct Codegen<'g> {
    globals: &'g Globals,
    types: Vec<(Vec<ValType>, Vec<ValType>)>,
    type_ids: HashMap<(Vec<ValType>, Vec<ValType>), u32>,
    funcs: Vec<FuncSlot>,
    /// Table slot -> defined function index.
    table_funcs: Vec<u32>,

    fn_slots: HashMap<String, u32>,
    fn_zero: HashMap<String, usize>,

    /// (enum name, constructor index, arguments already consumed) -> table slot.
    ctor_steps: HashMap<(String, usize, usize), u32>,

    /// A rendered type -> printer function id.
    printers: HashMap<String, usize>,

    data: Vec<u8>,
    strings: HashMap<String, (i32, i32)>,
    literals: HashMap<Vec<u8>, i32>,

    alloc_id: usize,
    memcpy_id: usize,
    slice_id: usize,
    concat_id: usize,
    eq_id: usize,
    get_id: usize,
    utf8_id: usize,
    print_bytes_id: usize,
    print_u32_id: usize,
    print_nat_id: usize,
    print_bool_id: usize,
    print_unit_id: usize,
    print_str_id: usize,
    print_literal_bytes_id: usize,
    write_stream_id: usize,
    realloc_id: usize,
    run_id: usize,

    succ_index: i32,
}

impl<'g> Codegen<'g> {
    fn new(globals: &'g Globals) -> Self {
        let succ_index = globals
            .get_enum("Nat")
            .and_then(|e| e.ctors.iter().position(|c| c.name == "Succ"))
            .unwrap_or(1) as i32;
        Self {
            globals,
            types: Vec::new(),
            type_ids: HashMap::new(),
            funcs: Vec::new(),
            table_funcs: Vec::new(),
            fn_slots: HashMap::new(),
            fn_zero: HashMap::new(),
            ctor_steps: HashMap::new(),
            printers: HashMap::new(),
            data: Vec::new(),
            strings: HashMap::new(),
            literals: HashMap::new(),
            alloc_id: 0,
            memcpy_id: 0,
            slice_id: 0,
            concat_id: 0,
            eq_id: 0,
            get_id: 0,
            utf8_id: 0,
            print_bytes_id: 0,
            print_u32_id: 0,
            print_nat_id: 0,
            print_bool_id: 0,
            print_unit_id: 0,
            print_str_id: 0,
            print_literal_bytes_id: 0,
            write_stream_id: 0,
            realloc_id: 0,
            run_id: 0,
            succ_index,
        }
    }

    fn ty(&mut self, params: &[ValType], results: &[ValType]) -> u32 {
        let key = (params.to_vec(), results.to_vec());
        if let Some(&id) = self.type_ids.get(&key) {
            return id;
        }
        let id = self.types.len() as u32;
        self.types.push(key.clone());
        self.type_ids.insert(key, id);
        id
    }

    fn new_func(&mut self, params: &[ValType], results: &[ValType]) -> usize {
        let ty = self.ty(params, results);
        self.funcs.push(FuncSlot { ty, body: None });
        self.funcs.len() - 1
    }

    fn set_body(&mut self, id: usize, body: Function) {
        self.funcs[id].body = Some(body);
    }

    fn closure_ty(&mut self) -> u32 {
        self.ty(&[ValType::I32, ValType::I32], &[ValType::I32])
    }

    /// Allocate both a WASM function and a table slot for a unary closure.
    fn new_lambda_func(&mut self) -> (usize, u32) {
        let id = self.new_func(&[ValType::I32, ValType::I32], &[ValType::I32]);
        let slot = self.table_funcs.len() as u32;
        self.table_funcs.push(defined(id));
        (id, slot)
    }

    fn intern(&mut self, s: &str) -> (i32, i32) {
        if let Some(&p) = self.strings.get(s) {
            return p;
        }
        let ptr = DATA_BASE + self.data.len() as u32;
        self.data.extend_from_slice(s.as_bytes());
        let p = (ptr as i32, s.len() as i32);
        self.strings.insert(s.to_string(), p);
        p
    }

    fn alloc_fn(&self) -> u32 {
        defined(self.alloc_id)
    }

    /// Intern a primitive `String` or `Bytes` literal and return the address of
    /// its descriptor. Equal bytes share one descriptor.
    fn intern_literal(&mut self, bytes: &[u8]) -> i32 {
        if let Some(&ptr) = self.literals.get(bytes) {
            return ptr;
        }
        let data_ptr = DATA_BASE + self.data.len() as u32;
        self.data.extend_from_slice(bytes);
        let desc_ptr = DATA_BASE + self.data.len() as u32;
        self.data.extend_from_slice(&data_ptr.to_le_bytes());
        self.data.extend_from_slice(&0u32.to_le_bytes());
        self.data
            .extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        self.literals.insert(bytes.to_vec(), desc_ptr as i32);
        desc_ptr as i32
    }

    fn print_bytes_fn(&self) -> u32 {
        defined(self.print_bytes_id)
    }

    fn print_u32_fn(&self) -> u32 {
        defined(self.print_u32_id)
    }

    fn init_builtins(&mut self) -> Result<()> {
        self.alloc_id = self.new_func(&[ValType::I32], &[ValType::I32]);
        self.print_bytes_id = self.new_func(&[ValType::I32, ValType::I32], &[]);
        self.print_u32_id = self.new_func(&[ValType::I32], &[]);
        self.print_nat_id = self.new_func(&[ValType::I32], &[]);
        self.print_bool_id = self.new_func(&[ValType::I32], &[]);
        self.print_unit_id = self.new_func(&[ValType::I32], &[]);
        self.print_str_id = self.new_func(&[ValType::I32], &[]);
        self.print_literal_bytes_id = self.new_func(&[ValType::I32], &[]);
        self.write_stream_id =
            self.new_func(&[ValType::I32, ValType::I32, ValType::I32], &[ValType::I32]);
        self.realloc_id = self.new_func(&[ValType::I32; 4], &[ValType::I32]);
        self.run_id = self.new_func(&[], &[ValType::I32]);

        self.gen_alloc();
        self.gen_runtime();
        self.gen_realloc();
        self.gen_write_stream();
        self.gen_print_bytes();
        self.gen_print_u32();
        self.gen_print_nat();
        self.gen_print_bool();
        self.gen_print_unit();
        self.gen_print_str();
        self.gen_print_literal_bytes();
        Ok(())
    }

    fn gen_alloc(&mut self) {
        let mut fc = FnCompiler::new(self, 1);
        let ptr = fc.local();
        fc.emit(I::GlobalGet(0));
        fc.emit(I::LocalSet(ptr));
        fc.emit(I::GlobalGet(0));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Add);
        fc.emit(I::GlobalSet(0));

        // Grow the memory if the new heap end is above the current size.
        fc.emit(I::GlobalGet(0));
        fc.emit(I::MemorySize(0));
        fc.emit(I::I32Const(65_536));
        fc.emit(I::I32Mul);
        fc.emit(I::I32GtU);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::GlobalGet(0));
        fc.emit(I::MemorySize(0));
        fc.emit(I::I32Const(65_536));
        fc.emit(I::I32Mul);
        fc.emit(I::I32Sub);
        fc.emit(I::I32Const(65_535));
        fc.emit(I::I32Add);
        fc.emit(I::I32Const(16));
        fc.emit(I::I32ShrU);
        fc.emit(I::MemoryGrow(0));
        fc.emit(I::Drop);
        fc.emit(I::End);

        fc.emit(I::LocalGet(ptr));
        let body = fc.finish();
        let id = self.alloc_id;
        self.set_body(id, body);
    }

    fn write_stream_fn(&self) -> u32 {
        defined(self.write_stream_id)
    }

    /// Write a byte range to an output stream, chunking with
    /// `blocking-write-and-flush` and dropping a failed stream's error
    /// resource. Returns 0 on success, 1 when the stream is closed, and 2 for
    /// any other error.
    fn gen_write_stream(&mut self) {
        let mut fc = FnCompiler::new(self, 3);
        let handle = fc.local();
        let ptr = fc.local();
        let rem = fc.local();
        let chunk = fc.local();

        fc.emit(I::LocalGet(0));
        fc.emit(I::LocalSet(handle));
        fc.emit(I::LocalGet(1));
        fc.emit(I::LocalSet(ptr));
        fc.emit(I::LocalGet(2));
        fc.emit(I::LocalSet(rem));

        fc.emit(I::Block(BlockType::Empty));
        fc.emit(I::Loop(BlockType::Empty));
        fc.emit(I::LocalGet(rem));
        fc.emit(I::I32Eqz);
        fc.emit(I::BrIf(1));

        // chunk = min(rem, 4096)
        fc.emit(I::LocalGet(rem));
        fc.emit(I::I32Const(4096));
        fc.emit(I::I32LtU);
        fc.emit(I::If(BlockType::Result(ValType::I32)));
        fc.emit(I::LocalGet(rem));
        fc.emit(I::Else);
        fc.emit(I::I32Const(4096));
        fc.emit(I::End);
        fc.emit(I::LocalSet(chunk));

        fc.emit(I::LocalGet(handle));
        fc.emit(I::LocalGet(ptr));
        fc.emit(I::LocalGet(chunk));
        fc.emit(I::I32Const(RETPTR));
        fc.emit(I::Call(IMPORT_WRITE_FLUSH));

        fc.emit(I::I32Const(RETPTR));
        fc.emit(I::I32Load8U(mem8(0)));
        fc.emit(I::If(BlockType::Empty));
        // A non-`closed` stream error owns an `error` resource that must be
        // dropped before the status is returned.
        fc.emit(I::I32Const(RETPTR + 4));
        fc.emit(I::I32Load8U(mem8(0)));
        fc.emit(I::I32Eqz);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::I32Const(RETPTR + 8));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::Call(IMPORT_DROP_ERROR));
        fc.emit(I::I32Const(2));
        fc.emit(I::Return);
        fc.emit(I::Else);
        fc.emit(I::I32Const(1));
        fc.emit(I::Return);
        fc.emit(I::End);
        fc.emit(I::End);

        fc.emit(I::LocalGet(ptr));
        fc.emit(I::LocalGet(chunk));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(ptr));
        fc.emit(I::LocalGet(rem));
        fc.emit(I::LocalGet(chunk));
        fc.emit(I::I32Sub);
        fc.emit(I::LocalSet(rem));
        fc.emit(I::Br(0));
        fc.emit(I::End);
        fc.emit(I::End);

        fc.emit(I::I32Const(0));

        let body = fc.finish();
        let id = self.write_stream_id;
        self.set_body(id, body);
    }

    fn gen_print_bytes(&mut self) {
        let write_stream = self.write_stream_fn();
        let mut fc = FnCompiler::new(self, 2);
        let out = fc.local();
        fc.emit(I::Call(IMPORT_GET_STDOUT));
        fc.emit(I::LocalSet(out));
        fc.emit(I::LocalGet(out));
        fc.emit(I::LocalGet(0));
        fc.emit(I::LocalGet(1));
        fc.emit(I::Call(write_stream));
        fc.emit(I::Drop);
        fc.emit(I::LocalGet(out));
        fc.emit(I::Call(IMPORT_DROP_OUTPUT_STREAM));
        let body = fc.finish();
        let id = self.print_bytes_id;
        self.set_body(id, body);
    }

    fn gen_print_u32(&mut self) {
        let print_bytes = self.print_bytes_fn();
        let mut fc = FnCompiler::new(self, 1);
        let i = fc.local();

        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Eqz);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::I32Const(DIGIT_END));
        fc.emit(I::I32Const(b'0' as i32));
        fc.emit(I::I32Store8(mem8(0)));
        fc.emit(I::I32Const(DIGIT_END));
        fc.emit(I::I32Const(1));
        fc.emit(I::Call(print_bytes));
        fc.emit(I::Return);
        fc.emit(I::End);

        fc.emit(I::I32Const(DIGIT_END));
        fc.emit(I::LocalSet(i));
        fc.emit(I::Block(BlockType::Empty));
        fc.emit(I::Loop(BlockType::Empty));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Eqz);
        fc.emit(I::BrIf(1));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Sub);
        fc.emit(I::LocalSet(i));
        fc.emit(I::LocalGet(i));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Const(10));
        fc.emit(I::I32RemU);
        fc.emit(I::I32Const(b'0' as i32));
        fc.emit(I::I32Add);
        fc.emit(I::I32Store8(mem8(0)));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Const(10));
        fc.emit(I::I32DivU);
        fc.emit(I::LocalSet(0));
        fc.emit(I::Br(0));
        fc.emit(I::End);
        fc.emit(I::End);

        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Const(DIGIT_END));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Sub);
        fc.emit(I::Call(print_bytes));

        let body = fc.finish();
        let id = self.print_u32_id;
        self.set_body(id, body);
    }

    fn gen_print_nat(&mut self) {
        let succ = self.succ_index;
        let print_u32 = self.print_u32_fn();
        let mut fc = FnCompiler::new(self, 1);
        let count = fc.local();
        let cursor = fc.local();

        fc.emit(I::LocalGet(0));
        fc.emit(I::LocalSet(cursor));
        fc.emit(I::Block(BlockType::Empty));
        fc.emit(I::Loop(BlockType::Empty));
        fc.emit(I::LocalGet(cursor));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::I32Const(succ));
        fc.emit(I::I32Ne);
        fc.emit(I::BrIf(1));
        fc.emit(I::LocalGet(count));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(count));
        fc.emit(I::LocalGet(cursor));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::LocalSet(cursor));
        fc.emit(I::Br(0));
        fc.emit(I::End);
        fc.emit(I::End);

        fc.emit(I::LocalGet(count));
        fc.emit(I::Call(print_u32));

        let body = fc.finish();
        let id = self.print_nat_id;
        self.set_body(id, body);
    }

    fn gen_print_bool(&mut self) {
        let (t_ptr, t_len) = self.intern("True");
        let (f_ptr, f_len) = self.intern("False");
        let pb = self.print_bytes_fn();
        let mut fc = FnCompiler::new(self, 1);
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::I32Const(t_ptr));
        fc.emit(I::I32Const(t_len));
        fc.emit(I::Call(pb));
        fc.emit(I::Else);
        fc.emit(I::I32Const(f_ptr));
        fc.emit(I::I32Const(f_len));
        fc.emit(I::Call(pb));
        fc.emit(I::End);
        let body = fc.finish();
        let id = self.print_bool_id;
        self.set_body(id, body);
    }

    fn gen_print_unit(&mut self) {
        let (ptr, len) = self.intern("Unit");
        let pb = self.print_bytes_fn();
        let mut fc = FnCompiler::new(self, 1);
        fc.emit(I::I32Const(ptr));
        fc.emit(I::I32Const(len));
        fc.emit(I::Call(pb));
        let body = fc.finish();
        let id = self.print_unit_id;
        self.set_body(id, body);
    }

    /// Runtime helpers for the primitive `String` and `Bytes` types.
    ///
    /// A value is a pointer to a 12-byte descriptor `{ data, start, len }`.
    /// Descriptors and byte buffers are bump-allocated and never freed.
    fn gen_runtime(&mut self) {
        self.memcpy_id = self.new_func(&[ValType::I32; 3], &[]);
        self.slice_id = self.new_func(&[ValType::I32; 3], &[ValType::I32]);
        self.concat_id = self.new_func(&[ValType::I32; 2], &[ValType::I32]);
        self.eq_id = self.new_func(&[ValType::I32; 2], &[ValType::I32]);
        self.get_id = self.new_func(&[ValType::I32; 2], &[ValType::I32]);
        self.utf8_id = self.new_func(&[ValType::I32], &[ValType::I32]);

        self.gen_memcpy();
        self.gen_slice();
        self.gen_concat();
        self.gen_eq();
        self.gen_get();
        self.gen_utf8_check();
    }

    fn gen_realloc(&mut self) {
        let memcpy = defined(self.memcpy_id);
        let mut fc = FnCompiler::new(self, 4);
        let ptr = fc.local();
        let copy = fc.local();
        // A zero-sized allocation still returns an aligned, non-null pointer.
        fc.emit(I::LocalGet(3));
        fc.emit(I::I32Eqz);
        fc.emit(I::If(BlockType::Result(ValType::I32)));
        fc.emit(I::LocalGet(2));
        fc.emit(I::Else);
        fc.emit(I::GlobalGet(0));
        fc.emit(I::LocalGet(2));
        fc.emit(I::I32Add);
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Sub);
        fc.emit(I::I32Const(0));
        fc.emit(I::LocalGet(2));
        fc.emit(I::I32Sub);
        fc.emit(I::I32And);
        fc.emit(I::LocalSet(ptr));
        fc.emit(I::LocalGet(ptr));
        fc.emit(I::LocalGet(3));
        fc.emit(I::I32Add);
        fc.emit(I::GlobalSet(0));

        fc.emit(I::GlobalGet(0));
        fc.emit(I::MemorySize(0));
        fc.emit(I::I32Const(65_536));
        fc.emit(I::I32Mul);
        fc.emit(I::I32GtU);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::GlobalGet(0));
        fc.emit(I::MemorySize(0));
        fc.emit(I::I32Const(65_536));
        fc.emit(I::I32Mul);
        fc.emit(I::I32Sub);
        fc.emit(I::I32Const(65_535));
        fc.emit(I::I32Add);
        fc.emit(I::I32Const(16));
        fc.emit(I::I32ShrU);
        fc.emit(I::MemoryGrow(0));
        fc.emit(I::Drop);
        fc.emit(I::End);

        // A bump allocator cannot free, so only the live prefix is copied.
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Eqz);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::Else);
        fc.emit(I::LocalGet(1));
        fc.emit(I::LocalGet(3));
        fc.emit(I::I32LtU);
        fc.emit(I::If(BlockType::Result(ValType::I32)));
        fc.emit(I::LocalGet(1));
        fc.emit(I::Else);
        fc.emit(I::LocalGet(3));
        fc.emit(I::End);
        fc.emit(I::LocalSet(copy));
        fc.emit(I::LocalGet(ptr));
        fc.emit(I::LocalGet(0));
        fc.emit(I::LocalGet(copy));
        fc.emit(I::Call(memcpy));
        fc.emit(I::End);

        fc.emit(I::LocalGet(ptr));
        fc.emit(I::End);
        let body = fc.finish();
        let id = self.realloc_id;
        self.set_body(id, body);
    }

    fn gen_memcpy(&mut self) {
        let mut fc = FnCompiler::new(self, 3);
        fc.emit(I::Block(BlockType::Empty));
        fc.emit(I::Loop(BlockType::Empty));
        fc.emit(I::LocalGet(2));
        fc.emit(I::I32Eqz);
        fc.emit(I::BrIf(1));
        fc.emit(I::LocalGet(0));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Load8U(mem8(0)));
        fc.emit(I::I32Store8(mem8(0)));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(0));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(1));
        fc.emit(I::LocalGet(2));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Sub);
        fc.emit(I::LocalSet(2));
        fc.emit(I::Br(0));
        fc.emit(I::End);
        fc.emit(I::End);
        let body = fc.finish();
        let id = self.memcpy_id;
        self.set_body(id, body);
    }

    fn gen_slice(&mut self) {
        let alloc = self.alloc_fn();
        let mut fc = FnCompiler::new(self, 3);
        let len = fc.local();
        let out = fc.local();
        // `len = desc.len`
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(8)));
        fc.emit(I::LocalSet(len));
        emit_min_u(&mut fc, 1, len);
        emit_min_u(&mut fc, 2, len);
        // `if end < start { end = start }`
        fc.emit(I::LocalGet(2));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32LtU);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::LocalGet(1));
        fc.emit(I::LocalSet(2));
        fc.emit(I::End);
        // `out = alloc(12)`, then fill the descriptor.
        fc.emit(I::I32Const(12));
        fc.emit(I::Call(alloc));
        fc.emit(I::LocalSet(out));
        fc.emit(I::LocalGet(out));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::I32Store(mem(0)));
        fc.emit(I::LocalGet(out));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Add);
        fc.emit(I::I32Store(mem(4)));
        fc.emit(I::LocalGet(out));
        fc.emit(I::LocalGet(2));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Sub);
        fc.emit(I::I32Store(mem(8)));
        fc.emit(I::LocalGet(out));
        let body = fc.finish();
        let id = self.slice_id;
        self.set_body(id, body);
    }

    fn gen_concat(&mut self) {
        let alloc = self.alloc_fn();
        let memcpy = defined(self.memcpy_id);
        let mut fc = FnCompiler::new(self, 2);
        let alen = fc.local();
        let blen = fc.local();
        let total = fc.local();
        let buf = fc.local();
        let out = fc.local();
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(8)));
        fc.emit(I::LocalSet(alen));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Load(mem(8)));
        fc.emit(I::LocalSet(blen));
        fc.emit(I::LocalGet(alen));
        fc.emit(I::LocalGet(blen));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(total));
        fc.emit(I::LocalGet(total));
        fc.emit(I::Call(alloc));
        fc.emit(I::LocalSet(buf));
        // Copy the first buffer.
        fc.emit(I::LocalGet(buf));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::I32Add);
        fc.emit(I::LocalGet(alen));
        fc.emit(I::Call(memcpy));
        // Copy the second buffer after the first.
        fc.emit(I::LocalGet(buf));
        fc.emit(I::LocalGet(alen));
        fc.emit(I::I32Add);
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::I32Add);
        fc.emit(I::LocalGet(blen));
        fc.emit(I::Call(memcpy));
        fc.emit(I::I32Const(12));
        fc.emit(I::Call(alloc));
        fc.emit(I::LocalSet(out));
        fc.emit(I::LocalGet(out));
        fc.emit(I::LocalGet(buf));
        fc.emit(I::I32Store(mem(0)));
        fc.emit(I::LocalGet(out));
        fc.emit(I::I32Const(0));
        fc.emit(I::I32Store(mem(4)));
        fc.emit(I::LocalGet(out));
        fc.emit(I::LocalGet(total));
        fc.emit(I::I32Store(mem(8)));
        fc.emit(I::LocalGet(out));
        let body = fc.finish();
        let id = self.concat_id;
        self.set_body(id, body);
    }

    fn gen_eq(&mut self) {
        let mut fc = FnCompiler::new(self, 2);
        let alen = fc.local();
        let blen = fc.local();
        let ap = fc.local();
        let bp = fc.local();
        let i = fc.local();
        let byte = fc.local();
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(8)));
        fc.emit(I::LocalSet(alen));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Load(mem(8)));
        fc.emit(I::LocalSet(blen));
        fc.emit(I::LocalGet(alen));
        fc.emit(I::LocalGet(blen));
        fc.emit(I::I32Ne);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::I32Const(0));
        fc.emit(I::Return);
        fc.emit(I::End);
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(ap));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(bp));
        fc.emit(I::Block(BlockType::Empty));
        fc.emit(I::Loop(BlockType::Empty));
        fc.emit(I::LocalGet(i));
        fc.emit(I::LocalGet(alen));
        fc.emit(I::I32GeU);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::I32Const(1));
        fc.emit(I::Return);
        fc.emit(I::End);
        fc.emit(I::LocalGet(ap));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Add);
        fc.emit(I::I32Load8U(mem8(0)));
        fc.emit(I::LocalSet(byte));
        fc.emit(I::LocalGet(bp));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Add);
        fc.emit(I::I32Load8U(mem8(0)));
        fc.emit(I::LocalGet(byte));
        fc.emit(I::I32Ne);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::I32Const(0));
        fc.emit(I::Return);
        fc.emit(I::End);
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(i));
        fc.emit(I::Br(0));
        fc.emit(I::End);
        fc.emit(I::End);
        fc.emit(I::I32Const(0));
        let body = fc.finish();
        let id = self.eq_id;
        self.set_body(id, body);
    }

    fn gen_get(&mut self) {
        let mut fc = FnCompiler::new(self, 2);
        let len = fc.local();
        let data = fc.local();
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(8)));
        fc.emit(I::LocalSet(len));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(data));
        fc.emit(I::LocalGet(1));
        fc.emit(I::LocalGet(len));
        fc.emit(I::I32GeU);
        fc.emit(I::If(BlockType::Result(ValType::I32)));
        fc.emit(I::I32Const(0));
        fc.emit(I::Else);
        fc.emit(I::LocalGet(data));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Add);
        fc.emit(I::I32Load8U(mem8(0)));
        fc.emit(I::End);
        let body = fc.finish();
        let id = self.get_id;
        self.set_body(id, body);
    }

    /// Validate a descriptor's bytes as UTF-8. Returns `0` when the bytes are
    /// valid, otherwise the 1-based index of the first byte of the first
    /// invalid sequence. Validation is strict: overlong forms, surrogates,
    /// code points above `U+10FFFF`, and truncated sequences are rejected.
    fn gen_utf8_check(&mut self) {
        let mut fc = FnCompiler::new(self, 1);
        let len = fc.local();
        let data = fc.local();
        let i = fc.local();
        let b = fc.local();
        let n = fc.local();
        let lo = fc.local();
        let hi = fc.local();
        let j = fc.local();
        let c = fc.local();

        // len and data start.
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(8)));
        fc.emit(I::LocalSet(len));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(data));
        fc.emit(I::I32Const(0));
        fc.emit(I::LocalSet(i));

        fc.emit(I::Loop(BlockType::Empty));
        // End of input: valid.
        fc.emit(I::LocalGet(i));
        fc.emit(I::LocalGet(len));
        fc.emit(I::I32GeU);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::I32Const(0));
        fc.emit(I::Return);
        fc.emit(I::End);

        // b = bytes[i]
        fc.emit(I::LocalGet(data));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Add);
        fc.emit(I::I32Load8U(mem8(0)));
        fc.emit(I::LocalSet(b));

        // ASCII fast path.
        fc.emit(I::LocalGet(b));
        fc.emit(I::I32Const(0x80));
        fc.emit(I::I32LtU);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(i));
        fc.emit(I::Br(1));
        fc.emit(I::End);

        // n defaults to 1 (invalid); lo/hi bound the second byte.
        fc.emit(I::I32Const(1));
        fc.emit(I::LocalSet(n));
        fc.emit(I::I32Const(0x80));
        fc.emit(I::LocalSet(lo));
        fc.emit(I::I32Const(0xbf));
        fc.emit(I::LocalSet(hi));

        for (from, to, width) in [(0xc2, 0xdf, 2), (0xe0, 0xef, 3), (0xf0, 0xf4, 4)] {
            fc.emit(I::LocalGet(b));
            fc.emit(I::I32Const(from));
            fc.emit(I::I32GeU);
            fc.emit(I::LocalGet(b));
            fc.emit(I::I32Const(to));
            fc.emit(I::I32LeU);
            fc.emit(I::I32And);
            fc.emit(I::If(BlockType::Empty));
            fc.emit(I::I32Const(width));
            fc.emit(I::LocalSet(n));
            fc.emit(I::End);
        }

        // No valid lead byte for this class.
        fc.emit(I::LocalGet(n));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Eq);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::Return);
        fc.emit(I::End);

        // Narrow the second byte range for the constrained lead bytes.
        for (lead, new_lo, new_hi) in [
            (0xe0, Some(0xa0), None),
            (0xed, None, Some(0x9f)),
            (0xf0, Some(0x90), None),
            (0xf4, None, Some(0x8f)),
        ] {
            fc.emit(I::LocalGet(b));
            fc.emit(I::I32Const(lead));
            fc.emit(I::I32Eq);
            fc.emit(I::If(BlockType::Empty));
            if let Some(v) = new_lo {
                fc.emit(I::I32Const(v));
                fc.emit(I::LocalSet(lo));
            }
            if let Some(v) = new_hi {
                fc.emit(I::I32Const(v));
                fc.emit(I::LocalSet(hi));
            }
            fc.emit(I::End);
        }

        // Truncated sequence.
        fc.emit(I::LocalGet(i));
        fc.emit(I::LocalGet(n));
        fc.emit(I::I32Add);
        fc.emit(I::LocalGet(len));
        fc.emit(I::I32GtU);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::Return);
        fc.emit(I::End);

        // Second byte.
        fc.emit(I::LocalGet(data));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Add);
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::I32Load8U(mem8(0)));
        fc.emit(I::LocalSet(c));
        fc.emit(I::LocalGet(c));
        fc.emit(I::LocalGet(lo));
        fc.emit(I::I32LtU);
        fc.emit(I::LocalGet(c));
        fc.emit(I::LocalGet(hi));
        fc.emit(I::I32GtU);
        fc.emit(I::I32Or);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::Return);
        fc.emit(I::End);

        // Remaining continuation bytes.
        fc.emit(I::I32Const(2));
        fc.emit(I::LocalSet(j));
        fc.emit(I::Block(BlockType::Empty));
        fc.emit(I::Loop(BlockType::Empty));
        fc.emit(I::LocalGet(j));
        fc.emit(I::LocalGet(n));
        fc.emit(I::I32GeU);
        fc.emit(I::BrIf(1));
        fc.emit(I::LocalGet(data));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Add);
        fc.emit(I::LocalGet(j));
        fc.emit(I::I32Add);
        fc.emit(I::I32Load8U(mem8(0)));
        fc.emit(I::LocalSet(c));
        fc.emit(I::LocalGet(c));
        fc.emit(I::I32Const(0x80));
        fc.emit(I::I32LtU);
        fc.emit(I::LocalGet(c));
        fc.emit(I::I32Const(0xbf));
        fc.emit(I::I32GtU);
        fc.emit(I::I32Or);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::Return);
        fc.emit(I::End);
        fc.emit(I::LocalGet(j));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(j));
        fc.emit(I::Br(0));
        fc.emit(I::End);
        fc.emit(I::End);

        // Advance past the validated sequence.
        fc.emit(I::LocalGet(i));
        fc.emit(I::LocalGet(n));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(i));
        fc.emit(I::Br(0));
        fc.emit(I::End);

        fc.emit(I::I32Const(0));
        let body = fc.finish();
        let id = self.utf8_id;
        self.set_body(id, body);
    }

    fn gen_print_str(&mut self) {
        let print_bytes = self.print_bytes_fn();
        let mut fc = FnCompiler::new(self, 1);
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::I32Add);
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(8)));
        fc.emit(I::Call(print_bytes));
        let body = fc.finish();
        let id = self.print_str_id;
        self.set_body(id, body);
    }

    fn gen_print_literal_bytes(&mut self) {
        let print_bytes = self.print_bytes_fn();
        let (open_ptr, open_len) = self.intern("b\"");
        let (quote_ptr, quote_len) = self.intern("\"");
        let (escape_ptr, escape_len) = self.intern("\\x");
        let mut fc = FnCompiler::new(self, 1);
        let len = fc.local();
        let data = fc.local();
        let i = fc.local();
        let byte = fc.local();
        let nib = fc.local();
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(8)));
        fc.emit(I::LocalSet(len));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::LocalGet(0));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(data));
        fc.emit(I::I32Const(open_ptr));
        fc.emit(I::I32Const(open_len));
        fc.emit(I::Call(print_bytes));
        fc.emit(I::Block(BlockType::Empty));
        fc.emit(I::Loop(BlockType::Empty));
        fc.emit(I::LocalGet(i));
        fc.emit(I::LocalGet(len));
        fc.emit(I::I32GeU);
        fc.emit(I::BrIf(1));
        fc.emit(I::LocalGet(data));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Add);
        fc.emit(I::I32Load8U(mem8(0)));
        fc.emit(I::LocalSet(byte));
        fc.emit(I::LocalGet(byte));
        fc.emit(I::I32Const(4));
        fc.emit(I::I32ShrU);
        fc.emit(I::LocalSet(nib));
        fc.emit(I::I32Const(escape_ptr));
        fc.emit(I::I32Const(escape_len));
        fc.emit(I::Call(print_bytes));
        emit_hex_nibble(&mut fc, nib, DIGIT_END);
        fc.emit(I::LocalGet(byte));
        fc.emit(I::I32Const(15));
        fc.emit(I::I32And);
        fc.emit(I::LocalSet(nib));
        emit_hex_nibble(&mut fc, nib, DIGIT_END + 1);
        fc.emit(I::I32Const(DIGIT_END));
        fc.emit(I::I32Const(2));
        fc.emit(I::Call(print_bytes));
        fc.emit(I::LocalGet(i));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(i));
        fc.emit(I::Br(0));
        fc.emit(I::End);
        fc.emit(I::End);
        fc.emit(I::I32Const(quote_ptr));
        fc.emit(I::I32Const(quote_len));
        fc.emit(I::Call(print_bytes));
        let body = fc.finish();
        let id = self.print_literal_bytes_id;
        self.set_body(id, body);
    }

    fn ensure_fn(&mut self, name: &str) -> Result<FnRef> {
        if let Some(&slot) = self.fn_slots.get(name) {
            return Ok(FnRef::Closure(slot));
        }
        if let Some(&id) = self.fn_zero.get(name) {
            return Ok(FnRef::Zero(id));
        }
        let num_params = self
            .globals
            .get_fn_num_params(name)
            .ok_or_else(|| Error::new(format!("undefined function `{}`", name)))?;
        let body = self
            .globals
            .get_fn_body(name)
            .cloned()
            .ok_or_else(|| Error::new(format!("function `{}` has no body", name)))?;

        if num_params > 0 {
            let inner = match &*body {
                Tm::Lam { body, .. } => body.clone(),
                _ => {
                    return Err(Error::new(format!(
                        "internal: function `{}` is not represented as a lambda",
                        name
                    )));
                }
            };
            let (id, slot) = self.new_lambda_func();
            self.fn_slots.insert(name.to_string(), slot);
            let fin = {
                let mut fc = FnCompiler::new(self, 2);
                fc.env = vec![Place {
                    local: 1,
                    offset: None,
                }];
                fc.compile(&inner)?;
                fc.finish()
            };
            self.set_body(id, fin);
            Ok(FnRef::Closure(slot))
        } else {
            let id = self.new_func(&[], &[ValType::I32]);
            self.fn_zero.insert(name.to_string(), id);
            let fin = {
                let mut fc = FnCompiler::new(self, 0);
                fc.compile(&body)?;
                fc.finish()
            };
            self.set_body(id, fin);
            Ok(FnRef::Zero(id))
        }
    }

    /// Generate the closure function for one step of a partially applied
    /// constructor and return its table slot.
    fn ensure_ctor_step(&mut self, enum_name: &str, index: usize, step: usize) -> Result<u32> {
        let key = (enum_name.to_string(), index, step);
        if let Some(&slot) = self.ctor_steps.get(&key) {
            return Ok(slot);
        }
        let info = self
            .globals
            .get_ctor_info(enum_name, index)
            .cloned()
            .ok_or_else(|| Error::new(format!("unknown constructor `{}#{}`", enum_name, index)))?;
        let total = info.num_params + info.num_fields;
        if step >= total {
            return Err(Error::new("internal: constructor step out of range"));
        }

        let (id, slot) = self.new_lambda_func();
        self.ctor_steps.insert(key, slot);

        let captured = step.saturating_sub(info.num_params);
        let caps: Vec<Place> = (0..captured)
            .map(|j| Place {
                local: 0,
                offset: Some(8 + 4 * j as u32),
            })
            .collect();

        let fin = {
            let mut fc = FnCompiler::new(self, 2);
            if step < info.num_params {
                if step + 1 == total {
                    fc.build_ctor(index, &[]);
                } else {
                    let next = fc.cg.ensure_ctor_step(enum_name, index, step + 1)?;
                    fc.make_closure(next, &caps);
                }
            } else if step + 1 == total {
                let mut fields = caps.clone();
                fields.push(Place {
                    local: 1,
                    offset: None,
                });
                fc.build_ctor(index, &fields);
            } else {
                let next = fc.cg.ensure_ctor_step(enum_name, index, step + 1)?;
                let mut next_caps = caps.clone();
                next_caps.push(Place {
                    local: 1,
                    offset: None,
                });
                fc.make_closure(next, &next_caps);
            }
            fc.finish()
        };
        self.set_body(id, fin);
        Ok(slot)
    }

    fn printer_for(&mut self, ty: &V) -> Result<usize> {
        let ty = force(self.globals, ty.clone());
        match &*ty {
            Val::PrimTy(PrimTy::U32) => Ok(self.print_u32_id),
            Val::PrimTy(PrimTy::String) => Ok(self.print_str_id),
            Val::PrimTy(PrimTy::Bytes) => Ok(self.print_literal_bytes_id),
            Val::Ind(name, args) => match name.as_str() {
                "Nat" => Ok(self.print_nat_id),
                "Bool" => Ok(self.print_bool_id),
                "Unit" => Ok(self.print_unit_id),
                _ => self.enum_printer(name, args),
            },
            _ => Err(Error::new(
                "the result of `main` is not printable by the WASM backend",
            )),
        }
    }

    fn enum_printer(&mut self, name: &str, args: &[V]) -> Result<usize> {
        let key = type_key(name, args);
        if let Some(&id) = self.printers.get(&key) {
            return Ok(id);
        }
        let info = self
            .globals
            .get_enum(name)
            .cloned()
            .ok_or_else(|| Error::new(format!("unknown enum `{}`", name)))?;

        // Register the function before generating its body so recursive types
        // such as `List[T]` can call themselves.
        let id = self.new_func(&[ValType::I32], &[]);
        self.printers.insert(key, id);

        // Collect (constructor index, name, field printer ids).
        let mut entries: Vec<(usize, String, Vec<usize>)> = Vec::new();
        for (idx, ctor) in info.ctors.iter().enumerate() {
            let mut field_ty = eval(self.globals, &Vec::new(), &ctor.ty);
            for p in args {
                field_ty = apply_pi(self.globals, field_ty, p.clone()).ok_or_else(|| {
                    Error::new(format!(
                        "cannot instantiate the field types of `{}` for printing",
                        ctor.name
                    ))
                })?;
            }
            let mut printers = Vec::new();
            for j in 0..ctor.num_fields {
                let f = force(self.globals, field_ty);
                let (dom, closure) = match &*f {
                    Val::Pi(dom, closure, _) => (dom.clone(), closure.clone()),
                    _ => {
                        return Err(Error::new(format!(
                            "cannot print field {} of constructor `{}`",
                            j, ctor.name
                        )));
                    }
                };
                if contains_neutral(&dom) {
                    return Err(Error::new(format!(
                        "cannot print `{}`: field {} of `{}` depends on an earlier field",
                        name, j, ctor.name
                    )));
                }
                printers.push(self.printer_for(&dom)?);
                field_ty = eval_closure(self.globals, &closure, vvar(0));
            }
            entries.push((idx, ctor.name.clone(), printers));
        }

        let fin = {
            let mut fc = FnCompiler::new(self, 1);
            fc.print_enum_branches(&entries, 0)?;
            fc.finish()
        };
        self.set_body(id, fin);
        Ok(id)
    }

    fn gen_run(&mut self, main_id: usize, printer_id: usize) -> Result<()> {
        let (nl_ptr, nl_len) = self.intern("\n");
        let print_bytes = self.print_bytes_fn();
        let fin = {
            let mut fc = FnCompiler::new(self, 0);
            // `main` evaluates to an action; run it before printing its result.
            fc.emit(I::Call(defined(main_id)));
            let action = fc.local();
            fc.emit(I::LocalSet(action));
            emit_call_closure(&mut fc, action, None);
            fc.emit(I::Call(defined(printer_id)));
            fc.emit(I::I32Const(nl_ptr));
            fc.emit(I::I32Const(nl_len));
            fc.emit(I::Call(print_bytes));
            fc.emit(I::I32Const(0));
            fc.finish()
        };
        let id = self.run_id;
        self.set_body(id, fin);
        Ok(())
    }

    fn build(mut self) -> Result<Vec<u8>> {
        self.init_builtins()?;

        let main_ty_tm = self
            .globals
            .get_ty("main")
            .cloned()
            .ok_or_else(|| Error::new("this program needs `fn main(): IO[T]`"))?;
        let main_ty = force(self.globals, eval(self.globals, &Vec::new(), &main_ty_tm));
        let main_result = match &*main_ty {
            Val::Io(inner) => inner.clone(),
            _ => return Err(Error::new("`main` must have type `IO[T]`")),
        };
        let printer_id = self.printer_for(&main_result)?;
        let main_id = match self.ensure_fn("main")? {
            FnRef::Zero(id) => id,
            FnRef::Closure(_) => return Err(Error::new("`main` must not take parameters")),
        };
        self.gen_run(main_id, printer_id)?;

        self.encode_core()
    }

    fn encode_core(&mut self) -> Result<Vec<u8>> {
        for (i, f) in self.funcs.iter().enumerate() {
            if f.body.is_none() {
                return Err(Error::new(format!(
                    "internal: function {} has no generated body",
                    i
                )));
            }
        }

        // Allocate the import types before writing the type section.
        let get_stdout_ty = self.ty(&[], &[ValType::I32]);
        let get_stderr_ty = self.ty(&[], &[ValType::I32]);
        let write_ty = self.ty(
            &[ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            &[],
        );
        let drop_ty = self.ty(&[ValType::I32], &[]);
        let get_stdin_ty = self.ty(&[], &[ValType::I32]);
        let retptr_ty = self.ty(&[ValType::I32], &[]);
        let input_read_ty = self.ty(&[ValType::I32, ValType::I64, ValType::I32], &[]);
        let open_at_ty = self.ty(&[ValType::I32; 7], &[]);
        let descriptor_read_ty = self.ty(
            &[ValType::I32, ValType::I64, ValType::I64, ValType::I32],
            &[],
        );
        let descriptor_write_ty = self.ty(
            &[
                ValType::I32,
                ValType::I32,
                ValType::I32,
                ValType::I64,
                ValType::I32,
            ],
            &[],
        );

        let heap_start = {
            let end = DATA_BASE + self.data.len() as u32 + 16;
            let start = HEAP_START.max(end);
            (start + 3) & !3
        };
        let pages = (heap_start as u64 + 1).div_ceil(65_536).max(2);

        let mut types = TypeSection::new();
        for (params, results) in &self.types {
            types
                .ty()
                .function(params.iter().copied(), results.iter().copied());
        }

        let mut imports = ImportSection::new();
        imports.import(
            "wasi:cli/stdout@0.2.9",
            "get-stdout",
            EntityType::Function(get_stdout_ty),
        );
        imports.import(
            "wasi:cli/stderr@0.2.9",
            "get-stderr",
            EntityType::Function(get_stderr_ty),
        );
        imports.import(
            "wasi:io/streams@0.2.9",
            "[method]output-stream.blocking-write-and-flush",
            EntityType::Function(write_ty),
        );
        imports.import(
            "wasi:io/error@0.2.9",
            "[resource-drop]error",
            EntityType::Function(drop_ty),
        );
        imports.import(
            "wasi:io/streams@0.2.9",
            "[resource-drop]output-stream",
            EntityType::Function(drop_ty),
        );
        imports.import(
            "wasi:cli/environment@0.2.9",
            "get-arguments",
            EntityType::Function(retptr_ty),
        );
        imports.import(
            "wasi:cli/stdin@0.2.9",
            "get-stdin",
            EntityType::Function(get_stdin_ty),
        );
        imports.import(
            "wasi:io/streams@0.2.9",
            "[method]input-stream.blocking-read",
            EntityType::Function(input_read_ty),
        );
        imports.import(
            "wasi:io/streams@0.2.9",
            "[resource-drop]input-stream",
            EntityType::Function(drop_ty),
        );
        imports.import(
            "wasi:filesystem/preopens@0.2.9",
            "get-directories",
            EntityType::Function(retptr_ty),
        );
        imports.import(
            "wasi:filesystem/types@0.2.9",
            "[method]descriptor.open-at",
            EntityType::Function(open_at_ty),
        );
        imports.import(
            "wasi:filesystem/types@0.2.9",
            "[method]descriptor.read",
            EntityType::Function(descriptor_read_ty),
        );
        imports.import(
            "wasi:filesystem/types@0.2.9",
            "[method]descriptor.write",
            EntityType::Function(descriptor_write_ty),
        );
        imports.import(
            "wasi:filesystem/types@0.2.9",
            "[resource-drop]descriptor",
            EntityType::Function(drop_ty),
        );

        let mut functions = FunctionSection::new();
        for f in &self.funcs {
            functions.function(f.ty);
        }

        let table_len = (self.table_funcs.len() as u64).max(1);
        let mut tables = TableSection::new();
        tables.table(TableType {
            element_type: RefType::FUNCREF,
            table64: false,
            minimum: table_len,
            maximum: Some(table_len),
            shared: false,
        });

        let mut memories = MemorySection::new();
        memories.memory(MemoryType {
            minimum: pages,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });

        let mut globals = GlobalSection::new();
        globals.global(
            GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &ConstExpr::i32_const(heap_start as i32),
        );

        let mut exports = ExportSection::new();
        exports.export("memory", ExportKind::Memory, 0);
        exports.export(
            "wasi:cli/run@0.2.9#run",
            ExportKind::Func,
            defined(self.run_id),
        );
        exports.export("cabi_realloc", ExportKind::Func, defined(self.realloc_id));

        let mut elements = ElementSection::new();
        elements.active(
            Some(0),
            &ConstExpr::i32_const(0),
            Elements::Functions(Cow::Borrowed(self.table_funcs.as_slice())),
        );

        let mut code = CodeSection::new();
        for f in &self.funcs {
            code.function(f.body.as_ref().unwrap());
        }

        let mut module = Module::new();
        module.section(&types);
        module.section(&imports);
        module.section(&functions);
        module.section(&tables);
        module.section(&memories);
        module.section(&globals);
        module.section(&exports);
        module.section(&elements);
        module.section(&code);

        if !self.data.is_empty() {
            let mut data = DataSection::new();
            data.active(
                0,
                &ConstExpr::i32_const(DATA_BASE as i32),
                self.data.iter().copied(),
            );
            module.section(&data);
        }

        Ok(module.finish())
    }
}

/// The environment of a thunk: its captured values live in the closure block's
/// fields, in the order they were captured. A thunk's ignored argument is not
/// part of the term's context.
fn captured_env(count: usize) -> Vec<Place> {
    (0..count)
        .map(|j| Place {
            local: 0,
            offset: Some(8 + 4 * j as u32),
        })
        .collect()
}

/// Call a closure value. Actions ignore the argument, so `arg` is `None`; a
/// continuation receives the produced value.
fn emit_call_closure(fc: &mut FnCompiler<'_, '_>, closure: u32, arg: Option<u32>) {
    let ty = fc.cg.closure_ty();
    fc.emit(I::LocalGet(closure));
    match arg {
        Some(arg) => fc.emit(I::LocalGet(arg)),
        None => fc.emit(I::I32Const(0)),
    }
    fc.emit(I::LocalGet(closure));
    fc.emit(I::I32Load(mem(4)));
    fc.emit(I::CallIndirect {
        type_index: ty,
        table_index: 0,
    });
}

fn emit_min_u(fc: &mut FnCompiler<'_, '_>, target: u32, limit: u32) {
    fc.emit(I::LocalGet(target));
    fc.emit(I::LocalGet(limit));
    fc.emit(I::I32GtU);
    fc.emit(I::If(BlockType::Empty));
    fc.emit(I::LocalGet(limit));
    fc.emit(I::LocalSet(target));
    fc.emit(I::End);
}

fn emit_hex_nibble(fc: &mut FnCompiler<'_, '_>, nib: u32, addr: i32) {
    fc.emit(I::I32Const(addr));
    fc.emit(I::LocalGet(nib));
    fc.emit(I::I32Const(10));
    fc.emit(I::I32LtU);
    fc.emit(I::If(BlockType::Result(ValType::I32)));
    fc.emit(I::LocalGet(nib));
    fc.emit(I::I32Const(b'0' as i32));
    fc.emit(I::I32Add);
    fc.emit(I::Else);
    fc.emit(I::LocalGet(nib));
    fc.emit(I::I32Const(b'a' as i32 - 10));
    fc.emit(I::I32Add);
    fc.emit(I::End);
    fc.emit(I::I32Store8(mem8(0)));
}

struct FnCompiler<'a, 'g> {
    cg: &'a mut Codegen<'g>,
    code: Vec<I<'static>>,
    next_local: u32,
    params: u32,
    env: Vec<Place>,
}

impl<'a, 'g> FnCompiler<'a, 'g> {
    fn new(cg: &'a mut Codegen<'g>, params: u32) -> Self {
        Self {
            cg,
            code: Vec::new(),
            next_local: params,
            params,
            env: Vec::new(),
        }
    }

    fn emit(&mut self, i: I<'static>) {
        self.code.push(i);
    }

    fn local(&mut self) -> u32 {
        let l = self.next_local;
        self.next_local += 1;
        l
    }

    fn load(&mut self, p: Place) {
        self.emit(I::LocalGet(p.local));
        if let Some(off) = p.offset {
            self.emit(I::I32Load(mem(off)));
        }
    }

    fn finish(self) -> Function {
        let extra = self.next_local - self.params;
        let locals = vec![ValType::I32; extra as usize];
        let mut f = Function::new_with_locals_types(locals);
        for i in &self.code {
            f.instruction(i);
        }
        f.instruction(&I::End);
        f
    }

    /// Build a constructor block whose fields are the given places.
    fn build_ctor(&mut self, index: usize, fields: &[Place]) {
        if fields.is_empty() {
            let size = 4i32;
            self.emit(I::I32Const(size));
            self.emit(I::Call(self.cg.alloc_fn()));
            let ptr = self.local();
            self.emit(I::LocalSet(ptr));
            self.emit(I::LocalGet(ptr));
            self.emit(I::I32Const(index as i32));
            self.emit(I::I32Store(mem(0)));
            self.emit(I::LocalGet(ptr));
            return;
        }

        let size = 4 * (1 + fields.len() as i32);
        self.emit(I::I32Const(size));
        self.emit(I::Call(self.cg.alloc_fn()));
        let ptr = self.local();
        self.emit(I::LocalSet(ptr));
        self.emit(I::LocalGet(ptr));
        self.emit(I::I32Const(index as i32));
        self.emit(I::I32Store(mem(0)));
        for (j, p) in fields.iter().enumerate() {
            self.emit(I::LocalGet(ptr));
            self.load(*p);
            self.emit(I::I32Store(mem(4 + 4 * j as u32)));
        }
        self.emit(I::LocalGet(ptr));
    }

    /// Build a closure block, capturing `caps` in order.
    fn make_closure(&mut self, slot: u32, caps: &[Place]) {
        let size = 8 + 4 * caps.len() as i32;
        self.emit(I::I32Const(size));
        self.emit(I::Call(self.cg.alloc_fn()));
        let ptr = self.local();
        self.emit(I::LocalSet(ptr));
        self.emit(I::LocalGet(ptr));
        self.emit(I::I32Const(TAG_CLOSURE));
        self.emit(I::I32Store(mem(0)));
        self.emit(I::LocalGet(ptr));
        self.emit(I::I32Const(slot as i32));
        self.emit(I::I32Store(mem(4)));
        for (j, p) in caps.iter().enumerate() {
            self.emit(I::LocalGet(ptr));
            self.load(*p);
            self.emit(I::I32Store(mem(8 + 4 * j as u32)));
        }
        self.emit(I::LocalGet(ptr));
    }

    fn print_str(&mut self, s: &str) {
        let (ptr, len) = self.cg.intern(s);
        let pb = self.cg.print_bytes_fn();
        self.emit(I::I32Const(ptr));
        self.emit(I::I32Const(len));
        self.emit(I::Call(pb));
    }

    fn compile(&mut self, tm: &RcTm) -> Result<()> {
        match &**tm {
            Tm::Var(i) => {
                if let Some(p) = self.env.get(*i).copied() {
                    self.load(p);
                } else {
                    self.emit(I::I32Const(0));
                }
            }
            Tm::Type(_) | Tm::Prop | Tm::Pi { .. } | Tm::Ind(..) | Tm::Eq(..) | Tm::Refl(_) => {
                self.emit(I::I32Const(0));
            }
            Tm::PrimTy(_) => self.emit(I::I32Const(0)),
            Tm::PrimLit(PrimLit::U32(n)) => self.emit(I::I32Const(*n as i32)),
            Tm::PrimLit(PrimLit::Str(bytes) | PrimLit::Bytes(bytes)) => {
                let ptr = self.cg.intern_literal(bytes);
                self.emit(I::I32Const(ptr));
            }
            Tm::PrimOp(op, args) => self.compile_prim_op(*op, args)?,
            Tm::Io(_) => self.emit(I::I32Const(0)),
            Tm::IoPure(v) => self.compile_thunk(v)?,
            Tm::IoBind { m, k } => self.compile_bind(m, k)?,
            Tm::Extern { name, args } => self.compile_extern(name, args)?,
            Tm::Const(name) => self.compile_const(name)?,
            Tm::Ctor(enum_name, index, args) => self.compile_ctor(enum_name, *index, args)?,
            Tm::App(f, a) => self.compile_app(f, a)?,
            Tm::Match {
                motive,
                scrut,
                branches,
            } => self.compile_match(motive, scrut, branches)?,
            Tm::J { refl_case, .. } => self.compile(refl_case)?,
            Tm::Lam { body, .. } => self.compile_lambda(body)?,
        }
        Ok(())
    }

    fn compile_extern(&mut self, name: &str, args: &[RcTm]) -> Result<()> {
        let import = self
            .cg
            .globals
            .get_extern(name)
            .map(|info| info.import.clone())
            .ok_or_else(|| Error::new(format!("unknown extern `{}`", name)))?;
        let capability = neo_host::lookup(&import)
            .ok_or_else(|| Error::new(format!("unknown host capability `{}`", import)))?;
        self.compile_thunk_with(|child| match capability.lowering {
            neo_host::Lowering::StderrWrite => child.compile_stderr_write(args),
            neo_host::Lowering::Argv => child.compile_argv(),
            neo_host::Lowering::StdinReadAll => child.compile_stdin_read_all(),
            neo_host::Lowering::FileRead => child.compile_file_read(args),
            neo_host::Lowering::FileWrite => child.compile_file_write(args),
        })
    }

    fn compile_stderr_write(&mut self, args: &[RcTm]) -> Result<()> {
        let [arg] = args else {
            return Err(Error::new("`stderr.write` takes one argument"));
        };
        self.compile(arg)?;
        let desc = self.local();
        self.emit(I::LocalSet(desc));
        let ptr = self.local();
        self.emit(I::LocalGet(desc));
        self.emit(I::I32Load(mem(0)));
        self.emit(I::LocalGet(desc));
        self.emit(I::I32Load(mem(4)));
        self.emit(I::I32Add);
        self.emit(I::LocalSet(ptr));

        let handle = self.local();
        self.emit(I::Call(IMPORT_GET_STDERR));
        self.emit(I::LocalSet(handle));
        let status = self.local();
        self.emit(I::LocalGet(handle));
        self.emit(I::LocalGet(ptr));
        self.emit(I::LocalGet(desc));
        self.emit(I::I32Load(mem(8)));
        self.emit(I::Call(self.cg.write_stream_fn()));
        self.emit(I::LocalSet(status));
        self.emit(I::LocalGet(handle));
        self.emit(I::Call(IMPORT_DROP_OUTPUT_STREAM));

        let globals = self.cg.globals;
        let ok = ctor_index(globals, "Result", "Ok")?;
        let err = ctor_index(globals, "Result", "Err")?;
        let unit = ctor_index(globals, "Unit", "Unit")?;
        let closed = ctor_index(globals, "IoError", "Closed")?;
        let failed = ctor_index(globals, "IoError", "Failed")?;

        self.emit(I::LocalGet(status));
        self.emit(I::I32Eqz);
        self.emit(I::If(BlockType::Result(ValType::I32)));
        let unit_local = self.local();
        self.build_ctor(unit, &[]);
        self.emit(I::LocalSet(unit_local));
        self.build_ctor(
            ok,
            &[Place {
                local: unit_local,
                offset: None,
            }],
        );
        self.emit(I::Else);
        self.emit(I::LocalGet(status));
        self.emit(I::I32Const(1));
        self.emit(I::I32Eq);
        self.emit(I::If(BlockType::Result(ValType::I32)));
        let closed_local = self.local();
        self.build_ctor(closed, &[]);
        self.emit(I::LocalSet(closed_local));
        self.build_ctor(
            err,
            &[Place {
                local: closed_local,
                offset: None,
            }],
        );
        self.emit(I::Else);
        let failed_local = self.local();
        self.build_ctor(failed, &[]);
        self.emit(I::LocalSet(failed_local));
        self.build_ctor(
            err,
            &[Place {
                local: failed_local,
                offset: None,
            }],
        );
        self.emit(I::End);
        self.emit(I::End);
        Ok(())
    }

    fn load_mem(&mut self, addr: i32, offset: u32) {
        self.emit(I::I32Const(addr));
        self.emit(I::I32Load(mem(offset)));
    }

    fn load_u8_mem(&mut self, addr: i32, offset: u32) {
        self.emit(I::I32Const(addr));
        self.emit(I::I32Load8U(mem8(offset)));
    }

    fn build_io_ok(&mut self, payload: Option<u32>) -> Result<()> {
        let ok = ctor_index(self.cg.globals, "Result", "Ok")?;
        let unit = ctor_index(self.cg.globals, "Unit", "Unit")?;
        match payload {
            Some(local) => self.build_ctor(
                ok,
                &[Place {
                    local,
                    offset: None,
                }],
            ),
            None => {
                let unit_local = self.local();
                self.build_ctor(unit, &[]);
                self.emit(I::LocalSet(unit_local));
                self.build_ctor(
                    ok,
                    &[Place {
                        local: unit_local,
                        offset: None,
                    }],
                );
            }
        }
        Ok(())
    }

    fn build_io_err(&mut self, ctor: &str) -> Result<()> {
        let err = ctor_index(self.cg.globals, "Result", "Err")?;
        let error = ctor_index(self.cg.globals, "IoError", ctor)?;
        let error_local = self.local();
        self.build_ctor(error, &[]);
        self.emit(I::LocalSet(error_local));
        self.build_ctor(
            err,
            &[Place {
                local: error_local,
                offset: None,
            }],
        );
        Ok(())
    }

    fn make_bytes_descriptor(&mut self, ptr: u32, len: u32) -> u32 {
        self.emit(I::I32Const(12));
        self.emit(I::Call(self.cg.alloc_fn()));
        let desc = self.local();
        self.emit(I::LocalSet(desc));
        self.emit(I::LocalGet(desc));
        self.emit(I::LocalGet(ptr));
        self.emit(I::I32Store(mem(0)));
        self.emit(I::LocalGet(desc));
        self.emit(I::I32Const(0));
        self.emit(I::I32Store(mem(4)));
        self.emit(I::LocalGet(desc));
        self.emit(I::LocalGet(len));
        self.emit(I::I32Store(mem(8)));
        desc
    }

    /// Read the process argument vector and build a `List[String]`.
    fn compile_argv(&mut self) -> Result<()> {
        self.emit(I::I32Const(ARGV_RET));
        self.emit(I::Call(IMPORT_GET_ARGUMENTS));
        let base = self.local();
        self.load_mem(ARGV_RET, 0);
        self.emit(I::LocalSet(base));
        let len = self.local();
        self.load_mem(ARGV_RET, 4);
        self.emit(I::LocalSet(len));

        let index = self.local();
        self.emit(I::LocalGet(len));
        self.emit(I::LocalSet(index));
        let acc = self.local();
        let nil = ctor_index(self.cg.globals, "List", "Nil")?;
        let cons = ctor_index(self.cg.globals, "List", "Cons")?;
        self.build_ctor(nil, &[]);
        self.emit(I::LocalSet(acc));

        self.emit(I::Block(BlockType::Empty));
        self.emit(I::Loop(BlockType::Empty));
        self.emit(I::LocalGet(index));
        self.emit(I::I32Eqz);
        self.emit(I::BrIf(1));
        self.emit(I::LocalGet(index));
        self.emit(I::I32Const(1));
        self.emit(I::I32Sub);
        self.emit(I::LocalSet(index));

        let offset = self.local();
        self.emit(I::LocalGet(index));
        self.emit(I::I32Const(8));
        self.emit(I::I32Mul);
        self.emit(I::LocalSet(offset));
        let ptr = self.local();
        self.emit(I::LocalGet(base));
        self.emit(I::LocalGet(offset));
        self.emit(I::I32Add);
        self.emit(I::I32Load(mem(0)));
        self.emit(I::LocalSet(ptr));
        let text_len = self.local();
        self.emit(I::LocalGet(base));
        self.emit(I::LocalGet(offset));
        self.emit(I::I32Add);
        self.emit(I::I32Load(mem(4)));
        self.emit(I::LocalSet(text_len));

        let desc = self.make_bytes_descriptor(ptr, text_len);
        self.build_ctor(
            cons,
            &[
                Place {
                    local: desc,
                    offset: None,
                },
                Place {
                    local: acc,
                    offset: None,
                },
            ],
        );
        self.emit(I::LocalSet(acc));
        self.emit(I::Br(0));
        self.emit(I::End);
        self.emit(I::End);
        self.emit(I::LocalGet(acc));
        Ok(())
    }

    /// Read standard input to end and build a `Bytes` value.
    fn compile_stdin_read_all(&mut self) -> Result<()> {
        let handle = self.local();
        self.emit(I::Call(IMPORT_GET_STDIN));
        self.emit(I::LocalSet(handle));
        let acc = self.local();
        let empty = self.cg.intern_literal(b"");
        self.emit(I::I32Const(empty));
        self.emit(I::LocalSet(acc));
        let failed = self.local();
        self.emit(I::I32Const(0));
        self.emit(I::LocalSet(failed));

        self.emit(I::Block(BlockType::Empty));
        self.emit(I::Loop(BlockType::Empty));
        self.emit(I::LocalGet(handle));
        self.emit(I::I64Const(4096));
        self.emit(I::I32Const(READ_RET));
        self.emit(I::Call(IMPORT_INPUT_READ));

        let tag = self.local();
        self.load_u8_mem(READ_RET, 0);
        self.emit(I::LocalSet(tag));
        self.emit(I::LocalGet(tag));
        self.emit(I::I32Eqz);
        self.emit(I::If(BlockType::Empty));
        self.emit(I::Else);
        let inner = self.local();
        self.load_u8_mem(READ_RET, 4);
        self.emit(I::LocalSet(inner));
        // `closed` is the normal end-of-stream condition.
        self.emit(I::LocalGet(inner));
        self.emit(I::I32Const(1));
        self.emit(I::I32Eq);
        self.emit(I::If(BlockType::Empty));
        self.emit(I::Br(3));
        self.emit(I::End);
        self.emit(I::I32Const(1));
        self.emit(I::LocalSet(failed));
        let error = self.local();
        self.load_mem(READ_RET, 8);
        self.emit(I::LocalSet(error));
        self.emit(I::LocalGet(error));
        self.emit(I::Call(IMPORT_DROP_ERROR));
        self.emit(I::Br(2));
        self.emit(I::End);

        let ptr = self.local();
        self.load_mem(READ_RET, 4);
        self.emit(I::LocalSet(ptr));
        let len = self.local();
        self.load_mem(READ_RET, 8);
        self.emit(I::LocalSet(len));
        self.emit(I::LocalGet(len));
        self.emit(I::I32Eqz);
        self.emit(I::If(BlockType::Empty));
        self.emit(I::Br(2));
        self.emit(I::End);

        let chunk = self.make_bytes_descriptor(ptr, len);
        self.emit(I::LocalGet(acc));
        self.emit(I::LocalGet(chunk));
        self.emit(I::Call(defined(self.cg.concat_id)));
        self.emit(I::LocalSet(acc));
        self.emit(I::Br(0));
        self.emit(I::End);
        self.emit(I::End);

        self.emit(I::LocalGet(handle));
        self.emit(I::Call(IMPORT_DROP_INPUT_STREAM));
        self.emit(I::LocalGet(failed));
        self.emit(I::If(BlockType::Result(ValType::I32)));
        self.build_io_err("Failed")?;
        self.emit(I::Else);
        self.build_io_ok(Some(acc))?;
        self.emit(I::End);
        Ok(())
    }

    fn emit_open(
        &mut self,
        dirs_base: u32,
        dirs_len: u32,
        path_ptr: u32,
        path_len: u32,
        open_flags: i32,
        desc_flags: i32,
    ) -> u32 {
        let desc = self.local();
        self.emit(I::I32Const(0));
        self.emit(I::LocalSet(desc));
        let index = self.local();
        self.emit(I::I32Const(0));
        self.emit(I::LocalSet(index));

        self.emit(I::Block(BlockType::Empty));
        self.emit(I::Loop(BlockType::Empty));
        self.emit(I::LocalGet(index));
        self.emit(I::LocalGet(dirs_len));
        self.emit(I::I32GeU);
        self.emit(I::BrIf(1));
        let offset = self.local();
        self.emit(I::LocalGet(index));
        self.emit(I::I32Const(12));
        self.emit(I::I32Mul);
        self.emit(I::LocalSet(offset));
        let dir = self.local();
        self.emit(I::LocalGet(dirs_base));
        self.emit(I::LocalGet(offset));
        self.emit(I::I32Add);
        self.emit(I::I32Load(mem(0)));
        self.emit(I::LocalSet(dir));
        self.emit(I::LocalGet(dir));
        self.emit(I::I32Const(1));
        self.emit(I::LocalGet(path_ptr));
        self.emit(I::LocalGet(path_len));
        self.emit(I::I32Const(open_flags));
        self.emit(I::I32Const(desc_flags));
        self.emit(I::I32Const(OPEN_RET));
        self.emit(I::Call(IMPORT_OPEN_AT));
        let tag = self.local();
        self.load_u8_mem(OPEN_RET, 0);
        self.emit(I::LocalSet(tag));
        self.emit(I::LocalGet(tag));
        self.emit(I::I32Eqz);
        self.emit(I::If(BlockType::Empty));
        self.load_mem(OPEN_RET, 4);
        self.emit(I::LocalSet(desc));
        self.emit(I::Br(2));
        self.emit(I::End);
        self.emit(I::LocalGet(index));
        self.emit(I::I32Const(1));
        self.emit(I::I32Add);
        self.emit(I::LocalSet(index));
        self.emit(I::Br(0));
        self.emit(I::End);
        self.emit(I::End);
        desc
    }

    fn compile_file_read(&mut self, args: &[RcTm]) -> Result<()> {
        let [path] = args else {
            return Err(Error::new("`file.read` takes one argument"));
        };
        self.compile(path)?;
        let path_desc = self.local();
        self.emit(I::LocalSet(path_desc));
        let path_ptr = self.local();
        self.emit(I::LocalGet(path_desc));
        self.emit(I::I32Load(mem(0)));
        self.emit(I::LocalGet(path_desc));
        self.emit(I::I32Load(mem(4)));
        self.emit(I::I32Add);
        self.emit(I::LocalSet(path_ptr));
        let path_len = self.local();
        self.emit(I::LocalGet(path_desc));
        self.emit(I::I32Load(mem(8)));
        self.emit(I::LocalSet(path_len));

        let failed = self.local();
        self.emit(I::I32Const(0));
        self.emit(I::LocalSet(failed));
        let acc = self.local();
        let empty = self.cg.intern_literal(b"");
        self.emit(I::I32Const(empty));
        self.emit(I::LocalSet(acc));
        let offset = self.local();
        self.emit(I::I32Const(0));
        self.emit(I::LocalSet(offset));

        self.emit(I::I32Const(DIRS_RET));
        self.emit(I::Call(IMPORT_GET_DIRECTORIES));
        let dirs_base = self.local();
        self.load_mem(DIRS_RET, 0);
        self.emit(I::LocalSet(dirs_base));
        let dirs_len = self.local();
        self.load_mem(DIRS_RET, 4);
        self.emit(I::LocalSet(dirs_len));
        let desc = self.emit_open(dirs_base, dirs_len, path_ptr, path_len, 0, 1);

        self.emit(I::LocalGet(desc));
        self.emit(I::I32Eqz);
        self.emit(I::If(BlockType::Empty));
        self.emit(I::I32Const(1));
        self.emit(I::LocalSet(failed));
        self.emit(I::Else);
        self.emit(I::Block(BlockType::Empty));
        self.emit(I::Loop(BlockType::Empty));
        self.emit(I::LocalGet(desc));
        self.emit(I::I64Const(4096));
        self.emit(I::LocalGet(offset));
        self.emit(I::I64ExtendI32U);
        self.emit(I::I32Const(READ_RET));
        self.emit(I::Call(IMPORT_DESCRIPTOR_READ));
        let read_tag = self.local();
        self.load_u8_mem(READ_RET, 0);
        self.emit(I::LocalSet(read_tag));
        self.emit(I::LocalGet(read_tag));
        self.emit(I::I32Eqz);
        self.emit(I::If(BlockType::Empty));
        self.emit(I::Else);
        self.emit(I::I32Const(1));
        self.emit(I::LocalSet(failed));
        self.emit(I::Br(2));
        self.emit(I::End);

        let ptr = self.local();
        self.load_mem(READ_RET, 4);
        self.emit(I::LocalSet(ptr));
        let len = self.local();
        self.load_mem(READ_RET, 8);
        self.emit(I::LocalSet(len));
        let eof = self.local();
        self.load_u8_mem(READ_RET, 12);
        self.emit(I::LocalSet(eof));

        self.emit(I::LocalGet(len));
        self.emit(I::I32Eqz);
        self.emit(I::If(BlockType::Empty));
        self.emit(I::Br(2));
        self.emit(I::End);
        let chunk = self.make_bytes_descriptor(ptr, len);
        self.emit(I::LocalGet(acc));
        self.emit(I::LocalGet(chunk));
        self.emit(I::Call(defined(self.cg.concat_id)));
        self.emit(I::LocalSet(acc));
        self.emit(I::LocalGet(offset));
        self.emit(I::LocalGet(len));
        self.emit(I::I32Add);
        self.emit(I::LocalSet(offset));
        self.emit(I::LocalGet(eof));
        self.emit(I::If(BlockType::Empty));
        self.emit(I::Br(2));
        self.emit(I::End);
        self.emit(I::Br(0));
        self.emit(I::End);
        self.emit(I::End);
        self.emit(I::LocalGet(desc));
        self.emit(I::Call(IMPORT_DROP_DESCRIPTOR));
        self.emit(I::End);

        self.emit(I::LocalGet(failed));
        self.emit(I::If(BlockType::Result(ValType::I32)));
        self.build_io_err("Failed")?;
        self.emit(I::Else);
        self.build_io_ok(Some(acc))?;
        self.emit(I::End);
        Ok(())
    }

    fn compile_file_write(&mut self, args: &[RcTm]) -> Result<()> {
        let [path, data] = args else {
            return Err(Error::new("`file.write` takes two arguments"));
        };
        self.compile(path)?;
        let path_desc = self.local();
        self.emit(I::LocalSet(path_desc));
        let path_ptr = self.local();
        self.emit(I::LocalGet(path_desc));
        self.emit(I::I32Load(mem(0)));
        self.emit(I::LocalGet(path_desc));
        self.emit(I::I32Load(mem(4)));
        self.emit(I::I32Add);
        self.emit(I::LocalSet(path_ptr));
        let path_len = self.local();
        self.emit(I::LocalGet(path_desc));
        self.emit(I::I32Load(mem(8)));
        self.emit(I::LocalSet(path_len));

        self.compile(data)?;
        let data_desc = self.local();
        self.emit(I::LocalSet(data_desc));
        let data_ptr = self.local();
        self.emit(I::LocalGet(data_desc));
        self.emit(I::I32Load(mem(0)));
        self.emit(I::LocalGet(data_desc));
        self.emit(I::I32Load(mem(4)));
        self.emit(I::I32Add);
        self.emit(I::LocalSet(data_ptr));
        let data_len = self.local();
        self.emit(I::LocalGet(data_desc));
        self.emit(I::I32Load(mem(8)));
        self.emit(I::LocalSet(data_len));

        let failed = self.local();
        self.emit(I::I32Const(0));
        self.emit(I::LocalSet(failed));
        let written = self.local();
        self.emit(I::I32Const(0));
        self.emit(I::LocalSet(written));

        self.emit(I::I32Const(DIRS_RET));
        self.emit(I::Call(IMPORT_GET_DIRECTORIES));
        let dirs_base = self.local();
        self.load_mem(DIRS_RET, 0);
        self.emit(I::LocalSet(dirs_base));
        let dirs_len = self.local();
        self.load_mem(DIRS_RET, 4);
        self.emit(I::LocalSet(dirs_len));
        let desc = self.emit_open(dirs_base, dirs_len, path_ptr, path_len, 9, 2);

        self.emit(I::LocalGet(desc));
        self.emit(I::I32Eqz);
        self.emit(I::If(BlockType::Empty));
        self.emit(I::I32Const(1));
        self.emit(I::LocalSet(failed));
        self.emit(I::Else);
        self.emit(I::Block(BlockType::Empty));
        self.emit(I::Loop(BlockType::Empty));
        self.emit(I::LocalGet(written));
        self.emit(I::LocalGet(data_len));
        self.emit(I::I32GeU);
        self.emit(I::BrIf(1));
        self.emit(I::LocalGet(desc));
        self.emit(I::LocalGet(data_ptr));
        self.emit(I::LocalGet(written));
        self.emit(I::I32Add);
        self.emit(I::LocalGet(data_len));
        self.emit(I::LocalGet(written));
        self.emit(I::I32Sub);
        self.emit(I::LocalGet(written));
        self.emit(I::I64ExtendI32U);
        self.emit(I::I32Const(WRITE_RET));
        self.emit(I::Call(IMPORT_DESCRIPTOR_WRITE));
        let write_tag = self.local();
        self.load_u8_mem(WRITE_RET, 0);
        self.emit(I::LocalSet(write_tag));
        self.emit(I::LocalGet(write_tag));
        self.emit(I::I32Eqz);
        self.emit(I::If(BlockType::Empty));
        self.emit(I::Else);
        self.emit(I::I32Const(1));
        self.emit(I::LocalSet(failed));
        self.emit(I::Br(2));
        self.emit(I::End);
        let n = self.local();
        self.load_mem(WRITE_RET, 8);
        self.emit(I::LocalSet(n));
        self.emit(I::LocalGet(n));
        self.emit(I::I32Eqz);
        self.emit(I::If(BlockType::Empty));
        self.emit(I::I32Const(1));
        self.emit(I::LocalSet(failed));
        self.emit(I::Br(2));
        self.emit(I::End);
        self.emit(I::LocalGet(written));
        self.emit(I::LocalGet(n));
        self.emit(I::I32Add);
        self.emit(I::LocalSet(written));
        self.emit(I::Br(0));
        self.emit(I::End);
        self.emit(I::End);
        self.emit(I::LocalGet(desc));
        self.emit(I::Call(IMPORT_DROP_DESCRIPTOR));
        self.emit(I::End);

        self.emit(I::LocalGet(failed));
        self.emit(I::If(BlockType::Result(ValType::I32)));
        self.build_io_err("Failed")?;
        self.emit(I::Else);
        self.build_io_ok(None)?;
        self.emit(I::End);
        Ok(())
    }

    fn compile_const(&mut self, name: &str) -> Result<()> {
        if let Some(info) = self.cg.globals.get_ctor(name).cloned() {
            let total = info.num_params + info.num_fields;
            if total == 0 {
                self.build_ctor(info.index, &[]);
            } else {
                let slot = self.cg.ensure_ctor_step(&info.enum_name, info.index, 0)?;
                self.make_closure(slot, &[]);
            }
            return Ok(());
        }

        match self.cg.ensure_fn(name)? {
            FnRef::Closure(slot) => self.make_closure(slot, &[]),
            FnRef::Zero(id) => self.emit(I::Call(defined(id))),
        }
        Ok(())
    }

    fn compile_ctor(&mut self, enum_name: &str, index: usize, args: &[RcTm]) -> Result<()> {
        let info = self
            .cg
            .globals
            .get_ctor_info(enum_name, index)
            .cloned()
            .ok_or_else(|| Error::new(format!("unknown constructor `{}#{}`", enum_name, index)))?;
        if args.len() < info.num_params {
            return Err(Error::new(format!(
                "constructor `{}` is missing its type parameters",
                info.name
            )));
        }
        let fields = &args[info.num_params..];
        if fields.len() != info.num_fields {
            return Err(Error::new(format!(
                "constructor `{}` expects {} field(s), got {}",
                info.name,
                info.num_fields,
                fields.len()
            )));
        }

        if fields.is_empty() {
            self.build_ctor(index, &[]);
            return Ok(());
        }

        let size = 4 * (1 + fields.len() as i32);
        self.emit(I::I32Const(size));
        self.emit(I::Call(self.cg.alloc_fn()));
        let ptr = self.local();
        self.emit(I::LocalSet(ptr));
        self.emit(I::LocalGet(ptr));
        self.emit(I::I32Const(index as i32));
        self.emit(I::I32Store(mem(0)));
        for (j, field) in fields.iter().enumerate() {
            self.emit(I::LocalGet(ptr));
            self.compile(field)?;
            self.emit(I::I32Store(mem(4 + 4 * j as u32)));
        }
        self.emit(I::LocalGet(ptr));
        Ok(())
    }

    /// Turn a 0/1 flag into a `Bool` constructor value.
    fn bool_from_flag(&mut self) -> Result<()> {
        let (_, false_index) =
            self.cg.globals.bool_ctor(false).ok_or_else(|| {
                Error::new("primitive comparisons require the prelude `Bool` type")
            })?;
        let (_, true_index) =
            self.cg.globals.bool_ctor(true).ok_or_else(|| {
                Error::new("primitive comparisons require the prelude `Bool` type")
            })?;
        let flag = self.local();
        self.emit(I::LocalSet(flag));
        self.emit(I::LocalGet(flag));
        self.emit(I::If(BlockType::Result(ValType::I32)));
        self.emit(I::I32Const(true_index as i32));
        self.emit(I::Else);
        self.emit(I::I32Const(false_index as i32));
        self.emit(I::End);
        let tag = self.local();
        self.emit(I::LocalSet(tag));
        self.emit(I::I32Const(4));
        self.emit(I::Call(self.cg.alloc_fn()));
        let ptr = self.local();
        self.emit(I::LocalSet(ptr));
        self.emit(I::LocalGet(ptr));
        self.emit(I::LocalGet(tag));
        self.emit(I::I32Store(mem(0)));
        self.emit(I::LocalGet(ptr));
        Ok(())
    }

    /// Compile a primitive operation. `U32` values are unboxed `i32`s,
    /// boolean results are ordinary `Bool` constructors, and `String` and
    /// `Bytes` values are descriptor pointers.
    fn compile_prim_op(&mut self, op: PrimOp, args: &[RcTm]) -> Result<()> {
        let arg = |i: usize| {
            args.get(i).ok_or_else(|| {
                Error::new(format!(
                    "internal: `{}` is missing argument {}",
                    op.name(),
                    i
                ))
            })
        };
        match op {
            PrimOp::Lt | PrimOp::Le | PrimOp::Gt | PrimOp::Ge | PrimOp::Eq | PrimOp::Ne => {
                self.compile(arg(0)?)?;
                self.compile(arg(1)?)?;
                self.emit(match op {
                    PrimOp::Lt => I::I32LtU,
                    PrimOp::Le => I::I32LeU,
                    PrimOp::Gt => I::I32GtU,
                    PrimOp::Ge => I::I32GeU,
                    PrimOp::Eq => I::I32Eq,
                    PrimOp::Ne => I::I32Ne,
                    _ => return Err(Error::new("internal: comparison not handled")),
                });
                self.bool_from_flag()?;
            }
            PrimOp::Div | PrimOp::Rem => {
                let lhs = self.local();
                let rhs = self.local();
                self.compile(arg(0)?)?;
                self.emit(I::LocalSet(lhs));
                self.compile(arg(1)?)?;
                self.emit(I::LocalSet(rhs));
                // Division by zero is total: it yields zero.
                self.emit(I::LocalGet(rhs));
                self.emit(I::I32Eqz);
                self.emit(I::If(BlockType::Result(ValType::I32)));
                self.emit(I::I32Const(0));
                self.emit(I::Else);
                self.emit(I::LocalGet(lhs));
                self.emit(I::LocalGet(rhs));
                self.emit(if matches!(op, PrimOp::Div) {
                    I::I32DivU
                } else {
                    I::I32RemU
                });
                self.emit(I::End);
            }
            PrimOp::Add
            | PrimOp::Sub
            | PrimOp::Mul
            | PrimOp::And
            | PrimOp::Or
            | PrimOp::Xor
            | PrimOp::Shl
            | PrimOp::Shr => {
                self.compile(arg(0)?)?;
                self.compile(arg(1)?)?;
                self.emit(match op {
                    PrimOp::Add => I::I32Add,
                    PrimOp::Sub => I::I32Sub,
                    PrimOp::Mul => I::I32Mul,
                    PrimOp::And => I::I32And,
                    PrimOp::Or => I::I32Or,
                    PrimOp::Xor => I::I32Xor,
                    PrimOp::Shl => I::I32Shl,
                    PrimOp::Shr => I::I32ShrU,
                    _ => return Err(Error::new("internal: primitive operation not handled")),
                });
            }
            PrimOp::StrEq | PrimOp::BytesEq | PrimOp::StrNe | PrimOp::BytesNe => {
                self.compile(arg(0)?)?;
                self.compile(arg(1)?)?;
                let eq = defined(self.cg.eq_id);
                self.emit(I::Call(eq));
                if matches!(op, PrimOp::StrNe | PrimOp::BytesNe) {
                    self.emit(I::I32Eqz);
                }
                self.bool_from_flag()?;
            }
            PrimOp::StrLength | PrimOp::BytesLength => {
                self.compile(arg(0)?)?;
                self.emit(I::I32Load(mem(8)));
            }
            PrimOp::StrSlice | PrimOp::BytesSlice => {
                for i in 0..3 {
                    self.compile(arg(i)?)?;
                }
                let slice = defined(self.cg.slice_id);
                self.emit(I::Call(slice));
            }
            PrimOp::StrConcat | PrimOp::BytesConcat => {
                self.compile(arg(0)?)?;
                self.compile(arg(1)?)?;
                let concat = defined(self.cg.concat_id);
                self.emit(I::Call(concat));
            }
            PrimOp::StrToBytes | PrimOp::StrFromBytes => {
                self.compile(arg(0)?)?;
            }
            PrimOp::BytesUtf8Check => {
                self.compile(arg(0)?)?;
                let utf8 = defined(self.cg.utf8_id);
                self.emit(I::Call(utf8));
            }
            PrimOp::BytesGet => {
                self.compile(arg(0)?)?;
                self.compile(arg(1)?)?;
                let get = defined(self.cg.get_id);
                self.emit(I::Call(get));
            }
        }
        Ok(())
    }

    fn compile_app(&mut self, f: &RcTm, a: &RcTm) -> Result<()> {
        let c = self.local();
        self.compile(f)?;
        self.emit(I::LocalSet(c));
        self.emit(I::LocalGet(c));
        self.compile(a)?;
        self.emit(I::LocalGet(c));
        self.emit(I::I32Load(mem(4)));
        let closure_ty = self.cg.closure_ty();
        self.emit(I::CallIndirect {
            type_index: closure_ty,
            table_index: 0,
        });
        Ok(())
    }

    /// Compile a term into a zero-argument thunk. The closure takes an ignored
    /// argument but the body is compiled in the current environment, not under
    /// a binder.
    fn compile_thunk(&mut self, body: &RcTm) -> Result<()> {
        self.compile_thunk_with(|child| child.compile(body))
    }

    /// Build a zero-argument action thunk whose body is emitted by `emit`.
    fn compile_thunk_with(
        &mut self,
        emit: impl FnOnce(&mut FnCompiler<'_, 'g>) -> Result<()>,
    ) -> Result<()> {
        let caps = self.env.clone();
        let (id, slot) = self.cg.new_lambda_func();
        self.make_closure(slot, &caps);

        let fin = {
            let mut child = FnCompiler::new(&mut *self.cg, 2);
            child.env = captured_env(caps.len());
            emit(&mut child)?;
            child.finish()
        };
        self.cg.set_body(id, fin);
        Ok(())
    }

    /// Compile `io_bind(m, k)` into a thunk that runs `m`, applies `k`, and
    /// runs the resulting action.
    fn compile_bind(&mut self, action: &RcTm, next: &RcTm) -> Result<()> {
        let caps = self.env.clone();
        let (id, slot) = self.cg.new_lambda_func();
        self.make_closure(slot, &caps);

        let fin = {
            let mut child = FnCompiler::new(&mut *self.cg, 2);
            child.env = captured_env(caps.len());

            // Run the first action; the argument is ignored.
            child.compile(action)?;
            let first = child.local();
            child.emit(I::LocalSet(first));
            emit_call_closure(&mut child, first, None);
            let produced = child.local();
            child.emit(I::LocalSet(produced));

            // Apply the continuation to the produced value, then run the
            // action it returns.
            child.compile(next)?;
            let continuation = child.local();
            child.emit(I::LocalSet(continuation));
            emit_call_closure(&mut child, continuation, Some(produced));
            let deferred = child.local();
            child.emit(I::LocalSet(deferred));
            emit_call_closure(&mut child, deferred, None);
            child.finish()
        };
        self.cg.set_body(id, fin);
        Ok(())
    }

    fn compile_lambda(&mut self, body: &RcTm) -> Result<()> {
        let caps = self.env.clone();
        let (id, slot) = self.cg.new_lambda_func();
        self.make_closure(slot, &caps);

        let fin = {
            let mut child = FnCompiler::new(&mut *self.cg, 2);
            let mut env = Vec::with_capacity(1 + caps.len());
            env.push(Place {
                local: 1,
                offset: None,
            });
            for j in 0..caps.len() {
                env.push(Place {
                    local: 0,
                    offset: Some(8 + 4 * j as u32),
                });
            }
            child.env = env;
            child.compile(body)?;
            child.finish()
        };
        self.cg.set_body(id, fin);
        Ok(())
    }

    fn compile_match(
        &mut self,
        motive: &RcTm,
        scrut: &RcTm,
        branches: &[crate::core::Branch],
    ) -> Result<()> {
        let Tm::Lam { dom, .. } = &**motive else {
            return Err(Error::new("match motive is not a lambda"));
        };
        let Tm::Ind(enum_name, _) = &**dom else {
            return Err(Error::new("match motive does not range over an enum"));
        };
        let enum_name = enum_name.clone();
        let mut entries: Vec<(usize, usize, RcTm)> = Vec::new();
        for b in branches {
            if b.impossible {
                continue;
            }
            let ci = self
                .cg
                .globals
                .get_ctor(&format!("{}::{}", enum_name, b.ctor))
                .ok_or_else(|| Error::new(format!("unknown constructor `{}` in match", b.ctor)))?;
            entries.push((ci.index, b.binders, b.body.clone()));
        }
        entries.sort_by_key(|e| e.0);

        let s = self.local();
        self.compile(scrut)?;
        self.emit(I::LocalSet(s));
        self.match_chain(s, &entries, 0)
    }

    fn match_chain(
        &mut self,
        scrut: u32,
        entries: &[(usize, usize, RcTm)],
        pos: usize,
    ) -> Result<()> {
        if pos >= entries.len() {
            self.emit(I::Unreachable);
            return Ok(());
        }
        let (index, binders, body) = &entries[pos];
        self.emit(I::LocalGet(scrut));
        self.emit(I::I32Load(mem(0)));
        self.emit(I::I32Const(*index as i32));
        self.emit(I::I32Eq);
        self.emit(I::If(BlockType::Result(ValType::I32)));

        let saved = self.env.clone();
        let mut env = Vec::with_capacity(binders + saved.len());
        // Branch binders are pushed in declaration order, so the last field
        // has de Bruijn index 0.
        for i in 0..*binders {
            let field = binders - 1 - i;
            env.push(Place {
                local: scrut,
                offset: Some(4 + 4 * field as u32),
            });
        }
        env.extend(saved.iter().copied());
        self.env = env;
        self.compile(body)?;
        self.env = saved;

        self.emit(I::Else);
        self.match_chain(scrut, entries, pos + 1)?;
        self.emit(I::End);
        Ok(())
    }

    fn print_enum_branches(
        &mut self,
        entries: &[(usize, String, Vec<usize>)],
        pos: usize,
    ) -> Result<()> {
        if pos >= entries.len() {
            self.emit(I::Unreachable);
            return Ok(());
        }
        let (index, name, printers) = &entries[pos];
        self.emit(I::LocalGet(0));
        self.emit(I::I32Load(mem(0)));
        self.emit(I::I32Const(*index as i32));
        self.emit(I::I32Eq);
        self.emit(I::If(BlockType::Empty));

        self.print_str(name);
        if !printers.is_empty() {
            self.print_str("(");
            for (j, pid) in printers.iter().enumerate() {
                if j > 0 {
                    self.print_str(", ");
                }
                self.emit(I::LocalGet(0));
                self.emit(I::I32Load(mem(4 + 4 * j as u32)));
                self.emit(I::Call(defined(*pid)));
            }
            self.print_str(")");
        }

        self.emit(I::Else);
        self.print_enum_branches(entries, pos + 1)?;
        self.emit(I::End);
        Ok(())
    }
}

fn type_key(name: &str, args: &[V]) -> String {
    if args.is_empty() {
        return name.to_string();
    }
    let mut out = String::new();
    out.push_str(name);
    out.push('<');
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&value_key(a));
    }
    out.push('>');
    out
}

fn value_key(v: &V) -> String {
    match &**v {
        Val::Type(u) => format!("Type{}", u),
        Val::Prop => "Prop".to_string(),
        Val::PrimTy(t) => t.name().to_string(),
        Val::PrimLit(lit) => lit.show(),
        Val::Ind(name, args) => type_key(name, args),
        Val::Ctor(name, index, args) => {
            let mut out = format!("{}#{}", name, index);
            if !args.is_empty() {
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&value_key(a));
                }
                out.push(')');
            }
            out
        }
        Val::Eq(a, b, c) => {
            format!("Eq({},{},{})", value_key(a), value_key(b), value_key(c))
        }
        Val::Refl(a) => format!("Refl({})", value_key(a)),
        Val::Io(a) => format!("IO<{}>", value_key(a)),
        Val::Action(_) => "Action".to_string(),
        Val::Neutral(n) => match &**n {
            crate::value::Neutral::Var(i) => format!("Var{}", i),
            crate::value::Neutral::Global(g) => format!("Global{}", g),
            crate::value::Neutral::PrimOp(op, args) => {
                let args: Vec<String> = args.iter().map(value_key).collect();
                format!("{}({})", op.name(), args.join(","))
            }
            _ => "Neutral".to_string(),
        },
        Val::Pi(..) | Val::Lam(..) | Val::Glob(_) => "Opaque".to_string(),
    }
}

fn contains_neutral(v: &V) -> bool {
    match &**v {
        Val::Neutral(n) => neutral_has_var(n),
        Val::Ind(_, args) => args.iter().any(contains_neutral),
        Val::Ctor(_, _, args) => args.iter().any(contains_neutral),
        Val::Eq(a, b, c) => contains_neutral(a) || contains_neutral(b) || contains_neutral(c),
        Val::Refl(a) => contains_neutral(a),
        Val::Pi(dom, _, _) => contains_neutral(dom),
        Val::Lam(_) => false,
        Val::Io(a) => contains_neutral(a),
        Val::Action(_) => true,
        Val::Type(_) | Val::Prop | Val::PrimTy(_) | Val::PrimLit(_) | Val::Glob(_) => false,
    }
}

fn neutral_has_var(n: &crate::value::Neutral) -> bool {
    match n {
        crate::value::Neutral::Var(_) => true,
        crate::value::Neutral::App(f, a) => neutral_has_var(f) || contains_neutral(a),
        crate::value::Neutral::Match { scrut, .. } => contains_neutral(scrut),
        crate::value::Neutral::J { proof, .. } => contains_neutral(proof),
        crate::value::Neutral::PrimOp(_, args) => args.iter().any(contains_neutral),
        crate::value::Neutral::Global(_) => false,
    }
}

fn resolve_wit() -> Result<(Resolve, WorldId)> {
    let mut resolve = Resolve::new();
    let parse = |path: &str, text: &str| {
        UnresolvedPackageGroup::parse(path, text)
            .map_err(|(_, e)| Error::new(format!("failed to parse {}: {:?}", path, e)))
    };
    let deps = vec![
        parse("wit/deps/io.wit", WIT_IO)?,
        parse("wit/deps/clocks.wit", WIT_CLOCKS)?,
        parse("wit/deps/random.wit", WIT_RANDOM)?,
        parse("wit/deps/filesystem.wit", WIT_FILESYSTEM)?,
        parse("wit/deps/http.wit", WIT_HTTP)?,
        parse("wit/deps/sockets.wit", WIT_SOCKETS)?,
        parse("wit/deps/cli.wit", WIT_CLI)?,
    ];
    let main = parse("wit/neo.wit", NEO_WIT)?;
    let pkg = resolve
        .push_groups(main, deps)
        .map_err(|e| Error::new(format!("failed to resolve WIT: {:?}", e)))?;
    let world = resolve
        .select_world(&[pkg], Some("neo"))
        .map_err(|e| Error::new(format!("failed to select WIT world: {:?}", e)))?;
    Ok((resolve, world))
}

/// Compile the elaborated program to a WASI 0.2 component.
pub fn build_component(globals: &Globals, output: &Path) -> Result<()> {
    let component = build_component_bytes(globals)?;
    std::fs::write(output, component)
        .map_err(|e| Error::new(format!("cannot write `{}`: {}", output.display(), e)))?;
    Ok(())
}

/// Compile the elaborated program to component bytes without writing a file.
///
/// This is the byte-level entry point used by the bootstrap oracle, which
/// hashes and compares builds.
pub fn build_component_bytes(globals: &Globals) -> Result<Vec<u8>> {
    // The prelude defines no `main`; only the user's program is compiled.
    let cg = Codegen::new(globals);
    let core = cg.build()?;

    let (resolve, world) = resolve_wit()?;
    let mut bytes = core;
    embed_component_metadata(&mut bytes, &resolve, world, StringEncoding::UTF8, true)
        .map_err(|e| Error::new(format!("failed to embed component metadata: {}", e)))?;

    ComponentEncoder::default()
        .module(&bytes)
        .map_err(|e| Error::new(format!("failed to parse core module: {}", e)))?
        .validate(true)
        .encode()
        .map_err(|e| Error::new(format!("failed to encode component: {:#}", e)))
}
