//! The Neo backend: compiles checked programs into WASI 0.2 components.

pub mod codegen;

pub use neo_kernel::{core, env, eval, value};
