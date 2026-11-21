use crate::lexer::{Tok, Token};
use crate::surface::*;
use neo_common::error::{Error, Result};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

pub fn parse_source(src: &str) -> Result<Program> {
    let tokens = crate::lexer::lex(src)?;
    let mut p = Parser::new(tokens);
    p.parse_program()
}

pub fn parse_expr_source(src: &str) -> Result<Expr> {
    let tokens = crate::lexer::lex(src)?;
    let mut p = Parser::new(tokens);
    p.skip_newlines();
    let e = p.parse_expr()?;
    p.skip_newlines();
    if !p.at(&Tok::Eof) {
        return Err(Error::at(
            p.peek().span,
            "unexpected token after expression",
        ));
    }
    Ok(e)
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn parse_program(&mut self) -> Result<Program> {
        let mut decls = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::Eof) {
                break;
            }
            let decl = match &self.peek().tok {
                Tok::Enum => Decl::Enum(self.parse_enum()?),
                Tok::Extern => Decl::Extern(self.parse_extern()?),
                Tok::Fn => Decl::Fn(self.parse_fn()?),
                _ => {
                    return Err(Error::at(
                        self.peek().span,
                        "expected an `enum`, `extern fn`, or `fn` declaration",
                    ));
                }
            };
            decls.push(decl);
        }
        Ok(Program { decls })
    }

    fn parse_enum(&mut self) -> Result<EnumDecl> {
        let span = self.peek().span;
        self.expect(&Tok::Enum, "expected `enum`")?;
        let name = self.expect_ident("expected enum name")?;
        let params = if self.at(&Tok::LBracket) {
            self.parse_params(Tok::LBracket, Tok::RBracket, true)?
        } else {
            Vec::new()
        };
        if self.at(&Tok::LParen) {
            return Err(Error::at(
                self.peek().span,
                "enum parameters must be written in `[]`, not `()`",
            ));
        }
        let sort = if self.at(&Tok::Colon) {
            self.advance();
            self.skip_newlines();
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.skip_newlines();
        self.expect(&Tok::LBrace, "expected `{` after enum header")?;
        let mut ctors = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::RBrace) {
                self.advance();
                break;
            }
            if self.at(&Tok::Eof) {
                return Err(Error::at(span, "unterminated enum declaration"));
            }
            ctors.push(self.parse_ctor()?);
        }
        Ok(EnumDecl {
            name,
            params,
            sort,
            ctors,
            span,
        })
    }

    fn parse_ctor(&mut self) -> Result<CtorDecl> {
        let span = self.peek().span;
        let name = self.expect_ident("expected constructor name")?;
        let fields = if self.at(&Tok::LParen) {
            self.parse_fields()?
        } else {
            Vec::new()
        };
        let result = if self.at(&Tok::Colon) {
            self.advance();
            self.skip_newlines();
            Some(self.parse_expr()?)
        } else {
            None
        };
        if !self.at(&Tok::Newline) && !self.at(&Tok::RBrace) && !self.at(&Tok::Eof) {
            return Err(Error::at(
                self.peek().span,
                "expected a newline or `}` after constructor",
            ));
        }
        Ok(CtorDecl {
            name,
            fields,
            result,
            span,
        })
    }

    fn parse_fn(&mut self) -> Result<FnDecl> {
        let span = self.peek().span;
        self.expect(&Tok::Fn, "expected `fn`")?;
        let first = self.expect_ident("expected function name")?;
        let (namespace, name) = if self.at(&Tok::Dot) {
            self.advance();
            let method = self.expect_ident("expected a method name after `.`")?;
            (Some(first), method)
        } else {
            (None, first)
        };
        let mut params = Vec::new();
        if self.at(&Tok::LBracket) {
            params.extend(self.parse_params(Tok::LBracket, Tok::RBracket, true)?);
        }
        params.extend(self.parse_params(Tok::LParen, Tok::RParen, false)?);
        self.expect(&Tok::Colon, "expected `:` after function parameters")?;
        self.skip_newlines();
        let ret = self.parse_expr()?;
        self.skip_newlines();
        let body = self.parse_block()?;
        Ok(FnDecl {
            namespace,
            name,
            params,
            ret,
            body,
            span,
        })
    }

    /// `extern fn stderr.write(s: String): IO[T] = "import"`.
    fn parse_extern(&mut self) -> Result<ExternDecl> {
        let span = self.peek().span;
        self.expect(&Tok::Extern, "expected `extern`")?;
        self.expect(&Tok::Fn, "expected `fn` after `extern`")?;
        let namespace = self.expect_ident("expected a capability namespace")?;
        if !self.at(&Tok::Dot) {
            return Err(Error::at(
                self.peek().span,
                "an extern name needs a capability namespace, such as `stderr.write`",
            ));
        }
        self.advance();
        let name = self.expect_ident("expected a capability name after `.`")?;
        let mut params = Vec::new();
        if self.at(&Tok::LBracket) {
            params.extend(self.parse_params(Tok::LBracket, Tok::RBracket, true)?);
        }
        params.extend(self.parse_params(Tok::LParen, Tok::RParen, false)?);
        self.expect(&Tok::Colon, "expected `:` after extern parameters")?;
        self.skip_newlines();
        // The return type is a type, so stop before a top-level `=`. The `=`
        // token is otherwise the equality-type former.
        let ret = self.parse_cmp()?;
        self.skip_newlines();
        self.expect(&Tok::Eq, "expected `=` and an import string")?;
        self.skip_newlines();
        let import = match &self.peek().tok {
            Tok::Str(bytes) => {
                let text = std::str::from_utf8(bytes).map_err(|_| {
                    Error::at(self.peek().span, "the import string must be valid UTF-8")
                })?;
                text.to_string()
            }
            _ => {
                return Err(Error::at(
                    self.peek().span,
                    "expected an import string after `=`",
                ));
            }
        };
        self.advance();
        Ok(ExternDecl {
            namespace,
            name,
            params,
            ret,
            import,
            span,
        })
    }

    fn parse_params(&mut self, open: Tok, close: Tok, implicit: bool) -> Result<Vec<Param>> {
        self.expect(&open, "expected a parameter list")?;
        let mut params = Vec::new();
        if self.at(&close) {
            self.advance();
            if implicit && params.is_empty() {
                return Err(Error::at(
                    self.peek().span,
                    "an implicit parameter list cannot be empty",
                ));
            }
            return Ok(params);
        }
        loop {
            let span = self.peek().span;
            let name = self.expect_ident("expected parameter name")?;
            self.expect(&Tok::Colon, "expected `:` after parameter name")?;
            self.skip_newlines();
            let ty = self.parse_expr()?;
            params.push(Param {
                name,
                ty,
                span,
                implicit,
            });
            if self.at(&Tok::Comma) {
                self.advance();
                self.skip_newlines();
                continue;
            }
            self.expect(&close, "expected the end of the parameter list")?;
            break;
        }
        Ok(params)
    }

    fn parse_fields(&mut self) -> Result<Vec<Field>> {
        self.expect(&Tok::LParen, "expected `(`")?;
        let mut fields = Vec::new();
        if self.at(&Tok::RParen) {
            self.advance();
            return Ok(fields);
        }
        loop {
            let span = self.peek().span;
            let named =
                matches!(self.peek().tok, Tok::Ident(_)) && matches!(self.peek2().tok, Tok::Colon);
            if named {
                let name = self.expect_ident("expected field name")?;
                self.expect(&Tok::Colon, "expected `:` after field name")?;
                self.skip_newlines();
                let ty = self.parse_expr()?;
                fields.push(Field {
                    name: Some(name),
                    ty,
                    span,
                });
            } else {
                let ty = self.parse_expr()?;
                fields.push(Field {
                    name: None,
                    ty,
                    span,
                });
            }
            if self.at(&Tok::Comma) {
                self.advance();
                self.skip_newlines();
                continue;
            }
            self.expect(&Tok::RParen, "expected `)` after fields")?;
            break;
        }
        Ok(fields)
    }

    fn parse_block(&mut self) -> Result<Block> {
        let span = self.peek().span;
        self.expect(&Tok::LBrace, "expected `{`")?;
        let mut bindings = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::RBrace) {
                return Err(Error::at(
                    self.peek().span,
                    "a block must end in an expression",
                ));
            }
            if self.at(&Tok::Eof) {
                return Err(Error::at(span, "unterminated block"));
            }
            if self.at(&Tok::Let) {
                let bspan = self.peek().span;
                self.advance();
                let name = self.expect_ident("expected a binding name after `let`")?;
                let ty = if self.at(&Tok::Colon) {
                    self.advance();
                    self.skip_newlines();
                    Some(self.parse_annotation()?)
                } else {
                    None
                };
                self.expect(&Tok::Eq, "expected `=` after the binding pattern")?;
                self.skip_newlines();
                let value = self.parse_expr()?;
                bindings.push(Binding {
                    name,
                    ty,
                    value,
                    span: bspan,
                });
                if self.at(&Tok::Newline) {
                    self.advance();
                } else if self.at(&Tok::RBrace) {
                    return Err(Error::at(
                        self.peek().span,
                        "a block must end in an expression",
                    ));
                } else {
                    return Err(Error::at(
                        self.peek().span,
                        "expected a newline after a binding",
                    ));
                }
                continue;
            }

            let tail = self.parse_expr()?;
            self.skip_newlines();
            self.expect(&Tok::RBrace, "expected `}` after block")?;
            return Ok(Block {
                bindings,
                tail: Box::new(tail),
                span,
            });
        }
    }

    fn parse_expr(&mut self) -> Result<Expr> {
        if self.at(&Tok::Fn) {
            return self.parse_fn_expr();
        }
        self.parse_equality()
    }

    /// Types in `let` annotations are parsed below `=`, so the binding
    /// separator stays unambiguous. Function and equality types therefore need
    /// parentheses in annotations.
    fn parse_annotation(&mut self) -> Result<Expr> {
        if self.at(&Tok::Fn) {
            return Err(Error::at(
                self.peek().span,
                "parenthesize a function-type annotation",
            ));
        }
        self.parse_cmp()
    }

    fn parse_fn_expr(&mut self) -> Result<Expr> {
        let span = self.peek().span;
        self.expect(&Tok::Fn, "expected `fn`")?;
        let mut binders = Vec::new();
        if self.at(&Tok::LBracket) {
            binders.extend(self.parse_binders(Tok::LBracket, Tok::RBracket, true)?);
        }
        binders.extend(self.parse_binders(Tok::LParen, Tok::RParen, false)?);
        if self.at(&Tok::Arrow) {
            self.advance();
            self.skip_newlines();
            let body = self.parse_expr()?;
            Ok(Expr::FnType(binders, Box::new(body), span))
        } else if self.at(&Tok::FatArrow) {
            self.advance();
            self.skip_newlines();
            let body = self.parse_expr()?;
            Ok(Expr::FnLam(binders, Box::new(body), span))
        } else {
            Err(Error::at(
                self.peek().span,
                "expected `->` or `=>` after function binders",
            ))
        }
    }

    fn parse_binders(&mut self, open: Tok, close: Tok, implicit: bool) -> Result<Vec<Binder>> {
        self.expect(&open, "expected a binder list")?;
        let mut binders = Vec::new();
        if self.at(&close) {
            self.advance();
            if implicit {
                return Err(Error::at(
                    self.peek().span,
                    "an implicit binder list cannot be empty",
                ));
            }
            return Ok(binders);
        }
        loop {
            let bspan = self.peek().span;
            let named =
                matches!(self.peek().tok, Tok::Ident(_)) && matches!(self.peek2().tok, Tok::Colon);
            if named {
                let name = self.expect_ident("expected binder name")?;
                self.expect(&Tok::Colon, "expected `:` after binder name")?;
                self.skip_newlines();
                let ty = self.parse_expr()?;
                binders.push(Binder {
                    name: Some(name),
                    ty,
                    span: bspan,
                    implicit,
                });
            } else {
                let ty = self.parse_expr()?;
                binders.push(Binder {
                    name: None,
                    ty,
                    span: bspan,
                    implicit,
                });
            }
            if self.at(&Tok::Comma) {
                self.advance();
                self.skip_newlines();
                continue;
            }
            self.expect(&close, "expected the end of the binder list")?;
            break;
        }
        Ok(binders)
    }

    fn parse_equality(&mut self) -> Result<Expr> {
        let lhs = self.parse_cmp()?;
        if !self.at(&Tok::Eq) {
            return Ok(lhs);
        }
        let span = self.peek().span;
        self.advance();
        self.skip_newlines();
        let rhs = self.parse_cmp()?;
        if self.at(&Tok::Eq) {
            return Err(Error::at(
                self.peek().span,
                "equality is not associative; parenthesize",
            ));
        }
        Ok(Expr::Eq(Box::new(lhs), Box::new(rhs), span))
    }

    fn parse_cmp(&mut self) -> Result<Expr> {
        let lhs = self.parse_bitor()?;
        let op = match self.peek().tok {
            Tok::Lt => BinOp::Lt,
            Tok::Le => BinOp::Le,
            Tok::Gt => BinOp::Gt,
            Tok::Ge => BinOp::Ge,
            Tok::EqEq => BinOp::Eq,
            Tok::Ne => BinOp::Ne,
            _ => return Ok(lhs),
        };
        let span = self.peek().span;
        self.advance();
        self.skip_newlines();
        let rhs = self.parse_bitor()?;
        if matches!(
            self.peek().tok,
            Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge | Tok::EqEq | Tok::Ne
        ) {
            return Err(Error::at(
                self.peek().span,
                "comparison operators are not associative; parenthesize",
            ));
        }
        Ok(Expr::BinOp {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span,
        })
    }

    fn parse_bitor(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_bitxor()?;
        while self.at(&Tok::Pipe) {
            let span = self.peek().span;
            self.advance();
            self.skip_newlines();
            let rhs = self.parse_bitxor()?;
            lhs = Expr::BinOp {
                op: BinOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_bitxor(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_bitand()?;
        while self.at(&Tok::Caret) {
            let span = self.peek().span;
            self.advance();
            self.skip_newlines();
            let rhs = self.parse_bitand()?;
            lhs = Expr::BinOp {
                op: BinOp::Xor,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_bitand(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_shift()?;
        while self.at(&Tok::Amp) {
            let span = self.peek().span;
            self.advance();
            self.skip_newlines();
            let rhs = self.parse_shift()?;
            lhs = Expr::BinOp {
                op: BinOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_shift(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.peek().tok {
                Tok::Shl => BinOp::Shl,
                Tok::Shr => BinOp::Shr,
                _ => return Ok(lhs),
            };
            let span = self.peek().span;
            self.advance();
            self.skip_newlines();
            let rhs = self.parse_add()?;
            lhs = Expr::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
    }

    fn parse_add(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek().tok {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => return Ok(lhs),
            };
            let span = self.peek().span;
            self.advance();
            self.skip_newlines();
            let rhs = self.parse_mul()?;
            lhs = Expr::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
    }

    fn parse_mul(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_app()?;
        loop {
            let op = match self.peek().tok {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Rem,
                _ => return Ok(lhs),
            };
            let span = self.peek().span;
            self.advance();
            self.skip_newlines();
            let rhs = self.parse_app()?;
            lhs = Expr::BinOp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
    }

    fn parse_app(&mut self) -> Result<Expr> {
        let mut f = self.parse_atom()?;
        loop {
            if self.at(&Tok::LBracket) || self.at(&Tok::LParen) {
                let span = self.peek().span;
                if self.at(&Tok::LBracket) {
                    let implicit = self.parse_args(Tok::LBracket, Tok::RBracket, true)?;
                    let args = if self.at(&Tok::LParen) {
                        self.parse_args(Tok::LParen, Tok::RParen, false)?
                    } else {
                        Vec::new()
                    };
                    f = Expr::App {
                        f: Box::new(f),
                        implicit,
                        args,
                        span,
                    };
                } else {
                    let args = self.parse_args(Tok::LParen, Tok::RParen, false)?;
                    f = Expr::App {
                        f: Box::new(f),
                        implicit: Vec::new(),
                        args,
                        span,
                    };
                }
                continue;
            }
            if self.at(&Tok::Dot) {
                let span = self.peek().span;
                self.advance();
                let name = self.expect_ident("expected a method name after `.`")?;
                let implicit = if self.at(&Tok::LBracket) {
                    self.parse_args(Tok::LBracket, Tok::RBracket, true)?
                } else {
                    Vec::new()
                };
                if !self.at(&Tok::LParen) {
                    return Err(Error::at(
                        self.peek().span,
                        "a method call needs `(...)` arguments",
                    ));
                }
                let args = self.parse_args(Tok::LParen, Tok::RParen, false)?;
                f = Expr::Method {
                    receiver: Box::new(f),
                    name,
                    implicit,
                    args,
                    span,
                };
                continue;
            }
            break;
        }
        Ok(f)
    }

    fn parse_args(&mut self, open: Tok, close: Tok, implicit: bool) -> Result<Vec<Expr>> {
        self.expect(&open, "expected an argument list")?;
        let mut args = Vec::new();
        if self.at(&close) {
            self.advance();
            if implicit {
                return Err(Error::at(
                    self.peek().span,
                    "an implicit argument list cannot be empty",
                ));
            }
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr()?);
            if self.at(&Tok::Comma) {
                self.advance();
                self.skip_newlines();
                continue;
            }
            self.expect(&close, "expected the end of the argument list")?;
            break;
        }
        Ok(args)
    }

    fn parse_atom(&mut self) -> Result<Expr> {
        match self.peek().tok.clone() {
            Tok::Ident(name) => {
                let span = self.peek().span;
                self.advance();
                if name == "refl" {
                    Ok(Expr::Refl(span))
                } else {
                    Ok(Expr::Var(name, span))
                }
            }
            Tok::Numeral(n, suffix) => {
                let span = self.peek().span;
                self.advance();
                Ok(Expr::Numeral(n, suffix, span))
            }
            Tok::Str(s) => {
                let span = self.peek().span;
                self.advance();
                Ok(Expr::Str(s, span))
            }
            Tok::Bytes(b) => {
                let span = self.peek().span;
                self.advance();
                Ok(Expr::Bytes(b, span))
            }
            Tok::Type => {
                let span = self.peek().span;
                self.advance();
                let level = if let Tok::Numeral(n, suffix) = self.peek().tok {
                    if suffix.is_some() {
                        return Err(Error::at(
                            self.peek().span,
                            "a universe level cannot have a suffix",
                        ));
                    }
                    self.advance();
                    n as u32
                } else {
                    0
                };
                Ok(Expr::Type(level, span))
            }
            Tok::Prop => {
                let span = self.peek().span;
                self.advance();
                Ok(Expr::Prop(span))
            }
            Tok::LParen => {
                let span = self.peek().span;
                self.advance();
                let e = self.parse_expr()?;
                if self.at(&Tok::Colon) {
                    self.advance();
                    self.skip_newlines();
                    let ty = self.parse_expr()?;
                    self.expect(&Tok::RParen, "expected `)` after ascription")?;
                    Ok(Expr::Ascribe(Box::new(e), Box::new(ty), span))
                } else {
                    self.expect(&Tok::RParen, "expected `)`")?;
                    Ok(Expr::Paren(Box::new(e), span))
                }
            }
            Tok::LBrace => {
                let b = self.parse_block()?;
                Ok(Expr::Block(b))
            }
            Tok::Match => self.parse_match(),
            Tok::Fn => self.parse_fn_expr(),
            _ => Err(Error::at(self.peek().span, "expected an expression")),
        }
    }

    fn parse_match(&mut self) -> Result<Expr> {
        let span = self.peek().span;
        self.expect(&Tok::Match, "expected `match`")?;
        let scrut = self.parse_expr()?;
        self.skip_newlines();
        self.expect(&Tok::LBrace, "expected `{` after match scrutinee")?;
        let mut arms = Vec::new();
        loop {
            self.skip_newlines();
            if self.at(&Tok::RBrace) {
                self.advance();
                break;
            }
            if self.at(&Tok::Eof) {
                return Err(Error::at(span, "unterminated match"));
            }
            let aspan = self.peek().span;
            let pattern = self.parse_pattern()?;
            self.expect(&Tok::FatArrow, "expected `=>` after pattern")?;
            self.skip_newlines();
            let body = self.parse_expr()?;
            arms.push(Arm {
                pattern,
                body,
                span: aspan,
            });
            if self.at(&Tok::Newline) {
                self.advance();
            } else if self.at(&Tok::RBrace) {
                self.advance();
                break;
            } else {
                return Err(Error::at(
                    self.peek().span,
                    "expected a newline after match arm",
                ));
            }
        }
        Ok(Expr::Match(Box::new(scrut), arms, span))
    }

    fn parse_pattern(&mut self) -> Result<Pattern> {
        let span = self.peek().span;
        match self.peek().tok.clone() {
            Tok::Underscore => {
                self.advance();
                Ok(Pattern::Wild(span))
            }
            Tok::Ident(name) => {
                self.advance();
                if self.at(&Tok::LParen) {
                    self.advance();
                    let mut pats = Vec::new();
                    if self.at(&Tok::RParen) {
                        self.advance();
                    } else {
                        loop {
                            pats.push(self.parse_pattern()?);
                            if self.at(&Tok::Comma) {
                                self.advance();
                                continue;
                            }
                            self.expect(&Tok::RParen, "expected `)` after pattern")?;
                            break;
                        }
                    }
                    Ok(Pattern::Ctor(name, pats, span))
                } else {
                    Ok(Pattern::Var(name, span))
                }
            }
            _ => Err(Error::at(self.peek().span, "expected a pattern")),
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek2(&self) -> &Token {
        &self.tokens[(self.pos + 1).min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) {
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn at(&self, t: &Tok) -> bool {
        &self.peek().tok == t
    }

    fn skip_newlines(&mut self) {
        while self.at(&Tok::Newline) {
            self.advance();
        }
    }

    fn expect(&mut self, t: &Tok, msg: &str) -> Result<()> {
        if self.at(t) {
            self.advance();
            Ok(())
        } else {
            Err(Error::at(self.peek().span, msg))
        }
    }

    fn expect_ident(&mut self, msg: &str) -> Result<String> {
        match self.peek().tok.clone() {
            Tok::Ident(s) => {
                self.advance();
                Ok(s)
            }
            _ => Err(Error::at(self.peek().span, msg)),
        }
    }
}
