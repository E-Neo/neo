//! File-backed module loading.
//!
//! `mod name;` is expanded before resolution. A child module is looked up
//! first as `name.neo` and then as `name/mod.neo` in the declaring module's
//! directory. Loaded files keep their own directory for their children.

use std::path::{Path, PathBuf};

use crate::parser::parse_source;
use crate::surface::{Item, ModDecl, Program};
use neo_common::error::{Error, Result};

/// Read and parse a source file, recursively expanding `mod name;`.
pub fn parse_file(path: impl AsRef<Path>) -> Result<Program> {
    let path = path.as_ref();
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut stack = Vec::new();
    load_file(path, dir, &mut stack)
}

fn load_file(path: &Path, module_dir: &Path, stack: &mut Vec<PathBuf>) -> Result<Program> {
    let identity = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if stack.contains(&identity) {
        let chain = stack
            .iter()
            .chain(std::iter::once(&identity))
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");
        return Err(Error::new(format!("cyclic module files: {}", chain)));
    }
    stack.push(identity);
    let source = std::fs::read_to_string(path)
        .map_err(|e| Error::new(format!("cannot read `{}`: {}", path.display(), e)))?;
    let program = parse_source(&source).map_err(|e| in_file(path, e))?;
    let items = expand_items(program.items, module_dir, stack)?;
    stack.pop();
    Ok(Program { items })
}

fn expand_items(
    items: Vec<Item>,
    module_dir: &Path,
    stack: &mut Vec<PathBuf>,
) -> Result<Vec<Item>> {
    items
        .into_iter()
        .map(|item| match item {
            Item::Mod(decl) => expand_module(decl, module_dir, stack),
            other => Ok(other),
        })
        .collect()
}

fn expand_module(decl: ModDecl, module_dir: &Path, stack: &mut Vec<PathBuf>) -> Result<Item> {
    let ModDecl {
        name,
        is_pub,
        items,
        span,
    } = decl;
    let child_dir = module_dir.join(&name);
    let items = match items {
        Some(items) => expand_items(items, &child_dir, stack)?,
        None => {
            let file = module_file(module_dir, &name, span)?;
            load_file(&file, &child_dir, stack)?.items
        }
    };
    Ok(Item::Mod(ModDecl {
        name,
        is_pub,
        items: Some(items),
        span,
    }))
}

fn module_file(module_dir: &Path, name: &str, span: neo_common::error::Span) -> Result<PathBuf> {
    let flat = module_dir.join(format!("{name}.neo"));
    let nested = module_dir.join(name).join("mod.neo");
    match (flat.is_file(), nested.is_file()) {
        (true, false) => Ok(flat),
        (false, true) => Ok(nested),
        (true, true) => Err(Error::at(
            span,
            format!(
                "module `{}` is ambiguous: both `{}` and `{}` exist",
                name,
                flat.display(),
                nested.display()
            ),
        )),
        (false, false) => Err(Error::at(
            span,
            format!(
                "cannot find module `{}`: expected `{}` or `{}`",
                name,
                flat.display(),
                nested.display()
            ),
        )),
    }
}

fn in_file(path: &Path, mut error: Error) -> Error {
    error.message = format!("{}: {}", path.display(), error.message);
    error
}
