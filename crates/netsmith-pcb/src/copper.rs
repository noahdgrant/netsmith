//! The board's copper as geometry data: for each copper layer the pads, tracks, pours and
//! planes and copper drawings as polygons, and for the board the vias, the drilled holes and
//! the solder mask openings and the silkscreen of each face. It is what the 3D view's scene
//! draws the copper from (ADR 0105), and what a 2D view can draw it from instead of the
//! server's SVG.
//!
//! **The same copper as the Gerbers.** The board is read into the plotting model of
//! `netsmith-geom` ([`netsmith_geom::Artwork`]) from the tree of the `.kicad_pcb` that
//! [`crate::kicad_pcb`] writes, in memory: no file, no text and no KiCad in between. Then every
//! shape is drawn by the code that writes the Gerbers, into a pen that keeps polygons
//! ([`netsmith_geom::Polygons`]) instead of a file: a pad with
//! [`netsmith_geom::PadOutline::draw`], a pour with [`netsmith_geom::fill_pour`] (the filler
//! `fab` uses, with the clearances, thermal reliefs and edge band the copper files get), the
//! mask with [`netsmith_geom::plot_mask`]. So the pours here are the fill the Gerbers hold,
//! cut into straight pieces no further than 2 µm from its curves, and a footprint on the back,
//! a turned pad or a custom pad is where and what the file says, by one code.
//!
//! **Coordinates.** Millimetres in the board's own coordinates, as [`Board`] holds them and the
//! 2D PCB view draws them: the origin at the sheet's top left, x to the right and **y down**
//! (KiCad's board frame). A polygon's outer ring has a positive shoelace sum in these
//! coordinates (it runs clockwise on the screen) and its holes run the other way. The scene,
//! whose y is up, converts.
//!
//! **Order.** Layers from the front to the back (`F.Cu`, `In1.Cu`, ..., `B.Cu`); pads in the
//! order of [`Board::parts`] and each footprint's pads; tracks and vias in the board's order;
//! pours in the order of the planes and then the outer pours. The same board gives the same
//! data.
//!
//! **Text** on a copper or mask layer is drawn with fab's stroke font, as in the Gerbers.

use std::collections::HashMap;

use netsmith_geom::{
    Artwork, ArtworkPadKind, Pen, PolygonArea, Pt as GeomPt, TrackShape, drill_holes, edge_outline,
    fill_pour, gerber_escape, pad_outline, plot_drawings, plot_mask,
};

use netsmith_geom::Polygons;

use crate::board::Board;
use crate::footprint::{Drill, PadKind};
use crate::geometry::Pt;

/// Why a board's copper cannot be drawn: the board's file would not plot either.
#[derive(Debug, Clone, PartialEq)]
pub struct CopperError(pub String);

impl std::fmt::Display for CopperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the board's copper: {}", self.0)
    }
}

impl std::error::Error for CopperError {}

/// A polygon with holes, in board coordinates. `outer` has a positive shoelace sum (it runs
/// clockwise on the screen, y down) and each hole a negative one. A piece of copper inside a
/// hole is an area of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub outer: Vec<Pt>,
    pub holes: Vec<Vec<Pt>>,
}

impl Area {
    /// The area in mm², the holes taken away.
    pub fn size(&self) -> f64 {
        let a = |p: &[Pt]| shoelace(p).abs() / 2.0;
        a(&self.outer) - self.holes.iter().map(|h| a(h)).sum::<f64>()
    }
}

/// Twice the signed area of a ring (positive when it runs clockwise on the screen).
fn shoelace(ring: &[Pt]) -> f64 {
    let mut s = 0.0;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        s += a.x * b.y - b.x * a.y;
    }
    s
}

/// The copper of a board.
#[derive(Debug, Clone, PartialEq)]
pub struct Copper {
    /// Each copper layer, from the front to the back.
    pub layers: Vec<LayerCopper>,
    /// The vias. Each is a through via, with a ring of copper on every copper layer.
    pub vias: Vec<ViaCopper>,
    /// Every drilled hole: the pads' (plated or not) and then the vias'.
    pub holes: Vec<Hole>,
    /// The openings of the solder mask on the front (`F.Mask`) and then on the back (`B.Mask`).
    pub mask: Vec<MaskOpenings>,
    /// The silkscreen of the front (`F.SilkS`) and then of the back (`B.SilkS`).
    pub silk: Vec<SilkInk>,
}

/// The copper of one layer.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerCopper {
    /// `F.Cu`, `In1.Cu`, ..., `B.Cu`.
    pub layer: String,
    /// The pads with copper on this layer.
    pub pads: Vec<PadCopper>,
    pub tracks: Vec<TrackCopper>,
    /// The filled pours (outer layers) and planes (inner layers), as the Gerbers have them:
    /// the zone less the clearances round other nets' copper, the holes and the board edge,
    /// with thermal reliefs round its own net's pads. The pads, tracks and vias themselves are
    /// not in it (they are drawn over it, as in the file).
    pub pours: Vec<Pour>,
    /// The fills the board file holds as they are, as the Gerbers draw them: netsmith's
    /// teardrops (issue #431, S11), which the filler does not make but writes into the file.
    pub fills: Vec<Pour>,
    /// What footprints draw on this copper layer that is not a pad (lines, shapes, text).
    pub drawings: Vec<Area>,
}

/// A pad's copper on a layer.
#[derive(Debug, Clone, PartialEq)]
pub struct PadCopper {
    /// The part's path in the design (`power.reg`).
    pub part: String,
    /// The part's designator (`U3`).
    pub refdes: String,
    /// The pad's number (`1`, `A1`); several pads of a part can share one.
    pub number: String,
    pub net: Option<String>,
    /// A connector pad (a surface pad with no paste) counts as surface mount.
    pub kind: PadKind,
    /// The pad's centre on the board.
    pub at: Pt,
    /// The pad's turn on the board in degrees, counter-clockwise on the screen, as the board
    /// file writes it (with the part's turn in it; for a part on the back, of the mirrored pad).
    pub angle: f64,
    /// The pad's size before its turn.
    pub size: (f64, f64),
    /// The pad's hole, if it has one (its place is in [`Copper::holes`]).
    pub drill: Option<Drill>,
    /// The copper's shape on the board: one area for most pads, more for a custom pad whose
    /// primitives do not touch.
    pub areas: Vec<Area>,
}

/// A straight piece of track.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackCopper {
    pub net: Option<String>,
    pub from: Pt,
    pub to: Pt,
    /// The copper's width; the ends are round.
    pub width: f64,
}

impl TrackCopper {
    /// The copper of the piece: what a round pen of its width sweeps from one end to the other
    /// (a stadium), drawn by the pen the Gerbers' strokes are, with its ends cut no further
    /// than 2 µm from their curves.
    pub fn outline(&self) -> Area {
        let mut pen = Polygons::new();
        Pen::line(
            &mut pen,
            self.width,
            None,
            GeomPt::new(self.from.x, self.from.y),
            GeomPt::new(self.to.x, self.to.y),
            &[],
        );
        areas(&pen).into_iter().next().unwrap_or(Area {
            outer: Vec::new(),
            holes: Vec::new(),
        })
    }
}

/// A via.
#[derive(Debug, Clone, PartialEq)]
pub struct ViaCopper {
    pub net: Option<String>,
    pub at: Pt,
    /// The ring's outer diameter, the same on every layer.
    pub diameter: f64,
    pub drill: f64,
    /// Whether it runs from the front copper to the back. Always true: netsmith makes only
    /// through vias, and the plotting reader refuses others.
    pub through: bool,
}

/// What a hole is drilled for.
#[derive(Debug, Clone, PartialEq)]
pub enum HoleOf {
    Pad {
        part: String,
        refdes: String,
        number: String,
    },
    /// The via at this index of [`Copper::vias`].
    Via(usize),
}

/// A drilled hole, round or a slot.
#[derive(Debug, Clone, PartialEq)]
pub struct Hole {
    /// The centre of a round hole; one end of a slot's path.
    pub at: Pt,
    /// The other end of a slot's path (the drill runs from `at` to it), `None` for a round hole.
    pub slot_to: Option<Pt>,
    pub diameter: f64,
    /// Plated through (a pad with copper or a via) rather than a bare hole.
    pub plated: bool,
    pub of: HoleOf,
}

/// The pour or plane of one net on a layer: one piece of its copper.
#[derive(Debug, Clone, PartialEq)]
pub struct Pour {
    pub net: Option<String>,
    pub area: Area,
}

/// Where the solder mask of a face is open.
#[derive(Debug, Clone, PartialEq)]
pub struct MaskOpenings {
    /// `F.Mask` or `B.Mask`.
    pub layer: String,
    pub areas: Vec<Area>,
}

/// The silkscreen of a face: the ink of its legend Gerber, drawn by the same code (the
/// footprints' lines, arcs, circles and polygons with their widths, and their text in fab's
/// stroke font). Nothing is clipped: the Gerber is not either, and the board house takes the ink
/// off the pads.
#[derive(Debug, Clone, PartialEq)]
pub struct SilkInk {
    /// `F.SilkS` or `B.SilkS`.
    pub layer: String,
    pub areas: Vec<Area>,
}

/// The board in netsmith-geom's plotting model, as fab reads it from the board file: built
/// from the same tree in memory.
pub fn artwork(board: &Board) -> Result<Artwork, CopperError> {
    let tree = crate::export::board_tree(board);
    Artwork::from_sexp(&tree).map_err(|e| CopperError(e.to_string()))
}

fn pt(p: GeomPt) -> Pt {
    Pt::new(p.x, p.y)
}

fn area(a: PolygonArea) -> Area {
    Area {
        outer: a.outer.into_iter().map(pt).collect(),
        holes: a
            .holes
            .into_iter()
            .map(|h| h.into_iter().map(pt).collect())
            .collect(),
    }
}

fn areas(pen: &Polygons) -> Vec<Area> {
    pen.areas().into_iter().map(area).collect()
}

fn kind(k: ArtworkPadKind) -> PadKind {
    match k {
        ArtworkPadKind::Smd | ArtworkPadKind::Connect => PadKind::Smd,
        ArtworkPadKind::Thru => PadKind::ThruHole,
        ArtworkPadKind::Npth => PadKind::NpThruHole,
    }
}

/// The board's copper.
pub fn copper(board: &Board) -> Result<Copper, CopperError> {
    let art = artwork(board)?;
    let rules = netsmith_geom::plot_rules_of(Some(&crate::export::kicad_pro(board)));
    let outline = edge_outline(&art).map_err(CopperError)?;
    let paths: HashMap<&str, &str> = board
        .parts
        .iter()
        .map(|p| (p.refdes.as_str(), p.path.as_str()))
        .collect();
    let path_of = |refdes: &str| paths.get(refdes).copied().unwrap_or_default().to_string();
    // What the fab writers note about the drawings (a glyph the font lacks); the Gerbers'
    // notes say it, so it is not repeated here.
    let mut notes = Vec::new();

    let mut layers = Vec::new();
    for layer in &art.copper {
        let mut pads_here = Vec::new();
        for pad in art.pads.iter().filter(|p| art.pad_copper_on(p, layer)) {
            let mut pen = Polygons::new();
            pad_outline(pad).draw(&mut pen, None, &[]);
            pads_here.push(PadCopper {
                part: path_of(&pad.part),
                refdes: pad.part.clone(),
                number: pad.number.clone(),
                net: pad.net.clone(),
                kind: kind(pad.kind),
                at: pt(pad.at),
                angle: pad.angle,
                size: pad.size,
                drill: pad.drill.map(|d| Drill {
                    width: d.width,
                    height: d.length,
                }),
                areas: areas(&pen),
            });
        }

        let mut tracks = Vec::new();
        for t in art.tracks.iter().filter(|t| t.layer == *layer) {
            let points = match &t.shape {
                TrackShape::Line(a, b) => vec![*a, *b],
                // An arc (a router's rounded bend, or a board file's) is cut into straight
                // pieces no further than 2 µm from it.
                TrackShape::Arc(arc) => arc.points(),
            };
            for w in points.windows(2) {
                tracks.push(TrackCopper {
                    net: t.net.clone(),
                    from: pt(w[0]),
                    to: pt(w[1]),
                    width: t.width,
                });
            }
        }

        let mut pen = Polygons::new();
        fill_pour(&mut pen, &art, layer, &outline, &rules).map_err(CopperError)?;
        let mut pours = Vec::new();
        for (tag, pieces) in pen.areas_by_net() {
            let net = net_of(&art, tag.as_deref());
            pours.extend(pieces.into_iter().map(|a| Pour {
                net: net.clone(),
                area: area(a),
            }));
        }

        // A fill the board file holds already, drawn as the plotter draws it.
        let fills = art
            .zones
            .iter()
            .filter(|z| z.keepout.is_none())
            .flat_map(|z| {
                z.filled
                    .iter()
                    .filter(|(l, poly)| l == layer && poly.len() >= 3)
                    .map(|(_, poly)| Pour {
                        net: z.net.clone(),
                        area: Area {
                            outer: poly.iter().copied().map(pt).collect(),
                            holes: Vec::new(),
                        },
                    })
            })
            .collect();

        let mut pen = Polygons::new();
        plot_drawings(&mut pen, &art, layer, None, &mut notes);
        layers.push(LayerCopper {
            layer: layer.clone(),
            pads: pads_here,
            tracks,
            pours,
            fills,
            drawings: areas(&pen),
        });
    }

    let through = art.copper.len() >= 2;
    let vias = art
        .vias
        .iter()
        .map(|v| ViaCopper {
            net: v.net.clone(),
            at: pt(v.at),
            diameter: v.diameter,
            drill: v.drill,
            through,
        })
        .collect();

    let holes = drill_holes(&art)
        .into_iter()
        .map(|h| Hole {
            at: pt(h.at),
            slot_to: h.slot_to.map(pt),
            diameter: h.diameter,
            plated: h.plated,
            of: if h.via {
                HoleOf::Via(h.index)
            } else {
                let pad = &art.pads[h.index];
                HoleOf::Pad {
                    part: path_of(&pad.part),
                    refdes: pad.part.clone(),
                    number: pad.number.clone(),
                }
            },
        })
        .collect();

    let mut mask = Vec::new();
    for layer in ["F.Mask", "B.Mask"] {
        let mut pen = Polygons::new();
        plot_mask(&mut pen, &art, layer, &mut notes).map_err(|e| CopperError(e.to_string()))?;
        mask.push(MaskOpenings {
            layer: layer.to_string(),
            areas: areas(&pen),
        });
    }

    let mut silk = Vec::new();
    for layer in ["F.SilkS", "B.SilkS"] {
        let mut pen = Polygons::new();
        plot_drawings(&mut pen, &art, layer, None, &mut notes);
        silk.push(SilkInk {
            layer: layer.to_string(),
            areas: areas(&pen),
        });
    }

    Ok(Copper {
        layers,
        vias,
        holes,
        mask,
        silk,
    })
}

/// The fill of one net on one layer that has zones of a routes file ([`zone_fills`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneFill {
    /// KiCad's name of the layer: `F.Cu`, `In1.Cu`.
    pub layer: String,
    pub net: String,
    /// Its pieces, as [`LayerCopper::pours`] has them.
    pub areas: Vec<Area>,
}

/// The fill of each layer that has a zone of a routes file ([`Board::zoned`]), by net: what
/// the Gerbers hold, filled by `netsmith_geom::fill` from the board file in memory (each zone
/// less its clearances round other nets' copper, by KiCad's priority rule where zones overlap).
/// Empty for a board with no such zone, at no cost.
pub fn zone_fills(board: &Board) -> Result<Vec<ZoneFill>, CopperError> {
    if board.zones.is_empty() {
        return Ok(Vec::new());
    }
    let art = artwork(board)?;
    let rules = netsmith_geom::plot_rules_of(Some(&crate::export::kicad_pro(board)));
    let outline = edge_outline(&art).map_err(CopperError)?;
    let mut fills = Vec::new();
    for layer in art.copper.iter().filter(|l| board.zoned(l)) {
        let mut pen = Polygons::new();
        fill_pour(&mut pen, &art, layer, &outline, &rules).map_err(CopperError)?;
        for (tag, pieces) in pen.areas_by_net() {
            let Some(net) = net_of(&art, tag.as_deref()) else {
                continue;
            };
            fills.push(ZoneFill {
                layer: layer.clone(),
                net,
                areas: pieces.into_iter().map(area).collect(),
            });
        }
    }
    Ok(fills)
}

/// The net whose zone the filler drew with the attribute `tag` (`N,<net>`, escaped as a
/// Gerber attribute is; `N,N/C` is no net).
fn net_of(art: &Artwork, tag: Option<&str>) -> Option<String> {
    let tag = tag?;
    art.zones
        .iter()
        .filter_map(|z| z.net.as_deref())
        .find(|n| tag == format!("N,{}", gerber_escape(n)))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{BoardPour, CopperLayer, Placed, Side, Track, Via};

    fn four() -> Board {
        crate::board::tests::four_layer_board()
    }

    fn part<'a>(board: &'a mut Board, refdes: &str) -> &'a mut Placed {
        board.parts.iter_mut().find(|p| p.refdes == refdes).unwrap()
    }

    fn layer<'a>(c: &'a Copper, name: &str) -> &'a LayerCopper {
        c.layers.iter().find(|l| l.layer == name).unwrap()
    }

    /// Even-odd over a ring.
    fn in_ring(ring: &[Pt], p: Pt) -> bool {
        let mut inside = false;
        for i in 0..ring.len() {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
                inside = !inside;
            }
        }
        inside
    }

    fn in_area(a: &Area, p: Pt) -> bool {
        in_ring(&a.outer, p) && !a.holes.iter().any(|h| in_ring(h, p))
    }

    fn poured(l: &LayerCopper, net: &str, p: Pt) -> bool {
        l.pours
            .iter()
            .any(|q| q.net.as_deref() == Some(net) && in_area(&q.area, p))
    }

    /// A point of the footprint's frame where the SVG view draws it: the part's
    /// `translate(at) rotate(-rotation) scale(-1 1)` (the mirror only on the back), then the
    /// pad's `translate(pad.at) rotate(-pad.angle)`, as `svg.rs` writes them.
    fn svg_point(placed: &Placed, pad: &crate::Pad, local: Pt) -> Pt {
        let rot = |p: Pt, deg: f64| {
            let (s, c) = deg.to_radians().sin_cos();
            Pt::new(p.x * c - p.y * s, p.x * s + p.y * c)
        };
        let q = rot(local, -pad.angle);
        let q = Pt::new(q.x + pad.at.x, q.y + pad.at.y);
        let q = if placed.mirrored() {
            Pt::new(-q.x, q.y)
        } else {
            q
        };
        let q = rot(q, -placed.rotation);
        Pt::new(q.x + placed.at.x, q.y + placed.at.y)
    }

    fn bounds(points: impl Iterator<Item = Pt>) -> (Pt, Pt) {
        points.fold(
            (Pt::new(f64::MAX, f64::MAX), Pt::new(f64::MIN, f64::MIN)),
            |(lo, hi), p| {
                (
                    Pt::new(lo.x.min(p.x), lo.y.min(p.y)),
                    Pt::new(hi.x.max(p.x), hi.y.max(p.y)),
                )
            },
        )
    }

    #[test]
    fn pads_are_where_the_svg_view_draws_them_turned_and_on_the_back() {
        let mut board = four();
        part(&mut board, "R1").rotation = 90.0;
        {
            let d1 = part(&mut board, "D1");
            d1.side = Side::Back;
            d1.rotation = 270.0;
        }
        part(&mut board, "J1").rotation = 30.0;
        let c = copper(&board).unwrap();
        let mut checked = 0;
        for placed in &board.parts {
            for pad in &placed.footprint.pads {
                let face = match (placed.mirrored(), pad.kind) {
                    (_, PadKind::ThruHole) => "F.Cu",
                    (true, _) => "B.Cu",
                    (false, _) => "F.Cu",
                };
                let found = layer(&c, face)
                    .pads
                    .iter()
                    .find(|p| p.refdes == placed.refdes && p.number == pad.number)
                    .unwrap_or_else(|| panic!("{}.{} on {face}", placed.refdes, pad.number));
                assert_eq!(found.part, placed.path);
                let centre = svg_point(placed, pad, Pt::new(0.0, 0.0));
                assert!(
                    found.at.distance(centre) < 1e-6,
                    "{}.{}: {:?} and the view's {centre:?}",
                    placed.refdes,
                    pad.number,
                    found.at
                );
                // At a right angle, the copper spans the box the view draws the pad in.
                if placed.rotation % 90.0 == 0.0 {
                    let (w, h) = (pad.width / 2.0, pad.height / 2.0);
                    let drawn = bounds(
                        [(-w, -h), (w, -h), (w, h), (-w, h)]
                            .into_iter()
                            .map(|(x, y)| svg_point(placed, pad, Pt::new(x, y))),
                    );
                    let copper = bounds(found.areas.iter().flat_map(|a| a.outer.iter().copied()));
                    for (a, b) in [(drawn.0, copper.0), (drawn.1, copper.1)] {
                        assert!(
                            a.distance(b) < 0.003,
                            "{}.{}: the view's box {drawn:?}, the copper's {copper:?}",
                            placed.refdes,
                            pad.number
                        );
                    }
                }
                checked += 1;
            }
        }
        assert!(checked >= 6, "{checked}");
        // A surface pad of a part on the back is on the back copper only.
        assert!(!layer(&c, "F.Cu").pads.iter().any(|p| p.refdes == "D1"));
    }

    /// A point on the board at least `room` from every pad's centre.
    fn free_point(board: &Board, room: f64) -> Pt {
        let o = board.outline;
        let centres: Vec<Pt> = board
            .parts
            .iter()
            .flat_map(|p| p.footprint.pads.iter().map(|pad| p.pad_centre(pad)))
            .collect();
        let mut best = (Pt::new(0.0, 0.0), 0.0);
        for i in 1..40 {
            for j in 1..40 {
                let p = Pt::new(
                    o.min.x + o.width() * f64::from(i) / 40.0,
                    o.min.y + o.height() * f64::from(j) / 40.0,
                );
                let edge = (p.x - o.min.x)
                    .min(o.max.x - p.x)
                    .min(p.y - o.min.y)
                    .min(o.max.y - p.y);
                let d = centres
                    .iter()
                    .map(|c| c.distance(p))
                    .fold(f64::MAX, f64::min)
                    .min(edge);
                if d > best.1 {
                    best = (p, d);
                }
            }
        }
        assert!(
            best.1 >= room,
            "no free point {room} mm from the pads: {best:?}"
        );
        best.0
    }

    #[test]
    fn a_via_is_a_plated_hole_cut_out_of_the_other_nets_plane_and_joined_to_its_own() {
        let mut board = four();
        let at = free_point(&board, 1.5);
        board.vias.push(Via {
            net: "3V3".into(),
            at,
            diameter: 0.6,
            drill: 0.3,
        });
        let c = copper(&board).unwrap();
        let via = c.vias.last().unwrap();
        assert_eq!(
            (
                via.net.as_deref(),
                via.at,
                via.diameter,
                via.drill,
                via.through
            ),
            (Some("3V3"), at, 0.6, 0.3, true)
        );
        let hole = c
            .holes
            .iter()
            .find(|h| h.of == HoleOf::Via(c.vias.len() - 1))
            .unwrap();
        assert!(hole.plated && hole.diameter == 0.3 && hole.at == at);
        // The ground plane keeps its clearance from the via; the 3.3 V plane runs through it.
        let gap = board.rules.plane_clearance;
        let gnd = layer(&c, "In1.Cu");
        assert!(!poured(gnd, "GND", at));
        assert!(!poured(gnd, "GND", Pt::new(at.x + 0.3 + gap * 0.9, at.y)));
        assert!(poured(gnd, "GND", Pt::new(at.x + 0.3 + gap + 0.05, at.y)));
        assert!(poured(layer(&c, "In2.Cu"), "3V3", at));
    }

    #[test]
    fn a_track_is_a_line_on_its_layer_and_an_outer_pour_keeps_clear_of_it() {
        let mut board = four();
        board.pours.push(BoardPour {
            layer: CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: None,
        });
        let at = free_point(&board, 1.5);
        let (from, to) = (Pt::new(at.x - 1.0, at.y), Pt::new(at.x + 1.0, at.y));
        board.tracks.push(Track {
            net: "3V3".into(),
            layer: CopperLayer::Front,
            from,
            to,
            width: 0.25,
            mid: None,
        });
        let c = copper(&board).unwrap();
        let top = layer(&c, "F.Cu");
        assert!(top.tracks.contains(&TrackCopper {
            net: Some("3V3".into()),
            from,
            to,
            width: 0.25,
        }));
        assert!(layer(&c, "B.Cu").tracks.is_empty());
        let gap = board.rules.plane_clearance;
        assert!(!poured(top, "GND", at));
        assert!(!poured(top, "GND", Pt::new(at.x, at.y + 0.125 + gap * 0.9)));
        assert!(poured(top, "GND", Pt::new(at.x, at.y + 0.125 + gap + 0.05)));
    }

    #[test]
    fn a_pour_is_cut_round_other_nets_pads_and_joined_to_its_own_by_spokes() {
        let mut board = four();
        board.pours.push(BoardPour {
            layer: CopperLayer::Front,
            net: "GND".into(),
            stitch_pitch: None,
        });
        let c = copper(&board).unwrap();
        let top = layer(&c, "F.Cu");
        assert!(!top.pours.is_empty());
        assert!(top.pours.iter().all(|p| p.net.as_deref() == Some("GND")));
        let pad = |refdes: &str, number: &str| {
            top.pads
                .iter()
                .find(|p| p.refdes == refdes && p.number == number)
                .unwrap()
        };
        // R1.1 is on 3V3: the pour has a hole round it.
        let other = pad("R1", "1");
        assert_eq!(other.net.as_deref(), Some("3V3"));
        assert!(!poured(top, "GND", other.at));
        let hole = top
            .pours
            .iter()
            .flat_map(|p| &p.area.holes)
            .find(|h| in_ring(h, other.at))
            .expect("a hole round R1.1");
        // The hole is the pad grown by the clearance (R1.2, on a third net, is near enough
        // that its clearance joins the same hole, so the hole reaches further on that side).
        let (lo, hi) = bounds(hole.iter().copied());
        let (plo, phi) = bounds(other.areas.iter().flat_map(|a| a.outer.iter().copied()));
        let margins = [plo.x - lo.x, plo.y - lo.y, hi.x - phi.x, hi.y - phi.y];
        let gap = board.rules.plane_clearance;
        let least = margins.iter().copied().fold(f64::MAX, f64::min);
        assert!((least - gap).abs() < 0.005, "{margins:?} {gap}");
        // D1.1 is on GND: a thermal relief, whose spokes reach the pad's centre.
        let own = pad("D1", "1");
        assert_eq!(own.net.as_deref(), Some("GND"));
        assert!(poured(top, "GND", own.at));
    }

    #[test]
    fn the_artwork_is_the_one_fab_reads_from_the_board_file() {
        let mut board = four();
        board.pours.push(BoardPour {
            layer: CopperLayer::Back,
            net: "GND".into(),
            stitch_pitch: None,
        });
        part(&mut board, "D1").side = Side::Back;
        let from_file = Artwork::parse(&crate::kicad_pcb(&board)).unwrap();
        assert_eq!(artwork(&board).unwrap(), from_file);
    }

    #[test]
    fn the_mask_opens_over_each_pad_on_its_face() {
        let board = four();
        let c = copper(&board).unwrap();
        assert_eq!(
            c.mask.iter().map(|m| m.layer.as_str()).collect::<Vec<_>>(),
            ["F.Mask", "B.Mask"]
        );
        for pad in &layer(&c, "F.Cu").pads {
            assert!(
                c.mask[0].areas.iter().any(|a| in_area(a, pad.at)),
                "{}.{}",
                pad.refdes,
                pad.number
            );
        }
        // Only the through-hole header has pads on the back.
        for pad in &layer(&c, "B.Cu").pads {
            assert_eq!(pad.refdes, "J1");
            assert!(c.mask[1].areas.iter().any(|a| in_area(a, pad.at)));
        }
    }

    #[test]
    fn a_track_has_the_outline_of_a_round_ended_stroke() {
        let t = TrackCopper {
            net: None,
            from: Pt::new(1.0, 2.0),
            to: Pt::new(4.0, 2.0),
            width: 0.5,
        };
        let o = t.outline();
        assert!(o.holes.is_empty());
        let (lo, hi) = bounds(o.outer.iter().copied());
        assert!((lo.x - 0.75).abs() < 1e-9 && (hi.x - 4.25).abs() < 1e-9);
        assert!((lo.y - 1.75).abs() < 1e-9 && (hi.y - 2.25).abs() < 1e-9);
        // The rectangle between the ends and a disc at each end, a little less the cut curves.
        let want = 3.0 * 0.5 + std::f64::consts::PI * 0.25 * 0.25;
        assert!((o.size() - want).abs() < 5e-3, "{} and {want}", o.size());
    }

    #[test]
    fn the_same_board_gives_the_same_copper() {
        assert_eq!(copper(&four()).unwrap(), copper(&four()).unwrap());
    }
}
