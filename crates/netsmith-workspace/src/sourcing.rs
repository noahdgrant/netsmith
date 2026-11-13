//! The bill of materials: the sourcing the design stage made, grouped as `bom.csv` is, with what the
//! catalog records about each part and what the supply provider says about its price and
//! stock (ADR 0047). A generic part is first resolved to the catalog row that stands for it.
//! A price or stock nobody can give is unknown, and the line says why.

use netsmith_design::Assembly;
use netsmith_editor::{
    ApiError,
    api::{Bom, BomLine},
};
use netsmith_solve::Catalogs;

use super::analysis::BoardData;
use netsmith_catalog::refresh;
use netsmith_catalog::resolve::resolve_generics;
use netsmith_catalog::supply::{Lookup, PartKey, Quote, Supply};

/// Parts that are the same kind, value, package and number go on one line.
pub fn bom(
    data: &BoardData,
    catalogs: &Catalogs,
    supply: &Supply,
    today: &str,
    own_catalogs: Vec<String>,
) -> Result<Bom, ApiError> {
    let built = &data.built;
    let libraries = data.libraries()?;
    let mut lines: Vec<BomLine> = Vec::new();
    let (sourcing, unresolved) = resolve_generics(&built.sourcing, catalogs);
    // A part with `.assembly` `none` is on no bill of materials.
    for (part, source) in built
        .design
        .parts
        .iter()
        .zip(&sourcing)
        .filter(|(_, s)| s.assembly != Assembly::None)
    {
        let footprint = libraries
            .mapping
            .map(&source.kind, source.package.as_deref())
            .ok()
            .map(|m| {
                m.footprint
                    .rsplit(':')
                    .next()
                    .unwrap_or(&m.footprint)
                    .to_string()
            });
        // The generic a picked provider stands for (ADR 0068), for a part that is a use itself.
        // Every capacitor of a bank stands for it: the later ones are `x#2`, `x#3` of the use.
        let via = built
            .providers
            .iter()
            .find(|p| p.path == use_path(&part.path))
            .map(|p| p.generic.clone())
            .filter(|generic| *generic != source.kind);
        let existing = lines.iter_mut().find(|l| {
            l.via == via
                && l.value == source.value
                && l.footprint == footprint
                && l.lcsc == source.lcsc
                && l.kind == source.kind
                && l.hand_solder == (source.assembly == Assembly::Hand)
        });
        match existing {
            Some(line) => {
                line.designators.push(part.refdes.clone());
                line.quantity += 1;
            }
            None => {
                lines.push(BomLine {
                    designators: vec![part.refdes.clone()],
                    quantity: 1,
                    value: source.value.clone(),
                    kind: source.kind.clone(),
                    package: source.package.clone(),
                    footprint,
                    lcsc: source.lcsc.clone(),
                    mpn: source.mpn.clone(),
                    library: source.library.map(|l| l.to_string()),
                    unit_usd: None,
                    line_usd: None,
                    stock: None,
                    date: None,
                    age_days: None,
                    stale: false,
                    supply: None,
                    unknown: None,
                    unresolved: unresolved
                        .iter()
                        .find(|u| u.refdes == part.refdes)
                        .map(|u| u.checked(&built.rechecks, catalogs)),
                    hand_solder: source.assembly == Assembly::Hand,
                    via,
                });
            }
        }
    }
    for line in &mut lines {
        line.designators.sort_by_key(|d| designator_key(d));
        price(line, catalogs, supply, today);
    }
    lines.sort_by_key(|l| designator_key(&l.designators[0]));
    let (hand, lines): (Vec<_>, Vec<_>) = lines.into_iter().partition(|l| l.hand_solder);
    let total_usd = {
        let priced: Vec<f64> = lines.iter().filter_map(|l| l.line_usd).collect();
        (!priced.is_empty()).then(|| priced.iter().sum())
    };
    let unpriced = lines
        .iter()
        .filter(|l| l.unit_usd.is_none() && (l.lcsc.is_some() || l.unresolved.is_some()))
        .count();
    let mut note = String::from(
        "Price and stock come from the supply provider the project configures ([supply] in \
         netsmith.toml), cached with the time they were read. Where no provider has the part, \
         a catalog row's own price, stock and date cells (JLCPCB's numbers, entered by hand) \
         are used if it has them; otherwise they are unknown: nothing guesses them, and an \
         unknown stock is not 0. They move, so check the supplier before ordering. JLCPCB \
         assembly stock and LCSC retail stock differ; each line says which one it shows. \
         \"Check parts against LCSC\" only verifies each row's manufacturer's number and \
         package against LCSC's record and never writes a price or stock. \
         The total is for one board, without the feeder fee JLCPCB charges for each Extended \
         part and without shipping.",
    );
    if unpriced > 0 {
        note.push_str(&format!(
            " The total leaves out {unpriced} line{} with no known price.",
            if unpriced == 1 { "" } else { "s" }
        ));
    }
    Ok(Bom {
        board: built.label(),
        lines,
        hand,
        total_usd,
        note,
        stale_after_days: refresh::STALE_DAYS,
        catalogs: own_catalogs,
    })
}

/// Fill in a line's price, stock and the day they were read from the provider; if it has
/// nothing, from the catalog row's own cells; and if that has nothing, say they are unknown
/// and why.
fn price(line: &mut BomLine, catalogs: &Catalogs, supply: &Supply, today: &str) {
    // A made-up part (no number) has nothing to look up and shows nothing.
    let Some(lcsc) = line.lcsc.clone() else {
        return;
    };
    let key = PartKey {
        lcsc: Some(lcsc.clone()),
        mpn: line.mpn.clone(),
    };
    let reason = match supply.lookup(&key) {
        Lookup::Quoted(quote) => {
            if quote.currency == "USD" {
                use_quote(line, &quote, today);
                return;
            }
            format!(
                "{} answered in {}, and only US dollars are shown",
                quote.provider, quote.currency
            )
        }
        Lookup::Unknown(why) => why,
    };
    // The cells of the catalog row, entered by hand, are the last resort.
    if let Some(record) = catalogs.supplier(&lcsc)
        && (record.unit_price_usd.is_some() || record.stock.is_some())
    {
        line.unit_usd = record.unit_price_usd;
        line.stock = record.stock;
        line.date = record.date.clone();
        line.supply = Some("catalog file, entered by hand".to_string());
        age(line, today, record.date.as_deref());
        line.line_usd = line.unit_usd.map(|p| p * f64::from(line.quantity));
        return;
    }
    line.unknown = Some(reason);
}

fn use_quote(line: &mut BomLine, quote: &Quote, today: &str) {
    let day = refresh::date_of(quote.fetched);
    line.stock = quote.stock;
    line.date = Some(day.clone());
    line.supply = Some(quote.source());
    if let Some((unit, from)) = quote.price_for(line.quantity) {
        line.unit_usd = Some(unit);
        line.line_usd = Some(unit * f64::from(line.quantity));
        if let Some(from) = from {
            line.supply = Some(format!(
                "{}; the price is for {from} pieces or more",
                quote.source()
            ));
        }
    }
    age(line, today, Some(&day));
}

/// How old the numbers are, and whether that is more than the stale age.
fn age(line: &mut BomLine, today: &str, date: Option<&str>) {
    let age = date.and_then(|d| refresh::age_days(d, today));
    line.age_days = age.and_then(|a| i32::try_from(a).ok());
    line.stale = age.is_some_and(|a| a > i64::from(refresh::STALE_DAYS));
}

/// The path of the use a part belongs to: a bank's second and later capacitors are the use's
/// path and `#2`, `#3` (ADR 0052), and the first has the path itself.
fn use_path(part_path: &str) -> &str {
    match part_path.rsplit_once('#') {
        Some((use_path, k)) if !k.is_empty() && k.bytes().all(|b| b.is_ascii_digit()) => use_path,
        _ => part_path,
    }
}

/// `C2` before `C10`: the letters, then the number.
fn designator_key(designator: &str) -> (String, u32) {
    let split = designator
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(designator.len());
    let (letters, digits) = designator.split_at(split);
    (letters.to_string(), digits.parse().unwrap_or(0))
}
