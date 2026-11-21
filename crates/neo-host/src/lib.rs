//! Host capabilities and the interpreter for `IO` actions.
//!
//! The trusted kernel represents a host action as an opaque
//! [`neo_kernel::value::Action::Extern`] value. This crate owns the closed
//! table of capabilities, validates that a program only names capabilities
//! the backends know, and executes actions for the interpreter and REPL.

use std::io::Read;

use neo_common::error::{Error, Result};
use neo_kernel::core::{PrimLit, PrimTy, RcTm, ind, io, pi, prim_ty};
use neo_kernel::env::Globals;
use neo_kernel::eval::{force, vapp};
use neo_kernel::value::{Action, V, Val, vctor, vind, vprimlit, vprimty};

/// How a backend lowers a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lowering {
    /// Write a `String` to standard error.
    StderrWrite,
    /// Read the process argument vector as `List[String]`.
    Argv,
    /// Read standard input to end as `Bytes`.
    StdinReadAll,
    /// Read a file path as `Bytes`.
    FileRead,
    /// Write `Bytes` to a file path, truncating or creating it.
    FileWrite,
}

/// A capability the compiler knows how to lower and execute.
#[derive(Clone, Copy, Debug)]
pub struct Capability {
    /// The explicit import string written in an `extern` declaration.
    pub import: &'static str,
    /// The closed Neo type of the capability.
    pub ty: fn() -> RcTm,
    pub lowering: Lowering,
}

/// The closed capability registry. Unknown imports are rejected during
/// elaboration so the interpreter and the WASM backend accept the same
/// programs.
pub const CAPABILITIES: &[Capability] = &[
    Capability {
        import: "wasi:cli/stderr@0.2.9#write",
        ty: stderr_write_ty,
        lowering: Lowering::StderrWrite,
    },
    Capability {
        import: "wasi:cli/environment@0.2.9#get-arguments",
        ty: argv_ty,
        lowering: Lowering::Argv,
    },
    Capability {
        import: "neo:stdin@0.1.0#read-all",
        ty: stdin_read_all_ty,
        lowering: Lowering::StdinReadAll,
    },
    Capability {
        import: "neo:file@0.1.0#read",
        ty: file_read_ty,
        lowering: Lowering::FileRead,
    },
    Capability {
        import: "neo:file@0.1.0#write",
        ty: file_write_ty,
        lowering: Lowering::FileWrite,
    },
];

pub fn lookup(import: &str) -> Option<&'static Capability> {
    CAPABILITIES.iter().find(|cap| cap.import == import)
}

fn result_unit_io_error() -> RcTm {
    ind(
        "Result",
        vec![ind("Unit", Vec::new()), ind("IoError", Vec::new())],
    )
}

fn stderr_write_ty() -> RcTm {
    pi("s", prim_ty(PrimTy::String), io(result_unit_io_error()))
}

fn bytes_ty() -> RcTm {
    prim_ty(PrimTy::Bytes)
}

fn result_bytes_io_error() -> RcTm {
    ind("Result", vec![bytes_ty(), ind("IoError", Vec::new())])
}

fn argv_ty() -> RcTm {
    io(ind("List", vec![prim_ty(PrimTy::String)]))
}

fn stdin_read_all_ty() -> RcTm {
    io(result_bytes_io_error())
}

fn file_read_ty() -> RcTm {
    pi("path", prim_ty(PrimTy::String), io(result_bytes_io_error()))
}

fn file_write_ty() -> RcTm {
    pi(
        "path",
        prim_ty(PrimTy::String),
        pi(
            "data",
            bytes_ty(),
            io(ind(
                "Result",
                vec![ind("Unit", Vec::new()), ind("IoError", Vec::new())],
            )),
        ),
    )
}

/// A host that can execute capabilities. Implementations decode arguments and
/// return the value declared by the capability's Neo type.
pub trait Host {
    fn run(&mut self, capability: &Capability, args: &[V]) -> Result<V>;
}

/// Execute an action value once. Actions are deferred values, so this is the
/// only place where a program's side effects happen.
pub fn run_action(globals: &Globals, host: &mut dyn Host, action: &V) -> Result<V> {
    let action = force(globals, action.clone());
    match &*action {
        Val::Action(a) => match &**a {
            Action::Pure(v) => Ok(v.clone()),
            Action::Bind(m, k) => {
                let produced = run_action(globals, host, m)?;
                let next = vapp(globals, k.clone(), produced);
                run_action(globals, host, &next)
            }
            Action::Extern { import, args, .. } => {
                let capability = lookup(import)
                    .ok_or_else(|| Error::new(format!("unknown host capability `{}`", import)))?;
                host.run(capability, args)
            }
        },
        _ => Err(Error::new("`main` did not evaluate to an `IO` action")),
    }
}

/// The real host used by the interpreter and REPL.
#[derive(Clone, Copy, Debug, Default)]
pub struct StdioHost;

impl Host for StdioHost {
    fn run(&mut self, capability: &Capability, args: &[V]) -> Result<V> {
        match capability.lowering {
            Lowering::StderrWrite => {
                let [arg] = args else {
                    return Err(Error::new("`stderr.write` takes one argument"));
                };
                let bytes = string_arg(arg)?;
                let outcome = write_stderr(bytes);
                Ok(stderr_result(outcome))
            }
            Lowering::Argv => {
                let args = std::env::args_os()
                    .map(|arg| ok_string(arg.to_string_lossy().as_bytes()))
                    .collect();
                Ok(vlist(args))
            }
            Lowering::StdinReadAll => {
                let mut bytes = Vec::new();
                match std::io::stdin().read_to_end(&mut bytes) {
                    Ok(_) => Ok(ok_bytes(bytes)),
                    Err(_) => Ok(err_bytes()),
                }
            }
            Lowering::FileRead => {
                let [path] = args else {
                    return Err(Error::new("`file.read` takes one argument"));
                };
                let path = string_arg(path)?;
                let path = match std::str::from_utf8(path) {
                    Ok(path) => path,
                    Err(_) => return Ok(err_bytes()),
                };
                match std::fs::read(path) {
                    Ok(bytes) => Ok(ok_bytes(bytes)),
                    Err(_) => Ok(err_bytes()),
                }
            }
            Lowering::FileWrite => {
                let [path, data] = args else {
                    return Err(Error::new("`file.write` takes two arguments"));
                };
                let path = string_arg(path)?;
                let path = match std::str::from_utf8(path) {
                    Ok(path) => path,
                    Err(_) => return Ok(err_unit()),
                };
                let data = bytes_arg(data)?;
                match std::fs::write(path, data) {
                    Ok(()) => Ok(ok_unit()),
                    Err(_) => Ok(err_unit()),
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StderrOutcome {
    Ok,
    Closed,
    Failed,
}

fn string_arg(v: &V) -> Result<&[u8]> {
    match &**v {
        Val::PrimLit(PrimLit::Str(bytes)) => Ok(bytes),
        _ => Err(Error::new("host capability expected a `String` argument")),
    }
}

fn bytes_arg(v: &V) -> Result<&[u8]> {
    match &**v {
        Val::PrimLit(PrimLit::Bytes(bytes)) => Ok(bytes),
        _ => Err(Error::new("host capability expected a `Bytes` argument")),
    }
}

fn write_stderr(bytes: &[u8]) -> StderrOutcome {
    use std::io::Write;
    let mut stderr = std::io::stderr().lock();
    match stderr.write_all(bytes).and_then(|()| stderr.flush()) {
        Ok(()) => StderrOutcome::Ok,
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => StderrOutcome::Closed,
        Err(_) => StderrOutcome::Failed,
    }
}

/// Build the `Result[Unit, IoError]` value a `stderr.write` capability
/// produces. Host implementations use this so their results match the
/// WASM backend.
pub fn stderr_result(outcome: StderrOutcome) -> V {
    let unit_ty = vind("Unit", Vec::new());
    let error_ty = vind("IoError", Vec::new());
    match outcome {
        StderrOutcome::Ok => {
            let unit = vctor("Unit", 0, Vec::new());
            vctor("Result", 0, vec![unit_ty, error_ty, unit])
        }
        StderrOutcome::Closed => {
            let closed = vctor("IoError", 0, Vec::new());
            vctor("Result", 1, vec![unit_ty, error_ty, closed])
        }
        StderrOutcome::Failed => {
            let failed = vctor("IoError", 1, Vec::new());
            vctor("Result", 1, vec![unit_ty, error_ty, failed])
        }
    }
}

fn error_ty() -> V {
    vind("IoError", Vec::new())
}

fn string_ty() -> V {
    vprimty(PrimTy::String)
}

fn bytes_value(bytes: Vec<u8>) -> V {
    vprimlit(PrimLit::Bytes(std::rc::Rc::from(bytes.as_slice())))
}

fn ok_string(bytes: &[u8]) -> V {
    vprimlit(PrimLit::Str(std::rc::Rc::from(bytes)))
}

fn vlist(items: Vec<V>) -> V {
    let mut list = vctor("List", 0, vec![string_ty()]);
    for item in items.into_iter().rev() {
        list = vctor("List", 1, vec![string_ty(), item, list]);
    }
    list
}

fn ok_bytes(bytes: Vec<u8>) -> V {
    vctor(
        "Result",
        0,
        vec![vprimty(PrimTy::Bytes), error_ty(), bytes_value(bytes)],
    )
}

fn err_bytes() -> V {
    err_value(bytes_ty_value())
}

fn ok_unit() -> V {
    vctor(
        "Result",
        0,
        vec![
            vind("Unit", Vec::new()),
            error_ty(),
            vctor("Unit", 0, Vec::new()),
        ],
    )
}

fn err_unit() -> V {
    err_value(vind("Unit", Vec::new()))
}

fn bytes_ty_value() -> V {
    vprimty(PrimTy::Bytes)
}

fn err_value(ok_ty: V) -> V {
    vctor(
        "Result",
        1,
        vec![ok_ty, error_ty(), vctor("IoError", 1, Vec::new())],
    )
}
