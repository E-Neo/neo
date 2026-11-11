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

use crate::core::{RcTm, Tm};
use crate::env::Globals;
use crate::eval::{apply_pi, eval, eval_closure, force};
use crate::value::{V, Val, vvar};
use neo_common::error::{Error, Result};

/// The tag stored in the first word of a closure block. Constructor indices are
/// small, so this cannot collide with an enum tag.
const TAG_CLOSURE: i32 = 0x7fff_ffffu32 as i32;

/// Reserved low-memory scratch space. The first 32 bytes hold WASI return
/// areas, bytes 64..96 hold the decimal conversion buffer.
const RETPTR: i32 = 0;
const DIGIT_END: i32 = 96;

/// Static strings are placed here, above the scratch area.
const DATA_BASE: u32 = 1024;

/// The number of imported WASI functions. Function indices start after these.
const NUM_IMPORTS: u32 = 3;

const HEAP_START: u32 = 65_536;

const IMPORT_GET_STDOUT: u32 = 0;
const IMPORT_WRITE: u32 = 1;
const IMPORT_FLUSH: u32 = 2;

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

    alloc_id: usize,
    print_bytes_id: usize,
    print_u32_id: usize,
    print_nat_id: usize,
    print_bool_id: usize,
    print_unit_id: usize,
    print_string_id: usize,
    run_id: usize,

    succ_index: i32,
}

impl<'g> Codegen<'g> {
    fn new(globals: &'g Globals) -> Self {
        let succ_index = globals
            .get_enum("Nat")
            .and_then(|e| e.ctors.iter().position(|c| c.name == "succ"))
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
            alloc_id: 0,
            print_bytes_id: 0,
            print_u32_id: 0,
            print_nat_id: 0,
            print_bool_id: 0,
            print_unit_id: 0,
            print_string_id: 0,
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
        self.print_string_id = self.new_func(&[ValType::I32], &[]);
        self.run_id = self.new_func(&[], &[ValType::I32]);

        self.gen_alloc();
        self.gen_print_bytes();
        self.gen_print_u32();
        self.gen_print_nat();
        self.gen_print_bool();
        self.gen_print_unit();
        self.gen_print_string()?;
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

    fn gen_print_bytes(&mut self) {
        let mut fc = FnCompiler::new(self, 2);
        let out = fc.local();
        fc.emit(I::Call(IMPORT_GET_STDOUT));
        fc.emit(I::LocalSet(out));
        fc.emit(I::LocalGet(out));
        fc.emit(I::LocalGet(0));
        fc.emit(I::LocalGet(1));
        fc.emit(I::I32Const(RETPTR));
        fc.emit(I::Call(IMPORT_WRITE));
        fc.emit(I::LocalGet(out));
        fc.emit(I::I32Const(RETPTR));
        fc.emit(I::Call(IMPORT_FLUSH));
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
        let (t_ptr, t_len) = self.intern("true");
        let (f_ptr, f_len) = self.intern("false");
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
        let (ptr, len) = self.intern("unit");
        let pb = self.print_bytes_fn();
        let mut fc = FnCompiler::new(self, 1);
        fc.emit(I::I32Const(ptr));
        fc.emit(I::I32Const(len));
        fc.emit(I::Call(pb));
        let body = fc.finish();
        let id = self.print_unit_id;
        self.set_body(id, body);
    }

    fn gen_print_string(&mut self) -> Result<()> {
        let string_info = self
            .globals
            .get_enum("String")
            .ok_or_else(|| Error::new("the prelude is missing `String`"))?;
        let nil = string_info
            .ctors
            .iter()
            .position(|c| c.name == "str_nil")
            .ok_or_else(|| Error::new("`String` is missing `str_nil`"))? as i32;
        let cons = string_info
            .ctors
            .iter()
            .position(|c| c.name == "str_cons")
            .ok_or_else(|| Error::new("`String` is missing `str_cons`"))? as i32;
        let nat_info = self
            .globals
            .get_enum("Nat")
            .ok_or_else(|| Error::new("the prelude is missing `Nat`"))?;
        let succ = nat_info
            .ctors
            .iter()
            .position(|c| c.name == "succ")
            .ok_or_else(|| Error::new("`Nat` is missing `succ`"))? as i32;

        let print_bytes = self.print_bytes_fn();
        let mut fc = FnCompiler::new(self, 1);
        let cursor = fc.local();
        let byte = fc.local();
        let count = fc.local();

        fc.emit(I::LocalGet(0));
        fc.emit(I::LocalSet(cursor));

        // Walk the cons list.
        fc.emit(I::Block(BlockType::Empty));
        fc.emit(I::Loop(BlockType::Empty));
        fc.emit(I::LocalGet(cursor));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::I32Const(nil));
        fc.emit(I::I32Eq);
        fc.emit(I::BrIf(1));
        fc.emit(I::LocalGet(cursor));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::I32Const(cons));
        fc.emit(I::I32Ne);
        fc.emit(I::If(BlockType::Empty));
        fc.emit(I::Unreachable);
        fc.emit(I::End);

        // Convert the unary `Nat` byte to a machine integer.
        fc.emit(I::LocalGet(cursor));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::LocalSet(byte));
        fc.emit(I::I32Const(0));
        fc.emit(I::LocalSet(count));
        fc.emit(I::Block(BlockType::Empty));
        fc.emit(I::Loop(BlockType::Empty));
        fc.emit(I::LocalGet(byte));
        fc.emit(I::I32Load(mem(0)));
        fc.emit(I::I32Const(succ));
        fc.emit(I::I32Ne);
        fc.emit(I::BrIf(1));
        fc.emit(I::LocalGet(count));
        fc.emit(I::I32Const(1));
        fc.emit(I::I32Add);
        fc.emit(I::LocalSet(count));
        fc.emit(I::LocalGet(byte));
        fc.emit(I::I32Load(mem(4)));
        fc.emit(I::LocalSet(byte));
        fc.emit(I::Br(0));
        fc.emit(I::End);
        fc.emit(I::End);

        // Store the low byte and write it as a single-byte string.
        fc.emit(I::I32Const(DIGIT_END));
        fc.emit(I::LocalGet(count));
        fc.emit(I::I32Const(255));
        fc.emit(I::I32And);
        fc.emit(I::I32Store8(mem8(0)));
        fc.emit(I::I32Const(DIGIT_END));
        fc.emit(I::I32Const(1));
        fc.emit(I::Call(print_bytes));

        // Advance to the tail.
        fc.emit(I::LocalGet(cursor));
        fc.emit(I::I32Load(mem(8)));
        fc.emit(I::LocalSet(cursor));
        fc.emit(I::Br(0));
        fc.emit(I::End);
        fc.emit(I::End);

        let body = fc.finish();
        let id = self.print_string_id;
        self.set_body(id, body);
        Ok(())
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
            Val::Ind(name, args) => match name.as_str() {
                "Nat" => Ok(self.print_nat_id),
                "Bool" => Ok(self.print_bool_id),
                "Unit" => Ok(self.print_unit_id),
                "String" => Ok(self.print_string_id),
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
        // such as `List(T)` can call themselves.
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
                    Val::Pi(dom, closure) => (dom.clone(), closure.clone()),
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
            fc.emit(I::Call(defined(main_id)));
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
            .ok_or_else(|| Error::new("this program needs `fn main(): T`"))?;
        let main_ty = force(self.globals, eval(self.globals, &Vec::new(), &main_ty_tm));
        if matches!(&*main_ty, Val::Pi(..)) {
            return Err(Error::new("`main` must not take parameters"));
        }
        let printer_id = self.printer_for(&main_ty)?;
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
        let write_ty = self.ty(
            &[ValType::I32, ValType::I32, ValType::I32, ValType::I32],
            &[],
        );
        let flush_ty = self.ty(&[ValType::I32, ValType::I32], &[]);

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
            "wasi:io/streams@0.2.9",
            "[method]output-stream.write",
            EntityType::Function(write_ty),
        );
        imports.import(
            "wasi:io/streams@0.2.9",
            "[method]output-stream.blocking-flush",
            EntityType::Function(flush_ty),
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
            Tm::Const(name) => self.compile_const(name)?,
            Tm::Ctor(enum_name, index, args) => self.compile_ctor(enum_name, *index, args)?,
            Tm::App(f, a) => self.compile_app(f, a)?,
            Tm::Match {
                motive: _,
                scrut,
                branches,
            } => self.compile_match(scrut, branches)?,
            Tm::J { refl_case, .. } => self.compile(refl_case)?,
            Tm::Lam { body, .. } => self.compile_lambda(body)?,
        }
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

    fn compile_match(&mut self, scrut: &RcTm, branches: &[crate::core::Branch]) -> Result<()> {
        let mut entries: Vec<(usize, usize, RcTm)> = Vec::new();
        for b in branches {
            if b.impossible {
                continue;
            }
            let ci =
                self.cg.globals.get_ctor(&b.ctor).ok_or_else(|| {
                    Error::new(format!("unknown constructor `{}` in match", b.ctor))
                })?;
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
        Val::Neutral(n) => match &**n {
            crate::value::Neutral::Var(i) => format!("Var{}", i),
            crate::value::Neutral::Global(g) => format!("Global{}", g),
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
        Val::Pi(dom, _) => contains_neutral(dom),
        Val::Lam(_) => false,
        Val::Type(_) | Val::Prop | Val::Glob(_) => false,
    }
}

fn neutral_has_var(n: &crate::value::Neutral) -> bool {
    match n {
        crate::value::Neutral::Var(_) => true,
        crate::value::Neutral::App(f, a) => neutral_has_var(f) || contains_neutral(a),
        crate::value::Neutral::Match { scrut, .. } => contains_neutral(scrut),
        crate::value::Neutral::J { proof, .. } => contains_neutral(proof),
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
    // The prelude defines no `main`; only the user's program is compiled.
    let cg = Codegen::new(globals);
    let core = cg.build()?;

    let (resolve, world) = resolve_wit()?;
    let mut bytes = core;
    embed_component_metadata(&mut bytes, &resolve, world, StringEncoding::UTF8, true)
        .map_err(|e| Error::new(format!("failed to embed component metadata: {}", e)))?;

    let component = ComponentEncoder::default()
        .module(&bytes)
        .map_err(|e| Error::new(format!("failed to parse core module: {}", e)))?
        .validate(true)
        .encode()
        .map_err(|e| Error::new(format!("failed to encode component: {:#}", e)))?;

    std::fs::write(output, component)
        .map_err(|e| Error::new(format!("cannot write `{}`: {}", output.display(), e)))?;
    Ok(())
}
