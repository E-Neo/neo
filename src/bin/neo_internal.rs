//! Development-only command line for the self-hosting corpus.
//!
//! This binary is not part of the language interface. It exists so the test
//! suite can record and compare deterministic compiler reports.

use std::path::Path;
use std::process::ExitCode;

use neo::error::{Error, Result};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run_with_stack(args) {
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

#[cfg(not(target_family = "wasm"))]
fn run_with_stack(args: Vec<String>) -> Result<String> {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || run(&args))
        .map_err(|e| Error::new(format!("cannot start worker thread: {e}")))?
        .join()
        .map_err(|_| Error::new("worker thread panicked"))?
}

#[cfg(target_family = "wasm")]
fn run_with_stack(args: Vec<String>) -> Result<String> {
    run(&args)
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
        Some("bench") => {
            let path = args
                .get(1)
                .ok_or_else(|| Error::new("`bench` needs a file"))?;
            let runs = match args.get(2) {
                Some(n) => n
                    .parse::<usize>()
                    .map_err(|_| Error::new("`bench` runs must be a number"))?,
                None => 5,
            };
            bench(path, runs)
        }
        Some(other) => Err(Error::new(format!(
            "unknown command `{other}`; expected `report` or `corpus`"
        ))),
        None => Err(Error::new(
            "usage: neo-internal report <file> | neo-internal corpus [root] | neo-internal bench <file> [runs]",
        )),
    }
}

fn bench(path: &str, runs: usize) -> Result<String> {
    use std::process::Command;
    use std::time::Instant;

    let program = neo::load::parse_file(path)?;
    let mut globals = neo::prelude::new_globals()?;
    neo::elab::elaborate_program(&mut globals, &program)?;
    let wasm = std::env::temp_dir().join(format!("neo-bench-{}.wasm", std::process::id()));
    neo::codegen::build_component(&globals, &wasm)
        .map_err(|e| Error::new(format!("code generation failed: {e}")))?;
    let size = std::fs::metadata(&wasm).map(|m| m.len()).unwrap_or(0);
    let _ = Command::new("wasmtime").arg(&wasm).output();
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        let out = Command::new("wasmtime")
            .arg(&wasm)
            .output()
            .map_err(|e| Error::new(format!("cannot run wasmtime: {e}")))?;
        if out.status.code().is_none() {
            return Err(Error::new("wasmtime failed"));
        }
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let _ = std::fs::remove_file(&wasm);
    let min = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    Ok(format!(
        "{}\truns={}\tmin={:.3}ms\tmean={:.3}ms\tbytes={}\n",
        path, runs, min, mean, size
    ))
}
