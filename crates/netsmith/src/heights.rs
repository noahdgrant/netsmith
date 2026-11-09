//! Where a part's height comes from (ADR 0045 section 11, ADR 0094).
//!
//! Three sources, the strongest first: the design states `.height` (the source or the physical
//! file), else the `height` cell of the catalog row the part was picked as (a datasheet figure
//! with its source), else the part has no height and a limit reports it unchecked. The design stage
//! lists only the first; this module adds the second, once the generic parts have their rows,
//! and keeps what it added so that a report can say where each number came from.

use netsmith_core::{Design, PartHeight};
use netsmith_design::Sourced;
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

/// How many parts of a board have a known height, for the notes of a board that has a height
/// limit: the parts counted are those that have a body (a label has none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub parts: usize,
    pub stated: usize,
    pub catalog: usize,
}

impl Coverage {
    pub fn of(design: &Design, catalog: &[CatalogHeight]) -> Option<Self> {
        if !design
            .features
            .keepouts
            .iter()
            .any(|k| k.max_height.is_some())
        {
            return None;
        }
        let bodies: Vec<&str> = design
            .parts
            .iter()
            .filter(|p| p.kind != "Label")
            .map(|p| p.path.as_str())
            .collect();
        let known = bodies
            .iter()
            .filter(|path| design.features.heights.iter().any(|h| h.path == **path))
            .count();
        let from_catalog = catalog
            .iter()
            .filter(|c| bodies.contains(&c.path.as_str()))
            .count();
        Some(Self {
            parts: bodies.len(),
            stated: known - from_catalog.min(known),
            catalog: from_catalog,
        })
    }

    /// The line for `fab-notes.txt`.
    pub fn line(&self) -> String {
        let known = self.stated + self.catalog;
        format!(
            "note: {known} of {} parts have a known height ({} stated in the design, {} from catalog rows); a part under a height limit with none is reported unchecked, never as a pass",
            self.parts, self.stated, self.catalog
        )
    }
}

/// What `fab-notes.txt` says about heights, for a board that has a height limit: how many
/// parts have a known height, and each height that is a catalog row's, with the row, so that a
/// person can look the figure up. Nothing for a board with no limit (ADR 0045 section 11).
pub fn notes(design: &Design, catalog: &[CatalogHeight]) -> Vec<String> {
    let Some(coverage) = Coverage::of(design, catalog) else {
        return Vec::new();
    };
    let mut lines = vec![coverage.line()];
    lines.extend(catalog.iter().map(|c| {
        format!(
            "note: height of {} (`{}`) is {:.2} mm, from the `height` of catalog row {} (the row's `source` cell says which document and page); the design states none",
            c.refdes, c.path, c.height, c.row()
        )
    }));
    lines
}
