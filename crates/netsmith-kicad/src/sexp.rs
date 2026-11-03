//! Minimal S-expression reader and KiCad-style writer.
//!
//! The writer reproduces KiCad's layout (tabs, one nested list per line) so that
//! generated files diff cleanly against files KiCad itself saved.

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Sexp {
    /// Bare token: keyword, number, `yes`/`no`.
    Atom(String),
    /// Double-quoted string.
    Str(String),
    List(Vec<Sexp>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    UnexpectedEof,
    UnexpectedClose(usize),
    UnterminatedString(usize),
    TrailingInput(usize),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedEof => write!(f, "unexpected end of input"),
            Self::UnexpectedClose(at) => write!(f, "unexpected ')' at byte {at}"),
            Self::UnterminatedString(at) => write!(f, "unterminated string at byte {at}"),
            Self::TrailingInput(at) => write!(f, "trailing input at byte {at}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl Sexp {
    pub fn atom(s: impl Into<String>) -> Self {
        Self::Atom(s.into())
    }

    pub fn string(s: impl Into<String>) -> Self {
        Self::Str(s.into())
    }

    pub fn num(n: f64) -> Self {
        Self::Atom(fmt_num(n))
    }

    pub fn yes_no(b: bool) -> Self {
        Self::atom(if b { "yes" } else { "no" })
    }

    /// `(name item...)`
    pub fn node(name: &str, items: Vec<Sexp>) -> Self {
        let mut all = Vec::with_capacity(items.len() + 1);
        all.push(Self::atom(name));
        all.extend(items);
        Self::List(all)
    }

    /// The leading atom of a list, e.g. `symbol` in `(symbol "R" ...)`.
    pub fn head(&self) -> Option<&str> {
        match self {
            Self::List(items) => match items.first() {
                Some(Self::Atom(a)) => Some(a),
                _ => None,
            },
            _ => None,
        }
    }

    /// Items after the head.
    pub fn args(&self) -> &[Sexp] {
        match self {
            Self::List(items) if !items.is_empty() => &items[1..],
            _ => &[],
        }
    }

    /// First child list whose head is `name`.
    pub fn find(&self, name: &str) -> Option<&Sexp> {
        self.args().iter().find(|c| c.head() == Some(name))
    }

    pub fn find_all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Sexp> {
        self.args().iter().filter(move |c| c.head() == Some(name))
    }

    /// Text of an atom or string.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Atom(s) | Self::Str(s) => Some(s),
            Self::List(_) => None,
        }
    }

    pub fn parse(input: &str) -> Result<Sexp, ParseError> {
        let mut p = Parser {
            src: input.as_bytes(),
            pos: 0,
        };
        p.skip_ws();
        let value = p.value()?;
        p.skip_ws();
        if p.pos != p.src.len() {
            return Err(ParseError::TrailingInput(p.pos));
        }
        Ok(value)
    }

    /// Render in KiCad's style, ending with a newline.
    pub fn to_kicad_string(&self) -> String {
        let mut out = String::new();
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        match self {
            Self::Atom(a) => out.push_str(a),
            Self::Str(s) => write_quoted(out, s),
            Self::List(items) => {
                out.push('(');
                // Leading scalars stay on the opening line.
                let lead = items
                    .iter()
                    .take_while(|i| !matches!(i, Self::List(_)))
                    .count();
                for (i, item) in items[..lead].iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    item.write(out, depth);
                }
                let rest = &items[lead..];
                if rest.is_empty() {
                    out.push(')');
                    return;
                }
                // KiCad keeps the points of a polyline on one line.
                if matches!(items.first(), Some(Self::Atom(a)) if a == "pts")
                    && rest.iter().all(|i| i.head() == Some("xy"))
                {
                    out.push('\n');
                    for _ in 0..=depth {
                        out.push('\t');
                    }
                    for (i, point) in rest.iter().enumerate() {
                        if i > 0 {
                            out.push(' ');
                        }
                        point.write(out, depth + 1);
                    }
                    out.push('\n');
                    for _ in 0..depth {
                        out.push('\t');
                    }
                    out.push(')');
                    return;
                }
                for item in rest {
                    out.push('\n');
                    for _ in 0..=depth {
                        out.push('\t');
                    }
                    item.write(out, depth + 1);
                }
                out.push('\n');
                for _ in 0..depth {
                    out.push('\t');
                }
                out.push(')');
            }
        }
    }
}

fn write_quoted(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Format a coordinate or size: at most four decimals, no trailing zeros, never `-0`.
pub fn fmt_num(n: f64) -> String {
    let mut s = String::new();
    write!(s, "{n:.4}").expect("writing to a String cannot fail");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".to_string() } else { s }
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn skip_ws(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn value(&mut self) -> Result<Sexp, ParseError> {
        match self.src.get(self.pos) {
            None => Err(ParseError::UnexpectedEof),
            Some(b'(') => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_ws();
                    match self.src.get(self.pos) {
                        None => return Err(ParseError::UnexpectedEof),
                        Some(b')') => {
                            self.pos += 1;
                            return Ok(Sexp::List(items));
                        }
                        Some(_) => items.push(self.value()?),
                    }
                }
            }
            Some(b')') => Err(ParseError::UnexpectedClose(self.pos)),
            Some(b'"') => self.string(),
            Some(_) => {
                let start = self.pos;
                while self.pos < self.src.len()
                    && !self.src[self.pos].is_ascii_whitespace()
                    && !matches!(self.src[self.pos], b'(' | b')' | b'"')
                {
                    self.pos += 1;
                }
                Ok(Sexp::Atom(
                    String::from_utf8_lossy(&self.src[start..self.pos]).into_owned(),
                ))
            }
        }
    }

    fn string(&mut self) -> Result<Sexp, ParseError> {
        let start = self.pos;
        self.pos += 1;
        let mut bytes = Vec::new();
        while let Some(&b) = self.src.get(self.pos) {
            self.pos += 1;
            match b {
                b'"' => return Ok(Sexp::Str(String::from_utf8_lossy(&bytes).into_owned())),
                b'\\' => {
                    let esc = *self
                        .src
                        .get(self.pos)
                        .ok_or(ParseError::UnterminatedString(start))?;
                    self.pos += 1;
                    bytes.push(match esc {
                        b'n' => b'\n',
                        other => other,
                    });
                }
                other => bytes.push(other),
            }
        }
        Err(ParseError::UnterminatedString(start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_lists_and_strings() {
        let s = Sexp::parse(r#"(a "b c" (d 1 2) e)"#).unwrap();
        assert_eq!(s.head(), Some("a"));
        assert_eq!(s.args()[0], Sexp::Str("b c".into()));
        assert_eq!(s.find("d").unwrap().args().len(), 2);
    }

    #[test]
    fn rejects_unbalanced_input() {
        assert_eq!(Sexp::parse("(a (b)"), Err(ParseError::UnexpectedEof));
        assert_eq!(Sexp::parse("(a))"), Err(ParseError::TrailingInput(3)));
    }

    #[test]
    fn string_escapes_round_trip() {
        let s = Sexp::string("say \"hi\" \\ done");
        let printed = s.to_kicad_string();
        assert_eq!(Sexp::parse(&printed).unwrap(), s);
    }

    #[test]
    fn formats_numbers_compactly() {
        assert_eq!(fmt_num(76.2), "76.2");
        assert_eq!(fmt_num(100.0), "100");
        assert_eq!(fmt_num(-0.00001), "0");
        assert_eq!(fmt_num(1.0 / 3.0), "0.3333");
    }

    #[test]
    fn matches_kicad_layout() {
        let s = Sexp::parse(r#"(a (at 1 2 0) (effects (font (size 1 1))))"#).unwrap();
        assert_eq!(
            s.to_kicad_string(),
            "(a\n\t(at 1 2 0)\n\t(effects\n\t\t(font\n\t\t\t(size 1 1)\n\t\t)\n\t)\n)\n"
        );
    }
}
