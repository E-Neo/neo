//! Deterministic reports used by the self-hosting test corpus.
//!
//! This is development tooling, not part of the language. For each program it
//! records whether elaboration succeeds, the type of `main` as rendered by the
//! kernel pretty printer, and the size and 64-bit hash of the compiled
//! component. Comparing these reports across processes is the determinism gate
//! for the bootstrap work.

use std::path::Path;

use crate::codegen::build_component_bytes;
use crate::elab::elaborate_program;
use crate::env::Globals;
use crate::error::{Error, Result};
use crate::parser::parse_source;
use crate::prelude::new_globals;
use crate::pretty::show_term;

/// One tab-separated line describing a program.
///
/// The fields are: verdict, `main` type, byte length, and hash. Escaping keeps
/// a diagnostic on one line.
pub fn report(source: &str) -> String {
    match elaborate(source) {
        Err(e) => format!("reject\t{}\t-\t-", escape(&e.message)),
        Ok(globals) => {
            let main = globals
                .get_ty("main")
                .map(|ty| show_term(&globals, ty))
                .unwrap_or_else(|| "<none>".to_string());
            match build_component_bytes(&globals) {
                Ok(bytes) => format!(
                    "ok\t{}\t{}\t0x{:016x}",
                    escape(&main),
                    bytes.len(),
                    hash(&bytes)
                ),
                Err(e) => format!("build-error\t{}\t-\t-", escape(&e.message)),
            }
        }
    }
}

/// Parse and elaborate a program against a fresh prelude.
pub fn elaborate(source: &str) -> Result<Globals> {
    let mut globals = new_globals()?;
    let program = parse_source(source)?;
    elaborate_program(&mut globals, &program)?;
    Ok(globals)
}

/// Report every corpus program under `root`, sorted by path.
///
/// The corpus is `examples/*.neo` plus `tests/corpus/*.neo`. Output paths are
/// relative to `root` and use `/`.
pub fn corpus_report(root: &Path) -> Result<String> {
    let mut out = String::new();
    for path in corpus_files(root)? {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| Error::new(format!("`{}` is outside the corpus root", path.display())))?
            .to_string_lossy()
            .replace('\\', "/");
        let source = std::fs::read_to_string(&path)
            .map_err(|e| Error::new(format!("cannot read `{}`: {}", path.display(), e)))?;
        out.push_str(&relative);
        out.push('\t');
        out.push_str(&report(&source));
        out.push('\n');
    }
    Ok(out)
}

/// Every `.neo` file in the corpus, in sorted order.
pub fn corpus_files(root: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for relative in ["examples", "tests/corpus"] {
        let dir = root.join(relative);
        if !dir.exists() {
            continue;
        }
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| Error::new(format!("cannot read `{}`: {}", dir.display(), e)))?;
        for entry in entries {
            let entry = entry.map_err(|e| Error::new(e.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("neo") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
}

/// FNV-1a, 64 bit. Stable across processes and platforms, unlike `DefaultHasher`.
fn hash(bytes: &[u8]) -> u64 {
    let mut value: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value
}
