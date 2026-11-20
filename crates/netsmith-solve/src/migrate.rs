//! Moving a catalog file's `lcsc` and `library` columns into offers (ADR 0143).
//!
//! A catalog row once held the supplier's number (`lcsc`) and JLCPCB's library of it
//! (`library`). Both are now an offer, a row of `offers/<supplier>.csv` ([`crate::offers`]), and a
//! catalog file that still has either column is refused ([`crate::table`]). [`catalog`] turns
//! one such file into the same file without the two columns, every other cell as it was written,
//! and the offers its rows made: for a row with an `lcsc`, an offer of `lcsc` of that number, and
//! one of `jlcpcb` of that number with the row's `library` as its `class` when the row names one
//! (JLCPCB sorts every part it assembles in a library, so a row with none was not one JLCPCB
//! listed). Each offer is of the row's `manufacturer` and `mpn`, has the row's `source` (where its
//! number was read) and `checked`, and its `notes` name the row it came from.
//!
//! `netsmith project migrate-offers` runs it over a project's catalogs, and nothing else writes
//! the columns: what it cannot move (an `lcsc` with no `mpn`, which no offer can name) is an
//! error naming the row, and a `library` with no `lcsc` is dropped with a note.

/// The header of an offers file, as [`offer_lines`] writes its rows.
pub const OFFERS_HEADER: &str = "manufacturer,mpn,sku,class,source,checked,notes\n";

/// One offer a catalog row made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovedOffer {
    /// `jlcpcb` or `lcsc`.
    pub supplier: String,
    pub manufacturer: Option<String>,
    pub mpn: String,
    pub sku: String,
    /// The row's `library` for a `jlcpcb` offer; empty for `lcsc`.
    pub class: Option<String>,
    pub source: String,
    pub checked: Option<String>,
    /// The row it came from, `catalogs/led.csv:3`.
    pub from: String,
}

/// A catalog file with its columns moved out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migrated {
    /// The file's text without the `lcsc` and `library` columns.
    pub catalog: String,
    /// The offers its rows made, in the order of the rows, each row's `jlcpcb` offer first.
    pub offers: Vec<MovedOffer>,
    /// What was dropped, one line each: a `library` of a row with no `lcsc`.
    pub notes: Vec<String>,
}

/// The catalog file `file` (what a message calls it) with the text `text`, its `lcsc` and
/// `library` columns moved into offers. `None` when the file has neither column.
pub fn catalog(file: &str, text: &str) -> Result<Option<Migrated>, String> {
    let records = raw_records(text).map_err(|(line, why)| format!("{file}:{line}: {why}"))?;
    let Some(header) = records.iter().find(|r| !r.blank) else {
        return Ok(None);
    };
    let names: Vec<String> = header
        .fields
        .iter()
        .map(|f| unquote(f).trim().to_string())
        .collect();
    let at = |name: &str| names.iter().position(|n| n == name);
    if at("lcsc").is_none() && at("library").is_none() {
        return Ok(None);
    }
    let drop: Vec<usize> = ["lcsc", "library"].into_iter().filter_map(at).collect();
    let mut out = String::new();
    let mut offers = Vec::new();
    let mut notes = Vec::new();
    let header_line = header.line;
    for record in &records {
        if record.blank {
            out.push_str(&record.end);
            continue;
        }
        let kept: Vec<&str> = record
            .fields
            .iter()
            .enumerate()
            .filter(|(i, _)| !drop.contains(i))
            .map(|(_, f)| f.as_str())
            .collect();
        out.push_str(&kept.join(","));
        out.push_str(&record.end);
        if record.line == header_line {
            continue;
        }
        let here = format!("{file}:{}", record.line);
        if record.fields.len() != names.len() {
            return Err(format!(
                "{here}: the row has {} fields but the header (line {header_line}) has {}",
                record.fields.len(),
                names.len()
            ));
        }
        let cell = |name: &str| {
            at(name)
                .map(|i| unquote(&record.fields[i]).trim().to_string())
                .filter(|c| !c.is_empty())
        };
        let (lcsc, library) = (cell("lcsc"), cell("library"));
        let Some(sku) = lcsc else {
            if let Some(library) = library {
                notes.push(format!(
                    "{here}: the library `{library}` was dropped: the row has no lcsc, so there is no offer to give it to"
                ));
            }
            continue;
        };
        let Some(mpn) = cell("mpn") else {
            return Err(format!(
                "{here}: the row has the lcsc `{sku}` and no `mpn`; an offer names its part by the manufacturer's part number (ADR 0143): write the row's `mpn`, read from its source, and run again"
            ));
        };
        let Some(source) = cell("source") else {
            return Err(format!(
                "{here}: the row for lcsc {sku} has no source; an offer needs the URL or text its number was read from: write the row's `source` and run again"
            ));
        };
        let manufacturer = cell("manufacturer");
        let checked = cell("checked");
        if let Some(class) = library {
            offers.push(MovedOffer {
                supplier: "jlcpcb".into(),
                manufacturer: manufacturer.clone(),
                mpn: mpn.clone(),
                sku: sku.clone(),
                class: Some(class),
                source: source.clone(),
                checked: checked.clone(),
                from: here.clone(),
            });
        }
        offers.push(MovedOffer {
            supplier: "lcsc".into(),
            manufacturer,
            mpn,
            sku,
            class: None,
            source,
            checked,
            from: here,
        });
    }
    Ok(Some(Migrated {
        catalog: out,
        offers,
        notes,
    }))
}

/// Add the catalog file `file` of `name` to `catalogs` as the migration moves it: a text with the
/// `lcsc` and `library` columns is added without them, with the offers its rows make; one with
/// neither is added as it is. What a test or a tool that holds a catalog in the old format reads
/// it with, as `netsmith project migrate-offers` would write it.
pub fn add_to(
    catalogs: &mut crate::Catalogs,
    name: &str,
    file: &str,
    text: &str,
) -> Result<(), String> {
    let Some(moved) = catalog(file, text)? else {
        return catalogs.add_file(name, file, text);
    };
    catalogs.add_file(name, file, &moved.catalog)?;
    for supplier in crate::offers::SUPPLIERS {
        let mine: Vec<MovedOffer> = moved
            .offers
            .iter()
            .filter(|o| o.supplier == supplier)
            .cloned()
            .collect();
        if !mine.is_empty() {
            catalogs.add_offers_file(
                supplier,
                &format!("offers/{supplier}.csv"),
                &format!("{OFFERS_HEADER}{}", offer_lines(&mine)),
            )?;
        }
    }
    Ok(())
}

/// Write the catalog file `file` (relative to the project root `root`, `catalogs/led.csv`) from
/// `text`, as the migration moves it: a text with the `lcsc` and `library` columns is written
/// without them, after the offers its rows make are appended to `root/offers/<supplier>.csv`
/// (made with its header when there is none); one with neither column is written as it is. What
/// a test that keeps a project's rows in the old format writes them with.
pub fn write_to(root: &std::path::Path, file: &str, text: &str) -> Result<(), String> {
    let write = |path: &std::path::Path, text: &str| {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    };
    let Some(moved) = catalog(file, text)? else {
        return write(&root.join(file), text);
    };
    // The offers first: a watcher that builds again on the catalog then finds them there.
    for supplier in crate::offers::SUPPLIERS {
        let path = root
            .join(crate::offers::FOLDER)
            .join(format!("{supplier}.csv"));
        let mut offers =
            std::fs::read_to_string(&path).unwrap_or_else(|_| OFFERS_HEADER.to_string());
        // An offer the file has already (the same part and sku) is not written again.
        let mut had = crate::offers::Offers::empty();
        had.add_file(supplier, &path.display().to_string(), &offers)?;
        let mine: Vec<MovedOffer> = moved
            .offers
            .iter()
            .filter(|o| o.supplier == supplier)
            .filter(|o| {
                !matches!(
                    had.of(supplier, o.manufacturer.as_deref(), &o.mpn),
                    Ok(Some(old)) if old.sku == o.sku
                )
            })
            .cloned()
            .collect();
        if mine.is_empty() {
            continue;
        }
        offers.push_str(&offer_lines(&mine));
        write(&path, &offers)?;
    }
    write(&root.join(file), &moved.catalog)
}

/// The rows of an offers file for `offers`, one line each and no header: what is appended to
/// `offers/<supplier>.csv` for the offers of that supplier.
pub fn offer_lines(offers: &[MovedOffer]) -> String {
    let mut out = String::new();
    for o in offers {
        let notes = format!(
            "moved from the {} of {} by netsmith project migrate-offers",
            if o.class.is_some() {
                "lcsc and library columns"
            } else {
                "lcsc column"
            },
            o.from
        );
        let fields = [
            o.manufacturer.clone().unwrap_or_default(),
            o.mpn.clone(),
            o.sku.clone(),
            o.class.clone().unwrap_or_default(),
            o.source.clone(),
            o.checked.clone().unwrap_or_default(),
            notes,
        ];
        let fields: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
        out.push_str(&fields.join(","));
        out.push('\n');
    }
    out
}

/// A CSV field as a file writes it: quoted, with its quotes doubled, when it holds a comma, a
/// quote or a line break.
pub fn csv_field(text: &str) -> String {
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// One record of a CSV file as written: its fields with their quotes, the line it starts on, and
/// what ends it (`\n`, `\r\n` or nothing at the end of the file). A blank line is a record of
/// its own that holds nothing.
struct RawRecord {
    fields: Vec<String>,
    line: usize,
    end: String,
    blank: bool,
}

fn raw_records(text: &str) -> Result<Vec<RawRecord>, (usize, String)> {
    let mut records = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let (mut quoted, mut line, mut start, mut quote_line) = (false, 1, 1, 1);
    let mut chars = text.chars().peekable();
    let mut finish = |fields: &mut Vec<String>, field: &mut String, start: usize, end: &str| {
        fields.push(std::mem::take(field));
        let blank = fields.len() == 1 && fields[0].trim().is_empty();
        records.push(RawRecord {
            fields: std::mem::take(fields),
            line: start,
            end: end.to_string(),
            blank,
        });
    };
    while let Some(c) = chars.next() {
        if quoted {
            field.push(c);
            if c == '\n' {
                line += 1;
            }
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            }
            continue;
        }
        match c {
            '"' => {
                quoted = true;
                quote_line = line;
                field.push(c);
            }
            ',' => fields.push(std::mem::take(&mut field)),
            '\r' if chars.peek() == Some(&'\n') => {
                chars.next();
                finish(&mut fields, &mut field, start, "\r\n");
                line += 1;
                start = line;
            }
            '\n' => {
                finish(&mut fields, &mut field, start, "\n");
                line += 1;
                start = line;
            }
            _ => field.push(c),
        }
    }
    if quoted {
        return Err((quote_line, "a quoted field is never closed".to_string()));
    }
    if !field.is_empty() || !fields.is_empty() {
        finish(&mut fields, &mut field, start, "");
    }
    Ok(records)
}

/// A field's text: without its quotes, and with a doubled quote one.
fn unquote(field: &str) -> String {
    let trimmed = field.trim();
    match trimmed.strip_prefix('"').and_then(|f| f.strip_suffix('"')) {
        Some(inner) => inner.replace("\"\"", "\""),
        None => field.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_columns_go_and_each_numbered_row_makes_its_offers() {
        let text = "lcsc,mpn,name,library,source,checked\nC1,ZZ-1,\"One, the first\",Basic,\"the record, read 2026-11-01\",2026-11-02\nC2,ZZ-2,Two,,the record,\n,,Made up,,,\n";
        let m = catalog("catalogs/zz.csv", text).unwrap().unwrap();
        assert_eq!(
            m.catalog,
            "mpn,name,source,checked\nZZ-1,\"One, the first\",\"the record, read 2026-11-01\",2026-11-02\nZZ-2,Two,the record,\n,Made up,,\n"
        );
        assert_eq!(m.offers.len(), 3);
        assert_eq!(
            (m.offers[0].supplier.as_str(), m.offers[0].class.as_deref()),
            ("jlcpcb", Some("Basic"))
        );
        assert_eq!(
            offer_lines(&m.offers[..2]),
            ",ZZ-1,C1,Basic,\"the record, read 2026-11-01\",2026-11-02,moved from the lcsc and library columns of catalogs/zz.csv:2 by netsmith project migrate-offers\n,ZZ-1,C1,,\"the record, read 2026-11-01\",2026-11-02,moved from the lcsc column of catalogs/zz.csv:2 by netsmith project migrate-offers\n"
        );
        assert_eq!(m.offers[2].supplier, "lcsc");
        assert_eq!(m.offers[2].from, "catalogs/zz.csv:3");
        assert!(m.notes.is_empty());
    }

    #[test]
    fn a_file_without_the_columns_is_left() {
        assert_eq!(catalog("c.csv", "mpn,source\nA,x\n").unwrap(), None);
        assert_eq!(catalog("c.csv", "").unwrap(), None);
    }

    #[test]
    fn what_cannot_be_moved_is_named() {
        let e = catalog("c.csv", "lcsc,mpn,source\nC1,,x\n").unwrap_err();
        assert!(
            e.starts_with("c.csv:2: the row has the lcsc `C1` and no `mpn`"),
            "{e}"
        );
        let e = catalog("c.csv", "lcsc,mpn,source\nC1,A,\n").unwrap_err();
        assert!(
            e.starts_with("c.csv:2: the row for lcsc C1 has no source"),
            "{e}"
        );
        let m = catalog("c.csv", "lcsc,mpn,library,source\n,A,Basic,x\n")
            .unwrap()
            .unwrap();
        assert_eq!(
            m.notes,
            [
                "c.csv:2: the library `Basic` was dropped: the row has no lcsc, so there is no offer to give it to"
            ]
        );
        assert_eq!(m.catalog, "mpn,source\nA,x\n");
    }

    #[test]
    fn line_ends_and_a_quoted_line_break_are_kept() {
        let text = "lcsc,mpn,notes,source\r\nC1,A,\"two\nlines\",x\r\n";
        let m = catalog("c.csv", text).unwrap().unwrap();
        assert_eq!(m.catalog, "mpn,notes,source\r\nA,\"two\nlines\",x\r\n");
    }
}
