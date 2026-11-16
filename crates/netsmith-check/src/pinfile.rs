//! The pin file beside a source file: `<stem>.pins.toml` (ADR 0101).
//!
//! It holds the solution of the pin requests of the source (`.status: .u.GPIO`): for each
//! request, the pin the tool picked, and whether a person locked it. A request for the end of a
//! bus (`.dbg: .u.Uart`, #788) has an entry of its own shape: the peripheral it got and the pin of
//! each line (`peripheral = "UARTE1"`, `pins = { rx = "p01", tx = "p00" }`). It is plain TOML, optional,
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

/// The answer of one request for the end of a bus (`.dbg: .u.Uart`, #788): the peripheral of
/// the part, and the pin of each of its lines.
#[derive(Debug, Clone)]
pub struct BusEntry {
    /// The peripheral's name in the part (`UARTE1`).
    pub peripheral: String,
    /// The pin of each line, by the line's name (`tx`), each the pin's path in the part.
    pub pins: BTreeMap<String, String>,
    pub lock: bool,
    pub source: EntrySource,
    /// Where the entry's table starts in the file it was read from, as for a [`PinEntry`].
    pub at: Option<Span>,
}

impl PartialEq for BusEntry {
    fn eq(&self, other: &Self) -> bool {
        self.peripheral == other.peripheral
            && self.pins == other.pins
            && self.lock == other.lock
            && self.source == other.source
    }
}

impl Eq for BusEntry {}

/// The entries of a file: board name, then request path, each in byte order. A request for a
/// pin is in `boards`, a request for the end of a bus in `buses`; one key is in one of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PinFile {
    pub boards: BTreeMap<String, BTreeMap<String, PinEntry>>,
    pub buses: BTreeMap<String, BTreeMap<String, BusEntry>>,
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
        self.boards.values().all(BTreeMap::is_empty) && self.buses.values().all(BTreeMap::is_empty)
    }

    /// The entries of one board.
    pub fn board(&self, name: &str) -> Option<&BTreeMap<String, PinEntry>> {
        self.boards.get(name)
    }

    /// The entries of the requests for the end of a bus of one board.
    pub fn buses_of(&self, name: &str) -> Option<&BTreeMap<String, BusEntry>> {
        self.buses.get(name)
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
        let names: std::collections::BTreeSet<&String> =
            self.boards.keys().chain(self.buses.keys()).collect();
        for board in names {
            let pins = self.boards.get(board);
            let buses = self.buses.get(board);
            let keys: std::collections::BTreeSet<&String> = pins
                .into_iter()
                .flat_map(|e| e.keys())
                .chain(buses.into_iter().flat_map(|e| e.keys()))
                .collect();
            for key in keys {
                out.push_str(&format!("\n[{}.pins.{}]\n", toml_key(board), toml_key(key)));
                let (lock, source) = if let Some(e) = pins.and_then(|p| p.get(key)) {
                    out.push_str(&format!("pin = {}\n", toml_string(&e.pin)));
                    out.push_str(&format!("function = {}\n", toml_string(&e.function)));
                    (e.lock, e.source)
                } else {
                    let e = &buses.expect("a key of one of the two")[key];
                    out.push_str(&format!("peripheral = {}\n", toml_string(&e.peripheral)));
                    let lines: Vec<String> = e
                        .pins
                        .iter()
                        .map(|(line, pin)| format!("{} = {}", toml_key(line), toml_string(pin)))
                        .collect();
                    out.push_str(&format!("pins = {{ {} }}\n", lines.join(", ")));
                    (e.lock, e.source)
                };
                if lock {
                    out.push_str("lock = true\n");
                }
                out.push_str(&format!("source = {}\n", toml_string(source.word())));
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
            let (entries, buses) = self.board(name, board);
            if !entries.is_empty() {
                self.file.boards.insert(name.to_string(), entries);
            }
            if !buses.is_empty() {
                self.file.buses.insert(name.to_string(), buses);
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

    fn board(
        &mut self,
        name: &str,
        table: &DeTable<'_>,
    ) -> (BTreeMap<String, PinEntry>, BTreeMap<String, BusEntry>) {
        let mut entries = BTreeMap::new();
        let mut buses = BTreeMap::new();
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
                if fields.get("peripheral").is_some() {
                    if let Some(e) = self.bus_entry(name, request, key_span(req), fields) {
                        buses.insert(request.to_string(), e);
                    }
                } else if let Some(e) = self.entry(name, request, key_span(req), fields) {
                    entries.insert(request.to_string(), e);
                }
            }
        }
        (entries, buses)
    }

    /// An entry of a request for the end of a bus: it has `peripheral` and `pins`, a table of
    /// the pin of each line, and `lock` and `source` as any entry.
    fn bus_entry(
        &mut self,
        board: &str,
        request: &str,
        at: Range<usize>,
        fields: &DeTable<'_>,
    ) -> Option<BusEntry> {
        let place = format!("{board}.pins.{request}");
        let mut peripheral = None;
        let mut pins: Option<BTreeMap<String, String>> = None;
        let mut lock = false;
        let mut source = EntrySource::Hand;
        let mut ok = true;
        for (key, value) in fields.iter() {
            let k = key.get_ref().as_ref();
            match (k, value.get_ref()) {
                ("peripheral", DeValue::String(s)) if !s.is_empty() => {
                    peripheral = Some(s.to_string());
                }
                ("pins", DeValue::Table(lines)) => {
                    let mut map = BTreeMap::new();
                    for (line, pin) in lines.iter() {
                        let l = line.get_ref().as_ref();
                        match pin.get_ref() {
                            DeValue::String(s) if !s.is_empty() => {
                                map.insert(l.to_string(), s.to_string());
                            }
                            other => {
                                ok = false;
                                let found = describe(other);
                                self.fail(
                                    pin.span(),
                                    format!("`{place}.pins.{l}` is {found}; it is the pin's name in quotes"),
                                );
                            }
                        }
                    }
                    if map.is_empty() && ok {
                        ok = false;
                        self.fail(
                            value.span(),
                            format!("`{place}.pins` is empty; it has the pin of each line of the peripheral"),
                        );
                    }
                    pins = Some(map);
                }
                ("lock", DeValue::Boolean(b)) => lock = *b,
                ("source", DeValue::String(s)) if s.as_ref() == "hand" => {
                    source = EntrySource::Hand;
                }
                ("source", DeValue::String(s)) if s.as_ref() == "auto" => {
                    source = EntrySource::Auto;
                }
                ("peripheral" | "pins" | "lock" | "source", other) => {
                    ok = false;
                    let found = describe(other);
                    let want = match k {
                        "peripheral" => "a name in quotes",
                        "pins" => "a table of the pin of each line, `{ tx = \"p00\" }`",
                        "lock" => "`true` or `false`",
                        _ => "\"hand\" or \"auto\"",
                    };
                    self.fail(
                        value.span(),
                        format!("`{place}.{k}` is {found}; it is {want}"),
                    );
                }
                (other, _) => {
                    ok = false;
                    self.fail(
                        key.span(),
                        format!(
                            "unknown key `{other}` in `{place}`; an entry of a request for the end of a bus has `peripheral`, `pins`, `lock` and `source`"
                        ),
                    );
                }
            }
        }
        if pins.is_none() && ok {
            ok = false;
            self.fail(at.clone(), format!("`{place}` has no `pins`"));
        }
        if !ok {
            return None;
        }
        Some(BusEntry {
            peripheral: peripheral?,
            pins: pins?,
            lock,
            source,
            at: Some(Span::new(at.start, at.end)),
        })
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
