//! The text of a STEP file (ISO 10303-21, "Part 21") as data: the header's entities and the
//! data section's numbered instances, each a simple entity (`#5=LINE('',#3,#4);`) or a complex
//! one (`#6=(BOUNDED_CURVE()B_SPLINE_CURVE(...)...);`), with every parameter typed: `$`, `*`,
//! integers, reals, strings (their escapes decoded), enumerations, binaries, references, lists
//! and typed parameters (`LENGTH_MEASURE(1.E-06)`).
//!
//! The parser works on bytes, keeps the line each instance starts on for messages, and fails on
//! the first thing it cannot read with that line: it never guesses. The instances are kept by id,
//! so whatever reads them walks them in id order.

use std::collections::BTreeMap;
use std::fmt;

/// A parameter of an entity.
#[derive(Debug, Clone, PartialEq)]
pub enum Param {
    /// `$`: no value.
    Null,
    /// `*`: a value derived from others.
    Derived,
    Int(i64),
    Real(f64),
    /// A string with its escapes decoded.
    Str(String),
    /// `.NAME.`, without the dots: `T`, `F`, `MILLI`.
    Enum(String),
    /// `"..."`: the hex digits as written.
    Binary(String),
    /// `#n`.
    Ref(u64),
    List(Vec<Param>),
    /// `NAME(...)`, a value of a defined type: `LENGTH_MEASURE(25.4)`.
    Typed(String, Vec<Param>),
}

impl Param {
    pub fn as_ref(&self) -> Option<u64> {
        match self {
            Param::Ref(r) => Some(*r),
            _ => None,
        }
    }

    /// A real, or an integer as a real; a typed parameter's single value too.
    pub fn as_real(&self) -> Option<f64> {
        match self {
            Param::Real(x) => Some(*x),
            Param::Int(i) => Some(*i as f64),
            Param::Typed(_, v) if v.len() == 1 => v[0].as_real(),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Param::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Param]> {
        match self {
            Param::List(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_enum(&self) -> Option<&str> {
        match self {
            Param::Enum(e) => Some(e),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Param::Str(s) => Some(s),
            _ => None,
        }
    }

    /// `.T.` or `.F.`.
    pub fn as_bool(&self) -> Option<bool> {
        match self.as_enum()? {
            "T" => Some(true),
            "F" => Some(false),
            _ => None,
        }
    }
}

/// One entity: its name in upper case and its parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub name: String,
    pub params: Vec<Param>,
}

/// A numbered instance of the data section: one entity, or the parts of a complex instance in
/// the order they are written.
#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    /// The line its `#n=` is on, from 1.
    pub line: u32,
    pub parts: Vec<Entity>,
}

impl Instance {
    /// The part named `name`, the entity itself for a simple instance.
    pub fn part(&self, name: &str) -> Option<&Entity> {
        self.parts.iter().find(|e| e.name == name)
    }

    /// The entity's name, or the parts' names joined with `+` for a complex instance.
    pub fn name(&self) -> String {
        let names: Vec<&str> = self.parts.iter().map(|e| e.name.as_str()).collect();
        names.join("+")
    }

    pub fn is_complex(&self) -> bool {
        self.parts.len() > 1
    }
}

/// A file read: the header's entities and the instances by id.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Exchange {
    pub header: Vec<Entity>,
    pub data: BTreeMap<u64, Instance>,
}

/// Why a file could not be read as Part 21.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    /// It does not start with `ISO-10303-21;`.
    NotStep,
    /// It does, but `line` holds something the grammar does not allow.
    Syntax { line: u32, message: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::NotStep => {
                f.write_str("not a STEP file: it does not start with ISO-10303-21;")
            }
            ParseError::Syntax { line, message } => write!(f, "line {line}: {message}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Reads `bytes` as a Part 21 file.
pub fn parse(bytes: &[u8]) -> Result<Exchange, ParseError> {
    let mut p = Parser {
        s: bytes,
        at: 0,
        line: 1,
    };
    // A byte order mark may come first.
    if p.s.starts_with(&[0xEF, 0xBB, 0xBF]) {
        p.at = 3;
    }
    p.skip();
    if !p.eat_word(b"ISO-10303-21") {
        return Err(ParseError::NotStep);
    }
    p.skip();
    if !p.eat(b';') {
        return Err(ParseError::NotStep);
    }
    let mut ex = Exchange::default();
    p.skip();
    p.expect_keyword("HEADER")?;
    p.expect(b';')?;
    loop {
        p.skip();
        let word = p.keyword()?;
        if word == "ENDSEC" {
            p.expect(b';')?;
            break;
        }
        let params = p.params()?;
        p.expect(b';')?;
        ex.header.push(Entity { name: word, params });
    }
    // One or more data sections, then the end.
    loop {
        p.skip();
        let word = p.keyword()?;
        match word.as_str() {
            "END-ISO-10303-21" => {
                p.expect(b';')?;
                return Ok(ex);
            }
            "DATA" => {
                p.skip();
                if p.peek() == Some(b'(') {
                    p.params()?;
                }
                p.expect(b';')?;
                p.data(&mut ex)?;
            }
            other => {
                return Err(p.error(format!("expected DATA or END-ISO-10303-21, found {other}")));
            }
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    at: usize,
    line: u32,
}

impl Parser<'_> {
    fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError::Syntax {
            line: self.line,
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.at).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.at += 1;
        if c == b'\n' {
            self.line += 1;
        }
        Some(c)
    }

    /// Skips white space and comments.
    fn skip(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => {
                    self.bump();
                }
                Some(b'/') if self.s.get(self.at + 1) == Some(&b'*') => {
                    self.bump();
                    self.bump();
                    while self.at < self.s.len() {
                        if self.peek() == Some(b'*') && self.s.get(self.at + 1) == Some(&b'/') {
                            self.bump();
                            self.bump();
                            break;
                        }
                        self.bump();
                    }
                }
                _ => return,
            }
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self, w: &[u8]) -> bool {
        if self.s[self.at..].starts_with(w) {
            for _ in 0..w.len() {
                self.bump();
            }
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), ParseError> {
        self.skip();
        if self.eat(c) {
            Ok(())
        } else {
            Err(self.error(format!("expected '{}', found {}", c as char, self.found())))
        }
    }

    fn found(&self) -> String {
        match self.peek() {
            None => "the end of the file".into(),
            Some(c) if c.is_ascii_graphic() => format!("'{}'", c as char),
            Some(c) => format!("byte {c:#04x}"),
        }
    }

    fn expect_keyword(&mut self, k: &str) -> Result<(), ParseError> {
        let w = self.keyword()?;
        if w == k {
            Ok(())
        } else {
            Err(self.error(format!("expected {k}, found {w}")))
        }
    }

    /// A keyword: letters, digits, `_` and `-` (for `END-ISO-10303-21`), upper-cased, with a
    /// user-defined one's `!` kept.
    fn keyword(&mut self) -> Result<String, ParseError> {
        self.skip();
        let start = self.at;
        if self.peek() == Some(b'!') {
            self.bump();
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' || c == b'-' {
                self.bump();
            } else {
                break;
            }
        }
        if self.at == start || !self.s[start..self.at].iter().any(u8::is_ascii_alphabetic) {
            return Err(self.error(format!("expected a keyword, found {}", self.found())));
        }
        Ok(String::from_utf8_lossy(&self.s[start..self.at]).to_ascii_uppercase())
    }

    fn data(&mut self, ex: &mut Exchange) -> Result<(), ParseError> {
        loop {
            self.skip();
            if self.peek() != Some(b'#') {
                self.expect_keyword("ENDSEC")?;
                self.expect(b';')?;
                return Ok(());
            }
            let line = self.line;
            self.bump();
            let id = self.unsigned()?;
            self.expect(b'=')?;
            self.skip();
            let mut parts = Vec::new();
            if self.eat(b'(') {
                loop {
                    self.skip();
                    if self.eat(b')') {
                        break;
                    }
                    let name = self.keyword()?;
                    let params = self.params()?;
                    parts.push(Entity { name, params });
                }
                if parts.is_empty() {
                    return Err(self.error(format!("#{id} is an empty complex instance")));
                }
            } else {
                let name = self.keyword()?;
                let params = self.params()?;
                parts.push(Entity { name, params });
            }
            self.expect(b';')?;
            if ex.data.insert(id, Instance { line, parts }).is_some() {
                return Err(ParseError::Syntax {
                    line,
                    message: format!("#{id} is defined twice"),
                });
            }
        }
    }

    fn unsigned(&mut self) -> Result<u64, ParseError> {
        let start = self.at;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.bump();
        }
        std::str::from_utf8(&self.s[start..self.at])
            .ok()
            .and_then(|t| t.parse().ok())
            .ok_or_else(|| {
                self.error(format!(
                    "expected an instance number, found {}",
                    self.found()
                ))
            })
    }

    /// `( p, p, ... )`.
    fn params(&mut self) -> Result<Vec<Param>, ParseError> {
        self.expect(b'(')?;
        let mut out = Vec::new();
        self.skip();
        if self.eat(b')') {
            return Ok(out);
        }
        loop {
            out.push(self.param()?);
            self.skip();
            if self.eat(b')') {
                return Ok(out);
            }
            if !self.eat(b',') {
                return Err(self.error(format!("expected ',' or ')', found {}", self.found())));
            }
        }
    }

    fn param(&mut self) -> Result<Param, ParseError> {
        self.skip();
        match self.peek() {
            Some(b'$') => {
                self.bump();
                Ok(Param::Null)
            }
            Some(b'*') => {
                self.bump();
                Ok(Param::Derived)
            }
            Some(b'#') => {
                self.bump();
                Ok(Param::Ref(self.unsigned()?))
            }
            Some(b'\'') => self.string().map(Param::Str),
            Some(b'"') => {
                self.bump();
                let start = self.at;
                while let Some(c) = self.peek() {
                    if c == b'"' {
                        break;
                    }
                    if !c.is_ascii_hexdigit() {
                        return Err(self.error("a binary holds hex digits only"));
                    }
                    self.bump();
                }
                let hex = String::from_utf8_lossy(&self.s[start..self.at]).into_owned();
                self.expect(b'"')?;
                Ok(Param::Binary(hex))
            }
            Some(b'.') => {
                self.bump();
                let start = self.at;
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphanumeric() || c == b'_' {
                        self.bump();
                    } else {
                        break;
                    }
                }
                let e = String::from_utf8_lossy(&self.s[start..self.at]).to_ascii_uppercase();
                if e.is_empty() || !self.eat(b'.') {
                    return Err(self.error("an enumeration is written .NAME."));
                }
                Ok(Param::Enum(e))
            }
            Some(b'(') => self.params().map(Param::List),
            Some(b'+' | b'-' | b'0'..=b'9') => self.number(),
            Some(c) if c.is_ascii_alphabetic() || c == b'!' => {
                let name = self.keyword()?;
                let params = self.params()?;
                Ok(Param::Typed(name, params))
            }
            _ => Err(self.error(format!("expected a parameter, found {}", self.found()))),
        }
    }

    fn number(&mut self) -> Result<Param, ParseError> {
        let start = self.at;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.bump();
        }
        let digits = |p: &mut Self| {
            let s = p.at;
            while matches!(p.peek(), Some(b'0'..=b'9')) {
                p.bump();
            }
            p.at > s
        };
        let whole = digits(self);
        let mut real = false;
        if self.eat(b'.') {
            real = true;
            digits(self);
        }
        if matches!(self.peek(), Some(b'E' | b'e')) {
            real = true;
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            if !digits(self) {
                return Err(self.error("an exponent needs digits"));
            }
        }
        if !whole {
            return Err(self.error("a number starts with a digit"));
        }
        let text = std::str::from_utf8(&self.s[start..self.at]).unwrap_or("");
        let bad = || self.error(format!("{text} is not a number"));
        if real {
            // `f64::from_str` takes `1.`, `1.E-06` and `-0.5`, rounded correctly on every machine.
            let x: f64 = text.parse().map_err(|_| bad())?;
            if !x.is_finite() {
                return Err(bad());
            }
            Ok(Param::Real(x))
        } else {
            text.parse().map(Param::Int).map_err(|_| bad())
        }
    }

    /// A string: `'...'` with `''` for a quote and the `\` escapes of Part 21 decoded: `\\`,
    /// `\X\hh` (one ISO 8859-1 character), `\X2\hhhh...\X0\` (UTF-16), `\X4\hhhhhhhh...\X0\`
    /// (UTF-32), `\S\c` (a character of the upper half) and `\P?\` (a code page, ignored).
    fn string(&mut self) -> Result<String, ParseError> {
        let line = self.line;
        self.bump();
        let mut out = String::new();
        let unterminated = || ParseError::Syntax {
            line,
            message: "a string is not closed".into(),
        };
        loop {
            let c = self.bump().ok_or_else(unterminated)?;
            match c {
                b'\'' => {
                    if self.peek() == Some(b'\'') {
                        self.bump();
                        out.push('\'');
                    } else {
                        return Ok(out);
                    }
                }
                b'\\' => self.escape(&mut out)?,
                b'\r' | b'\n' => {}
                c => out.push(char::from(c)),
            }
        }
    }

    fn hex(&mut self, n: usize) -> Result<u32, ParseError> {
        let mut v = 0u32;
        for _ in 0..n {
            let c = self
                .bump()
                .ok_or_else(|| self.error("a string is not closed"))?;
            let d = char::from(c)
                .to_digit(16)
                .ok_or_else(|| self.error("a \\X escape needs hex digits"))?;
            v = v * 16 + d;
        }
        Ok(v)
    }

    fn escape(&mut self, out: &mut String) -> Result<(), ParseError> {
        let bad = |p: &Self| p.error("a string has an escape Part 21 does not define");
        match self.bump() {
            Some(b'\\') => out.push('\\'),
            Some(b'S') => {
                if !self.eat(b'\\') {
                    return Err(bad(self));
                }
                let c = self.bump().ok_or_else(|| bad(self))?;
                out.push(char::from(c | 0x80));
            }
            Some(b'P') => {
                self.bump();
                if !self.eat(b'\\') {
                    return Err(bad(self));
                }
            }
            Some(b'X') => match self.bump() {
                Some(b'\\') => out.push(char::from_u32(self.hex(2)?).ok_or_else(|| bad(self))?),
                Some(d @ (b'2' | b'4')) => {
                    if !self.eat(b'\\') {
                        return Err(bad(self));
                    }
                    let width = if d == b'2' { 4 } else { 8 };
                    let mut units = Vec::new();
                    loop {
                        if self.peek() == Some(b'\\') {
                            self.bump();
                            if self.eat_word(b"X0\\") {
                                break;
                            }
                            return Err(bad(self));
                        }
                        units.push(self.hex(width)?);
                    }
                    if d == b'2' {
                        let u16s: Vec<u16> = units.iter().map(|&u| u as u16).collect();
                        out.push_str(&String::from_utf16(&u16s).map_err(|_| bad(self))?);
                    } else {
                        for u in units {
                            out.push(char::from_u32(u).ok_or_else(|| bad(self))?);
                        }
                    }
                }
                _ => return Err(bad(self)),
            },
            _ => return Err(bad(self)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(data: &str) -> String {
        format!(
            "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('x'),'2;1');\nENDSEC;\nDATA;\n{data}\nENDSEC;\nEND-ISO-10303-21;\n"
        )
    }

    #[test]
    fn simple_and_complex_instances_with_every_kind_of_parameter() {
        let text = file(
            "#1=CARTESIAN_POINT('',(0.,-1.5,2.E-03));\n/* a comment */\n#3=FOO($,*,.T.,7,#1,\"0F\",LENGTH_MEASURE(25.4),());\n#2=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));",
        );
        let ex = parse(text.as_bytes()).unwrap();
        assert_eq!(ex.header[0].name, "FILE_DESCRIPTION");
        let ids: Vec<u64> = ex.data.keys().copied().collect();
        assert_eq!(ids, [1, 2, 3], "in id order");
        let p = &ex.data[&1];
        assert_eq!(p.line, 6);
        assert_eq!(
            p.parts[0].params[1],
            Param::List(vec![
                Param::Real(0.0),
                Param::Real(-1.5),
                Param::Real(0.002)
            ])
        );
        let f = &ex.data[&3].parts[0].params;
        assert_eq!(
            f,
            &vec![
                Param::Null,
                Param::Derived,
                Param::Enum("T".into()),
                Param::Int(7),
                Param::Ref(1),
                Param::Binary("0F".into()),
                Param::Typed("LENGTH_MEASURE".into(), vec![Param::Real(25.4)]),
                Param::List(vec![]),
            ]
        );
        let unit = &ex.data[&2];
        assert!(unit.is_complex());
        assert_eq!(unit.name(), "LENGTH_UNIT+NAMED_UNIT+SI_UNIT");
        assert_eq!(
            unit.part("SI_UNIT").unwrap().params[0],
            Param::Enum("MILLI".into())
        );
    }

    #[test]
    fn strings_decode_their_escapes() {
        let text = file(
            r"#1=A('it''s','a\\b','\X\E9','\X2\00E9263A\X0\','\X4\0001F600\X0\','\S\i','x\P1\y');",
        );
        let ex = parse(text.as_bytes()).unwrap();
        let s: Vec<&str> = ex.data[&1].parts[0]
            .params
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        assert_eq!(s, ["it's", "a\\b", "é", "é☺", "😀", "é", "xy"]);
    }

    #[test]
    fn what_is_not_part21_says_so() {
        assert_eq!(parse(b"solid cube\nfacet normal"), Err(ParseError::NotStep));
        assert_eq!(parse(b""), Err(ParseError::NotStep));
        let cases = [
            ("#1=LINE('',#2,#3)", 7),           // no semicolon: found at ENDSEC
            ("#1=LINE('',#2,#3);\n#1=X();", 7), // twice
            ("#1=LINE('',#2,#3;", 6),
            ("#1=X('open);", 6),
            ("#1=X(1.5.);", 6),
            ("#1=X(\"0G\");", 6),
            ("#1=X(.T);", 6),
            ("#1=X('\\Q');", 6),
        ];
        for (data, line) in cases {
            match parse(file(data).as_bytes()) {
                Err(ParseError::Syntax { line: l, .. }) => assert_eq!(l, line, "{data}"),
                other => panic!("{data}: {other:?}"),
            }
        }
        // Truncated anywhere: an error, never a panic.
        let whole = file("#1=CARTESIAN_POINT('',(0.,1.,2.));\n#2=(A()B(.X.,'s'));");
        for n in 0..whole.len() - 1 {
            assert!(parse(&whole.as_bytes()[..n]).is_err(), "cut at {n}");
        }
    }
}
