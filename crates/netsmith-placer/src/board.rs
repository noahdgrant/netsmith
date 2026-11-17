//! A board from a design: the board model loads the footprints and turns the requirements of
//! the physical file into what the placer is told ([`Board::unplaced`]), the placer places the
//! parts, and the board is made of them ([`Unplaced::into_board`](netsmith_pcb::Unplaced::into_board)). These were
//! `Board::from_design`, `Board::from_design_placed` and `Board::precheck` of `netsmith-pcb`
//! before the placer had a crate of its own (issue #518).

use netsmith_core::Design;
use netsmith_kicad::Mapping;
use netsmith_pcb::{
    Board, BoardError, DoesNotFit, FootprintLibrary, LoadedBoard, ORIGIN, PlaceOptions, Placed,
    Precheck, Pt, Rect, Rules,
};

use crate::{fit, place};

/// Choose a footprint for each part, place them, and keep the nets. The same design
/// always gives the same board. The design must say how it is made (its `process`), and
/// the board must have chosen a thickness for it. The mapping says which footprint each
/// part has and the library holds it.
pub fn board_from_design(
    design: &Design,
    mapping: &Mapping,
    library: &FootprintLibrary,
) -> Result<Board, BoardError> {
    board_from_design_placed(design, mapping, library, &PlaceOptions::default())
}

/// What the requirements of `design` prove about where its parts can go, before any
/// placing: the same proofs [`board_from_design_placed`] makes first (and fails with
/// [`BoardError::Unplaceable`]), with the warnings for what is only suspicious. It takes
/// milliseconds. `Err` is a design the placer could not start on either.
pub fn precheck(
    design: &Design,
    mapping: &Mapping,
    library: &FootprintLibrary,
    placement: &PlaceOptions,
) -> Result<Precheck, BoardError> {
    let LoadedBoard {
        rules,
        placement,
        parts,
        ..
    } = Board::load(design, mapping, library, placement)?;
    let plan = netsmith_pcb::constraint_plan(design, &parts, &placement, rules.clearance)
        .map_err(BoardError::Constraints)?;
    let found = netsmith_pcb::before_placing(
        design,
        &parts,
        &placement,
        &plan,
        &rules,
        netsmith_pcb::outline_box(design),
    );
    Ok(if placement.lines_warn {
        found.lines_as_warnings()
    } else {
        found
    })
}

/// [`board_from_design`], with what is known about where the parts go: the face each is on,
/// and the size the board must be. The board model makes the board before placing
/// ([`Board::unplaced`], which refuses what is wrong with the design), the placer places the
/// parts, and the board is made of them ([`Unplaced::into_board`](netsmith_pcb::Unplaced::into_board)).
pub fn board_from_design_placed(
    design: &Design,
    mapping: &Mapping,
    library: &FootprintLibrary,
    placement: &PlaceOptions,
) -> Result<Board, BoardError> {
    let mut u = Board::unplaced(design, mapping, library, placement)?;
    // A board that states no outline is as big as its parts need, as ever.
    let plane_nets: Vec<String> = u.planes.iter().map(|p| p.net.clone()).collect();
    // The outer layers and each inner layer that no plane takes.
    let inner = u.process.stackup.layers.saturating_sub(2) as usize;
    let planes_inner = (1..=inner)
        .filter(|n| u.planes.iter().any(|p| p.layer == format!("In{n}.Cu")))
        .count();
    let signal_layers = 2 + inner - planes_inner;
    let (outline, corner_radius) = match &design.features.outline {
        None => (
            place::arrange_best(
                &mut u.parts,
                &u.nets,
                &u.placement,
                &place::Context {
                    rules: &u.rules,
                    planes: &plane_nets,
                    corner_radius: 0.0,
                    pours: !design.features.pours.is_empty(),
                    signal_layers,
                },
            )
            .map_err(|e| {
                BoardError::DoesNotFit(explained(e, design, &u.parts, &u.base, &u.rules))
            })?,
            0.0,
        ),
        Some(wanted) => (
            fit::fit(
                &mut u.parts,
                &u.nets,
                wanted,
                u.polygon.as_ref(),
                &u.rules,
                &u.placement,
                &plane_nets,
                !design.features.pours.is_empty(),
                signal_layers,
            )
            .map_err(|e| {
                BoardError::DoesNotFit(explained(e, design, &u.parts, &u.base, &u.rules))
            })?,
            wanted.corner_radius,
        ),
    };
    Ok(u.into_board(design, outline, corner_radius))
}

/// The board as the placer ended with it when it refused with `error` (the parts do not fit,
/// or a seed's closest try): every part where [`DoesNotFit::parts`] says it ended, in the outline
/// it was given. Nothing is placed again. A caller that shows a layout file's conflicts (the
/// editor, `dev board3d`, issue #779) shows this board with the placement check's findings instead of
/// no board; `fab` still refuses it.
pub fn board_where_ended(
    design: &Design,
    mapping: &Mapping,
    library: &FootprintLibrary,
    placement: &PlaceOptions,
    error: &DoesNotFit,
) -> Result<Board, BoardError> {
    let mut u = Board::unplaced(design, mapping, library, placement)?;
    for p in &mut u.parts {
        if let Some(state) = error.parts.iter().find(|s| s.part == p.refdes) {
            p.at = ORIGIN + Pt::new(state.at.0, state.at.1);
            p.rotation = state.rotation;
            p.side = state.side;
        }
    }
    let outline = Rect::new(
        ORIGIN,
        Pt::new(ORIGIN.x + error.outline.0, ORIGIN.y + error.outline.1),
    );
    let corner_radius = design
        .features
        .outline
        .as_ref()
        .map_or(0.0, |o| o.corner_radius);
    Ok(u.into_board(design, outline, corner_radius))
}

/// The refusal with the requirements of the physical file that are not met where the placer
/// ended with the parts, and the lines that cannot all hold (ADR 0045).
/// `base` is what the layout file and the caller said, before the requirements were turned
/// into places.
fn explained(
    mut error: DoesNotFit,
    design: &Design,
    parts: &[Placed],
    base: &PlaceOptions,
    rules: &Rules,
) -> DoesNotFit {
    // The parts as the refusal lists them: where the placer ended, not where they started.
    let ended: Vec<Placed> = parts
        .iter()
        .map(|p| {
            let mut q = p.clone();
            if let Some(state) = error.parts.iter().find(|s| s.part == p.refdes) {
                q.at = ORIGIN + Pt::new(state.at.0, state.at.1);
                q.rotation = state.rotation;
                q.side = state.side;
            }
            q
        })
        .collect();
    let outline = Rect::new(
        ORIGIN,
        Pt::new(ORIGIN.x + error.outline.0, ORIGIN.y + error.outline.1),
    );
    error.requirements =
        netsmith_pcb::explain_requirements(design, &ended, outline, base, rules).map(Box::new);
    error
}

#[cfg(test)]
mod tests {
    use netsmith_pcb::{Layout, Side, place_options};

    use super::*;
    use crate::testlib::{fixtures, led_design};

    fn placed(design: &Design) -> Board {
        board_from_design(design, &Mapping::standard(), &fixtures()).unwrap()
    }

    #[test]
    fn placement_is_valid_and_the_same_every_time() {
        let board = placed(&led_design());
        assert_eq!(board.check_placement(), vec![]);
        assert_eq!(board, placed(&led_design()));
    }

    /// The LED board with each part's path its designator in lower case, as a layout names them.
    fn led_with_paths() -> Design {
        let mut design = led_design();
        for p in &mut design.parts {
            p.path = p.refdes.to_lowercase();
        }
        design
    }

    #[test]
    fn a_layout_puts_the_parts_it_locks_where_it_says_and_the_others_go_round_them() {
        let layout = Layout::parse(
            "version = 1\n[LedExample]\nseed = 7\n[LedExample.parts.r1]\nat = [5.0, 4.0]\nrotation = 90.0\nlock = true\n[LedExample.parts.d1]\nat = [12.0, 6.0]\nside = \"bottom\"\n",
            "led.pcb.toml",
        )
        .unwrap();
        let design = led_with_paths();
        let options = place_options(&design, Some(&layout));
        let board =
            board_from_design_placed(&design, &Mapping::standard(), &fixtures(), &options).unwrap();
        let r1 = board.part("R1").unwrap();
        assert_eq!((r1.at, r1.rotation), (ORIGIN + Pt::new(5.0, 4.0), 90.0));
        assert_eq!(board.part("D1").unwrap().side, Side::Back);
        assert_eq!(board.check_placement(), vec![]);
    }
}
