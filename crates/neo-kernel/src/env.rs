use std::collections::HashMap;

use crate::core::RcTm;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sort {
    Prop,
    Type(u32),
}

impl Sort {
    pub fn level(self) -> u32 {
        match self {
            Sort::Prop => 0,
            Sort::Type(u) => u,
        }
    }

    pub fn max(a: Sort, b: Sort) -> Sort {
        Sort::Type(a.level().max(b.level()))
    }
}

#[derive(Clone, Debug)]
pub struct CtorInfo {
    pub name: String,
    pub enum_name: String,
    pub index: usize,
    pub num_params: usize,
    pub num_fields: usize,
    pub ty: RcTm,
}

impl CtorInfo {
    pub fn arity(&self) -> usize {
        self.num_params + self.num_fields
    }
}

#[derive(Clone, Debug)]
pub struct EnumInfo {
    pub name: String,
    pub param_names: Vec<String>,
    pub param_tys: Vec<RcTm>,
    pub sort: Sort,
    pub ctors: Vec<CtorInfo>,
    /// True when a `Prop`-sorted enum may be eliminated into a `Type`:
    /// the enum is empty, or it has exactly one constructor and every field
    /// is itself a proposition. This is the singleton-elimination rule.
    pub prop_eliminable: bool,
}

#[derive(Clone, Debug)]
pub enum GlobalDef {
    Fn {
        ty: RcTm,
        body: Option<RcTm>,
        num_params: usize,
        /// A body may only be unfolded once it has passed the authoritative
        /// recursion and kernel checks. Unchecked bodies stay opaque.
        checked: bool,
    },
    Ctor {
        enum_name: String,
        index: usize,
        ty: RcTm,
        arity: usize,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Globals {
    pub defs: HashMap<String, GlobalDef>,
    pub enums: HashMap<String, EnumInfo>,
}

impl Globals {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_fn_placeholder(&mut self, name: impl Into<String>, ty: RcTm, num_params: usize) {
        self.defs.insert(
            name.into(),
            GlobalDef::Fn {
                ty,
                body: None,
                num_params,
                checked: false,
            },
        );
    }

    /// Commit a body that has already passed the recursion checker and the
    /// kernel type check. Only committed bodies are unfolded by evaluation.
    pub fn set_fn_body(&mut self, name: &str, body: RcTm) {
        if let Some(GlobalDef::Fn {
            body: b, checked, ..
        }) = self.defs.get_mut(name)
        {
            *b = Some(body);
            *checked = true;
        }
    }

    pub fn get_fn_body(&self, name: &str) -> Option<&RcTm> {
        match self.defs.get(name) {
            Some(GlobalDef::Fn {
                body: Some(b),
                checked: true,
                ..
            }) => Some(b),
            _ => None,
        }
    }

    pub fn get_fn_num_params(&self, name: &str) -> Option<usize> {
        match self.defs.get(name) {
            Some(GlobalDef::Fn { num_params, .. }) => Some(*num_params),
            _ => None,
        }
    }

    /// A view of the globals in which the listed definitions are opaque.
    /// Used by the program checker so that checking a definition never
    /// depends on the body currently being checked.
    pub fn sealed(&self, names: &[&str]) -> Globals {
        let mut sealed = self.clone();
        for name in names {
            if let Some(GlobalDef::Fn { body, checked, .. }) = sealed.defs.get_mut(*name) {
                *body = None;
                *checked = false;
            }
        }
        sealed
    }

    /// Names of all functions with a committed body, in deterministic order.
    pub fn fn_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .defs
            .iter()
            .filter_map(|(name, def)| match def {
                GlobalDef::Fn {
                    body: Some(_),
                    checked: true,
                    ..
                } => Some(name.clone()),
                _ => None,
            })
            .collect();
        names.sort();
        names
    }

    pub fn get_ty(&self, name: &str) -> Option<&RcTm> {
        match self.defs.get(name) {
            Some(GlobalDef::Fn { ty, .. }) => Some(ty),
            Some(GlobalDef::Ctor { ty, .. }) => Some(ty),
            None => None,
        }
    }

    pub fn get_ctor(&self, name: &str) -> Option<&CtorInfo> {
        match self.defs.get(name) {
            Some(GlobalDef::Ctor {
                enum_name, index, ..
            }) => self.enums.get(enum_name).and_then(|e| e.ctors.get(*index)),
            _ => None,
        }
    }

    pub fn get_ctor_info(&self, enum_name: &str, index: usize) -> Option<&CtorInfo> {
        self.enums.get(enum_name).and_then(|e| e.ctors.get(index))
    }

    pub fn get_enum(&self, name: &str) -> Option<&EnumInfo> {
        self.enums.get(name)
    }

    pub fn sort_of_enum(&self, name: &str) -> Option<Sort> {
        self.enums.get(name).map(|e| e.sort)
    }

    pub fn is_constructor(&self, name: &str) -> bool {
        matches!(self.defs.get(name), Some(GlobalDef::Ctor { .. }))
    }

    pub fn ctor_arity(&self, enum_name: &str, index: usize) -> Option<usize> {
        self.enums
            .get(enum_name)
            .and_then(|e| e.ctors.get(index))
            .map(|c| c.arity())
    }
}
