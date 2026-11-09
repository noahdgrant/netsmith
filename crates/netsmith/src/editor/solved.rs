//! What the solver worked out for a part and the source did not state: the capacitance in
//! use after the DC bias, the tolerance it chose, the ratings of the row it picked and the
//! row's identity. Read from the solved model, as the power tree is.

use netsmith_check::{Domain, InstId, Member, Model};
use netsmith_design::Sourced;
use netsmith_editor::api::{Range, Rating, Solved, SolvedRow, Tolerance};
use netsmith_quantity::{Dimension, Quantity};

/// The part `inst`, which is sourced as `source`, or `None` if there is nothing to say.
pub fn of(model: &Model, inst: InstId, source: &Sourced) -> Option<Solved> {
    let fields: Vec<(&str, &netsmith_check::Field)> = model
        .instance(inst)
        .children
        .iter()
        .filter_map(|(name, member)| match member {
            Member::Field(f) => Some((name.as_str(), model.field(*f))),
            _ => None,
        })
        .collect();
    // A settled quantity the source did not state: the solver or the catalog row gave it.
    let derived = |name: &str| -> Option<Quantity> {
        let (_, field) = fields.iter().find(|(n, _)| *n == name)?;
        match &field.domain {
            Domain::Quantity(q) if field.narrowed_at.is_empty() => Some(*q),
            _ => None,
        }
    };

    let effective_capacitance = derived("effective_capacitance");
    let bias = effective_capacitance.and(derived("bias"));
    let tolerance = [
        ("resistance", Dimension::RESISTANCE),
        ("capacitance", Dimension::CAPACITANCE),
        ("inductance", Dimension::INDUCTANCE),
    ]
    .into_iter()
    .find_map(|(name, dim)| derived(name).filter(|q| q.dim() == dim))
    .and_then(|q| tolerance_of(&q));
    let ratings: Vec<Rating> = fields
        .iter()
        .filter(|(name, _)| {
            name.starts_with("max_") || name.starts_with("rated_") || *name == "saturation_current"
        })
        .filter_map(|(name, _)| {
            let q = derived(name)?;
            [Dimension::VOLTAGE, Dimension::CURRENT, Dimension::POWER]
                .contains(&q.dim())
                .then(|| Rating {
                    name: (*name).to_string(),
                    range: range_of(&q),
                })
        })
        .collect();
    let catalog = model
        .instance(inst)
        .catalog
        .as_ref()
        .map(|c| c.name.clone());
    let row =
        (catalog.is_some() || source.lcsc.is_some() || source.mpn.is_some()).then(|| SolvedRow {
            catalog,
            mpn: source.mpn.clone(),
            lcsc: source.lcsc.clone(),
            library: source.library.map(|l| l.to_string()),
            generic: source.generic.is_some(),
        });

    let solved = Solved {
        effective_capacitance: effective_capacitance.as_ref().map(range_of),
        bias: bias.as_ref().map(range_of),
        tolerance,
        ratings: (!ratings.is_empty()).then_some(ratings),
        row,
    };
    (solved.effective_capacitance.is_some()
        || solved.tolerance.is_some()
        || solved.ratings.is_some()
        || solved.row.is_some())
    .then_some(solved)
}

fn range_of(q: &Quantity) -> Range {
    Range {
        lo: q.lo(),
        hi: q.hi(),
        text: q.to_string(),
    }
}

/// Half the range over its middle, as a percentage; none for a point or a range that
/// reaches zero or below.
fn tolerance_of(q: &Quantity) -> Option<Tolerance> {
    if q.is_point() || q.lo() <= 0.0 {
        return None;
    }
    let percent = (q.hi() - q.lo()) / (q.hi() + q.lo()) * 100.0;
    let percent = (percent * 1000.0).round() / 1000.0;
    Some(Tolerance {
        percent,
        text: format!("+/- {percent}%"),
    })
}
