//! Source positions and parse errors.

/// Byte range into the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// The largest character boundary of `text` that is at most `at` (and at most its length). A span's
/// end can fall inside a multi-byte character (a name ends in a letter that takes two bytes, and
/// callers look at the last byte of it), and slicing `text` there panics.
pub fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

impl ParseError {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    /// 1-based line and column of the start of the error.
    pub fn line_col(&self, src: &str) -> (usize, usize) {
        let before = &src[..floor_boundary(src, self.span.start)];
        let line = before.matches('\n').count() + 1;
        let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        (line, col)
    }

    /// A multi-line message with the offending source line underlined.
    pub fn render(&self, file: &str, src: &str) -> String {
        let (line, col) = self.line_col(src);
        let text = src.lines().nth(line - 1).unwrap_or("");
        let width = src
            .get(self.span.start..self.span.end.min(src.len()))
            .map_or(1, |s| s.lines().next().map_or(1, |l| l.chars().count()))
            .max(1);
        let gutter = line.to_string().len();
        format!(
            "error: {msg}\n{pad} --> {file}:{line}:{col}\n{pad} |\n{line} | {text}\n{pad} | {indent}{marks}\n",
            msg = self.message,
            pad = " ".repeat(gutter),
            indent = " ".repeat(col - 1),
            marks = "^".repeat(width),
        )
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (at byte {})", self.message, self.span.start)
    }
}

impl std::error::Error for ParseError {}
