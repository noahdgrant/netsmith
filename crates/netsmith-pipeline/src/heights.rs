//! Where a part's height comes from (ADR 0045 section 11, ADR 0094, ADR 0106).
//!
//! Four sources, the strongest first: the design states `.height` (the source or the physical
//! file), else the `height` cell of the catalog row the part was picked as (a datasheet figure
//! with its source), else the _Standard height_ of its footprint (the package standard's
//! maximum, `netsmith_pcb::StandardHeight`), else the part has no height and a limit reports it
//! unchecked. The design stage lists only the first; this module adds the second, once the
//! generic parts have their rows, and keeps what it added so that a report can say where each
//! number came from. The third is never written into the design: the board reads it from the
//! footprint where a part has no height of its own (`Placed::standard_height`), so a check can
//! say that a pass rests on a standard and not on the part.
//!
//! **A standard height under a limit** (ADR 0106). Where the standard's maximum fits the limit,
//! the part passes, and `fab`, `fab-notes.txt` and `board3d` say that the pass is the
//! standard's (`Board::standard_passes`). Where it does not, the part is unchecked, as a part of
//! unknown height is, with the standard's figure in the message: a part in that package may be
//! lower. It is never `TooTall`, and the placer does not read it.
//!
//! **The rule of a height limit** (ADR 0045 section 11; the guide's "Height limits" has the
//! examples).
//!
//! - *A height.* `.height` is a built-in `Length` member of every part, stated by a part kind,
//!   a user part or the board (`.sensor.height &= 8.5mm`): one value, never a range, never
//!   negative; zero is a statement ("flat"), not a default. A part with no `.height` has no
//!   height, not zero and not a guess: where a limit applies to it, it is reported by name as
//!   unchecked. The footprint's 3D model is not read, and a KiCad footprint has no height of
//!   its own. The spelling `.height` is the owner's (2026-11-03); it made the label's own
//!   `.height` become `.text_height` (a breaking rename of `std::parts::mechanical::Label`).
//! - *A limit.* `Keepout` has `.max_height`, one value above zero. A keepout that states it and
//!   none of `.tracks`, `.vias`, `.copper` and `.parts` forbids nothing else (owner,
//!   2026-11-04); one that states neither is still the error "forbids nothing". A height-only
//!   keepout is not exported to KiCad as a rule area (owner, 2026-11-04). `netsmith dxf import`
//!   reads no limit from a drawing, since a DXF has no height on a 2D loop.
//! - *Both faces.* `.layers` names the faces a limit applies to: `top`, `bottom`, or `all` (the
//!   default). A limit on inner layers only limits no face and is an error. A part is counted
//!   on the face it is placed on, by its `.height`; the part of a through-hole part below the
//!   board is not limited.
//! - *The report.* `Board::check_placement` gives a `TooTall` problem for each part of known
//!   height whose body (`F.Fab`, else the courtyard) overlaps a limit on its face and is over
//!   it by more than half a micrometre; it stops `fab`. `Board::heights_unchecked` lists the
//!   parts with no height under a limit: a warning in `fab`, `fab-notes.txt` and the editor
//!   (`height_unchecked`), an error when constraints are strict (`fab --constraints error` or
//!   `constraints = "error"` written in the manifest; `--constraints warn` wins; the owner's
//!   reading, 2026-11-03). A label has no body and is left out.
//! - *The placer.* The annealer keeps a part of known height out of a zone it is over, on its
//!   face, with the hard cost of a keepout for parts, measured on the body grown by
//!   `HEIGHT_ZONE_MARGIN` (0.25 mm, the placer's keepout gap; owner, 2026-11-04: the body plus a
//!   margin and not the courtyard, because a connector whose courtyard goes 4.4 mm past its body
//!   could not stand beside a limit its body clears). A footprint with no fabrication outline
//!   keeps its courtyard, with no margin. A part of unknown height is not kept out. The fit's
//!   face-sharing step knows heights (`place::face_forbidden`): a part that states a height and
//!   no face stays off a face where the limits leave no room for its body.
//! - *A keepout a part carries* is in the part's frame and moves with it. The check and the
//!   placer measure it against every other part (`PlaceOptions::carried`); a carried zone that
//!   forbids parts keeps out parts on both faces, and the face-sharing step does not read it.
//! - *The editor.* A limit is drawn amber with `max 3 mm`; the hover says how tall a part stands
//!   and what each limit allows; a finding about a keepout lights it; a drag of a part of stated
//!   height stops before a step that would put its body over a limit it is too tall for
//!   (`pcb/limits.ts`). The arrow-key nudge, turn and flip are not held.
//!
//! **Choices of section 11, accepted as built by the owner, 2026-11-11** (each was a question in
//! the ADR's text; the answer is what the rules above do):
//! - The catalog `height` column is the second source, after a stated `.height` (ADR 0094).
//! - A height-only keepout forbids nothing else, so `.parts &= "allow"` need not be written out
//!   (confirmed 2026-11-04).
//! - `.layers` names the faces a limit applies to (confirmed 2026-11-04).
//! - An unknown height under a limit is an error only when `error` is asked for explicitly, by
//!   the flag or the manifest, and a warning otherwise, although the key defaults to `error`.
//! - The margin is `HEIGHT_ZONE_MARGIN`, the placer's constant, 0.25 mm, and not a value of the
//!   process.
//! - The editor's drag stops at a limit, and the arrow-key nudge, turn and flip are not held; a
//!   part with no stated height is not held (as in the placer and the check); a keepout for
//!   parts is named by a drag and refuses nothing.
//! - The fit's face-sharing step does not read a limit or a keepout that a part carries, and a
//!   carried zone that forbids parts keeps out parts on both faces, with no reading of
//!   `.layers`.
//!
//! Not built: reading a limit from the DXF (a layer that carries a height, or a 3D entity),
//! which needs a rule that is not a guess (issue #27).

use std::collections::BTreeMap;

use netsmith_3d::HeightProvenance;
use netsmith_core::{Design, PartHeight};
use netsmith_design::Sourced;
use netsmith_pcb::{Board, StandardHeight};
use netsmith_solve::Catalogs;

use crate::resolve::resolve_generics;

/// A part whose height is the catalog row's, because the design states none.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogHeight {
    pub refdes: String,
    /// The part's path in the design, as in `design.features.heights`.
    pub path: String,
    /// In mm.
    pub height: f64,
    /// The row's LCSC number, if it has one.
    pub lcsc: Option<String>,
    /// The row's manufacturer's number, if it has one.
    pub mpn: Option<String>,
}

impl CatalogHeight {
    /// The row as a message names it: `LQH32PN4R7NN0L (C98021)`.
    pub fn row(&self) -> String {
        match (&self.mpn, &self.lcsc) {
            (Some(mpn), Some(lcsc)) => format!("{mpn} ({lcsc})"),
            (Some(one), None) | (None, Some(one)) => one.clone(),
            (None, None) => "a catalog row".to_string(),
        }
    }
}

/// Give every part that the design states no height for the height of its catalog row, if
/// the row has one, and return which parts got one. A part whose height the design states is
/// left as the design says, whatever its row says.
pub(crate) fn from_catalog_rows(
    design: &mut Design,
    sourcing: &[Sourced],
    catalogs: &Catalogs,
) -> Vec<CatalogHeight> {
    let (resolved, _) = resolve_generics(sourcing, catalogs);
    let mut found = Vec::new();
    for (part, s) in design.parts.iter().zip(&resolved) {
        let Some(height) = s.catalog_height else {
            continue;
        };
        if design.features.heights.iter().any(|h| h.path == part.path) {
            continue;
        }
        found.push(CatalogHeight {
            refdes: part.refdes.clone(),
            path: part.path.clone(),
            height,
            lcsc: s.lcsc.clone(),
            mpn: s.mpn.clone(),
        });
    }
    for h in &found {
        design.features.heights.push(PartHeight {
            path: h.path.clone(),
            height: h.height,
        });
    }
    design.features.heights.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// Where each part's height came from in the design (the glossary's _Height provenance_), by
/// path: `catalog` for a part of `catalog`, else `stated` for a part the design states a height
/// for, else `unknown`. The design does not know footprints, so a part this calls `unknown` may
/// have a standard height on the board: [`Coverage`] and the 3D scene (`netsmith_3d::Scene`)
/// read that from the board, after this.
pub fn provenance(
    design: &Design,
    catalog: &[CatalogHeight],
) -> BTreeMap<String, HeightProvenance> {
    design
        .parts
        .iter()
        .map(|p| {
            let from = if catalog.iter().any(|c| c.path == p.path) {
                HeightProvenance::Catalog
            } else if design.features.heights.iter().any(|h| h.path == p.path) {
                HeightProvenance::Stated
            } else {
                HeightProvenance::Unknown
            };
            (p.path.clone(), from)
        })
        .collect()
}

/// How many parts of a board have a known height, for the notes of a board that has a height
/// limit: the parts counted are those that have a body (a label has none). A standard height
/// counts as known and is counted apart (ADR 0106).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub parts: usize,
    pub stated: usize,
    pub catalog: usize,
    pub standard: usize,
}

impl Coverage {
    /// The coverage of `design` placed as `board`, which says which parts have a standard height.
    pub fn of(design: &Design, catalog: &[CatalogHeight], board: &Board) -> Option<Self> {
        if !design
            .features
            .keepouts
            .iter()
            .any(|k| k.max_height.is_some())
        {
            return None;
        }
        let from = provenance(design, catalog);
        let count = |want| {
            design
                .parts
                .iter()
                .filter(|p| p.kind != "Label" && from[&p.path] == want)
                .count()
        };
        let standard = design
            .parts
            .iter()
            .filter(|p| p.kind != "Label" && from[&p.path] == HeightProvenance::Unknown)
            .filter(|p| standard_of(board, &p.path).is_some())
            .count();
        Some(Self {
            parts: design.parts.iter().filter(|p| p.kind != "Label").count(),
            stated: count(HeightProvenance::Stated),
            catalog: count(HeightProvenance::Catalog),
            standard,
        })
    }

    /// The line for `fab-notes.txt`.
    pub fn line(&self) -> String {
        let known = self.stated + self.catalog + self.standard;
        format!(
            "note: {known} of {} parts have a known height ({} stated in the design, {} from catalog rows, {} the package standard's maximum); a part under a height limit with none is reported unchecked, never as a pass",
            self.parts, self.stated, self.catalog, self.standard
        )
    }
}

/// The standard height of the part at `path` on `board`, if it has no height of its own.
fn standard_of(board: &Board, path: &str) -> Option<&'static StandardHeight> {
    board
        .parts
        .iter()
        .find(|p| p.path == path)
        .and_then(|p| p.standard_height())
}

/// What `fab-notes.txt` says about heights, for a board that has a height limit: how many
/// parts have a known height, each height that is a catalog row's, with the row, so that a
/// person can look the figure up, each that is a package standard's, with the standard, and
/// each pass under a limit that rests on a standard height. Nothing for a board with no limit
/// (ADR 0045 section 11).
pub fn notes(design: &Design, catalog: &[CatalogHeight], board: &Board) -> Vec<String> {
    let Some(coverage) = Coverage::of(design, catalog, board) else {
        return Vec::new();
    };
    let mut lines = vec![coverage.line()];
    lines.extend(catalog.iter().map(|c| {
        format!(
            "note: height of {} (`{}`) is {:.2} mm, from the `height` of catalog row {} (the row's `source` cell says which document and page); the design states none",
            c.refdes, c.path, c.height, c.row()
        )
    }));
    let mut standard: Vec<&netsmith_pcb::Placed> = board
        .parts
        .iter()
        .filter(|p| p.standard_height().is_some())
        .collect();
    standard.sort_by(|a, b| a.path.cmp(&b.path));
    lines.extend(standard.iter().filter_map(|p| {
        let s = p.standard_height()?;
        Some(format!(
            "note: height of {} (`{}`) is {:.2} mm, the greatest its package standard allows ({}), not a figure of the part: the design states none and its catalog row has none",
            p.refdes,
            p.path,
            s.max,
            s.cited()
        ))
    }));
    lines.extend(board.standard_passes().iter().map(|p| format!("note: {p}")));
    lines
}
