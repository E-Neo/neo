//! Host capabilities and the interpreter for `IO` actions.
//!
//! The trusted kernel represents a host action as an opaque
//! [`neo_kernel::value::Action::Extern`] value. This crate owns the closed
//! table of capabilities, validates that a program only names capabilities
//! the backends know, and executes actions for the interpreter and REPL.

use neo_common::error::{Error, Result};
use neo_kernel::core::{PrimLit, PrimTy, RcTm, ind, io, pi, prim_ty};
use neo_kernel::env::Globals;
use neo_kernel::eval::{force, vapp};
use neo_kernel::value::{Action, V, Val, vctor, vind};

/// How a backend lowers a capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lowering {
    /// `wasi:cli/stderr@0.2.9#write`: write a `String` to standard error.
    StderrWrite,
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
pub const CAPABILITIES: &[Capability] = &[Capability {
    import: "wasi:cli/stderr@0.2.9#write",
    ty: stderr_write_ty,
    lowering: Lowering::StderrWrite,
}];

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
