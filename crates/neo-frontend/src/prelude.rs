use crate::elab::elaborate_prelude;
use crate::env::Globals;
use crate::parser::parse_source;
use crate::surface::Program;
use neo_common::error::Result;

/// The Stage 0 prelude, split into files and embedded into the compiler.
pub const PRELUDE_FILES: &[&str] = &[
    include_str!("prelude/core.neo"),
    include_str!("prelude/stdlib.neo"),
    include_str!("prelude/host.neo"),
];

/// Parse and merge the embedded prelude files into one root program.
pub fn prelude_program() -> Result<Program> {
    let mut items = Vec::new();
    for source in PRELUDE_FILES {
        items.extend(parse_source(source)?.items);
    }
    Ok(Program {
        items,
        files: Vec::new(),
    })
}

/// Install the prelude declarations into a fresh global environment.
pub fn install(globals: &mut Globals) -> Result<()> {
    let program = prelude_program()?;
    elaborate_prelude(globals, &program)
}

pub fn new_globals() -> Result<Globals> {
    let mut globals = Globals::new();
    install(&mut globals)?;
    Ok(globals)
}
