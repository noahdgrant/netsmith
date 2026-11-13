//! netsmith's one S-expression reader, and the two ways it writes one: KiCad's layout (tabs, one
//! nested list per line, [`Sexp::to_kicad_string`]), so that generated files diff cleanly
//! against files KiCad itself saved, and one line ([`Sexp::write`]).
//!
//! The reader reads a file as KiCad reads it: a string's escapes are KiCad's (`\n`, `\t`, and
//! any other character after a backslash stands for itself), and a bare token runs to the next
//! space or parenthesis. It works from the text of the file, not from a writer's Rust tree, so
//! fabrication reads exactly what KiCad and the board house read (issue #611 joined the reader
//! of the schematic writer and fab's board reader into this one).

use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Sexp {
    /// A bare token: keyword, number, `yes`/`no`.
    Atom(String),
    /// A double-quoted string, with escapes resolved.
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
            Self::UnexpectedEof => write!(f, "the file ends inside a parenthesis"),
            Self::UnexpectedClose(at) => write!(f, "unexpected ')' at byte {at}"),
            Self::UnterminatedString(at) => write!(f, "unterminated string at byte {at}"),
            Self::TrailingInput(at) => {
                write!(f, "unexpected text after the closing ')' at byte {at}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

impl Sexp {
    /// A bare token.
    pub fn atom(s: impl Into<String>) -> Self {
        Self::Atom(s.into())
    }

    /// A quoted string.
    pub fn string(s: impl Into<String>) -> Self {
        Self::Str(s.into())
    }

    /// A number as a bare token, written by [`fmt_num`].
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

    /// The leading atom of a list, such as `footprint` in `(footprint "R" ...)`.
    pub fn head(&self) -> Option<&str> {
        match self {
            Self::List(items) => match items.first() {
                Some(Self::Atom(a)) => Some(a),
                _ => None,
            },
            _ => None,
        }
    }

    /// All the items of a list, its head among them.
    pub fn items(&self) -> &[Sexp] {
        match self {
            Self::List(items) => items,
            _ => &[],
        }
    }

    /// The items of a list after its head.
    pub fn args(&self) -> &[Sexp] {
        match self {
            Self::List(items) if !items.is_empty() => &items[1..],
            _ => &[],
        }
    }

    /// The first child list headed by `name`.
    pub fn find(&self, name: &str) -> Option<&Sexp> {
        self.args().iter().find(|c| c.head() == Some(name))
    }

    /// The child lists headed by `name`.
    pub fn find_all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Sexp> + 'a {
        self.args().iter().filter(move |c| c.head() == Some(name))
    }

    /// The text of an atom or string.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Atom(s) | Self::Str(s) => Some(s),
            Self::List(_) => None,
        }
    }

    /// Reads a whole file holding exactly one expression.
    pub fn parse(src: &str) -> Result<Sexp, ParseError> {
        let bytes = src.as_bytes();
        let mut pos = 0;
        skip_space(bytes, &mut pos);
        let value = read(src, &mut pos)?;
        skip_space(bytes, &mut pos);
        if pos < bytes.len() {
            return Err(ParseError::TrailingInput(pos));
        }
        Ok(value)
    }

    /// The text in KiCad's layout, ending with a newline.
    pub fn to_kicad_string(&self) -> String {
        let mut out = String::new();
        self.write_kicad(&mut out, 0);
        out.push('\n');
        out
    }

    fn write_kicad(&self, out: &mut String, depth: usize) {
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
                    item.write_kicad(out, depth);
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
                        point.write_kicad(out, depth + 1);
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
                    item.write_kicad(out, depth + 1);
                }
                out.push('\n');
                for _ in 0..depth {
                    out.push('\t');
                }
                out.push(')');
            }
        }
    }

    /// The text of the expression, on one line. [`Sexp::parse`] reads it back.
    pub fn write(&self, out: &mut String) {
        match self {
            Sexp::Atom(a) => out.push_str(a),
            Sexp::Str(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\t' => out.push_str("\\t"),
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Sexp::List(items) => {
                out.push('(');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(' ');
                    }
                    item.write(out);
                }
                out.push(')');
            }
        }
    }
}

/// A string as KiCad's layout quotes it: a tab stays a tab.
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

fn skip_space(bytes: &[u8], pos: &mut usize) {
    while *pos < bytes.len() && bytes[*pos].is_ascii_whitespace() {
        *pos += 1;
    }
}

fn read(src: &str, pos: &mut usize) -> Result<Sexp, ParseError> {
    let bytes = src.as_bytes();
    match bytes.get(*pos) {
        None => Err(ParseError::UnexpectedEof),
        Some(b')') => Err(ParseError::UnexpectedClose(*pos)),
        Some(b'(') => {
            *pos += 1;
            let mut items = Vec::new();
            loop {
                skip_space(bytes, pos);
                match bytes.get(*pos) {
                    None => return Err(ParseError::UnexpectedEof),
                    Some(b')') => {
                        *pos += 1;
                        return Ok(Sexp::List(items));
                    }
                    Some(_) => items.push(read(src, pos)?),
                }
            }
        }
        Some(b'"') => {
            let start = *pos;
            *pos += 1;
            let mut out = String::new();
            let mut run = *pos;
            loop {
                match bytes.get(*pos) {
                    None => return Err(ParseError::UnterminatedString(start)),
                    Some(b'"') => {
                        out.push_str(&src[run..*pos]);
                        *pos += 1;
                        return Ok(Sexp::Str(out));
                    }
                    Some(b'\\') => {
                        out.push_str(&src[run..*pos]);
                        *pos += 1;
                        match bytes.get(*pos) {
                            None => return Err(ParseError::UnterminatedString(start)),
                            Some(b'n') => out.push('\n'),
                            Some(b't') => out.push('\t'),
                            Some(_) => {
                                // `\"`, `\\` and anything else stand for themselves; take a
                                // whole character so multi-byte text stays intact.
                                let ch = src[*pos..].chars().next().unwrap_or('\\');
                                out.push(ch);
                                *pos += ch.len_utf8() - 1;
                            }
                        }
                        *pos += 1;
                        run = *pos;
                    }
                    Some(_) => *pos += 1,
                }
            }
        }
        Some(_) => {
            let start = *pos;
            while *pos < bytes.len()
                && !bytes[*pos].is_ascii_whitespace()
                && bytes[*pos] != b'('
                && bytes[*pos] != b')'
            {
                *pos += 1;
            }
            Ok(Sexp::Atom(src[start..*pos].to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_lists_and_strings() {
        let s = Sexp::parse("(a (b \"x y\" 1.5) (c))").unwrap();
        assert_eq!(s.head(), Some("a"));
        assert_eq!(s.find("b").unwrap().items()[1].text(), Some("x y"));
        assert_eq!(s.find("b").unwrap().args()[1].text(), Some("1.5"));
        assert!(s.find("c").is_some());
        assert!(s.find("d").is_none());
        let s = Sexp::parse(r#"(a "b c" (d 1 2) e)"#).unwrap();
        assert_eq!(s.args()[0], Sexp::Str("b c".into()));
        assert_eq!(s.find("d").unwrap().args().len(), 2);
        assert_eq!(s.find_all("d").count(), 1);
    }

    #[test]
    fn resolves_escapes_and_keeps_unicode() {
        let s = Sexp::parse("(a \"say \\\"hi\\\" \u{b5}F\\\\\" \"a\\tb\\nc\")").unwrap();
        assert_eq!(s.items()[1].text(), Some("say \"hi\" \u{b5}F\\"));
        assert_eq!(s.items()[2].text(), Some("a\tb\nc"));
    }

    #[test]
    fn what_write_makes_parse_reads() {
        let src = "(a (b \"x \\\"y\\\" \\\\ z\" 1.5) (c))";
        let s = Sexp::parse(src).unwrap();
        let mut out = String::new();
        s.write(&mut out);
        assert_eq!(Sexp::parse(&out).unwrap(), s);
        assert_eq!(out, src);
    }

    #[test]
    fn reports_malformed_input() {
        assert_eq!(Sexp::parse("(a (b)"), Err(ParseError::UnexpectedEof));
        assert_eq!(Sexp::parse("(a))"), Err(ParseError::TrailingInput(3)));
        assert_eq!(
            Sexp::parse("(a \"b)"),
            Err(ParseError::UnterminatedString(3))
        );
        assert_eq!(Sexp::parse(")"), Err(ParseError::UnexpectedClose(0)));
    }

    #[test]
    fn string_escapes_round_trip_in_kicads_layout() {
        let s = Sexp::string("say \"hi\" \\ done\tand a tab");
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
