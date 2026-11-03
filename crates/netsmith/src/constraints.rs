//! The design's requirements on where parts go (ADR 0045) as the design rules check wants
//! them: which footprints each is about, what it measures and the range it allows.

use netsmith_check::Sources;
use netsmith_core::{Design, PlaceField};
use netsmith_fab::drc::Severity;
use netsmith_fab::drc::constraints::{Constraint, Constraints, Measure, SideRule};
use netsmith_lower::Sites;

/// Say where the design states each requirement: `widget.nsm:14`, or several places
/// separated by commas, on the bound or the side it is about. `sites` are what lowering
/// found, and `sources` the files the spans point into.
pub fn name_sources(design: &mut Design, sites: &Sites, sources: &Sources) {
    for (index, field, spans) in sites {
        let mut lines: Vec<String> = Vec::new();
        for (source, span) in spans {
            let s = sources.get(*source);
            let line = s.text[..span.start.min(s.text.len())].matches('\n').count() + 1;
            let place = format!("{}:{line}", s.name);
            if !lines.contains(&place) {
                lines.push(place);
            }
        }
        let text = lines.join(", ");
        let Some(placement) = design.features.constraints.get_mut(*index) else {
            continue;
        };
        if field == "side" {
            placement.side_source = text;
        } else if let Some(f) = PlaceField::from_word(field)
            && let Some(b) = placement.bounds.iter_mut().find(|b| b.field == f)
        {
            b.source = text;
        }
    }
}

fn measure(field: PlaceField) -> Measure {
    match field {
        PlaceField::X => Measure::X,
        PlaceField::Y => Measure::Y,
        PlaceField::Left => Measure::Left,
        PlaceField::Right => Measure::Right,
        PlaceField::Top => Measure::Top,
        PlaceField::Bottom => Measure::Bottom,
        PlaceField::Rotation => Measure::Rotation,
    }
}

/// The requirements of `design` for the check, if it has any. A violation is a warning when
/// `warn` is set (`constraints = "warn"`), else an error.
pub fn of_design(design: &Design, warn: bool) -> Option<Constraints> {
    if design.features.constraints.is_empty() {
        return None;
    }
    let mut list = Vec::new();
    let mut sides = Vec::new();
    for p in &design.features.constraints {
        // A part is itself; a module is every part inside it.
        let prefix = format!("{}.", p.path);
        let parts: Vec<String> = design
            .parts
            .iter()
            .filter(|part| {
                if p.module {
                    part.path.starts_with(&prefix)
                } else {
                    part.path == p.path
                }
            })
            .map(|part| part.refdes.clone())
            .collect();
        for b in &p.bounds {
            list.push(Constraint {
                id: format!("{}.place.{}", p.path, b.field.word()),
                source: b.source.clone(),
                parts: parts.clone(),
                measure: measure(b.field),
                lo: b.lo,
                hi: b.hi,
            });
        }
        if let (Some(side), Some(part)) = (&p.side, parts.first()) {
            sides.push(SideRule {
                id: format!("{}.place.side", p.path),
                source: p.side_source.clone(),
                part: part.clone(),
                side: side.clone(),
            });
        }
    }
    Some(Constraints {
        list,
        sides,
        severity: if warn {
            Severity::Warning
        } else {
            Severity::Error
        },
    })
}
