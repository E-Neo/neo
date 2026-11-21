//! Keep the examples used by the documentation honest: every `.neo` file in
//! `examples/` must parse, elaborate, and evaluate its `main` successfully.
#![cfg(not(target_family = "wasm"))]

use std::fs;
use std::path::PathBuf;

use neo::elab::elaborate_program;
use neo::eval::{eval, force};
use neo::parser::parse_source;
use neo::prelude::new_globals;
use neo::value::{V, Val};

/// The examples Only need to produce their values; host writes are discarded
/// instead of touching the test process's stderr.
struct SilentHost;

impl neo::host::Host for SilentHost {
    fn run(&mut self, capability: &neo::host::Capability, _args: &[V]) -> neo::error::Result<V> {
        match capability.lowering {
            neo::host::Lowering::StderrWrite => {
                Ok(neo::host::stderr_result(neo::host::StderrOutcome::Ok))
            }
            _ => Err(neo::error::Error::new("unexpected capability")),
        }
    }
}

#[test]
fn examples_elaborate_and_evaluate() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut checked = 0;

    for entry in fs::read_dir(&dir).expect("read examples directory") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("neo") {
            continue;
        }

        let source = fs::read_to_string(&path).expect("read example");
        let mut globals = new_globals().expect("prelude");
        let program = parse_source(&source).unwrap_or_else(|e| panic!("{}: {}", path.display(), e));
        elaborate_program(&mut globals, &program)
            .unwrap_or_else(|e| panic!("{}: {}", path.display(), e));

        assert!(
            globals.get_ty("main").is_some(),
            "{} does not define `main`",
            path.display()
        );

        let action = eval(&globals, &Vec::new(), &neo::core::const_("main"));
        let value = force(
            &globals,
            neo::host::run_action(&globals, &mut SilentHost, &action)
                .unwrap_or_else(|e| panic!("{}: {}", path.display(), e)),
        );
        assert!(
            matches!(&*value, Val::Ctor(..) | Val::PrimLit(..)),
            "{}: `main` did not evaluate to a data value",
            path.display()
        );

        checked += 1;
    }

    assert!(
        checked >= 8,
        "expected at least 8 examples, checked {checked}"
    );
}
