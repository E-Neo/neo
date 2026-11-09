use neo_common::error::Span;

#[derive(Clone, Debug)]
pub struct Program {
    pub decls: Vec<Decl>,
}

#[derive(Clone, Debug)]
pub enum Decl {
    Enum(EnumDecl),
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
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Expr,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: Expr,
    pub span: Span,
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
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Binder {
    pub name: Option<String>,
    pub ty: Expr,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Var(String, Span),
    Nat(u64, Span),
    Type(u32, Span),
    Prop(Span),
    FnType(Vec<Binder>, Box<Expr>, Span),
    FnLam(Vec<Binder>, Box<Expr>, Span),
    App(Box<Expr>, Vec<Expr>, Span),
    Match(Box<Expr>, Vec<Arm>, Span),
    Eq(Box<Expr>, Box<Expr>, Span),
    Refl(Span),
    Block(Block),
    Paren(Box<Expr>, Span),
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Var(_, s)
            | Expr::Nat(_, s)
            | Expr::Type(_, s)
            | Expr::Prop(s)
            | Expr::FnType(_, _, s)
            | Expr::FnLam(_, _, s)
            | Expr::App(_, _, s)
            | Expr::Match(_, _, s)
            | Expr::Eq(_, _, s)
            | Expr::Refl(s)
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
