//! Development-only command line for the self-hosting corpus.
//!
//! This binary is not part of the language interface. It exists so the test
//! suite can record and compare deterministic compiler reports.

use std::path::Path;
use std::process::ExitCode;

use neo::error::{Error, Result};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<String> {
    match args.first().map(String::as_str) {
        Some("report") => {
            let path = args
                .get(1)
                .ok_or_else(|| Error::new("`report` needs a file"))?;
            let source = std::fs::read_to_string(path)
                .map_err(|e| Error::new(format!("cannot read `{path}`: {e}")))?;
            Ok(format!("{path}\t{}\n", neo::oracle::report(&source)))
        }
        Some("corpus") => {
            let root = match args.get(1) {
                Some(path) => Path::new(path),
                None => Path::new("."),
            };
            neo::oracle::corpus_report(root)
        }
        Some(other) => Err(Error::new(format!(
            "unknown command `{other}`; expected `report` or `corpus`"
        ))),
        None => Err(Error::new(
            "usage: neo-internal report <file> | neo-internal corpus [root]",
        )),
    }
}
