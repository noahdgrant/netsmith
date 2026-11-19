//! The formatter (issue #1103): one layout for `.nsm` source, what `netsmith project fmt` writes.
//!
//! It lays out the tokens, the plain comments and the doc comments of a file that parses, and
//! changes nothing else: the formatted text parses to the same syntax tree (the imports aside,
//! which it sorts) and holds the same comments, which [`format`] checks before it answers. The
//! rules are the guide's (`docs/language/25-comments.md`, _The layout of a file_):
//!
//! - 4 spaces of indent for each block, and 100 columns. A line that continues a statement (after
//!   a connection or relation operator, inside parentheses, a polygon's next corners) is indented
//!   one more than the statement's first line.
//! - The line breaks are the formatter's, as rustfmt makes them: a statement or a block written
//!   over several lines is joined onto one line when it fits (the statements of a block parted by
//!   `;`), and broken only when it does not. The body of a definition or of a section (`pins`,
//!   `lines`, `support`, ...) is always one statement to a line, and a statement or block with a
//!   comment, a doc comment or an attribute inside it stays broken (see [`join`]).
//! - A line longer than 100 columns is broken: a block on the line goes over several, one
//!   statement to a line (this is how a long `n x kind` statement breaks: in its block, never
//!   between the count and the kind); then a connection breaks after each `~`, `~>` or `?~`, a
//!   relation after its operator and its value after `+/-`, `+`, `-`, `*` or `/`; a polygon
//!   before a corner. A comment does not count, and is never broken.
//! - One space between tokens, none inside brackets, before `,`, `;`, `:` and `.` or around `::`
//!   and `..` (a range of rows, `.a .. .b`, keeps its spaces), and one space after `,` and `:`
//!   (a function's kind touches its name, `UART1_TX:output`). `±` is written `+/-`.
//! - The rows of a `pins` table are aligned in columns (the pin, its kind, its name, its pads),
//!   and so are the rows of a `lines` block (the line and its direction), over the rows that are
//!   not parted by a blank line.
//! - Two spaces before a comment at the end of a line.
//! - At most one blank line in a row, none at the start or end of a block, and one after the
//!   file's `//!` lines, after the imports and between two definitions.
//! - The imports at the head of the file are sorted as the editor's _Sort the imports_ sorts them:
//!   the standard library's, then other projects', then the project's own (`src::`), each group in
//!   the order of its paths, with one blank line between two groups. The names in an import's
//!   braces are sorted. A comment above an import moves with it.

use crate::ast::{DefKind, File};
use crate::error::{ParseError, Span};
use crate::lexer::{Tok, lex_keeping_comments};
use crate::parser::parse;

/// The columns a line may take.
pub const WIDTH: usize = 100;
/// The spaces of one level of indent.
pub const INDENT: usize = 4;

/// Why a text could not be formatted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatError {
    /// The text does not parse: the formatter lays out a program, and this is not one yet.
    Parse(ParseError),
    /// The layout would have changed what the text says. This is a fault of the formatter, and
    /// the text is left as it is.
    Changed(String),
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "{}", e.message),
            Self::Changed(why) => write!(
                f,
                "the formatter would change what the file says ({why}); the file is left as it is (a fault of netsmith: please report it)"
            ),
        }
    }
}

/// `src` in the formatter's layout. The answer parses to what `src` parses to, with the imports
/// sorted, and keeps every comment; formatting it again changes nothing.
pub fn format(src: &str) -> Result<String, FormatError> {
    let before = parse(src).map_err(FormatError::Parse)?;
    let out = layout(src).map_err(FormatError::Parse)?;
    let after = parse(&out)
        .map_err(|e| FormatError::Changed(format!("the result does not parse: {}", e.message)))?;
    if program(&before) != program(&after) {
        return Err(FormatError::Changed(
            "the result parses to another program".into(),
        ));
    }
    if comments(src) != comments(&out) {
        return Err(FormatError::Changed(
            "the result does not hold the same comments".into(),
        ));
    }
    Ok(out)
}

/// What a syntax tree says, apart from where it is written: its debug text with every span left
/// out, and the imports in the order the formatter sorts them.
fn program(file: &File) -> String {
    let mut file = file.clone();
    for import in &mut file.imports {
        if let Some(names) = &mut import.names {
            names.sort_by(|a, b| a.name.cmp(&b.name));
        }
    }
    file.imports.sort_by_key(|i| i.to_source());
    let text = format!("{file:?}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(at) = rest.find("Span { start: ") {
        out.push_str(&rest[..at]);
        let end = rest[at..].find('}').map_or(rest.len(), |e| at + e + 1);
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The plain comments of `src`, sorted (an import's comment moves with it).
fn comments(src: &str) -> Vec<String> {
    let mut out: Vec<String> = lex_keeping_comments(src)
        .map(|(_, spans)| {
            spans
                .iter()
                .map(|s| src[s.start..s.end].trim_end().to_string())
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// A token as the formatter sees it.
#[derive(Debug, Clone)]
struct T {
    /// `None` for a plain comment.
    tok: Option<Tok>,
    /// The text as written (a comment without the spaces after it).
    text: String,
    /// Whether the source has white space before it on its line.
    space: bool,
}

impl T {
    fn is(&self, tok: &Tok) -> bool {
        self.tok.as_ref() == Some(tok)
    }

    fn is_comment(&self) -> bool {
        self.tok.is_none()
    }

    fn is_doc(&self) -> bool {
        matches!(self.tok, Some(Tok::DocOuter(_) | Tok::DocInner(_)))
    }

    fn is_word(&self, word: &str) -> bool {
        matches!(&self.tok, Some(Tok::Ident(w)) if w == word)
    }

    fn opens(&self) -> bool {
        matches!(
            self.tok,
            Some(Tok::LBrace | Tok::LParen | Tok::LBracket | Tok::HashBracket)
        )
    }

    fn closes(&self) -> bool {
        matches!(self.tok, Some(Tok::RBrace | Tok::RParen | Tok::RBracket))
    }

    /// A word, a number, a string or a quoted name: what two of need a space between them.
    fn is_atom(&self) -> bool {
        matches!(
            self.tok,
            Some(Tok::Ident(_) | Tok::QuotedIdent(_) | Tok::Number(_) | Tok::Str(_))
        )
    }
}

/// An operator that the parser lets a statement go on after, at the end of a line.
fn continues(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::Tilde
            | Tok::TildeArrow
            | Tok::QuestionTilde
            | Tok::AmpEq
            | Tok::QuestionEq
            | Tok::EqEq
            | Tok::GtEq
            | Tok::LtEq
            | Tok::PlusMinus
            | Tok::Plus
            | Tok::Minus
            | Tok::Star
            | Tok::Slash
    )
}

fn is_connection(tok: &Tok) -> bool {
    matches!(tok, Tok::Tilde | Tok::TildeArrow | Tok::QuestionTilde)
}

fn is_relation(tok: &Tok) -> bool {
    matches!(
        tok,
        Tok::AmpEq | Tok::QuestionEq | Tok::EqEq | Tok::GtEq | Tok::LtEq
    )
}

/// An operator with a space on each side (`+` and `-` when they are not a sign).
fn is_binary(tok: &Tok) -> bool {
    is_connection(tok)
        || is_relation(tok)
        || matches!(
            tok,
            Tok::Eq | Tok::PlusMinus | Tok::Star | Tok::Slash | Tok::Plus | Tok::Minus
        )
}

/// A line of the result, before it is written.
#[derive(Debug, Clone)]
struct Line {
    toks: Vec<T>,
    /// The comment at its end, or the whole line when `toks` is empty.
    comment: Option<String>,
    /// A blank line before it in the source.
    blank: bool,
    /// It goes on with the statement of the line before.
    cont: bool,
    indent: usize,
    /// What the innermost block it is in begins with (`pins`, `lines`), when it
    /// is in a block of the source.
    block: Option<String>,
    /// The number of brackets open at its start.
    depth: usize,
    /// The number of brackets open at its end.
    depth_after: usize,
}

impl Line {
    fn code(toks: Vec<T>) -> Self {
        Self {
            toks,
            comment: None,
            blank: false,
            cont: false,
            indent: 0,
            block: None,
            depth: 0,
            depth_after: 0,
        }
    }

    fn first(&self) -> Option<&T> {
        self.toks.first()
    }
}

/// The text of `src` in the formatter's layout, unchecked.
fn layout(src: &str) -> Result<String, ParseError> {
    let lines = source_lines(src)?;
    let lines = split_blocks(lines);
    let lines = indent(join(&indent(lines)));
    let mut out: Vec<Line> = Vec::new();
    for line in lines {
        out.extend(break_long(line));
    }
    let rendered = align(&out);
    Ok(write(&out, rendered))
}

/// The tokens of `src` by the line of the source they are on, with the plain comments.
fn source_lines(src: &str) -> Result<Vec<Line>, ParseError> {
    let (tokens, comments) = lex_keeping_comments(src)?;
    let mut starts = vec![0usize];
    starts.extend(src.match_indices('\n').map(|(i, _)| i + 1));
    let line_of = |at: usize| starts.partition_point(|&s| s <= at) - 1;
    let mut all: Vec<(Span, Option<Tok>)> = tokens
        .into_iter()
        .filter(|t| !matches!(t.tok, Tok::Newline | Tok::Eof))
        .map(|t| (t.span, Some(t.tok)))
        .chain(comments.into_iter().map(|s| (s, None)))
        .collect();
    all.sort_by_key(|(s, _)| s.start);
    let mut lines: Vec<(usize, Line)> = Vec::new();
    for (span, tok) in all {
        let line = line_of(span.start);
        let before = &src[starts[line]..span.start];
        let space = before.ends_with([' ', '\t', '\r']);
        let raw = &src[span.start..span.end];
        let text = match &tok {
            None | Some(Tok::DocOuter(_) | Tok::DocInner(_)) => raw.trim_end().to_string(),
            Some(Tok::PlusMinus) => "+/-".to_string(),
            _ => raw.to_string(),
        };
        let t = T { tok, text, space };
        match lines.last_mut() {
            Some((l, last)) if *l == line => last.toks.push(t),
            _ => {
                let blank = lines.last().is_some_and(|(l, _)| {
                    src[starts[*l + 1]..starts[line]]
                        .lines()
                        .any(|text| text.trim().is_empty())
                });
                let mut fresh = Line::code(vec![t]);
                fresh.blank = blank;
                lines.push((line, fresh));
            }
        }
    }
    Ok(lines
        .into_iter()
        .map(|(_, mut line)| {
            if line.toks.last().is_some_and(T::is_comment) {
                line.comment = line.toks.pop().map(|t| t.text);
            }
            line
        })
        .collect())
}

/// Each statement of a block written over several lines on a line of its own: a line is split
/// after a `{` that is closed on a later line, before a `}` that was opened on an earlier one,
/// and at a `;` between two statements of such a block (or of the file). A `;` before a `}` and
/// at the end of a line goes, and so does a `,` at the end of an import's names on one line;
/// the names in an import's braces are sorted.
fn split_blocks(lines: Vec<Line>) -> Vec<Line> {
    // The line each bracket is closed on, by the place of its opener.
    let mut closed_on: Vec<Vec<Option<usize>>> =
        lines.iter().map(|l| vec![None; l.toks.len()]).collect();
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        for (j, t) in line.toks.iter().enumerate() {
            if t.opens() {
                stack.push((i, j));
            } else if t.closes()
                && let Some((oi, oj)) = stack.pop()
            {
                closed_on[oi][oj] = Some(i);
            }
        }
    }
    // Whether each opener still open at the start of a line is a `{` (a block over lines).
    let mut open: Vec<bool> = Vec::new();
    let mut out = Vec::new();
    for (i, line) in lines.into_iter().enumerate() {
        let mut pieces: Vec<Line> = Vec::new();
        let mut piece: Vec<T> = Vec::new();
        // The openers of this line still open, each with whether it closes on this line.
        let mut local: Vec<bool> = Vec::new();
        let toks = sort_import_names(line.toks);
        let mut toks = toks.into_iter().enumerate().peekable();
        let at_statement_level = |local: &Vec<bool>, open: &Vec<bool>| {
            local.is_empty() && open.last().is_none_or(|b| *b)
        };
        while let Some((j, t)) = toks.next() {
            if t.opens() {
                // The body of a definition or of a section is over lines, also when it was
                // written on one (an empty one, `{}`, aside).
                let item = t.is(&Tok::LBrace)
                    && opens_item(&piece)
                    && !toks.peek().is_some_and(|(_, next)| next.is(&Tok::RBrace));
                let here = closed_on[i][j] == Some(i) && !item;
                let block = t.is(&Tok::LBrace) && !here;
                if here {
                    local.push(true);
                    piece.push(t);
                } else {
                    open.push(block);
                    piece.push(t);
                    if block && toks.peek().is_some() {
                        pieces.push(Line::code(std::mem::take(&mut piece)));
                    }
                }
                continue;
            }
            if t.closes() {
                if local.pop().is_none() {
                    let block = open.pop().unwrap_or(false);
                    if block && !piece.is_empty() {
                        pieces.push(Line::code(std::mem::take(&mut piece)));
                    }
                }
                piece.push(t);
                continue;
            }
            if t.is(&Tok::Semi) {
                if at_statement_level(&local, &open) {
                    if !piece.is_empty() {
                        pieces.push(Line::code(std::mem::take(&mut piece)));
                    }
                    continue;
                }
                // A `;` before the `}` of a block on this line goes.
                if toks.peek().is_some_and(|(_, next)| next.is(&Tok::RBrace)) {
                    continue;
                }
            }
            piece.push(t);
        }
        if !piece.is_empty() {
            pieces.push(Line::code(piece));
        }
        if pieces.is_empty() {
            // A line with nothing but a comment (or `;`).
            if line.comment.is_none() {
                continue;
            }
            pieces.push(Line::code(Vec::new()));
        }
        pieces[0].blank = line.blank;
        if let Some(last) = pieces.last_mut() {
            last.comment = line.comment;
        }
        out.extend(pieces);
    }
    out
}

/// The words that begin a definition (`board`, `part`, ... and a physical file's `physical`) or
/// a section of one (`pins`, `pads`, `lines`, `support`, `layout`, `defaults`, `case`, `sequence`, `scenario`): a block whose
/// statements are always one to a line, as rustfmt lays out the body of a function.
fn is_item_word(word: &str) -> bool {
    DefKind::from_keyword(word).is_some()
        || matches!(
            word,
            "physical"
                | "pins"
                | "pads"
                | "lines"
                | "support"
                | "layout"
                | "defaults"
                | "case"
                | "sequence"
                | "scenario"
        )
}

/// Whether a `{` after the tokens `head` of a statement opens the body of a definition or of a
/// section: whether the statement, past its attributes (`#[series(E24)]`), begins with one of
/// their words.
fn opens_item(head: &[T]) -> bool {
    let mut k = 0;
    while head.get(k).is_some_and(|t| t.is(&Tok::HashBracket)) {
        let mut depth = 0usize;
        while let Some(t) = head.get(k) {
            k += 1;
            if t.opens() {
                depth += 1;
            } else if t.closes() {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
        }
    }
    // `.variants { ... }`, a part's table of order codes (issue 1311): one row to a line.
    if head.get(k).is_some_and(|t| t.is(&Tok::LeadingDot))
        && head
            .get(k + 1)
            .is_some_and(|t| matches!(&t.tok, Some(Tok::Ident(w)) if w == "variants"))
    {
        return true;
    }
    head.get(k)
        .is_some_and(|t| matches!(&t.tok, Some(Tok::Ident(w)) if is_item_word(w)))
}

/// Each statement and block written over several lines joined onto one line (issue #1103: the
/// line breaks are the formatter's, not the author's), which [`break_long`] breaks again when it
/// is longer than [`WIDTH`], as it breaks a line written so: what fits is on one line, and what
/// does not is laid out the same however it was written. What is not joined: the body of a
/// definition or of a section, and a statement or block with a comment, a doc comment or an
/// attribute inside it (a comment is never moved onto a line with code); a block that is not
/// joined keeps its `{` line and its `}` line, and what is in it is joined in its turn.
fn join(lines: &[Line]) -> Vec<Line> {
    let mut out = Vec::with_capacity(lines.len());
    join_range(lines, &mut out);
    out
}

fn join_range(lines: &[Line], out: &mut Vec<Line>) {
    let mut k = 0;
    while k < lines.len() {
        // The statement that begins on line `k`: up to the line that closes what it opens, and
        // the lines that go on with it.
        let depth = lines[k].depth;
        let mut e = k;
        while e + 1 < lines.len() && (lines[e].depth_after > depth || lines[e + 1].cont) {
            e += 1;
        }
        let unit = &lines[k..=e];
        if e > k
            && let Some((joined, in_parens)) = joined(unit)
            && (!in_parens || width(&joined) <= WIDTH)
        {
            // Joined however long: `break_long` breaks it again where the width needs, as it
            // breaks a line written so. Only a line end inside parentheses, which it does not
            // break, is kept when the line would be too long.
            out.push(joined);
            k = e + 1;
            continue;
        }
        out.push(lines[k].clone());
        join_range(&lines[k + 1..=e], out);
        k = e + 1;
    }
}

/// The lines of one statement on one line, with a `;` between two statements of a block, and
/// whether a line end was inside parentheses or brackets; `None` when they may not be joined.
fn joined(unit: &[Line]) -> Option<(Line, bool)> {
    let last = unit.len() - 1;
    for (n, line) in unit.iter().enumerate() {
        let first = line.first()?;
        if (n < last && line.comment.is_some())
            || first.is(&Tok::HashBracket)
            || line.toks.iter().any(T::is_doc)
            || (line.toks.last().is_some_and(|t| t.is(&Tok::LBrace)) && opens_item(&line.toks))
        {
            return None;
        }
    }
    let mut toks: Vec<T> = Vec::new();
    // The brackets open, each with whether it is a `{` (whose statements a `;` parts).
    let mut open: Vec<bool> = Vec::new();
    let mut in_parens = false;
    for (n, line) in unit.iter().enumerate() {
        if n > 0 {
            in_parens |= open.last() == Some(&false);
            let prev = toks.last().expect("a line of code has a token");
            let next = &line.toks[0];
            let between_statements = open.last() == Some(&true)
                && !line.cont
                && !prev.opens()
                && !prev.is(&Tok::Comma)
                && !prev.tok.as_ref().is_some_and(continues)
                && !next.closes()
                && !next.is(&Tok::Comma);
            if between_statements {
                toks.push(T {
                    tok: Some(Tok::Semi),
                    text: ";".into(),
                    space: false,
                });
            }
        }
        for (j, t) in line.toks.iter().enumerate() {
            if t.opens() {
                open.push(t.is(&Tok::LBrace));
            } else if t.closes() {
                open.pop();
            }
            let mut t = t.clone();
            if j == 0 && n > 0 {
                // A line end is white space.
                t.space = true;
            }
            toks.push(t);
        }
    }
    let line = Line {
        toks: sort_import_names(toks),
        comment: unit[last].comment.clone(),
        depth_after: unit[last].depth_after,
        ..unit[0].clone()
    };
    Some((line, in_parens))
}

/// The names in the braces of an import on one line, sorted, with no `,` after the last.
fn sort_import_names(toks: Vec<T>) -> Vec<T> {
    if !toks.first().is_some_and(|t| t.is_word("import")) {
        return toks;
    }
    let Some(open) = toks.iter().position(|t| t.is(&Tok::LBrace)) else {
        return toks;
    };
    let Some(close) = toks.iter().position(|t| t.is(&Tok::RBrace)) else {
        return toks;
    };
    if close < open
        || toks[open + 1..close]
            .iter()
            .any(|t| t.opens() || t.closes())
    {
        return toks;
    }
    let mut names: Vec<Vec<T>> = toks[open + 1..close]
        .split(|t| t.is(&Tok::Comma))
        .filter(|n| !n.is_empty())
        .map(<[T]>::to_vec)
        .collect();
    names.sort_by_key(|n| n.iter().map(|t| t.text.clone()).collect::<Vec<_>>());
    let comma = toks[open + 1..close]
        .iter()
        .find(|t| t.is(&Tok::Comma))
        .cloned();
    let mut out: Vec<T> = toks[..=open].to_vec();
    for (k, name) in names.into_iter().enumerate() {
        if k > 0 {
            out.push(comma.clone().expect("two names have a comma between them"));
        }
        out.extend(name);
    }
    out.extend(toks[close..].iter().cloned());
    out
}

/// The indent of every line, and what it is in.
fn indent(mut lines: Vec<Line>) -> Vec<Line> {
    // Each open bracket: the token, the indent of the line it is on, and what that line begins with.
    let mut stack: Vec<(Tok, usize, Option<String>)> = Vec::new();
    // The indent of the statement the last line of code began, and whether that statement is a
    // polygon (whose next corners go on over lines that begin with `(`, `round` or `arc`).
    let mut statement = 0usize;
    let mut polygon = false;
    let mut last_code: Option<Tok> = None;
    for line in &mut lines {
        line.depth = stack.len();
        let Some(first) = line.first().cloned() else {
            // A comment alone: at the indent of what follows it in its block.
            line.indent = match stack.last() {
                Some((_, indent, _)) => indent + 1,
                None => 0,
            };
            line.block = stack.last().and_then(|(_, _, b)| b.clone());
            line.depth_after = stack.len();
            continue;
        };
        let in_parens = stack
            .last()
            .is_some_and(|(t, _, _)| !matches!(t, Tok::LBrace));
        let after_operator = last_code.as_ref().is_some_and(continues);
        let corner =
            polygon && (first.is(&Tok::LParen) || first.is_word("round") || first.is_word("arc"));
        let base = if first.closes() {
            stack.last().map_or(0, |(_, indent, _)| *indent)
        } else {
            stack.last().map_or(0, |(_, indent, _)| indent + 1)
        };
        line.cont = !first.closes() && !in_parens && (after_operator || corner);
        line.indent = if line.cont { statement + 1 } else { base };
        if !line.cont {
            statement = line.indent;
            polygon = first.is_word("polygon");
        }
        line.block = stack.last().and_then(|(_, _, b)| b.clone());
        let begins = line
            .toks
            .iter()
            .find(|t| !t.closes())
            .and_then(|t| match &t.tok {
                Some(Tok::Ident(w)) => Some(w.clone()),
                _ => None,
            });
        for t in &line.toks {
            if t.opens() {
                let tok = t.tok.clone().expect("a bracket is a token");
                stack.push((tok, line.indent, begins.clone()));
            } else if t.closes() {
                stack.pop();
            }
        }
        line.depth_after = stack.len();
        last_code = line.toks.last().and_then(|t| t.tok.clone());
    }
    lines
}

/// Where a line is, for the spaces between its tokens.
#[derive(Debug, Clone, Copy)]
struct Ctx {
    /// An import, whose braces hold names with no space inside them: `{Resistor, Led}`.
    import: bool,
}

/// Whether there is a space before `toks[i]`, which is not the first token of its line.
fn space(toks: &[T], i: usize, ctx: Ctx) -> bool {
    let (prev, next) = (&toks[i - 1], &toks[i]);
    let after = toks.get(i + 1);
    let prev_sign = is_sign(toks, i - 1);
    let next_sign = is_sign(toks, i);
    let (Some(p), Some(n)) = (&prev.tok, &next.tok) else {
        return true;
    };
    if ctx.import && (matches!(p, Tok::LBrace) || matches!(n, Tok::RBrace)) {
        return false;
    }
    if matches!(p, Tok::LParen | Tok::LBracket | Tok::HashBracket) {
        return false;
    }
    if matches!(n, Tok::RParen | Tok::RBracket | Tok::Comma | Tok::Semi) {
        return false;
    }
    if matches!(p, Tok::Comma | Tok::Semi) {
        return true;
    }
    if matches!(p, Tok::ColonColon) || matches!(n, Tok::ColonColon) {
        return false;
    }
    if matches!(n, Tok::Colon) {
        return false;
    }
    if matches!(p, Tok::Colon) {
        // A function's kind touches its name, `UART1_TX:output`; the type of a member or of a
        // type parameter does not.
        let member = i >= 3
            && matches!(
                toks[i - 3].tok,
                Some(Tok::LeadingDot | Tok::Dot | Tok::Lt | Tok::Comma)
            );
        return member || next.space;
    }
    if matches!(n, Tok::Dot) || matches!(p, Tok::Dot | Tok::LeadingDot) {
        return false;
    }
    if matches!(p, Tok::Lt) || matches!(n, Tok::Lt | Tok::Gt) {
        return false;
    }
    if prev_sign {
        return false;
    }
    if is_binary(p) {
        return true;
    }
    if is_binary(n) && !next_sign {
        return true;
    }
    if matches!(n, Tok::LeadingDot) {
        return true;
    }
    if matches!(p, Tok::DotDot) {
        // A range open at its top touches its `..`, and its block's `}` does not: `{ .x &= 1V.. }`.
        return matches!(n, Tok::RBrace);
    }
    if matches!(n, Tok::DotDot) {
        return after.is_some_and(|a| a.is(&Tok::LeadingDot));
    }
    if matches!(p, Tok::LBrace) {
        return !matches!(n, Tok::RBrace);
    }
    if matches!(n, Tok::LBrace | Tok::RBrace) || matches!(p, Tok::RBrace) {
        return true;
    }
    if matches!(n, Tok::LParen) {
        return !matches!(p, Tok::Ident(_)) || next.space;
    }
    if matches!(n, Tok::LBracket) {
        return !(matches!(p, Tok::Ident(_) | Tok::RBracket)) && next.space;
    }
    true
}

/// Whether the `+` or `-` at `i` is a sign rather than an operator between two values.
fn is_sign(toks: &[T], i: usize) -> bool {
    if !matches!(toks[i].tok, Some(Tok::Plus | Tok::Minus)) {
        return false;
    }
    let Some(prev) = i.checked_sub(1).map(|k| &toks[k]) else {
        return true;
    };
    let value_before = prev.is_atom() || prev.closes() || prev.is(&Tok::Gt);
    if !value_before {
        return true;
    }
    // After a word, a sign is written touching its value and apart from the word (`at -40C`).
    let touches_next = toks.get(i + 1).is_some_and(|n| !n.space);
    matches!(prev.tok, Some(Tok::Ident(_))) && toks[i].space && touches_next
}

/// The text of `toks` on one line, without indent, and where each token starts in it.
fn render(toks: &[T]) -> (String, Vec<bool>) {
    let ctx = Ctx {
        import: toks.first().is_some_and(|t| t.is_word("import")),
    };
    let mut out = String::new();
    let mut spaces = Vec::with_capacity(toks.len());
    for (i, t) in toks.iter().enumerate() {
        let gap = i > 0 && space(toks, i, ctx);
        if gap {
            out.push(' ');
        }
        spaces.push(gap);
        out.push_str(&t.text);
    }
    (out, spaces)
}

fn width(line: &Line) -> usize {
    line.indent * INDENT + render(&line.toks).0.chars().count()
}

/// `line`, over several lines when it is longer than [`WIDTH`].
fn break_long(line: Line) -> Vec<Line> {
    if line.toks.is_empty() || width(&line) <= WIDTH || is_row(&line) {
        return vec![line];
    }
    if let Some(lines) = expand_block(&line) {
        return lines.into_iter().flat_map(break_long).collect();
    }
    if line.first().is_some_and(|t| t.is_word("polygon")) {
        return fill(
            line,
            |toks, k| toks[k].is(&Tok::LParen) && !toks[k - 1].is_word("through"),
            false,
        );
    }
    let top = top_level(&line.toks);
    let connection = top
        .iter()
        .any(|&k| line.toks[k].tok.as_ref().is_some_and(is_connection));
    let pieces = if connection {
        split_after(&line, is_connection)
    } else if let Some(&k) = top
        .iter()
        .find(|&&k| line.toks[k].tok.as_ref().is_some_and(is_relation))
    {
        let mut rest = line.clone();
        let head: Vec<T> = rest.toks.drain(..=k).collect();
        vec![head, rest.toks]
    } else {
        vec![line.toks.clone()]
    };
    if pieces.len() < 2 {
        return vec![line];
    }
    let mut out = Vec::new();
    let count = pieces.len();
    for (k, toks) in pieces.into_iter().enumerate() {
        let mut piece = Line {
            toks,
            comment: None,
            blank: k == 0 && line.blank,
            cont: k > 0 || line.cont,
            indent: if k == 0 {
                line.indent
            } else {
                statement_indent(&line) + 1
            },
            ..line.clone()
        };
        if k == count - 1 {
            piece.comment = line.comment.clone();
        }
        if k > 0 && width(&piece) > WIDTH {
            out.extend(fill(
                piece,
                |toks, k| {
                    toks[k - 1].tok.as_ref().is_some_and(|t| {
                        matches!(
                            t,
                            Tok::PlusMinus | Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash
                        )
                    }) && !is_sign(toks, k - 1)
                },
                true,
            ));
        } else {
            out.push(piece);
        }
    }
    out
}

/// The indent of the first line of the statement `line` is in.
fn statement_indent(line: &Line) -> usize {
    if line.cont {
        line.indent.saturating_sub(1)
    } else {
        line.indent
    }
}

/// The places of the tokens of `toks` that are in no bracket opened on the line.
fn top_level(toks: &[T]) -> Vec<usize> {
    let mut depth = 0usize;
    let mut out = Vec::new();
    for (k, t) in toks.iter().enumerate() {
        if t.closes() {
            depth = depth.saturating_sub(1);
        }
        if depth == 0 {
            out.push(k);
        }
        if t.opens() {
            depth += 1;
        }
    }
    out
}

/// The tokens of `line` in pieces, each ending after a top-level operator `is`.
fn split_after(line: &Line, is: impl Fn(&Tok) -> bool) -> Vec<Vec<T>> {
    let top = top_level(&line.toks);
    let mut pieces = vec![Vec::new()];
    for (k, t) in line.toks.iter().enumerate() {
        pieces.last_mut().expect("a piece").push(t.clone());
        if top.contains(&k) && t.tok.as_ref().is_some_and(&is) && k + 1 < line.toks.len() {
            pieces.push(Vec::new());
        }
    }
    pieces
}

/// `line` broken at the top-level places `can_break` allows, as few times as the width needs;
/// the lines after the first go on with its statement.
fn fill(line: Line, can_break: impl Fn(&[T], usize) -> bool, already_cont: bool) -> Vec<Line> {
    let top = top_level(&line.toks);
    let mut out: Vec<Line> = Vec::new();
    let mut current = Line {
        toks: Vec::new(),
        comment: None,
        ..line.clone()
    };
    let next_indent = if already_cont {
        line.indent
    } else {
        statement_indent(&line) + 1
    };
    for (k, t) in line.toks.iter().enumerate() {
        if k > 0 && top.contains(&k) && can_break(&line.toks, k) && !current.toks.is_empty() {
            let mut trial = current.clone();
            // The rest of the line up to the next place a break is allowed.
            let mut end = k + 1;
            while end < line.toks.len() && !(top.contains(&end) && can_break(&line.toks, end)) {
                end += 1;
            }
            trial.toks.extend(line.toks[k..end].iter().cloned());
            if width(&trial) > WIDTH {
                out.push(std::mem::replace(
                    &mut current,
                    Line {
                        toks: Vec::new(),
                        comment: None,
                        blank: false,
                        cont: true,
                        indent: next_indent,
                        ..line.clone()
                    },
                ));
            }
        }
        current.toks.push(t.clone());
    }
    current.comment = line.comment.clone();
    out.push(current);
    out
}

/// A block written on `line` in braces, over several lines: the line up to its `{`, each of its
/// statements one level in (or, for a list of names or columns parted by `,`, as many of them to
/// a line as fit), and its `}` with the rest of the line. `None` when the line has no such block.
fn expand_block(line: &Line) -> Option<Vec<Line>> {
    let toks = &line.toks;
    let top = top_level(toks);
    let open = *top.iter().find(|&&k| toks[k].is(&Tok::LBrace))?;
    let mut depth = 0usize;
    let mut close = None;
    for (k, t) in toks.iter().enumerate().skip(open) {
        if t.opens() {
            depth += 1;
        } else if t.closes() {
            depth -= 1;
            if depth == 0 {
                close = Some(k);
                break;
            }
        }
    }
    let close = close?;
    let inner = &toks[open + 1..close];
    let inner_top: Vec<usize> = top_level(inner);
    let statements = inner_top.iter().any(|&k| {
        inner[k].tok.as_ref().is_some_and(|t| {
            matches!(t, Tok::Semi | Tok::Colon | Tok::Eq) || is_connection(t) || is_relation(t)
        })
    });
    let list = !statements && inner_top.iter().any(|&k| inner[k].is(&Tok::Comma));
    if inner.is_empty() {
        return None;
    }
    let mut out = vec![Line {
        toks: toks[..=open].to_vec(),
        comment: None,
        ..line.clone()
    }];
    if list {
        // A list (a catalog's columns, an import's names): as many to a line as fit, broken
        // after a `,`.
        out.extend(fill(
            inner_line(line, inner.to_vec()),
            |toks, k| toks[k - 1].is(&Tok::Comma),
            true,
        ));
        out.push(Line {
            toks: toks[close..].to_vec(),
            comment: line.comment.clone(),
            blank: false,
            cont: false,
            indent: line.indent,
            ..line.clone()
        });
        return Some(out);
    }
    let mut piece = Vec::new();
    for (k, t) in inner.iter().enumerate() {
        if inner_top.contains(&k) && t.is(&Tok::Semi) {
            if !piece.is_empty() {
                out.push(inner_line(line, std::mem::take(&mut piece)));
            }
            continue;
        }
        piece.push(t.clone());
    }
    if !piece.is_empty() {
        out.push(inner_line(line, piece));
    }
    out.push(Line {
        toks: toks[close..].to_vec(),
        comment: line.comment.clone(),
        blank: false,
        cont: false,
        indent: line.indent,
        ..line.clone()
    });
    Some(out)
}

fn inner_line(line: &Line, toks: Vec<T>) -> Line {
    Line {
        toks,
        comment: None,
        blank: false,
        cont: false,
        indent: line.indent + 1,
        block: line.first().and_then(|t| match &t.tok {
            Some(Tok::Ident(w)) => Some(w.clone()),
            _ => None,
        }),
        ..line.clone()
    }
}

/// A row of a `pins` table or of a `lines` block.
fn is_row(line: &Line) -> bool {
    !line.cont
        && matches!(line.block.as_deref(), Some("pins" | "lines"))
        && line.first().is_some_and(|t| t.is(&Tok::LeadingDot))
}

/// The columns of a row: the pin, its kind, its name and its pads (one column), and the rest; or,
/// in a `lines` block, the line, its direction and the rest.
fn columns(line: &Line) -> Vec<String> {
    let (text, spaces) = render(&line.toks);
    // The cells: the tokens with no space between them.
    let mut cells: Vec<(String, bool)> = Vec::new();
    let mut at = 0usize;
    for (k, t) in line.toks.iter().enumerate() {
        let len = t.text.len() + usize::from(spaces[k]);
        let piece = &text[at + usize::from(spaces[k])..at + len];
        if spaces[k] || cells.is_empty() {
            cells.push((piece.to_string(), matches!(t.tok, Some(Tok::Str(_)))));
        } else {
            let cell = cells.last_mut().expect("a cell");
            cell.0.push_str(piece);
            cell.1 = false;
        }
        at += len;
    }
    let mut cols: Vec<String> = Vec::new();
    let mut rest = cells.into_iter().peekable();
    for _ in 0..2 {
        if let Some((cell, _)) = rest.next() {
            cols.push(cell);
        }
    }
    if line.block.as_deref() == Some("pins")
        && let Some((name, _)) = rest.next_if(|(_, s)| *s)
    {
        cols.push(name);
        let mut pads = Vec::new();
        while let Some((pad, _)) = rest.next_if(|(_, s)| *s) {
            pads.push(pad);
        }
        if !pads.is_empty() {
            cols.push(pads.join(" "));
        }
    }
    let tail: Vec<String> = rest.map(|(c, _)| c).collect();
    if !tail.is_empty() {
        cols.push(tail.join(" "));
    }
    cols
}

/// The text of each line without its indent and comment: the rows of each table in columns.
fn align(lines: &[Line]) -> Vec<String> {
    let mut out: Vec<String> = lines.iter().map(|l| render(&l.toks).0).collect();
    let mut i = 0;
    while i < lines.len() {
        if !is_row(&lines[i]) {
            i += 1;
            continue;
        }
        // A run: rows of one block, with comments alone between them and no blank line.
        let mut run = vec![i];
        let mut k = i + 1;
        while k < lines.len() && !lines[k].blank {
            if is_row(&lines[k]) && lines[k].indent == lines[i].indent {
                run.push(k);
            } else if !lines[k].toks.is_empty() {
                break;
            }
            k += 1;
        }
        let rows: Vec<Vec<String>> = run.iter().map(|&r| columns(&lines[r])).collect();
        let most = rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut widths = vec![0usize; most];
        for row in &rows {
            for (c, cell) in row.iter().enumerate().take(row.len().saturating_sub(1)) {
                widths[c] = widths[c].max(cell.chars().count());
            }
        }
        for (&r, row) in run.iter().zip(&rows) {
            let mut text = String::new();
            for (c, cell) in row.iter().enumerate() {
                text.push_str(cell);
                if c + 1 < row.len() {
                    let pad = widths[c] - cell.chars().count();
                    text.push_str(&" ".repeat(pad + 1));
                }
            }
            out[r] = text;
        }
        i = k;
    }
    out
}

/// The lines written out: their indent and text, the comments, the blank lines, and the imports
/// sorted.
fn write(lines: &[Line], text: Vec<String>) -> String {
    let mut written: Vec<(String, bool)> = Vec::new();
    // What each written line is, for the blank lines and the imports.
    let mut kinds: Vec<Kind> = Vec::new();
    for (k, (line, code)) in lines.iter().zip(text).enumerate() {
        let pad = " ".repeat(line.indent * INDENT);
        let body = match (&line.comment, code.is_empty()) {
            (Some(c), true) => c.clone(),
            (Some(c), false) => format!("{code}  {c}"),
            (None, _) => code,
        };
        let kind = kind(line);
        let prev = k.checked_sub(1).map(|p| &lines[p]);
        let mut blank = line.blank && prev.is_some();
        if let Some(prev) = prev {
            let opened = prev.depth_after > prev.depth
                && prev.toks.last().is_some_and(|t| t.is(&Tok::LBrace));
            if opened || line.first().is_some_and(T::closes) {
                blank = false;
            }
            if line.depth == 0 && blank_required(&kinds, lines, k) {
                blank = true;
            }
        }
        written.push((format!("{pad}{body}").trim_end().to_string(), blank));
        kinds.push(kind);
    }
    sort_imports(&mut written, &kinds);
    let mut out = String::new();
    for (k, (text, blank)) in written.iter().enumerate() {
        if *blank && k > 0 {
            out.push('\n');
        }
        out.push_str(text);
        out.push('\n');
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A `//!` line of the file.
    FileDoc,
    /// A line that begins an import.
    Import,
    /// A comment alone, or a `///` line, or an attribute: what goes with the line after it.
    Leading,
    /// Any other line.
    Code,
}

fn kind(line: &Line) -> Kind {
    match line.first() {
        None => Kind::Leading,
        Some(t) if line.depth == 0 && matches!(t.tok, Some(Tok::DocInner(_))) => Kind::FileDoc,
        Some(t) if t.is_doc() || t.is(&Tok::HashBracket) => Kind::Leading,
        Some(t) if line.depth == 0 && t.is_word("import") => Kind::Import,
        Some(_) => Kind::Code,
    }
}

/// Whether the top-level line `k` must have a blank line before it: after the file's `//!` lines,
/// after the last import, and after a definition.
fn blank_required(kinds: &[Kind], lines: &[Line], k: usize) -> bool {
    let prev = &lines[k - 1];
    let here = kind(&lines[k]);
    let before = kinds[k - 1];
    if before == Kind::FileDoc {
        return here != Kind::FileDoc;
    }
    if before == Kind::Import && prev.depth_after == 0 {
        // The next line of code: an import goes on with the run.
        let next = lines[k..].iter().map(kind).find(|k| *k != Kind::Leading);
        return next != Some(Kind::Import);
    }
    // The end of a definition (a line that closes a block back to the top level, or one written
    // on one line with its braces).
    prev.depth_after == 0
        && prev.toks.iter().any(|t| t.is(&Tok::RBrace))
        && !prev.first().is_some_and(|t| t.is_word("import"))
}

/// The group of an import's path, as the editor's _Sort the imports_ has it
/// (`crates/netsmith-editor/web/src/source/imports.ts`): 0 for `std`, 2 for `src`, 1 for another
/// project's.
fn import_group(path: &str) -> u8 {
    match path.split("::").next() {
        Some("std") => 0,
        Some("src") => 2,
        _ => 1,
    }
}

/// `import a::b::{C, D}` is `a::b`, and `import a::b::C` is `a::b::C`: what orders it.
fn import_path(line: &str) -> String {
    let rest = line
        .trim_start()
        .strip_prefix("import")
        .unwrap_or(line)
        .trim_start();
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .unwrap_or(rest.len());
    rest[..end].trim_end_matches(':').to_string()
}

/// The imports at the head of the file sorted as the editor's _Sort the imports_ sorts them: the
/// standard library's, then other projects', then the project's own, each group in the order of
/// its paths, one blank line between two groups and none inside one. The comments just above an
/// import move with it; a comment with a blank line between it and the next import ends the
/// imports there (before the first import, it stays where it is).
fn sort_imports(written: &mut [(String, bool)], kinds: &[Kind]) {
    struct Entry {
        lines: Vec<String>,
        path: String,
        group: u8,
    }
    let mut entries: Vec<Entry> = Vec::new();
    let mut first: Option<usize> = None;
    let mut end = 0;
    let mut pending: Vec<String> = Vec::new();
    let mut pending_from: Option<usize> = None;
    let mut i = 0;
    while i < written.len() {
        let (text, blank) = &written[i];
        if *blank {
            if !pending.is_empty() && first.is_some() {
                break;
            }
            pending.clear();
            pending_from = None;
        }
        match kinds[i] {
            Kind::FileDoc if first.is_none() => i += 1,
            Kind::Import => {
                first.get_or_insert(pending_from.unwrap_or(i));
                let mut body = vec![text.clone()];
                // A list of names over several lines, up to its `}`.
                if text.contains('{') && !text.contains('}') {
                    while i + 1 < written.len() {
                        i += 1;
                        body.push(written[i].0.clone());
                        if written[i].0.contains('}') {
                            break;
                        }
                    }
                }
                let path = import_path(text);
                entries.push(Entry {
                    lines: pending.drain(..).chain(body).collect(),
                    group: import_group(&path),
                    path,
                });
                pending_from = None;
                i += 1;
                end = i;
            }
            Kind::Leading
                if text.trim_start().starts_with("//") && !text.trim_start().starts_with("///") =>
            {
                pending_from.get_or_insert(i);
                pending.push(text.clone());
                i += 1;
            }
            _ => break,
        }
    }
    let Some(first) = first else {
        return;
    };
    entries.sort_by(|a, b| (a.group, &a.path).cmp(&(b.group, &b.path)));
    let blank = written[first].1;
    let mut lines: Vec<(String, bool)> = Vec::new();
    for (n, entry) in entries.iter().enumerate() {
        let apart = n > 0 && entry.group != entries[n - 1].group;
        for (k, line) in entry.lines.iter().enumerate() {
            lines.push((line.clone(), k == 0 && apart));
        }
    }
    lines[0].1 = blank;
    for (j, line) in lines.into_iter().enumerate() {
        written[first + j] = line;
    }
    debug_assert!(first + entries.iter().map(|e| e.lines.len()).sum::<usize>() == end);
}
