//! The reader's model against what netsmith's own writer writes and against the small files of
//! [`super::fixtures`].

use super::fixtures::{self, F, MM};
use super::geometry::{Curve, Surface};
use super::model::{Model, model};
use super::*;

fn model_of(text: &str) -> Model {
    let ex = part21::parse(text.as_bytes()).unwrap_or_else(|e| panic!("{e}\n{text}"));
    model(&ex).unwrap_or_else(|e| panic!("{e}\n{text}"))
}

fn surfaces(m: &Model) -> Vec<&'static str> {
    m.faces
        .iter()
        .map(|f| match f.surface {
            Surface::Plane(_) => "plane",
            Surface::Cylinder { .. } => "cylinder",
            Surface::Cone { .. } => "cone",
            Surface::Sphere { .. } => "sphere",
            Surface::Torus { .. } => "torus",
            Surface::BSpline(_) => "bspline",
        })
        .collect()
}

#[test]
fn the_writers_board_is_every_solid_it_wrote_with_its_colours() {
    let text = crate::step::step(&crate::step::tests::scene(), "Demo");
    let m = model_of(&text);
    assert_eq!(m.placed.len(), text.matches("MANIFOLD_SOLID_BREP(").count());
    assert_eq!(m.faces.len(), text.matches("ADVANCED_FACE(").count());
    let names: Vec<&str> = m.solids.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names[0], "board");
    assert!(
        names.contains(&"pins") && names.contains(&"metal"),
        "{names:?}"
    );
    for s in &m.solids {
        assert!(s.colour.is_some(), "{} is painted", s.name);
        assert_eq!(s.mm, 1.0);
        assert!(s.shells.iter().all(|sh| sh.closed));
    }
    let kinds = surfaces(&m);
    assert!(kinds.contains(&"plane") && kinds.contains(&"cylinder"));
    // Each part's placement is the identity, as the writer puts every solid where it is.
    for p in &m.placed {
        assert_eq!(p.place, model::Affine::IDENTITY);
    }
}

#[test]
fn each_kind_of_surface_and_curve_is_read() {
    assert_eq!(
        surfaces(&model_of(&fixtures::cylinder())),
        ["cylinder", "plane", "plane"]
    );
    assert_eq!(
        surfaces(&model_of(&fixtures::sphere())),
        ["sphere", "sphere"]
    );
    let cone = model_of(&fixtures::cone(1.0, 2.0));
    assert_eq!(surfaces(&cone), ["cone", "plane", "plane"]);
    match cone.faces[0].surface {
        Surface::Cone { r, tan, .. } => {
            assert_eq!(r, 1.0);
            assert!((tan - 0.3_f64.tan()).abs() < 1e-15);
        }
        _ => unreachable!(),
    }
    // The apex is a vertex loop.
    let pointed = model_of(&fixtures::cone(0.0, 2.0));
    assert!(matches!(pointed.faces[0].bounds[1].0, model::Loop::Vertex(p) if p == [0.0; 3]));
    let torus = model_of(&fixtures::torus());
    assert!(torus.faces[0].bounds.is_empty());
    assert!(matches!(
        torus.faces[0].surface,
        Surface::Torus {
            major: 3.0,
            minor: 1.0,
            ..
        }
    ));
    let patch = model_of(&fixtures::patch());
    assert!(!patch.solids[0].shells[0].closed);
    match &patch.faces[0].surface {
        Surface::BSpline(b) => {
            assert_eq!((b.u.degree, b.v.degree), (3, 3));
            assert_eq!(b.points, fixtures::patch_points());
            assert!(b.weights.is_none());
        }
        _ => unreachable!(),
    }
    assert!(
        patch
            .edges
            .iter()
            .all(|e| matches!(&e.curve, Curve::BSpline(b) if b.knots.degree == 3))
    );
}

#[test]
fn a_rational_b_spline_keeps_its_weights() {
    let m = model_of(&fixtures::rational_quarter());
    let w = std::f64::consts::FRAC_1_SQRT_2;
    match &m.faces[0].surface {
        Surface::BSpline(b) => {
            assert_eq!(b.weights.as_ref().unwrap()[1], [w, w]);
            assert_eq!(b.u.knots, [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
            // The middle of the arc is on the circle of radius 2.
            let (p, _, _) = b.eval(0.5, 0.0);
            assert!((p[0].hypot(p[1]) - 2.0).abs() < 1e-15);
        }
        _ => unreachable!(),
    }
    let arcs: Vec<_> = m
        .edges
        .iter()
        .filter_map(|e| match &e.curve {
            Curve::BSpline(b) => Some(b),
            _ => None,
        })
        .collect();
    assert_eq!(arcs.len(), 2);
    assert!(
        arcs.iter()
            .all(|b| b.weights.as_deref() == Some(&[1.0, w, 1.0][..]))
    );
}

#[test]
fn an_inch_file_is_a_unit_of_25_4_mm() {
    let m = model_of(&fixtures::inch_block());
    assert_eq!(m.solids[0].mm, 25.4);
}

#[test]
fn an_assembly_places_its_child_by_mapped_item_and_by_transformation() {
    let m = model_of(&fixtures::assembly());
    assert_eq!(m.solids.len(), 1, "one solid");
    assert_eq!(m.placed.len(), 2, "placed twice");
    let corner = [2.0, 1.0, 1.0];
    assert_eq!(m.placed[0].place.apply(corner), [2.0, 1.0, 6.0]);
    assert_eq!(m.placed[1].place.apply(corner), [9.0, 2.0, 1.0]);
}

#[test]
fn what_cannot_be_read_is_an_error_that_names_it() {
    let model_err = |text: &str| -> StepError {
        match part21::parse(text.as_bytes()) {
            Err(e) => e.into(),
            Ok(ex) => model(&ex).unwrap_err(),
        }
    };
    assert_eq!(
        model_err("solid x\nendsolid x\n"),
        StepError::Parse(ParseError::NotStep)
    );
    let good = fixtures::cylinder();
    // An entity the reader does not read, in a solid.
    let odd = good.replacen("CYLINDRICAL_SURFACE(", "SURFACE_OF_REVOLUTION(", 1);
    let e = model_err(&odd);
    match &e {
        StepError::Unsupported { entity, line, .. } => {
            assert_eq!(entity, "SURFACE_OF_REVOLUTION");
            assert!(*line > 0);
        }
        other => panic!("{other:?}"),
    }
    let message = e.to_string();
    assert!(
        message.contains("SURFACE_OF_REVOLUTION") && message.starts_with("line "),
        "{message}"
    );
    // A reference to nothing.
    let at = good.find("EDGE_CURVE('',").unwrap();
    let mut broken = good.clone();
    broken.replace_range(at..at + 14, "EDGE_CURVE('',#99999,");
    assert!(matches!(
        model_err(&broken),
        StepError::Missing { id: 99999, .. }
    ));
    // Parameters that are not what the standard says.
    let wrong = good.replacen("CYLINDRICAL_SURFACE('',", "CYLINDRICAL_SURFACE('',.T.,", 1);
    assert!(matches!(model_err(&wrong), StepError::Malformed { .. }));
    // No solid.
    let none = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=CARTESIAN_POINT('',(0.,0.,0.));\nENDSEC;\nEND-ISO-10303-21;\n";
    assert_eq!(model_err(none), StepError::NoSolid);
}

#[test]
fn malformed_input_is_an_error_and_never_a_panic() {
    for text in [
        fixtures::rational_quarter(),
        fixtures::cone(0.0, 1.0),
        fixtures::assembly(),
    ] {
        let bytes = text.as_bytes();
        let read_model = |b: &[u8]| {
            part21::parse(b)
                .map_err(StepError::from)
                .and_then(|ex| model(&ex))
        };
        // Truncated anywhere, short of the last line's end, which the grammar does not need.
        for n in (0..bytes.len() - 1).step_by(7) {
            assert!(read_model(&bytes[..n]).is_err(), "cut at {n}");
        }
        // Bytes changed to digits, references and punctuation, from a fixed seed.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let swaps = b"0123456789#(),.;'$*-E";
        for _ in 0..1500 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut b = bytes.to_vec();
            let at = (seed % b.len() as u64) as usize;
            b[at] = swaps[((seed >> 32) % swaps.len() as u64) as usize];
            let _ = read_model(&b);
        }
    }
}

#[test]
fn the_writers_own_helpers_build_a_file_the_reader_reads() {
    // The fixture builder itself: a cylinder at a unit of millimetres.
    let mut f = F::new();
    let s = f.cylinder(1.0, 1.0);
    let m = model_of(&f.file(&[s], MM));
    assert_eq!(m.solids[0].name, "cylinder");
}
