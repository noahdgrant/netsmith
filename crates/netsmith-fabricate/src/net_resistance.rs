//! The DC resistance of a net's copper between two pads or groups of pads on a board (issue
//! #1703): `netsmith_fab::resistance` on the board file of the board's copper, with the board's
//! copper, its sizing for a current (`netsmith_design::guidelines::sizing_of`, the design rules
//! check's) and the net's one current (issue #1584) where no current is given. `board
//! resistance` prints it.

use netsmith_fab::resistance::{self, Ask, Copper, Resistance};
use netsmith_geom::Artwork;
use netsmith_pcb::{Board, kicad_pcb};

/// The resistance of `net` between `from` and `to` on `board` as it stands, at `amps`, or at the
/// net's current when `amps` is `None` (no current when the net has none known), its heat at
/// the net's rms current where it has one.
pub fn of_board(
    board: &Board,
    net: &str,
    from: &[String],
    to: &[String],
    amps: Option<f64>,
) -> Result<Resistance, String> {
    if !board.nets.iter().any(|n| n.name == net) {
        return Err(format!("the board has no net {net}"));
    }
    let art = Artwork::parse(&kicad_pcb(board)).map_err(|e| e.to_string())?;
    let process = &board.process;
    let outer = process.stackup.copper_weight * netsmith_core::OUNCE_OF_COPPER;
    let copper = Copper {
        outer,
        inner: process
            .stackup
            .inner
            .as_ref()
            .map_or(outer, |i| i.copper)
            .max(0.0),
        plating: process.hole_plating,
    };
    // The net's one current (issue #1584), its peak for the drop and its rms for the heat
    // (issue #1690); a current given is both.
    let known = board
        .net_currents
        .iter()
        .find(|c| c.net == net && c.current > 0.0);
    let current = amps.or_else(|| known.map(|c| c.current));
    let heating = match amps {
        Some(_) => None,
        None => known
            .map(|c| c.heating())
            .filter(|&h| h < known.map_or(0.0, |c| c.current)),
    };
    resistance::of(
        &art,
        &Ask {
            net,
            from,
            to,
            current,
            heating,
            copper,
            sizing: netsmith_design::guidelines::sizing_of(process),
        },
    )
}
