//! The `pads` block of a part (issue #1321): what the part states of its footprint's pads, one
//! pad and one field to a row.
//!
//! ```text
//! part SmdHeader1x4 provides Connector {
//!     ...
//!     pads "SMD" {
//!         "1" .contact_offset = (1.175mm, 0mm)
//!         "2" .contact_offset = (-1.175mm, 0mm)
//!     }
//! }
//! ```
//!
//! The one field is `.contact_offset`: where the pad's contact is, the point a mating contact
//! meets, from the centre of the pad's copper, in the footprint's frame (x right, y down, as the
//! footprint's pads are). A pad no row names has its contact at its centre. A surface-mount pin
//! header whose feet alternate either side of its pins states the offset that takes each foot's
//! centre back to the pin's axis, and a system's mates are fitted and checked on the contacts
//! (ADR 0110), not on the copper. Only a part's own definition holds a block, as only it holds a
//! `pins` table; `pads "SMD" { }` is for one of the part's packages, and the design keeps its
//! rows only for an instance in that package.

use netsmith_lang::ast::Pads;

use crate::checker::Checker;
use crate::model::PadContact;

/// The fields a row of a `pads` block may state.
const FIELDS: [&str; 1] = ["contact_offset"];

impl Checker<'_> {
    /// A `pads` block: each row becomes a [`PadContact`] of the part instance.
    pub(crate) fn pads(&mut self, block: &Pads) {
        if !self.support_allowed() {
            self.error(
                block.span,
                "`pads` can only appear directly inside a part's own definition, for the pads of its footprint",
            );
            return;
        }
        let part = self.current();
        let tables = self.model.instance(part).tables.clone();
        let named: Vec<String> = tables.iter().filter_map(|t| t.package.clone()).collect();
        let mut packages: Vec<Option<String>> = Vec::new();
        for (package, span) in &block.packages {
            if package.trim().is_empty() {
                self.error(
                    *span,
                    "the package of `pads \"...\"` is empty: name the package whose pads the block is about, or leave it out for every package",
                );
                return;
            }
            if !named.is_empty() && !named.iter().any(|p| p == package) {
                let message = format!(
                    "the part has no pin table for the package \"{package}\"; its packages are {}",
                    named
                        .iter()
                        .map(|p| format!("\"{p}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                self.error(*span, message);
                return;
            }
            packages.push(Some(package.clone()));
        }
        if packages.is_empty() {
            packages.push(None);
        }
        let mut seen: Vec<&str> = Vec::new();
        for row in &block.rows {
            if !FIELDS.contains(&row.field.name.as_str()) {
                self.error(
                    row.field.span,
                    format!(
                        "a pad has no field `{}`; a row of `pads` states `.contact_offset = (X, Y)`, where the pad's contact is from its centre",
                        row.field.name
                    ),
                );
                continue;
            }
            if seen.contains(&row.pad.as_str()) {
                self.error(
                    row.pad_span,
                    format!(
                        "pad \"{}\" already has a `.contact_offset` in this block",
                        row.pad
                    ),
                );
                continue;
            }
            seen.push(&row.pad);
            // The pads of the tables the block is for: a pad of none of them is a mistake.
            let pads_of = |package: &Option<String>| -> Vec<&str> {
                tables
                    .iter()
                    .filter(|t| package.is_none() || t.package.is_none() || t.package == *package)
                    .flat_map(|t| {
                        t.rows
                            .iter()
                            .flat_map(|r| r.pads.iter().map(String::as_str))
                    })
                    .collect()
            };
            if !tables.is_empty()
                && packages
                    .iter()
                    .any(|p| !pads_of(p).contains(&row.pad.as_str()))
            {
                self.error(
                    row.pad_span,
                    format!(
                        "the part's pin table has no pad \"{}\": a row of `pads` names a pad of the table, by its number",
                        row.pad
                    ),
                );
                continue;
            }
            let what = format!("`.contact_offset` of pad \"{}\"", row.pad);
            let x = self.any_length(&row.x, "the point", &what);
            let y = self.any_length(&row.y, "the point", &what);
            let (Some(x), Some(y)) = (x, y) else {
                continue;
            };
            for package in &packages {
                let contact = PadContact {
                    part,
                    package: package.clone(),
                    pad: row.pad.clone(),
                    offset: (x, y),
                };
                if !self.model.pad_contacts.contains(&contact) {
                    self.model.pad_contacts.push(contact);
                }
            }
        }
    }
}
