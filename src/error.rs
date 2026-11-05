use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Clone, Debug)]
pub struct Error {
    pub message: String,
    pub span: Option<Span>,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
        }
    }

    pub fn at(span: Span, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: Some(span),
        }
    }

    pub fn with_span(mut self, span: Span) -> Self {
        if self.span.is_none() {
            self.span = Some(span);
        }
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.span {
            Some(s) => write!(f, "{}:{}: {}", s.line, s.col, self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
