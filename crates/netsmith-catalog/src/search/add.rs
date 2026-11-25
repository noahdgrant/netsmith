//! `netsmith catalog add --from <supplier> <sku or mpn> [--to catalogs/x.csv]`: a row and its
//! offers made from a search provider's record of one part (ADR 0143, issue #1460).
//!
//! The provider is asked again for the part ([`SearchProvider::record`]): a found part printed by
//! `catalog search` is never written from what was printed. The record must name exactly one part
//! that is the one asked for: one that `supplier` offers under the sku `number`, else one whose
//! `mpn` is `number` exactly (the provider's match ignores `-` and `.`, so its near numbers are
//! not taken) and that `supplier` has exactly one offer of. Anything else writes nothing and says
//! what the record holds, so the sku can be given.
//!
//! With `--to` the row is appended to that catalog file with `mpn`, `package` (only when the
//! record states it, which on Nexar needs `--details`), `source` and `notes`, and `manufacturer`
//! when the file has the column, each as the record spells it. The offers go in the project's
//! offers files of the suppliers the record names (`offers/lcsc.csv`, `offers/digikey.csv`,
//! `offers/mouser.csv`, made with their header when there are none): the named supplier's offer,
//! and the offer of each other supplier that has exactly one (a supplier with several, as cut
//! tape and a reel, is listed, not written: which packaging a project buys is its choice, ADR 0143
//! leaves it open). Each has a `source` that names the record and the day, and the day in
//! `checked`. Every other cell stays empty and is listed as needing the datasheet, as `catalog add
//! --lcsc` does; an LCSC offer's `class` (the library JLCPCB gives the part) is listed too, since
//! no record read here says it. Nothing is written when the catalog file already has the part, or
//! the named supplier's offers file has its sku.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use super::{Answer, FoundPart, SUPPLIERS, SearchProvider, SupplierOffer, SupplyError};
use crate::refresh::add::{AddError, LEFT_EMPTY, csv_cell};
use crate::refresh::csvfile::CsvFile;
use crate::refresh::{date_of, is_lcsc_number, write_atomically};

/// An offer `catalog add --from` wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenOffer {
    pub supplier: String,
    pub sku: String,
    /// The offers file and the line of the offer.
    pub file: String,
    pub line: usize,
}

/// What `catalog add --from` did.
#[derive(Debug, Clone, PartialEq)]
pub struct AddedFrom {
    /// The supplier and the number asked for.
    pub supplier: String,
    pub number: String,
    pub part: FoundPart,
    /// The named supplier's sku of the part.
    pub sku: String,
    /// What the provider is called (`Nexar (Octopart)`).
    pub described: String,
    /// The day the record was read (UTC), as the `source` says.
    pub read: String,
    /// The file and the line of the row written, with `--to`.
    pub written: Option<(String, usize)>,
    pub offers: Vec<WrittenOffer>,
    /// Offers of the record that were not written, and why.
    pub not_written: Vec<String>,
    /// The columns the row has a value in.
    pub filled: Vec<String>,
    /// The columns, and offer cells, a person must fill from the datasheet or a supplier's page.
    pub to_fill: Vec<String>,
    /// What the check of the package found and the person accepted
    /// ([`crate::refresh::Vet`]): empty when the package agrees with its footprint and the
    /// record's description.
    pub package_check: Vec<String>,
}

/// Where the offers of a supplier go: the path and what messages call it.
pub type OffersFile<'a> = &'a dyn Fn(&str) -> Result<(PathBuf, String), String>;

/// The part of `answer` that is `number` at `supplier`, and the index of that supplier's offer in
/// its offers: see the module's header. The error says what the record holds instead.
pub fn pick<'a>(
    answer: &'a Answer,
    supplier: &str,
    number: &str,
) -> Result<(&'a FoundPart, usize), String> {
    let number = number.trim();
    let at = |p: &FoundPart| -> Vec<usize> {
        p.offers
            .iter()
            .enumerate()
            .filter(|(_, o)| o.supplier == supplier)
            .map(|(i, _)| i)
            .collect()
    };
    let by_sku: Vec<(&FoundPart, usize)> = answer
        .parts
        .iter()
        .filter_map(|p| {
            at(p)
                .into_iter()
                .find(|&i| p.offers[i].sku == number)
                .map(|i| (p, i))
        })
        .collect();
    let name = |p: &FoundPart| format!("{} {}", p.manufacturer, p.mpn);
    match by_sku.as_slice() {
        [one] => return Ok(*one),
        [] => {}
        many => {
            return Err(format!(
                "{}'s record has the {supplier} sku {number} on {} parts ({}): nothing was written",
                answer.described,
                many.len(),
                many.iter()
                    .map(|(p, _)| name(p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    let by_mpn: Vec<&FoundPart> = answer.parts.iter().filter(|p| p.mpn == number).collect();
    match by_mpn.as_slice() {
        [] => {
            let near: Vec<String> = answer.parts.iter().map(name).collect();
            Err(format!(
                "{}'s record has no part that {supplier} offers as {number}, and none whose mpn is {number}{}: nothing was written",
                answer.described,
                if near.is_empty() {
                    String::new()
                } else {
                    format!(" (it has {})", near.join(", "))
                }
            ))
        }
        [part] => {
            let offers = at(part);
            match offers.as_slice() {
                [i] => Ok((part, *i)),
                [] => Err(format!(
                    "{}'s record of {} has no offer of {supplier}{}: nothing was written",
                    answer.described,
                    name(part),
                    if part.offers.is_empty() {
                        String::new()
                    } else {
                        format!(
                            " (it has offers of {})",
                            part.offers
                                .iter()
                                .map(|o| o.supplier.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    }
                )),
                many => Err(format!(
                    "{}'s record of {} has {} offers of {supplier} ({}): name one by its sku with --from {supplier} <sku>; nothing was written",
                    answer.described,
                    name(part),
                    many.len(),
                    many.iter()
                        .map(|&i| offer_name(&part.offers[i]))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
        }
        many => Err(format!(
            "{}'s record has {} parts whose mpn is {number} ({}): name one by its {supplier} sku; nothing was written",
            answer.described,
            many.len(),
            many.iter().map(|p| name(p)).collect::<Vec<_>>().join(", ")
        )),
    }
}

fn offer_name(offer: &SupplierOffer) -> String {
    match &offer.packaging {
        Some(p) => format!("{} ({p})", offer.sku),
        None => offer.sku.clone(),
    }
}

/// An offers file read for appending: its path, what messages call it, the parsed file.
struct OffersText {
    path: PathBuf,
    shown: String,
    file: CsvFile,
}

fn read_offers(path: PathBuf, shown: String) -> Result<OffersText, AddError> {
    let stopped = AddError::Stopped;
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            netsmith_solve::migrate::OFFERS_HEADER.to_string()
        }
        Err(e) => return Err(stopped(format!("{shown}: {e}"))),
    };
    let file = CsvFile::parse(&shown, text).map_err(stopped)?;
    if !file.header().iter().any(|h| h.trim() == "sku") {
        return Err(stopped(format!(
            "{shown}: the file has no `sku` column; an offers file has manufacturer, mpn, sku, class, source, checked and notes"
        )));
    }
    Ok(OffersText { path, shown, file })
}

impl OffersText {
    fn column(&self, name: &str) -> Option<usize> {
        self.file.header().iter().position(|h| h.trim() == name)
    }

    /// The line of an offer of this sku, or of this part.
    fn has(&self, sku: &str, manufacturer: Option<&str>, mpn: &str) -> Option<usize> {
        let (s, m, p) = (
            self.column("sku"),
            self.column("manufacturer"),
            self.column("mpn"),
        );
        (1..self.file.len())
            .find(|&r| {
                let cell = |c: Option<usize>| c.map(|c| self.file.value(r, c).trim().to_string());
                let maker = cell(m).unwrap_or_default();
                cell(s).as_deref() == Some(sku)
                    || (cell(p).as_deref() == Some(mpn)
                        && (maker.is_empty() || manufacturer.is_none_or(|x| x == maker)))
            })
            .map(|r| self.file.line(r))
    }
}

/// Ask `provider` for the record of `number` at `supplier`, check it names the part, and with `to`
/// append the row to that catalog file and the offers to the files `offers_file` names. `to` is the
/// path and what messages call it.
pub fn add_from(
    provider: &mut dyn SearchProvider,
    (supplier, number): (&str, &str),
    details: bool,
    to: Option<(&Path, &str)>,
    offers_file: OffersFile<'_>,
    vet: crate::refresh::Vet<'_>,
) -> Result<AddedFrom, AddError> {
    let stopped = AddError::Stopped;
    if !SUPPLIERS.contains(&supplier) {
        return Err(stopped(format!(
            "`{supplier}` is not a supplier netsmith has offers of: {}",
            SUPPLIERS.join(", ")
        )));
    }
    if supplier == "lcsc" && !is_lcsc_number(number) && number.starts_with('C') {
        return Err(stopped(format!(
            "`{number}` is not an LCSC number like C25104"
        )));
    }
    // The catalog file first: a file that cannot take the row is said before anything is asked.
    let catalog = match to {
        Some((path, shown)) => {
            let text = fs::read_to_string(path).map_err(|e| stopped(format!("{shown}: {e}")))?;
            let file = CsvFile::parse(shown, text).map_err(stopped)?;
            if file.is_empty() {
                return Err(stopped(format!(
                    "{shown}: the file is empty; the first line names the columns"
                )));
            }
            let header = file.header();
            for needed in ["mpn", "source"] {
                if !header.iter().any(|h| h.trim() == needed) {
                    return Err(stopped(format!(
                        "{shown}: the file has no `{needed}` column, which a row from a record needs"
                    )));
                }
            }
            if let Some(old) = ["lcsc", "library"]
                .into_iter()
                .find(|c| header.iter().any(|h| h.trim() == *c))
            {
                return Err(stopped(format!(
                    "{shown}: the file has the column `{old}`, which a catalog file no longer has (ADR 0143): run `netsmith project migrate-offers` first"
                )));
            }
            Some((path, shown, file))
        }
        None => None,
    };
    let answer = provider
        .record(supplier, number, details)
        .map_err(|e| match e {
            SupplyError::Offline => stopped(format!(
                "--offline, and no answer for {supplier} {number} is kept: run without --offline to ask {}",
                provider.describe()
            )),
            SupplyError::Unavailable(why) => stopped(why),
        })?;
    let (part, named) = pick(&answer, supplier, number).map_err(AddError::Refused)?;
    let read = date_of(answer.read);
    let mut added = AddedFrom {
        supplier: supplier.to_string(),
        number: number.to_string(),
        part: part.clone(),
        sku: part.offers[named].sku.clone(),
        described: answer.described.clone(),
        read: read.clone(),
        written: None,
        offers: Vec::new(),
        not_written: Vec::new(),
        filled: Vec::new(),
        to_fill: Vec::new(),
        package_check: Vec::new(),
    };
    if supplier == "lcsc" && !is_lcsc_number(&added.sku) {
        return Err(AddError::Refused(format!(
            "{}'s record gives LCSC's number for {} as `{}`, which is not an LCSC number like C25104: nothing was written",
            answer.described, part.mpn, added.sku
        )));
    }
    let Some((path, shown, file)) = catalog else {
        return Ok(added);
    };
    let header = file.header();
    let column = |name: &str| header.iter().position(|h| h.trim() == name);
    let maker_written = column("manufacturer").is_some();
    let mpn_at = column("mpn").expect("checked above");
    let maker_at = column("manufacturer");
    if let Some(record) = (1..file.len()).find(|&r| {
        file.value(r, mpn_at).trim() == part.mpn
            && maker_at.is_none_or(|m| {
                let maker = file.value(r, m);
                maker.trim().is_empty() || maker.trim() == part.manufacturer
            })
    }) {
        return Err(AddError::Refused(format!(
            "{shown}:{}: the file already has a row for {}: add its offers to the offers files instead",
            file.line(record),
            part.mpn
        )));
    }
    let descriptions: Vec<&str> = part.description.as_deref().into_iter().collect();
    added.package_check = vet(part.package.as_deref(), &descriptions).map_err(AddError::Refused)?;
    let manufacturer = maker_written.then(|| part.manufacturer.clone());

    // The offers to write: the named one, and each other supplier's only offer.
    let mut chosen: Vec<&SupplierOffer> = Vec::new();
    for s in SUPPLIERS {
        let of: Vec<&SupplierOffer> = part.offers_of(s).collect();
        if s == supplier {
            chosen.push(&part.offers[named]);
        } else if let [one] = of.as_slice() {
            if s == "lcsc" && !is_lcsc_number(&one.sku) {
                added.not_written.push(format!(
                    "the lcsc offer `{}`: not an LCSC number like C25104",
                    one.sku
                ));
            } else {
                chosen.push(one);
            }
        } else if of.len() > 1 {
            added.not_written.push(format!(
                "{} offers of {s} ({}): which packaging the project buys is its choice; add one with `netsmith catalog add --from {s} <sku>` before the row exists, or write it in offers/{s}.csv by hand",
                of.len(),
                of.iter().map(|o| offer_name(o)).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    let mut writes: Vec<(PathBuf, String, WrittenOffer)> = Vec::new();
    let mut offer_fill: Vec<String> = Vec::new();
    for offer in chosen {
        let (path, file_shown) = offers_file(&offer.supplier).map_err(stopped)?;
        // Each supplier has a file of its own, so a run writes one offer to each.
        let offers = read_offers(path, file_shown)?;
        if let Some(line) = offers.has(&offer.sku, manufacturer.as_deref(), &part.mpn) {
            let why = format!(
                "{}:{line}: the file already has an offer of {} or of {}",
                offers.shown, offer.sku, part.mpn
            );
            if offer.supplier == supplier {
                return Err(AddError::Refused(format!("{why}: nothing was written")));
            }
            added
                .not_written
                .push(format!("the {} offer {}: {why}", offer.supplier, offer.sku));
            continue;
        }
        let source = format!(
            "{} record of {} by {}, read {read} from {} (the match of `{number}` at {}): {}'s offer {}{}; written by `netsmith catalog add --from`",
            answer.described,
            part.mpn,
            part.manufacturer,
            answer.endpoint,
            supplier,
            offer.seller,
            offer.sku,
            offer
                .packaging
                .as_deref()
                .map(|p| format!(" ({p})"))
                .unwrap_or_default()
        );
        let mut text = offers.file.text().to_string();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        let line = text.lines().count() + 1;
        let mut cells = Vec::new();
        for column in offers.file.header().iter().map(|h| h.trim()) {
            let value = match column {
                "manufacturer" => manufacturer.clone().unwrap_or_default(),
                "mpn" => part.mpn.clone(),
                "sku" => offer.sku.clone(),
                "source" => source.clone(),
                "checked" => read.clone(),
                "notes" => format!(
                    "Written by `netsmith catalog add --from` from {}'s record",
                    answer.described
                ),
                _ => String::new(),
            };
            cells.push(csv_cell(&value));
        }
        text.push_str(&cells.join(","));
        text.push('\n');
        if offer.supplier == "lcsc" {
            offer_fill.push(format!(
                "the class of the offer ({}, the library from JLCPCB's page) for a part JLCPCB assembles",
                offers.shown
            ));
        }
        let written = WrittenOffer {
            supplier: offer.supplier.clone(),
            sku: offer.sku.clone(),
            file: offers.shown.clone(),
            line,
        };
        added.offers.push(written.clone());
        writes.push((offers.path.clone(), text, written));
    }

    // The row.
    let mut source = format!(
        "{} record of {} by {}{}, read {read} from {} (the match of `{number}` at {supplier})",
        answer.described,
        part.mpn,
        part.manufacturer,
        part.url
            .as_deref()
            .map(|u| format!(" ({u})"))
            .unwrap_or_default(),
        answer.endpoint,
    );
    if maker_written {
        source.push_str("; manufacturer: as the record spells it");
    }
    if part.package.is_some() {
        source.push_str("; package: the record's `case_package`, as it spells it");
    }
    source.push_str("; written by `netsmith catalog add --from`");
    let mut cells = Vec::new();
    for column in header.iter().map(|h| h.trim()) {
        let value = match column {
            "manufacturer" => manufacturer.clone(),
            "mpn" => Some(part.mpn.clone()),
            "package" => part.package.clone(),
            "source" => Some(source.clone()),
            "notes" => Some(format!(
                "Written by `netsmith catalog add --from` from {}'s record alone: fill the empty columns from the datasheet{} (with its source in `source`), check the package against std/kicad.toml{}",
                answer.described,
                part.datasheet
                    .as_deref()
                    .map(|d| format!(" ({d})"))
                    .unwrap_or_default(),
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
    // The row's columns first, then the offers' cells.
    added.to_fill.append(&mut offer_fill);
    if !maker_written {
        added.to_fill.push(
            "manufacturer: the file has no `manufacturer` column; add it to record the part's maker".to_string(),
        );
    }
    let mut text = file.text().to_string();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    let line = text.lines().count() + 1;
    text.push_str(&cells.join(","));
    text.push('\n');

    for (path, _, _) in &writes {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| stopped(format!("{}: {e}", dir.display())))?;
        }
    }
    write_atomically(path, &text).map_err(stopped)?;
    for (path, text, _) in &writes {
        write_atomically(path, text).map_err(stopped)?;
    }
    added.written = Some((shown.to_string(), line));
    Ok(added)
}

/// What `catalog add --from` prints.
pub fn render_added_from(added: &AddedFrom) -> String {
    let p = &added.part;
    let mut out = format!(
        "{}'s record of {} {} (read {}): {} sku {}{}\n",
        added.described,
        p.manufacturer,
        p.mpn,
        added.read,
        added.supplier,
        added.sku,
        p.package
            .as_deref()
            .map(|k| format!(", package {k}"))
            .unwrap_or_else(|| ", package not stated".to_string())
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
            for o in &added.offers {
                out.push_str(&format!(
                    "added the {} offer {} to {}:{}\n",
                    o.supplier, o.sku, o.file, o.line
                ));
            }
            for why in &added.not_written {
                out.push_str(&format!("not written: {why}\n"));
            }
            if !added.to_fill.is_empty() {
                out.push_str(&format!(
                    "fill from the datasheet (with its source) or the supplier's page: {}\na part that reads an empty one of these refuses the row until it is filled\n",
                    added.to_fill.join(", ")
                ));
            }
        }
    }
    for found in &added.package_check {
        out.push_str(&format!("package check (accepted): {found}\n"));
    }
    out
}

/// What `catalog add --from --format json` prints: what [`render_added_from`] says, as data.
pub fn json_added_from(added: &AddedFrom) -> serde_json::Value {
    json!({
        "command": "catalog add",
        "ok": true,
        "from": {"supplier": added.supplier, "number": added.number},
        "provider": added.described,
        "manufacturer": added.part.manufacturer,
        "mpn": added.part.mpn,
        "package": added.part.package,
        "sku": added.sku,
        "read": added.read,
        "part": super::json_found(&added.part),
        "written": added.written.as_ref().map(|(file, line)| json!({"file": file, "line": line})),
        "offers": added.offers.iter().map(|o| json!({
            "supplier": o.supplier,
            "sku": o.sku,
            "file": o.file,
            "line": o.line,
        })).collect::<Vec<_>>(),
        "not_written": added.not_written,
        "filled": added.filled,
        "to_fill": added.to_fill,
        "package_check": added.package_check,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::Stock;

    fn offer(supplier: &str, sku: &str) -> SupplierOffer {
        SupplierOffer {
            supplier: supplier.into(),
            seller: supplier.into(),
            authorized: true,
            sku: sku.into(),
            stock: Stock::Unknown,
            packaging: None,
            moq: None,
            price_breaks: Vec::new(),
            currency: None,
            updated: None,
        }
    }

    fn part(maker: &str, mpn: &str, offers: Vec<SupplierOffer>) -> FoundPart {
        FoundPart {
            manufacturer: maker.into(),
            mpn: mpn.into(),
            description: None,
            package: None,
            values: Vec::new(),
            datasheet: None,
            url: None,
            offers,
            other_sellers: 0,
        }
    }

    fn answer(parts: Vec<FoundPart>) -> Answer {
        Answer {
            provider: "nexar".into(),
            described: "Nexar (Octopart)".into(),
            endpoint: "http://127.0.0.1/graphql".into(),
            hits: Some(parts.len() as u64),
            parts,
            read: 0,
            cached: false,
            details: false,
            requests: 1,
        }
    }

    #[test]
    fn the_record_must_name_one_part_by_its_sku_or_its_exact_mpn() {
        let a = answer(vec![
            part(
                "Acme",
                "ZZ-1",
                vec![offer("digikey", "DK-1"), offer("digikey", "DK-2")],
            ),
            part("Acme", "ZZ1", vec![offer("mouser", "M-1")]),
        ]);
        assert_eq!(pick(&a, "digikey", "DK-2").unwrap().1, 1);
        assert_eq!(pick(&a, "mouser", "ZZ1").unwrap().0.mpn, "ZZ1");
        // Two packagings of one supplier: the sku must say which.
        let e = pick(&a, "digikey", "ZZ-1").unwrap_err();
        assert!(e.contains("2 offers of digikey (DK-1, DK-2)"), "{e}");
        // A near number the provider matched is not the part.
        let e = pick(&a, "mouser", "ZZ-1X").unwrap_err();
        assert!(
            e.contains("none whose mpn is ZZ-1X") && e.contains("Acme ZZ1"),
            "{e}"
        );
        let e = pick(&a, "lcsc", "ZZ1").unwrap_err();
        assert!(
            e.contains("has no offer of lcsc (it has offers of mouser)"),
            "{e}"
        );
        let two = answer(vec![
            part("Acme", "ZZ9", vec![offer("mouser", "M-1")]),
            part("Other", "ZZ9", vec![offer("mouser", "M-2")]),
        ]);
        let e = pick(&two, "mouser", "ZZ9").unwrap_err();
        assert!(e.contains("2 parts whose mpn is ZZ9"), "{e}");
    }
}
