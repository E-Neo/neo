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
                Tok::Fn => Decl::Fn(self.parse_fn()?),
                _ => {
                    return Err(Error::at(
                        self.peek().span,
                        "expected an `enum` or `fn` declaration",
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
        let params = if self.at(&Tok::LParen) {
            self.parse_params()?
        } else {
            Vec::new()
        };
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
        let name = self.expect_ident("expected function name")?;
        let params = self.parse_params()?;
        self.expect(&Tok::Colon, "expected `:` after function parameters")?;
        self.skip_newlines();
        let ret = self.parse_expr()?;
        self.skip_newlines();
        let body = self.parse_block()?;
        Ok(FnDecl {
            name,
            params,
            ret,
            body,
            span,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>> {
        self.expect(&Tok::LParen, "expected `(`")?;
        let mut params = Vec::new();
        if self.at(&Tok::RParen) {
            self.advance();
            return Ok(params);
        }
        loop {
            let span = self.peek().span;
            let name = self.expect_ident("expected parameter name")?;
            self.expect(&Tok::Colon, "expected `:` after parameter name")?;
            self.skip_newlines();
            let ty = self.parse_expr()?;
            params.push(Param { name, ty, span });
            if self.at(&Tok::Comma) {
                self.advance();
                self.skip_newlines();
                continue;
            }
            self.expect(&Tok::RParen, "expected `)` after parameters")?;
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
            let is_binding =
                matches!(self.peek().tok, Tok::Ident(_)) && matches!(self.peek2().tok, Tok::Eq);
            if is_binding {
                let bspan = self.peek().span;
                let name = self.expect_ident("expected binding name")?;
                self.expect(&Tok::Eq, "expected `=`")?;
                self.skip_newlines();
                let value = self.parse_expr()?;
                bindings.push(Binding {
                    name,
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
        self.parse_eq()
    }

    fn parse_fn_expr(&mut self) -> Result<Expr> {
        let span = self.peek().span;
        self.expect(&Tok::Fn, "expected `fn`")?;
        self.expect(&Tok::LParen, "expected `(` after `fn`")?;
        let mut binders = Vec::new();
        if self.at(&Tok::RParen) {
            self.advance();
        } else {
            loop {
                let bspan = self.peek().span;
                let named = matches!(self.peek().tok, Tok::Ident(_))
                    && matches!(self.peek2().tok, Tok::Colon);
                if named {
                    let name = self.expect_ident("expected binder name")?;
                    self.expect(&Tok::Colon, "expected `:` after binder name")?;
                    self.skip_newlines();
                    let ty = self.parse_expr()?;
                    binders.push(Binder {
                        name: Some(name),
                        ty,
                        span: bspan,
                    });
                } else {
                    let ty = self.parse_expr()?;
                    binders.push(Binder {
                        name: None,
                        ty,
                        span: bspan,
                    });
                }
                if self.at(&Tok::Comma) {
                    self.advance();
                    self.skip_newlines();
                    continue;
                }
                self.expect(&Tok::RParen, "expected `)` after binders")?;
                break;
            }
        }
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

    fn parse_eq(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_app()?;
        while self.at(&Tok::EqEq) {
            let span = self.peek().span;
            self.advance();
            self.skip_newlines();
            let rhs = self.parse_app()?;
            lhs = Expr::Eq(Box::new(lhs), Box::new(rhs), span);
        }
        Ok(lhs)
    }

    fn parse_app(&mut self) -> Result<Expr> {
        let mut f = self.parse_atom()?;
        while self.at(&Tok::LParen) {
            let span = self.peek().span;
            self.advance();
            let mut args = Vec::new();
            if self.at(&Tok::RParen) {
                self.advance();
            } else {
                loop {
                    args.push(self.parse_expr()?);
                    if self.at(&Tok::Comma) {
                        self.advance();
                        self.skip_newlines();
                        continue;
                    }
                    self.expect(&Tok::RParen, "expected `)` after arguments")?;
                    break;
                }
            }
            f = Expr::App(Box::new(f), args, span);
        }
        Ok(f)
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
            Tok::Nat(n) => {
                let span = self.peek().span;
                self.advance();
                Ok(Expr::Nat(n, span))
            }
            Tok::Str(s) => {
                let span = self.peek().span;
                self.advance();
                Ok(Expr::Str(s, span))
            }
            Tok::Type => {
                let span = self.peek().span;
                self.advance();
                let level = if let Tok::Nat(n) = self.peek().tok {
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
                self.expect(&Tok::RParen, "expected `)`")?;
                Ok(Expr::Paren(Box::new(e), span))
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
