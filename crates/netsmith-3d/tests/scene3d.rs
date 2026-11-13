//! The 3D scene of a board (ADR 0105, `netsmith_3d::scene3d`): an invented 30 x 20 mm board with
//! a 4 x 2 mm box on each face, a second box on the top, and a two-pin through-hole row, each
//! placed by hand. Heights are set on the placed parts, as the design would state them.

use std::collections::BTreeMap;

use crate::common;

use netsmith_3d::scene3d::{
    CopperLayer as CopperLayer3, CopperPad, HeightProvenance, Pins, Plan, PlanArea, Scene,
    SceneCopper,
};
use netsmith_core::{Design, Features, Net, Outline, Part, PinRef};
use netsmith_kicad::testlib::TempDir;
use netsmith_pcb::protrusion::Protrusion;
use netsmith_pcb::{
    Board, BoardKeepout, BoardPour, CopperLayer, FootprintLibrary, PlaceOptions, Pt, Side, Track,
    Via,
};

/// A box 4 mm along x and 2 mm along y, its origin at its centre, a pad at each end.
const BOX: &str = "(footprint \"Box\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
     (property \"Reference\" \"REF**\" (at 0 -3 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (property \"Value\" \"Box\" (at 0 3 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (attr smd) \
     (pad \"1\" smd rect (at -1.5 0) (size 0.8 1.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
     (pad \"2\" smd rect (at 1.5 0) (size 0.8 1.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
     (fp_rect (start -2 -1) (end 2 1) (stroke (width 0.1) (type solid)) (fill none) (layer \"F.Fab\")) \
     (fp_rect (start -2.2 -1.2) (end 2.2 1.2) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n";

/// Two plated holes 2.54 mm apart along y.
const ROW: &str = "(footprint \"Row2\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
     (property \"Reference\" \"REF**\" (at 0 -3 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (property \"Value\" \"Row2\" (at 0 3 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (attr through_hole) \
     (pad \"1\" thru_hole circle (at 0 0) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\")) \
     (pad \"2\" thru_hole circle (at 0 2.54) (size 1.7 1.7) (drill 1) (layers \"*.Cu\" \"*.Mask\")) \
     (fp_rect (start -1.27 -1.27) (end 1.27 3.81) (stroke (width 0.1) (type solid)) (fill none) (layer \"F.Fab\")) \
     (fp_rect (start -1.8 -1.8) (end 1.8 4.3) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n";

fn part(refdes: &str, kind: &str, path: &str) -> Part {
    Part {
        provides: Vec::new(),
        refdes: refdes.into(),
        kind: kind.into(),
        value: kind.into(),
        package: Some(kind.into()),
        path: path.into(),
        pins: Vec::new(),
    }
}

/// The board, placed by the placer and then moved by hand: `a` on the top at (10, 5) from the
/// outline's top-left corner, `b` on the top at (22, 5), `under` on the bottom at (10, 15) and
/// the row on the top at (25, 12), all unturned.
pub(crate) fn board() -> Board {
    board_with(common::process())
}

/// [`board`] made with `process`.
pub(crate) fn board_with(process: netsmith_core::Process) -> Board {
    let folder = TempDir::new("scene3d");
    std::fs::write(folder.join("Box.kicad_mod"), BOX).unwrap();
    std::fs::write(folder.join("Row2.kicad_mod"), ROW).unwrap();
    let mapping = "[[part]]\nkind = \"Box\"\npackage = \"Box\"\nsymbol = \"t:R\"\nfootprint = \"t:Box\"\n\
                   [[part]]\nkind = \"Row2\"\npackage = \"Row2\"\nsymbol = \"t:R\"\nfootprint = \"t:Row2\"\n";
    let mapping = netsmith_kicad::Mapping::parse(mapping, "scene3d", std::path::Path::new("."))
        .unwrap_or_else(|e| panic!("{e}"));
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
        .with_library("t", folder.path());
    let design = Design {
        modules: Vec::new(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        name: "Scene".into(),
        process: Some(process),
        features: Features {
            outline: Some(Outline {
                width: (30.0, 30.0),
                height: (20.0, 20.0),
                corner_radius: 0.0,
                polygon: None,
            }),
            ..Features::default()
        },
        parts: vec![
            part("U1", "Box", "a"),
            part("U2", "Box", "b"),
            part("U3", "Box", "under"),
            part("J1", "Row2", "row"),
        ],
        nets: ["SIG:U1.1,U2.1", "GND:U1.2,U3.2", "3V3:U2.2"]
            .map(|n| {
                let (name, pins) = n.split_once(':').unwrap();
                Net {
                    name: name.into(),
                    pins: pins
                        .split(',')
                        .map(|p| {
                            let (refdes, pin) = p.split_once('.').unwrap();
                            PinRef {
                                refdes: refdes.into(),
                                pin: pin.into(),
                            }
                        })
                        .collect(),
                }
            })
            .to_vec(),
    };
    let mut board = netsmith_place_route::board_from_design_placed(
        &design,
        &mapping,
        &library,
        &PlaceOptions::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let origin = board.outline.min;
    for (path, x, y, side, height) in [
        ("a", 10.0, 5.0, Side::Front, Some(2.0)),
        ("b", 22.0, 5.0, Side::Front, Some(3.0)),
        ("under", 10.0, 15.0, Side::Back, Some(1.2)),
        ("row", 25.0, 12.0, Side::Front, None),
    ] {
        let p = board.parts.iter_mut().find(|p| p.path == path).unwrap();
        p.at = origin + Pt::new(x, y);
        p.rotation = 0.0;
        p.side = side;
        p.height = height;
    }
    board
}

/// A height limit at `(x, y)` from the outline's corner, `w` by `h`, on `layers`.
fn limit(name: &str, at: (f64, f64), size: (f64, f64), layers: &[&str], max: f64) -> BoardKeepout {
    BoardKeepout {
        name: name.into(),
        part: None,
        at: Some(Pt::new(at.0, at.1)),
        size,
        layers: layers.iter().map(|l| l.to_string()).collect(),
        tracks: false,
        vias: false,
        copper: false,
        parts: false,
        max_height: Some(max),
        drawn: false,
    }
}

pub(crate) fn scene(board: &Board) -> Scene {
    let provenance = BTreeMap::from([("b".to_string(), HeightProvenance::Catalog)]);
    Scene::of(board, &provenance)
}

#[test]
fn the_board_is_its_outline_and_stackup_with_the_top_face_at_zero() {
    let s = scene(&board());
    assert_eq!((s.board.width, s.board.height), (30.0, 20.0));
    assert_eq!(s.board.thickness, 1.6);
    let first = &s.board.layers[0];
    let last = s.board.layers.last().unwrap();
    assert_eq!((first.name.as_str(), first.z_top), ("F.Mask", 0.0));
    assert_eq!((last.name.as_str(), last.z_bottom), ("B.Mask", -1.6));
    // Each layer starts where the one above it ends.
    for pair in s.board.layers.windows(2) {
        assert_eq!(pair[0].z_bottom, pair[1].z_top, "{pair:?}");
    }
    // The two plated holes of the row, by path and pad, in plan with y up.
    let holes: Vec<_> = s
        .board
        .holes
        .iter()
        .map(|h| (h.from, h.diameter, h.plated))
        .collect();
    assert_eq!(holes.len(), 2);
    assert_eq!((holes[0].0.x, holes[0].0.y), (25.0, 8.0));
    assert_eq!((holes[1].0.x, holes[1].0.y), (25.0, 5.46));
    assert!(holes.iter().all(|h| h.1 == 1.0 && h.2));
}

#[test]
fn a_body_stands_up_from_the_top_face_and_hangs_down_from_the_bottom() {
    let s = scene(&board());
    let a = s.body("a").unwrap();
    assert_eq!(a.z_range(), (Some(0.0), Some(2.0)));
    assert_eq!(a.provenance, HeightProvenance::Stated);
    // At (10, 5) from the top-left, y down: (10, 15) in plan.
    assert_eq!((a.bounds.min.x, a.bounds.min.y), (8.0, 14.0));
    assert_eq!((a.bounds.max.x, a.bounds.max.y), (12.0, 16.0));
    let under = s.body("under").unwrap();
    assert_eq!(under.z_range(), (Some(-2.8), Some(-1.6)));
    assert_eq!(s.body("b").unwrap().provenance, HeightProvenance::Catalog);
    // A part with no height has none: no zero and no guess.
    let row = s.body("row").unwrap();
    assert_eq!(row.z_range(), (Some(0.0), None));
    assert_eq!(row.provenance, HeightProvenance::Unknown);
    let c = s.coverage();
    assert_eq!((c.known, c.total, c.stated, c.catalog), (3, 4, 2, 1));
    assert_eq!(s.tallest(Side::Front).unwrap().path, "b");
    assert_eq!(s.tallest(Side::Back).unwrap().path, "under");
    // The bodies are by path.
    let paths: Vec<&str> = s.bodies.iter().map(|b| b.path.as_str()).collect();
    assert_eq!(paths, ["a", "b", "row", "under"]);
}

#[test]
fn a_turned_part_has_its_turned_corners() {
    let mut board = board();
    board
        .parts
        .iter_mut()
        .find(|p| p.path == "a")
        .unwrap()
        .rotation = 90.0;
    let a = scene(&board).body("a").unwrap().clone();
    // 2 mm along x and 4 mm along y once turned.
    assert_eq!(a.bounds.max.x - a.bounds.min.x, 2.0);
    assert_eq!(a.bounds.max.y - a.bounds.min.y, 4.0);
    assert_eq!(a.rotation, 90.0);
}

#[test]
fn the_pins_below_the_board_are_known_only_where_the_design_states_them() {
    let mut board = board();
    assert!(matches!(
        scene(&board).body("row").unwrap().pins,
        Pins::Unknown { .. }
    ));
    assert_eq!(scene(&board).body("a").unwrap().pins, Pins::None);
    board
        .parts
        .iter_mut()
        .find(|p| p.path == "row")
        .unwrap()
        .protrusion = Some(Protrusion {
        length: 1.5,
        source: "scene.physical.nsm:3".into(),
    });
    match &scene(&board).body("row").unwrap().pins {
        Pins::Known {
            z_top,
            z_bottom,
            length,
            ..
        } => assert_eq!((*z_top, *z_bottom, *length), (-1.6, -3.1, 1.5)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_limit_names_the_tallest_part_under_it_its_margin_and_the_unknown_ones() {
    let mut board = board();
    // Over `a` and the row on the top, and over `under` on the bottom.
    board.keepouts = vec![
        limit("lid", (0.0, 0.0), (30.0, 20.0), &["top"], 2.5),
        limit("floor", (5.0, 12.0), (10.0, 6.0), &["bottom"], 1.0),
    ];
    let s = scene(&board);
    let names: Vec<&str> = s.limits.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["floor", "lid"]);
    let lid = &s.limits[1];
    assert_eq!(lid.faces.len(), 1);
    let top = &lid.faces[0];
    assert_eq!((top.z_bottom, top.z_top), (0.0, 2.5));
    assert_eq!(top.tallest.as_ref().unwrap().path, "b");
    assert_eq!(top.margin, Some(-0.5));
    assert_eq!(top.unknown, ["row"]);
    let floor = &s.limits[0].faces[0];
    assert_eq!(
        (floor.side, floor.z_bottom, floor.z_top),
        (Side::Back, -2.6, -1.6)
    );
    assert_eq!(floor.margin, Some(-0.2));
    // The findings are the placement check's own.
    let tall: Vec<(&str, &str)> = s
        .findings
        .too_tall
        .iter()
        .map(|t| (t.path.as_str(), t.keepout.as_str()))
        .collect();
    assert_eq!(tall, [("b", "lid"), ("under", "floor")]);
    let unchecked: Vec<&str> = s
        .findings
        .unchecked
        .iter()
        .map(|u| u.path.as_str())
        .collect();
    assert_eq!(unchecked, ["row"]);
}

#[test]
fn two_bodies_on_one_face_that_share_space_are_an_overlap_and_faces_apart_are_not() {
    let mut board = board();
    assert!(scene(&board).findings.overlaps.is_empty());
    // `b` half over `a`, on the same face; `under` is right below `a` on the other face.
    let origin = board.outline.min;
    board.parts.iter_mut().find(|p| p.path == "b").unwrap().at = origin + Pt::new(12.0, 5.0);
    let s = scene(&board);
    assert_eq!(s.findings.overlaps.len(), 1, "{:?}", s.findings.overlaps);
    let o = &s.findings.overlaps[0];
    assert_eq!(
        (o.a.as_str(), o.b.as_str(), o.side),
        ("a", "b", Side::Front)
    );
    assert_eq!((o.z_bottom, o.z_top), (0.0, 2.0));
    // A body of unknown height overlaps nothing that can be told.
    board
        .parts
        .iter_mut()
        .find(|p| p.path == "b")
        .unwrap()
        .height = None;
    assert!(scene(&board).findings.overlaps.is_empty());
}

#[test]
fn the_same_board_gives_the_same_scene() {
    assert_eq!(scene(&board()), scene(&board()));
}

fn copper(s: &Scene) -> &SceneCopper {
    s.copper.as_ref().unwrap_or_else(|e| panic!("{e}"))
}

/// Even-odd over a ring.
fn in_ring(ring: &[Plan], p: Plan) -> bool {
    let mut inside = false;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x) {
            inside = !inside;
        }
    }
    inside
}

pub(crate) fn in_area(a: &PlanArea, p: Plan) -> bool {
    in_ring(&a.outer, p) && !a.holes.iter().any(|h| in_ring(h, p))
}

/// Twice the signed area: positive for a ring that runs counterclockwise, y up.
fn shoelace(r: &[Plan]) -> f64 {
    (0..r.len())
        .map(|i| {
            let (a, b) = (r[i], r[(i + 1) % r.len()]);
            a.x * b.y - b.x * a.y
        })
        .sum()
}

fn extent(a: &PlanArea) -> (Plan, Plan) {
    a.outer.iter().fold(
        (
            Plan {
                x: f64::MAX,
                y: f64::MAX,
            },
            Plan {
                x: f64::MIN,
                y: f64::MIN,
            },
        ),
        |(lo, hi), p| {
            (
                Plan {
                    x: lo.x.min(p.x),
                    y: lo.y.min(p.y),
                },
                Plan {
                    x: hi.x.max(p.x),
                    y: hi.y.max(p.y),
                },
            )
        },
    )
}

fn pad<'a>(l: &'a CopperLayer3, refdes: &str, n: &str) -> &'a CopperPad {
    l.pads
        .iter()
        .find(|p| p.refdes == refdes && p.number == n)
        .unwrap_or_else(|| panic!("{refdes}.{n} on {}", l.name))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The board with a track, a via and a pour on the front, all by hand, in layout-file
/// coordinates (from the outline's top-left corner, y down).
pub(crate) fn routed() -> Board {
    let mut board = board();
    let origin = board.outline.min;
    board.tracks.push(Track {
        net: "SIG".into(),
        layer: CopperLayer::Front,
        from: origin + Pt::new(14.0, 6.0),
        to: origin + Pt::new(18.0, 6.0),
        width: 0.25,
    });
    board.vias.push(Via {
        net: "SIG".into(),
        at: origin + Pt::new(14.0, 12.0),
        diameter: 0.6,
        drill: 0.3,
    });
    board.pours.push(BoardPour {
        layer: CopperLayer::Front,
        net: "GND".into(),
        stitch_pitch: None,
    });
    board
}

#[test]
fn each_copper_layer_is_at_its_height_of_the_stackup() {
    let s = scene(&routed());
    let c = copper(&s);
    let names: Vec<&str> = c.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["F.Cu", "B.Cu"]);
    for l in &c.layers {
        let stack = s.board.layers.iter().find(|k| k.name == l.name).unwrap();
        assert_eq!(
            (l.z_top, l.z_bottom, l.thickness),
            (stack.z_top, stack.z_bottom, stack.thickness)
        );
    }
    // The copper is under the solder mask: 10 um of it on the top face.
    assert_eq!((c.layers[0].z_top, c.layers[0].z_bottom), (-0.01, -0.045));
    assert_eq!((c.layers[1].z_top, c.layers[1].z_bottom), (-1.555, -1.59));
    // A via is a barrel from the top copper to the bottom copper.
    let via = &c.vias[0];
    assert_eq!((via.z_top, via.z_bottom), (-0.01, -1.59));
    // The mask is on the outside of each face.
    assert_eq!(
        c.mask
            .iter()
            .map(|m| (m.layer.as_str(), m.side, m.z_top, m.z_bottom))
            .collect::<Vec<_>>(),
        [
            ("F.Mask", Side::Front, 0.0, -0.01),
            ("B.Mask", Side::Back, -1.59, -1.6)
        ]
    );
}

#[test]
fn pads_tracks_and_vias_are_in_the_scene_frame_with_y_up() {
    let s = scene(&routed());
    let c = copper(&s);
    let (top, bottom) = (&c.layers[0], &c.layers[1]);
    // U1 is at (10, 5) from the top-left corner, its pads 1.5 mm either side: in plan, y up,
    // at (8.5, 15) and (11.5, 15). Its pads are 0.8 by 1.6 mm.
    let p1 = pad(top, "U1", "1");
    assert_eq!((p1.part.as_str(), p1.net.as_deref()), ("a", Some("SIG")));
    assert_eq!((p1.at.x, p1.at.y), (8.5, 15.0));
    assert_eq!(p1.size, (0.8, 1.6));
    let (lo, hi) = extent(&p1.areas[0]);
    assert!(
        near(lo.x, 8.1) && near(hi.x, 8.9) && near(lo.y, 14.2) && near(hi.y, 15.8),
        "{lo:?} {hi:?}"
    );
    assert_eq!(
        (pad(top, "U1", "2").at.x, pad(top, "U1", "2").at.y),
        (11.5, 15.0)
    );
    // The part on the bottom face is mirrored, as the 2D view draws it (`scale(-1 1)`): its
    // pad 1 is at +1.5, and at (10, 15) from the top-left corner it is at y = 5, up. Its
    // surface pads are on the back copper only.
    assert!(!top.pads.iter().any(|p| p.refdes == "U3"));
    let u3 = pad(bottom, "U3", "1");
    assert_eq!((u3.part.as_str(), u3.at.x, u3.at.y), ("under", 11.5, 5.0));
    assert_eq!(
        (pad(bottom, "U3", "2").at.x, pad(bottom, "U3", "2").at.y),
        (8.5, 5.0)
    );
    // Pads are in order of their part's path.
    let parts: Vec<&str> = top.pads.iter().map(|p| p.part.as_str()).collect();
    assert_eq!(parts, ["a", "a", "b", "b", "row", "row"]);
    // The track from (14, 6) to (18, 6) is at y = 14, with round ends.
    let t = &top.tracks[0];
    assert_eq!(
        (t.from.x, t.from.y, t.to.x, t.to.y, t.width),
        (14.0, 14.0, 18.0, 14.0, 0.25)
    );
    assert_eq!(t.net.as_deref(), Some("SIG"));
    let (lo, hi) = extent(&t.area);
    assert!(
        near(lo.x, 13.875) && near(hi.x, 18.125) && near(lo.y, 13.875) && near(hi.y, 14.125),
        "{lo:?} {hi:?}"
    );
    assert!(bottom.tracks.is_empty());
    // The via at (14, 12) is at y = 8.
    let via = &c.vias[0];
    assert_eq!(
        (via.at.x, via.at.y, via.diameter, via.drill),
        (14.0, 8.0, 0.6, 0.3)
    );
    assert_eq!(via.net.as_deref(), Some("SIG"));
}

#[test]
fn rings_turn_counterclockwise_outside_and_clockwise_inside_and_a_pour_has_holes_where_pads_are() {
    let s = scene(&routed());
    let c = copper(&s);
    let top = &c.layers[0];
    let all = c
        .layers
        .iter()
        .flat_map(|l| {
            l.pads
                .iter()
                .flat_map(|p| p.areas.iter())
                .chain(l.tracks.iter().map(|t| &t.area))
                .chain(l.pours.iter().map(|p| &p.area))
        })
        .chain(c.mask.iter().flat_map(|m| m.areas.iter()));
    let mut holes = 0;
    for a in all {
        assert!(shoelace(&a.outer) > 0.0, "{:?}", a.outer);
        for h in &a.holes {
            assert!(shoelace(h) < 0.0);
            holes += 1;
        }
    }
    assert!(holes > 0);
    // The pour of GND is cut round the pad of SIG at (8.5, 15) in plan, and is whole at the
    // flipped place (8.5, 5), and it joins U1.2 (on GND) at (11.5, 15).
    assert!(!top.pours.is_empty() && top.pours.iter().all(|p| p.net.as_deref() == Some("GND")));
    let poured = |x: f64, y: f64| top.pours.iter().any(|p| in_area(&p.area, Plan { x, y }));
    assert!(!poured(8.5, 15.0));
    assert!(poured(8.5, 5.0));
    assert!(poured(11.5, 15.0));
    assert!(poured(2.0, 2.0));
    // Nothing is outside the 30 x 20 outline.
    for p in &top.pours {
        let (lo, hi) = extent(&p.area);
        assert!(
            lo.x >= 0.0 && lo.y >= 0.0 && hi.x <= 30.0 && hi.y <= 20.0,
            "{lo:?} {hi:?}"
        );
    }
    // Per net, the copper adds up: the pads of SIG are 2 x 0.8 x 1.6 and its track is a
    // stadium.
    let sig = top.area_of(Some("SIG"));
    let want = 2.0 * 0.8 * 1.6 + 4.0 * 0.25 + std::f64::consts::PI * 0.125 * 0.125;
    assert!((sig - want).abs() < 5e-3, "{sig} and {want}");
    // The row's pads have no net: that copper is listed first.
    assert_eq!(top.nets(), [None, Some("3V3"), Some("GND"), Some("SIG")]);
}

#[test]
fn the_mask_is_open_over_the_pads_of_each_face_in_the_scene_frame() {
    let s = scene(&routed());
    let c = copper(&s);
    let open =
        |face: usize, x: f64, y: f64| c.mask[face].areas.iter().any(|a| in_area(a, Plan { x, y }));
    assert!(open(0, 8.5, 15.0) && open(0, 11.5, 15.0));
    assert!(!open(0, 8.5, 5.0));
    assert!(open(1, 11.5, 5.0) && open(1, 8.5, 5.0));
    assert!(!open(1, 8.5, 15.0));
}

#[test]
fn a_four_layer_board_has_copper_on_four_layers_with_the_planes_inside() {
    let board = board_with(common::four_layer_process());
    let s = scene(&board);
    let c = copper(&s);
    let names: Vec<&str> = c.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"]);
    // Each plane is one net's sheet of copper, between the layers' own Z.
    for (l, net) in c.layers[1..3].iter().zip(["GND", "3V3"]) {
        assert!(
            l.pads
                .iter()
                .all(|p| p.kind == netsmith_pcb::PadKind::ThruHole)
                && l.tracks.is_empty()
        );
        assert!(
            !l.pours.is_empty() && l.pours.iter().all(|p| p.net.as_deref() == Some(net)),
            "{}",
            l.name
        );
        assert!(l.z_top > l.z_bottom && l.z_bottom > -s.board.stack_thickness);
    }
    let z: Vec<f64> = c.layers.iter().map(|l| l.z_top).collect();
    assert!(z.windows(2).all(|w| w[0] > w[1]), "{z:?}");
}

/// The volume a closed mesh holds: positive when its triangles face out.
fn volume(m: &netsmith_3d::mesh::Mesh) -> f64 {
    m.indices
        .chunks(3)
        .map(|t| {
            let [a, b, c] = [0, 1, 2].map(|k| m.positions[t[k] as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

/// The board with `a` (on the top) and `under` (on the bottom) given the generated footprint
/// `id`, turned a quarter, `height` mm tall.
fn shaped(id: &str, height: Option<f64>) -> Board {
    let mut board = board();
    for path in ["a", "under"] {
        let p = board.parts.iter_mut().find(|p| p.path == path).unwrap();
        p.footprint = netsmith_pcb::generate::generate(id).unwrap();
        p.rotation = 90.0;
        p.height = height;
    }
    board
}

#[test]
fn a_shaped_part_lands_where_its_box_was_on_either_face_and_turned() {
    use netsmith_3d::mesh::{Material, of_scene};
    for (id, h) in [
        ("Resistor_SMD:R_0603_1608Metric", 0.5),
        ("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm", 1.75),
    ] {
        let board = shaped(id, Some(h));
        let s = scene(&board);
        let meshes = of_scene(&s);
        let (left, bottom) = (board.outline.min.x, board.outline.max.y);
        for path in ["a", "under"] {
            let body = s.body(path).unwrap();
            assert!(body.shape.is_some(), "{id} {path}");
            let mine: Vec<_> = meshes.iter().filter(|m| m.id == path).collect();
            assert!(mine.iter().any(|m| m.material == Material::Metal), "{id}");
            for m in &mine {
                assert!(
                    volume(m) > 0.0,
                    "{id} {path}: a {} mesh is inside out",
                    m.material
                );
            }
            let b = netsmith_3d::mesh::bounds_of(
                &mine.iter().map(|m| (*m).clone()).collect::<Vec<_>>(),
            )
            .unwrap();
            // In plan: inside the body box and the pads, as the scene puts them.
            let placed = board.parts.iter().find(|p| p.path == path).unwrap();
            let mut env = body.bounds;
            for pad in &placed.footprint.pads {
                let r = placed.pad_bounds(pad);
                env.min.x = env.min.x.min(r.min.x - left);
                env.max.x = env.max.x.max(r.max.x - left);
                env.min.y = env.min.y.min(bottom - r.max.y);
                env.max.y = env.max.y.max(bottom - r.min.y);
            }
            let eps = 1e-6;
            assert!(
                b.min[0] >= env.min.x - eps
                    && b.max[0] <= env.max.x + eps
                    && b.min[1] >= env.min.y - eps
                    && b.max[1] <= env.max.y + eps,
                "{id} {path}: {b:?} outside {env:?}"
            );
            // The body box's middle is the shape's: nothing is turned the wrong way.
            let mid = |lo: f64, hi: f64| 0.5 * (lo + hi);
            assert!(
                (mid(b.min[0], b.max[0]) - mid(body.bounds.min.x, body.bounds.max.x)).abs() < 1e-6
            );
            assert!(
                (mid(b.min[1], b.max[1]) - mid(body.bounds.min.y, body.bounds.max.y)).abs() < 1e-6
            );
            // Turned a quarter, the part is longer along y than along x.
            assert!(
                b.max[1] - b.min[1] > b.max[0] - b.min[0],
                "{id} {path}: {b:?}"
            );
            // In z: from the face out to the height, a bottom part hanging down.
            let (lo, hi) = match body.side {
                Side::Back => (-1.6 - h, -1.6),
                _ => (0.0, h),
            };
            assert!(
                (b.min[2] - lo).abs() < 1e-9 && (b.max[2] - hi).abs() < 1e-9,
                "{id} {path}: {b:?}"
            );
            // The pin 1 dot is nearest pad 1, also mirrored on the bottom.
            if let Some(dot) = mine.iter().find(|m| m.material == Material::Mark) {
                let c = dot.bounds().unwrap().centre();
                let near = placed
                    .footprint
                    .pads
                    .iter()
                    .min_by(|p, q| {
                        let d = |pad: &netsmith_pcb::Pad| {
                            let at = placed.pad_centre(pad);
                            (at.x - left - c[0]).hypot(bottom - at.y - c[1])
                        };
                        d(p).total_cmp(&d(q))
                    })
                    .unwrap();
                assert_eq!(near.number, "1", "{id} {path}");
            }
        }
        // The file's footprint has no package record: still its box.
        let box_ = meshes.iter().filter(|m| m.id == "b").collect::<Vec<_>>();
        assert!(s.body("b").unwrap().shape.is_none());
        assert_eq!(box_.len(), 1);
        assert_eq!(box_[0].material, Material::Body);
        assert_eq!(box_[0].triangle_count(), 12);
        // The same board gives the same meshes.
        assert_eq!(meshes, of_scene(&scene(&shaped(id, Some(h)))));
        // To look at: `NETSMITH_RENDER3D_OUT=<folder>` writes the board from above and below.
        if let Some(out) = std::env::var_os("NETSMITH_RENDER3D_OUT") {
            use netsmith_3d::raster3d::{RenderOptions, View, render_view};
            std::fs::create_dir_all(&out).unwrap();
            let stem = id.split(':').nth(1).unwrap_or(id);
            for view in [View::Iso, View::IsoBottom] {
                let png = render_view(&meshes, view, &RenderOptions::default()).to_png();
                let file = format!("shaped-{stem}-{}.png", view.name());
                std::fs::write(std::path::Path::new(&out).join(file), png).unwrap();
            }
        }
    }
}

#[test]
fn a_shaped_footprint_of_unknown_height_is_still_its_flat_outline() {
    use netsmith_3d::mesh::{Material, of_scene};
    let s = scene(&shaped("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm", None));
    let meshes = of_scene(&s);
    for path in ["a", "under"] {
        assert!(s.body(path).unwrap().shape.is_none());
        let mine: Vec<_> = meshes.iter().filter(|m| m.id == path).collect();
        assert_eq!(mine.len(), 1, "{path}");
        assert_eq!(mine[0].material, Material::BodyUnknown);
        assert!(!mine[0].closed);
    }
}

/// The board of [`board`] with `row` (no height of its own) on a SOT-23, whose package standard
/// allows 1.12 mm (JEDEC TO-236, ADR 0106), and `a` (stated) and `b` (catalog) on a SOIC-8, whose
/// standard they do not take.
fn board_with_standard_heights() -> Board {
    let mut board = board();
    for (path, id) in [
        ("a", "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm"),
        ("b", "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm"),
        ("row", "Package_TO_SOT_SMD:SOT-23"),
    ] {
        board
            .parts
            .iter_mut()
            .find(|p| p.path == path)
            .unwrap()
            .library_id = id.into();
    }
    // `under` keeps its box footprint and loses its height: it is unknown.
    board
        .parts
        .iter_mut()
        .find(|p| p.path == "under")
        .unwrap()
        .height = None;
    board
}

#[test]
fn stated_beats_catalog_beats_standard_beats_unknown() {
    let board = board_with_standard_heights();
    let s = scene(&board);
    let a = s.body("a").unwrap();
    assert_eq!(
        (a.provenance, a.height, a.standard),
        (HeightProvenance::Stated, Some(2.0), None)
    );
    let b = s.body("b").unwrap();
    assert_eq!(
        (b.provenance, b.height, b.standard),
        (HeightProvenance::Catalog, Some(3.0), None)
    );
    let row = s.body("row").unwrap();
    assert_eq!(
        (row.provenance, row.height),
        (HeightProvenance::Standard, Some(1.12))
    );
    assert_eq!(row.z_range(), (Some(0.0), Some(1.12)));
    assert_eq!(
        row.standard.map(|s| s.cited()).as_deref(),
        Some("JEDEC TO-236, drawing TI DBZ0003A")
    );
    let under = s.body("under").unwrap();
    assert_eq!(
        (under.provenance, under.height),
        (HeightProvenance::Unknown, None)
    );
    let c = s.coverage();
    assert_eq!(
        (c.known, c.total, c.stated, c.catalog, c.standard),
        (3, 4, 1, 1, 1)
    );
    assert!(HeightProvenance::Stated < HeightProvenance::Catalog);
    assert!(HeightProvenance::Catalog < HeightProvenance::Standard);
    assert!(HeightProvenance::Standard < HeightProvenance::Unknown);
    assert_eq!(HeightProvenance::Standard.word(), "standard");
}

#[test]
fn a_standard_height_that_fits_a_limit_is_a_pass_said_to_be_the_standards() {
    let mut board = board_with_standard_heights();
    board.keepouts = vec![limit("lid", (20.0, 8.0), (10.0, 10.0), &["top"], 1.5)];
    assert_eq!(board.heights_unchecked(), []);
    let passes = board.standard_passes();
    assert_eq!(passes.len(), 1, "{passes:?}");
    assert_eq!(
        passes[0].to_string(),
        "`row` (J1) fits the 1.50 mm the keepout `lid` allows on the top face by its package standard (JEDEC TO-236, drawing TI DBZ0003A, at most 1.12 mm), not by a height of its own: the design states none and its catalog row has none"
    );
    assert!(
        !board
            .check_placement()
            .iter()
            .any(|p| matches!(p, netsmith_pcb::PlacementProblem::TooTall { .. }))
    );
}

#[test]
fn a_standard_height_over_a_limit_leaves_the_part_unchecked_and_never_too_tall() {
    let mut board = board_with_standard_heights();
    board.keepouts = vec![limit("lid", (20.0, 8.0), (10.0, 10.0), &["top"], 1.0)];
    assert_eq!(board.standard_passes(), []);
    let unchecked = board.heights_unchecked();
    assert_eq!(unchecked.len(), 1, "{unchecked:?}");
    assert_eq!(
        unchecked[0].to_string(),
        "height of `row` (J1) is unchecked: the keepout `lid` limits parts on the top face to 1.00 mm, the part states no height, and its package standard (JEDEC TO-236) allows up to 1.12 mm, so a part in that package may or may not fit (say `.row.height &= 1mm`, with the real number from its datasheet)"
    );
    assert!(
        !board
            .check_placement()
            .iter()
            .any(|p| matches!(p, netsmith_pcb::PlacementProblem::TooTall { .. }))
    );
    // The scene names it as the board's check does.
    let s = scene(&board);
    assert_eq!(s.findings.unchecked.len(), 1);
    assert!(s.findings.too_tall.is_empty());
}
