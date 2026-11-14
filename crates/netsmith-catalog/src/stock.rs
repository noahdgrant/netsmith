//! The note for a part the solver picked although a file says it is out of stock (issue #176).
//!
//! The solver does not read stock (ADR 0047) and the pick is the same with or without this
//! note. After the design stage, when each part has its reference designator, a part whose catalog row
//! states a `stock` of 0, or whose LCSC number the standard `considered.csv` records as out of
//! stock when it was read, gets one line among the board's notes. An empty `stock` cell states
//! nothing and gets none, and neither does a consigned part (ADR 0084): the owner ships it. The editor shows the same line.

use netsmith_design::{Assembly, Designed};
use netsmith_solve::{Catalogs, SoldOut};

/// The notes of a designed board, one for each part (a bank is one part) that a file says is
/// out of stock, in the order of the design.
pub fn notes(designed: &Designed, catalogs: &Catalogs) -> Vec<String> {
    // Each part of a bank has the same pick: one note, naming every reference designator.
    let mut found: Vec<(netsmith_check::InstId, Vec<&str>, usize, SoldOut)> = Vec::new();
    for (i, (sourced, origin)) in designed.sourcing.iter().zip(&designed.origins).enumerate() {
        if let Some(entry) = found.iter_mut().find(|(o, ..)| o == origin) {
            entry.1.push(&sourced.refdes);
            continue;
        }
        // The owner buys and ships a consigned part, so what a supplier has of it does not matter.
        if sourced.assembly == Assembly::Consigned {
            continue;
        }
        let Some(lcsc) = sourced.lcsc.as_deref() else {
            continue;
        };
        if let Some(why) = catalogs.sold_out(lcsc) {
            found.push((*origin, vec![&sourced.refdes], i, why));
        }
    }
    found
        .into_iter()
        .map(|(_, refdes, i, why)| {
            let banked = refdes.len() > 1;
            let sourced = &designed.sourcing[i];
            let lcsc = sourced.lcsc.as_deref().unwrap_or_default();
            let part = match sourced.mpn.as_deref() {
                Some(mpn) => format!("{lcsc} ({mpn})"),
                None => lcsc.to_string(),
            };
            // A bank of two or more has the paths `bulk[0]`, `bulk[1]`: the note is for the use.
            let path = &designed.design.parts[i].path;
            let path = match path.strip_suffix(']').and_then(|p| p.rsplit_once('[')) {
                Some((use_path, _)) if banked => use_path,
                _ => path,
            };
            let reason = match why {
                SoldOut::Stated { at } => {
                    format!("its catalog row ({at}) states a stock of 0")
                }
                SoldOut::Considered => "data/jlcpcb/considered.csv records it as out of stock \
                     when it was read, and a catalog of this project offers it"
                    .to_string(),
            };
            format!(
                "{} (`{path}`) is {part}, and {reason}: the solver does not read stock, so it \
                 picked the part; choose another row of the catalog or state a different value, \
                 and read the supplier's stock before ordering",
                refdes.join(", ")
            )
        })
        .collect()
}
