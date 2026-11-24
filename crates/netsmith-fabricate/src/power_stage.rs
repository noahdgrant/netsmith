//! The power stage of each switching regulator of a board as netsmith has it (issue #1585):
//! [`netsmith_fab::power_stage`] of the board file `fab` writes, with the regulators the design
//! names ([`netsmith_core::switching_regulators`]) and the keepout of the board's process.
//! `board nets --power-stage` prints it and `board drc` notes its foreign copper.

use netsmith_core::{Design, Process};
use netsmith_fab::power_stage::{DEFAULT_SWITCH_NODE_KEEPOUT, PowerStage};
use netsmith_geom::Artwork;
use netsmith_pcb::{Board, kicad_pcb};

/// How near foreign copper may come to a switch node on a board of `process`, in mm: its
/// `.switch_node_keepout`, or 1 mm when it states none.
pub fn keepout(process: &Process) -> f64 {
    process
        .switch_node_keepout
        .unwrap_or(DEFAULT_SWITCH_NODE_KEEPOUT)
}

/// The power stages of `design`'s switching regulators on `board`'s copper as it stands.
pub fn of_board(board: &Board, design: &Design) -> Result<Vec<PowerStage>, String> {
    let regulators = netsmith_core::switching_regulators(design);
    if regulators.is_empty() {
        return Ok(Vec::new());
    }
    let art = Artwork::parse(&kicad_pcb(board)).map_err(|e| e.to_string())?;
    Ok(netsmith_fab::power_stage::of(
        &art,
        &regulators,
        keepout(&board.process),
    ))
}
