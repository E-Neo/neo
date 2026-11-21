use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::core::{PrimOp, PrimTy};
use crate::env::{Globals, ItemVisibility};
use crate::surface::*;
use neo_common::error::{Error, Result, Span};

/// A synthetic prefix for a name that has more than one candidate, such as a
/// proposition and a constructor both spelled `True`.
pub const AMBIG_PREFIX: &str = "\u{1}name:";

pub fn ambiguous_name(name: &str) -> String {
    format!("{AMBIG_PREFIX}{name}")
}

pub fn ambiguous_candidates<'a>(globals: &'a Globals, name: &str) -> Option<&'a [String]> {
    let simple = name.strip_prefix(AMBIG_PREFIX)?;
    globals.names.prelude.get(simple).map(Vec::as_slice)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// A user program. Names are resolved against the prelude and against the
    /// program's own root exports.
    Program,
    /// The prelude itself. Unqualified constructor names are resolved by
    /// spelling because the prelude's auto-import table does not exist yet.
    Prelude,
}

/// A resolved program together with the surface metadata to install in
/// `Globals`.
pub struct Resolution {
    pub program: Program,
    modules: Vec<String>,
    visibility: Vec<(String, ItemVisibility)>,
    root: Vec<(String, Vec<String>)>,
    root_modules: Vec<String>,
    prelude: Vec<(String, Vec<String>)>,
}

impl Resolution {
    /// Install module, visibility, and export metadata. Must run before the
    /// elaborator so method dispatch can check privacy.
    pub fn commit(&self, globals: &mut Globals) {
        for module in &self.modules {
            globals.names.modules.insert(module.clone());
        }
        for (key, vis) in &self.visibility {
            globals.names.visibility.insert(key.clone(), vis.clone());
        }
        merge_exports(&mut globals.names.root, &self.root);
        for module in &self.root_modules {
            globals.names.root_modules.insert(module.clone());
        }
        merge_exports(&mut globals.names.prelude, &self.prelude);
    }
}

fn merge_exports(table: &mut HashMap<String, Vec<String>>, exports: &[(String, Vec<String>)]) {
    for (name, candidates) in exports {
        let entry = table.entry(name.clone()).or_default();
        for candidate in candidates {
            if !entry.contains(candidate) {
                entry.push(candidate.clone());
            }
        }
    }
}

#[derive(Clone, Debug)]
enum NameTarget {
    Item(String),
    Module(String),
}

#[derive(Clone, Debug)]
enum DeclRef {
    Enum(EnumDecl),
    Fn(FnDecl),
    Extern(ExternDecl),
    Impl(ImplDecl),
}

struct ModuleInfo {
    path: String,
    parent: Option<usize>,
    items: HashMap<String, NameTarget>,
    /// Visibility of each name in `items`, including public re-exports.
    item_vis: HashMap<String, ItemVisibility>,
    imports: HashMap<String, NameTarget>,
    uses: Vec<UseDecl>,
    decls: Vec<DeclRef>,
}

struct EnumEntry {
    key: String,
    params: usize,
    is_pub: bool,
    module: String,
    ctors: Vec<String>,
    fields: Vec<usize>,
}

struct Tree<'g> {
    globals: &'g Globals,
    modules: Vec<ModuleInfo>,
    module_index: HashMap<String, usize>,
    enums: HashMap<String, EnumEntry>,
    ctor_enum: HashMap<String, String>,
    vis: HashMap<String, ItemVisibility>,
    module_vis: HashMap<String, ItemVisibility>,
    known_modules: HashSet<String>,
    mode: Mode,
}

#[derive(Default, Clone, Debug)]
struct Deps {
    items: BTreeSet<String>,
    methods: BTreeSet<String>,
}

#[derive(Clone, Debug)]
enum UnitKind {
    Enum(EnumDecl),
    Fn(FnDecl),
    Extern(ExternDecl),
}

struct Unit {
    key: String,
    dep_items: BTreeSet<String>,
    dep_methods: BTreeSet<String>,
    is_method: bool,
    kind: UnitKind,
}

/// Resolve a parsed program into a flat, canonically named program whose
/// declarations are ordered for elaboration.
///
/// Resolution is complete before elaboration starts: modules are hoisted,
/// visibility is checked, and every global name becomes a canonical key.
pub fn resolve(globals: &Globals, program: &Program, mode: Mode) -> Result<Resolution> {
    let mut tree = Tree {
        globals,
        modules: Vec::new(),
        module_index: HashMap::new(),
        enums: HashMap::new(),
        ctor_enum: HashMap::new(),
        vis: HashMap::new(),
        module_vis: HashMap::new(),
        known_modules: globals.names.modules.clone(),
        mode,
    };
    tree.modules.push(ModuleInfo {
        path: String::new(),
        parent: None,
        items: HashMap::new(),
        item_vis: HashMap::new(),
        imports: HashMap::new(),
        uses: Vec::new(),
        decls: Vec::new(),
    });
    tree.module_index.insert(String::new(), 0);
    tree.build_items(0, &program.items)?;
    tree.resolve_imports()?;

    let mut units = Vec::new();
    for index in 0..tree.modules.len() {
        tree.build_units(index, &mut units)?;
    }
    let ordered = tree.order_units(units);

    let mut items = Vec::new();
    for unit in ordered {
        items.push(match unit.kind {
            UnitKind::Enum(e) => Item::Enum(e),
            UnitKind::Fn(f) => Item::Fn(f),
            UnitKind::Extern(e) => Item::Extern(e),
        });
    }

    let (root, root_modules, prelude) = match mode {
        Mode::Program => {
            let (items, modules) = tree.root_exports();
            (items, modules, Vec::new())
        }
        Mode::Prelude => (Vec::new(), Vec::new(), tree.prelude_exports()),
    };

    let mut modules: Vec<String> = tree
        .modules
        .iter()
        .map(|module| module.path.clone())
        .collect();
    modules.retain(|module| !module.is_empty());
    modules.sort();
    modules.dedup();

    let mut visibility: Vec<(String, ItemVisibility)> = tree
        .vis
        .iter()
        .map(|(key, vis)| (key.clone(), vis.clone()))
        .collect();
    visibility.sort_by(|a, b| a.0.cmp(&b.0));

    Ok(Resolution {
        program: Program { items },
        modules,
        visibility,
        root,
        root_modules,
        prelude,
    })
}

/// Resolve a single expression in the interactive environment.
pub fn resolve_expression(globals: &Globals, expr: &Expr) -> Result<Expr> {
    let tree = Tree {
        globals,
        modules: Vec::new(),
        module_index: HashMap::new(),
        enums: HashMap::new(),
        ctor_enum: HashMap::new(),
        vis: HashMap::new(),
        module_vis: HashMap::new(),
        known_modules: globals.names.modules.clone(),
        mode: Mode::Program,
    };
    let mut expr = expr.clone();
    tree.resolve_expr("", &mut HashSet::new(), &mut expr, &mut Deps::default())?;
    Ok(expr)
}

impl Tree<'_> {
    fn build_items(&mut self, module: usize, items: &[Item]) -> Result<()> {
        for item in items {
            match item {
                Item::Mod(decl) => {
                    let parent_path = self.modules[module].path.clone();
                    let path = join_path(&parent_path, &decl.name);
                    if PrimTy::from_name(&decl.name).is_some()
                        || PrimOp::from_name(&decl.name).is_some()
                        || decl.name == "IO"
                    {
                        return Err(Error::at(
                            decl.span,
                            format!("`{}` is a built-in name and cannot be a module", decl.name),
                        ));
                    }
                    if self.modules[module].items.contains_key(&decl.name) {
                        return Err(Error::at(
                            decl.span,
                            format!("`{}` is already defined in this module", decl.name),
                        ));
                    }
                    if self.module_index.contains_key(&path) {
                        return Err(Error::at(
                            decl.span,
                            format!("module `{}` is defined twice", path),
                        ));
                    }
                    if self.known_modules.contains(&path) {
                        return Err(Error::at(
                            decl.span,
                            format!("module `{}` is already defined", path),
                        ));
                    }
                    let index = self.modules.len();
                    self.modules.push(ModuleInfo {
                        path: path.clone(),
                        parent: Some(module),
                        items: HashMap::new(),
                        item_vis: HashMap::new(),
                        imports: HashMap::new(),
                        uses: Vec::new(),
                        decls: Vec::new(),
                    });
                    self.module_index.insert(path.clone(), index);
                    self.record_item(
                        module,
                        &decl.name,
                        NameTarget::Module(path.clone()),
                        decl.is_pub,
                    );
                    self.module_vis.insert(
                        path,
                        ItemVisibility {
                            module: parent_path,
                            public: decl.is_pub,
                        },
                    );
                    let Some(items) = &decl.items else {
                        return Err(Error::at(
                            decl.span,
                            format!(
                                "file module `{}` was not loaded; use `load::parse_file`",
                                decl.name
                            ),
                        ));
                    };
                    self.build_items(index, items)?;
                }
                Item::Use(decl) => {
                    self.modules[module].uses.push(decl.clone());
                }
                Item::Enum(decl) => {
                    let simple = decl.name.clone();
                    let key = join_path(&self.modules[module].path, &simple);
                    if self.modules[module].items.contains_key(&simple) {
                        return Err(Error::at(
                            decl.span,
                            format!("`{}` is already defined in this module", simple),
                        ));
                    }
                    if self.known_modules.contains(&key) {
                        return Err(Error::at(
                            decl.span,
                            format!("`{}` is already defined as a module", key),
                        ));
                    }
                    let mut decl = decl.clone();
                    decl.name = key.clone();
                    let mut seen = HashSet::new();
                    for ctor in &decl.ctors {
                        if !seen.insert(ctor.name.clone()) {
                            return Err(Error::at(
                                ctor.span,
                                format!("constructor `{}` is defined twice", ctor.name),
                            ));
                        }
                        let ctor_key = format!("{key}::{}", ctor.name);
                        self.ctor_enum.insert(ctor_key.clone(), key.clone());
                        self.vis.insert(
                            ctor_key,
                            ItemVisibility {
                                module: self.modules[module].path.clone(),
                                public: decl.is_pub,
                            },
                        );
                    }
                    let ctors: Vec<String> = decl.ctors.iter().map(|c| c.name.clone()).collect();
                    let fields: Vec<usize> = decl.ctors.iter().map(|c| c.fields.len()).collect();
                    self.enums.insert(
                        key.clone(),
                        EnumEntry {
                            key: key.clone(),
                            params: decl.params.len(),
                            is_pub: decl.is_pub,
                            module: self.modules[module].path.clone(),
                            ctors,
                            fields,
                        },
                    );
                    self.vis.insert(
                        key.clone(),
                        ItemVisibility {
                            module: self.modules[module].path.clone(),
                            public: decl.is_pub,
                        },
                    );
                    self.record_item(module, &simple, NameTarget::Item(key), decl.is_pub);
                    self.modules[module].decls.push(DeclRef::Enum(decl));
                }
                Item::Fn(decl) => {
                    if self.modules[module].items.contains_key(&decl.name) {
                        return Err(Error::at(
                            decl.span,
                            format!("`{}` is already defined in this module", decl.name),
                        ));
                    }
                    let local_key = join_path(&self.modules[module].path, &decl.name);
                    if self.known_modules.contains(&local_key) {
                        return Err(Error::at(
                            decl.span,
                            format!("`{}` is already defined as a module", local_key),
                        ));
                    }
                    let mut decl = decl.clone();
                    decl.module = self.modules[module].path.clone();
                    let key = fn_key(&decl);
                    self.vis.insert(
                        key.clone(),
                        ItemVisibility {
                            module: self.modules[module].path.clone(),
                            public: decl.is_pub,
                        },
                    );
                    self.record_item(module, &decl.name, NameTarget::Item(key), decl.is_pub);
                    self.modules[module].decls.push(DeclRef::Fn(decl));
                }
                Item::Extern(decl) => {
                    if self.modules[module].items.contains_key(&decl.name) {
                        return Err(Error::at(
                            decl.span,
                            format!("`{}` is already defined in this module", decl.name),
                        ));
                    }
                    let local_key = join_path(&self.modules[module].path, &decl.name);
                    if self.known_modules.contains(&local_key) {
                        return Err(Error::at(
                            decl.span,
                            format!("`{}` is already defined as a module", local_key),
                        ));
                    }
                    let mut decl = decl.clone();
                    decl.module = self.modules[module].path.clone();
                    let key = decl.full_name();
                    self.vis.insert(
                        key.clone(),
                        ItemVisibility {
                            module: self.modules[module].path.clone(),
                            public: decl.is_pub,
                        },
                    );
                    self.record_item(module, &decl.name, NameTarget::Item(key), decl.is_pub);
                    self.modules[module].decls.push(DeclRef::Extern(decl));
                }
                Item::Impl(decl) => {
                    self.modules[module].decls.push(DeclRef::Impl(decl.clone()));
                }
            }
        }
        Ok(())
    }

    fn record_item(&mut self, module: usize, name: &str, target: NameTarget, public: bool) {
        let path = self.modules[module].path.clone();
        self.modules[module].items.insert(name.to_string(), target);
        self.modules[module].item_vis.insert(
            name.to_string(),
            ItemVisibility {
                module: path,
                public,
            },
        );
    }

    fn resolve_imports(&mut self) -> Result<()> {
        // Re-exports make imports order-dependent: `use b::Circle` needs
        // `b`'s own `pub use` first. Retry failed imports until a pass makes
        // no progress, then report the first remaining error.
        let mut pending: Vec<(usize, UseDecl, Option<Error>)> = Vec::new();
        for index in 0..self.modules.len() {
            let uses = std::mem::take(&mut self.modules[index].uses);
            for use_decl in uses {
                pending.push((index, use_decl, None));
            }
        }
        while !pending.is_empty() {
            let mut next = Vec::new();
            let mut progress = false;
            for (index, use_decl, _) in pending {
                match self.resolve_use_tree(index, &[], &use_decl.tree, use_decl.is_pub) {
                    Ok(()) => progress = true,
                    Err(error) => next.push((index, use_decl, Some(error))),
                }
            }
            if next.is_empty() {
                return Ok(());
            }
            if !progress {
                let (_, _, error) = next.remove(0);
                return Err(error.expect("failed import carries an error"));
            }
            pending = next;
        }
        Ok(())
    }

    fn resolve_use_tree(
        &mut self,
        module: usize,
        prefix: &[String],
        tree: &UseTree,
        public: bool,
    ) -> Result<()> {
        match tree {
            UseTree::Name {
                segments,
                alias,
                span,
            } => {
                let mut full = prefix.to_vec();
                full.extend_from_slice(segments);
                let from = self.modules[module].path.clone();
                let target = self.resolve_path_ref(&from, &full)?;
                if public {
                    self.check_public_path(&from, &full, *span)?;
                }
                let name = alias
                    .clone()
                    .unwrap_or_else(|| full.last().cloned().unwrap_or_default());
                self.add_binding(module, &name, target, *span, public)
            }
            UseTree::Glob { segments, span } => {
                let mut full = prefix.to_vec();
                full.extend_from_slice(segments);
                let from = self.modules[module].path.clone();
                let target = self.resolve_path_ref(&from, &full)?;
                if public {
                    self.check_public_path(&from, &full, *span)?;
                }
                match target {
                    NameTarget::Module(path) => {
                        let mut bindings = Vec::new();
                        self.glob_module(&path, &from, &mut bindings)?;
                        for (name, target) in bindings {
                            self.add_binding(module, &name, target, *span, public)?;
                        }
                        Ok(())
                    }
                    NameTarget::Item(key) => {
                        let Some(entry) = self.enum_entry(&key) else {
                            return Err(Error::at(
                                *span,
                                format!(
                                    "`{}` is not an enum, so `{}::*` has no constructors",
                                    key, key
                                ),
                            ));
                        };
                        for ctor in entry.ctors {
                            let ctor_key = format!("{}::{}", key, ctor);
                            self.add_binding(
                                module,
                                &ctor,
                                NameTarget::Item(ctor_key),
                                *span,
                                public,
                            )?;
                        }
                        Ok(())
                    }
                }
            }
            UseTree::Group {
                prefix: group,
                items,
                ..
            } => {
                let mut next = prefix.to_vec();
                next.extend_from_slice(group);
                for item in items {
                    self.resolve_use_tree(module, &next, item, public)?;
                }
                Ok(())
            }
        }
    }

    fn glob_module(
        &self,
        path: &str,
        from: &str,
        out: &mut Vec<(String, NameTarget)>,
    ) -> Result<()> {
        if let Some(&index) = self.module_index.get(path) {
            let mut names: Vec<String> = self.modules[index].items.keys().cloned().collect();
            names.sort();
            for name in names {
                let visible = self.modules[index]
                    .item_vis
                    .get(&name)
                    .is_some_and(|vis| vis.visible_from(from));
                if visible {
                    let target = self.modules[index].items.get(&name).unwrap().clone();
                    out.push((name, target));
                }
            }
            return Ok(());
        }
        // A prelude module is known only through `globals`.
        let prefix = format!("{path}::");
        let mut names: Vec<String> = Vec::new();
        for key in self.globals.enums.keys() {
            if let Some(rest) = key.strip_prefix(&prefix)
                && !rest.contains("::")
                && self.visible(key, from)
            {
                names.push(rest.to_string());
            }
        }
        for key in self.globals.defs.keys() {
            if let Some(rest) = key.strip_prefix(&prefix)
                && !rest.contains("::")
                && !rest.contains('.')
                && self.visible(key, from)
            {
                names.push(rest.to_string());
            }
        }
        for key in self.globals.externs.keys() {
            if let Some(rest) = key.strip_prefix(&prefix)
                && !rest.contains("::")
                && self.visible(key, from)
            {
                names.push(rest.to_string());
            }
        }
        names.sort();
        names.dedup();
        for name in names {
            let key = format!("{path}::{name}");
            out.push((name, NameTarget::Item(key)));
        }
        Ok(())
    }

    fn add_binding(
        &mut self,
        module: usize,
        name: &str,
        target: NameTarget,
        span: Span,
        public: bool,
    ) -> Result<()> {
        if public && !self.public_target(&target) {
            return Err(Error::at(
                span,
                format!("`{}` is private and cannot be re-exported", name),
            ));
        }
        let existing_item = self.modules[module].items.get(name).cloned();
        let existing_import = self.modules[module].imports.get(name).cloned();
        if let Some(existing) = &existing_item {
            if !same_target(existing, &target) {
                return Err(Error::at(
                    span,
                    format!("`{}` is already defined in this module", name),
                ));
            }
            if public {
                let path = self.modules[module].path.clone();
                self.modules[module].imports.remove(name);
                self.modules[module].item_vis.insert(
                    name.to_string(),
                    ItemVisibility {
                        module: path,
                        public: true,
                    },
                );
            }
            return Ok(());
        }
        if let Some(existing) = &existing_import
            && !same_target(existing, &target)
        {
            return Err(Error::at(
                span,
                format!("`{}` is imported twice with different meanings", name),
            ));
        }
        if matches!(target, NameTarget::Item(ref key) if key == name)
            && !public
            && existing_import.is_none()
            && self.modules[module].items.contains_key(name)
        {
            return Ok(());
        }
        if public {
            self.modules[module].imports.remove(name);
            self.record_item(module, name, target, true);
        } else {
            self.modules[module]
                .imports
                .insert(name.to_string(), target);
        }
        Ok(())
    }

    fn check_public_path(&self, from: &str, segments: &[String], span: Span) -> Result<()> {
        for k in 1..segments.len() {
            match self.resolve_path_ref(from, &segments[..k]) {
                Ok(NameTarget::Module(path)) => {
                    if self.module_vis.get(&path).is_some_and(|vis| !vis.public) {
                        return Err(Error::at(
                            span,
                            format!("module `{}` is private and cannot be re-exported", path),
                        ));
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    fn public_target(&self, target: &NameTarget) -> bool {
        match target {
            NameTarget::Item(key) => self
                .vis
                .get(key)
                .or_else(|| self.globals.names.visibility.get(key))
                .is_some_and(|vis| vis.public),
            NameTarget::Module(path) => self.module_vis.get(path).is_none_or(|vis| vis.public),
        }
    }

    fn build_units(&mut self, module: usize, units: &mut Vec<Unit>) -> Result<()> {
        let decls = std::mem::take(&mut self.modules[module].decls);
        let module_path = self.modules[module].path.clone();
        for decl in decls {
            match decl {
                DeclRef::Enum(mut decl) => {
                    let mut deps = Deps::default();
                    self.resolve_enum_decl(&module_path, &mut decl, &mut deps)?;
                    let key = decl.name.clone();
                    units.push(Unit {
                        key,
                        dep_items: deps.items,
                        dep_methods: deps.methods,
                        is_method: false,
                        kind: UnitKind::Enum(decl),
                    });
                }
                DeclRef::Fn(mut decl) => {
                    let mut deps = Deps::default();
                    self.resolve_fn_decl(&module_path, &mut decl, &mut deps, None)?;
                    let key = fn_key(&decl);
                    units.push(Unit {
                        key,
                        dep_items: deps.items,
                        dep_methods: deps.methods,
                        is_method: false,
                        kind: UnitKind::Fn(decl),
                    });
                }
                DeclRef::Extern(mut decl) => {
                    self.resolve_extern_decl(&module_path, &mut decl)?;
                    let key = decl.full_name();
                    units.push(Unit {
                        key,
                        dep_items: BTreeSet::new(),
                        dep_methods: BTreeSet::new(),
                        is_method: false,
                        kind: UnitKind::Extern(decl),
                    });
                }
                DeclRef::Impl(decl) => {
                    self.resolve_impl(&module_path, &decl, units)?;
                }
            }
        }
        Ok(())
    }

    fn resolve_enum_decl(&self, module: &str, decl: &mut EnumDecl, deps: &mut Deps) -> Result<()> {
        let mut locals = HashSet::new();
        for param in &mut decl.params {
            self.resolve_expr(module, &mut locals, &mut param.ty, deps)?;
            locals.insert(param.name.clone());
        }
        if let Some(sort) = &mut decl.sort {
            self.resolve_expr(module, &mut locals, sort, deps)?;
        }
        for (j, ctor) in decl.ctors.iter_mut().enumerate() {
            let mut field_locals = locals.clone();
            for field in &mut ctor.fields {
                self.resolve_expr(module, &mut field_locals, &mut field.ty, deps)?;
                let name = field
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("_f{j}_{}", field.span.line));
                field_locals.insert(name);
            }
            if let Some(result) = &mut ctor.result {
                self.resolve_expr(module, &mut field_locals, result, deps)?;
            }
        }
        Ok(())
    }

    fn resolve_fn_decl(
        &self,
        module: &str,
        decl: &mut FnDecl,
        deps: &mut Deps,
        impl_locals: Option<&HashSet<String>>,
    ) -> Result<()> {
        let mut locals = impl_locals.cloned().unwrap_or_default();
        for param in &mut decl.params {
            if !locals.insert(param.name.clone()) {
                return Err(Error::at(
                    param.span,
                    format!("parameter `{}` is bound twice", param.name),
                ));
            }
            self.resolve_expr(module, &mut locals, &mut param.ty, deps)?;
        }
        self.resolve_expr(module, &mut locals, &mut decl.ret, deps)?;
        let mut body = decl.body.clone();
        self.resolve_block(module, &mut locals, &mut body, deps)?;
        decl.body = body;
        Ok(())
    }

    fn resolve_extern_decl(&self, module: &str, decl: &mut ExternDecl) -> Result<()> {
        let mut locals = HashSet::new();
        let mut deps = Deps::default();
        for param in &mut decl.params {
            self.resolve_expr(module, &mut locals, &mut param.ty, &mut deps)?;
            locals.insert(param.name.clone());
        }
        self.resolve_expr(module, &mut locals, &mut decl.ret, &mut deps)?;
        Ok(())
    }

    fn resolve_impl(&mut self, module: &str, decl: &ImplDecl, units: &mut Vec<Unit>) -> Result<()> {
        let mut locals: HashSet<String> = HashSet::new();
        let mut impl_params = Vec::with_capacity(decl.params.len());
        let mut target_deps = Deps::default();
        for param in &decl.params {
            let mut param = param.clone();
            self.resolve_expr(module, &mut locals, &mut param.ty, &mut target_deps)?;
            locals.insert(param.name.clone());
            impl_params.push(param);
        }
        let mut target = decl.target.clone();
        let target_resolution =
            self.resolve_expr(module, &mut locals, &mut target, &mut target_deps);
        if target_resolution.is_err() {
            // Keep the original spelling so an unknown target gets the
            // `impl` diagnostic instead of a generic unknown-name error.
            target = decl.target.clone();
        }

        let (type_key, impl_args) = impl_target(&target).ok_or_else(|| {
            Error::at(
                decl.span,
                "an impl target must be a type name, such as `impl Shape` or `impl[T] List[T]`",
            )
        })?;
        if let Some(prim) = PrimTy::from_name(&type_key) {
            let _ = prim;
            if !decl.params.is_empty() || !impl_args.is_empty() {
                return Err(Error::at(
                    decl.span,
                    format!("primitive type `{}` takes no impl parameters", type_key),
                ));
            }
        } else {
            let (params, visible) = if let Some(entry) = self.enums.get(&type_key) {
                (entry.params, self.visible(&type_key, module))
            } else if let Some(info) = self.globals.get_enum(&type_key) {
                (info.param_names.len(), self.visible(&type_key, module))
            } else {
                return Err(Error::at(
                    decl.span,
                    format!("`{}` is not an enum or primitive type", type_key),
                ));
            };
            if !visible {
                return Err(Error::at(
                    decl.span,
                    format!("type `{}` is private here", type_key),
                ));
            }
            if params != decl.params.len() {
                return Err(Error::at(
                    decl.span,
                    format!(
                        "`{}` has {} type parameter(s), but the impl declares {}",
                        type_key,
                        params,
                        decl.params.len()
                    ),
                ));
            }
            if impl_args.len() != decl.params.len() {
                return Err(Error::at(
                    decl.span,
                    format!(
                        "the impl target needs {} argument(s) in `[]`, got {}",
                        decl.params.len(),
                        impl_args.len()
                    ),
                ));
            }
            for (arg, param) in impl_args.iter().zip(decl.params.iter()) {
                match arg {
                    Expr::Var(name, _) if name == &param.name => {}
                    _ => {
                        return Err(Error::at(
                            decl.span,
                            "the impl target must apply the impl's own type parameters in order",
                        ));
                    }
                }
            }
        }

        let mut seen = HashSet::new();
        for method in &decl.methods {
            if method.namespace.is_some() {
                return Err(Error::at(
                    method.span,
                    "methods are already inside an impl block",
                ));
            }
            if !seen.insert(method.name.clone()) {
                return Err(Error::at(
                    method.span,
                    format!(
                        "method `{}` is defined twice in one impl block",
                        method.name
                    ),
                ));
            }
            let key = format!("{}.{}", type_key, method.name);
            if self.vis.contains_key(&key) || self.globals.names.visibility.contains_key(&key) {
                return Err(Error::at(
                    method.span,
                    format!(
                        "method `{}` is already defined for `{}`",
                        method.name, type_key
                    ),
                ));
            }
            self.vis.insert(
                key.clone(),
                ItemVisibility {
                    module: module.to_string(),
                    public: method.is_pub,
                },
            );

            let mut method = method.clone();
            method.namespace = Some(type_key.clone());
            method.module = module.to_string();
            let mut deps = Deps::default();
            deps.items.insert(type_key.clone());
            self.resolve_fn_decl(module, &mut method, &mut deps, Some(&locals))?;

            let mut params = impl_params.clone();
            for param in &mut params {
                param.implicit = true;
            }
            params.extend(method.params);
            method.params = params;

            units.push(Unit {
                key,
                dep_items: deps.items,
                dep_methods: deps.methods,
                is_method: true,
                kind: UnitKind::Fn(method),
            });
        }
        Ok(())
    }

    fn order_units(&self, units: Vec<Unit>) -> Vec<Unit> {
        let mut method_keys: HashMap<String, Vec<String>> = HashMap::new();
        for unit in &units {
            if unit.is_method
                && let UnitKind::Fn(f) = &unit.kind
            {
                method_keys
                    .entry(f.name.clone())
                    .or_default()
                    .push(unit.key.clone());
            }
        }
        let index: HashMap<String, usize> = units
            .iter()
            .enumerate()
            .map(|(i, u)| (u.key.clone(), i))
            .collect();
        let mut deps: Vec<BTreeSet<String>> = Vec::with_capacity(units.len());
        for unit in &units {
            let mut keys = unit.dep_items.clone();
            for method in &unit.dep_methods {
                if let Some(keys_for_name) = method_keys.get(method) {
                    for key in keys_for_name {
                        if key != &unit.key {
                            keys.insert(key.clone());
                        }
                    }
                }
            }
            keys.remove(&unit.key);
            deps.push(keys);
        }
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); units.len()];
        let mut indegree = vec![0usize; units.len()];
        for (i, keys) in deps.iter().enumerate() {
            for key in keys {
                if let Some(&dep) = index.get(key)
                    && dep != i
                {
                    dependents[dep].push(i);
                    indegree[i] += 1;
                }
            }
        }
        let mut done = vec![false; units.len()];
        let mut order = Vec::with_capacity(units.len());
        while order.len() < units.len() {
            let next = (0..units.len()).find(|&i| !done[i] && indegree[i] == 0);
            let next = match next {
                Some(i) => i,
                None => {
                    // A dependency cycle: keep source order. Self-recursion is
                    // fine, while genuine mutual recursion is rejected during
                    // elaboration because the other definition is still opaque.
                    (0..units.len())
                        .find(|&i| !done[i])
                        .expect("remaining unit")
                }
            };
            done[next] = true;
            order.push(next);
            for &dependent in &dependents[next] {
                indegree[dependent] = indegree[dependent].saturating_sub(1);
            }
        }
        let mut units: Vec<Option<Unit>> = units.into_iter().map(Some).collect();
        order
            .into_iter()
            .map(|i| units[i].take().expect("unit emitted once"))
            .collect()
    }

    fn root_exports(&self) -> (Vec<(String, Vec<String>)>, Vec<String>) {
        let mut table: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut modules = Vec::new();
        let names = self.modules[0]
            .items
            .iter()
            .chain(self.modules[0].imports.iter());
        for (name, target) in names {
            match target {
                NameTarget::Item(key) => {
                    table.entry(name.clone()).or_default().insert(key.clone());
                    if let Some(entry) = self.enums.get(key) {
                        for ctor in &entry.ctors {
                            table
                                .entry(ctor.clone())
                                .or_default()
                                .insert(format!("{}::{}", key, ctor));
                        }
                    }
                }
                NameTarget::Module(path) => modules.push(path.clone()),
            }
        }
        modules.sort();
        (
            table
                .into_iter()
                .map(|(name, candidates)| (name, candidates.into_iter().collect()))
                .collect(),
            modules,
        )
    }

    fn prelude_exports(&self) -> Vec<(String, Vec<String>)> {
        let mut table: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (name, target) in &self.modules[0].items {
            let NameTarget::Item(key) = target else {
                continue;
            };
            let Some(entry) = self.enums.get(key) else {
                continue;
            };
            if !entry.is_pub {
                continue;
            }
            table.entry(name.clone()).or_default().insert(key.clone());
            for ctor in &entry.ctors {
                table
                    .entry(ctor.clone())
                    .or_default()
                    .insert(format!("{}::{}", key, ctor));
            }
        }
        table
            .into_iter()
            .map(|(name, candidates)| (name, candidates.into_iter().collect()))
            .collect()
    }
}

fn same_target(a: &NameTarget, b: &NameTarget) -> bool {
    match (a, b) {
        (NameTarget::Item(a), NameTarget::Item(b)) => a == b,
        (NameTarget::Module(a), NameTarget::Module(b)) => a == b,
        _ => false,
    }
}

fn join_path(module: &str, name: &str) -> String {
    if module.is_empty() {
        name.to_string()
    } else {
        format!("{module}::{name}")
    }
}

fn fn_key(decl: &FnDecl) -> String {
    match &decl.namespace {
        Some(ns) => format!("{}.{}", ns, decl.name),
        None if decl.module.is_empty() => decl.name.clone(),
        None => format!("{}::{}", decl.module, decl.name),
    }
}

fn impl_target(target: &Expr) -> Option<(String, Vec<Expr>)> {
    match target {
        Expr::Var(name, _) => Some((name.clone(), Vec::new())),
        Expr::Path(segments, _) => Some((segments.join("::"), Vec::new())),
        Expr::App {
            f, implicit, args, ..
        } if args.is_empty() => {
            let (head, mut previous) = impl_target(f)?;
            if !previous.is_empty() {
                return None;
            }
            previous.extend(implicit.iter().cloned());
            Some((head, previous))
        }
        Expr::Paren(inner, _) => impl_target(inner),
        _ => None,
    }
}

// Resolution of names and paths.
impl Tree<'_> {
    fn resolve_block(
        &self,
        module: &str,
        locals: &mut HashSet<String>,
        block: &mut Block,
        deps: &mut Deps,
    ) -> Result<()> {
        let base = locals.len();
        let mut pushed = Vec::new();
        for binding in &mut block.bindings {
            if let Some(ty) = &mut binding.ty {
                self.resolve_expr(module, locals, ty, deps)?;
            }
            self.resolve_expr(module, locals, &mut binding.value, deps)?;
            if locals.insert(binding.name.clone()) {
                pushed.push(binding.name.clone());
            }
        }
        self.resolve_expr(module, locals, &mut block.tail, deps)?;
        for name in pushed.into_iter().rev() {
            locals.remove(&name);
        }
        debug_assert_eq!(locals.len(), base);
        Ok(())
    }

    fn resolve_expr(
        &self,
        module: &str,
        locals: &mut HashSet<String>,
        expr: &mut Expr,
        deps: &mut Deps,
    ) -> Result<()> {
        match expr {
            Expr::Var(name, span) => {
                if locals.contains(name) {
                    return Ok(());
                }
                match self.resolve_name(module, name) {
                    Ok(ResolvedName::Local) => Ok(()),
                    Ok(ResolvedName::Item(key)) => {
                        self.record_dep(&key, deps);
                        *name = key;
                        Ok(())
                    }
                    Ok(ResolvedName::Ambiguous(candidates)) => {
                        *name = ambiguous_name(name);
                        for candidate in candidates {
                            self.record_dep(&candidate, deps);
                        }
                        Ok(())
                    }
                    Err(err) => Err(Error::at(*span, err.message)),
                }
            }
            Expr::Path(segments, span) => match self.resolve_path_ref(module, segments) {
                Ok(NameTarget::Item(key)) => {
                    self.record_dep(&key, deps);
                    *expr = Expr::Var(key, *span);
                    Ok(())
                }
                Ok(NameTarget::Module(path)) => Err(Error::at(
                    *span,
                    format!("`{}` is a module, not a value", path),
                )),
                Err(err) => Err(Error::at(*span, err.message)),
            },
            Expr::Numeral(..)
            | Expr::Str(..)
            | Expr::Bytes(..)
            | Expr::Type(..)
            | Expr::Prop(..) => Ok(()),
            Expr::Refl(_) => Ok(()),
            Expr::FnType(binders, body, _) | Expr::FnLam(binders, body, _) => {
                let mut added = Vec::new();
                for binder in binders.iter_mut() {
                    self.resolve_expr(module, locals, &mut binder.ty, deps)?;
                    if let Some(name) = &binder.name {
                        if !locals.insert(name.clone()) {
                            return Err(Error::at(
                                binder.span,
                                format!("`{}` is already bound", name),
                            ));
                        }
                        added.push(name.clone());
                    }
                }
                self.resolve_expr(module, locals, body, deps)?;
                for name in added.into_iter().rev() {
                    locals.remove(&name);
                }
                Ok(())
            }
            Expr::App {
                f, implicit, args, ..
            } => {
                self.resolve_expr(module, locals, f, deps)?;
                for arg in implicit.iter_mut().chain(args.iter_mut()) {
                    self.resolve_expr(module, locals, arg, deps)?;
                }
                Ok(())
            }
            Expr::Method {
                receiver,
                name,
                site,
                implicit,
                args,
                ..
            } => {
                self.resolve_expr(module, locals, receiver, deps)?;
                for arg in implicit.iter_mut().chain(args.iter_mut()) {
                    self.resolve_expr(module, locals, arg, deps)?;
                }
                deps.methods.insert(name.clone());
                *site = module.to_string();
                Ok(())
            }
            Expr::BinOp { lhs, rhs, .. } => {
                self.resolve_expr(module, locals, lhs, deps)?;
                self.resolve_expr(module, locals, rhs, deps)
            }
            Expr::Match(scrut, arms, _) => {
                self.resolve_expr(module, locals, scrut, deps)?;
                for arm in arms.iter_mut() {
                    let mut binders = HashSet::new();
                    self.resolve_pattern(module, &mut arm.pattern, &mut binders, deps)?;
                    let mut extended = locals.clone();
                    extended.extend(binders);
                    self.resolve_expr(module, &mut extended, &mut arm.body, deps)?;
                }
                Ok(())
            }
            Expr::Eq(lhs, rhs, _) => {
                self.resolve_expr(module, locals, lhs, deps)?;
                self.resolve_expr(module, locals, rhs, deps)
            }
            Expr::Ascribe(expr, ty, _) => {
                self.resolve_expr(module, locals, expr, deps)?;
                self.resolve_expr(module, locals, ty, deps)
            }
            Expr::Block(block) => self.resolve_block(module, locals, block, deps),
            Expr::Paren(inner, _) => self.resolve_expr(module, locals, inner, deps),
        }
    }

    fn resolve_pattern(
        &self,
        module: &str,
        pattern: &mut Pattern,
        binders: &mut HashSet<String>,
        deps: &mut Deps,
    ) -> Result<()> {
        match pattern {
            Pattern::Wild(_) => Ok(()),
            Pattern::Var(name, span) => {
                if let Some(ctor) = self.nullary_ctor(module, name)? {
                    self.record_dep(&ctor, deps);
                    *pattern = Pattern::Ctor(ctor, Vec::new(), *span);
                    Ok(())
                } else {
                    binders.insert(name.clone());
                    Ok(())
                }
            }
            Pattern::Ctor(name, subpats, span) => {
                let segments: Vec<String> = name.split("::").map(str::to_string).collect();
                let key = self.resolve_ctor_path(module, &segments, *span)?;
                self.record_dep(&key, deps);
                *name = key;
                for subpat in subpats.iter_mut() {
                    self.resolve_pattern(module, subpat, binders, deps)?;
                }
                Ok(())
            }
        }
    }

    fn nullary_ctor(&self, module: &str, name: &str) -> Result<Option<String>> {
        let candidates = self.lookup(module, name);
        let mut ctors = Vec::new();
        for candidate in candidates {
            if let NameTarget::Item(key) = candidate
                && self.is_ctor(&key)
                && self.ctor_fields(&key) == Some(0)
            {
                ctors.push(key);
            }
        }
        ctors.sort();
        ctors.dedup();
        match ctors.len() {
            0 => Ok(None),
            1 => Ok(Some(ctors.remove(0))),
            _ => Err(Error::new(format!(
                "`{}` names more than one constructor; qualify it with its enum",
                name
            ))),
        }
    }

    fn resolve_ctor_path(&self, module: &str, segments: &[String], span: Span) -> Result<String> {
        let target = self
            .resolve_path_ref(module, segments)
            .map_err(|err| Error::at(span, err.message))?;
        match target {
            NameTarget::Item(key) if self.is_ctor(&key) => Ok(key),
            NameTarget::Item(key) => {
                Err(Error::at(span, format!("`{}` is not a constructor", key)))
            }
            NameTarget::Module(path) => Err(Error::at(
                span,
                format!("`{}` is a module, not a constructor", path),
            )),
        }
    }

    fn resolve_name(&self, module: &str, name: &str) -> Result<ResolvedName> {
        let candidates = self.lookup(module, name);
        if candidates.is_empty() {
            if builtin_name(name) {
                return Ok(ResolvedName::Local);
            }
            if let Some(op) = PrimOp::from_name(name) {
                if self.mode == Mode::Prelude || op.is_internal() {
                    return Ok(ResolvedName::Local);
                }
                return Err(Error::new(format!(
                    "`{}` is a low-level primitive; use the corresponding method",
                    name
                )));
            }
            return Err(Error::new(format!("unknown name `{}`", name)));
        }
        let mut items = Vec::new();
        for candidate in candidates {
            match candidate {
                NameTarget::Item(key) => items.push(key),
                NameTarget::Module(path) => {
                    return Err(Error::new(format!("`{}` is a module, not a value", path)));
                }
            }
        }
        items.sort();
        items.dedup();
        if items.len() == 1 {
            Ok(ResolvedName::Item(items.remove(0)))
        } else {
            Ok(ResolvedName::Ambiguous(items))
        }
    }

    fn resolve_path_ref(&self, module: &str, segments: &[String]) -> Result<NameTarget> {
        let Some(first) = segments.first() else {
            return Err(Error::new("empty path"));
        };
        if first == "self" || first == "super" {
            let base = if first == "self" {
                module.to_string()
            } else {
                self.parent_module(module)?
            };
            return self.walk_module(&base, &segments[1..], module);
        }
        if segments.len() == 2 && first == "IO" && (segments[1] == "pure" || segments[1] == "bind")
        {
            return Ok(NameTarget::Item(format!("IO::{}", segments[1])));
        }
        let candidates = self.lookup(module, first);
        if !candidates.is_empty() {
            if segments.len() == 1 {
                let mut targets = candidates;
                targets.sort_by_key(target_sort_key);
                targets.dedup_by(|a, b| same_target(a, b));
                if targets.len() == 1 {
                    return Ok(targets.remove(0));
                }
                return Err(Error::new(format!(
                    "`{}` is ambiguous; qualify it",
                    display_segments(segments)
                )));
            }
            // A multi-segment path can only start at a module or a type, so
            // constructors that share the spelling do not make it ambiguous.
            let mut structural: Vec<NameTarget> = candidates
                .into_iter()
                .filter(|target| match target {
                    NameTarget::Module(_) => true,
                    NameTarget::Item(key) => self.is_enum(key),
                })
                .collect();
            if structural.len() > 1 {
                return Err(Error::new(format!(
                    "`{}` is ambiguous; qualify it",
                    display_segments(segments)
                )));
            }
            if structural.is_empty() {
                return Err(Error::new(format!("`{}` is not a module or a type", first)));
            }
            match structural.remove(0) {
                NameTarget::Module(path) => {
                    return self.walk_module(&path, &segments[1..], module);
                }
                NameTarget::Item(key) => {
                    if segments.len() == 2 && self.is_enum(&key) {
                        return self.associated(&key, &segments[1], module);
                    }
                    return Err(Error::new(format!("`{}` is not a module or a type", key)));
                }
            }
        }
        self.walk_module("", segments, module)
    }

    fn walk_module(&self, base: &str, segments: &[String], from: &str) -> Result<NameTarget> {
        if segments.is_empty() {
            self.check_module_visible(base, from)?;
            return Ok(NameTarget::Module(base.to_string()));
        }
        let mut current = base.to_string();
        for (i, segment) in segments.iter().enumerate() {
            let last = i + 1 == segments.len();
            if let Some(target) = self.item_in_module(&current, segment, from)? {
                match target {
                    NameTarget::Module(path) => {
                        if last {
                            return Ok(NameTarget::Module(path));
                        }
                        current = path;
                    }
                    NameTarget::Item(key) => {
                        if last {
                            return Ok(NameTarget::Item(key));
                        }
                        if self.is_enum(&key) && i + 2 == segments.len() {
                            return self.associated(&key, &segments[i + 1], from);
                        }
                        return Err(Error::new(format!("`{}` is not a module or a type", key)));
                    }
                }
            } else {
                return Err(Error::new(format!(
                    "unknown name `{}` in path `{}`",
                    segment,
                    display_segments(segments)
                )));
            }
        }
        Ok(NameTarget::Module(current))
    }

    fn associated(&self, enum_key: &str, member: &str, from: &str) -> Result<NameTarget> {
        self.check_item_visible(enum_key, from)?;
        let Some(entry) = self.enum_entry(enum_key) else {
            return Err(Error::new(format!("`{}` is not an enum", enum_key)));
        };
        if entry.ctors.iter().any(|c| c == member) {
            let key = format!("{}::{}", enum_key, member);
            self.check_item_visible(&key, from)?;
            return Ok(NameTarget::Item(key));
        }
        Err(Error::new(format!(
            "`{}` has no associated item `{}`",
            enum_key, member
        )))
    }

    fn item_in_module(&self, module: &str, name: &str, from: &str) -> Result<Option<NameTarget>> {
        if let Some(&index) = self.module_index.get(module) {
            if let Some(target) = self.modules[index].items.get(name) {
                if let Some(vis) = self.modules[index].item_vis.get(name)
                    && !vis.visible_from(from)
                {
                    let shown = match target {
                        NameTarget::Item(key) => key.clone(),
                        NameTarget::Module(path) => path.clone(),
                    };
                    return Err(Error::new(format!("`{}` is private here", shown)));
                }
                return Ok(Some(target.clone()));
            }
            let path = join_path(module, name);
            if self.known_modules.contains(&path) {
                self.check_module_visible(&path, from)?;
                return Ok(Some(NameTarget::Module(path)));
            }
            if self.globals_item(&path) {
                self.check_item_visible(&path, from)?;
                return Ok(Some(NameTarget::Item(path)));
            }
            return Ok(None);
        }
        let path = join_path(module, name);
        if self.known_modules.contains(&path) {
            self.check_module_visible(&path, from)?;
            return Ok(Some(NameTarget::Module(path)));
        }
        if self.globals_item(&path) {
            self.check_item_visible(&path, from)?;
            return Ok(Some(NameTarget::Item(path)));
        }
        Ok(None)
    }

    fn lookup(&self, module: &str, name: &str) -> Vec<NameTarget> {
        let Some(&index) = self.module_index.get(module) else {
            return self.globals_lookup(module, name);
        };
        let mut targets = Vec::new();
        if let Some(target) = self.modules[index].items.get(name) {
            targets.push(target.clone());
        }
        if let Some(target) = self.modules[index].imports.get(name) {
            targets.push(target.clone());
        }
        let module_path = self.modules[index].path.clone();
        for entry in self.enums.values() {
            if entry.module == module_path && entry.ctors.iter().any(|c| c == name) {
                targets.push(NameTarget::Item(format!("{}::{}", entry.key, name)));
            }
        }
        if targets.is_empty()
            && let Some(candidates) = self.globals.names.prelude.get(name)
        {
            return candidates
                .iter()
                .map(|key| NameTarget::Item(key.clone()))
                .collect();
        }
        if self.known_modules.contains(name) && !name.contains("::") {
            targets.push(NameTarget::Module(name.to_string()));
        }
        targets.sort_by_key(target_sort_key);
        targets.dedup_by(|a, b| same_target(a, b));
        targets
    }

    fn globals_lookup(&self, module: &str, name: &str) -> Vec<NameTarget> {
        if module.is_empty()
            && let Some(keys) = self.globals.names.root.get(name)
        {
            return keys
                .iter()
                .map(|key| NameTarget::Item(key.clone()))
                .collect();
        }
        if let Some(candidates) = self.globals.names.prelude.get(name) {
            return candidates
                .iter()
                .map(|key| NameTarget::Item(key.clone()))
                .collect();
        }
        let path = join_path(module, name);
        if self.globals_item(&path) {
            return vec![NameTarget::Item(path)];
        }
        if self.known_modules.contains(name) && !name.contains("::") {
            return vec![NameTarget::Module(name.to_string())];
        }
        Vec::new()
    }

    fn globals_item(&self, key: &str) -> bool {
        self.globals.enums.contains_key(key)
            || self.globals.defs.contains_key(key)
            || self.globals.externs.contains_key(key)
    }

    fn is_enum(&self, key: &str) -> bool {
        self.enums.contains_key(key) || self.globals.get_enum(key).is_some()
    }

    fn enum_entry(&self, key: &str) -> Option<EnumEntry> {
        if let Some(entry) = self.enums.get(key) {
            return Some(EnumEntry {
                key: entry.key.clone(),
                params: entry.params,
                is_pub: entry.is_pub,
                module: entry.module.clone(),
                ctors: entry.ctors.clone(),
                fields: entry.fields.clone(),
            });
        }
        let info = self.globals.get_enum(key)?;
        Some(EnumEntry {
            key: key.to_string(),
            params: info.param_names.len(),
            is_pub: self.visible(key, ""),
            module: key
                .rsplit_once("::")
                .map_or(String::new(), |(m, _)| m.to_string()),
            ctors: info.ctors.iter().map(|c| c.name.clone()).collect(),
            fields: info.ctors.iter().map(|c| c.num_fields).collect(),
        })
    }

    fn is_ctor(&self, key: &str) -> bool {
        self.ctor_enum.contains_key(key) || self.globals.is_constructor(key)
    }

    fn ctor_fields(&self, key: &str) -> Option<usize> {
        if let Some(enum_key) = self.ctor_enum.get(key) {
            let short = key.rsplit("::").next().unwrap_or(key);
            let entry = self.enum_entry(enum_key)?;
            let index = entry.ctors.iter().position(|c| c == short)?;
            return entry.fields.get(index).copied();
        }
        self.globals.get_ctor(key).map(|c| c.num_fields)
    }

    fn parent_module(&self, module: &str) -> Result<String> {
        let Some(&index) = self.module_index.get(module) else {
            return Err(Error::new(format!(
                "`super` is not available in module `{}`",
                module
            )));
        };
        let Some(parent) = self.modules[index].parent else {
            return Err(Error::new("`super` has no enclosing module"));
        };
        Ok(self.modules[parent].path.clone())
    }

    fn visible(&self, key: &str, from: &str) -> bool {
        if let Some(vis) = self.vis.get(key) {
            return vis.visible_from(from);
        }
        if let Some(vis) = self.globals.names.visibility.get(key) {
            return vis.visible_from(from);
        }
        true
    }

    fn module_visible(&self, path: &str, from: &str) -> bool {
        match self.module_vis.get(path) {
            Some(vis) => vis.visible_from(from),
            None => true,
        }
    }

    fn check_module_visible(&self, path: &str, from: &str) -> Result<()> {
        if self.module_visible(path, from) {
            Ok(())
        } else {
            Err(Error::new(format!("module `{}` is private here", path)))
        }
    }

    fn check_item_visible(&self, key: &str, from: &str) -> Result<()> {
        if self.visible(key, from) {
            Ok(())
        } else {
            Err(Error::new(format!("`{}` is private here", key)))
        }
    }

    fn record_dep(&self, key: &str, deps: &mut Deps) {
        if key.starts_with("IO::") {
            return;
        }
        if let Some(enum_key) = self.ctor_enum.get(key) {
            deps.items.insert(enum_key.clone());
            return;
        }
        if let Some(info) = self.globals.get_ctor(key) {
            deps.items.insert(info.enum_name.clone());
            return;
        }
        deps.items.insert(key.to_string());
    }
}

#[derive(Clone, Debug)]
enum ResolvedName {
    Local,
    Item(String),
    Ambiguous(Vec<String>),
}

fn target_sort_key(target: &NameTarget) -> (u8, String) {
    match target {
        NameTarget::Item(key) => (0, key.clone()),
        NameTarget::Module(path) => (1, path.clone()),
    }
}

fn builtin_name(name: &str) -> bool {
    matches!(name, "IO") || PrimTy::from_name(name).is_some()
}

fn display_segments(segments: &[String]) -> String {
    segments.join("::")
}
