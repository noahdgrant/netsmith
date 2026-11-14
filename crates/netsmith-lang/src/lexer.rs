//! Turns source text into tokens. Newlines are tokens because statements end at a
//! newline or `;`; the parser decides where they are insignificant.

use crate::ast::Number;
use crate::error::{ParseError, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    QuotedIdent(String),
    Number(Number),
    Str(String),
    LBrace,
    RBrace,
    LParen,
    RParen,
    /// `[` and `]` around the index of one part of a group (`.decoupling[2]`, ADR 0119).
    LBracket,
    RBracket,
    /// `#[`, which opens an attribute (`#[bank]`, `#[role(bias)]`, ADR 0125).
    HashBracket,
    Comma,
    Semi,
    Colon,
    ColonColon,
    /// A dot touching the name before it: member access, as in `a.b`.
    Dot,
    /// A dot that starts a name: `.a` is a member of the enclosing definition.
    LeadingDot,
    DotDot,
    Tilde,
    TildeArrow,
    AmpEq,
    /// `?=`, a default a board may replace.
    QuestionEq,
    Eq,
    EqEq,
    GtEq,
    LtEq,
    /// `<` and `>` around the type parameters of a module and the type arguments of a use
    /// (`Buck<Lt8641>`, ADR 0115). Not a comparison: those are `<=` and `>=`.
    Lt,
    Gt,
    Plus,
    Minus,
    Star,
    Slash,
    PlusMinus,
    /// `/// text`, documenting the item that follows.
    DocOuter(String),
    /// `//! text`, documenting the file or definition it is inside.
    DocInner(String),
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

pub fn lex(src: &str) -> Result<Vec<Token>, ParseError> {
    Lexer {
        src,
        chars: src.char_indices().collect(),
        i: 0,
    }
    .run()
}

struct Lexer<'a> {
    src: &'a str,
    chars: Vec<(usize, char)>,
    i: usize,
}

impl Lexer<'_> {
    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.i + n).map(|&(_, c)| c)
    }

    /// Byte offset of the next character.
    fn pos(&self) -> usize {
        self.chars.get(self.i).map_or(self.src.len(), |&(p, _)| p)
    }

    fn eat(&mut self, s: &str) -> bool {
        let matches = s
            .chars()
            .enumerate()
            .all(|(n, c)| self.peek_at(n) == Some(c));
        if matches {
            self.i += s.chars().count();
        }
        matches
    }

    fn run(mut self) -> Result<Vec<Token>, ParseError> {
        let mut out: Vec<Token> = Vec::new();
        while let Some(c) = self.peek_at(0) {
            let start = self.pos();
            match c {
                ' ' | '\t' | '\r' => self.i += 1,
                '\n' => {
                    self.i += 1;
                    if !matches!(
                        out.last(),
                        Some(Token {
                            tok: Tok::Newline,
                            ..
                        })
                    ) {
                        out.push(Token {
                            tok: Tok::Newline,
                            span: Span::new(start, start + 1),
                        });
                    }
                }
                '/' if self.peek_at(1) == Some('/') => {
                    // `///` and `//!` are doc comments; `////` and `//` are plain.
                    let doc = match (self.peek_at(2), self.peek_at(3)) {
                        (Some('/'), Some('/')) => None,
                        (Some('/'), _) => Some(false),
                        (Some('!'), _) => Some(true),
                        _ => None,
                    };
                    while self.peek_at(0).is_some_and(|c| c != '\n') {
                        self.i += 1;
                    }
                    if let Some(inner) = doc {
                        let end = self.pos();
                        let raw = self.src[start + 3..end].trim_end();
                        let text = raw.strip_prefix(' ').unwrap_or(raw).to_string();
                        let tok = if inner {
                            Tok::DocInner(text)
                        } else {
                            Tok::DocOuter(text)
                        };
                        out.push(Token {
                            tok,
                            span: Span::new(start, end),
                        });
                    }
                }
                '/' if self.peek_at(1) == Some('*') => return Err(self.block_comment()),
                '"' => out.push(self.string()?),
                '\'' => out.push(self.quoted_ident()?),
                c if c.is_ascii_digit() => out.push(self.number()?),
                c if c.is_alphabetic() || c == '_' => out.push(self.ident()),
                _ => out.push(self.punct()?),
            }
        }
        let end = self.src.len();
        out.push(Token {
            tok: Tok::Eof,
            span: Span::new(end, end),
        });
        Ok(out)
    }

    fn ident(&mut self) -> Token {
        let start = self.pos();
        while self
            .peek_at(0)
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            self.i += 1;
        }
        let end = self.pos();
        Token {
            tok: Tok::Ident(self.src[start..end].to_string()),
            span: Span::new(start, end),
        }
    }

    fn number(&mut self) -> Result<Token, ParseError> {
        let start = self.pos();
        let is_hex = self.peek_at(0) == Some('0')
            && matches!(self.peek_at(1), Some('x' | 'X'))
            && self.peek_at(2).is_some_and(|c| c.is_ascii_hexdigit());
        if is_hex {
            self.i += 2;
            let digits = self.pos();
            while self.peek_at(0).is_some_and(|c| c.is_ascii_hexdigit()) {
                self.i += 1;
            }
            let end = self.pos();
            let value = u64::from_str_radix(&self.src[digits..end], 16)
                .map_err(|_| ParseError::new(Span::new(start, end), "hex literal is too large"))?;
            let number = Number {
                text: self.src[start..end].to_string(),
                value: value as f64,
                unit: None,
            };
            return Ok(Token {
                tok: Tok::Number(number),
                span: Span::new(start, end),
            });
        }

        self.digits();
        // A fraction needs a digit after the dot, so `1..5` stays a range.
        if self.peek_at(0) == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
            self.digits();
        }
        // An exponent needs digits, so `5eV` is five electron-volts, not `5e` + `V`.
        if matches!(self.peek_at(0), Some('e' | 'E')) {
            let signed = matches!(self.peek_at(1), Some('+' | '-'));
            let first = self.peek_at(if signed { 2 } else { 1 });
            if first.is_some_and(|c| c.is_ascii_digit()) {
                self.i += if signed { 2 } else { 1 };
                self.digits();
            }
        }
        let digits_end = self.pos();
        if self.peek_at(0) == Some('%') {
            self.i += 1;
        } else {
            while self.peek_at(0).is_some_and(char::is_alphabetic) {
                self.i += 1;
            }
        }
        let end = self.pos();
        let value: f64 = self.src[start..digits_end]
            .parse()
            .map_err(|_| ParseError::new(Span::new(start, end), "invalid number"))?;
        let unit = (end > digits_end).then(|| self.src[digits_end..end].to_string());
        let number = Number {
            text: self.src[start..end].to_string(),
            value,
            unit,
        };
        Ok(Token {
            tok: Tok::Number(number),
            span: Span::new(start, end),
        })
    }

    fn digits(&mut self) {
        while self.peek_at(0).is_some_and(|c| c.is_ascii_digit()) {
            self.i += 1;
        }
    }

    fn string(&mut self) -> Result<Token, ParseError> {
        let start = self.pos();
        self.i += 1;
        let mut s = String::new();
        loop {
            let unterminated =
                || ParseError::new(Span::new(start, start + 1), "this string is never closed");
            match self.peek_at(0) {
                None | Some('\n') => return Err(unterminated()),
                Some('"') => {
                    self.i += 1;
                    break;
                }
                Some('\\') => {
                    let esc_at = self.pos();
                    self.i += 1;
                    match self.peek_at(0) {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some(c @ ('"' | '\\')) => s.push(c),
                        Some(c) => {
                            return Err(ParseError::new(
                                Span::new(esc_at, esc_at + 1 + c.len_utf8()),
                                format!("unknown escape `\\{c}`"),
                            ));
                        }
                        None => return Err(unterminated()),
                    }
                    self.i += 1;
                }
                Some(c) => {
                    s.push(c);
                    self.i += 1;
                }
            }
        }
        Ok(Token {
            tok: Tok::Str(s),
            span: Span::new(start, self.pos()),
        })
    }

    fn quoted_ident(&mut self) -> Result<Token, ParseError> {
        let start = self.pos();
        self.i += 1;
        let from = self.pos();
        while self.peek_at(0).is_some_and(|c| c != '\'' && c != '\n') {
            self.i += 1;
        }
        if self.peek_at(0) != Some('\'') {
            return Err(ParseError::new(
                Span::new(start, start + 1),
                "this quoted name is never closed",
            ));
        }
        let name = self.src[from..self.pos()].to_string();
        self.i += 1;
        Ok(Token {
            tok: Tok::QuotedIdent(name),
            span: Span::new(start, self.pos()),
        })
    }

    /// `/*` starts nothing: a comment is `//`, `///` or `//!`. Say so at the slash, rather
    /// than reading on into a stray `/` and `*` whose error would not.
    fn block_comment(&mut self) -> ParseError {
        let start = self.pos();
        ParseError::new(
            Span::new(start, start + 2),
            "block comments do not exist; use `//`",
        )
    }

    fn punct(&mut self) -> Result<Token, ParseError> {
        let start = self.pos();
        // A single dot is member access if it touches a name, and the start of a name
        // otherwise. Deciding here keeps the parser blind to whitespace.
        if self.peek_at(0) == Some('.') && self.peek_at(1) != Some('.') {
            let touches_name = self
                .i
                .checked_sub(1)
                .and_then(|i| self.chars.get(i))
                .is_some_and(|&(_, c)| c.is_alphanumeric() || c == '_' || c == '\'' || c == ']');
            self.i += 1;
            let tok = if touches_name {
                Tok::Dot
            } else {
                Tok::LeadingDot
            };
            return Ok(Token {
                tok,
                span: Span::new(start, self.pos()),
            });
        }
        // Longer spellings first.
        let table: [(&str, Tok); 27] = [
            ("#[", Tok::HashBracket),
            ("::", Tok::ColonColon),
            ("~>", Tok::TildeArrow),
            ("&=", Tok::AmpEq),
            ("?=", Tok::QuestionEq),
            ("==", Tok::EqEq),
            (">=", Tok::GtEq),
            ("<=", Tok::LtEq),
            ("<", Tok::Lt),
            (">", Tok::Gt),
            ("+/-", Tok::PlusMinus),
            ("±", Tok::PlusMinus),
            ("..", Tok::DotDot),
            ("~", Tok::Tilde),
            ("=", Tok::Eq),
            ("+", Tok::Plus),
            ("-", Tok::Minus),
            ("*", Tok::Star),
            ("/", Tok::Slash),
            (":", Tok::Colon),
            ("{", Tok::LBrace),
            ("}", Tok::RBrace),
            ("(", Tok::LParen),
            (")", Tok::RParen),
            ("[", Tok::LBracket),
            ("]", Tok::RBracket),
            (",", Tok::Comma),
        ];
        for (text, tok) in table {
            if self.eat(text) {
                return Ok(Token {
                    tok,
                    span: Span::new(start, self.pos()),
                });
            }
        }
        if self.eat(";") {
            return Ok(Token {
                tok: Tok::Semi,
                span: Span::new(start, self.pos()),
            });
        }
        let c = self.peek_at(0).unwrap_or('?');
        Err(ParseError::new(
            Span::new(start, start + c.len_utf8()),
            format!("unexpected character `{c}`"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(src: &str) -> Vec<Number> {
        lex(src)
            .unwrap()
            .into_iter()
            .filter_map(|t| match t.tok {
                Tok::Number(n) => Some(n),
                _ => None,
            })
            .collect()
    }

    fn parts(n: &Number) -> (f64, Option<&str>) {
        (n.value, n.unit.as_deref())
    }

    #[test]
    fn splits_value_and_unit_suffix() {
        let n = numbers("100nF 1kohm 8mA 3.3V 5%");
        assert_eq!(parts(&n[0]), (100.0, Some("nF")));
        assert_eq!(parts(&n[1]), (1.0, Some("kohm")));
        assert_eq!(parts(&n[2]), (8.0, Some("mA")));
        assert_eq!(parts(&n[3]), (3.3, Some("V")));
        assert_eq!(parts(&n[4]), (5.0, Some("%")));
    }

    #[test]
    fn keeps_a_range_apart_from_a_fraction() {
        let n = numbers("1.8V..2.2V 1..5");
        assert_eq!(
            n.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(),
            ["1.8V", "2.2V", "1", "5"]
        );
    }

    #[test]
    fn exponents_need_digits() {
        let n = numbers("5e3 2.5e-3A 5eV");
        assert_eq!(parts(&n[0]), (5000.0, None));
        assert_eq!(parts(&n[1]), (0.0025, Some("A")));
        assert_eq!(parts(&n[2]), (5.0, Some("eV")));
    }

    #[test]
    fn reads_hex_without_a_unit() {
        let n = numbers("0x1A");
        assert_eq!(parts(&n[0]), (26.0, None));
        assert_eq!(n[0].text, "0x1A");
    }

    #[test]
    fn plus_minus_has_two_spellings() {
        let kinds = |s: &str| {
            lex(s)
                .unwrap()
                .into_iter()
                .map(|t| t.tok)
                .collect::<Vec<_>>()
        };
        assert_eq!(kinds("a +/- b"), kinds("a ± b"));
    }

    #[test]
    fn comments_run_to_end_of_line_and_collapse_newlines() {
        let toks = lex("a // note\n\n\nb").unwrap();
        let kinds: Vec<_> = toks.into_iter().map(|t| t.tok).collect();
        assert_eq!(
            kinds,
            [
                Tok::Ident("a".into()),
                Tok::Newline,
                Tok::Ident("b".into()),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn dots_are_member_access_only_when_they_touch_a_name() {
        let dots = |s: &str| {
            lex(s)
                .unwrap()
                .into_iter()
                .filter(|t| matches!(t.tok, Tok::Dot | Tok::LeadingDot))
                .map(|t| t.tok)
                .collect::<Vec<_>>()
        };
        assert_eq!(dots(".a.b"), [Tok::LeadingDot, Tok::Dot]);
        assert_eq!(dots(".a .b"), [Tok::LeadingDot, Tok::LeadingDot]);
        assert_eq!(dots("x ~> .y.'5'"), [Tok::LeadingDot, Tok::Dot]);
        assert_eq!(dots("(.a - .b)"), [Tok::LeadingDot, Tok::LeadingDot]);
        assert_eq!(dots("1V..2V"), [], "a range is not a dot");
    }

    #[test]
    fn doc_comments_follow_rust_spelling() {
        let kinds = |s: &str| {
            lex(s)
                .unwrap()
                .into_iter()
                .map(|t| t.tok)
                .collect::<Vec<_>>()
        };
        assert_eq!(kinds("/// outer\n")[0], Tok::DocOuter("outer".into()));
        assert_eq!(kinds("//! inner\n")[0], Tok::DocInner("inner".into()));
        assert_eq!(kinds("///\n")[0], Tok::DocOuter(String::new()));
        assert_eq!(
            kinds("///  two spaces\n")[0],
            Tok::DocOuter(" two spaces".into())
        );
        // Four slashes and `//` are ordinary comments.
        assert_eq!(kinds("//// banner\n"), [Tok::Newline, Tok::Eof]);
        assert_eq!(kinds("// plain\n"), [Tok::Newline, Tok::Eof]);
    }

    #[test]
    fn a_block_comment_is_an_error_at_its_opening() {
        for src in ["a /* x */ b", "/* never closed", "a /* x /* y */ z */ b"] {
            let e = lex(src).unwrap_err();
            assert_eq!(e.message, "block comments do not exist; use `//`", "{src}");
            let at = src.find("/*").unwrap();
            assert_eq!((e.span.start, e.span.end), (at, at + 2), "{src}");
        }
    }

    #[test]
    fn question_equals_is_one_token_and_a_lone_question_mark_is_not() {
        let toks: Vec<_> = lex(".f ?= 5V")
            .unwrap()
            .into_iter()
            .map(|t| t.tok)
            .collect();
        assert!(toks.contains(&Tok::QuestionEq), "{toks:?}");
        assert!(
            lex(".f ? 5V")
                .unwrap_err()
                .message
                .contains("unexpected character `?`")
        );
    }

    #[test]
    fn string_escapes() {
        let toks = lex(r#""a\"b\\c""#).unwrap();
        assert_eq!(toks[0].tok, Tok::Str("a\"b\\c".into()));
    }

    #[test]
    fn rejects_unterminated_string_and_stray_characters() {
        assert!(lex("\"abc").unwrap_err().message.contains("never closed"));
        assert!(
            lex("a @ b")
                .unwrap_err()
                .message
                .contains("unexpected character `@`")
        );
    }
}
