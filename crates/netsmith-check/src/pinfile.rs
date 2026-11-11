//! The pin file beside a source file: `<stem>.pins.toml` (ADR 0101).
//!
//! It holds the solution of the pin requests of the source (`.status: .u.GPIO`): for each
//! request, the pin the tool picked, and whether a person locked it. It is plain TOML, optional,
//! `version = 1`, one table for each board keyed by the board's definition name, the entries
//! keyed by the request's path. It is written the same way every time, so that
//! `PinFile::parse(&file.to_text())` gives `file` again and a run that changes nothing leaves
//! the file's bytes alone.
//!
//! This module only reads and writes the text. What an entry means is decided in `assign.rs`.

use std::collections::BTreeMap;
use std::ops::Range;

use netsmith_lang::Span;
use toml::Spanned;
use toml::de::{DeInteger, DeTable, DeValue};

/// The version of the format this reads and writes.
pub const VERSION: i64 = 1;

/// Who chose an entry's pin: `"hand"` for a person, `"auto"` for the tool. Kept as written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntrySource {
    Hand,
    Auto,
}

impl EntrySource {
    pub fn word(self) -> &'static str {
        match self {
            Self::Hand => "hand",
            Self::Auto => "auto",
        }
    }
}

/// The pin of one request.
#[derive(Debug, Clone)]
pub struct PinEntry {
    /// The pin's path in the part (`p02`), not its pad: the pad changes with the package.
    pub pin: String,
    /// The function the request asked for, which the pin offers (`GPIO`).
    pub function: String,
    /// A promise to firmware: a run never changes the pin, and a lock that no longer holds is
    /// an error.
    pub lock: bool,
    pub source: EntrySource,
    /// Where the entry's table starts in the file it was read from; `None` for one made by a
    /// run. Not part of what the entry says, so it is left out of `==`.
    pub at: Option<Span>,
}

impl PartialEq for PinEntry {
    fn eq(&self, other: &Self) -> bool {
        self.pin == other.pin
            && self.function == other.function
            && self.lock == other.lock
            && self.source == other.source
    }
}

impl Eq for PinEntry {}

/// The entries of a file: board name, then request path, each in byte order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PinFile {
    pub boards: BTreeMap<String, BTreeMap<String, PinEntry>>,
}

/// One thing wrong with a file's text, at a byte range of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinFileError {
    pub span: Span,
    pub message: String,
}

impl PinFile {
    /// Whether no board has an entry.
    pub fn is_empty(&self) -> bool {
        self.boards.values().all(BTreeMap::is_empty)
    }

    /// The entries of one board.
    pub fn board(&self, name: &str) -> Option<&BTreeMap<String, PinEntry>> {
        self.boards.get(name)
    }

    /// Read the text of a pin file. An empty file, or one of comments, has no entries. Every
    /// fault is returned, in the order of the file; nothing is left half read.
    pub fn parse(text: &str) -> Result<PinFile, Vec<PinFileError>> {
        let table = match DeTable::parse(text) {
            Ok(t) => t,
            Err(e) => {
                let span = e.span().unwrap_or(0..0);
                return Err(vec![PinFileError {
                    span: Span::new(span.start, span.end),
                    message: e.message().to_string(),
                }]);
            }
        };
        let mut reader = Reader {
            errors: Vec::new(),
            file: PinFile::default(),
        };
        reader.document(table.get_ref());
        if reader.errors.is_empty() {
            Ok(reader.file)
        } else {
            let mut errors = reader.errors;
            errors.sort_by(|a, b| (a.span.start, &a.message).cmp(&(b.span.start, &b.message)));
            Err(errors)
        }
    }

    /// The text of the file, the same for the same entries: `version`, then the boards in name
    /// order, in each the entries by key in byte order, with `lock` only when it is true.
    pub fn to_text(&self) -> String {
        let mut out = format!("version = {VERSION}\n");
        for (board, entries) in &self.boards {
            for (key, e) in entries {
                out.push_str(&format!("\n[{}.pins.{}]\n", toml_key(board), toml_key(key)));
                out.push_str(&format!("pin = {}\n", toml_string(&e.pin)));
                out.push_str(&format!("function = {}\n", toml_string(&e.function)));
                if e.lock {
                    out.push_str("lock = true\n");
                }
                out.push_str(&format!("source = {}\n", toml_string(e.source.word())));
            }
        }
        out
    }
}

/// A key as TOML writes it: bare when it can be, else quoted (`"hub.scl"`).
pub(crate) fn toml_key(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if bare {
        key.to_string()
    } else {
        toml_string(key)
    }
}

pub(crate) fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct Reader {
    errors: Vec<PinFileError>,
    file: PinFile,
}

impl Reader {
    fn fail(&mut self, span: Range<usize>, message: String) {
        self.errors.push(PinFileError {
            span: Span::new(span.start, span.end),
            message,
        });
    }

    fn document(&mut self, table: &DeTable<'_>) {
        let mut versioned = false;
        let mut first_board: Option<Range<usize>> = None;
        for (key, value) in table.iter() {
            let name = key.get_ref().as_ref();
            if name == "version" {
                versioned = true;
                match value.get_ref() {
                    DeValue::Integer(i) if int_of(i) == Some(VERSION) => {}
                    other => {
                        let found = describe(other);
                        self.fail(
                            value.span(),
                            format!("`version` is {found}, and this reads only version {VERSION}"),
                        );
                    }
                }
                continue;
            }
            first_board.get_or_insert(key.span());
            let DeValue::Table(board) = value.get_ref() else {
                let found = describe(value.get_ref());
                self.fail(
                    value.span(),
                    format!("`{name}` is {found}; a board is a table, `[{name}.pins.<request>]`"),
                );
                continue;
            };
            let entries = self.board(name, board);
            if !entries.is_empty() {
                self.file.boards.insert(name.to_string(), entries);
            }
        }
        if let Some(span) = first_board
            && !versioned
        {
            self.fail(
                span,
                format!("a pin file starts with `version = {VERSION}`; it has no `version`"),
            );
        }
    }

    fn board(&mut self, name: &str, table: &DeTable<'_>) -> BTreeMap<String, PinEntry> {
        let mut entries = BTreeMap::new();
        for (key, value) in table.iter() {
            let k = key.get_ref().as_ref();
            if k != "pins" {
                self.fail(
                    key.span(),
                    format!(
                        "unknown key `{k}` in the table of board `{name}`; the only key is `pins`"
                    ),
                );
                continue;
            }
            let DeValue::Table(pins) = value.get_ref() else {
                let found = describe(value.get_ref());
                self.fail(
                    value.span(),
                    format!("`{name}.pins` is {found}; it is a table of requests"),
                );
                continue;
            };
            for (req, entry) in pins.iter() {
                let request = req.get_ref().as_ref();
                let DeValue::Table(fields) = entry.get_ref() else {
                    let found = describe(entry.get_ref());
                    self.fail(
                        entry.span(),
                        format!("`{name}.pins.{request}` is {found}; an entry is a table with `pin`, `function` and `source`"),
                    );
                    continue;
                };
                if let Some(e) = self.entry(name, request, key_span(req), fields) {
                    entries.insert(request.to_string(), e);
                }
            }
        }
        entries
    }

    fn entry(
        &mut self,
        board: &str,
        request: &str,
        at: Range<usize>,
        fields: &DeTable<'_>,
    ) -> Option<PinEntry> {
        let place = format!("{board}.pins.{request}");
        let mut pin = None;
        let mut function = None;
        let mut lock = false;
        let mut source = EntrySource::Hand;
        let mut ok = true;
        for (key, value) in fields.iter() {
            let k = key.get_ref().as_ref();
            let text = |v: &Spanned<DeValue<'_>>| match v.get_ref() {
                DeValue::String(s) => Some(s.to_string()),
                _ => None,
            };
            match k {
                "pin" | "function" => match text(value) {
                    Some(s) if !s.is_empty() => {
                        if k == "pin" {
                            pin = Some(s);
                        } else {
                            function = Some(s);
                        }
                    }
                    _ => {
                        ok = false;
                        let found = describe(value.get_ref());
                        self.fail(
                            value.span(),
                            format!("`{place}.{k}` is {found}; it is a name in quotes"),
                        );
                    }
                },
                "lock" => match value.get_ref() {
                    DeValue::Boolean(b) => lock = *b,
                    other => {
                        ok = false;
                        let found = describe(other);
                        self.fail(
                            value.span(),
                            format!("`{place}.lock` is {found}; it is `true` or `false`"),
                        );
                    }
                },
                "source" => match text(value).as_deref() {
                    Some("hand") => source = EntrySource::Hand,
                    Some("auto") => source = EntrySource::Auto,
                    _ => {
                        ok = false;
                        self.fail(
                            value.span(),
                            format!("`{place}.source` is not \"hand\" or \"auto\""),
                        );
                    }
                },
                other => {
                    ok = false;
                    self.fail(
                        key.span(),
                        format!(
                            "unknown key `{other}` in `{place}`; the keys are `pin`, `function`, `lock` and `source`"
                        ),
                    );
                }
            }
        }
        for (what, found) in [("pin", pin.is_some()), ("function", function.is_some())] {
            if !found && ok {
                ok = false;
                self.fail(at.clone(), format!("`{place}` has no `{what}`"));
            }
        }
        if !ok {
            return None;
        }
        Some(PinEntry {
            pin: pin?,
            function: function?,
            lock,
            source,
            at: Some(Span::new(at.start, at.end)),
        })
    }
}

/// The range of the header of an entry: its key, which is where the line of the table is.
fn key_span(key: &Spanned<impl AsRef<str>>) -> Range<usize> {
    key.span()
}

fn int_of(i: &DeInteger<'_>) -> Option<i64> {
    i64::from_str_radix(i.as_str(), i.radix()).ok()
}

fn describe(v: &DeValue<'_>) -> &'static str {
    match v {
        DeValue::String(_) => "text",
        DeValue::Integer(_) => "a whole number",
        DeValue::Float(_) => "a number",
        DeValue::Boolean(_) => "true or false",
        DeValue::Datetime(_) => "a date",
        DeValue::Array(_) => "a list",
        DeValue::Table(_) => "a table",
    }
}
