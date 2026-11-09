//! Neo: a dependently typed language with a small trusted proof kernel.

pub use neo_codegen::codegen;
pub use neo_common::error;
pub use neo_frontend::{elab, lexer, parser, prelude, surface};
pub use neo_kernel::{check, conv, core, env, eval, positivity, pretty, quote, recursion, value};
