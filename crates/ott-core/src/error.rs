#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    #[must_use]
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Byte offset into the original UTF-8 source.
    pub start: usize,
    /// Byte offset into the original UTF-8 source.
    pub end: usize,
}

impl Span {
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone)]
pub struct OttError {
    pub message: String,
    pub span: Option<Span>,
    pub position: Option<Position>,
}

impl OttError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: None,
            position: None,
        }
    }

    #[must_use]
    pub fn with_span(mut self, span: Span, src: &str) -> Self {
        self.position = Some(position_from_offset(src, span.start));
        self.span = Some(span);
        self
    }

    #[must_use]
    pub fn with_position(mut self, pos: Position) -> Self {
        self.position = Some(pos);
        self
    }
}

impl std::fmt::Display for OttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(pos) = self.position {
            write!(
                f,
                "{msg} (line {line}, col {col})",
                msg = self.message,
                line = pos.line,
                col = pos.column
            )
        } else {
            write!(f, "{msg}", msg = self.message)
        }
    }
}

impl std::error::Error for OttError {}

pub type OttResult<T> = Result<T, OttError>;

#[must_use]
pub fn position_from_offset(src: &str, offset: usize) -> Position {
    let mut line = 1usize;
    let mut last_line_start = 0usize;

    for (i, b) in src.as_bytes().iter().enumerate() {
        if i >= offset {
            break;
        }
        if *b == b'\n' {
            line += 1;
            last_line_start = i + 1;
        }
    }

    // column is 1-indexed, measured in bytes for now (good enough for ASCII-heavy Ott input).
    let column = offset.saturating_sub(last_line_start) + 1;
    Position::new(line, column)
}
