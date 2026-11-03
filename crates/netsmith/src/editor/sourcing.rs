//! The bill of materials: the lowered sourcing, grouped as `bom.csv` is, with what the
//! parts catalog records about each part.

use netsmith_editor::{
    ApiError,
    api::{Bom, BomLine},
};
use netsmith_lower::Assembly;
use netsmith_solve::Catalogs;

use super::analysis::BoardData;
use crate::refresh;

/// Parts that are the same kind, value, package and number go on one line.
pub fn bom(
    data: &BoardData,
    catalogs: &Catalogs,
    today: &str,
    own_catalogs: Vec<String>,
) -> Result<Bom, ApiError> {
    let built = &data.built;
    let libraries = data.libraries()?;
    let mut lines: Vec<BomLine> = Vec::new();
    // A part with `.assembly` `none` is on no bill of materials.
    for (part, source) in built
        .design
        .parts
        .iter()
        .zip(&built.sourcing)
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
        let existing = lines.iter_mut().find(|l| {
            l.value == source.value
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
                let record = source.lcsc.as_deref().and_then(|n| catalogs.supplier(n));
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
                    unit_usd: record.as_ref().and_then(|r| r.unit_price_usd),
                    line_usd: None,
                    stock: record.as_ref().and_then(|r| r.stock),
                    date: record.as_ref().and_then(|r| r.date.clone()),
                    age_days: None,
                    stale: false,
                    hand_solder: source.assembly == Assembly::Hand,
                });
            }
        }
    }
    for line in &mut lines {
        line.designators.sort_by_key(|d| designator_key(d));
        line.line_usd = line.unit_usd.map(|p| p * f64::from(line.quantity));
        // A row with a part number and no date, or an old one, is stale. A made-up part has
        // no record and nothing to be stale about.
        let age = line
            .date
            .as_deref()
            .and_then(|d| refresh::age_days(d, today));
        line.age_days = age.and_then(|a| i32::try_from(a).ok());
        line.stale = line.lcsc.is_some() && age.is_none_or(|a| a > i64::from(refresh::STALE_DAYS));
    }
    lines.sort_by_key(|l| designator_key(&l.designators[0]));
    let (hand, lines): (Vec<_>, Vec<_>) = lines.into_iter().partition(|l| l.hand_solder);
    let total_usd = {
        let priced: Vec<f64> = lines.iter().filter_map(|l| l.line_usd).collect();
        (!priced.is_empty()).then(|| priced.iter().sum())
    };
    Ok(Bom {
        board: built.label(),
        lines,
        hand,
        total_usd,
        note: "Price and stock are the rows of the catalog files (built into netsmith and the \
               project's own) as they were last read, on the date shown; they move, so check \
               JLCPCB's parts page before ordering. They are JLCPCB's numbers, entered by hand; \
               nothing fetches them. \
               \"Check parts against LCSC\" only verifies each row's manufacturer's number and \
               package against LCSC's record and never writes a price or stock. The standard \
               catalogs ship no price, stock or date: read them from the supplier. \
               The total is for one board, without the feeder fee JLCPCB charges for each \
               Extended part and without shipping."
            .to_string(),
        stale_after_days: refresh::STALE_DAYS,
        catalogs: own_catalogs,
    })
}

/// `C2` before `C10`: the letters, then the number.
fn designator_key(designator: &str) -> (String, u32) {
    let split = designator
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(designator.len());
    let (letters, digits) = designator.split_at(split);
    (letters.to_string(), digits.parse().unwrap_or(0))
}
