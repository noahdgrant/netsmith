//! The design's requirements on where parts go (ADR 0045) as the design rules check wants
//! them: which footprints each is about, what it measures and the range it allows.
//!
//! **What was built and decided for the requirements** (ADR 0045 sections 8 and 9; the owner
//! decided on 2026-11-02). A violated requirement is an error that blocks `fab`, with its own
//! key `constraints = "error" | "warn"` under `[project]` in `netsmith.toml` and `fab
//! --constraints warn|error` for one run (both exist, the flag wins over the key, `build`
//! measures nothing). It is not a guideline: `guidelines` and `--strict` do not govern it,
//! because the user wrote it (ADR 0030 decision 3). There is no waiver: `--allow-drc-violations`
//! never lets one through, `package` refuses with `Error::Constraints`, which says that only
//! `constraints = "warn"` or `--constraints warn` relaxes it, and to relax a requirement a
//! person edits the range, which is in the diff. The notes of a package say how many of the
//! requirements are satisfied and, with `warn`, how many are not and that they are warnings. An
//! unchecked height is always a warning unless constraints are strict (`heights.rs`).
//!
//! `build` checks only what needs no placement (an empty intersection of bounds, a rotation
//! that is no multiple of 90, a side that is not `"top"` or `"bottom"`, a bound that uses
//! `outline.width` or `outline.height` when the outline does not state that size as one value,
//! a window outside a fixed outline); satisfaction is measured where a placement exists. The
//! measurement is one rule on the board file (`netsmith_fab::drc::constraints`), so `fab`, the
//! editor's findings and the cross-check share it. The tolerance is half a micrometre, as for
//! the rest of the check.

use netsmith_check::{SourceId, Sources};
use netsmith_core::{Design, PlaceField};
use netsmith_design::Sites;
use netsmith_fab::drc::Severity;
use netsmith_fab::drc::constraints::{
    Constraint, Constraints, Measure, Relation, RelationEnd, SideRule,
};
use netsmith_lang::Span;

/// Say where the design states each requirement: `widget.nsm:14`, or several places
/// separated by commas, on the bound or the side it is about. `sites` are what the design stage
/// found, and `sources` the files the spans point into.
pub fn name_sources(design: &mut Design, sites: &Sites, sources: &Sources) {
    for (index, field, spans) in sites {
        let text = lines_of(spans, sources);
        remember_lines(design, spans, sources);
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

/// Keep the text of each line that states a requirement, by its `file:line`, so that a
/// message about a requirement that is not met can quote the line.
fn remember_lines(design: &mut Design, spans: &[(SourceId, Span)], sources: &Sources) {
    for (source, span) in spans {
        let s = sources.get(*source);
        let at = span.start.min(s.text.len());
        let line = s.text[..at].matches('\n').count() + 1;
        let text = s.text.lines().nth(line - 1).unwrap_or("").trim();
        if !text.is_empty() {
            design
                .features
                .line_texts
                .insert(format!("{}:{line}", s.name), text.to_string());
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
    for site in sites {
        remember_lines(design, std::slice::from_ref(site), sources);
    }
}

/// Say where the design states each protrusion (`design.features.protrusions`, in the order of `sites`).
pub fn name_protrusion_sources(
    design: &mut Design,
    sites: &[Vec<(SourceId, Span)>],
    sources: &Sources,
) {
    for (protrusion, spans) in design.features.protrusions.iter_mut().zip(sites) {
        protrusion.source = lines_of(spans, sources);
    }
    for spans in sites {
        remember_lines(design, spans, sources);
    }
}

/// Say where the design states the file of each STEP model (`design.features.models`, in the
/// order of `sites`): the lines, and the file of each statement.
pub fn name_model_sources(design: &mut Design, sites: &[Vec<(SourceId, Span)>], sources: &Sources) {
    for (model, spans) in design.features.models.iter_mut().zip(sites) {
        model.source = lines_of(spans, sources);
        model.stated_in = spans
            .iter()
            .map(|(source, _)| sources.get(*source).name.clone())
            .collect();
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
