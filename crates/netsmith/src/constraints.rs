//! The design's requirements on where parts go (ADR 0045) as the design rules check wants
//! them: which footprints each is about, what it measures and the range it allows.

use netsmith_check::{SourceId, Sources};
use netsmith_core::{Design, PlaceField};
use netsmith_fab::drc::Severity;
use netsmith_fab::drc::constraints::{
    Constraint, Constraints, Measure, Relation, RelationEnd, SideRule,
};
use netsmith_lang::Span;
use netsmith_lower::Sites;

/// Say where the design states each requirement: `widget.nsm:14`, or several places
/// separated by commas, on the bound or the side it is about. `sites` are what lowering
/// found, and `sources` the files the spans point into.
pub fn name_sources(design: &mut Design, sites: &Sites, sources: &Sources) {
    for (index, field, spans) in sites {
        let text = lines_of(spans, sources);
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

/// The lines that state something, `widget.nsm:14`, separated by commas.
fn lines_of(spans: &[(SourceId, Span)], sources: &Sources) -> String {
    let mut lines: Vec<String> = Vec::new();
    for (source, span) in spans {
        let s = sources.get(*source);
        let line = s.text[..span.start.min(s.text.len())].matches('\n').count() + 1;
        let place = format!("{}:{line}", s.name);
        if !lines.contains(&place) {
            lines.push(place);
        }
    }
    lines.join(", ")
}

/// Say where the design states each relation between two parts (`design.features.offsets`,
/// in the order of `sites`).
pub fn name_offset_sources(design: &mut Design, sites: &[(SourceId, Span)], sources: &Sources) {
    for (offset, site) in design.features.offsets.iter_mut().zip(sites) {
        offset.source = lines_of(std::slice::from_ref(site), sources);
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
    if design.features.constraints.is_empty() && design.features.offsets.is_empty() {
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
    // A relation between two parts names both by designator and by path.
    let end = |path: &str, field: PlaceField| {
        design
            .parts
            .iter()
            .find(|part| part.path == path)
            .map(|part| RelationEnd {
                part: part.refdes.clone(),
                path: path.to_string(),
                measure: measure(field),
            })
    };
    let relations = design
        .features
        .offsets
        .iter()
        .filter_map(|o| {
            Some(Relation {
                id: format!(
                    "{}.place.{} - {}.place.{}",
                    o.b.path,
                    o.b.field.word(),
                    o.a.path,
                    o.a.field.word()
                ),
                source: o.source.clone(),
                a: end(&o.a.path, o.a.field)?,
                b: end(&o.b.path, o.b.field)?,
                lo: o.lo,
                hi: o.hi,
            })
        })
        .collect();
    Some(Constraints {
        list,
        sides,
        relations,
        severity: if warn {
            Severity::Warning
        } else {
            Severity::Error
        },
    })
}
