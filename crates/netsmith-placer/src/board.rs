//! A board from a design: the board model loads the footprints and turns the requirements of
//! the physical file into what the placer is told ([`Board::unplaced`]), the placer places the
//! parts, and the board is made of them ([`Unplaced::into_board`](netsmith_pcb::Unplaced::into_board)). These were
//! `Board::from_design`, `Board::from_design_placed` and `Board::precheck` of `netsmith-pcb`
//! before the placer had a crate of its own (issue #518).

use netsmith_core::Design;
use netsmith_kicad::Mapping;
use netsmith_pcb::board::Loaded;
use netsmith_pcb::geometry::{Pt, Rect};
use netsmith_pcb::placement::ORIGIN;
use netsmith_pcb::{
    Board, BoardError, DoesNotFit, FootprintLibrary, PlaceOptions, Placed, Precheck, Rules,
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
    let Loaded {
        rules,
        placement,
        parts,
        ..
    } = Board::load(design, mapping, library, placement)?;
    let plan = netsmith_pcb::constraints::plan(design, &parts, &placement, rules.clearance)
        .map_err(BoardError::Constraints)?;
    let found = netsmith_pcb::explain::before_placing(
        design,
        &parts,
        &placement,
        &plan,
        &rules,
        netsmith_pcb::board::outline_box(design),
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
            )
            .map_err(|e| {
                BoardError::DoesNotFit(explained(e, design, &u.parts, &u.base, &u.rules))
            })?,
            wanted.corner_radius,
        ),
    };
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
        netsmith_pcb::explain::explain(design, &ended, outline, base, rules).map(Box::new);
    error
}
