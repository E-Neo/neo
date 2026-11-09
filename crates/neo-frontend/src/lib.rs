//! The Neo frontend: lexer, parser, surface syntax, elaborator, and prelude.
//!
//! The frontend is untrusted. It produces core terms and enum declarations that
//! the kernel re-checks. A bug here can cause a program that should be accepted
//! to be rejected, but it cannot manufacture a false proof.

pub mod elab;
pub mod lexer;
pub mod parser;
pub mod prelude;
pub mod surface;

pub use neo_kernel::{check, conv, core, env, eval, positivity, pretty, quote, recursion, value};
