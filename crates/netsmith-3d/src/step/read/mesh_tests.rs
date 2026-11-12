//! The reader's triangles against what netsmith's own writer writes and against the small files
//! of [`super::fixtures`]: closed shells, their volumes, the tolerance, the same bits each time.

use std::collections::BTreeMap;

use super::fixtures;
use super::geometry::{cross, dot, sub};
use super::*;

/// Whether every side of every triangle is met by a side of another running the other way.
fn closed(m: &Mesh) -> Result<(), String> {
    let key = |i: u32| m.positions[i as usize].map(f64::to_bits);
    let mut count: BTreeMap<([u64; 3], [u64; 3]), i64> = BTreeMap::new();
    for t in m.indices.as_chunks::<3>().0 {
        for k in 0..3 {
            *count.entry((key(t[k]), key(t[(k + 1) % 3]))).or_default() += 1;
        }
    }
    for (&(a, b), &n) in &count {
        let back = count.get(&(b, a)).copied().unwrap_or(0);
        if back != n {
            let p = a.map(f64::from_bits);
            let q = b.map(f64::from_bits);
            return Err(format!(
                "the side {p:?} -> {q:?} is used {n} times one way and {back} the other"
            ));
        }
    }
    Ok(())
}

/// The volume a closed mesh holds: positive when its triangles face out.
fn volume(m: &Mesh) -> f64 {
    m.indices
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| m.positions[i as usize]);
            dot(a, cross(b, c)) / 6.0
        })
        .sum()
}

fn total_volume(s: &StepMesh) -> f64 {
    s.pieces.iter().map(|p| volume(&p.mesh)).sum()
}

fn all_closed(s: &StepMesh) {
    // A solid's pieces of different colours close together.
    let mut by_solid: BTreeMap<String, Mesh> = BTreeMap::new();
    for p in &s.pieces {
        by_solid
            .entry(p.mesh.id.clone())
            .or_insert_with(|| Mesh::new(p.mesh.id.clone(), Material::Body))
            .append(&p.mesh);
    }
    for (id, m) in &by_solid {
        if let Err(e) = closed(m) {
            panic!("{id}: {e}");
        }
    }
}

fn bits(s: &StepMesh) -> Vec<u64> {
    let mut out = Vec::new();
    for p in &s.pieces {
        for q in &p.mesh.positions {
            out.extend(q.map(f64::to_bits));
        }
        for n in p.mesh.normals.iter().flatten() {
            out.extend(n.map(f64::to_bits));
        }
        out.extend(p.mesh.indices.iter().map(|&i| u64::from(i)));
    }
    out
}

// ---- A small file, entity by entity.
fn read_ok(text: &str) -> StepMesh {
    match read(text.as_bytes()) {
        Ok(m) => m,
        Err(e) => panic!("{e}\n{text}"),
    }
}

fn near(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs()
}

#[test]
fn a_cylinder_with_a_seam_is_closed_and_holds_its_volume() {
    let m = read_ok(&fixtures::cylinder());
    all_closed(&m);
    let v = total_volume(&m);
    let want = std::f64::consts::PI * 4.0 * 3.0;
    assert!(near(v, want, 0.01), "{v} against {want}");
    let b = m.bounds().unwrap();
    assert!(
        (b.min[0] + 2.0).abs() < 1e-6 && (b.max[2] - 3.0).abs() < 1e-9,
        "{b:?}"
    );
}

#[test]
fn a_sphere_of_two_halves_closes_at_its_poles() {
    let r = 2.0;
    let m = read_ok(&fixtures::sphere());
    all_closed(&m);
    let want = 4.0 / 3.0 * std::f64::consts::PI * r * r * r;
    assert!(near(total_volume(&m), want, 0.02), "{}", total_volume(&m));
    // Every point is on the sphere, every triangle's middle within the tolerance of it.
    for p in &m.pieces {
        for q in &p.mesh.positions {
            assert!((dot(*q, *q).sqrt() - r).abs() < 1e-6);
        }
        for t in p.mesh.indices.as_chunks::<3>().0 {
            let [a, b, c] = t.map(|i| p.mesh.positions[i as usize]);
            let mid = [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0);
            assert!(r - dot(mid, mid).sqrt() <= CHORD_TOLERANCE * 1.5);
        }
    }
    let b = m.bounds().unwrap();
    assert!(
        (b.max[2] - r).abs() < 1e-9 && (b.min[2] + r).abs() < 1e-9,
        "{b:?}"
    );
}

#[test]
fn a_cone_is_closed_as_a_frustum_and_up_from_its_apex() {
    let pi = std::f64::consts::PI;
    let (h, t) = (2.0, 0.3_f64.tan());
    let frustum = read_ok(&fixtures::cone(1.0, h));
    all_closed(&frustum);
    let r1 = 1.0 + h * t;
    let want = pi * h / 3.0 * (1.0 + 1.0 * r1 + r1 * r1);
    assert!(
        near(total_volume(&frustum), want, 0.01),
        "{} {want}",
        total_volume(&frustum)
    );
    let pointed = read_ok(&fixtures::cone(0.0, h));
    all_closed(&pointed);
    let r = h * t;
    let want = pi * r * r * h / 3.0;
    assert!(
        near(total_volume(&pointed), want, 0.02),
        "{} {want}",
        total_volume(&pointed)
    );
    assert_eq!(pointed.bounds().unwrap().min[2], 0.0, "down to the apex");
}

#[test]
fn a_whole_torus_with_no_loop_is_closed() {
    let m = read_ok(&fixtures::torus());
    all_closed(&m);
    let pi = std::f64::consts::PI;
    let want = 2.0 * pi * pi * 3.0 * 1.0;
    assert!(near(total_volume(&m), want, 0.02), "{}", total_volume(&m));
    let b = m.bounds().unwrap();
    assert!(
        (b.max[0] - 5.0).abs() < 1e-6 && (b.max[2] - 4.0).abs() < 1e-6,
        "{b:?}"
    );
}

#[test]
fn a_bicubic_patch_is_within_the_tolerance_of_its_surface() {
    let pts = fixtures::patch_points();
    let m = read_ok(&fixtures::patch());
    let mesh = &m.pieces[0].mesh;
    assert!(!mesh.closed);
    assert!(mesh.triangle_count() > 8, "{}", mesh.triangle_count());
    let b = geometry::BSplineSurface {
        u: geometry::Knots {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        },
        v: geometry::Knots {
            degree: 3,
            knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
        },
        points: pts,
        weights: None,
    };
    let s = geometry::Surface::BSpline(b);
    for t in mesh.indices.as_chunks::<3>().0 {
        let [a, b_, c] = t.map(|i| mesh.positions[i as usize]);
        for q in [a, b_, c] {
            let (uv, _) = s.param(q, None);
            assert!(geometry::dist(s.point(uv[0], uv[1]), q) < 1e-5, "{q:?}");
        }
        let mid = [0, 1, 2].map(|k| (a[k] + b_[k] + c[k]) / 3.0);
        let (uv, _) = s.param(mid, None);
        assert!(
            geometry::dist(s.point(uv[0], uv[1]), mid) <= CHORD_TOLERANCE * 1.5,
            "{mid:?}"
        );
    }
    // Upward, as the surface's normal points: its z part is positive on a gentle patch.
    let up: f64 = mesh.normals.as_ref().unwrap().iter().map(|n| n[2]).sum();
    assert!(up > 0.0);
}

#[test]
fn a_rational_b_spline_is_an_exact_round() {
    let m = read_ok(&fixtures::rational_quarter());
    let mesh = &m.pieces[0].mesh;
    assert!(mesh.triangle_count() >= 6);
    for q in &mesh.positions {
        let r = q[0].hypot(q[1]);
        assert!((r - 2.0).abs() < 2e-6, "{q:?} is {r} from the axis");
    }
    for t in mesh.indices.as_chunks::<3>().0 {
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        let mid = [0, 1, 2].map(|k| (a[k] + b[k] + c[k]) / 3.0);
        assert!(2.0 - mid[0].hypot(mid[1]) <= CHORD_TOLERANCE * 1.5);
        // Out, away from the axis.
        let n = cross(sub(b, a), sub(c, a));
        assert!(
            n[0] * mid[0] + n[1] * mid[1] > 0.0,
            "{a:?} {b:?} {c:?} {:?}",
            mesh.normals.as_ref().unwrap()[t[0] as usize]
        );
    }
}

#[test]
fn an_inch_file_is_read_in_mm() {
    let m = read_ok(&fixtures::inch_block());
    all_closed(&m);
    let b = m.bounds().unwrap();
    assert_eq!(b.max, [25.4, 12.7, 6.35]);
    assert!(near(total_volume(&m), 25.4 * 12.7 * 6.35, 1e-9));
}

#[test]
fn an_assembly_places_its_children_by_transformation_and_by_mapped_item() {
    let m = read_ok(&fixtures::assembly());
    assert_eq!(m.pieces.len(), 2, "the child twice");
    all_closed(&m);
    let raised = m.pieces[0].mesh.bounds().unwrap();
    assert_eq!((raised.min, raised.max), ([0.0, 0.0, 5.0], [2.0, 1.0, 6.0]));
    let turned = m.pieces[1].mesh.bounds().unwrap();
    assert_eq!(
        (turned.min, turned.max),
        ([9.0, 0.0, 0.0], [10.0, 2.0, 1.0])
    );
    assert!(volume(&m.pieces[1].mesh) > 0.0);
}

// ---- What netsmith writes.

#[test]
fn the_writers_board_reads_back_closed_with_its_box_and_colours() {
    let scene = crate::step::tests::scene();
    let text = crate::step::step(&scene, "Demo");
    let m = read_ok(&text);
    all_closed(&m);
    for p in &m.pieces {
        assert!(p.mesh.closed, "{}", p.mesh.id);
        assert!(volume(&p.mesh) > 0.0, "{} faces out", p.mesh.id);
        assert!(p.colour.is_some(), "{} is painted", p.mesh.id);
    }
    let b = m.bounds().unwrap();
    assert_eq!(b.min, [0.0, 0.0, -3.1]);
    assert_eq!(b.max, [40.0, 20.0, 2.0]);
    let names: Vec<&str> = m.pieces.iter().map(|p| p.mesh.id.as_str()).collect();
    assert!(
        names.contains(&"board") && names.contains(&"pins"),
        "{names:?}"
    );
    // The board: 40 x 20 x 1.6 less its round corners, the cutout and the holes.
    let board = m.pieces.iter().find(|p| p.mesh.id == "board").unwrap();
    let pi = std::f64::consts::PI;
    let area = 40.0 * 20.0 - (4.0 - pi) * 4.0 - (24.0 - (4.0 - pi)) - pi * 0.04 - (2.0 + pi * 0.25);
    assert!(
        near(volume(&board.mesh), area * 1.6, 0.002),
        "{}",
        volume(&board.mesh)
    );
}

#[test]
fn the_same_bytes_give_the_same_triangles() {
    for text in [
        crate::step::step(&crate::step::tests::scene(), "Demo"),
        fixtures::rational_quarter(),
        fixtures::cone(1.0, 2.0),
    ] {
        let (a, b) = (read_ok(&text), read_ok(&text));
        assert_eq!(bits(&a), bits(&b));
        assert!(!bits(&a).is_empty());
    }
}

// ---- What is not right.

#[test]
fn read_names_what_it_cannot_read_and_a_face_it_cannot_draw() {
    assert_eq!(
        read(b"solid x\nendsolid x\n"),
        Err(StepError::Parse(ParseError::NotStep))
    );
    let good = fixtures::cylinder();
    // An entity the reader does not read, in a solid.
    let odd = good.replacen("CYLINDRICAL_SURFACE(", "SURFACE_OF_REVOLUTION(", 1);
    match read(odd.as_bytes()) {
        Err(StepError::Unsupported { entity, line, .. }) => {
            assert_eq!(entity, "SURFACE_OF_REVOLUTION");
            assert!(line > 0);
        }
        other => panic!("{other:?}"),
    }
    let message = read(odd.as_bytes()).unwrap_err().to_string();
    assert!(
        message.contains("SURFACE_OF_REVOLUTION") && message.starts_with("line "),
        "{message}"
    );
    // A reference to nothing.
    let at = good.find("EDGE_CURVE('',").unwrap();
    let mut broken = good.clone();
    broken.replace_range(at..at + 14, "EDGE_CURVE('',#99999,");
    match read(broken.as_bytes()) {
        Err(StepError::Missing { id: 99999, .. }) => {}
        other => panic!("{other:?}"),
    }
    // No solid.
    let none = "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=CARTESIAN_POINT('',(0.,0.,0.));\nENDSEC;\nEND-ISO-10303-21;\n";
    assert_eq!(read(none.as_bytes()), Err(StepError::NoSolid));
    // A cylinder's side bounded by one loop round it: a face that cannot be drawn, named.
    let (text, face) = fixtures::loop_round_a_cylinder();
    match read(text.as_bytes()) {
        Err(StepError::Face { id, line, message }) => {
            assert_eq!(id, face);
            assert!(line > 0 && message.contains("no pole"), "{message}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn malformed_input_is_an_error_and_never_a_panic() {
    let texts = [fixtures::rational_quarter(), fixtures::cone(0.0, 1.0)];
    for text in &texts {
        let bytes = text.as_bytes();
        // Truncated anywhere.
        // (Short of the last line's end, which the grammar does not need.)
        for n in (0..bytes.len() - 1).step_by(7) {
            assert!(read(&bytes[..n]).is_err(), "cut at {n}");
        }
        // Bytes changed: digits, references and names, from a fixed seed.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let swaps = b"0123456789#(),.;'$*-E";
        for _ in 0..1500 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let mut b = bytes.to_vec();
            let at = (seed % b.len() as u64) as usize;
            b[at] = swaps[((seed >> 32) % swaps.len() as u64) as usize];
            let _ = read(&b);
        }
    }
}

/// Reads every `.step` file of the folder `NETSMITH_STEP_CORPUS` (models whose licence keeps
/// them out of the repository) and prints the time, faces and triangles of each. Run it in
/// release: `NETSMITH_STEP_CORPUS=<dir> cargo test --release -p netsmith-3d --lib
/// local_corpus -- --ignored --nocapture`.
#[test]
#[ignore = "reads models from NETSMITH_STEP_CORPUS"]
fn local_corpus() {
    let dir = std::env::var("NETSMITH_STEP_CORPUS").expect("NETSMITH_STEP_CORPUS names a folder");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("step") || e.eq_ignore_ascii_case("stp"))
        })
        .collect();
    files.sort();
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let start = std::time::Instant::now();
        let parsed = part21::parse(&bytes).unwrap();
        let parse_ms = start.elapsed().as_secs_f64() * 1e3;
        let model = model::model(&parsed).unwrap();
        let mesh = mesh(&model);
        let total_ms = start.elapsed().as_secs_f64() * 1e3;
        let name = path.file_name().unwrap().to_string_lossy();
        match mesh {
            Ok(m) => {
                let again = read(&bytes).unwrap();
                let open: usize = {
                    let mut by_solid: BTreeMap<String, Mesh> = BTreeMap::new();
                    for p in &m.pieces {
                        by_solid
                            .entry(p.mesh.id.clone())
                            .or_insert_with(|| Mesh::new(p.mesh.id.clone(), Material::Body))
                            .append(&p.mesh);
                    }
                    by_solid.values().filter(|s| closed(s).is_err()).count()
                };
                let b = m.bounds().unwrap();
                // Each solid's volume, its colours' pieces together.
                let mut vols: Vec<f64> = Vec::new();
                let mut last = None;
                for p in &m.pieces {
                    if last != Some(&p.mesh.id) {
                        vols.push(0.0);
                        last = Some(&p.mesh.id);
                    }
                    *vols.last_mut().unwrap() += volume(&p.mesh);
                }
                vols.sort_by(f64::total_cmp);
                println!(
                    "  volumes, total {:.5}: {}",
                    vols.iter().sum::<f64>(),
                    vols.iter()
                        .map(|x| format!("{x:.5}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                println!(
                    "{name}: {:.0} kB, {} faces, {} solids, parse {parse_ms:.1} ms, total {total_ms:.1} ms, {} triangles, {} pieces, {open} solids not closed, box {:?} .. {:?}, same twice {}",
                    bytes.len() as f64 / 1024.0,
                    model.faces.len(),
                    model.placed.len(),
                    m.triangle_count(),
                    m.pieces.len(),
                    b.min,
                    b.max,
                    bits(&m) == bits(&again)
                );
            }
            Err(e) => println!("{name}: {e}"),
        }
    }
}
