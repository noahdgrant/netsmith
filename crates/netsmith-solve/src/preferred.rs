//! A project's preferred parts (ADR 0047, issue #802): `preferred.csv` beside `netsmith.toml`.
//!
//! The file lists parts by `mpn` (with an optional `manufacturer`) or `lcsc`, one a line, with
//! an optional `note` (why the project prefers it). A line names a row as a part's identity does
//! (ADR 0143): by its `mpn`, and its `manufacturer` when the line and the row both name one, when
//! the two have an `mpn`; by its `lcsc` otherwise (a line or a row with no `mpn`). When a generic part is resolved, the rows that fit and that the file
//! names come first, in the order of the file; the rest follow in the preferred-parts rule of
//! the code (JLCPCB Basic, then Preferred, then Extended, then file order). A listed row that a
//! provider says has no stock is still passed over. The file changes nothing else: the solver's
//! order of a catalog's rows, and so every pick it makes from rows, is the file order as
//! before (ADR 0022), so a design and its pick lock stay as they are.
//!
//! A line that names neither column, a part named twice and a column the format does not have
//! are refused with the file and line. A part the file names that no catalog has is not an
//! error: it is never a row that fits, so it changes nothing.

use crate::catalog::Candidate;
use crate::table::parse_csv;

/// The parts a project prefers, in the order of its file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preferred {
    parts: Vec<Entry>,
}

/// One line of the file: the part, by its LCSC number, its manufacturer's part number (with its
/// manufacturer, if the line names one), or both.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    lcsc: Option<String>,
    manufacturer: Option<String>,
    mpn: Option<String>,
}

const COLUMNS: [&str; 4] = ["lcsc", "manufacturer", "mpn", "note"];

impl Preferred {
    /// Read the text of a preferred-parts file; `file` names it in a message.
    pub fn parse(file: &str, text: &str) -> Result<Preferred, String> {
        let records = parse_csv(text).map_err(|e| format!("{file}: {e}"))?;
        let mut records = records.into_iter();
        let Some((line, header)) = records.next() else {
            return Ok(Preferred::default());
        };
        let header: Vec<String> = header.iter().map(|h| h.trim().to_string()).collect();
        for name in &header {
            if !COLUMNS.contains(&name.as_str()) {
                return Err(format!(
                    "{file}:{line}: unknown column `{name}`; a preferred-parts file has the columns `lcsc`, `manufacturer`, `mpn` and `note`"
                ));
            }
        }
        if !header.iter().any(|h| h == "lcsc" || h == "mpn") {
            return Err(format!(
                "{file}:{line}: the header needs a column `lcsc` or `mpn` to name the parts"
            ));
        }
        let at = |name: &str| header.iter().position(|h| h == name);
        let (lcsc_at, manufacturer_at, mpn_at) = (at("lcsc"), at("manufacturer"), at("mpn"));
        let mut parts: Vec<Entry> = Vec::new();
        for (line, fields) in records {
            let cell = |i: Option<usize>| {
                i.and_then(|i| fields.get(i))
                    .map(|f| f.trim())
                    .filter(|f| !f.is_empty())
                    .map(str::to_string)
            };
            if fields.len() > header.len() {
                return Err(format!(
                    "{file}:{line}: {} fields, but the header has {}",
                    fields.len(),
                    header.len()
                ));
            }
            let entry = Entry {
                lcsc: cell(lcsc_at),
                manufacturer: cell(manufacturer_at),
                mpn: cell(mpn_at),
            };
            if entry.lcsc.is_none() && entry.mpn.is_none() {
                return Err(format!(
                    "{file}:{line}: the line names no part; give its `lcsc` or its `mpn`"
                ));
            }
            if let (Some(maker), None) = (&entry.manufacturer, &entry.mpn) {
                return Err(format!(
                    "{file}:{line}: the line names the manufacturer `{maker}` and no `mpn`; a manufacturer names a part only with its part number (ADR 0143)"
                ));
            }
            if let Some(twice) = parts.iter().find_map(|p| p.same(&entry)) {
                return Err(format!(
                    "{file}:{line}: {twice} is listed twice; a part is listed once"
                ));
            }
            parts.push(entry);
        }
        Ok(Preferred { parts })
    }

    /// Where a row stands in the file (0 is the first line of parts), or `None` if the file
    /// does not name it: the first line that names it by its `mpn` (and `manufacturer`), else by
    /// its `lcsc` ([`Entry::names`]).
    pub fn position(&self, row: &Candidate) -> Option<usize> {
        self.parts.iter().position(|p| p.names(row))
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.parts.len()
    }
}

impl Entry {
    /// Whether the line names the row: by the `mpn` when both have one, with the manufacturer
    /// when both name one (ADR 0143), and by the `lcsc` when either has no `mpn`.
    fn names(&self, row: &Candidate) -> bool {
        match (&self.mpn, &row.mpn) {
            (Some(want), Some(have)) => {
                want == have
                    && match (&self.manufacturer, &row.manufacturer) {
                        (Some(w), Some(h)) => w == h,
                        _ => true,
                    }
            }
            _ => self.lcsc.is_some() && self.lcsc == row.lcsc,
        }
    }

    /// How this entry and `other` name the same part, if they do: the same `lcsc`, or the same
    /// `mpn` of the same manufacturer (or of a line that names none, which is any).
    fn same(&self, other: &Entry) -> Option<String> {
        if let (Some(a), Some(b)) = (&self.lcsc, &other.lcsc)
            && a == b
        {
            return Some(format!("`{a}`"));
        }
        match (&self.mpn, &other.mpn) {
            (Some(a), Some(b)) if a == b => match (&self.manufacturer, &other.manufacturer) {
                (Some(x), Some(y)) if x != y => None,
                (Some(m), _) | (_, Some(m)) => Some(format!("`{a}` of `{m}`")),
                (None, None) => Some(format!("`{a}`")),
            },
            _ => None,
        }
    }
}
