//! `netsmith catalog add --lcsc C123 [--to catalogs/x.csv]` (#454): a row made from LCSC's record.
//!
//! Finding the evidence for a row is the slow part of adding a part: for each LCSC number, the
//! record, the manufacturer's number, the package and the datasheet's ratings are copied into the
//! CSV by hand. This command does the part a record can do. It asks LCSC's product record, as
//! `catalog refresh` does (never JLCPCB, ADR 0039), and appends a row with `mpn`, `package` and
//! `source` filled in, the `source` naming the record and the day it was read, and `manufacturer`
//! when the file has the column (ADR 0143), from the same record as LCSC spells it
//! (`brandNameEn`), which the `source` then says and the `notes` ask a person to check against
//! the datasheet. The LCSC number is not a cell of the row: it is an offer (ADR 0143), appended to
//! the project's LCSC offers file (`offers/lcsc.csv`, made with its header when there is none)
//! with the same `source` and the day in `checked`. Every other cell stays empty, and the command
//! lists the columns that need the datasheet (ratings, curves, heights), and the offer's `class`
//! (the library from JLCPCB's page) that a part assembled by JLCPCB needs: an offer with no
//! `class` is not in the `jlcpcb-assembly` channel. A row with an empty column that a part needs is
//! refused where the part reads it, so the command cannot make a row that passes with a guess in
//! it: a person or an agent fills those cells from the datasheet, with its source, as
//! `data/jlcpcb/README.md` says.
//!
//! Without `--to` it prints what the record says and writes nothing.

use std::fs;
use std::path::Path;

use super::csvfile::CsvFile;
use super::http::{Fetcher, Got, Network};
use super::records::{Product, parse_product};
use super::{date_of, write_atomically};

/// The fixed columns a row leaves empty on purpose, which the command does not ask to be filled:
/// a supplier's price and stock (ADR 0047), the day of a check, and `name` and `label`, which
/// default to the `mpn`.
pub(crate) const LEFT_EMPTY: [&str; 6] = ["price", "stock", "date", "checked", "name", "label"];

/// What `catalog add` did.
#[derive(Debug, Clone, PartialEq)]
pub struct Added {
    pub product: Product,
    /// The day the record was read (UTC), as the `source` says.
    pub read: String,
    /// The file and the line of the row written, with `--to`.
    pub written: Option<(String, usize)>,
    /// The file and the line of the LCSC offer written beside it.
    pub offer: Option<(String, usize)>,
    /// The columns the row has a value in.
    pub filled: Vec<String>,
    /// The columns a person must fill from the datasheet or JLCPCB's page.
    pub to_fill: Vec<String>,
}

/// Why `catalog add` wrote nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddError {
    /// LCSC has no record of the number, or the file already has it: exit 1.
    Refused(String),
    /// The run could not be made (a file, the network, an answer of another shape): exit 2.
    Stopped(String),
}

/// The `source` of a row made from LCSC's record of `product`, read on `day`.
pub fn source_of(product: &Product, endpoint: &str, day: &str) -> String {
    let package = product.package.as_deref().unwrap_or("no package listed");
    format!(
        "LCSC product record for {lcsc} (the data behind https://www.lcsc.com/product-detail/{lcsc}.html, fetched from {endpoint}?productCode={lcsc}) read {day} ({mpn}, {package}); written by `netsmith catalog add`",
        lcsc = product.lcsc,
        mpn = product.mpn,
    )
}

/// The files `catalog add` writes, each with what messages call it: the catalog file the row goes
/// in, and the LCSC offers file its number goes in.
pub type Files<'a> = ((&'a Path, &'a str), (&'a Path, &'a str));

/// Ask LCSC for `lcsc` and, with `to`, append the row to that catalog file and its offer to the
/// offers file `offers` (LCSC's, `offers/lcsc.csv`, made when it does not exist). `shown` is what
/// messages call a file.
pub fn add(lcsc: &str, to: Option<Files<'_>>, network: Network) -> Result<Added, AddError> {
    let stopped = |e: String| AddError::Stopped(e);
    let (to, offers) = match to {
        Some((to, offers)) => (Some(to), Some(offers)),
        None => (None, None),
    };
    // The offers file, read first too: a number it has is a part already added.
    let offers = match offers {
        Some((path, shown)) => {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    netsmith_solve::migrate::OFFERS_HEADER.to_string()
                }
                Err(e) => return Err(stopped(format!("{shown}: {e}"))),
            };
            let file = CsvFile::parse(shown, text).map_err(stopped)?;
            let header = file.header();
            let sku = header
                .iter()
                .position(|h| h.trim() == "sku")
                .ok_or_else(|| stopped(format!("{shown}: the file has no `sku` column; an offers file has manufacturer, mpn, sku, class, source, checked and notes")))?;
            if let Some(record) = (1..file.len()).find(|&r| file.value(r, sku).trim() == lcsc) {
                return Err(AddError::Refused(format!(
                    "{shown}:{}: the file already has an offer of {lcsc}; `netsmith catalog refresh --lcsc {lcsc}` checks it",
                    file.line(record)
                )));
            }
            Some((path, shown, file))
        }
        None => None,
    };
    // Read the file first: a file that cannot take the row is said before anything is asked.
    let file = match to {
        Some((path, shown)) => {
            let text = fs::read_to_string(path).map_err(|e| stopped(format!("{shown}: {e}")))?;
            let file = CsvFile::parse(shown, text).map_err(stopped)?;
            if file.is_empty() {
                return Err(stopped(format!(
                    "{shown}: the file is empty; the first line names the columns"
                )));
            }
            let header = file.header();
            let column = |name: &str| header.iter().position(|h| h.trim() == name);
            for needed in ["mpn", "source"] {
                if column(needed).is_none() {
                    return Err(stopped(format!(
                        "{shown}: the file has no `{needed}` column, which a row from LCSC's record needs"
                    )));
                }
            }
            if let Some(old) = ["lcsc", "library"]
                .into_iter()
                .find(|c| column(c).is_some())
            {
                return Err(stopped(format!(
                    "{shown}: the file has the column `{old}`, which a catalog file no longer has (ADR 0143): run `netsmith project migrate-offers` first"
                )));
            }
            Some((path, shown, file))
        }
        None => None,
    };
    let endpoint = network.endpoints.lcsc.clone();
    let mut fetcher = Fetcher::new(network);
    let (product, secs) = match fetcher
        .get(lcsc, |body| parse_product(body, lcsc))
        .map_err(stopped)?
    {
        Got::Answer {
            value: Some(product),
            secs,
            ..
        } => (product, secs),
        Got::Answer { value: None, .. } => {
            return Err(AddError::Refused(format!(
                "LCSC has no record of {lcsc}: nothing was written"
            )));
        }
        Got::NotCached => {
            return Err(stopped(format!(
                "--offline, and no answer for {lcsc} is kept: run without --offline to ask LCSC"
            )));
        }
    };
    let read = date_of(secs);
    let mut added = Added {
        product: product.clone(),
        read: read.clone(),
        written: None,
        offer: None,
        filled: Vec::new(),
        to_fill: Vec::new(),
    };
    let (Some((path, shown, file)), Some((offers_path, offers_shown, offers_file))) =
        (file, offers)
    else {
        return Ok(added);
    };
    let header = file.header();
    let mpn_at = header
        .iter()
        .position(|h| h.trim() == "mpn")
        .expect("checked above");
    if let Some(record) = (1..file.len()).find(|&r| file.value(r, mpn_at).trim() == product.mpn) {
        return Err(AddError::Refused(format!(
            "{shown}:{}: the file already has a row for {}, the part {lcsc} is of: add its offer to {offers_shown} instead",
            file.line(record),
            product.mpn
        )));
    }
    let mut cells = Vec::new();
    let maker_written =
        header.iter().any(|h| h.trim() == "manufacturer") && product.manufacturer.is_some();
    let source = match (&product.manufacturer, maker_written) {
        (Some(maker), true) => format!(
            "{}; manufacturer: `{maker}`, the record's `brandNameEn`, as LCSC spells it",
            source_of(&product, &endpoint, &read)
        ),
        _ => source_of(&product, &endpoint, &read),
    };
    for column in header.iter().map(|h| h.trim()) {
        let value = match column {
            "manufacturer" => product.manufacturer.clone(),
            "mpn" => Some(product.mpn.clone()),
            "package" => product.package.clone(),
            "source" => Some(source.clone()),
            "notes" => Some(format!(
                "Written by `netsmith catalog add` from LCSC's record alone: fill the empty columns from the datasheet (with its source in `source`), write the `class` of its offer in offers/lcsc.csv from JLCPCB's page for a part JLCPCB assembles, and check the package against std/kicad.toml{}",
                if maker_written {
                    ", and the manufacturer's name against the datasheet's spelling"
                } else {
                    ""
                }
            )),
            _ => None,
        };
        match &value {
            Some(_) => added.filled.push(column.to_string()),
            None if !LEFT_EMPTY.contains(&column) => added.to_fill.push(column.to_string()),
            None => {}
        }
        cells.push(value.map(|v| csv_cell(&v)).unwrap_or_default());
    }
    let mut text = file.text().to_string();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let line = text.lines().count() + 1;
    text.push_str(&cells.join(","));
    text.push('\n');
    // The offer: of the row's manufacturer when the row has one, as an offer must be.
    let offer = netsmith_solve::migrate::MovedOffer {
        supplier: "lcsc".into(),
        manufacturer: product.manufacturer.clone().filter(|_| maker_written),
        mpn: product.mpn.clone(),
        sku: product.lcsc.clone(),
        class: None,
        source,
        checked: Some(read.clone()),
        from: String::new(),
    };
    let mut offers_text = offers_file.text().to_string();
    if !offers_text.is_empty() && !offers_text.ends_with('\n') {
        offers_text.push('\n');
    }
    let offer_line = offers_text.lines().count() + 1;
    offers_text.push_str(&offer_row(&offer));
    if let Some(dir) = offers_path.parent() {
        fs::create_dir_all(dir).map_err(|e| stopped(format!("{}: {e}", dir.display())))?;
    }
    write_atomically(path, &text).map_err(stopped)?;
    write_atomically(offers_path, &offers_text).map_err(stopped)?;
    added.written = Some((shown.to_string(), line));
    added.offer = Some((offers_shown.to_string(), offer_line));
    added.to_fill.push(
        "the class of the offer (offers/lcsc.csv, the library from JLCPCB's page) for a part JLCPCB assembles"
            .to_string(),
    );
    Ok(added)
}

/// The line of an offers file for an offer `catalog add` made.
fn offer_row(offer: &netsmith_solve::migrate::MovedOffer) -> String {
    let fields = [
        offer.manufacturer.clone().unwrap_or_default(),
        offer.mpn.clone(),
        offer.sku.clone(),
        String::new(),
        offer.source.clone(),
        offer.checked.clone().unwrap_or_default(),
        "Written by `netsmith catalog add` from LCSC's record".to_string(),
    ];
    let cells: Vec<String> = fields.iter().map(|f| csv_cell(f)).collect();
    format!("{}\n", cells.join(","))
}

/// A CSV field: quoted when it holds a comma, a quote or a line break, with a quote doubled.
pub(crate) fn csv_cell(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

/// What `catalog add` prints: the record, the row written, and the columns to fill.
pub fn render_added(added: &Added) -> String {
    let p = &added.product;
    let mut out = format!(
        "LCSC's record of {} (read {}): mpn {}, package {}{}\n",
        p.lcsc,
        added.read,
        p.mpn,
        p.package.as_deref().unwrap_or("not listed"),
        p.manufacturer
            .as_deref()
            .map(|m| format!(", manufacturer {m}"))
            .unwrap_or_default()
    );
    match &added.written {
        None => out.push_str(
            "nothing written: name a catalog file with --to <catalogs/x.csv> to add the row\n",
        ),
        Some((file, line)) => {
            out.push_str(&format!(
                "added {} to {file}:{line} with {}\n",
                p.mpn,
                added.filled.join(", ")
            ));
            if let Some((offers, at)) = &added.offer {
                out.push_str(&format!("added the offer of {} to {offers}:{at}\n", p.lcsc));
            }
            if !added.to_fill.is_empty() {
                out.push_str(&format!(
                    "fill from the datasheet (with its source) or JLCPCB's page: {}\na part that reads an empty one of these refuses the row until it is filled\n",
                    added.to_fill.join(", ")
                ));
            }
        }
    }
    out
}

/// What `catalog add --format json` prints: what [`render_added`] says, as data. `written` is the
/// `file` and `line` of the row made (null with no `--to`), `filled` the columns it has a value
/// in and `to_fill` the columns a person must fill.
pub fn json_added(added: &Added) -> serde_json::Value {
    use serde_json::json;
    let p = &added.product;
    json!({
        "command": "catalog add",
        "ok": true,
        "lcsc": p.lcsc,
        "manufacturer": p.manufacturer,
        "mpn": p.mpn,
        "package": p.package,
        "read": added.read,
        "written": added.written.as_ref().map(|(file, line)| json!({"file": file, "line": line})),
        "offer": added.offer.as_ref().map(|(file, line)| json!({"file": file, "line": line})),
        "filled": added.filled,
        "to_fill": added.to_fill,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_quoted_only_when_it_must_be() {
        assert_eq!(csv_cell("C25104"), "C25104");
        assert_eq!(csv_cell("a, b"), "\"a, b\"");
        assert_eq!(csv_cell("say \"x\""), "\"say \"\"x\"\"\"");
    }

    #[test]
    fn the_source_names_the_record_the_endpoint_and_the_day() {
        let p = Product {
            lcsc: "C25104".into(),
            manufacturer: Some("UNI-ROYAL".into()),
            mpn: "0402WGF3300TCE".into(),
            package: Some("0402".into()),
            stock: None,
        };
        assert_eq!(
            source_of(
                &p,
                "https://wmsc.lcsc.com/ftps/wm/product/detail",
                "2026-11-12"
            ),
            "LCSC product record for C25104 (the data behind https://www.lcsc.com/product-detail/C25104.html, fetched from https://wmsc.lcsc.com/ftps/wm/product/detail?productCode=C25104) read 2026-11-12 (0402WGF3300TCE, 0402); written by `netsmith catalog add`"
        );
    }
}
