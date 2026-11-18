use neo_common::error::{Error, Result, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Nat(u64),
    U32(u32),
    Str(String),
    Enum,
    Fn,
    Match,
    Type,
    Prop,
    EqEq,
    Arrow,
    FatArrow,
    Eq,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Underscore,
    Newline,
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

pub fn lex(src: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut out = Vec::new();

    let advance = |i: &mut usize, line: &mut usize, col: &mut usize| {
        if chars[*i] == '\n' {
            *line += 1;
            *col = 1;
        } else {
            *col += 1;
        }
        *i += 1;
    };

    while i < chars.len() {
        let c = chars[i];
        let span = Span::new(line, col);

        if c == ' ' || c == '\t' || c == '\r' {
            advance(&mut i, &mut line, &mut col);
            continue;
        }

        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                advance(&mut i, &mut line, &mut col);
            }
            continue;
        }

        if c == '\n' {
            advance(&mut i, &mut line, &mut col);
            if paren_depth == 0 && bracket_depth == 0 {
                out.push(Token {
                    tok: Tok::Newline,
                    span,
                });
            }
            continue;
        }

        if c == '"' {
            advance(&mut i, &mut line, &mut col);
            let mut text = String::new();
            loop {
                if i >= chars.len() {
                    return Err(Error::at(span, "unterminated string literal"));
                }
                let ch = chars[i];
                if ch == '"' {
                    advance(&mut i, &mut line, &mut col);
                    break;
                }
                if ch == '\n' {
                    return Err(Error::at(
                        Span::new(line, col),
                        "string literals cannot contain a newline",
                    ));
                }
                if ch == '\\' {
                    advance(&mut i, &mut line, &mut col);
                    if i >= chars.len() {
                        return Err(Error::at(span, "unterminated string literal"));
                    }
                    let escaped = chars[i];
                    let decoded = match escaped {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '\\' => '\\',
                        '"' => '"',
                        other => {
                            return Err(Error::at(
                                Span::new(line, col),
                                format!("unknown string escape `\\{}`", other),
                            ));
                        }
                    };
                    text.push(decoded);
                    advance(&mut i, &mut line, &mut col);
                    continue;
                }
                if !ch.is_ascii() {
                    return Err(Error::at(
                        Span::new(line, col),
                        "string literals must contain only ASCII characters",
                    ));
                }
                text.push(ch);
                advance(&mut i, &mut line, &mut col);
            }
            out.push(Token {
                tok: Tok::Str(text),
                span,
            });
            continue;
        }

        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '\'')
            {
                advance(&mut i, &mut line, &mut col);
            }
            let text: String = chars[start..i].iter().collect();
            let tok = match text.as_str() {
                "_" => Tok::Underscore,
                "enum" => Tok::Enum,
                "fn" => Tok::Fn,
                "match" => Tok::Match,
                "Type" => Tok::Type,
                "Prop" => Tok::Prop,
                _ => Tok::Ident(text),
            };
            out.push(Token { tok, span });
            continue;
        }

        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                advance(&mut i, &mut line, &mut col);
            }
            let text: String = chars[start..i].iter().collect();

            // A `u32` suffix selects the primitive word type. It must not run
            // into another identifier character, so `1u32x` is a numeral
            // followed by an identifier.
            let suffix_end = i + 3;
            let has_suffix = suffix_end <= chars.len()
                && chars[i..suffix_end] == ['u', '3', '2']
                && (suffix_end == chars.len()
                    || !(chars[suffix_end].is_ascii_alphanumeric()
                        || chars[suffix_end] == '_'
                        || chars[suffix_end] == '\''));
            if has_suffix {
                let n = text
                    .parse::<u32>()
                    .map_err(|_| Error::at(span, "u32 literal is too large"))?;
                for _ in 0..3 {
                    advance(&mut i, &mut line, &mut col);
                }
                out.push(Token {
                    tok: Tok::U32(n),
                    span,
                });
                continue;
            }

            let n = text
                .parse::<u64>()
                .map_err(|_| Error::at(span, "numeric literal is too large"))?;
            out.push(Token {
                tok: Tok::Nat(n),
                span,
            });
            continue;
        }

        match c {
            '(' => {
                paren_depth += 1;
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::LParen,
                    span,
                });
            }
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::RParen,
                    span,
                });
            }
            '[' => {
                bracket_depth += 1;
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::LBracket,
                    span,
                });
            }
            ']' => {
                bracket_depth = bracket_depth.saturating_sub(1);
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::RBracket,
                    span,
                });
            }
            '{' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::LBrace,
                    span,
                });
            }
            '}' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::RBrace,
                    span,
                });
            }
            ',' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::Comma,
                    span,
                });
            }
            ':' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::Colon,
                    span,
                });
            }
            '=' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    advance(&mut i, &mut line, &mut col);
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token {
                        tok: Tok::EqEq,
                        span,
                    });
                } else if i + 1 < chars.len() && chars[i + 1] == '>' {
                    advance(&mut i, &mut line, &mut col);
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token {
                        tok: Tok::FatArrow,
                        span,
                    });
                } else {
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token { tok: Tok::Eq, span });
                }
            }
            '-' => {
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    advance(&mut i, &mut line, &mut col);
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token {
                        tok: Tok::Arrow,
                        span,
                    });
                } else {
                    return Err(Error::at(span, "unexpected character '-'"));
                }
            }
            _ => {
                return Err(Error::at(span, format!("unexpected character '{}'", c)));
            }
        }
    }

    out.push(Token {
        tok: Tok::Eof,
        span: Span::new(line, col),
    });
    Ok(out)
}
