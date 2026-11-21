use std::rc::Rc;

use crate::core::{Branch, PrimLit, PrimOp, PrimTy, RcTm};

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
    PrimTy(PrimTy),
    PrimLit(PrimLit),
    Pi(V, Rc<Closure>, bool),
    Lam(Rc<Closure>),
    Neutral(Rc<Neutral>),
    Ind(String, Vec<V>),
    Ctor(String, usize, Vec<V>),
    Eq(V, V, V),
    Refl(V),
    Glob(String),
    /// The opaque action type `IO[A]`.
    Io(V),
    /// A deferred action value.
    Action(Rc<Action>),
}

/// A deferred `IO` action. Actions have no computation rules: they are
/// compared structurally and executed only by the program entry point.
#[derive(Clone, Debug)]
pub enum Action {
    /// `io_pure(v)`.
    Pure(V),
    /// `io_bind(m, k)`.
    Bind(V, V),
    /// A saturated host capability call.
    Extern {
        name: String,
        import: String,
        args: Vec<V>,
    },
}

#[derive(Clone, Debug)]
pub enum Neutral {
    Var(usize),
    Global(String),
    PrimOp(PrimOp, Vec<V>),
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
        motive: RcTm,
        refl_case: RcTm,
    },
}

pub fn vtype(u: u32) -> V {
    Rc::new(Val::Type(u))
}

pub fn vprop() -> V {
    Rc::new(Val::Prop)
}

pub fn vprimty(t: PrimTy) -> V {
    Rc::new(Val::PrimTy(t))
}

pub fn vprimlit(lit: PrimLit) -> V {
    Rc::new(Val::PrimLit(lit))
}

pub fn vprimop(op: PrimOp, args: Vec<V>) -> V {
    vneutral(Neutral::PrimOp(op, args))
}

pub fn vpi(dom: V, closure: Closure, implicit: bool) -> V {
    Rc::new(Val::Pi(dom, Rc::new(closure), implicit))
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

pub fn vio(t: V) -> V {
    Rc::new(Val::Io(t))
}

pub fn vpure(v: V) -> V {
    Rc::new(Val::Action(Rc::new(Action::Pure(v))))
}

pub fn vbind(m: V, k: V) -> V {
    Rc::new(Val::Action(Rc::new(Action::Bind(m, k))))
}

pub fn vextern(name: impl Into<String>, import: impl Into<String>, args: Vec<V>) -> V {
    Rc::new(Val::Action(Rc::new(Action::Extern {
        name: name.into(),
        import: import.into(),
        args,
    })))
}

pub fn vvar(level: usize) -> V {
    vneutral(Neutral::Var(level))
}
