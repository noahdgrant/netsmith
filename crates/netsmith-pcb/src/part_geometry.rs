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
use crate::copper::{areas, artwork};
use crate::footprint::{Pad, PadKind, PadShape};
use crate::geometry::{Pt, Rect};
use crate::summary::{SummaryBox, no_part};
use netsmith_geom::{Polygons, pad_outline};

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
    /// `rect`, `round_rect`, `oval`, `circle` or `custom` (a pad drawn from an anchor and
    /// primitives, which `polygons` gives as it is; issue 1516). For a custom pad `width` and
    /// `height` are its anchor's size, not its copper's.
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
    /// The copper of a custom pad as it is on the board (the anchor and its primitives, the
    /// part's turn and face applied, the places as `at`'s): one polygon for each piece of
    /// copper, with the holes it has. Left out for every other shape, whose copper `at`,
    /// `width`, `height`, `turn` and `shape` give.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polygons: Option<Vec<PadPolygon>>,
    /// The faces with copper: `top`, `bottom`, or both for a through-hole pad.
    pub copper: Vec<&'static str>,
    /// The hole's size in mm (`[width, height]`, a slot's two sides), for a pad with one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drill: Option<[f64; 2]>,
}

/// A piece of a custom pad's copper on the board: its outer ring and the holes in it, each a
/// list of points `[x, y]` in mm in the layout file's frame.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PadPolygon {
    pub outer: Vec<[f64; 2]>,
    pub holes: Vec<Vec<[f64; 2]>>,
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
        // The copper of the custom pads, drawn as the Gerbers draw it, from the board as `fab`
        // writes it; made only for a board with a custom pad among the parts asked for.
        let customs = if wanted
            .iter()
            .any(|p| p.footprint.pads.iter().any(is_custom))
        {
            let art = artwork(board).map_err(|e| format!("the pads' copper: {e}"))?;
            let mut by_pad: HashMap<(String, String), Vec<Vec<PadPolygon>>> = HashMap::new();
            for pad in &art.pads {
                let mut pen = Polygons::new();
                pad_outline(pad).draw(&mut pen, None, &[]);
                let polygons = areas(&pen)
                    .into_iter()
                    .map(|a| PadPolygon {
                        outer: a.outer.iter().map(|q| point(*q, origin)).collect(),
                        holes: a
                            .holes
                            .iter()
                            .map(|h| h.iter().map(|q| point(*q, origin)).collect())
                            .collect(),
                    })
                    .collect();
                by_pad
                    .entry((pad.part.clone(), pad.number.clone()))
                    .or_default()
                    .push(polygons);
            }
            by_pad
        } else {
            HashMap::new()
        };
        let parts = wanted
            .into_iter()
            .map(|p| {
                let court = p.courtyard();
                // Several pads of a part can share a number: the nth of them is the nth drawn.
                let mut seen: HashMap<&str, usize> = HashMap::new();
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
                        .map(|pad| {
                            let nth = seen.entry(pad.number.as_str()).or_default();
                            let polygons = is_custom(pad)
                                .then(|| {
                                    customs
                                        .get(&(p.refdes.clone(), pad.number.clone()))
                                        .and_then(|drawn| drawn.get(*nth))
                                        .cloned()
                                })
                                .flatten();
                            *nth += 1;
                            pad_geometry(p, pad, &net_of, origin, polygons)
                        })
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
        self.text_moved([0.0, 0.0], "mm from the outline's top left corner, y down")
    }

    /// [`PartGeometries::text`] with the places moved by `by`, mm, and `frame` the words that
    /// say the frame they are then in (the heading says it).
    pub fn text_moved(&self, by: [f64; 2], frame: &str) -> String {
        let (mx, my) = (|x: f64| round(x + by[0]), |y: f64| round(y + by[1]));
        let mut text = format!(
            "{}: {} x {} mm, {} parts ({frame})\n",
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
                mx(f.at[0]),
                my(f.at[1]),
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
                    mx(c.min[0]),
                    my(c.min[1]),
                    mx(c.max[0]),
                    my(c.max[1])
                ));
            }
            for pad in &p.pads {
                text.push_str(&format!(
                    "  pad {} {} {} at {},{} size {} x {} turn {} net {}\n",
                    pad.number,
                    pad.kind,
                    pad.shape,
                    mx(pad.at[0]),
                    my(pad.at[1]),
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

/// Whether the pad is a custom one: a `custom` pad with primitives, which the footprint reader
/// keeps as its anchor's shape and the box the primitives span ([`Pad::extent`]).
fn is_custom(pad: &Pad) -> bool {
    pad.extent.is_some()
}

fn point(p: Pt, origin: Pt) -> [f64; 2] {
    [round(p.x - origin.x), round(p.y - origin.y)]
}

fn pad_geometry(
    p: &Placed,
    pad: &Pad,
    net_of: &HashMap<(&str, &str), &str>,
    origin: Pt,
    polygons: Option<Vec<PadPolygon>>,
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
            _ if is_custom(pad) => "custom",
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
        polygons,
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

#[cfg(test)]
mod custom_pad_tests {
    use super::*;
    use crate::footprint::Footprint;

    /// A footprint whose first pad is a custom one (a 0.4 mm square anchor and a polygon that
    /// reaches 1.5 mm to the right and 1 mm down, with a hole's worth of nothing) and whose
    /// second is a plain rectangle.
    fn corner_footprint() -> Footprint {
        Footprint::parse(
            "L:CORNER",
            r#"(footprint "CORNER"
                (property "Reference" "REF**" (at 0 -2 0) (layer "F.SilkS"))
                (pad "1" smd custom (at 0 0) (size 0.4 0.4) (layers "F.Cu" "F.Mask" "F.Paste")
                    (options (clearance outline) (anchor rect))
                    (primitives
                        (gr_poly (pts (xy -0.2 -0.2) (xy 1.5 -0.2) (xy 1.5 0.2) (xy 0.2 0.2)
                            (xy 0.2 1) (xy -0.2 1)) (width 0) (fill yes))))
                (pad "2" smd rect (at 3 0) (size 1 1) (layers "F.Cu" "F.Mask" "F.Paste")))"#,
        )
        .unwrap()
    }

    fn board_with_corner_pad(turn: f64, side: crate::board::Side) -> Board {
        let mut board = crate::board::tests::led_board();
        board.parts[0].footprint = corner_footprint();
        board.parts[0].at = Pt::new(20.0, 20.0);
        board.parts[0].rotation = turn;
        board.parts[0].side = side;
        board
    }

    fn corners(polygon: &PadPolygon) -> Vec<[f64; 2]> {
        let mut c = polygon.outer.clone();
        c.sort_by(|a, b| a.partial_cmp(b).unwrap());
        c.dedup();
        c
    }

    #[test]
    fn a_custom_pad_gives_its_shape_and_its_copper_as_a_polygon_on_the_board() {
        let board = board_with_corner_pad(0.0, crate::board::Side::Front);
        let origin = board.outline.min;
        let part = &PartGeometries::of(&board, std::slice::from_ref(&board.parts[0].refdes))
            .unwrap()
            .parts[0];
        let custom = &part.pads[0];
        assert_eq!(custom.shape, "custom");
        let polygons = custom.polygons.as_ref().expect("a custom pad has polygons");
        assert_eq!(polygons.len(), 1, "{polygons:?}");
        assert!(polygons[0].holes.is_empty());
        // The L's six corners, in the board's frame with the part at (20, 20).
        let (x, y) = (20.0 - origin.x, 20.0 - origin.y);
        let mut want: Vec<[f64; 2]> = [
            (-0.2, -0.2),
            (1.5, -0.2),
            (1.5, 0.2),
            (0.2, 0.2),
            (0.2, 1.0),
            (-0.2, 1.0),
        ]
        .iter()
        .map(|(dx, dy)| [round(x + dx), round(y + dy)])
        .collect();
        want.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let got = corners(&polygons[0]);
        assert_eq!(got, want, "{got:?}");
        // `bounds` is the box of that copper, and not the 0.4 mm anchor's.
        assert_eq!(custom.bounds.min, [round(x - 0.2), round(y - 0.2)]);
        assert_eq!(custom.bounds.max, [round(x + 1.5), round(y + 1.0)]);
        assert_eq!((custom.width, custom.height), (0.4, 0.4));
        // A pad that is not custom has no polygons.
        assert_eq!(part.pads[1].shape, "rect");
        assert_eq!(part.pads[1].polygons, None);
    }

    #[test]
    fn a_custom_pads_polygon_follows_the_parts_turn_and_face() {
        for (turn, side) in [
            (90.0, crate::board::Side::Front),
            (0.0, crate::board::Side::Back),
            (90.0, crate::board::Side::Back),
        ] {
            let board = board_with_corner_pad(turn, side);
            let origin = board.outline.min;
            let part = &PartGeometries::of(&board, std::slice::from_ref(&board.parts[0].refdes))
                .unwrap()
                .parts[0];
            let polygon = &part.pads[0].polygons.as_ref().unwrap()[0];
            // Each corner is the footprint's point taken to the board by the part's frame
            // (the rule in the module's text), and the box of the corners is the pad's bounds.
            let placed = &board.parts[0];
            let mut want: Vec<[f64; 2]> = [
                (-0.2, -0.2),
                (1.5, -0.2),
                (1.5, 0.2),
                (0.2, 0.2),
                (0.2, 1.0),
                (-0.2, 1.0),
            ]
            .iter()
            .map(|(x, y)| {
                let q = placed.to_board(Pt::new(*x, *y));
                [round(q.x - origin.x), round(q.y - origin.y)]
            })
            .collect();
            want.sort_by(|a, b| a.partial_cmp(b).unwrap());
            assert_eq!(corners(polygon), want, "turn {turn} {side:?}");
            let xs = want.iter().map(|p| p[0]);
            let ys = want.iter().map(|p| p[1]);
            assert_eq!(
                part.pads[0].bounds.min,
                [
                    xs.clone().fold(f64::MAX, f64::min),
                    ys.clone().fold(f64::MAX, f64::min)
                ],
                "turn {turn} {side:?}"
            );
            assert_eq!(
                part.pads[0].bounds.max,
                [xs.fold(f64::MIN, f64::max), ys.fold(f64::MIN, f64::max)],
                "turn {turn} {side:?}"
            );
        }
    }

    #[test]
    fn a_board_with_no_custom_pad_has_no_polygons_in_its_json() {
        let board = crate::board::tests::led_board();
        let all = PartGeometries::of(&board, &[]).unwrap();
        let json = serde_json::to_string(&all).unwrap();
        assert!(!json.contains("polygons"), "{json}");
        assert!(!json.contains("\"custom\""), "{json}");
    }
}
