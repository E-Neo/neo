use std::rc::Rc;

pub type RcTm = Rc<Tm>;

/// A primitive type. Primitives are opaque: they have no constructors and are
/// eliminated only by the primitive operations below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimTy {
    U32,
    String,
    Bytes,
}

impl PrimTy {
    pub const ALL: [PrimTy; 3] = [PrimTy::U32, PrimTy::String, PrimTy::Bytes];

    pub fn name(self) -> &'static str {
        match self {
            PrimTy::U32 => "U32",
            PrimTy::String => "String",
            PrimTy::Bytes => "Bytes",
        }
    }

    pub fn from_name(name: &str) -> Option<PrimTy> {
        PrimTy::ALL.iter().copied().find(|ty| ty.name() == name)
    }
}

/// A primitive literal. `String` and `Bytes` both hold raw bytes; a `String`
/// is expected to contain UTF-8, but the representation does not enforce it, so
/// byte slicing must stay on character boundaries to remain meaningful.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimLit {
    U32(u32),
    Str(Rc<[u8]>),
    Bytes(Rc<[u8]>),
}

impl PrimLit {
    pub fn ty(&self) -> PrimTy {
        match self {
            PrimLit::U32(_) => PrimTy::U32,
            PrimLit::Str(_) => PrimTy::String,
            PrimLit::Bytes(_) => PrimTy::Bytes,
        }
    }

    pub fn show(&self) -> String {
        match self {
            PrimLit::U32(n) => n.to_string(),
            PrimLit::Str(bytes) => format!("\"{}\"", escape_bytes(bytes, true)),
            PrimLit::Bytes(bytes) => format!("b\"{}\"", escape_bytes(bytes, false)),
        }
    }
}

fn escape_bytes(bytes: &[u8], printable: bool) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            b'\n' => out.push_str("\\n"),
            b'\t' => out.push_str("\\t"),
            b'\r' => out.push_str("\\r"),
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            0x20..=0x7e if printable => out.push(b as char),
            0x20..=0x7e => out.push_str(&format!("\\x{:02x}", b)),
            _ => out.push_str(&format!("\\x{:02x}", b)),
        }
    }
    out
}

/// The result of evaluating a primitive operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimOutcome {
    U32(u32),
    Bool(bool),
    Bytes(Rc<[u8]>),
}

/// The result type of a primitive operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimRet {
    U32,
    Bool,
    Str,
    Bytes,
}

/// A primitive operation. Operations must be fully applied; they are not
/// first-class values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    StrLength,
    StrSlice,
    StrConcat,
    StrEq,
    StrNe,
    StrToBytes,
    BytesLength,
    BytesSlice,
    BytesConcat,
    BytesEq,
    BytesNe,
    BytesGet,
    StrFromBytes,
    BytesUtf8Check,
}

const U32_BINARY: [PrimTy; 2] = [PrimTy::U32, PrimTy::U32];
const STR_UNARY: [PrimTy; 1] = [PrimTy::String];
const STR_BINARY: [PrimTy; 2] = [PrimTy::String, PrimTy::String];
const STR_SLICE: [PrimTy; 3] = [PrimTy::String, PrimTy::U32, PrimTy::U32];
const BYTES_UNARY: [PrimTy; 1] = [PrimTy::Bytes];
const BYTES_BINARY: [PrimTy; 2] = [PrimTy::Bytes, PrimTy::Bytes];
const BYTES_SLICE: [PrimTy; 3] = [PrimTy::Bytes, PrimTy::U32, PrimTy::U32];
const BYTES_GET: [PrimTy; 2] = [PrimTy::Bytes, PrimTy::U32];

impl PrimOp {
    pub const ALL: [PrimOp; 30] = [
        PrimOp::Add,
        PrimOp::Sub,
        PrimOp::Mul,
        PrimOp::Div,
        PrimOp::Rem,
        PrimOp::And,
        PrimOp::Or,
        PrimOp::Xor,
        PrimOp::Shl,
        PrimOp::Shr,
        PrimOp::Lt,
        PrimOp::Le,
        PrimOp::Gt,
        PrimOp::Ge,
        PrimOp::Eq,
        PrimOp::Ne,
        PrimOp::StrLength,
        PrimOp::StrSlice,
        PrimOp::StrConcat,
        PrimOp::StrEq,
        PrimOp::StrNe,
        PrimOp::StrToBytes,
        PrimOp::BytesLength,
        PrimOp::BytesSlice,
        PrimOp::BytesConcat,
        PrimOp::BytesEq,
        PrimOp::BytesNe,
        PrimOp::BytesGet,
        PrimOp::StrFromBytes,
        PrimOp::BytesUtf8Check,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PrimOp::Add => "u32_add",
            PrimOp::Sub => "u32_sub",
            PrimOp::Mul => "u32_mul",
            PrimOp::Div => "u32_div",
            PrimOp::Rem => "u32_rem",
            PrimOp::And => "u32_and",
            PrimOp::Or => "u32_or",
            PrimOp::Xor => "u32_xor",
            PrimOp::Shl => "u32_shl",
            PrimOp::Shr => "u32_shr",
            PrimOp::Lt => "u32_lt",
            PrimOp::Le => "u32_le",
            PrimOp::Gt => "u32_gt",
            PrimOp::Ge => "u32_ge",
            PrimOp::Eq => "u32_eq",
            PrimOp::Ne => "u32_ne",
            PrimOp::StrLength => "str_length",
            PrimOp::StrSlice => "str_slice",
            PrimOp::StrConcat => "str_concat",
            PrimOp::StrEq => "str_eq",
            PrimOp::StrNe => "str_ne",
            PrimOp::StrToBytes => "str_to_bytes",
            PrimOp::BytesLength => "bytes_length",
            PrimOp::BytesSlice => "bytes_slice",
            PrimOp::BytesConcat => "bytes_concat",
            PrimOp::BytesEq => "bytes_eq",
            PrimOp::BytesNe => "bytes_ne",
            PrimOp::BytesGet => "bytes_get",
            PrimOp::StrFromBytes => "str_from_bytes",
            PrimOp::BytesUtf8Check => "bytes_utf8_check",
        }
    }

    pub fn from_name(name: &str) -> Option<PrimOp> {
        PrimOp::ALL.iter().copied().find(|op| op.name() == name)
    }

    /// Whether this operation is an internal implementation of an operator.
    /// Internal operations cannot be written by name in a program.
    pub fn is_internal(self) -> bool {
        self.operator().is_some()
    }

    /// The surface operator that implements this operation, if any.
    pub fn operator(self) -> Option<&'static str> {
        match self {
            PrimOp::Add => Some("+"),
            PrimOp::Sub => Some("-"),
            PrimOp::Mul => Some("*"),
            PrimOp::Div => Some("/"),
            PrimOp::Rem => Some("%"),
            PrimOp::And => Some("&"),
            PrimOp::Or => Some("|"),
            PrimOp::Xor => Some("^"),
            PrimOp::Shl => Some("<<"),
            PrimOp::Shr => Some(">>"),
            PrimOp::Lt => Some("<"),
            PrimOp::Le => Some("<="),
            PrimOp::Gt => Some(">"),
            PrimOp::Ge => Some(">="),
            PrimOp::Eq => Some("=="),
            PrimOp::Ne => Some("!="),
            _ => None,
        }
    }

    /// The argument types of this operation, in order.
    pub fn domains(self) -> &'static [PrimTy] {
        match self {
            PrimOp::StrLength => &STR_UNARY,
            PrimOp::StrSlice => &STR_SLICE,
            PrimOp::StrConcat | PrimOp::StrEq | PrimOp::StrNe => &STR_BINARY,
            PrimOp::StrToBytes => &STR_UNARY,
            PrimOp::BytesLength => &BYTES_UNARY,
            PrimOp::BytesSlice => &BYTES_SLICE,
            PrimOp::BytesConcat | PrimOp::BytesEq | PrimOp::BytesNe => &BYTES_BINARY,
            PrimOp::BytesGet => &BYTES_GET,
            PrimOp::StrFromBytes | PrimOp::BytesUtf8Check => &BYTES_UNARY,
            _ => &U32_BINARY,
        }
    }

    pub fn result(self) -> PrimRet {
        match self {
            PrimOp::Lt
            | PrimOp::Le
            | PrimOp::Gt
            | PrimOp::Ge
            | PrimOp::Eq
            | PrimOp::Ne
            | PrimOp::StrEq
            | PrimOp::StrNe
            | PrimOp::BytesEq
            | PrimOp::BytesNe => PrimRet::Bool,
            PrimOp::StrLength | PrimOp::BytesLength | PrimOp::BytesGet | PrimOp::BytesUtf8Check => {
                PrimRet::U32
            }
            PrimOp::StrSlice | PrimOp::StrConcat | PrimOp::StrFromBytes => PrimRet::Str,
            PrimOp::StrToBytes | PrimOp::BytesSlice | PrimOp::BytesConcat => PrimRet::Bytes,
            _ => PrimRet::U32,
        }
    }

    /// Whether the operation produces a `Bool`.
    pub fn returns_bool(self) -> bool {
        self.result() == PrimRet::Bool
    }

    /// Whether every argument is a literal of the expected type.
    pub fn accepts(self, args: &[PrimLit]) -> bool {
        let domains = self.domains();
        args.len() == domains.len() && args.iter().zip(domains).all(|(arg, dom)| arg.ty() == *dom)
    }

    /// Execute the operation on literals of the expected types.
    ///
    /// Arithmetic wraps, division and remainder by zero return zero, and shift
    /// amounts use their low five bits. String and byte slices clamp their
    /// bounds to the value length, and `bytes_get` returns `0` out of bounds.
    pub fn apply(self, args: &[PrimLit]) -> PrimOutcome {
        use PrimOutcome::{Bool, Bytes, U32};
        let x = arg_word(args, 0);
        let y = arg_word(args, 1);
        let z = arg_word(args, 2);
        match self {
            PrimOp::Add => U32(x.wrapping_add(y)),
            PrimOp::Sub => U32(x.wrapping_sub(y)),
            PrimOp::Mul => U32(x.wrapping_mul(y)),
            PrimOp::Div => U32(x.checked_div(y).unwrap_or(0)),
            PrimOp::Rem => U32(x.checked_rem(y).unwrap_or(0)),
            PrimOp::And => U32(x & y),
            PrimOp::Or => U32(x | y),
            PrimOp::Xor => U32(x ^ y),
            PrimOp::Shl => U32(x << (y & 31)),
            PrimOp::Shr => U32(x >> (y & 31)),
            PrimOp::Lt => Bool(x < y),
            PrimOp::Le => Bool(x <= y),
            PrimOp::Gt => Bool(x > y),
            PrimOp::Ge => Bool(x >= y),
            PrimOp::Eq => Bool(x == y),
            PrimOp::Ne => Bool(x != y),
            PrimOp::StrLength | PrimOp::BytesLength => U32(arg_bytes(args, 0).len() as u32),
            PrimOp::StrSlice | PrimOp::BytesSlice => {
                let bytes = arg_bytes(args, 0);
                let (start, end) = clamp_range(bytes.len(), y, z);
                Bytes(Rc::from(&bytes[start..end]))
            }
            PrimOp::StrConcat | PrimOp::BytesConcat => {
                let a = arg_bytes(args, 0);
                let b = arg_bytes(args, 1);
                let mut out = Vec::with_capacity(a.len() + b.len());
                out.extend_from_slice(a);
                out.extend_from_slice(b);
                Bytes(Rc::from(out))
            }
            PrimOp::StrEq | PrimOp::BytesEq => Bool(arg_bytes(args, 0) == arg_bytes(args, 1)),
            PrimOp::StrNe | PrimOp::BytesNe => Bool(arg_bytes(args, 0) != arg_bytes(args, 1)),
            PrimOp::StrToBytes | PrimOp::StrFromBytes => Bytes(Rc::from(arg_bytes(args, 0))),
            PrimOp::BytesGet => {
                let byte = arg_bytes(args, 0).get(y as usize).copied().unwrap_or(0);
                U32(byte as u32)
            }
            PrimOp::BytesUtf8Check => {
                U32(utf8_error(arg_bytes(args, 0)).map_or(0, |i| i as u32 + 1))
            }
        }
    }
}

fn arg_word(args: &[PrimLit], i: usize) -> u32 {
    match args.get(i) {
        Some(PrimLit::U32(n)) => *n,
        _ => 0,
    }
}

fn arg_bytes(args: &[PrimLit], i: usize) -> &[u8] {
    match args.get(i) {
        Some(PrimLit::Str(bytes) | PrimLit::Bytes(bytes)) => bytes,
        _ => &[],
    }
}

/// The index of the first byte that starts an invalid UTF-8 sequence.
///
/// Validation is strict: overlong forms, surrogates, code points above
/// `U+10FFFF`, and truncated sequences are all rejected. A truncated sequence
/// reports the index where it starts, not where it ends.
fn utf8_error(bytes: &[u8]) -> Option<usize> {
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        let b = bytes[i];
        if b < 0x80 {
            i += 1;
            continue;
        }
        let n = match b {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => return Some(i),
        };
        let (lo, hi) = match b {
            0xe0 => (0xa0, 0xbf),
            0xed => (0x80, 0x9f),
            0xf0 => (0x90, 0xbf),
            0xf4 => (0x80, 0x8f),
            _ => (0x80, 0xbf),
        };
        if i + n > len {
            return Some(i);
        }
        if !(lo..=hi).contains(&bytes[i + 1]) {
            return Some(i);
        }
        if bytes[i + 2..i + n]
            .iter()
            .any(|&c| !(0x80..=0xbf).contains(&c))
        {
            return Some(i);
        }
        i += n;
    }
    None
}

fn clamp_range(len: usize, start: u32, end: u32) -> (usize, usize) {
    let limit = len.min(u32::MAX as usize) as u32;
    let start = start.min(limit);
    let end = end.min(limit).max(start);
    (start as usize, end as usize)
}

#[derive(Clone, Debug)]
pub enum Tm {
    Var(usize),
    Const(String),
    Type(u32),
    Prop,
    /// A primitive type former.
    PrimTy(PrimTy),
    /// A primitive literal.
    PrimLit(PrimLit),
    /// A fully applied primitive operation.
    PrimOp(PrimOp, Vec<RcTm>),
    Pi {
        name: String,
        dom: RcTm,
        cod: RcTm,
        implicit: bool,
    },
    Lam {
        name: String,
        dom: RcTm,
        body: RcTm,
    },
    App(RcTm, RcTm),
    Ind(String, Vec<RcTm>),
    Ctor(String, usize, Vec<RcTm>),
    Match {
        motive: RcTm,
        scrut: RcTm,
        branches: Vec<Branch>,
    },
    Eq(RcTm, RcTm, RcTm),
    Refl(RcTm),
    J {
        ty: RcTm,
        lhs: RcTm,
        rhs: RcTm,
        proof: RcTm,
        motive: RcTm,
        refl_case: RcTm,
    },
}

#[derive(Clone, Debug)]
pub struct Branch {
    pub ctor: String,
    pub binders: usize,
    pub body: RcTm,
    /// True when the constructor's result indices cannot unify with the
    /// scrutinee type. Such a branch is unreachable for well-typed scrutinees:
    /// the kernel does not check its body, and the evaluator never selects it.
    pub impossible: bool,
}

pub fn rc(t: Tm) -> RcTm {
    Rc::new(t)
}

pub fn var(i: usize) -> RcTm {
    rc(Tm::Var(i))
}

pub fn prim_ty(t: PrimTy) -> RcTm {
    rc(Tm::PrimTy(t))
}

pub fn prim_lit(lit: PrimLit) -> RcTm {
    rc(Tm::PrimLit(lit))
}

pub fn prim_op(op: PrimOp, args: Vec<RcTm>) -> RcTm {
    rc(Tm::PrimOp(op, args))
}

pub fn app(f: RcTm, a: RcTm) -> RcTm {
    rc(Tm::App(f, a))
}

pub fn app_many(f: RcTm, args: Vec<RcTm>) -> RcTm {
    args.into_iter().fold(f, app)
}

pub fn pi(name: impl Into<String>, dom: RcTm, cod: RcTm) -> RcTm {
    pi_with(name, dom, cod, false)
}

pub fn pi_with(name: impl Into<String>, dom: RcTm, cod: RcTm, implicit: bool) -> RcTm {
    rc(Tm::Pi {
        name: name.into(),
        dom,
        cod,
        implicit,
    })
}

pub fn lam(name: impl Into<String>, dom: RcTm, body: RcTm) -> RcTm {
    rc(Tm::Lam {
        name: name.into(),
        dom,
        body,
    })
}

pub fn const_(name: impl Into<String>) -> RcTm {
    rc(Tm::Const(name.into()))
}

pub fn ind(name: impl Into<String>, args: Vec<RcTm>) -> RcTm {
    rc(Tm::Ind(name.into(), args))
}

pub fn ctor(enum_name: impl Into<String>, index: usize, args: Vec<RcTm>) -> RcTm {
    rc(Tm::Ctor(enum_name.into(), index, args))
}

pub fn eq(ty: RcTm, lhs: RcTm, rhs: RcTm) -> RcTm {
    rc(Tm::Eq(ty, lhs, rhs))
}

/// Shift free de Bruijn indices by `delta`, respecting binders.
pub fn shift(tm: &RcTm, delta: isize, cutoff: usize) -> RcTm {
    match &**tm {
        Tm::Var(i) => {
            if *i >= cutoff {
                var((*i as isize + delta) as usize)
            } else {
                tm.clone()
            }
        }
        Tm::Const(_) | Tm::Type(_) | Tm::Prop | Tm::PrimTy(_) | Tm::PrimLit(_) => tm.clone(),
        Tm::PrimOp(op, args) => {
            prim_op(*op, args.iter().map(|a| shift(a, delta, cutoff)).collect())
        }
        Tm::Pi {
            name,
            dom,
            cod,
            implicit,
        } => pi_with(
            name.clone(),
            shift(dom, delta, cutoff),
            shift(cod, delta, cutoff + 1),
            *implicit,
        ),
        Tm::Lam { name, dom, body } => lam(
            name.clone(),
            shift(dom, delta, cutoff),
            shift(body, delta, cutoff + 1),
        ),
        Tm::App(f, a) => app(shift(f, delta, cutoff), shift(a, delta, cutoff)),
        Tm::Ind(name, args) => ind(
            name.clone(),
            args.iter().map(|a| shift(a, delta, cutoff)).collect(),
        ),
        Tm::Ctor(name, index, args) => ctor(
            name.clone(),
            *index,
            args.iter().map(|a| shift(a, delta, cutoff)).collect(),
        ),
        Tm::Match {
            motive,
            scrut,
            branches,
        } => rc(Tm::Match {
            motive: shift(motive, delta, cutoff),
            scrut: shift(scrut, delta, cutoff),
            branches: branches
                .iter()
                .map(|b| Branch {
                    ctor: b.ctor.clone(),
                    binders: b.binders,
                    body: shift(&b.body, delta, cutoff + b.binders),
                    impossible: b.impossible,
                })
                .collect(),
        }),
        Tm::Eq(a, b, c) => eq(
            shift(a, delta, cutoff),
            shift(b, delta, cutoff),
            shift(c, delta, cutoff),
        ),
        Tm::Refl(a) => rc(Tm::Refl(shift(a, delta, cutoff))),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => rc(Tm::J {
            ty: shift(ty, delta, cutoff),
            lhs: shift(lhs, delta, cutoff),
            rhs: shift(rhs, delta, cutoff),
            proof: shift(proof, delta, cutoff),
            motive: shift(motive, delta, cutoff),
            refl_case: shift(refl_case, delta, cutoff),
        }),
    }
}

/// Substitute `replacement` for `Var(index)` in `tm`, shifting the
/// replacement under binders as required. The result is a capture-avoiding
/// substitution in the de Bruijn representation.
pub fn subst(tm: &RcTm, index: usize, replacement: &RcTm) -> RcTm {
    subst_at(tm, index, replacement, 0)
}

fn subst_at(tm: &RcTm, index: usize, replacement: &RcTm, depth: usize) -> RcTm {
    match &**tm {
        Tm::Var(i) => {
            if *i == index + depth {
                shift(replacement, depth as isize, 0)
            } else {
                tm.clone()
            }
        }
        Tm::Const(_) | Tm::Type(_) | Tm::Prop | Tm::PrimTy(_) | Tm::PrimLit(_) => tm.clone(),
        Tm::PrimOp(op, args) => prim_op(
            *op,
            args.iter()
                .map(|a| subst_at(a, index, replacement, depth))
                .collect(),
        ),
        Tm::Pi {
            name,
            dom,
            cod,
            implicit,
        } => pi_with(
            name.clone(),
            subst_at(dom, index, replacement, depth),
            subst_at(cod, index, replacement, depth + 1),
            *implicit,
        ),
        Tm::Lam { name, dom, body } => lam(
            name.clone(),
            subst_at(dom, index, replacement, depth),
            subst_at(body, index, replacement, depth + 1),
        ),
        Tm::App(f, a) => app(
            subst_at(f, index, replacement, depth),
            subst_at(a, index, replacement, depth),
        ),
        Tm::Ind(name, args) => ind(
            name.clone(),
            args.iter()
                .map(|a| subst_at(a, index, replacement, depth))
                .collect(),
        ),
        Tm::Ctor(name, i, args) => ctor(
            name.clone(),
            *i,
            args.iter()
                .map(|a| subst_at(a, index, replacement, depth))
                .collect(),
        ),
        Tm::Match {
            motive,
            scrut,
            branches,
        } => rc(Tm::Match {
            motive: subst_at(motive, index, replacement, depth),
            scrut: subst_at(scrut, index, replacement, depth),
            branches: branches
                .iter()
                .map(|b| Branch {
                    ctor: b.ctor.clone(),
                    binders: b.binders,
                    body: subst_at(&b.body, index, replacement, depth + b.binders),
                    impossible: b.impossible,
                })
                .collect(),
        }),
        Tm::Eq(a, b, c) => eq(
            subst_at(a, index, replacement, depth),
            subst_at(b, index, replacement, depth),
            subst_at(c, index, replacement, depth),
        ),
        Tm::Refl(a) => rc(Tm::Refl(subst_at(a, index, replacement, depth))),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => rc(Tm::J {
            ty: subst_at(ty, index, replacement, depth),
            lhs: subst_at(lhs, index, replacement, depth),
            rhs: subst_at(rhs, index, replacement, depth),
            proof: subst_at(proof, index, replacement, depth),
            motive: subst_at(motive, index, replacement, depth),
            refl_case: subst_at(refl_case, index, replacement, depth),
        }),
    }
}

/// Replace every occurrence of `target` in `tm` with `replacement`, shifting
/// under binders so that no free variable is captured. `target`, `replacement`,
/// and `tm` are terms in the same de Bruijn context.
pub fn abstract_term(target: &RcTm, replacement: &RcTm, tm: &RcTm) -> RcTm {
    abstract_at(target, replacement, tm, 0)
}

fn abstract_at(target: &RcTm, replacement: &RcTm, tm: &RcTm, depth: usize) -> RcTm {
    let target_here = shift(target, depth as isize, 0);
    if same_tm(tm, &target_here) {
        return shift(replacement, depth as isize, 0);
    }
    match &**tm {
        Tm::Var(_) | Tm::Const(_) | Tm::Type(_) | Tm::Prop | Tm::PrimTy(_) | Tm::PrimLit(_) => {
            tm.clone()
        }
        Tm::PrimOp(op, args) => prim_op(
            *op,
            args.iter()
                .map(|a| abstract_at(target, replacement, a, depth))
                .collect(),
        ),
        Tm::Pi {
            name,
            dom,
            cod,
            implicit,
        } => pi_with(
            name.clone(),
            abstract_at(target, replacement, dom, depth),
            abstract_at(target, replacement, cod, depth + 1),
            *implicit,
        ),
        Tm::Lam { name, dom, body } => lam(
            name.clone(),
            abstract_at(target, replacement, dom, depth),
            abstract_at(target, replacement, body, depth + 1),
        ),
        Tm::App(f, a) => app(
            abstract_at(target, replacement, f, depth),
            abstract_at(target, replacement, a, depth),
        ),
        Tm::Ind(name, args) => ind(
            name.clone(),
            args.iter()
                .map(|a| abstract_at(target, replacement, a, depth))
                .collect(),
        ),
        Tm::Ctor(name, index, args) => ctor(
            name.clone(),
            *index,
            args.iter()
                .map(|a| abstract_at(target, replacement, a, depth))
                .collect(),
        ),
        Tm::Match {
            motive,
            scrut,
            branches,
        } => rc(Tm::Match {
            motive: abstract_at(target, replacement, motive, depth),
            scrut: abstract_at(target, replacement, scrut, depth),
            branches: branches
                .iter()
                .map(|b| Branch {
                    ctor: b.ctor.clone(),
                    binders: b.binders,
                    body: abstract_at(target, replacement, &b.body, depth + b.binders),
                    impossible: b.impossible,
                })
                .collect(),
        }),
        Tm::Eq(a, b, c) => eq(
            abstract_at(target, replacement, a, depth),
            abstract_at(target, replacement, b, depth),
            abstract_at(target, replacement, c, depth),
        ),
        Tm::Refl(a) => rc(Tm::Refl(abstract_at(target, replacement, a, depth))),
        Tm::J {
            ty,
            lhs,
            rhs,
            proof,
            motive,
            refl_case,
        } => rc(Tm::J {
            ty: abstract_at(target, replacement, ty, depth),
            lhs: abstract_at(target, replacement, lhs, depth),
            rhs: abstract_at(target, replacement, rhs, depth),
            proof: abstract_at(target, replacement, proof, depth),
            motive: abstract_at(target, replacement, motive, depth),
            refl_case: abstract_at(target, replacement, refl_case, depth),
        }),
    }
}

/// Structural equality of terms up to binder names. This is a sound
/// approximation of conversion used for neutral eliminators.
pub fn same_tm(a: &RcTm, b: &RcTm) -> bool {
    match (&**a, &**b) {
        (Tm::Var(i), Tm::Var(j)) => i == j,
        (Tm::Const(x), Tm::Const(y)) => x == y,
        (Tm::Type(u), Tm::Type(v)) => u == v,
        (Tm::Prop, Tm::Prop) => true,
        (Tm::PrimTy(a), Tm::PrimTy(b)) => a == b,
        (Tm::PrimLit(a), Tm::PrimLit(b)) => a == b,
        (Tm::PrimOp(o1, a1), Tm::PrimOp(o2, a2)) => {
            o1 == o2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| same_tm(x, y))
        }
        (
            Tm::Pi {
                dom: d1, cod: c1, ..
            },
            Tm::Pi {
                dom: d2, cod: c2, ..
            },
        ) => same_tm(d1, d2) && same_tm(c1, c2),
        (
            Tm::Lam {
                dom: d1, body: b1, ..
            },
            Tm::Lam {
                dom: d2, body: b2, ..
            },
        ) => same_tm(d1, d2) && same_tm(b1, b2),
        (Tm::App(f1, a1), Tm::App(f2, a2)) => same_tm(f1, f2) && same_tm(a1, a2),
        (Tm::Ind(n1, a1), Tm::Ind(n2, a2)) => {
            n1 == n2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| same_tm(x, y))
        }
        (Tm::Ctor(e1, i1, a1), Tm::Ctor(e2, i2, a2)) => {
            e1 == e2
                && i1 == i2
                && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|(x, y)| same_tm(x, y))
        }
        (
            Tm::Match {
                motive: m1,
                scrut: s1,
                branches: b1,
            },
            Tm::Match {
                motive: m2,
                scrut: s2,
                branches: b2,
            },
        ) => {
            same_tm(m1, m2)
                && same_tm(s1, s2)
                && b1.len() == b2.len()
                && b1.iter().zip(b2).all(|(x, y)| {
                    x.ctor == y.ctor
                        && x.binders == y.binders
                        && x.impossible == y.impossible
                        && same_tm(&x.body, &y.body)
                })
        }
        (Tm::Eq(a1, b1, c1), Tm::Eq(a2, b2, c2)) => {
            same_tm(a1, a2) && same_tm(b1, b2) && same_tm(c1, c2)
        }
        (Tm::Refl(a1), Tm::Refl(a2)) => same_tm(a1, a2),
        (
            Tm::J {
                ty: t1,
                lhs: l1,
                rhs: r1,
                proof: p1,
                motive: m1,
                refl_case: rc1,
            },
            Tm::J {
                ty: t2,
                lhs: l2,
                rhs: r2,
                proof: p2,
                motive: m2,
                refl_case: rc2,
            },
        ) => {
            same_tm(t1, t2)
                && same_tm(l1, l2)
                && same_tm(r1, r2)
                && same_tm(p1, p2)
                && same_tm(m1, m2)
                && same_tm(rc1, rc2)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn konst(s: &str) -> RcTm {
        rc(Tm::Const(s.to_string()))
    }

    #[test]
    fn abstract_term_avoids_capture() {
        // Bind `x` and use the outer `Var(1)`; the occurrence under the
        // binder shifts to `Var(2)` and the replacement shifts to `Var(1)`.
        let target = var(1);
        let replacement = var(0);
        let tm = lam("x", rc(Tm::Type(0)), app(var(0), var(2)));
        let got = abstract_term(&target, &replacement, &tm);
        let expected = lam("x", rc(Tm::Type(0)), app(var(0), var(1)));
        assert!(same_tm(&got, &expected));
    }

    #[test]
    fn abstract_term_replaces_under_binders() {
        let target = konst("t");
        let replacement = var(0);
        // `fn(x: Type 0) -> t` with `t` replaced by the outer `Var(0)`.
        let tm = pi("x", rc(Tm::Type(0)), target.clone());
        let got = abstract_term(&target, &replacement, &tm);
        let expected = pi("x", rc(Tm::Type(0)), var(1));
        assert!(same_tm(&got, &expected));
    }

    #[test]
    fn abstract_term_leaves_other_variables_alone() {
        let target = konst("c");
        let replacement = konst("c");
        let tm = lam("x", rc(Tm::Type(0)), app(var(0), konst("t")));
        assert!(same_tm(&abstract_term(&target, &replacement, &tm), &tm));
    }
}
