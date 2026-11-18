//! The per-net table of a board as netsmith has it (issue #1223): `netsmith_fab::NetTable` of
//! the board file `fab` writes, with the board's diff pairs and each pair's skew limit, and its
//! groups of nets matched in length (which the file does not hold, issue #1234). `netsmith board nets` prints it and the
//! editor's PCB summary shows it.

use netsmith_fab::NetTable;
use netsmith_geom::Artwork;
use netsmith_pcb::{Board, kicad_pcb};

/// The table of `board`'s copper as it stands.
pub fn of_board(board: &Board) -> Result<NetTable, String> {
    let art = Artwork::parse(&kicad_pcb(board)).map_err(|e| e.to_string())?;
    Ok(NetTable::of(
        &art,
        &board.diff_pairs_with_skew(),
        &board.length_groups,
    ))
}
