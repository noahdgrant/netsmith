//! Offers files (ADR 0143, stage 2): what a supplier says about a part that a person can
//! record, one CSV file for each supplier.
//!
//! An **offer** is a row of `offers/<supplier>.csv` with the columns `manufacturer`, `mpn`,
//! `sku`, `class`, `source`, `checked` and `notes`, all seven in the header, in any order, and no
//! other. The part is named by its identity (ADR 0143), the `mpn` and the `manufacturer` as the
//! catalog row has them; `sku` is the supplier's own number for it, `class` the supplier's own
//! sorting of it, and `source` where both were read. The file holds no price and no stock: those
//! are a supply provider's quote (ADR 0047).
//!
//! | supplier | `sku` | `class` |
//! |---|---|---|
//! | `jlcpcb` | the LCSC number JLCPCB assembles the part by (`C25104`) | `Basic`, `Extended` or `Preferred`, as `library` is written: required |
//! | `lcsc` | LCSC's number for the part (`C25104`) | empty: LCSC sorts its parts in no classes |
//!
//! The suppliers are these two: a file named for another (`digikey.csv`) is an error, as a
//! catalog file whose name is not a catalog's is (ADR 0022). A new supplier is a new offers file
//! and a new provider (ADR 0143), added here with its rule for `class`.
//!
//! **What the loader refuses**, each with the `file:line`: a column that is not one of the seven
//! or a missing one, an offer with no `mpn` or no `sku`, an offer with no `source` (no number
//! comes from memory), a `class` the supplier does not have, a `checked` that is not a day
//! (`2026-11-02`), a `manufacturer` with no `mpn`, and a supplier that offers one part twice or
//! one `sku` for two parts. One part is the same `mpn`, and the same `manufacturer` when both
//! offers name one: an offer with no manufacturer is its `mpn` whoever makes it, as a catalog row
//! with none is ([`crate::table`]). Two offers of one part from one supplier are refused across
//! the standard file and a project's too, so a project cannot give a standard part another
//! number. An offer whose part is in no catalog is not an error: the build says so in a note
//! ([`crate::Catalogs::offer_notes`]), since the catalog that has it may be a dependency's that is
//! not read yet, or a row that was removed.
//!
//! **Where they are.** The standard files are `data/offers/<supplier>.csv`, embedded by
//! `build.rs`; a project's own are in the folders of its manifest's `offer_dirs` (default
//! `["offers"]`) and add their offers after the standard ones.
//!
//! **What reads them.** Until stage 3 (issue #1458) the catalog rows keep their `lcsc` and
//! `library` columns, and [`bought_by`] is the one place a row's supplier number and class are
//! read for the solver and for resolving a generic part: the `jlcpcb` offer, then the `lcsc`
//! offer, and the row's own column where there is no offer. A test holds every standard row's
//! columns equal to its offers (`tests/offers.rs`), so the two cannot drift before the columns go.

use std::collections::HashMap;
use std::sync::Arc;

use crate::catalog::Library;
use crate::table::{is_date, parse_csv};

/// The suppliers an offers file may be named for, in the order their offers are read for a
/// row's supplier number ([`bought_by`]).
pub const SUPPLIERS: [&str; 2] = ["jlcpcb", "lcsc"];

/// The folder of a project's offers files when its manifest says nothing, and of the standard
/// ones under `data/`.
pub const FOLDER: &str = "offers";

/// The columns of an offers file: every one of them, and no other.
const COLUMNS: [&str; 7] = [
    "manufacturer",
    "mpn",
    "sku",
    "class",
    "source",
    "checked",
    "notes",
];

/// Whether an offers file may be named `<name>.csv`: `name` is one of [`SUPPLIERS`].
pub fn is_supplier(name: &str) -> bool {
    SUPPLIERS.contains(&name)
}

/// The suppliers as a message lists them: `jlcpcb.csv or lcsc.csv`.
pub fn supplier_files() -> String {
    SUPPLIERS
        .iter()
        .map(|s| format!("{s}.csv"))
        .collect::<Vec<_>>()
        .join(" or ")
}

/// One row of an offers file.
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    /// The supplier the file is named for (`jlcpcb`).
    pub supplier: String,
    /// The part's manufacturer, as the datasheet spells it; `None` when the offer names none,
    /// and then the offer is of the `mpn` whoever makes it.
    pub manufacturer: Option<String>,
    pub mpn: String,
    /// The supplier's own number for the part.
    pub sku: String,
    /// JLCPCB's library of the part: `Some` for every offer of `jlcpcb`, `None` for `lcsc`.
    pub class: Option<Library>,
    pub source: String,
    /// The day a person or `catalog refresh` last found the offer right, `2026-11-02`.
    pub checked: Option<String>,
    pub notes: Option<String>,
    /// The file and line, as a message names it (`offers/jlcpcb.csv:3`).
    pub at: String,
    /// Whether the offer is from a file that ships in the binary (`data/offers/`).
    pub built_in: bool,
}

impl Offer {
    /// Whether this offer and a part named by `manufacturer` and `mpn` are one part by its
    /// identity (ADR 0143): the same `mpn`, and the same manufacturer when both name one.
    pub fn is_of(&self, manufacturer: Option<&str>, mpn: &str) -> bool {
        !mpn.is_empty()
            && self.mpn == mpn
            && match (self.manufacturer.as_deref(), manufacturer) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    }

    /// What a message calls the part: `` `0402WGF3300TCE` `` or, with its manufacturer,
    /// `` `0402WGF3300TCE` of `UNI-ROYAL` ``.
    fn part(&self) -> String {
        match &self.manufacturer {
            Some(m) => format!("`{}` of `{m}`", self.mpn),
            None => format!("`{}`", self.mpn),
        }
    }
}

/// The offers of every supplier, the standard ones and then a project's, in the order they were
/// read.
#[derive(Debug, Clone, Default)]
pub struct Offers {
    offers: Vec<Offer>,
    /// The offers of each supplier and `mpn`, by their place in `offers`.
    by_mpn: HashMap<(String, String), Vec<usize>>,
    /// The offer of each supplier and `sku`, by its place in `offers`.
    by_sku: HashMap<(String, String), usize>,
}

impl Offers {
    pub fn empty() -> Offers {
        Offers::default()
    }

    /// Read an offers file of `supplier` (the file's name without `.csv`, one of
    /// [`SUPPLIERS`]) and add its offers after these. `file` is what a message calls it
    /// (`offers/jlcpcb.csv`). A problem names the file and line.
    pub fn add_file(&mut self, supplier: &str, file: &str, text: &str) -> Result<(), String> {
        let offers = parse(supplier, file, text, false)?;
        self.add(offers)
    }

    /// [`Self::add_file`] of a file that ships in the binary (`data/offers/`).
    pub(crate) fn add_built_in_file(
        &mut self,
        supplier: &str,
        file: &str,
        text: &str,
    ) -> Result<(), String> {
        let offers = parse(supplier, file, text, true)?;
        self.add(offers)
    }

    /// Add offers after these, refusing a supplier's second offer of one part and its second
    /// part of one `sku`, with both places named.
    fn add(&mut self, offers: Vec<Offer>) -> Result<(), String> {
        for offer in offers {
            let sku = (offer.supplier.clone(), offer.sku.clone());
            if let Some(earlier) = self.by_sku.get(&sku).map(|&i| &self.offers[i]) {
                return Err(format!(
                    "{}: the sku `{}` is already the offer at {} (of {}); a supplier's number is one part",
                    offer.at,
                    offer.sku,
                    earlier.at,
                    earlier.part()
                ));
            }
            let key = (offer.supplier.clone(), offer.mpn.clone());
            let same = self
                .by_mpn
                .get(&key)
                .into_iter()
                .flatten()
                .map(|&i| &self.offers[i]);
            if let Some(earlier) = same
                .into_iter()
                .find(|o| o.is_of(offer.manufacturer.as_deref(), &offer.mpn))
            {
                let blank = match (&earlier.manufacturer, &offer.manufacturer) {
                    (None, Some(_)) => Some(&earlier.at),
                    (Some(_), None) => Some(&offer.at),
                    _ => None,
                };
                let why = match blank {
                    Some(at) => format!(
                        " (the offer at {at} names no manufacturer, which is that mpn whoever makes it: give it its manufacturer to make the two offers of two parts)"
                    ),
                    None => String::new(),
                };
                return Err(format!(
                    "{}: {} already has an offer of {} at {}{why}; a supplier offers a part once",
                    offer.at,
                    offer.part(),
                    offer.supplier,
                    earlier.at
                ));
            }
            let index = self.offers.len();
            self.by_mpn.entry(key).or_default().push(index);
            self.by_sku.insert(sku, index);
            self.offers.push(offer);
        }
        Ok(())
    }

    /// Every offer, in the order it was read.
    pub fn iter(&self) -> impl Iterator<Item = &Offer> {
        self.offers.iter()
    }

    pub fn len(&self) -> usize {
        self.offers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.offers.is_empty()
    }

    /// The offer of `supplier` for the part named by `manufacturer` and `mpn`, by its identity
    /// ([`Offer::is_of`]); `None` when the supplier offers no such part. A part that names no
    /// manufacturer, of whose `mpn` the supplier offers two parts of two manufacturers, is an
    /// error: which number it is bought by would be a guess.
    pub fn of(
        &self,
        supplier: &str,
        manufacturer: Option<&str>,
        mpn: &str,
    ) -> Result<Option<&Offer>, String> {
        let Some(found) = self.by_mpn.get(&(supplier.to_string(), mpn.to_string())) else {
            return Ok(None);
        };
        let mut matching = found
            .iter()
            .map(|&i| &self.offers[i])
            .filter(|o| o.is_of(manufacturer, mpn));
        let first = matching.next();
        if let (Some(first), Some(second)) = (first, matching.next()) {
            return Err(format!(
                "the mpn `{mpn}` names no manufacturer, and {supplier} offers two parts of it ({} at {} and {} at {})",
                first.part(),
                first.at,
                second.part(),
                second.at
            ));
        }
        Ok(first)
    }
}

/// Read one offers file of `supplier`. `built_in` marks the files of `data/offers/`.
fn parse(supplier: &str, file: &str, text: &str, built_in: bool) -> Result<Vec<Offer>, String> {
    if !is_supplier(supplier) {
        return Err(format!(
            "{file}: `{supplier}` is not a supplier netsmith knows: an offers file is {}",
            supplier_files()
        ));
    }
    let records =
        parse_csv(text).map_err(|e| format!("{file}:{}", e.trim_start_matches("line ")))?;
    let Some(((line, names), body)) = records.split_first() else {
        return Err(format!(
            "{file}: the file is empty; the first line names the columns, {}",
            COLUMNS.join(", ")
        ));
    };
    let names: Vec<String> = names.iter().map(|n| n.trim().to_string()).collect();
    for (i, n) in names.iter().enumerate() {
        if !COLUMNS.contains(&n.as_str()) {
            return Err(format!(
                "{file}:{line}: `{n}` is not a column of an offers file, which has {} (a price or a stock is a supply provider's quote, ADR 0047)",
                COLUMNS.join(", ")
            ));
        }
        if names[..i].contains(n) {
            return Err(format!("{file}:{line}: the column `{n}` is there twice"));
        }
    }
    if let Some(missing) = COLUMNS.iter().find(|c| !names.iter().any(|n| n == *c)) {
        return Err(format!(
            "{file}:{line}: there is no `{missing}` column; an offers file has {} (a cell may be empty where the column allows it)",
            COLUMNS.join(", ")
        ));
    }
    let column = |name: &str| {
        names
            .iter()
            .position(|n| n == name)
            .expect("every column was checked to be there")
    };
    let file: Arc<str> = file.into();
    let mut offers = Vec::new();
    for (line, fields) in body {
        let at = format!("{file}:{line}");
        if fields.len() != names.len() {
            return Err(format!(
                "{at}: the offer has {} fields but the header has {}",
                fields.len(),
                names.len()
            ));
        }
        let cell = |name: &str| {
            let text = fields[column(name)].trim();
            (!text.is_empty()).then(|| text.to_string())
        };
        let manufacturer = cell("manufacturer");
        let Some(mpn) = cell("mpn") else {
            return Err(match &manufacturer {
                Some(m) => format!(
                    "{at}: the offer names the manufacturer `{m}` and no `mpn`; a part is its manufacturer and its part number (ADR 0143)"
                ),
                None => format!(
                    "{at}: the offer has no `mpn`; an offer names its part by the manufacturer's part number, and its `manufacturer` when two parts share it (ADR 0143)"
                ),
            });
        };
        let Some(sku) = cell("sku") else {
            return Err(format!(
                "{at}: the offer of `{mpn}` has no `sku`; write the {supplier} number of the part"
            ));
        };
        let Some(source) = cell("source") else {
            return Err(format!(
                "{at}: the offer of `{mpn}` (sku `{sku}`) has no source; every offer needs the URL or text its number was read from in its `source` column, as a catalog row does: no number comes from memory"
            ));
        };
        let class = class(supplier, cell("class").as_deref()).map_err(|e| format!("{at}: {e}"))?;
        let checked = cell("checked");
        if let Some(day) = &checked
            && !is_date(day)
        {
            return Err(format!("{at}: checked `{day}` is not like 2026-10-31"));
        }
        offers.push(Offer {
            supplier: supplier.to_string(),
            manufacturer,
            mpn,
            sku,
            class,
            source,
            checked,
            notes: cell("notes"),
            at,
            built_in,
        });
    }
    Ok(offers)
}

/// The `class` cell of an offer of `supplier`, as the supplier sorts its parts: JLCPCB's
/// library, written as a catalog row's `library` is and required, and nothing for LCSC.
fn class(supplier: &str, cell: Option<&str>) -> Result<Option<Library>, String> {
    match (supplier, cell) {
        ("jlcpcb", Some("Basic")) => Ok(Some(Library::Basic)),
        ("jlcpcb", Some("Extended")) => Ok(Some(Library::Extended)),
        ("jlcpcb", Some("Preferred")) => Ok(Some(Library::Preferred)),
        ("jlcpcb", Some(other)) => Err(format!(
            "class `{other}` is not Basic, Extended or Preferred, the libraries JLCPCB sorts its parts in"
        )),
        ("jlcpcb", None) => Err(
            "the offer has no class; JLCPCB sorts every part it assembles in its Basic, Extended or Preferred library: write which"
                .to_string(),
        ),
        (_, None) => Ok(None),
        (_, Some(other)) => Err(format!(
            "class `{other}`: {supplier} sorts its parts in no classes, so the cell is empty"
        )),
    }
}

/// What a catalog row is bought by: its supplier number and JLCPCB's library of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Bought {
    pub lcsc: Option<String>,
    pub library: Option<Library>,
}

/// The supplier number and the class of the part `manufacturer` and `mpn`, whose catalog row
/// still has the columns `lcsc` and `library` (ADR 0143, stage 2): the one function the solver's
/// and a generic's resolution's reading of them goes through ([`crate::table`]). The number is
/// the `sku` of the part's `jlcpcb` offer, else of its `lcsc` offer, else the row's `lcsc`; the
/// class is the `class` of its `jlcpcb` offer, else the row's `library`. A row with no `mpn` has
/// no offer and is its columns. Stage 3 (issue #1458) removes the columns, and the offers alone
/// remain.
pub(crate) fn bought_by(
    offers: &Offers,
    manufacturer: Option<&str>,
    mpn: &str,
    columns: Bought,
) -> Result<Bought, String> {
    if mpn.is_empty() {
        return Ok(columns);
    }
    let jlcpcb = offers.of("jlcpcb", manufacturer, mpn)?;
    let lcsc = offers.of("lcsc", manufacturer, mpn)?;
    Ok(Bought {
        lcsc: jlcpcb.or(lcsc).map(|o| o.sku.clone()).or(columns.lcsc),
        library: jlcpcb.and_then(|o| o.class).or(columns.library),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "manufacturer,mpn,sku,class,source,checked,notes\n";

    fn load(supplier: &str, rows: &str) -> Result<Offers, String> {
        let mut offers = Offers::empty();
        offers.add_file(
            supplier,
            &format!("offers/{supplier}.csv"),
            &format!("{HEAD}{rows}"),
        )?;
        Ok(offers)
    }

    #[test]
    fn an_offer_is_read_with_its_class() {
        let offers = load("jlcpcb", ",ZZ1,C1,Basic,the record,2026-11-02,\n").unwrap();
        let offer = offers.of("jlcpcb", None, "ZZ1").unwrap().unwrap();
        assert_eq!(offer.sku, "C1");
        assert_eq!(offer.class, Some(Library::Basic));
        assert_eq!(offer.at, "offers/jlcpcb.csv:2");
        assert_eq!(offer.checked.as_deref(), Some("2026-11-02"));
        assert!(offers.of("lcsc", None, "ZZ1").unwrap().is_none());
    }

    #[test]
    fn the_columns_are_the_seven_in_any_order() {
        let mut offers = Offers::empty();
        offers
            .add_file(
                "lcsc",
                "offers/lcsc.csv",
                "sku,mpn,manufacturer,source,class,notes,checked\nC1,ZZ1,,the record,,,\n",
            )
            .unwrap();
        assert_eq!(offers.len(), 1);
        let e = Offers::empty()
            .add_file("lcsc", "offers/lcsc.csv", "mpn,sku,source\nZZ1,C1,x\n")
            .unwrap_err();
        assert!(
            e.starts_with("offers/lcsc.csv:1: there is no `manufacturer` column"),
            "{e}"
        );
        let e = Offers::empty()
            .add_file(
                "lcsc",
                "offers/lcsc.csv",
                "manufacturer,mpn,sku,class,source,checked,notes,price\n,ZZ1,C1,,x,,,1\n",
            )
            .unwrap_err();
        assert!(
            e.contains("`price` is not a column of an offers file"),
            "{e}"
        );
    }

    #[test]
    fn the_class_is_the_suppliers_own() {
        let e = load("jlcpcb", ",ZZ1,C1,basic,x,,\n").unwrap_err();
        assert!(
            e.starts_with("offers/jlcpcb.csv:2: class `basic` is not Basic"),
            "{e}"
        );
        let e = load("jlcpcb", ",ZZ1,C1,,x,,\n").unwrap_err();
        assert!(e.contains("the offer has no class"), "{e}");
        let e = load("lcsc", ",ZZ1,C1,Basic,x,,\n").unwrap_err();
        assert!(e.contains("lcsc sorts its parts in no classes"), "{e}");
    }

    #[test]
    fn the_checked_day_is_a_date() {
        let e = load("lcsc", ",ZZ1,C1,,x,yesterday,\n").unwrap_err();
        assert_eq!(
            e,
            "offers/lcsc.csv:2: checked `yesterday` is not like 2026-10-31"
        );
    }

    #[test]
    fn a_file_not_named_for_a_supplier_is_refused() {
        let e = Offers::empty()
            .add_file("digikey", "offers/digikey.csv", HEAD)
            .unwrap_err();
        assert_eq!(
            e,
            "offers/digikey.csv: `digikey` is not a supplier netsmith knows: an offers file is jlcpcb.csv or lcsc.csv"
        );
    }

    #[test]
    fn a_manufacturer_tells_two_parts_of_one_mpn_apart() {
        let offers = load("lcsc", "Acme,ZZ1,C1,,x,,\nWidget Co,ZZ1,C2,,x,,\n").unwrap();
        let sku = |m| offers.of("lcsc", m, "ZZ1").unwrap().map(|o| o.sku.clone());
        assert_eq!(sku(Some("Acme")).as_deref(), Some("C1"));
        assert_eq!(sku(Some("Widget Co")).as_deref(), Some("C2"));
        assert_eq!(sku(Some("Other")), None);
        let e = offers.of("lcsc", None, "ZZ1").unwrap_err();
        assert!(e.contains("offers two parts of it"), "{e}");
    }

    #[test]
    fn bought_by_reads_the_offers_and_falls_back_to_the_columns() {
        let mut offers = load("jlcpcb", ",ZZ1,C1,Extended,x,,\n").unwrap();
        offers
            .add_file("lcsc", "offers/lcsc.csv", &format!("{HEAD},ZZ2,C2,,x,,\n"))
            .unwrap();
        let columns = || Bought {
            lcsc: Some("C9".into()),
            library: Some(Library::Basic),
        };
        assert_eq!(
            bought_by(&offers, None, "ZZ1", columns()).unwrap(),
            Bought {
                lcsc: Some("C1".into()),
                library: Some(Library::Extended)
            }
        );
        // The LCSC offer gives the number, and the row's column the class.
        assert_eq!(
            bought_by(&offers, None, "ZZ2", columns()).unwrap(),
            Bought {
                lcsc: Some("C2".into()),
                library: Some(Library::Basic)
            }
        );
        assert_eq!(
            bought_by(&offers, None, "ZZ3", columns()).unwrap(),
            columns()
        );
        assert_eq!(bought_by(&offers, None, "", columns()).unwrap(), columns());
    }
}
