//! The parts of a placed board as geometry (issue #778): each part's frame (where its footprint's
//! origin is, its turn, its face), its courtyard as shapes, and its pads with their nets, places
//! and sizes, in one document that a script reads without the editor and without parsing the
//! `.kicad_pcb` that `fab` writes.
//!
//! Every place is in the layout file's frame: mm from the outline's top left corner, x to the
//! right and y down. A part also gives its courtyard shapes and its pads in the footprint's own
//! frame (`local`), with the rule that takes one to the other ([`Placed::to_board`]), so that a
//! script can compute where the pads go for a place it has not tried: the footprint's point
//! `(x, y)` is mirrored left to right first, `(-x, y)`, on the bottom face, then turned by `turn`
//! degrees counterclockwise on screen (`x' = x cos t + y sin t`, `y' = -x sin t + y cos t`, as y
//! is down) and then moved by `at`.

use std::collections::HashMap;

use serde::Serialize;

use crate::board::{Board, Placed};
use crate::conflict::{Conflict, PlacementFault};
use crate::footprint::{Pad, PadKind, PadShape};
use crate::geometry::{Pt, Rect};
use crate::summary::{SummaryBox, no_part};

/// A number to 0.001 mm: the places of a pad and the size of a courtyard shape to a micron.
fn round(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

/// The placed parts of one board, as geometry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartGeometries {
    pub board: String,
    /// The outline's box, in mm.
    pub width: f64,
    pub height: f64,
    /// The parts asked for, in board order; every part when none was asked for.
    pub parts: Vec<PartGeometry>,
    /// The faults of the placement check where the parts are, as `layout fit` and
    /// `board fab --format json` have them. Empty for a board `fab` would place.
    pub placement: Vec<PlacementFault>,
    /// The sets of lines that cannot all hold where the parts are; set by the caller from what
    /// placing said.
    pub conflicts: Vec<Conflict>,
    /// Why the placer refused the layout file, when it did: the parts are then where the placer
    /// ended, with the layout file's places. `null` for a board that was placed.
    pub placement_refused: Option<String>,
    /// The parts asked for that the layout file has no place for (and no requirement pins), as
    /// `layout parts --as-laid-out` finds them (issue 1308): they are not on the board, so they
    /// are not in `parts`. Empty for a board that was placed.
    pub no_entry: Vec<NoEntry>,
}

/// A part with no place in the layout file, left out of the parts of a board as it is laid out.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NoEntry {
    pub refdes: String,
    pub path: String,
}

/// One part: its frame, its courtyard and its pads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartGeometry {
    pub refdes: String,
    pub path: String,
    pub value: String,
    /// The footprint's name in its library.
    pub footprint: String,
    /// Where the footprint's origin is, its turn, its face and its size.
    pub frame: PartFrame,
    /// The box round the courtyard's shapes, on the board.
    pub courtyard: SummaryBox,
    /// The courtyard's shapes, on the board: each is its own room. A turn that is not a right
    /// angle gives the box round the shape as turned.
    pub courtyards: Vec<SummaryBox>,
    /// The same shapes in the footprint's own frame: the origin at `(0, 0)`, before the face's
    /// mirror and the turn.
    pub local_courtyards: Vec<SummaryBox>,
    /// The body's box on the board, what a requirement on a place measures.
    pub body: SummaryBox,
    /// How tall the body stands above its face, in mm, when the design states it.
    pub height: Option<f64>,
    pub pads: Vec<PadGeometry>,
}

/// The frame of a part, in the layout file's.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct PartFrame {
    /// The footprint's origin, as the layout file's `at`: `[x, y]`.
    pub at: [f64; 2],
    /// Degrees, counterclockwise on screen, from 0 up to 360, as the layout file's `turn`.
    pub turn: f64,
    /// `top` or `bottom`.
    pub side: &'static str,
    /// Whether the footprint is mirrored left to right before it is turned (the bottom face).
    pub mirrored: bool,
    /// The size of the box round the courtyard on the board: its width and height, in mm.
    pub width: f64,
    pub height: f64,
}

/// One pad of a part.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PadGeometry {
    pub number: String,
    /// The net the pad is on; `null` for a pad on none.
    pub net: Option<String>,
    /// `smd`, `thru_hole` or `np_thru_hole`.
    pub kind: &'static str,
    /// `rect`, `round_rect`, `oval` or `circle`.
    pub shape: &'static str,
    /// The centre of the copper on the board: `[x, y]`.
    pub at: [f64; 2],
    /// The centre of the copper in the footprint's own frame.
    pub local: [f64; 2],
    /// The pad's own size, in mm, along its own axes (before its turn). A custom pad's anchor
    /// size; `bounds` has its whole copper.
    pub width: f64,
    pub height: f64,
    /// The pad's total turn on the board, in degrees.
    pub turn: f64,
    /// The box the copper spans on the board.
    pub bounds: SummaryBox,
    /// The faces with copper: `top`, `bottom`, or both for a through-hole pad.
    pub copper: Vec<&'static str>,
    /// The hole's size in mm (`[width, height]`, a slot's two sides), for a pad with one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drill: Option<[f64; 2]>,
}

impl PartGeometries {
    /// The geometry of the parts of `board` that `which` names: a part's path, its designator
    /// or a module's path (every part inside it); with none named, every part. A name that is
    /// none of those is the error, with the nearest names.
    pub fn of(board: &Board, which: &[String]) -> Result<PartGeometries, String> {
        Self::of_partial(board, which, &[])
    }

    /// [`PartGeometries::of`] for a board that lacks the parts `absent` (designator, path): a
    /// part with no place in the layout file, left off the board. A name that is one of them,
    /// or a module with one inside, is no error: they are given as `no_entry`, those `which`
    /// names, or all of them when it names none.
    pub fn of_partial(
        board: &Board,
        which: &[String],
        absent: &[(String, String)],
    ) -> Result<PartGeometries, String> {
        let absent_by = |name: &str| {
            absent
                .iter()
                .any(|(refdes, path)| path_names(path, refdes, name))
        };
        let wanted: Vec<&Placed> = if which.is_empty() {
            board.parts.iter().collect()
        } else {
            for name in which {
                if !board.parts.iter().any(|p| names(p, name)) && !absent_by(name) {
                    return Err(no_part(board, name));
                }
            }
            board
                .parts
                .iter()
                .filter(|p| which.iter().any(|name| names(p, name)))
                .collect()
        };
        let origin = board.outline.min;
        let frame = |r: Rect, by: Pt| SummaryBox {
            min: [round(r.min.x - by.x), round(r.min.y - by.y)],
            max: [round(r.max.x - by.x), round(r.max.y - by.y)],
        };
        let net_of: HashMap<(&str, &str), &str> = board
            .nets
            .iter()
            .flat_map(|n| {
                n.pads
                    .iter()
                    .map(move |p| ((p.refdes.as_str(), p.number.as_str()), n.name.as_str()))
            })
            .collect();
        let parts = wanted
            .into_iter()
            .map(|p| {
                let court = p.courtyard();
                PartGeometry {
                    refdes: p.refdes.clone(),
                    path: p.path.clone(),
                    value: p.value.clone(),
                    footprint: p.footprint.name.clone(),
                    frame: PartFrame {
                        at: [round(p.at.x - origin.x), round(p.at.y - origin.y)],
                        turn: round(p.rotation),
                        side: p.side.word(),
                        mirrored: p.mirrored(),
                        width: round(court.max.x - court.min.x),
                        height: round(court.max.y - court.min.y),
                    },
                    courtyard: frame(court, origin),
                    courtyards: p
                        .courtyards()
                        .into_iter()
                        .map(|r| frame(r, origin))
                        .collect(),
                    local_courtyards: p
                        .footprint
                        .courtyard_shapes
                        .iter()
                        .map(|r| frame(*r, Pt::default()))
                        .collect(),
                    body: frame(p.body(), origin),
                    height: p.height,
                    pads: p
                        .footprint
                        .pads
                        .iter()
                        .map(|pad| pad_geometry(p, pad, &net_of, origin))
                        .collect(),
                }
            })
            .collect();
        Ok(PartGeometries {
            board: board.name.clone(),
            width: round(board.outline.max.x - origin.x),
            height: round(board.outline.max.y - origin.y),
            parts,
            placement: board.placement_faults(),
            conflicts: Vec::new(),
            placement_refused: None,
            no_entry: absent
                .iter()
                .filter(|(refdes, path)| {
                    which.is_empty() || which.iter().any(|name| path_names(path, refdes, name))
                })
                .map(|(refdes, path)| NoEntry {
                    refdes: refdes.clone(),
                    path: path.clone(),
                })
                .collect(),
        })
    }

    /// The lines a person reads: each part's frame, then its pads.
    pub fn text(&self) -> String {
        let mut text = format!(
            "{}: {} x {} mm, {} parts (mm from the outline's top left corner, y down)\n",
            self.board,
            self.width,
            self.height,
            self.parts.len()
        );
        for p in &self.parts {
            let f = &p.frame;
            text.push_str(&format!(
                "{} ({}) {} {}: at {},{} turn {} {}{}, courtyard {} x {} in {} shape{}\n",
                p.refdes,
                p.path,
                p.footprint,
                p.value,
                f.at[0],
                f.at[1],
                f.turn,
                f.side,
                if f.mirrored { " (mirrored)" } else { "" },
                f.width,
                f.height,
                p.courtyards.len(),
                if p.courtyards.len() == 1 { "" } else { "s" },
            ));
            for c in &p.courtyards {
                text.push_str(&format!(
                    "  courtyard {},{} to {},{}\n",
                    c.min[0], c.min[1], c.max[0], c.max[1]
                ));
            }
            for pad in &p.pads {
                text.push_str(&format!(
                    "  pad {} {} {} at {},{} size {} x {} turn {} net {}\n",
                    pad.number,
                    pad.kind,
                    pad.shape,
                    pad.at[0],
                    pad.at[1],
                    pad.width,
                    pad.height,
                    pad.turn,
                    pad.net.as_deref().unwrap_or("-"),
                ));
            }
        }
        for n in &self.no_entry {
            text.push_str(&format!(
                "{} ({}): no entry in the layout file, and no requirement pins it: it has no place yet\n",
                n.refdes, n.path
            ));
        }
        if let Some(refused) = &self.placement_refused {
            text.push_str(&format!("placer refused the layout file: {refused}\n"));
        }
        for f in &self.placement {
            text.push_str(&format!("finding: {}\n", f.text));
        }
        for c in &self.conflicts {
            text.push_str(&format!("conflict: {}\n", c.text));
        }
        text
    }
}

/// Whether `name` names the part: its path, its designator, or a module the part is in.
fn names(p: &Placed, name: &str) -> bool {
    path_names(&p.path, &p.refdes, name)
}

/// [`names`] for a part given by its path and designator.
fn path_names(path: &str, refdes: &str, name: &str) -> bool {
    path == name
        || refdes == name
        || path
            .strip_prefix(name)
            .is_some_and(|rest| rest.starts_with('.') || rest.starts_with('['))
}

fn pad_geometry(
    p: &Placed,
    pad: &Pad,
    net_of: &HashMap<(&str, &str), &str>,
    origin: Pt,
) -> PadGeometry {
    let at = p.pad_centre(pad);
    let bounds = p.pad_bounds(pad);
    let (front, back) = p.pad_layers(pad);
    let mut copper = Vec::new();
    if front {
        copper.push("top");
    }
    if back {
        copper.push("bottom");
    }
    PadGeometry {
        number: pad.number.clone(),
        net: net_of
            .get(&(p.refdes.as_str(), pad.number.as_str()))
            .map(ToString::to_string),
        kind: match pad.kind {
            PadKind::Smd => "smd",
            PadKind::ThruHole => "thru_hole",
            PadKind::NpThruHole => "np_thru_hole",
        },
        shape: match pad.shape {
            PadShape::Rect => "rect",
            PadShape::RoundRect => "round_rect",
            PadShape::Oval => "oval",
            PadShape::Circle => "circle",
        },
        at: [round(at.x - origin.x), round(at.y - origin.y)],
        local: [round(pad.at.x), round(pad.at.y)],
        width: round(pad.width),
        height: round(pad.height),
        turn: round(p.pad_angle(pad).rem_euclid(360.0)),
        bounds: SummaryBox {
            min: [
                round(bounds.min.x - origin.x),
                round(bounds.min.y - origin.y),
            ],
            max: [
                round(bounds.max.x - origin.x),
                round(bounds.max.y - origin.y),
            ],
        },
        copper,
        drill: pad.drill.map(|d| [round(d.width), round(d.height)]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Side;

    fn board() -> Board {
        crate::board::tests::led_board()
    }

    #[test]
    fn every_part_is_listed_with_its_frame_courtyard_and_pads() {
        let board = board();
        let all = PartGeometries::of(&board, &[]).unwrap();
        assert_eq!(all.parts.len(), board.parts.len());
        let part = &all.parts[0];
        let placed = &board.parts[0];
        assert_eq!(part.refdes, placed.refdes);
        assert_eq!(part.pads.len(), placed.footprint.pads.len());
        assert_eq!(
            part.courtyards.len(),
            placed.footprint.courtyard_shapes.len()
        );
        assert_eq!(part.frame.side, placed.side.word());
        // A pad's place is the footprint's point taken to the board by the part's frame.
        let pad = &placed.footprint.pads[0];
        let at = placed.to_board(pad.at);
        let origin = board.outline.min;
        assert_eq!(
            part.pads[0].at,
            [round(at.x - origin.x), round(at.y - origin.y)]
        );
    }

    #[test]
    fn a_name_picks_a_part_by_path_designator_or_module_and_a_wrong_one_is_refused() {
        let board = board();
        let first = &board.parts[0];
        for name in [&first.path, &first.refdes] {
            let one = PartGeometries::of(&board, std::slice::from_ref(name)).unwrap();
            assert!(one.parts.iter().any(|p| p.refdes == first.refdes));
        }
        let error = PartGeometries::of(&board, &["no.such.part".to_string()]).unwrap_err();
        assert!(error.contains("no part `no.such.part`"), "{error}");
    }

    #[test]
    fn a_part_on_the_bottom_face_is_mirrored_and_its_pads_follow_the_rule_in_the_docs() {
        let mut board = board();
        board.parts[0].side = Side::Back;
        board.parts[0].rotation = 90.0;
        let part = &PartGeometries::of(&board, &[]).unwrap().parts[0];
        assert!(part.frame.mirrored && board.parts[0].mirrored());
        assert_eq!(part.frame.side, "bottom");
        // The rule of the module's text: mirror x, turn counterclockwise on screen (y down),
        // then move by the origin.
        let (t, origin) = (90.0_f64.to_radians(), board.outline.min);
        for (pad, own) in part.pads.iter().zip(&board.parts[0].footprint.pads) {
            let (x, y) = (-own.at.x, own.at.y);
            let (rx, ry) = (x * t.cos() + y * t.sin(), -x * t.sin() + y * t.cos());
            let want = [
                round(board.parts[0].at.x + rx - origin.x),
                round(board.parts[0].at.y + ry - origin.y),
            ];
            assert_eq!(pad.at, want, "pad {}", pad.number);
            assert_eq!(pad.local, [round(own.at.x), round(own.at.y)]);
        }
    }
}
