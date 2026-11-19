//! Whether each part the placer places has room among the parts that stay where they are,
//! before any placing (issue #1283): [`parts_without_room`].
//!
//! A board whose locked parts leave no room for a large part used to be found out by the
//! placer, which tried every seed for its whole budget first. Here each part the placer places
//! (one with no entry in the layout file, or an unlocked entry) is measured against the free
//! space the outline leaves inside the edge wall ([`crate::Rules::edge_wall`]), less the
//! cutouts, the keepouts that keep parts off the face, and the room the locked parts and the
//! parts a requirement pins take on it ([`crate::Placed::room_on`]): if no free rectangle of a
//! face holds the box of the part's courtyard at any turn the placer may give it (the right
//! angles, and 45 degrees for a part whose `.place.turn_step` is 45, as the box round the turned
//! courtyard), on any face it may go on (its requirement's face, else both), the part has no
//! room. The finding names the part and the largest free rectangle of each face it may go on.
//!
//! The free space is the summary's grid ([`crate::summary`]: cells of 0.25 mm, coarser on a
//! board of more than 800 cells on a side), on which a cell an obstacle touches is taken. So
//! that the grid never says "no room" of a part that fits, a part needs one cell less than its
//! size on each axis (a free span of the board loses less than one cell to the grid). A part
//! that fits within a cell of the limit may still be said to fit. Not measured, so a part is
//! not said for them: the other parts the placer places (each part is measured alone, against
//! the parts that stay), the parts' routing margins, the other face of a part with a hole, a
//! courtyard of several shapes that nests round another part (its box is measured), the
//! requirements' regions, heights, and a part that may hang over an edge. A board whose outline
//! is not stated (the placer sizes it) is not measured.
//!
//! It is a warning of `board check`, `board build`, `layout check` and `board fab` (an error
//! with `--strict`), said before the placer starts.

use std::fmt;

use netsmith_core::Design;
use netsmith_kicad::Mapping;
use serde::Serialize;

use crate::board::{Board, Side, member, outline_box};
use crate::footprint::FootprintLibrary;
use crate::geometry::Rect;
use crate::placement::{ORIGIN, PlaceOptions};
use crate::summary::{Grid, SummaryBox, rooms};

/// The largest free rectangle of one face, in the layout file's frame (mm from the outline's
/// top left corner, y down).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FreeRoom {
    /// `top` or `bottom`.
    pub face: &'static str,
    /// `None` when nothing of the face is free.
    pub largest: Option<SummaryBox>,
}

/// A part the placer places that no free rectangle holds ([`parts_without_room`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoRoom {
    pub refdes: String,
    pub path: String,
    /// The box of its courtyard unturned, width and height in mm.
    pub courtyard: [f64; 2],
    /// The turns it was tried at, in degrees.
    pub turns: Vec<f64>,
    /// Each face it may go on, with the largest free rectangle there.
    pub faces: Vec<FreeRoom>,
}

impl fmt::Display for NoRoom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mm = crate::mm_text;
        let turns: Vec<String> = self.turns.iter().map(|t| mm(*t)).collect();
        let free: Vec<String> = self
            .faces
            .iter()
            .map(|face| match &face.largest {
                Some(b) => format!(
                    "{} x {} mm at ({}, {}) on the {}",
                    mm(b.max[0] - b.min[0]),
                    mm(b.max[1] - b.min[1]),
                    mm(b.min[0]),
                    mm(b.min[1]),
                    face.face
                ),
                None => format!("nothing on the {}", face.face),
            })
            .collect();
        let faces: Vec<&str> = self.faces.iter().map(|face| face.face).collect();
        write!(
            f,
            "{} has no room among the parts that stay where they are: no free rectangle of the {} holds its courtyard of {} x {} mm turned {} degrees, inside the edge wall and clear of the locked and pinned parts, the cutouts and the keepouts; the largest free rectangle is {}: move or unlock a locked part, give the board more room, or choose a smaller part",
            member(&self.path, &self.refdes),
            faces.join(" or the "),
            mm(self.courtyard[0]),
            mm(self.courtyard[1]),
            turns.join(", "),
            free.join(", and ")
        )
    }
}

/// What [`parts_without_room`] measured.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RoomCheck {
    /// How many parts the placer places were measured: each once, so a board of `n` of them is
    /// `n`, whatever their size (a count of the work, for tests).
    pub measured: usize,
    /// The parts with no room, in board order.
    pub no_room: Vec<NoRoom>,
}

/// The faces' words, as the layout file has them.
fn face_word(side: Side) -> &'static str {
    match side {
        Side::Front => "top",
        Side::Back => "bottom",
    }
}

/// Each part the placer places that has no room among the parts that stay where they are
/// (`options.fixed`, the layout file's locks, and the parts the requirements pin), as the
/// module's header says. Places nothing: the footprints are loaded and measured. A board whose
/// outline is not stated measures nothing. Refused, with the reason, where the board is
/// ([`Board::unplaced`]).
pub fn parts_without_room(
    design: &Design,
    mapping: &Mapping,
    library: &FootprintLibrary,
    options: &PlaceOptions,
) -> Result<RoomCheck, String> {
    let (outline, finite, _) = outline_box(design);
    if !finite {
        return Ok(RoomCheck::default());
    }
    let mut u = Board::unplaced(design, mapping, library, options).map_err(|e| e.to_string())?;
    let fixed = u.placement.fixed.clone();
    let sides = u.placement.sides.clone();
    let overhang = u.overhang.clone();
    for part in &mut u.parts {
        if let Some(f) = fixed.get(&part.refdes) {
            part.at = ORIGIN + f.at;
            part.rotation = f.rotation;
            part.side = f.side;
            part.reference_side = f.text;
        }
    }
    let free: Vec<_> = u
        .parts
        .iter()
        .filter(|p| !fixed.contains_key(&p.refdes) && !overhang.contains_key(&p.refdes))
        .cloned()
        .collect();
    let corner_radius = design
        .features
        .outline
        .as_ref()
        .map_or(0.0, |o| o.corner_radius);
    let mut board = u.into_board(design, outline, corner_radius);
    board.parts.retain(|p| fixed.contains_key(&p.refdes));
    // The free cells of each face, the face's own obstacles taken.
    let grids: Vec<(Side, Grid)> = [Side::Front, Side::Back]
        .into_iter()
        .map(|side| {
            let mut grid = Grid::inside(&board, board.rules.edge_wall());
            for (_, shape) in board.cutout_shapes() {
                grid.take(&shape.rect);
            }
            for k in board.keepout_areas() {
                if k.forbids_parts_on(side) {
                    grid.take(&k.rect);
                }
            }
            for r in rooms(&board, side) {
                grid.take(&r);
            }
            (side, grid)
        })
        .collect();
    let origin = board.outline.min;
    let frame = |r: &Rect| SummaryBox {
        min: [r.min.x - origin.x, r.min.y - origin.y],
        max: [r.max.x - origin.x, r.max.y - origin.y],
    };
    let mut check = RoomCheck::default();
    for part in &free {
        netsmith_core::job::checkpoint();
        check.measured += 1;
        let court = part.footprint.courtyard;
        let (w, h) = (court.width(), court.height());
        if w <= 0.0 || h <= 0.0 {
            continue;
        }
        let mut turns = vec![0.0, 90.0];
        let mut boxes = vec![(w, h), (h, w)];
        if crate::constraints::turn_step(design, &part.path) < 90.0 - 1e-9 {
            let d = (w + h) / std::f64::consts::SQRT_2;
            turns.push(45.0);
            boxes.push((d, d));
        }
        let faces: Vec<&(Side, Grid)> = grids
            .iter()
            .filter(|(side, _)| sides.get(&part.refdes).is_none_or(|s| s == side))
            .collect();
        // A cell fewer than the size on each axis: the grid takes the cells an obstacle
        // touches, so a free span is short of its length by less than one cell.
        let cells = |v: f64, cell: f64| ((v / cell + 1e-9).floor() as usize).saturating_sub(1);
        let fits = faces.iter().any(|(_, grid)| {
            boxes
                .iter()
                .any(|&(bw, bh)| grid.holds(cells(bw, grid.cell), cells(bh, grid.cell)))
        });
        if fits {
            continue;
        }
        check.no_room.push(NoRoom {
            refdes: part.refdes.clone(),
            path: part.path.clone(),
            courtyard: [w, h],
            turns,
            faces: faces
                .iter()
                .map(|(side, grid)| FreeRoom {
                    face: face_word(*side),
                    largest: grid.largest().map(|cells| frame(&grid.rect(cells))),
                })
                .collect(),
        });
    }
    Ok(check)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::tests::{fixtures, led_design};
    use crate::placement::Fixed;
    use crate::{Pt, ReferenceSide, place_options};

    /// The LED board on an outline of `width` x `height` mm.
    fn design(width: f64, height: f64) -> Design {
        let mut design = led_design();
        design.features.outline = Some(netsmith_core::Outline {
            width: (width, width),
            height: (height, height),
            corner_radius: 0.0,
            polygon: None,
        });
        design
    }

    fn check(design: &Design, options: &PlaceOptions) -> RoomCheck {
        parts_without_room(design, &Mapping::standard(), &fixtures(), options)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn lock(options: &mut PlaceOptions, refdes: &str, x: f64, y: f64) {
        options.fixed.insert(
            refdes.to_string(),
            Fixed {
                at: Pt::new(x, y),
                rotation: 0.0,
                side: Side::Front,
                text: ReferenceSide::default(),
            },
        );
    }

    #[test]
    fn every_part_of_a_roomy_board_has_room_and_each_is_measured_once() {
        let design = design(60.0, 40.0);
        let found = check(&design, &place_options(&design, None));
        assert_eq!(found.no_room, vec![]);
        assert_eq!(found.measured, design.parts.len());
    }

    #[test]
    fn a_locked_part_is_an_obstacle_and_not_measured() {
        let design = design(60.0, 40.0);
        let mut options = place_options(&design, None);
        lock(&mut options, "J1", 30.0, 20.0);
        let found = check(&design, &options);
        assert_eq!(found.measured, design.parts.len() - 1);
        assert_eq!(found.no_room, vec![]);
    }

    #[test]
    fn an_outline_the_design_leaves_open_measures_nothing() {
        let mut design = led_design();
        design.features.outline = None;
        let found = check(&design, &place_options(&design, None));
        assert_eq!(found, RoomCheck::default());
    }

    /// The board where the header J1, locked in the middle of a narrow outline, leaves a strip
    /// on each side that is narrower than the LED's courtyard: the LED, which a requirement
    /// keeps on the top, has no room there. (On the bottom the header takes only its pads, so
    /// the LED would have room there.)
    #[test]
    fn a_part_wider_than_every_free_strip_has_no_room_and_the_largest_free_rectangle_is_named() {
        // The header's courtyard on the board, to size the outline round it.
        let probe = design(60.0, 40.0);
        let mut options = place_options(&probe, None);
        lock(&mut options, "J1", 30.0, 20.0);
        let led = crate::board::tests::unplaced_board(&probe)
            .part("D1")
            .unwrap()
            .footprint
            .courtyard;
        let header = crate::board::tests::unplaced_board(&probe)
            .part("J1")
            .unwrap()
            .footprint
            .courtyard;
        let wall = crate::board::tests::unplaced_board(&probe)
            .rules
            .edge_wall();
        // As tall as the header and the walls, and as wide as the header and two walls and a
        // strip on each side of half the LED's smaller side.
        let small = led.width().min(led.height());
        let width = header.width() + 2.0 * wall + small;
        let height = header.height() + 2.0 * wall;
        let design = design(width, height);
        let mut options = place_options(&design, None);
        lock(
            &mut options,
            "J1",
            width / 2.0 - header.centre().x,
            height / 2.0 - header.centre().y,
        );
        let both = check(&design, &options);
        assert!(!both.no_room.iter().any(|n| n.refdes == "D1"), "{both:?}");
        options.sides.insert("D1".to_string(), Side::Front);
        let found = check(&design, &options);
        let d1 = found
            .no_room
            .iter()
            .find(|n| n.refdes == "D1")
            .unwrap_or_else(|| panic!("D1 has no room: {found:?}"));
        assert_eq!(d1.turns, [0.0, 90.0]);
        let faces: Vec<&str> = d1.faces.iter().map(|f| f.face).collect();
        assert_eq!(faces, ["top"]);
        let largest = d1.faces[0].largest.expect("a strip is free");
        assert!(largest.max[0] - largest.min[0] < small, "{largest:?}");
        let text = d1.to_string();
        assert!(text.contains("D1 has no room"), "{text}");
        assert!(text.contains("the largest free rectangle is"), "{text}");
        assert!(text.contains("on the top"), "{text}");
        // With room for the LED beside the header, it has room.
        let wide = self::design(width + 2.0 * led.width().max(led.height()), height);
        let mut options = place_options(&wide, None);
        options.sides.insert("D1".to_string(), Side::Front);
        lock(
            &mut options,
            "J1",
            wall + header.width() / 2.0 - header.centre().x,
            height / 2.0 - header.centre().y,
        );
        let found = check(&wide, &options);
        assert!(!found.no_room.iter().any(|n| n.refdes == "D1"), "{found:?}");
    }
}
