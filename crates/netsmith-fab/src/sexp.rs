//! A small S-expression reader, just enough to read the parts of a `.kicad_pcb` that
//! fabrication needs and the connectivity `netsmith compare` reads. It is deliberately not
//! shared with the schematic writer: this crate works from the file KiCad reads, not from
//! anyone's Rust API.

#[derive(Debug, Clone, PartialEq)]
pub enum Sexp {
    /// A bare token: keyword, number, `yes`.
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
            Self::TrailingInput(at) => write!(f, "unexpected text after the board at byte {at}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl Sexp {
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

    /// The text of an atom or string.
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Atom(s) | Self::Str(s) => Some(s),
            Self::List(_) => None,
        }
    }

    /// The items after the head, or all items of a list.
    pub fn items(&self) -> &[Sexp] {
        match self {
            Self::List(items) => items,
            _ => &[],
        }
    }

    /// The children of a list that are lists headed by `name`.
    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Sexp> + 'a {
        self.items().iter().filter(move |c| c.head() == Some(name))
    }

    /// The first child list headed by `name`.
    pub fn child(&self, name: &str) -> Option<&Sexp> {
        self.items().iter().find(|c| c.head() == Some(name))
    }
}

impl Sexp {
    /// A bare token.
    pub fn atom(text: impl Into<String>) -> Sexp {
        Sexp::Atom(text.into())
    }

    /// A quoted string.
    pub fn string(text: impl Into<String>) -> Sexp {
        Sexp::Str(text.into())
    }

    /// A list headed by `head`.
    pub fn node(head: &str, items: Vec<Sexp>) -> Sexp {
        let mut all = vec![Sexp::atom(head)];
        all.extend(items);
        Sexp::List(all)
    }

    /// The text of the expression, on one line. [`parse`] reads it back.
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

/// Reads a whole file holding exactly one list.
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
        let s = parse("(a (b \"x y\" 1.5) (c))").unwrap();
        assert_eq!(s.head(), Some("a"));
        assert_eq!(s.child("b").unwrap().items()[1].text(), Some("x y"));
        assert_eq!(s.child("b").unwrap().items()[2].text(), Some("1.5"));
        assert!(s.child("c").is_some());
        assert!(s.child("d").is_none());
    }

    #[test]
    fn resolves_escapes_and_keeps_unicode() {
        let s = parse("(a \"say \\\"hi\\\" \u{b5}F\\\\\")").unwrap();
        assert_eq!(s.items()[1].text(), Some("say \"hi\" \u{b5}F\\"));
    }

    #[test]
    fn what_write_makes_parse_reads() {
        let src = "(a (b \"x \\\"y\\\" \\\\ z\" 1.5) (c))";
        let s = parse(src).unwrap();
        let mut out = String::new();
        s.write(&mut out);
        assert_eq!(parse(&out).unwrap(), s);
        assert_eq!(out, src);
    }

    #[test]
    fn reports_malformed_input() {
        assert_eq!(parse("(a (b)"), Err(ParseError::UnexpectedEof));
        assert_eq!(parse("(a))"), Err(ParseError::TrailingInput(3)));
        assert_eq!(parse("(a \"b)"), Err(ParseError::UnterminatedString(3)));
        assert_eq!(parse(")"), Err(ParseError::UnexpectedClose(0)));
    }
}
