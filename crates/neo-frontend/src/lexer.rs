use crate::surface::NumSuffix;
use neo_common::error::{Error, Result, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    Numeral(u64, Option<NumSuffix>),
    Str(Vec<u8>),
    Bytes(Vec<u8>),
    Enum,
    Extern,
    Fn,
    Let,
    Match,
    Type,
    Prop,
    EqEq,
    Arrow,
    FatArrow,
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    Pipe,
    Caret,
    Shl,
    Shr,
    Lt,
    Le,
    Gt,
    Ge,
    Ne,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Dot,
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

        if c == '"' || (c == 'b' && i + 1 < chars.len() && chars[i + 1] == '"') {
            let kind = if c == 'b' {
                advance(&mut i, &mut line, &mut col);
                QuoteKind::Bytes
            } else {
                QuoteKind::Str
            };
            advance(&mut i, &mut line, &mut col);
            let bytes = lex_quoted(&chars, &mut i, &mut line, &mut col, span, kind)?;
            let tok = match kind {
                QuoteKind::Str => Tok::Str(bytes),
                QuoteKind::Bytes => Tok::Bytes(bytes),
            };
            out.push(Token { tok, span });
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
                "extern" => Tok::Extern,
                "fn" => Tok::Fn,
                "let" => Tok::Let,
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
                    tok: Tok::Numeral(n as u64, Some(NumSuffix::U32)),
                    span,
                });
                continue;
            }

            let n = text
                .parse::<u64>()
                .map_err(|_| Error::at(span, "numeric literal is too large"))?;
            out.push(Token {
                tok: Tok::Numeral(n, None),
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
            '.' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::Dot,
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
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token {
                        tok: Tok::Minus,
                        span,
                    });
                }
            }
            '+' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::Plus,
                    span,
                });
            }
            '*' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::Star,
                    span,
                });
            }
            '/' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::Slash,
                    span,
                });
            }
            '%' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::Percent,
                    span,
                });
            }
            '&' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::Amp,
                    span,
                });
            }
            '|' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::Pipe,
                    span,
                });
            }
            '^' => {
                advance(&mut i, &mut line, &mut col);
                out.push(Token {
                    tok: Tok::Caret,
                    span,
                });
            }
            '<' => {
                if i + 1 < chars.len() && chars[i + 1] == '<' {
                    advance(&mut i, &mut line, &mut col);
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token {
                        tok: Tok::Shl,
                        span,
                    });
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    advance(&mut i, &mut line, &mut col);
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token { tok: Tok::Le, span });
                } else {
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token { tok: Tok::Lt, span });
                }
            }
            '>' => {
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    advance(&mut i, &mut line, &mut col);
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token {
                        tok: Tok::Shr,
                        span,
                    });
                } else if i + 1 < chars.len() && chars[i + 1] == '=' {
                    advance(&mut i, &mut line, &mut col);
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token { tok: Tok::Ge, span });
                } else {
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token { tok: Tok::Gt, span });
                }
            }
            '!' => {
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    advance(&mut i, &mut line, &mut col);
                    advance(&mut i, &mut line, &mut col);
                    out.push(Token { tok: Tok::Ne, span });
                } else {
                    return Err(Error::at(span, "unexpected character '!'"));
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum QuoteKind {
    Str,
    Bytes,
}

/// Lex the body of a quoted literal. The opening quote has already been
/// consumed. `Str` content is UTF-8; `Bytes` content is raw.
fn lex_quoted(
    chars: &[char],
    i: &mut usize,
    line: &mut usize,
    col: &mut usize,
    span: Span,
    kind: QuoteKind,
) -> Result<Vec<u8>> {
    let advance = |i: &mut usize, line: &mut usize, col: &mut usize| {
        if chars[*i] == '\n' {
            *line += 1;
            *col = 1;
        } else {
            *col += 1;
        }
        *i += 1;
    };
    let mut out = Vec::new();
    loop {
        if *i >= chars.len() {
            return Err(Error::at(span, "unterminated string literal"));
        }
        let ch = chars[*i];
        if ch == '"' {
            advance(i, line, col);
            break;
        }
        if ch == '\n' {
            return Err(Error::at(
                Span::new(*line, *col),
                "string literals cannot contain a newline",
            ));
        }
        if ch == '\\' {
            advance(i, line, col);
            if *i >= chars.len() {
                return Err(Error::at(span, "unterminated string literal"));
            }
            let escaped = chars[*i];
            match escaped {
                'n' => out.push(b'\n'),
                't' => out.push(b'\t'),
                'r' => out.push(b'\r'),
                '\\' => out.push(b'\\'),
                '"' => out.push(b'"'),
                'x' => {
                    advance(i, line, col);
                    let mut value = 0u8;
                    for _ in 0..2 {
                        let Some(digit) = chars.get(*i).and_then(|c| c.to_digit(16)) else {
                            return Err(Error::at(
                                Span::new(*line, *col),
                                "expected two hex digits after `\\x`",
                            ));
                        };
                        value = value * 16 + digit as u8;
                        advance(i, line, col);
                    }
                    if kind == QuoteKind::Str && value > 0x7f {
                        return Err(Error::at(
                            Span::new(*line, *col),
                            "`\\x` above `7f` is not valid UTF-8; use a bytes literal",
                        ));
                    }
                    out.push(value);
                    continue;
                }
                other => {
                    return Err(Error::at(
                        Span::new(*line, *col),
                        format!("unknown string escape `\\{}`", other),
                    ));
                }
            }
            advance(i, line, col);
            continue;
        }
        if kind == QuoteKind::Bytes && !ch.is_ascii() {
            return Err(Error::at(
                Span::new(*line, *col),
                "bytes literals must contain only ASCII characters; use `\\x` escapes",
            ));
        }
        let mut buf = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        advance(i, line, col);
    }
    Ok(out)
}
