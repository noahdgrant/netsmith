//! Teardrops for a routed board (issue #431, S11): copper that widens each track where it meets
//! a pad or a via, so that a drill that wanders or an etch that undercuts does not break the
//! joint.
//!
//! The polygons are drawn by `netsmith_geom::teardrops` over the board as the check reads
//! it (the `.kicad_pcb` this crate writes, in memory) and to the check's own rules (the
//! `.kicad_pro` it writes, and the process's spaces a project file cannot say), so a teardrop is
//! kept only where the check passes with it. The sizes, the rules each one keeps and why netsmith
//! draws them rather than leaving them to KiCad are in that module's header. They go into the
//! board file as KiCad 9's teardrop zones ([`netsmith_pcb::kicad_pcb`]), which netsmith's Gerbers draw and
//! its check sees.
//!
//! The router adds them when it is done (`netsmith_router::route::route_with`, unless its
//! options switch them off), so every board netsmith routes has them.

use netsmith_geom::{DrcExtras, DrcRules, teardrops};

use netsmith_pcb::{Board, CopperLayer, Pt, Teardrop};

/// The process's spaces from a track that the check holds a teardrop to, which no project file
/// says: from holes, between surface-mount pads, from solder mask openings.
pub(crate) fn extras(board: &Board) -> DrcExtras {
    let r = &board.rules;
    DrcExtras {
        hole_to_track: Some(r.hole_clearance),
        plated_hole_clearance: Some(r.plated_hole_clearance),
        pad_to_pad: Some(r.pad_to_pad),
        mask_to_track: Some(r.mask_to_track),
        ..DrcExtras::default()
    }
}

/// The teardrops the board's copper has room for, in place of any it had.
pub fn add(board: &mut Board) {
    board.teardrops.clear();
    if board.tracks.is_empty() {
        return;
    }
    let Ok(art) = netsmith_pcb::copper_artwork(board) else {
        return;
    };
    let rules = DrcRules::from_project(&netsmith_pcb::kicad_pro(board)).unwrap_or_default();
    board.teardrops = teardrops(&art, &rules, &extras(board))
        .into_iter()
        .filter_map(|t| {
            let layer = match t.layer.as_str() {
                "F.Cu" => CopperLayer::Front,
                "B.Cu" => CopperLayer::Back,
                _ => return None,
            };
            Some(Teardrop {
                net: t.net,
                layer,
                points: t.polygon.into_iter().map(|p| Pt::new(p.x, p.y)).collect(),
            })
        })
        .collect();
}
