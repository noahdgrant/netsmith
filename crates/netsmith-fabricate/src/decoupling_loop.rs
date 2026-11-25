//! The decoupling loop of each capacitor of a board as netsmith has it (issue #1730):
//! [`netsmith_fab::decoupling_loop`] of the board file `fab` writes, with the capacitors of the
//! board's decoupling pairs ([`netsmith_pcb::decoupling_pairs`], the placer's and the router's).
//! `board nets --decoupling` prints it.
//!
//! **How a capacitor and the pin it serves are recognised.** A capacitor is one of a decoupling
//! pair: a two-pad capacitor between two nets that one same other part has `power` pins on, or
//! between two supply nets with an IC, a connector or a part of no kind on its rail (a bulk
//! capacitor of the rail is one too). Of its two nets the **return** is the one the router takes
//! for it: the net with a pour or a plane, and if both or neither has one, the net with more pads;
//! the other is the **supply**. The pin it serves is the decoupled parts' pin on the supply net
//! nearest its supply pad, and the return pin that part's pin on the return net nearest its
//! return pad ([`netsmith_fab::decoupling_loop`]).

use netsmith_fab::decoupling_loop::{Decoupled, DecouplingLoop, Side};
use netsmith_geom::Artwork;
use netsmith_pcb::{Board, decoupling_pairs, kicad_pcb};

/// Whether a pour or a plane carries `net`.
fn poured(board: &Board, net: &str) -> bool {
    board.planes.iter().any(|p| p.net == net) || board.pours.iter().any(|p| p.net == net)
}

/// The capacitors of `board`'s decoupling pairs, each with its supply and its return.
pub fn decoupled(board: &Board) -> Vec<Decoupled> {
    let net_of = |refdes: &str, pad: &str| {
        board
            .nets
            .iter()
            .position(|n| n.pads.iter().any(|p| p.refdes == refdes && p.number == pad))
    };
    decoupling_pairs(&board.parts, &board.nets)
        .into_iter()
        .filter_map(|pair| {
            let cap = &board.parts[pair.cap];
            let sides = pair.nets.map(|side| {
                let net = net_of(&cap.refdes, &side.cap_pad)?;
                Some((
                    net,
                    Side {
                        net: board.nets[net].name.clone(),
                        cap_pad: side.cap_pad,
                        pins: side
                            .pins
                            .into_iter()
                            .map(|(i, pad)| (board.parts[i].refdes.clone(), pad))
                            .collect(),
                    },
                ))
            });
            let [Some((a, sa)), Some((b, sb))] = sides else {
                return None;
            };
            let (na, nb) = (&board.nets[a], &board.nets[b]);
            let second_returns = match (poured(board, &na.name), poured(board, &nb.name)) {
                (true, false) => false,
                (false, true) => true,
                _ => nb.pads.len() > na.pads.len(),
            };
            let (supply, ret) = if second_returns { (sa, sb) } else { (sb, sa) };
            Some(Decoupled {
                cap: cap.refdes.clone(),
                path: cap.path.clone(),
                supply,
                ret,
            })
        })
        .collect()
}

/// The decoupling loops of `board` on its copper as it stands.
pub fn of_board(board: &Board) -> Result<Vec<DecouplingLoop>, String> {
    let caps = decoupled(board);
    if caps.is_empty() {
        return Ok(Vec::new());
    }
    let art = Artwork::parse(&kicad_pcb(board)).map_err(|e| e.to_string())?;
    Ok(netsmith_fab::decoupling_loop::of(&art, &caps))
}
