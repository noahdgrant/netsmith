use std::collections::BTreeMap;

use super::*;
use crate::scene3d::{
    Findings, HeightProvenance, Hole, HoleOwner, Placement, PlanBox, SceneCutout, SceneLayer,
};
use netsmith_pcb::{BodyMaterial as Made, BodySolid as Made3d, Pt, Side};

fn plan(x: f64, y: f64) -> Plan {
    Plan { x, y }
}

fn body(path: &str, refdes: &str, min: Plan, max: Plan, height: Option<f64>) -> Body {
    Body {
        path: path.into(),
        refdes: refdes.into(),
        kind: "Thing".into(),
        declared_kind: netsmith_testkit::std_kind("Thing")
            .and_then(netsmith_core::PartKind::from_word),
        category: crate::scene3d::PartCategory::of(None, refdes),
        side: Side::Front,
        layout_at: (0.0, 0.0),
        rotation: 0.0,
        corners: [min, plan(max.x, min.y), max, plan(min.x, max.y)],
        bounds: PlanBox { min, max },
        body_is_courtyard: false,
        z_face: 0.0,
        z_far: height,
        height,
        provenance: if height.is_some() {
            HeightProvenance::Stated
        } else {
            HeightProvenance::Unknown
        },
        standard: None,
        pins: Pins::None,
        shape: None,
        place: crate::scene3d::Placement {
            origin: plan(0.0, 0.0),
            ex: plan(1.0, 0.0),
            ey: plan(0.0, -1.0),
            z_face: 0.0,
            out: 1.0,
        },
        model: None,
    }
}

/// The solids of a small package in the footprint's frame (y down, z up from its face): a body
/// box, a lead across y, a pin and a ball.
fn package() -> Vec<Made3d> {
    vec![
        Made3d {
            material: Made::Body,
            primitive: BodyPrimitive::Box {
                min: [-1.0, -0.5, 0.0],
                max: [1.0, 0.5, 0.8],
            },
        },
        Made3d {
            material: Made::Metal,
            primitive: BodyPrimitive::Prism {
                axis: BodyAxis::Y,
                profile: vec![[1.0, 0.0], [1.4, 0.0], [1.4, 0.2], [1.0, 0.2]],
                from: -0.2,
                to: 0.2,
            },
        },
        Made3d {
            material: Made::Mark,
            primitive: BodyPrimitive::Cylinder {
                centre: Pt::new(-0.6, -0.2),
                radius: 0.1,
                z0: 0.7,
                z1: 0.8,
            },
        },
        Made3d {
            material: Made::Ball,
            primitive: BodyPrimitive::Spheroid {
                centre: Pt::new(0.0, 0.0),
                radius: 0.15,
                z0: -0.2,
                z1: 0.0,
            },
        },
    ]
}

/// A 40 x 20 mm board with rounded corners, a cutout, a round hole and a slot; a box `U1`, a
/// shaped part `Q1` on the bottom (turned and mirrored), and `J1` of unknown height.
pub(crate) fn scene() -> Scene {
    let mut u1 = body("u1", "U1", plan(10.0, 5.0), plan(14.0, 8.0), Some(2.0));
    u1.pins = Pins::Known {
        at: PlanBox {
            min: plan(10.5, 5.5),
            max: plan(13.5, 7.5),
        },
        length: 1.5,
        source: "a line".into(),
        z_top: -1.6,
        z_bottom: -3.1,
        tails: Vec::new(),
    };
    let mut q1 = body("q1", "Q1", plan(29.0, 4.0), plan(31.0, 6.0), Some(0.8));
    q1.side = Side::Back;
    q1.z_face = -1.6;
    q1.z_far = Some(-2.4);
    // Turned a quarter and mirrored for the bottom face: ex = (0, 1), ey = (1, 0).
    q1.shape = Some(PlacedShape {
        solids: package(),
        place: Placement {
            origin: plan(30.0, 5.0),
            ex: plan(0.0, 1.0),
            ey: plan(1.0, 0.0),
            z_face: -1.6,
            out: -1.0,
        },
    });
    let layer = |name: &str, kind, z_top: f64, z_bottom: f64| SceneLayer {
        name: name.into(),
        kind,
        thickness: z_top - z_bottom,
        z_top,
        z_bottom,
    };
    Scene {
        board: BoardSolid {
            name: "Demo".into(),
            width: 40.0,
            height: 20.0,
            corner_radius: 2.0,
            outline: Vec::new(),
            outline_edges: Vec::new(),
            thickness: 1.6,
            stack_thickness: 1.6,
            layers: vec![
                layer("F.Mask", LayerKind::Mask, 0.0, -0.01),
                layer("core", LayerKind::Core, -0.01, -1.59),
                layer("B.Mask", LayerKind::Mask, -1.59, -1.6),
            ],
            cutouts: vec![SceneCutout {
                name: "window".into(),
                at: PlanBox {
                    min: plan(18.0, 12.0),
                    max: plan(24.0, 16.0),
                },
                radius: 1.0,
            }],
            holes: vec![
                Hole {
                    owner: HoleOwner::Via { net: "GND".into() },
                    from: plan(35.0, 10.0),
                    to: plan(35.0, 10.0),
                    diameter: 0.4,
                    plated: true,
                },
                Hole {
                    owner: HoleOwner::Pad {
                        path: "u1".into(),
                        refdes: "U1".into(),
                        pad: "1".into(),
                    },
                    from: plan(5.0, 15.0),
                    to: plan(7.0, 15.0),
                    diameter: 1.0,
                    plated: false,
                },
            ],
        },
        copper: Ok(crate::scene3d::SceneCopper {
            layers: Vec::new(),
            vias: Vec::new(),
            mask: Vec::new(),
            silk: Vec::new(),
        }),
        bodies: vec![
            body("j1", "J1", plan(20.0, 5.0), plan(24.0, 9.0), None),
            q1,
            u1,
        ],
        limits: Vec::new(),
        findings: Findings::default(),
        warnings: Vec::new(),
        case: crate::case::CaseScene::default(),
    }
}

/// The DATA section as `id -> (entity, arguments)`; a complex entity's name is `(`.
fn entities(file: &str) -> BTreeMap<usize, (String, Vec<String>)> {
    let data = file
        .split("DATA;\n")
        .nth(1)
        .unwrap()
        .split("ENDSEC;")
        .next()
        .unwrap();
    let mut out = BTreeMap::new();
    for line in data.lines() {
        let (id, body) = line.split_once('=').unwrap();
        let id: usize = id.trim_start_matches('#').parse().unwrap();
        let body = body.trim_end_matches(';');
        let open = body.find('(').unwrap();
        let name = if open == 0 { "(" } else { &body[..open] };
        let args = if open == 0 {
            Vec::new()
        } else {
            split(&body[open + 1..body.len() - 1])
        };
        out.insert(id, (name.to_string(), args));
    }
    out
}

/// The arguments of an entity, split at the commas outside parentheses and quotes.
fn split(s: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut quoted) = (Vec::new(), String::new(), 0, false);
    for c in s.chars() {
        match c {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ',' if !quoted && depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out
}

fn id(s: &str) -> usize {
    s.trim_start_matches('#').parse().unwrap()
}

fn ids(s: &str) -> Vec<usize> {
    s.trim_matches(|c| c == '(' || c == ')')
        .split(',')
        .map(id)
        .collect()
}

/// Checks that every shell is closed and its loops are chained, and returns the number of
/// solids by name.
fn check_solids(file: &str) -> BTreeMap<String, usize> {
    let e = entities(file);
    let get = |i: usize, name: &str| -> &Vec<String> {
        let (n, a) = &e[&i];
        assert_eq!(n, name, "#{i}");
        a
    };
    let mut named = BTreeMap::new();
    for (n, a) in e.values() {
        if n != "MANIFOLD_SOLID_BREP" {
            continue;
        }
        *named
            .entry(a[0].trim_matches('\'').to_string())
            .or_insert(0) += 1;
        let shell = get(id(&a[1]), "CLOSED_SHELL");
        // Each edge once each way.
        let mut uses: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
        for face in ids(&shell[1]) {
            let face = get(face, "ADVANCED_FACE");
            for bound in ids(&face[1]) {
                let b = &e[&bound];
                assert!(b.0 == "FACE_OUTER_BOUND" || b.0 == "FACE_BOUND");
                let edges = ids(&get(id(&b.1[1]), "EDGE_LOOP")[1]);
                let mut ends = Vec::new();
                for oe in &edges {
                    let oe = get(*oe, "ORIENTED_EDGE");
                    let curve = id(&oe[3]);
                    let ec = get(curve, "EDGE_CURVE");
                    let (s, t) = (id(&ec[1]), id(&ec[2]));
                    let u = uses.entry(curve).or_default();
                    if oe[4] == ".T." {
                        u.0 += 1;
                        ends.push((s, t));
                    } else {
                        u.1 += 1;
                        ends.push((t, s));
                    }
                }
                for k in 0..ends.len() {
                    assert_eq!(ends[k].1, ends[(k + 1) % ends.len()].0, "a broken loop");
                }
            }
        }
        for (curve, u) in uses {
            assert_eq!(u, (1, 1), "edge #{curve} is not used once each way");
        }
    }
    named
}

/// FNV-1a of the bytes: a fingerprint that is the same on every platform and Rust release.
fn fingerprint(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[test]
fn a_rounded_rectangle_board_is_written_as_before_polygon_outlines() {
    // The bytes the writer gave this scene before a polygon outline was written as itself
    // (issue #543): a rounded rectangle's board did not change. A change of the writer that
    // moves them is reviewed in a STEP reader and this number updated.
    let file = step(&scene(), "Demo");
    assert_eq!(
        fingerprint(&file),
        12_552_831_323_492_200_493,
        "{} bytes",
        file.len()
    );
}

#[test]
fn the_same_scene_gives_the_same_bytes_and_a_fixed_header() {
    let (a, b) = (step(&scene(), "Demo"), step(&scene(), "Demo"));
    assert_eq!(a, b);
    assert!(a.starts_with("ISO-10303-21;\nHEADER;\n"));
    assert!(a.contains("FILE_NAME('Demo','1970-01-01T00:00:00'"));
    assert!(a.contains("FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));"));
    assert!(a.ends_with("ENDSEC;\nEND-ISO-10303-21;\n"));
    assert!(a.is_ascii());
}

#[test]
fn every_shell_is_closed_and_each_part_is_a_named_product() {
    let file = step(&scene(), "Demo");
    let solids = check_solids(&file);
    // The board; U1's box and pins; Q1's body, lead, mark and ball.
    let expected: BTreeMap<String, usize> = [
        ("board", 1),
        ("body", 2),
        ("metal", 2),
        ("mark", 1),
        ("pins", 1),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    assert_eq!(solids, expected);
    for p in [
        "PRODUCT('Demo','Demo',",
        "PRODUCT('board','board',",
        "PRODUCT('u1','U1','Thing; height stated'",
        "PRODUCT('q1','Q1','Thing; height stated'",
        "PRODUCT('j1','J1','Thing; height unknown: outline only'",
        "NEXT_ASSEMBLY_USAGE_OCCURRENCE('j1','J1',",
    ] {
        assert!(file.contains(p), "no {p}");
    }
}

#[test]
fn a_headers_stated_pins_are_one_closed_solid_each_where_the_tails_are() {
    // U1's pins as two tails of a header, 0.64 mm square, from 3.1 mm under its face to it, at
    // (1, 0) and (3, 0) of its footprint (y down): the scene's y is minus the footprint's.
    let mut s = scene();
    let u1 = s.bodies.iter_mut().find(|b| b.path == "u1").unwrap();
    u1.place.origin = plan(10.0, 6.0);
    let Pins::Known { tails, .. } = &mut u1.pins else {
        unreachable!()
    };
    *tails = [1.0, 3.0]
        .map(|x| Made3d {
            material: Made::Metal,
            primitive: BodyPrimitive::Box {
                min: [x - 0.32, -0.32, -3.1],
                max: [x + 0.32, 0.32, 0.0],
            },
        })
        .to_vec();
    let file = step(&s, "Demo");
    assert_eq!(check_solids(&file)["pins"], 2);
    for p in [
        "(10.68,5.68,-3.1)",
        "(11.32,6.32,0.)",
        "(12.68,5.68,-3.1)",
        "(13.32,6.32,0.)",
    ] {
        assert!(file.contains(&format!("CARTESIAN_POINT('',{p})")), "no {p}");
    }
}

#[test]
fn the_board_is_exact_with_its_round_corners_and_holes_as_cylinders() {
    let file = step(&scene(), "Demo");
    let e = entities(&file);
    let count = |name: &str| e.values().filter(|(n, _)| n == name).count();
    // The outline's four corners, the cutout's four, the round hole, the slot's two ends, and
    // the mark of Q1.
    assert_eq!(count("CYLINDRICAL_SURFACE"), 4 + 4 + 1 + 2 + 1);
    let board = e
        .values()
        .find(|(n, a)| n == "MANIFOLD_SOLID_BREP" && a[0] == "'board'")
        .unwrap();
    let shell = &e[&id(&board.1[1])];
    // 4 sides and 4 corners, the cutout's 8, the hole's 1, the slot's 4, and the two ends.
    assert_eq!(ids(&shell.1[1]).len(), 8 + 8 + 1 + 4 + 2);
    // The board runs through its thickness.
    assert!(file.contains("CARTESIAN_POINT('',(0.,0.,-1.6))"));
}

#[test]
fn a_part_of_unknown_height_is_an_outline_and_never_a_solid() {
    let file = step(&scene(), "Demo");
    assert!(file.contains("GEOMETRICALLY_BOUNDED_WIREFRAME_SHAPE_REPRESENTATION('J1',"));
    assert!(!file.contains("ADVANCED_BREP_SHAPE_REPRESENTATION('J1',"));
    assert!(file.contains("POLYLINE('outline',"));
    assert!(file.contains("CARTESIAN_POINT('',(20.,5.,0.))"));
}

#[test]
fn a_mirrored_part_is_turned_right_handed_and_stays_where_the_scene_puts_it() {
    let s = scene();
    let q1 = s.body("q1").unwrap();
    let solids = part_solids(q1);
    assert_eq!(solids.len(), 4);
    for (_, _, solid) in &solids {
        if let Solid::Extrusion(e) = solid {
            assert!(dot(cross(e.u, e.v), e.n) > 0.0);
            assert!(e.from < e.to);
            assert!(writer::signed_area(&e.loops[0]) > 0.0);
        }
    }
    // The body box, 2 x 1 x 0.8 in the footprint, hangs from the bottom face, turned a quarter;
    // it ends 0.02 mm short of the mark level with its top, as the 3D view draws it.
    let Solid::Extrusion(body) = &solids[0].2 else {
        panic!()
    };
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for s in &body.loops[0] {
        for t in [body.from, body.to] {
            let p = s.start();
            let q: Vec<f64> = (0..3)
                .map(|i| body.origin[i] + p[0] * body.u[i] + p[1] * body.v[i] + t * body.n[i])
                .collect();
            for i in 0..3 {
                lo[i] = lo[i].min(q[i]);
                hi[i] = hi[i].max(q[i]);
            }
        }
    }
    let close = |a: [f64; 3], b: [f64; 3]| (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9);
    assert!(close(lo, [29.5, 4.0, -2.38]), "{lo:?}");
    assert!(close(hi, [30.5, 6.0, -1.6]), "{hi:?}");
    // The ball's faces turn outward after the mirror: its volume is positive.
    let Solid::Polyhedron(ball) = &solids[3].2 else {
        panic!()
    };
    let volume: f64 = ball
        .faces
        .iter()
        .map(|f| {
            let a = ball.points[f[0]];
            (1..f.len() - 1)
                .map(|k| dot(a, cross(ball.points[f[k]], ball.points[f[k + 1]])))
                .sum::<f64>()
        })
        .sum();
    assert!(volume > 0.0);
}

#[test]
fn a_loop_runs_backward_with_its_arcs_turned() {
    let l = slot([0.0, 0.0], [2.0, 0.0], 1.0);
    assert!(writer::signed_area(&l) > 0.0);
    let r = writer::reversed(&l);
    assert!(writer::signed_area(&r) < 0.0);
    assert_eq!(writer::reversed(&r), l);
}

#[test]
fn reals_and_strings_are_written_as_step_wants_them() {
    assert_eq!(real(1.0), "1.");
    assert_eq!(real(-0.0), "0.");
    assert_eq!(real(-1e-12), "0.");
    assert_eq!(real(0.1 + 0.2), "0.3");
    assert_eq!(real(-2.5), "-2.5");
    assert_eq!(string("it's"), "'it''s'");
    assert_eq!(string("Ω"), "'\\X2\\03A9\\X0\\'");
}
