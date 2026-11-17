//! `netsmith board drc` (issue #1095): netsmith's own design rules check (ADR 0037) of a board
//! as it stands, with nothing routed and no package written.
//!
//! It is the check `fab` runs after routing, on the files `fab` writes, called alone: the same
//! `.kicad_pcb` and `.kicad_pro` text of the board ([`netsmith_pcb::kicad_pcb`]), the same extras
//! ([`crate::fab::drc_extras`]) and the same requirements of the design on where parts go
//! ([`netsmith_pipeline::constraints::of_design`]), through [`netsmith_fab::check`], which
//! `netsmith_fab::package` calls too. The board is the placed one with the routes file's copper
//! on it ([`crate::routes::apply_beside`]), as `fab` has it before the router: a connection the
//! file does not make is a finding, as it is for `fab` when the router leaves it.

use std::path::Path;

use netsmith_fab::{Checked, Options};
use netsmith_geom::Constraints;
use netsmith_pcb::{Board, kicad_pcb, kicad_pro};

/// The check of `board` as it is, with the design's `constraints` (`None` when it states none).
pub fn of_board(board: &Board, constraints: Option<Constraints>) -> Result<Checked, String> {
    let options = Options {
        drc: crate::fab::drc_extras(board),
        constraints,
        ..Options::default()
    };
    netsmith_fab::check(&kicad_pcb(board), Some(&kicad_pro(board)), &options)
        .map_err(|e| e.to_string())
}

/// The check of the KiCad board `pcb`, with the `.kicad_pro` beside it when there is one. A board
/// KiCad wrote says nothing of the process it was made for, so the rules are those of the project
/// file alone: none of `fab`'s extras (the process's mask web and via rules, the fiducials, the
/// stackup) and no requirement of a design.
pub fn of_kicad_pcb(pcb: &Path) -> Result<Checked, String> {
    let src =
        std::fs::read_to_string(pcb).map_err(|e| format!("cannot read {}: {e}", pcb.display()))?;
    netsmith_fab::check(
        &src,
        netsmith_geom::drc_project_of(pcb).as_deref(),
        &Options::default(),
    )
    .map_err(|e| format!("{}: {e}", pcb.display()))
}
