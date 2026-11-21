use neo_common::error::Span;

#[derive(Clone, Debug)]
pub struct Program {
    pub decls: Vec<Decl>,
}

#[derive(Clone, Debug)]
pub enum Decl {
    Enum(EnumDecl),
    Extern(ExternDecl),
    Fn(FnDecl),
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub sort: Option<Expr>,
    pub ctors: Vec<CtorDecl>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct CtorDecl {
    pub name: String,
    pub fields: Vec<Field>,
    pub result: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub name: Option<String>,
    pub ty: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    /// The type namespace for a method declaration such as `fn String.length`.
    pub namespace: Option<String>,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Expr,
    pub body: Block,
    pub span: Span,
}

/// A host capability declaration:
/// `extern fn stderr.write(s: String): IO[Result[Unit, IoError]] = "wasi:..."`.
/// The capability namespace is not a value; only saturated calls are terms.
#[derive(Clone, Debug)]
pub struct ExternDecl {
    pub namespace: String,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Expr,
    pub import: String,
    pub span: Span,
}

impl ExternDecl {
    pub fn full_name(&self) -> String {
        format!("{}.{}", self.namespace, self.name)
    }
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Expr,
    pub span: Span,
    pub implicit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumSuffix {
    U32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
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
}

impl BinOp {
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge | BinOp::Eq | BinOp::Ne
        )
    }

    pub fn is_equality(self) -> bool {
        matches!(self, BinOp::Eq | BinOp::Ne)
    }

    pub fn symbol(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::And => "&",
            BinOp::Or => "|",
            BinOp::Xor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Block {
    pub bindings: Vec<Binding>,
    pub tail: Box<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub name: String,
    pub ty: Option<Expr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Binder {
    pub name: Option<String>,
    pub ty: Expr,
    pub span: Span,
    pub implicit: bool,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Var(String, Span),
    Numeral(u64, Option<NumSuffix>, Span),
    Str(Vec<u8>, Span),
    Bytes(Vec<u8>, Span),
    Type(u32, Span),
    Prop(Span),
    FnType(Vec<Binder>, Box<Expr>, Span),
    FnLam(Vec<Binder>, Box<Expr>, Span),
    App {
        f: Box<Expr>,
        implicit: Vec<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Method {
        receiver: Box<Expr>,
        name: String,
        implicit: Vec<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    BinOp {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Match(Box<Expr>, Vec<Arm>, Span),
    Eq(Box<Expr>, Box<Expr>, Span),
    Refl(Span),
    Ascribe(Box<Expr>, Box<Expr>, Span),
    Block(Block),
    Paren(Box<Expr>, Span),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Var(_, s)
            | Expr::Numeral(_, _, s)
            | Expr::Str(_, s)
            | Expr::Bytes(_, s)
            | Expr::Type(_, s)
            | Expr::Prop(s)
            | Expr::FnType(_, _, s)
            | Expr::FnLam(_, _, s)
            | Expr::App { span: s, .. }
            | Expr::Method { span: s, .. }
            | Expr::BinOp { span: s, .. }
            | Expr::Match(_, _, s)
            | Expr::Eq(_, _, s)
            | Expr::Refl(s)
            | Expr::Ascribe(_, _, s)
            | Expr::Paren(_, s) => *s,
            Expr::Block(b) => b.span,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Arm {
    pub pattern: Pattern,
    pub body: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Pattern {
    Wild(Span),
    Var(String, Span),
    Ctor(String, Vec<Pattern>, Span),
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Wild(s) | Pattern::Var(_, s) | Pattern::Ctor(_, _, s) => *s,
        }
    }
}
