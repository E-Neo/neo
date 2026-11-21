use neo_common::error::Span;

#[derive(Clone, Debug)]
pub struct Program {
    pub items: Vec<Item>,
    /// Source files by `Span::file`, in loader order.
    pub files: Vec<String>,
}

/// A top-level or module-level item.
#[derive(Clone, Debug)]
pub enum Item {
    Mod(ModDecl),
    Use(UseDecl),
    Enum(EnumDecl),
    Extern(ExternDecl),
    Fn(FnDecl),
    Impl(ImplDecl),
}

#[derive(Clone, Debug)]
pub struct ModDecl {
    pub name: String,
    pub is_pub: bool,
    /// `None` for `mod name;`, which the file loader expands into items.
    pub items: Option<Vec<Item>>,
    pub span: Span,
}

/// `use geometry::Shape::{Circle, Square} as S`.
#[derive(Clone, Debug)]
pub struct UseDecl {
    pub is_pub: bool,
    pub tree: UseTree,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum UseTree {
    Name {
        segments: Vec<String>,
        alias: Option<String>,
        span: Span,
    },
    Glob {
        segments: Vec<String>,
        span: Span,
    },
    Group {
        prefix: Vec<String>,
        items: Vec<UseTree>,
        span: Span,
    },
}

impl UseTree {
    pub fn span(&self) -> Span {
        match self {
            UseTree::Name { span, .. }
            | UseTree::Glob { span, .. }
            | UseTree::Group { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub is_pub: bool,
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
    /// The canonical type name for a method lowered from an `impl` block.
    pub namespace: Option<String>,
    /// The canonical module path for a free function, empty at the root.
    pub module: String,
    pub name: String,
    pub is_pub: bool,
    pub params: Vec<Param>,
    pub ret: Expr,
    pub body: Block,
    pub span: Span,
}

/// An `impl[T: Type] List[T] { ... }` block.
#[derive(Clone, Debug)]
pub struct ImplDecl {
    pub params: Vec<Param>,
    pub target: Expr,
    pub methods: Vec<FnDecl>,
    pub span: Span,
}

/// A host capability declaration:
/// `pub extern fn write(s: String): IO[Result[Unit, IoError]] = "wasi:..."`.
#[derive(Clone, Debug)]
pub struct ExternDecl {
    /// The canonical module path containing the declaration.
    pub module: String,
    pub name: String,
    pub is_pub: bool,
    pub params: Vec<Param>,
    pub ret: Expr,
    pub import: String,
    pub span: Span,
}

impl ExternDecl {
    pub fn full_name(&self) -> String {
        if self.module.is_empty() {
            self.name.clone()
        } else {
            format!("{}::{}", self.module, self.name)
        }
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
    /// A `::`-separated path such as `geometry::Shape`.
    Path(Vec<String>, Span),
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
        /// The module where the method is used; the resolver fills this in.
        site: String,
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
            | Expr::Path(_, s)
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
    /// A constructor pattern. The name is a canonical constructor key after
    /// resolution; before resolution it may be a surface path.
    Ctor(String, Vec<Pattern>, Span),
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Wild(s) | Pattern::Var(_, s) | Pattern::Ctor(_, _, s) => *s,
        }
    }
}
