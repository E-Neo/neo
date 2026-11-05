use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use neo::elab::{elab_infer, elaborate_program};
use neo::env::Globals;
use neo::error::{Error, Result};
use neo::eval::{eval, force};
use neo::parser::{parse_expr_source, parse_source};
use neo::prelude::new_globals;
use neo::pretty::show_val;
use neo::value::{V, Val};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        None => repl(),
        Some("-h") | Some("--help") => {
            print_help();
            Ok(())
        }
        Some("-V") | Some("--version") => {
            println!("neo {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("build") => build_command(&args[1..]),
        Some("run") | Some("check") => Err(Error::new(format!(
            "`neo {}` is not a subcommand; use `neo <file.neo>` or `neo build <file.neo>`",
            args[0]
        ))),
        Some(path) => interpret_file(path),
    }
}

fn print_help() {
    println!(
        "neo {} - a dependently typed language for WASI 0.2\n\
         \n\
         Usage:\n\
           neo                 start the REPL\n\
           neo <file.neo>      interpret a file\n\
           neo build <file.neo> [-o out.wasm]\n\
                               compile to a WASI 0.2 component\n\
         \n\
         Options:\n\
           -h, --help          show this help\n\
           -V, --version       show the version",
        env!("CARGO_PKG_VERSION")
    );
}

fn interpret_file(path: &str) -> Result<()> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| Error::new(format!("cannot read `{}`: {}", path, e)))?;
    let mut globals = new_globals()?;
    let program = parse_source(&src).map_err(|e| add_file(&src, path, e))?;
    elaborate_program(&mut globals, &program).map_err(|e| add_file(&src, path, e))?;
    run_main(&globals)
}

fn build_command(args: &[String]) -> Result<()> {
    let mut input: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-o" | "--output" => {
                i += 1;
                let out = args.get(i).ok_or_else(|| Error::new("`-o` needs a path"))?;
                output = Some(PathBuf::from(out));
            }
            flag if flag.starts_with('-') => {
                return Err(Error::new(format!("unknown option `{}`", flag)));
            }
            file => {
                if input.is_some() {
                    return Err(Error::new("`neo build` takes one input file"));
                }
                input = Some(file.to_string());
            }
        }
        i += 1;
    }
    let input = input.ok_or_else(|| Error::new("`neo build` needs an input file"))?;
    let output = output.unwrap_or_else(|| {
        let mut p = PathBuf::from(&input);
        p.set_extension("wasm");
        p
    });
    let src = std::fs::read_to_string(&input)
        .map_err(|e| Error::new(format!("cannot read `{}`: {}", input, e)))?;
    let mut globals = new_globals()?;
    let program = parse_source(&src).map_err(|e| add_file(&src, &input, e))?;
    elaborate_program(&mut globals, &program).map_err(|e| add_file(&src, &input, e))?;
    ensure_main(&globals)?;
    neo::codegen::build_component(&globals, &output)
        .map_err(|e| Error::new(format!("code generation failed: {}", e)))?;
    println!("wrote {}", output.display());
    Ok(())
}

fn run_main(globals: &Globals) -> Result<()> {
    let ty = ensure_main(globals)?;
    let value = force(
        globals,
        eval(globals, &Vec::new(), &neo::core::const_("main")),
    );
    let text = print_value(globals, &value).ok_or_else(|| {
        Error::new(format!(
            "`main` has a type that cannot be printed: `{}`",
            show_val(globals, 0, &ty)
        ))
    })?;
    println!("{}", text);
    Ok(())
}

fn ensure_main(globals: &Globals) -> Result<V> {
    let ty = globals
        .get_ty("main")
        .ok_or_else(|| Error::new("this program needs `fn main(): T`"))?;
    let ty_v = force(globals, eval(globals, &Vec::new(), ty));
    if let Val::Pi(..) = &*ty_v {
        return Err(Error::new("`main` must not take parameters"));
    }
    if !is_printable(globals, &ty_v, &mut Vec::new()) {
        return Err(Error::new(format!(
            "the result type of `main` is not printable: `{}`",
            show_val(globals, 0, &ty_v)
        )));
    }
    Ok(ty_v)
}

fn is_printable(globals: &Globals, ty: &V, visiting: &mut Vec<String>) -> bool {
    match &*force(globals, ty.clone()) {
        Val::Ind(name, args) => {
            if matches!(name.as_str(), "Nat" | "Bool" | "Unit") {
                return true;
            }
            if visiting.iter().any(|n| n == name) {
                return true;
            }
            if !args.iter().all(|a| is_printable(globals, a, visiting)) {
                return false;
            }
            let info = match globals.get_enum(name) {
                Some(i) => i,
                None => return false,
            };
            visiting.push(name.clone());
            let ok = info.ctors.iter().all(|c| {
                let mut field_ty = eval(globals, &Vec::new(), &c.ty);
                for arg in args {
                    field_ty = match neo::eval::apply_pi(globals, field_ty, arg.clone()) {
                        Some(v) => v,
                        None => return false,
                    };
                }
                for _ in 0..c.num_fields {
                    let f = force(globals, field_ty);
                    match &*f {
                        Val::Pi(dom, closure) => {
                            if !is_printable(globals, dom, visiting) {
                                return false;
                            }
                            field_ty =
                                neo::eval::eval_closure(globals, closure, neo::value::vvar(0));
                        }
                        _ => return false,
                    }
                }
                true
            });
            visiting.pop();
            ok
        }
        _ => false,
    }
}

/// Render a fully evaluated value using the surface constructor names.
fn print_value(globals: &Globals, v: &V) -> Option<String> {
    let v = force(globals, v.clone());
    match &*v {
        Val::Ind(name, _) if name == "Nat" => Some(show_val(globals, 0, &v)),
        Val::Ctor(enum_name, index, args) => {
            if enum_name == "Nat" {
                return Some(show_val(globals, 0, &v));
            }
            let info = globals.get_enum(enum_name)?;
            let ctor = info.ctors.get(*index)?;
            let fields = &args[ctor.num_params.min(args.len())..];
            let mut out = ctor.name.clone();
            if !fields.is_empty() {
                let mut parts = Vec::new();
                for f in fields {
                    parts.push(print_value(globals, f)?);
                }
                out.push('(');
                out.push_str(&parts.join(", "));
                out.push(')');
            }
            Some(out)
        }
        Val::Type(u) => Some(format!("Type {}", u)),
        Val::Prop => Some("Prop".to_string()),
        _ => None,
    }
}

fn add_file(_src: &str, path: &str, mut e: Error) -> Error {
    e.message = format!("{}: {}", path, e.message);
    e
}

fn repl() -> Result<()> {
    let mut globals = new_globals()?;
    println!("neo {} (type :help for help)", env!("CARGO_PKG_VERSION"));
    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();
    loop {
        print!("neo> ");
        io::stdout().flush().ok();
        let line = match lines.next() {
            Some(Ok(l)) => l,
            Some(Err(e)) => return Err(Error::new(e.to_string())),
            None => break,
        };
        let mut input = line.trim().to_string();
        if input.is_empty() {
            continue;
        }
        if input == ":q" || input == ":quit" || input == ":exit" {
            break;
        }
        if input == ":help" {
            println!(
                "enter declarations (`enum ...`, `fn ...`) or expressions\n\
                 :quit  leave the REPL\n\
                 :help  show this message"
            );
            continue;
        }
        while brace_depth(&input) > 0 {
            print!("...> ");
            io::stdout().flush().ok();
            match lines.next() {
                Some(Ok(l)) => {
                    input.push('\n');
                    input.push_str(&l);
                }
                _ => break,
            }
        }
        if is_declaration(&input) {
            match parse_source(&input).and_then(|p| elaborate_program(&mut globals, &p)) {
                Ok(()) => println!("ok"),
                Err(e) => eprintln!("error: {}", e),
            }
        } else {
            match eval_expression(&globals, &input) {
                Ok(text) => println!("{}", text),
                Err(e) => eprintln!("error: {}", e),
            }
        }
    }
    Ok(())
}

fn brace_depth(s: &str) -> i32 {
    let mut d = 0;
    for c in s.chars() {
        match c {
            '{' => d += 1,
            '}' => d -= 1,
            _ => {}
        }
    }
    d
}

fn is_declaration(s: &str) -> bool {
    let t = s.trim_start();
    if t.starts_with("enum ") {
        return true;
    }
    // `fn name(` is a declaration; `fn(` is a lambda expression.
    if let Some(rest) = t.strip_prefix("fn ") {
        return rest
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
    }
    false
}

fn eval_expression(globals: &Globals, src: &str) -> Result<String> {
    let expr = parse_expr_source(src)?;
    let mut ctx = neo::check::Ctx::new();
    let (tm, ty) = elab_infer(globals, &mut ctx, &expr)?;
    neo::check::check(globals, &mut ctx, &tm, &ty).map_err(|e| {
        Error::at(
            expr.span(),
            format!("kernel rejected this expression: {}", e.message),
        )
    })?;
    let value = force(globals, eval(globals, &ctx.env, &tm));
    print_value(globals, &value).ok_or_else(|| {
        Error::new(format!(
            "cannot display a value of type `{}`",
            show_val(globals, ctx.level(), &ty)
        ))
    })
}
