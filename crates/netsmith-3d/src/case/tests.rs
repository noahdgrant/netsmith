//! The fit check on boxes written by hand: a board 40 by 24 mm, 1.6 mm thick, in a box whose
//! walls stand 0.3 mm off its edges, with a lid at 8 mm and bosses under its holes.

use super::*;
use crate::mesh::{Material, Mesh};
use crate::scene3d::{Hole, Pins};
use crate::step::read::Piece;
use netsmith_pcb::standard_height::StandardHeight;

const W: f64 = 40.0;
const H: f64 = 24.0;
const T: f64 = 1.6;

/// The twelve triangles of the box from `a` to `b`, facing out.
fn cube_triangles(a: [f64; 3], b: [f64; 3]) -> Vec<[[f64; 3]; 3]> {
    let p = |x: usize, y: usize, z: usize| {
        [
            if x == 0 { a[0] } else { b[0] },
            if y == 0 { a[1] } else { b[1] },
            if z == 0 { a[2] } else { b[2] },
        ]
    };
    let quads = [
        [p(0, 0, 0), p(0, 1, 0), p(1, 1, 0), p(1, 0, 0)],
        [p(0, 0, 1), p(1, 0, 1), p(1, 1, 1), p(0, 1, 1)],
        [p(0, 0, 0), p(1, 0, 0), p(1, 0, 1), p(0, 0, 1)],
        [p(0, 1, 0), p(0, 1, 1), p(1, 1, 1), p(1, 1, 0)],
        [p(0, 0, 0), p(0, 0, 1), p(0, 1, 1), p(0, 1, 0)],
        [p(1, 0, 0), p(1, 1, 0), p(1, 1, 1), p(1, 0, 1)],
    ];
    quads
        .iter()
        .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
        .collect()
}

fn cube(name: &str, a: [f64; 3], b: [f64; 3]) -> CaseSolid {
    CaseSolid::new(name, cube_triangles(a, b), true)
}

fn case(name: &str, solids: Vec<CaseSolid>) -> PlacedCase {
    PlacedCase {
        name: name.into(),
        file: format!("case/{name}.step"),
        sha256: String::new(),
        figures: CaseFigures::default(),
        solids,
    }
}

/// The box's floor and four walls, 0.3 mm off the board's edges, 2 mm thick, up to the lid.
fn walls() -> Vec<CaseSolid> {
    vec![
        cube("floor", [-2.3, -2.3, -8.0], [W + 2.3, H + 2.3, -6.0]),
        cube("wall_left", [-2.3, -2.3, -6.0], [-0.3, H + 2.3, 8.0]),
        cube("wall_right", [W + 0.3, -2.3, -6.0], [W + 2.3, H + 2.3, 8.0]),
        cube("wall_bottom", [-0.3, -2.3, -6.0], [W + 0.3, -0.3, 8.0]),
        cube("wall_top", [-0.3, H + 0.3, -6.0], [W + 0.3, H + 2.3, 8.0]),
    ]
}

fn lid() -> PlacedCase {
    case(
        "lid",
        vec![cube("lid", [-2.3, -2.3, 8.0], [W + 2.3, H + 2.3, 10.0])],
    )
}

fn board(holes: Vec<Hole>) -> BoardSolid {
    BoardSolid {
        name: "B".into(),
        width: W,
        height: H,
        corner_radius: 0.0,
        thickness: T,
        stack_thickness: T,
        layers: Vec::new(),
        cutouts: Vec::new(),
        holes,
        outline: Vec::new(),
    }
}

fn hole(path: &str, x: f64, y: f64) -> Hole {
    Hole {
        owner: HoleOwner::Pad {
            path: path.into(),
            refdes: path.to_uppercase(),
            pad: String::new(),
        },
        from: Plan { x, y },
        to: Plan { x, y },
        diameter: 3.2,
        plated: false,
    }
}

/// A body on the top face from `min` to `max` in plan, `height` tall (unknown for `None`).
fn body(path: &str, min: [f64; 2], max: [f64; 2], height: Option<f64>) -> Body {
    let corners = [
        Plan {
            x: min[0],
            y: min[1],
        },
        Plan {
            x: max[0],
            y: min[1],
        },
        Plan {
            x: max[0],
            y: max[1],
        },
        Plan {
            x: min[0],
            y: max[1],
        },
    ];
    Body {
        path: path.into(),
        refdes: format!("U_{path}"),
        kind: "Thing".into(),
        category: PartCategory::Other,
        side: Side::Front,
        layout_at: (0.0, 0.0),
        rotation: 0.0,
        corners,
        bounds: PlanBox {
            min: corners[0],
            max: corners[2],
        },
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
            origin: corners[0],
            ex: Plan { x: 1.0, y: 0.0 },
            ey: Plan { x: 0.0, y: -1.0 },
            z_face: 0.0,
            out: 1.0,
        },
        model: None,
    }
}

use crate::scene3d::PlanBox;

fn kinds(found: &[FitFinding]) -> Vec<(FitKind, String, String)> {
    found
        .iter()
        .map(|f| {
            (
                f.kind,
                f.path().unwrap_or("board").to_string(),
                f.solid.clone(),
            )
        })
        .collect()
}

#[test]
fn a_part_taller_than_the_lid_collides_by_how_far_it_goes_in() {
    let tall = body("tall", [10.0, 10.0], [12.0, 14.0], Some(9.0));
    let found = check(&board(vec![]), &[tall], &[lid()]);
    assert_eq!(
        kinds(&found),
        [(FitKind::Collision, "tall".into(), "lid".into())]
    );
    let f = &found[0];
    assert!(
        matches!(f.why, Why::Collision { depth: Some(d), .. } if (d - 1.0).abs() < 1e-9),
        "{f:?}"
    );
    assert_eq!(f.weight(), Weight::Error);
    assert_eq!(f.at[2], 8.0, "the deepest point is the lid's underside");
    let text = f.to_string();
    assert!(
        text.starts_with("U_tall (`tall`), 9.00 mm tall (stated), is inside the solid `lid` of the case model `lid` by 1.00 mm"),
        "{text}"
    );
}

#[test]
fn touching_is_contact_and_a_board_on_its_bosses_is_fine() {
    let base = case(
        "base",
        vec![
            cube("boss", [3.0, 3.0, -6.0], [7.0, 7.0, -T]),
            cube("floor", [-2.3, -2.3, -8.0], [W + 2.3, H + 2.3, -6.0]),
        ],
    );
    // A part that stands exactly up to the lid, and one 0.005 mm into it: both contact.
    let flush = body("flush", [10.0, 10.0], [12.0, 14.0], Some(8.0));
    let kiss = body("kiss", [20.0, 10.0], [22.0, 14.0], Some(8.005));
    let found = check(
        &board(vec![hole("h1", 5.0, 5.0)]),
        &[flush, kiss],
        &[base, lid()],
    );
    assert!(
        found
            .iter()
            .all(|f| f.kind != FitKind::Collision && f.kind != FitKind::Boss),
        "{found:?}"
    );
}

#[test]
fn a_boss_too_high_collides_with_the_board_and_a_pin_through_a_hole_does_not() {
    let base = case(
        "base",
        vec![
            cube("boss", [3.0, 3.0, -6.0], [7.0, 7.0, -1.0]),
            // A locating pin through the second hole, standing clear of its 3.2 mm drill.
            cube("pin", [34.5, 4.5, -6.0], [35.5, 5.5, 2.0]),
        ],
    );
    let found = check(
        &board(vec![hole("h1", 5.0, 5.0), hole("h2", 35.0, 5.0)]),
        &[],
        &[base],
    );
    assert_eq!(
        kinds(&found),
        [(FitKind::Collision, "board".into(), "boss".into())]
    );
    assert!(
        matches!(found[0].why, Why::Collision { depth: Some(d), .. } if (d - 0.6).abs() < 1e-9),
        "{:?}",
        found[0]
    );
    assert_eq!(found[0].weight(), Weight::Error);
    assert!(
        found[0].to_string().starts_with(
            "the board is inside the solid `boss` of the case model `base` by 0.60 mm"
        )
    );
}

#[test]
fn a_concave_outline_is_measured_by_its_material_not_its_box() {
    // An L: the board without its top-right quarter, where a post stands up through the notch.
    let mut l = board(vec![]);
    l.outline = [
        (0.0, 0.0),
        (W, 0.0),
        (W, 12.0),
        (20.0, 12.0),
        (20.0, H),
        (0.0, H),
    ]
    .map(|(x, y)| Plan { x, y })
    .to_vec();
    let post = || cube("post", [25.0, 15.0, -6.0], [35.0, 20.0, 8.0]);
    assert!(check(&l, &[], &[case("base", vec![post()])]).is_empty());
    // A boss 0.5 mm into the board's bottom under the L's corner, across a diagonal of the cut.
    let boss = cube("boss", [10.0, 6.0, -6.0], [30.0, 18.0, -T + 0.5]);
    let found = check(&l, &[], &[case("base", vec![boss])]);
    assert_eq!(
        kinds(&found),
        [(FitKind::Collision, "board".into(), "boss".into())]
    );
    assert!(
        matches!(found[0].why, Why::Collision { depth: Some(d), .. } if (d - 0.5).abs() < 1e-9),
        "{:?}",
        found[0]
    );
}

#[test]
fn a_part_near_a_wall_is_named_with_its_gap() {
    let near = body("near", [38.0, 10.0], [W - 0.1, 14.0], Some(2.0));
    let far = body("far", [20.0, 10.0], [22.0, 14.0], Some(2.0));
    let found = check(&board(vec![]), &[near, far], &[case("base", walls())]);
    assert_eq!(
        kinds(&found),
        [(FitKind::Clearance, "near".into(), "wall_right".into())]
    );
    match found[0].why {
        Why::Clearance { gap, least } => {
            assert!((gap - 0.4).abs() < 1e-4, "{gap}");
            assert_eq!(least, 0.5);
        }
        ref other => panic!("{other:?}"),
    }
    assert_eq!(found[0].weight(), Weight::Warning);
    // A narrower clearance lets it be.
    let mut base = case("base", walls());
    base.figures.clearance = 0.3;
    let near = body("near", [38.0, 10.0], [W - 0.1, 14.0], Some(2.0));
    assert!(check(&board(vec![]), &[near], &[base]).is_empty());
}

#[test]
fn a_part_wholly_inside_a_solid_collides_unless_the_shell_is_open() {
    // A block round the board and the part, as a case placed far off would be.
    let block = || cube_triangles([-5.0, -5.0, -3.0], [W + 5.0, H + 5.0, 20.0]);
    let inner = || body("inner", [10.0, 10.0], [12.0, 14.0], Some(3.0));
    let found = check(
        &board(vec![]),
        &[inner()],
        &[case("block", vec![CaseSolid::new("block", block(), true)])],
    );
    assert_eq!(
        kinds(&found),
        [
            (FitKind::Collision, "board".into(), "block".into()),
            (FitKind::Collision, "inner".into(), "block".into())
        ]
    );
    assert!(matches!(found[1].why, Why::Collision { depth: None, .. }));
    assert!(found[1].to_string().contains("lies wholly inside"));

    // An open shell gets no test of the inside, and says so; nothing is guessed.
    let found = check(
        &board(vec![]),
        &[inner()],
        &[case("block", vec![CaseSolid::new("block", block(), false)])],
    );
    assert_eq!(
        kinds(&found),
        [(FitKind::Unchecked, "board".into(), "block".into())]
    );
    assert_eq!(found[0].why, Why::OpenShell);
    assert_eq!(found[0].weight(), Weight::Unchecked);
    assert_eq!(found[0].severity(false), "warning");
    assert_eq!(found[0].severity(true), "error");
    assert!(found[0].to_string().contains("is not a closed shell"));
}

#[test]
fn a_part_of_unknown_height_under_the_lid_is_unchecked() {
    let bare = body("bare", [10.0, 10.0], [12.0, 14.0], None);
    let found = check(&board(vec![]), &[bare], &[case("base", walls()), lid()]);
    assert_eq!(
        kinds(&found),
        [(FitKind::Unchecked, "bare".into(), "lid".into())]
    );
    assert_eq!(found[0].why, Why::UnknownHeight);
    // With no lid, nothing is over it.
    let bare = body("bare", [10.0, 10.0], [12.0, 14.0], None);
    assert!(check(&board(vec![]), &[bare], &[case("base", walls())]).is_empty());
}

#[test]
fn a_standard_height_that_collides_is_unchecked_not_an_error() {
    let mut chip = body("chip", [10.0, 10.0], [12.0, 14.0], Some(9.0));
    chip.provenance = HeightProvenance::Standard;
    chip.standard = Some(StandardHeight {
        standard: "JEDEC MS-012",
        max: 9.0,
        drawing: "TI D0008A",
    });
    let found = check(&board(vec![]), &[chip], &[lid()]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, FitKind::Unchecked);
    assert!(matches!(found[0].why, Why::Standard { depth: Some(_), .. }));
    assert_eq!(found[0].weight(), Weight::Unchecked);
    assert!(
        found[0]
            .to_string()
            .contains("JEDEC MS-012, drawing TI D0008A")
    );
}

fn connector(min: [f64; 2], max: [f64; 2]) -> Body {
    let mut b = body("usb", min, max, Some(3.0));
    b.category = PartCategory::Connector;
    b
}

#[test]
fn a_connector_behind_a_wall_with_no_opening_cannot_be_reached() {
    let usb = connector([36.0, 10.0], [W, 14.0]);
    let found = check(&board(vec![]), &[usb], &[case("base", walls())]);
    let reach: Vec<&FitFinding> = found.iter().filter(|f| f.kind == FitKind::Reach).collect();
    assert_eq!(reach.len(), 1, "{found:?}");
    assert_eq!(
        reach[0].why,
        Why::ReachBlocked {
            edge: "right",
            distance: 0.3
        }
    );
    assert_eq!(reach[0].solid, "wall_right");
}

/// The right wall, `thick` mm thick, with an opening at y 9 to 15 and z -1 to 4.
fn wall_with_opening(thick: f64) -> Vec<CaseSolid> {
    let (x0, x1) = (W + 0.3, W + 0.3 + thick);
    vec![
        cube("wall_low", [x0, -2.3, -6.0], [x1, H + 2.3, -1.0]),
        cube("wall_high", [x0, -2.3, 4.0], [x1, H + 2.3, 8.0]),
        cube("wall_front", [x0, -2.3, -1.0], [x1, 9.0, 4.0]),
        cube("wall_back", [x0, 15.0, -1.0], [x1, H + 2.3, 4.0]),
    ]
}

#[test]
fn a_connector_too_deep_inside_its_opening_is_named_and_one_flush_enough_is_not() {
    let usb = || connector([36.0, 10.0], [W, 14.0]);
    let found = check(
        &board(vec![]),
        &[usb()],
        &[case("base", wall_with_opening(2.0))],
    );
    let reach: Vec<&FitFinding> = found.iter().filter(|f| f.kind == FitKind::Reach).collect();
    assert_eq!(reach.len(), 1, "{found:?}");
    assert_eq!(
        reach[0].why,
        Why::ReachDeep {
            edge: "right",
            depth: 2.3,
            most: 1.5
        }
    );
    assert!(reach[0].to_string().contains("stands 2.30 mm inside"));
    let found = check(
        &board(vec![]),
        &[usb()],
        &[case("base", wall_with_opening(0.8))],
    );
    assert!(found.iter().all(|f| f.kind != FitKind::Reach), "{found:?}");
}

#[test]
fn a_mounting_hole_with_no_boss_is_named() {
    let mut h1 = body("h1", [3.4, 3.4], [6.6, 6.6], Some(0.0));
    h1.kind = "MountingHole".into();
    let mut h2 = body("h2", [33.4, 3.4], [36.6, 6.6], Some(0.0));
    h2.kind = "MountingHole".into();
    let base = case(
        "base",
        vec![cube("boss", [3.0, 3.0, -6.0], [7.0, 7.0, -T - 0.2])],
    );
    let found = check(
        &board(vec![hole("h1", 5.0, 5.0), hole("h2", 35.0, 5.0)]),
        &[h1, h2],
        &[base, lid()],
    );
    assert_eq!(kinds(&found), [(FitKind::Boss, "h2".into(), String::new())]);
    assert_eq!(found[0].case, "base, lid");
    assert_eq!(found[0].weight(), Weight::Warning);
}

#[test]
fn a_case_model_is_turned_about_x_then_y_then_z_and_then_moved() {
    let (m, d) = placement([180.0, 0.0, 90.0], [1.0, 2.0, -1.6]);
    let apply =
        |p: [f64; 3]| [0, 1, 2].map(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + d[r]);
    // (1, 2, 3) about x by half a turn is (1, -2, -3), then a quarter about z is (2, 1, -3).
    let q = apply([1.0, 2.0, 3.0]);
    let want = [3.0, 3.0, -4.6];
    for k in 0..3 {
        assert!((q[k] - want[k]).abs() < 1e-12, "{q:?}");
    }
    let mut mesh = Mesh::new("lid", Material::Body);
    mesh.positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    mesh.indices = vec![0, 1, 2];
    let model = StepMesh {
        pieces: vec![
            Piece {
                colour: None,
                mesh: mesh.clone(),
            },
            // A second colour of the same solid joins it.
            Piece {
                colour: Some([255, 0, 0]),
                mesh,
            },
        ],
    };
    let placed = place(
        "lid",
        "case/lid.step",
        "00",
        CaseFigures::default(),
        &model,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 8.0],
    );
    assert_eq!(placed.solids.len(), 1);
    assert_eq!(placed.triangle_count(), 2);
    assert_eq!(placed.solids[0].triangles[0][1], [1.0, 0.0, 8.0]);
}
