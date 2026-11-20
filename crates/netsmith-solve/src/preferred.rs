//! A project's preferred parts (ADR 0047, issue #802): `preferred.csv` beside `netsmith.toml`.
//!
//! The file lists parts by `mpn`, with a `manufacturer` when two parts share the number, one a
//! line, with an optional `note` (why the project prefers it). A line names a row as a part's
//! identity does (ADR 0143): by its `mpn`, and its `manufacturer` when the line and the row both
//! name one. When a generic part is resolved, the rows that fit and that the file names come
//! first, in the order of the file; the rest follow in the order of the project's supply
//! channels, each channel's by its own rule (`jlcpcb-assembly`: Basic, then Preferred, then
//! Extended; a retail channel: in stock, then the lowest price; [`crate::Rule`]), then the file
//! order. A listed row that a provider says has no stock is still passed over. The file changes
//! nothing else: the solver's order of a catalog's rows, and so every pick it makes from rows, is
//! the file order as before (ADR 0022), so a design and its pick lock stay as they are.
//!
//! An `lcsc` column, which named a part by a supplier's number before ADR 0143 made the number an
//! offer, is refused at the header, with the `mpn` of each number the offers know
//! ([`Preferred::parse_with`]). A line that names no part, a part named twice and a column the
//! format does not have are refused with the file and line. A part the file names that no
//! catalog has is not an error: it is never a row that fits, so it changes nothing.

use crate::catalog::Candidate;
use crate::offers::{Offers, SUPPLIERS};
use crate::table::parse_csv;

/// The parts a project prefers, in the order of its file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preferred {
    parts: Vec<Entry>,
}

/// One line of the file: the part, by its manufacturer's part number, with its manufacturer if
/// the line names one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    manufacturer: Option<String>,
    mpn: String,
}

const COLUMNS: [&str; 3] = ["manufacturer", "mpn", "note"];

impl Preferred {
    /// Read the text of a preferred-parts file; `file` names it in a message.
    pub fn parse(file: &str, text: &str) -> Result<Preferred, String> {
        Preferred::parse_with(file, text, None)
    }

    /// [`Self::parse`], with the offers of the project: a file that still has an `lcsc` column
    /// is refused with the `mpn` of each of its numbers that an offer has, to write in its place.
    pub fn parse_with(
        file: &str,
        text: &str,
        offers: Option<&Offers>,
    ) -> Result<Preferred, String> {
        let records = parse_csv(text).map_err(|e| format!("{file}: {e}"))?;
        let mut records = records.into_iter();
        let Some((line, header)) = records.next() else {
            return Ok(Preferred::default());
        };
        let header: Vec<String> = header.iter().map(|h| h.trim().to_string()).collect();
        if let Some(lcsc_at) = header.iter().position(|h| h == "lcsc") {
            return Err(retired_lcsc(file, line, lcsc_at, records, offers));
        }
        for name in &header {
            if !COLUMNS.contains(&name.as_str()) {
                return Err(format!(
                    "{file}:{line}: unknown column `{name}`; a preferred-parts file has the columns `mpn`, `manufacturer` and `note`"
                ));
            }
        }
        if !header.iter().any(|h| h == "mpn") {
            return Err(format!(
                "{file}:{line}: the header needs a column `mpn` to name the parts"
            ));
        }
        let at = |name: &str| header.iter().position(|h| h == name);
        let (manufacturer_at, mpn_at) = (at("manufacturer"), at("mpn"));
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
            let manufacturer = cell(manufacturer_at);
            let Some(mpn) = cell(mpn_at) else {
                return Err(match manufacturer {
                    Some(maker) => format!(
                        "{file}:{line}: the line names the manufacturer `{maker}` and no `mpn`; a manufacturer names a part only with its part number (ADR 0143)"
                    ),
                    None => format!("{file}:{line}: the line names no part; give its `mpn`"),
                });
            };
            let entry = Entry { manufacturer, mpn };
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
    /// does not name it: the first line that names it by its `mpn` (and `manufacturer`,
    /// [`Entry::names`]).
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
    /// Whether the line names the row: by the `mpn`, with the manufacturer when both name one
    /// (ADR 0143).
    fn names(&self, row: &Candidate) -> bool {
        row.mpn.as_deref() == Some(self.mpn.as_str())
            && match (&self.manufacturer, &row.manufacturer) {
                (Some(w), Some(h)) => w == h,
                _ => true,
            }
    }

    /// How this entry and `other` name the same part, if they do: the same `mpn` of the same
    /// manufacturer (or of a line that names none, which is any).
    fn same(&self, other: &Entry) -> Option<String> {
        if self.mpn != other.mpn {
            return None;
        }
        let a = &self.mpn;
        match (&self.manufacturer, &other.manufacturer) {
            (Some(x), Some(y)) if x != y => None,
            (Some(m), _) | (_, Some(m)) => Some(format!("`{a}` of `{m}`")),
            (None, None) => Some(format!("`{a}`")),
        }
    }
}

/// The error for a preferred-parts file whose header at `line` still has an `lcsc` column (its
/// place `lcsc_at`): the number is an offer's `sku` now (ADR 0143), so a line names its part by
/// the `mpn`. With `offers`, the message gives the `mpn` of each number an offer has.
fn retired_lcsc(
    file: &str,
    line: usize,
    lcsc_at: usize,
    records: impl Iterator<Item = (usize, Vec<String>)>,
    offers: Option<&Offers>,
) -> String {
    let mut out = format!(
        "{file}:{line}: the column `lcsc` has gone from a preferred-parts file (ADR 0143): a supplier's number is an offer, so a line names its part by the `mpn`, with the `manufacturer` when two parts share it (the header `mpn,manufacturer,note`)"
    );
    let Some(offers) = offers else {
        out.push_str("; the offers files (`offers/lcsc.csv`) say which part a number is");
        return out;
    };
    let mut lines = Vec::new();
    for (at, fields) in records {
        let Some(sku) = fields
            .get(lcsc_at)
            .map(|f| f.trim())
            .filter(|f| !f.is_empty())
        else {
            continue;
        };
        let found = SUPPLIERS.iter().find_map(|supplier| {
            offers
                .iter()
                .find(|o| o.supplier == *supplier && o.sku == sku)
        });
        lines.push(match found {
            Some(o) => match &o.manufacturer {
                Some(m) => format!("line {at}: `{sku}` is `{}` of `{m}` ({})", o.mpn, o.at),
                None => format!("line {at}: `{sku}` is `{}` ({})", o.mpn, o.at),
            },
            None => format!("line {at}: no offer has `{sku}`: write the `mpn` of its row by hand"),
        });
    }
    if !lines.is_empty() {
        out.push_str(&format!("; {}", lines.join("; ")));
    }
    out
}
