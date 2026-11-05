use std::rc::Rc;

use crate::core::{Branch, RcTm};

pub type V = Rc<Val>;
pub type Env = Vec<V>;

#[derive(Clone, Debug)]
pub struct Closure {
    pub dom: Option<RcTm>,
    pub env: Env,
    pub body: RcTm,
}

#[derive(Clone, Debug)]
pub enum Val {
    Type(u32),
    Prop,
    Pi(V, Rc<Closure>),
    Lam(Rc<Closure>),
    Neutral(Rc<Neutral>),
    Ind(String, Vec<V>),
    Ctor(String, usize, Vec<V>),
    Eq(V, V, V),
    Refl(V),
    Glob(String),
}

#[derive(Clone, Debug)]
pub enum Neutral {
    Var(usize),
    Global(String),
    App(Rc<Neutral>, V),
    Match {
        env: Env,
        motive: RcTm,
        scrut: V,
        branches: Vec<Branch>,
    },
    J {
        env: Env,
        ty: RcTm,
        lhs: RcTm,
        rhs: RcTm,
        proof: V,
        result_ty: RcTm,
        refl_case: RcTm,
    },
}

pub fn vtype(u: u32) -> V {
    Rc::new(Val::Type(u))
}

pub fn vprop() -> V {
    Rc::new(Val::Prop)
}

pub fn vpi(dom: V, closure: Closure) -> V {
    Rc::new(Val::Pi(dom, Rc::new(closure)))
}

pub fn vlam(closure: Closure) -> V {
    Rc::new(Val::Lam(Rc::new(closure)))
}

pub fn vneutral(n: Neutral) -> V {
    Rc::new(Val::Neutral(Rc::new(n)))
}

pub fn vind(name: impl Into<String>, args: Vec<V>) -> V {
    Rc::new(Val::Ind(name.into(), args))
}

pub fn vctor(enum_name: impl Into<String>, index: usize, args: Vec<V>) -> V {
    Rc::new(Val::Ctor(enum_name.into(), index, args))
}

pub fn veq(ty: V, lhs: V, rhs: V) -> V {
    Rc::new(Val::Eq(ty, lhs, rhs))
}

pub fn vrefl(v: V) -> V {
    Rc::new(Val::Refl(v))
}

pub fn vglob(name: impl Into<String>) -> V {
    Rc::new(Val::Glob(name.into()))
}

pub fn vvar(level: usize) -> V {
    vneutral(Neutral::Var(level))
}
