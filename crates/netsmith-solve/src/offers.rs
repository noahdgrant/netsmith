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
//! | `lcsc` | LCSC's number for the part (`C25104`), which JLCPCB's parts library assembles by too | `Basic`, `Extended` or `Preferred`, the sorting JLCPCB gives the part; empty when JLCPCB's library does not list it |
//!
//! One file holds a supplier's numbers, and a channel reads the file of its supplier: JLCPCB's
//! assembly stock and LCSC's retail stock are two channels of the `lcsc` numbers (ADR 0039, ADR
//! 0047), so `jlcpcb-assembly` takes the `lcsc` offers that have a `class` and `lcsc-retail`
//! takes all of them. A file named for another supplier is an error, as a catalog file whose name
//! is not a catalog's is (ADR 0022), and `jlcpcb.csv`, the file JLCPCB's classes were once in, is
//! one whose message says to merge it into `lcsc.csv`. A new supplier is a new offers file and a
//! new provider (ADR 0143), added here with its rule for `class`.
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
//! **What reads them.** A catalog row has no supplier's number and no supplier's class (ADR 0143,
//! stage 3): [`bought_by`] is the one place a row's number and class are read, for the solver,
//! for resolving a generic part and for the pick lock, from the part's offers. A row with no offer
//! is a part that cannot be bought, and a bill of materials says so.

use std::collections::HashMap;
use std::sync::Arc;

use crate::catalog::Library;
use crate::table::{is_date, parse_csv};

/// The suppliers an offers file may be named for.
pub const SUPPLIERS: [&str; 1] = ["lcsc"];

/// The name of the file JLCPCB's classes were in before they were the `class` of the `lcsc`
/// offers (ADR 0143): an offers file of that name is refused with what to do.
pub const MERGED_INTO_LCSC: &str = "jlcpcb";

/// The message for an offers file `file` named `jlcpcb.csv`.
pub fn merge_into_lcsc(file: &str) -> String {
    format!(
        "{file}: JLCPCB's parts library uses LCSC's numbers, so its offers are the `lcsc` offers now (ADR 0143): merge the file into `lcsc.csv` beside it, the `class` of each of its offers into the `class` of the `lcsc` offer of that part (an `lcsc` offer whose `class` is empty is a part JLCPCB's library does not list), and remove it"
    )
}

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

/// The suppliers as a message lists them: `lcsc.csv`.
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
    /// The supplier the file is named for (`lcsc`).
    pub supplier: String,
    /// The part's manufacturer, as the datasheet spells it; `None` when the offer names none,
    /// and then the offer is of the `mpn` whoever makes it.
    pub manufacturer: Option<String>,
    pub mpn: String,
    /// The supplier's own number for the part.
    pub sku: String,
    /// The supplier's sorting of the part: for `lcsc`, the library JLCPCB's parts library lists
    /// the part in, `None` when it does not list it.
    pub class: Option<Library>,
    pub source: String,
    /// The day a person or `catalog refresh` last found the offer right, `2026-11-02`.
    pub checked: Option<String>,
    pub notes: Option<String>,
    /// The file and line, as a message names it (`offers/lcsc.csv:3`).
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
    /// (`offers/lcsc.csv`). A problem names the file and line.
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
    if supplier == MERGED_INTO_LCSC {
        return Err(merge_into_lcsc(file));
    }
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

/// The `class` cell of an offer of `supplier`, as the supplier sorts its parts: for `lcsc`, the
/// library JLCPCB's parts library lists the part in, written as a catalog row's `library` once
/// was, and empty when it does not list it.
fn class(supplier: &str, cell: Option<&str>) -> Result<Option<Library>, String> {
    match (supplier, cell) {
        (_, None) => Ok(None),
        ("lcsc", Some("Basic")) => Ok(Some(Library::Basic)),
        ("lcsc", Some("Extended")) => Ok(Some(Library::Extended)),
        ("lcsc", Some("Preferred")) => Ok(Some(Library::Preferred)),
        ("lcsc", Some(other)) => Err(format!(
            "class `{other}` is not Basic, Extended or Preferred, the libraries JLCPCB sorts the parts of its assembly library in (empty when JLCPCB's library does not list the part)"
        )),
        (_, Some(other)) => Err(format!(
            "class `{other}`: {supplier} sorts its parts in no classes, so the cell is empty"
        )),
    }
}

/// What a catalog row is bought by: its supplier number and JLCPCB's library of it, and the sku
/// for each channel that has an offer of it, by the channel's word, in the order of
/// [`crate::lock::CHANNELS`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Bought {
    pub lcsc: Option<String>,
    pub library: Option<Library>,
    pub skus: Vec<(String, String)>,
}

/// The supplier number and the class of the part `manufacturer` and `mpn`, read from its offers
/// (ADR 0143): the one function the solver's, a generic's resolution's and the pick lock's
/// reading of them goes through ([`crate::table`]). The number is the `sku` of the part's `lcsc`
/// offer and the class its `class`; `jlcpcb-assembly` has the offer when it has a class, and
/// `lcsc-retail` always. A part with no offer, or a row with no `mpn` (a made-up part), is bought
/// by nothing.
pub(crate) fn bought_by(
    offers: &Offers,
    manufacturer: Option<&str>,
    mpn: &str,
) -> Result<Bought, String> {
    if mpn.is_empty() {
        return Ok(Bought::default());
    }
    let Some(offer) = offers.of("lcsc", manufacturer, mpn)? else {
        return Ok(Bought::default());
    };
    let mut skus = Vec::new();
    if offer.class.is_some() {
        skus.push(("jlcpcb-assembly".to_string(), offer.sku.clone()));
    }
    skus.push(("lcsc-retail".to_string(), offer.sku.clone()));
    Ok(Bought {
        lcsc: Some(offer.sku.clone()),
        library: offer.class,
        skus,
    })
}

impl Offers {
    /// The first offer of any supplier, in the order of [`SUPPLIERS`], whose `sku` is `sku`: what
    /// a supplier's number that a source or a file still names is the part of (ADR 0143).
    pub fn by_sku(&self, sku: &str) -> Option<&Offer> {
        SUPPLIERS.iter().find_map(|supplier| {
            self.by_sku
                .get(&(supplier.to_string(), sku.to_string()))
                .map(|&i| &self.offers[i])
        })
    }
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
        let offers = load("lcsc", ",ZZ1,C1,Basic,the record,2026-11-02,\n").unwrap();
        let offer = offers.of("lcsc", None, "ZZ1").unwrap().unwrap();
        assert_eq!(offer.sku, "C1");
        assert_eq!(offer.class, Some(Library::Basic));
        assert_eq!(offer.at, "offers/lcsc.csv:2");
        assert_eq!(offer.checked.as_deref(), Some("2026-11-02"));
        assert!(offers.of("lcsc", None, "ZZ2").unwrap().is_none());
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
        let e = load("lcsc", ",ZZ1,C1,basic,x,,\n").unwrap_err();
        assert!(
            e.starts_with("offers/lcsc.csv:2: class `basic` is not Basic"),
            "{e}"
        );
        // An empty class is a part JLCPCB's library does not list.
        let offers = load("lcsc", ",ZZ1,C1,,x,,\n").unwrap();
        assert_eq!(offers.of("lcsc", None, "ZZ1").unwrap().unwrap().class, None);
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
            "offers/digikey.csv: `digikey` is not a supplier netsmith knows: an offers file is lcsc.csv"
        );
        let e = Offers::empty()
            .add_file("jlcpcb", "offers/jlcpcb.csv", HEAD)
            .unwrap_err();
        assert!(
            e.starts_with("offers/jlcpcb.csv: JLCPCB's parts library uses LCSC's numbers")
                && e.contains("merge the file into `lcsc.csv`"),
            "{e}"
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
    fn bought_by_reads_the_offers_only() {
        let offers = load("lcsc", ",ZZ1,C1,Extended,x,,\n,ZZ2,C2,,x,,\n").unwrap();
        assert_eq!(
            bought_by(&offers, None, "ZZ1").unwrap(),
            Bought {
                lcsc: Some("C1".into()),
                library: Some(Library::Extended),
                skus: vec![
                    ("jlcpcb-assembly".into(), "C1".into()),
                    ("lcsc-retail".into(), "C1".into())
                ],
            }
        );
        // An offer with no class is not in JLCPCB's assembly library.
        assert_eq!(
            bought_by(&offers, None, "ZZ2").unwrap(),
            Bought {
                lcsc: Some("C2".into()),
                library: None,
                skus: vec![("lcsc-retail".into(), "C2".into())],
            }
        );
        assert_eq!(bought_by(&offers, None, "ZZ3").unwrap(), Bought::default());
        assert_eq!(bought_by(&offers, None, "").unwrap(), Bought::default());
        assert_eq!(offers.by_sku("C2").map(|o| o.mpn.as_str()), Some("ZZ2"));
        assert!(offers.by_sku("C3").is_none());
    }
}
