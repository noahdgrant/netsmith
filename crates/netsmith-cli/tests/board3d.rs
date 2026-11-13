//! `netsmith board3d` (ADR 0105): the board's 3D scene as a JSON report, run on the repository's
//! own boards as a person runs it. The board is placed as `fab` places it (the layout file beside
//! the source respected), nothing is written, and the same source gives the same bytes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `netsmith board3d <args>` in `dir`, with KiCad switched off: the command needs none.
fn board3d(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("board3d")
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn json(dir: &Path, args: &[&str]) -> (serde_json::Value, String) {
    let out = board3d(dir, args);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{args:?}: {stderr}");
    let text = String::from_utf8(out.stdout).unwrap();
    (serde_json::from_str(&text).unwrap(), text)
}

fn fixtures() -> PathBuf {
    repo().join("scripts/editor-verify-js/fixtures")
}

fn part<'a>(report: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    report["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["path"] == path)
        .unwrap_or_else(|| panic!("no part `{path}`"))
}

#[test]
fn the_lidded_board_reports_the_part_too_tall_the_unknown_one_and_the_margin() {
    let (r, _) = json(&fixtures(), &["height_limits.nsm", "--json"]);
    assert_eq!(r["schema"], 2);
    assert_eq!(r["units"], "mm");
    assert_eq!(r["board"]["name"], "Lidded");
    assert_eq!(r["board"]["thickness"], 1.6);
    // The layout file puts the tall header at (5, 12) from the top-left corner.
    let tall = part(&r, "tall");
    assert_eq!(tall["layout"]["x"], 5.0);
    assert_eq!(tall["layout"]["y"], 12.0);
    assert_eq!(tall["z_bottom"], 0.0);
    assert_eq!(tall["z_top"], 9.0);
    assert_eq!(tall["provenance"], "stated");
    let bare = part(&r, "bare");
    assert_eq!(bare["provenance"], "unknown");
    assert!(bare["z_top"].is_null() && bare["height"].is_null());
    // Its pins come out of the bottom face by an unknown length.
    assert!(bare["pins"]["protrusion"].is_null());

    let lid = &r["height_limits"][0];
    assert_eq!(lid["name"], "lid");
    assert_eq!(lid["faces"], serde_json::json!(["top"]));
    assert_eq!(lid["max_height"], 3.0);
    assert_eq!(lid["tallest"]["path"], "tall");
    assert_eq!(lid["margin"], -6.0);
    assert_eq!(lid["per_face"][0]["unknown"], serde_json::json!(["bare"]));

    assert_eq!(r["too_tall"][0]["path"], "tall");
    assert_eq!(r["too_tall"][0]["limit_name"], "lid");
    let unknown = &r["unknown_heights"];
    assert_eq!(unknown.as_array().unwrap().len(), 1);
    assert_eq!(unknown[0]["path"], "bare");
    assert_eq!(unknown[0]["under_limits"], serde_json::json!(["lid"]));
    assert!(
        unknown[0]["reason"]
            .as_str()
            .unwrap()
            .contains("no catalog row"),
        "{}",
        unknown[0]["reason"]
    );
    // The same count as the coverage line of fab-notes.txt.
    assert_eq!(
        r["coverage"],
        serde_json::json!({"known": 2, "total": 3, "stated": 2, "catalog": 0, "standard": 0})
    );
    assert_eq!(r["tallest"]["top"]["path"], "tall");
    assert!(r["tallest"]["bottom"].is_null());
    assert_eq!(r["overlaps"], serde_json::json!([]));
}

#[test]
fn the_same_board_gives_the_same_bytes_and_writes_nothing() {
    let examples = repo().join("examples");
    let before = std::fs::read_dir(&examples).unwrap().count();
    for file in ["led.nsm", "four_layer.nsm"] {
        let (_, first) = json(&examples, &[file, "--json"]);
        let (_, second) = json(&examples, &[file, "--json"]);
        assert_eq!(first, second, "{file}");
    }
    assert_eq!(std::fs::read_dir(&examples).unwrap().count(), before);
}

#[test]
fn a_height_from_a_catalog_row_names_the_row_and_the_stackup_has_each_layer_at_its_height() {
    let (r, _) = json(&repo().join("examples"), &["four_layer.nsm", "--json"]);
    let layers = r["board"]["layers"].as_array().unwrap();
    let names: Vec<&str> = layers.iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "F.Mask",
            "F.Cu",
            "dielectric 1",
            "In1.Cu",
            "dielectric 2",
            "In2.Cu",
            "dielectric 3",
            "B.Cu",
            "B.Mask"
        ]
    );
    assert_eq!(layers[0]["z_top"], 0.0);
    for pair in layers.windows(2) {
        assert_eq!(pair[0]["z_bottom"], pair[1]["z_top"]);
    }
    let power = part(&r, "power_in");
    assert_eq!(power["provenance"], "catalog");
    assert!(
        power["catalog_row"].as_str().unwrap().contains('C'),
        "{power}"
    );
    assert_eq!(r["coverage"]["catalog"], 4);
}

#[test]
fn the_summary_names_the_tallest_part_and_what_is_unknown() {
    let out = board3d(&fixtures(), &["height_limits.nsm"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.starts_with("Lidded: 40 x 24 mm, 1.6 mm thick"),
        "{text}"
    );
    assert!(text.contains("heights: 2 of 3 parts known"), "{text}");
    assert!(text.contains("margin -6.00 mm"), "{text}");
    assert!(text.contains("unknown height: J3 (`bare`)"), "{text}");
}

#[test]
fn a_file_of_several_boards_needs_board_and_names_them() {
    let examples = repo().join("examples");
    let out = board3d(&examples, &["two_boards.nsm", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("--board"), "{text}");
    assert!(text.contains("TwoBoards/led"), "{text}");
    let (r, _) = json(
        &examples,
        &["two_boards.nsm", "--board", "TwoBoards/led", "--json"],
    );
    assert_eq!(r["board"]["name"], "TwoBoards/led");
    let out = board3d(&examples, &["two_boards.nsm", "--board", "nope"]);
    assert_eq!(out.status.code(), Some(1));
}

fn copper_layer<'a>(report: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    report["copper"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == name)
        .unwrap_or_else(|| panic!("no copper layer `{name}`"))
}

fn net_area(layer: &serde_json::Value, net: &str) -> f64 {
    layer["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["net"] == net)
        .unwrap_or_else(|| panic!("no copper of `{net}` on {}", layer["name"]))["area"]
        .as_f64()
        .unwrap()
}

#[test]
fn the_four_layer_example_has_copper_on_four_layers_with_the_planes_inside() {
    let (r, _) = json(&repo().join("examples"), &["four_layer.nsm", "--json"]);
    let copper = &r["copper"];
    let layers = copper["layers"].as_array().unwrap();
    let names: Vec<&str> = layers.iter().map(|l| l["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"]);
    // Each copper layer is at the height of its entry in the stackup.
    for l in layers {
        let stack = r["board"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|k| k["name"] == l["name"])
            .unwrap();
        for key in ["z_top", "z_bottom", "thickness"] {
            assert_eq!(l[key], stack[key], "{} {key}", l["name"]);
        }
    }
    // The inner layers are planes, each of one net; the outer layer has the pads.
    let (gnd, v3) = (copper_layer(&r, "In1.Cu"), copper_layer(&r, "In2.Cu"));
    for (plane, net) in [(gnd, "GND"), (v3, "3V3")] {
        assert!(plane["pours"].as_u64().unwrap() >= 1, "{plane}");
        assert!(net_area(plane, net) > 100.0, "{plane}");
    }
    assert!(copper_layer(&r, "F.Cu")["pads"].as_u64().unwrap() > 0);
    // Nothing is routed by this command, so there are no tracks or vias.
    assert_eq!(copper["vias"], 0);
    assert_eq!(copper_layer(&r, "F.Cu")["tracks"], 0);
    assert!(copper["mask_openings"]["top"].as_u64().unwrap() > 0);
    // Areas are rounded to 1e-4 mm2, so they print with at most four decimals.
    for n in layers.iter().flat_map(|l| l["nets"].as_array().unwrap()) {
        let a = n["area"].as_f64().unwrap();
        assert_eq!((a * 1e4).round() / 1e4, a);
    }
}

#[test]
fn a_two_layer_board_has_two_copper_layers_and_the_same_bytes_every_run() {
    let examples = repo().join("examples");
    let (r, first) = json(&examples, &["led.nsm", "--json"]);
    let names: Vec<&str> = r["copper"]["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["F.Cu", "B.Cu"]);
    assert!(copper_layer(&r, "F.Cu")["pads"].as_u64().unwrap() > 0);
    let (_, second) = json(&examples, &["led.nsm", "--json"]);
    assert_eq!(first, second);
}

/// The scene of the board in `source` (a path from the repository), built and placed as
/// `board3d` does (the same target, loader and catalogs, its pick lock held, its physical and
/// layout files read), with no KiCad.
fn scene_of(source: &str) -> netsmith_3d::Scene {
    let path = repo().join(source);
    let target =
        netsmith::project::Target::resolve_from(&repo(), Some(path.to_str().unwrap())).unwrap();
    let file = &target.files[0];
    let text = std::fs::read_to_string(&file.path).unwrap();
    let catalogs = target.catalogs().unwrap();
    let (held, _) = netsmith::pick_lock::open(&file.path, &catalogs, false).unwrap();
    let nowhere = netsmith_testkit::TempDir::new("render-no-kicad");
    let search = netsmith_kicad::Search::under(nowhere.path()).with_no_kicad();
    let root = target.project.as_ref().map(|p| p.root.as_path());
    let libraries = netsmith::Libraries::for_project_by(root, &search).unwrap();
    netsmith::board3d::scene_of(
        &file.name,
        &text,
        &target.loader().unwrap(),
        held.as_ref().unwrap_or(&catalogs),
        &libraries,
        Some(&netsmith::layout::layout_path(&file.path)),
        None,
    )
    .unwrap()
}

/// Draws the scene's meshes from the iso and top views into `dir`, and into
/// `NETSMITH_RENDER3D_OUT` when it is set, to look at; returns the iso picture's bytes.
fn render(scene: &netsmith_3d::Scene, name: &str, dir: &Path) -> Vec<u8> {
    use netsmith_3d::{RenderOptions, View, render_view};
    let meshes = netsmith_3d::meshes_of_scene(scene);
    let options = RenderOptions {
        width: 800,
        height: 500,
        ..RenderOptions::default()
    };
    let mut iso = Vec::new();
    for view in [View::Iso, View::Top] {
        let png = render_view(&meshes, view, &options).to_png();
        let file = format!("{name}-{}.png", view.name());
        std::fs::write(dir.join(&file), &png).unwrap();
        if let Some(out) = std::env::var_os("NETSMITH_RENDER3D_OUT") {
            std::fs::create_dir_all(&out).unwrap();
            std::fs::write(Path::new(&out).join(&file), &png).unwrap();
        }
        if view == View::Iso {
            iso = png;
        }
    }
    iso
}

#[test]
fn the_led_example_is_drawn_from_its_scene_the_same_bytes_each_time() {
    use netsmith_3d::{Material, meshes_of_scene};
    let scene = scene_of("examples/led.nsm");
    let meshes = meshes_of_scene(&scene);
    // The board, then one mesh per part with its path for an id.
    assert_eq!(meshes[0].id, "board");
    assert_eq!(meshes[0].material, Material::Substrate);
    for body in &scene.bodies {
        let mine: Vec<_> = meshes.iter().filter(|m| m.id == body.path).collect();
        let want = if body.height.is_some() {
            Material::Body
        } else {
            Material::BodyUnknown
        };
        assert!(
            mine.iter().any(|m| m.material == want),
            "{}: {:?}",
            body.path,
            mine.iter().map(|m| m.material).collect::<Vec<_>>()
        );
    }
    let dir = netsmith_testkit::TempDir::new("render-led");
    let a = render(&scene, "led", dir.path());
    let b = render(&scene_of("examples/led.nsm"), "led", dir.path());
    assert_eq!(a, b);
    assert_eq!(meshes_of_scene(&scene), meshes_of_scene(&scene_of("examples/led.nsm")));
}

/// A fiducial, a mounting hole and a pad-only test point state `.height &= 0mm` in std: a body
/// of 0 mm is a statement, so each counts as known in the coverage, is `stated`, is no unknown
/// height, and is not drawn as the flat outline of an unknown body.
#[test]
fn a_fiducial_a_mounting_hole_and_a_test_point_are_stated_flat_and_not_unknown() {
    use netsmith_3d::{Material, meshes_of_scene, HeightProvenance};
    for (source, kinds) in [
        (
            "examples/outline_and_pours.nsm",
            ["Fiducial", "MountingHole"],
        ),
        ("examples/two_buck/two_buck.nsm", ["TestPoint", "TestPoint"]),
    ] {
        let scene = scene_of(source);
        let meshes = meshes_of_scene(&scene);
        let flat: Vec<_> = scene
            .bodies
            .iter()
            .filter(|b| kinds.contains(&b.kind.as_str()))
            .collect();
        assert!(
            !flat.is_empty(),
            "{source}: no fiducial, hole or test point"
        );
        for b in &flat {
            assert_eq!(b.height, Some(0.0), "{source}: {}", b.path);
            assert_eq!(b.provenance, HeightProvenance::Stated, "{}", b.path);
            assert!(
                !meshes
                    .iter()
                    .any(|m| m.id == b.path && m.material == Material::BodyUnknown),
                "{}: drawn as an unknown height",
                b.path
            );
        }
        let known = scene.bodies.iter().filter(|b| b.height.is_some()).count();
        assert_eq!(scene.coverage().known, known, "{source}");
    }
}

#[test]
fn the_four_layer_example_draws_its_copper_mask_and_silk_per_net() {
    use netsmith_3d::{Material, Mesh, meshes_of_scene};
    let scene = scene_of("examples/four_layer.nsm");
    let meshes = meshes_of_scene(&scene);
    let has = |m: Material| meshes.iter().any(|x| x.material == m);
    for m in [
        Material::Copper,
        Material::Finish,
        Material::MaskTop,
        Material::Mark,
    ] {
        // Unrouted and with no outer pour, the example has no copper under its mask.
        assert!(has(m), "no {m} mesh");
    }
    // Every copper mesh is named by a net of the board, or is the copper with no net.
    let copper = scene.copper.as_ref().unwrap();
    let nets: Vec<String> = copper
        .layers
        .iter()
        .flat_map(|l| l.nets())
        .map(|n| n.unwrap_or(netsmith_3d::NO_NET).to_string())
        .collect();
    for m in meshes
        .iter()
        .filter(|m| matches!(m.material, Material::Copper | Material::Finish))
        .filter(|m| m.closed)
    {
        assert!(nets.contains(&m.id), "{}", m.id);
    }
    let triangles: usize = meshes.iter().map(Mesh::triangle_count).sum();
    assert!(triangles < 300_000, "{triangles} triangles");
    // The pictures are drawn by the shape test below.
}

#[test]
fn the_four_layer_example_draws_its_generated_parts_as_their_package_shapes() {
    use netsmith_3d::{Material, meshes_of_scene};
    let scene = scene_of("examples/four_layer.nsm");
    let meshes = meshes_of_scene(&scene);
    let shaped: Vec<_> = scene.bodies.iter().filter(|b| b.shape.is_some()).collect();
    assert!(!shaped.is_empty(), "no part of four_layer is shaped");
    for b in &shaped {
        // Shaped only with a known height, and the shape reaches it exactly.
        let h = b.height.unwrap();
        // The part's meshes but its plated barrels, which share its id.
        let mine: Vec<_> = meshes
            .iter()
            .filter(|m| m.id == b.path && m.material != Material::Copper)
            .cloned()
            .collect();
        assert!(
            mine.iter().any(|m| m.material == Material::Metal),
            "{}",
            b.path
        );
        let bb = netsmith_3d::mesh_bounds(&mine).unwrap();
        let (lo, hi) = b.z_range();
        assert!(
            (bb.min[2] - lo.unwrap()).abs() < 1e-9 && (bb.max[2] - hi.unwrap()).abs() < 1e-9,
            "{} ({h} mm): {bb:?}",
            b.path
        );
    }
    let dir = netsmith_testkit::TempDir::new("render-four-layer");
    let a = render(&scene, "four_layer", dir.path());
    assert_eq!(
        a,
        render(
            &scene_of("examples/four_layer.nsm"),
            "four_layer",
            dir.path()
        )
    );
}

#[test]
fn a_pin_header_is_as_tall_as_its_catalog_row_and_its_plastic_is_the_sourced_height() {
    use netsmith_3d::{Material, meshes_of_scene};
    // The four-layer example's headers are picked as JXTCONN PH2.54-1X2P-H25 (C42431798): the
    // row's height is the pin's tip, 2.50 mm of plastic plus 6.00 +/-0.2 mm of pin.
    let (r, _) = json(&repo().join("examples"), &["four_layer.nsm", "--json"]);
    for path in ["power_in", "power_out"] {
        let p = part(&r, path);
        assert_eq!(p["provenance"], "catalog", "{p}");
        assert!(
            p["catalog_row"].as_str().unwrap().contains("C42431798"),
            "{p}"
        );
        assert_eq!(p["height"], 8.9, "{p}");
        assert_eq!(p["z_top"], 8.9, "{p}");
    }
    let scene = scene_of("examples/four_layer.nsm");
    let meshes = meshes_of_scene(&scene);
    for b in scene.bodies.iter().filter(|b| b.kind == "Header1x2") {
        assert_eq!(b.height, Some(8.9), "{}", b.path);
        let top = |m: Material| {
            let mine: Vec<_> = meshes
                .iter()
                .filter(|x| x.id == b.path && x.material == m)
                .cloned()
                .collect();
            netsmith_3d::mesh_bounds(&mine).unwrap().max[2] - b.z_face
        };
        // The plastic is the 2.54 mm family's 2.50 mm and the pins go on to the row's height.
        assert!((top(Material::Body) - 2.5).abs() < 1e-6, "{}", b.path);
        assert!((top(Material::Metal) - 8.9).abs() < 1e-6, "{}", b.path);
    }
}

#[test]
fn a_limit_a_part_breaks_is_drawn_as_broken() {
    use netsmith_3d::{Material, meshes_of_scene};
    let scene = scene_of("scripts/editor-verify-js/fixtures/height_limits.nsm");
    let meshes = meshes_of_scene(&scene);
    let limits: Vec<_> = meshes
        .iter()
        .filter(|m| m.id.starts_with("limit:"))
        .map(|m| m.material)
        .collect();
    assert!(
        limits.contains(&Material::LimitViolated),
        "{limits:?} {:?}",
        scene.limits
    );
    assert!(meshes.iter().any(|m| m.material == Material::BodyUnknown));
    let dir = netsmith_testkit::TempDir::new("render-lidded");
    render(&scene, "lidded", dir.path());
}

/// The JSON chunk of a `.glb` file.
fn glb_json(glb: &[u8]) -> serde_json::Value {
    netsmith_3d::json_chunk(glb).expect("a GLB file")
}

fn glb_node<'a>(doc: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["name"] == name)
        .unwrap_or_else(|| panic!("no node `{name}`"))
}

#[test]
fn glb_writes_the_scene_as_gltf_and_prints_nothing_else() {
    let examples = repo().join("examples");
    let before = std::fs::read_dir(&examples).unwrap().count();
    let out = netsmith_testkit::TempDir::new("board3d-glb");
    let (a, b) = (out.join("a.glb"), out.join("b.glb"));
    for file in [&a, &b] {
        let run = board3d(
            &examples,
            &["four_layer.nsm", "--glb", file.to_str().unwrap()],
        );
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(
            run.stdout.is_empty(),
            "{}",
            String::from_utf8_lossy(&run.stdout)
        );
    }
    let glb = std::fs::read(&a).unwrap();
    assert_eq!(glb, std::fs::read(&b).unwrap(), "the same bytes each time");
    assert_eq!(&glb[..4], b"glTF");
    let doc = glb_json(&glb);
    let root = &glb_node(&doc, "board")["extras"]["netsmith"];
    assert_eq!(root["kind"], "board");
    assert_eq!(root["board"], "FourLayerExample");
    assert_eq!(root["stack_thickness_mm"], 1.6062);
    let j1 = &glb_node(&doc, "J1")["extras"]["netsmith"];
    assert_eq!(j1["path"], "power_in");
    assert_eq!(j1["provenance"], "catalog");
    assert_eq!(std::fs::read_dir(&examples).unwrap().count(), before);

    // With `--json` too, the report is printed and the same file written.
    let c = out.join("c.glb");
    let (report, _) = json(
        &examples,
        &["four_layer.nsm", "--json", "--glb", c.to_str().unwrap()],
    );
    assert_eq!(report["board"]["name"], "FourLayerExample");
    assert_eq!(std::fs::read(&c).unwrap(), glb);

    let run = board3d(&examples, &["four_layer.nsm", "--glb"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("--glb <FILE.GLB>"));
}

#[test]
fn an_unknown_height_is_flat_in_the_glb_and_the_limit_it_breaks_is_marked() {
    let out = netsmith_testkit::TempDir::new("board3d-glb-lid");
    let file = out.join("lid.glb");
    let run = board3d(
        &fixtures(),
        &["height_limits.nsm", "--glb", file.to_str().unwrap()],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let doc = glb_json(&std::fs::read(&file).unwrap());
    let bare = &glb_node(&doc, "J3")["extras"]["netsmith"];
    assert_eq!(bare["provenance"], "unknown");
    assert!(bare["height_mm"].is_null() && bare["z_top_mm"].is_null());
    let mesh = glb_node(&doc, "J3")["mesh"].as_u64().unwrap() as usize;
    let material = doc["meshes"][mesh]["primitives"][0]["material"]
        .as_u64()
        .unwrap() as usize;
    assert_eq!(doc["materials"][material]["name"], "body-unknown");
    let lid = &glb_node(&doc, "limit:lid:top")["extras"]["netsmith"];
    assert_eq!(lid["violated"], true);
    assert_eq!(lid["tallest"], "tall");
    let codes: Vec<&str> = glb_node(&doc, "board")["extras"]["netsmith"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].as_str().unwrap())
        .collect();
    assert_eq!(codes, ["too_tall", "height_unchecked"]);
}

#[test]
fn step_writes_the_board_and_its_parts_as_an_assembly_and_prints_nothing_else() {
    let examples = repo().join("examples");
    let out = netsmith_testkit::TempDir::new("board3d-step");
    let (a, b) = (out.join("a.step"), out.join("b.step"));
    for file in [&a, &b] {
        let run = board3d(
            &examples,
            &["four_layer.nsm", "--step", file.to_str().unwrap()],
        );
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        assert!(run.stdout.is_empty());
    }
    let step = std::fs::read_to_string(&a).unwrap();
    assert_eq!(
        step,
        std::fs::read_to_string(&b).unwrap(),
        "the same bytes each time"
    );
    assert!(step.starts_with("ISO-10303-21;"));
    assert!(step.contains("FILE_NAME('FourLayerExample','1970-01-01T00:00:00'"));
    // The board, and each part a product named by its designator, with its path as its id.
    assert!(step.contains("PRODUCT('board','board',"));
    assert!(step.contains("PRODUCT('power_in','J1','"));
    assert!(step.contains("MANIFOLD_SOLID_BREP('board',"));
    // The board's four holes are cylinders through its 1.6 mm.
    assert!(step.contains("CYLINDRICAL_SURFACE("));
    assert!(step.contains(",-1.6))"));
    // netsmith's own reader (ADR 0108) reads it back: the board's solid is its outline through
    // its thickness, and every solid faces out.
    let scene = scene_of("examples/four_layer.nsm");
    let model = netsmith_3d::read_step(step.as_bytes()).unwrap();
    let board = model.pieces.iter().find(|p| p.mesh.id == "board").unwrap();
    let b = board.mesh.bounds().unwrap();
    let nm = |x: f64| (x * 1e6).round() / 1e6;
    assert_eq!(b.min, [0.0, 0.0, -nm(scene.board.thickness)]);
    assert_eq!(b.max, [nm(scene.board.width), nm(scene.board.height), 0.0]);
    assert!(model.pieces.len() > 1, "the parts too");
    for p in &model.pieces {
        assert!(
            p.mesh.closed && p.mesh.triangle_count() > 0,
            "{}",
            p.mesh.id
        );
    }

    // With `--glb` and `--json` too, all three are written, the same as alone.
    let (c, g) = (out.join("c.step"), out.join("c.glb"));
    let (report, _) = json(
        &examples,
        &[
            "four_layer.nsm",
            "--json",
            "--glb",
            g.to_str().unwrap(),
            "--step",
            c.to_str().unwrap(),
        ],
    );
    assert_eq!(report["board"]["name"], "FourLayerExample");
    assert_eq!(std::fs::read_to_string(&c).unwrap(), step);
    assert!(g.exists());

    let run = board3d(&examples, &["four_layer.nsm", "--step"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("--step <FILE.STEP>"));
}

#[test]
fn an_unknown_height_is_an_outline_in_the_step_and_never_a_solid() {
    let out = netsmith_testkit::TempDir::new("board3d-step-lid");
    let file = out.join("lid.step");
    let run = board3d(
        &fixtures(),
        &["height_limits.nsm", "--step", file.to_str().unwrap()],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let step = std::fs::read_to_string(&file).unwrap();
    assert!(step.contains("PRODUCT('bare','J3','"));
    assert!(step.contains("; height unknown: outline only'"));
    assert!(step.contains("GEOMETRICALLY_BOUNDED_WIREFRAME_SHAPE_REPRESENTATION('J3',"));
    assert!(!step.contains("ADVANCED_BREP_SHAPE_REPRESENTATION('J3',"));
    assert!(step.contains("ADVANCED_BREP_SHAPE_REPRESENTATION('J1',"));
}

/// A GET over a socket: the status, the head (lower case) and the body's bytes.
fn http_bytes(port: u16, target: &str) -> (u16, String, Vec<u8>) {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(60)))
        .unwrap();
    write!(
        s,
        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, head, raw[end + 4..].to_vec())
}

#[test]
fn the_editor_serves_the_bytes_that_board3d_writes_for_the_placement_it_draws() {
    let dir = netsmith_testkit::TempDir::new("board3d-editor-glb");
    for name in [
        "height_limits.nsm",
        "height_limits.pcb.toml",
        "height_limits.physical.nsm",
    ] {
        std::fs::copy(fixtures().join(name), dir.join(name)).unwrap();
    }
    // The command line as a person runs it, with the KiCad this machine has, as the editor in
    // this process has it.
    let file = dir.join("cli.glb");
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board3d", "height_limits.nsm", "--glb"])
        .arg(&file)
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let written = std::fs::read(&file).unwrap();

    let target = netsmith::project::Target::resolve_from(
        dir.path(),
        Some(dir.join("height_limits.nsm").to_str().unwrap()),
    )
    .unwrap();
    let editor = netsmith::editor::Workspace::open(target).unwrap();
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || netsmith_editor::serve(listener, std::sync::Arc::new(editor)));

    let (status, head, scene) = http_bytes(port, "/api/pcb/scene?board=Lidded");
    assert_eq!(status, 200, "{head}");
    let scene: serde_json::Value = serde_json::from_slice(&scene).unwrap();
    let (status, head, served) = http_bytes(port, "/api/pcb/3d?board=Lidded");
    assert_eq!(status, 200, "{head}");
    assert!(head.contains("content-type: model/gltf-binary"), "{head}");
    let stamp = scene["layoutStamp"].as_str().unwrap();
    assert!(
        head.contains(&format!(
            "x-netsmith-layout-stamp: {}",
            stamp.to_ascii_lowercase()
        )),
        "the stamp of the placement the 2D view drew ({stamp}): {head}"
    );
    assert!(served == written, "the editor's file is the command's");
    let (status, _, _) = http_bytes(port, "/api/pcb/3d?board=Nope");
    assert_eq!(status, 404);
}

/// The editor serves a system's name as its system scene (ADR 0110): the bytes `board3d --board
/// System --glb` writes, each member placed as the 2D view has it, and the members' stamps.
#[test]
fn the_editor_serves_the_system_scene_that_board3d_writes() {
    let dir = netsmith_testkit::TempDir::new("board3d-editor-system");
    std::fs::copy(
        repo().join("examples/two_boards.nsm"),
        dir.join("two_boards.nsm"),
    )
    .unwrap();
    std::fs::copy(
        repo().join("examples/two_boards.lock.toml"),
        dir.join("two_boards.lock.toml"),
    )
    .unwrap();
    let file = dir.join("cli.glb");
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board3d", "two_boards.nsm", "--board", "TwoBoards", "--glb"])
        .arg(&file)
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let written = std::fs::read(&file).unwrap();

    let target = netsmith::project::Target::resolve_from(
        dir.path(),
        Some(dir.join("two_boards.nsm").to_str().unwrap()),
    )
    .unwrap();
    let editor = netsmith::editor::Workspace::open(target).unwrap();
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || netsmith_editor::serve(listener, std::sync::Arc::new(editor)));

    let (status, head, served) = http_bytes(port, "/api/pcb/3d?board=TwoBoards");
    assert_eq!(status, 200, "{head}");
    assert!(head.contains("content-type: model/gltf-binary"), "{head}");
    assert!(served == written, "the editor's file is the command's");
    let doc = glb_json(&served);
    assert_eq!(doc["nodes"][0]["name"], "system");
    // One stamp per member, in the system's order.
    let stamp = head
        .lines()
        .find_map(|l| l.strip_prefix("x-netsmith-layout-stamp: "))
        .unwrap_or_default();
    assert_eq!(stamp.split(',').count(), 2, "{head}");
}

// ---- `netsmith render`: the scene drawn to a PNG (ADR 0105) --------------------------------

/// `netsmith render <args>` in `dir`, with KiCad switched off: the command needs none.
fn render_cmd(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("render")
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

/// The bytes of the PNG `netsmith render <source> <args> --out <out>` writes into `out`.
fn picture(source: &Path, out: &Path, args: &[&str]) -> Vec<u8> {
    let result = render_cmd(
        source.parent().unwrap(),
        &[
            &[source.file_name().unwrap().to_str().unwrap()][..],
            args,
            &["--out", out.to_str().unwrap()],
        ]
        .concat(),
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(result.status.success(), "{args:?}: {stderr}");
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim(),
        out.to_str().unwrap(),
        "the command prints the path it wrote"
    );
    std::fs::read(out).unwrap()
}

/// A PNG's width and height from its header.
fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(&png[12..16], b"IHDR");
    let at = |i: usize| u32::from_be_bytes(png[i..i + 4].try_into().unwrap());
    (at(16), at(20))
}

fn led() -> PathBuf {
    repo().join("examples/led.nsm")
}

#[test]
fn render_gives_the_same_bytes_twice_and_names_its_file_after_board_and_view() {
    let cwd = netsmith_testkit::TempDir::new("render-default-name");
    let run = || render_cmd(cwd.path(), &[led().to_str().unwrap(), "--size", "320x200"]);
    let first = run();
    assert!(first.status.success(), "{:?}", first.stderr);
    // The default file is `<board>-<view>.png` (the default view is iso), here in the current
    // folder, and the command prints it.
    assert_eq!(
        String::from_utf8_lossy(&first.stdout).trim(),
        "LedExample-iso.png"
    );
    let bytes = std::fs::read(cwd.path().join("LedExample-iso.png")).unwrap();
    assert_eq!(png_size(&bytes), (320, 200));
    std::fs::remove_file(cwd.path().join("LedExample-iso.png")).unwrap();
    assert!(run().status.success());
    assert_eq!(
        bytes,
        std::fs::read(cwd.path().join("LedExample-iso.png")).unwrap()
    );
    // It draws the board as the scene says: the same bytes as the rasterizer gives the scene.
    let scene = scene_of("examples/led.nsm");
    let options = netsmith_3d::RenderOptions {
        width: 320,
        height: 200,
        ..Default::default()
    };
    let direct = netsmith_3d::render_view(
        &netsmith_3d::meshes_of_scene(&scene),
        netsmith_3d::View::Iso,
        &options,
    )
    .to_png();
    assert_eq!(bytes, direct);
    // Nothing else is written beside the source.
    assert!(!repo().join("examples/LedExample-iso.png").exists());
}

#[test]
fn every_view_writes_a_png_of_the_size_asked_for_and_they_differ() {
    let dir = netsmith_testkit::TempDir::new("render-views");
    let mut seen: Vec<Vec<u8>> = Vec::new();
    for view in netsmith_3d::View::ALL {
        let out = dir.path().join(format!("{}.png", view.name()));
        let png = picture(&led(), &out, &["--view", view.name(), "--size", "256x160"]);
        assert_eq!(png_size(&png), (256, 160), "{}", view.name());
        assert!(!seen.contains(&png), "{} repeats a view", view.name());
        seen.push(png);
    }
    // The lenses, the themes and x-ray each change the picture.
    let base = picture(&led(), &dir.path().join("b.png"), &["--size", "256x160"]);
    for flags in [
        &["--perspective"][..],
        &["--theme", "dark"],
        &["--xray"],
        &["--ortho"],
    ] {
        let args = [&["--size", "256x160"][..], flags].concat();
        let png = picture(&led(), &dir.path().join("f.png"), &args);
        assert_eq!(png_size(&png), (256, 160));
        assert_eq!(png == base, *flags == ["--ortho"], "{flags:?}");
    }
}

#[test]
fn a_highlight_by_designator_or_path_lights_that_part_and_an_unknown_id_is_an_error() {
    let dir = netsmith_testkit::TempDir::new("render-highlight");
    let (size, out) = (["--size", "256x160"], dir.path().join("h.png"));
    let plain = picture(&led(), &out, &size);
    let by_refdes = picture(&led(), &out, &[&size[..], &["--highlight", "J1"]].concat());
    let by_path = picture(
        &led(),
        &out,
        &[&size[..], &["--highlight", "power_in"]].concat(),
    );
    assert_ne!(plain, by_refdes);
    assert_eq!(by_refdes, by_path);
    // Several ids, repeated or comma-separated, are the same request.
    let comma = picture(
        &led(),
        &out,
        &[&size[..], &["--highlight", "J1,net:GND"]].concat(),
    );
    let twice = picture(
        &led(),
        &out,
        &[&size[..], &["--highlight", "J1", "--highlight", "net:GND"]].concat(),
    );
    assert_eq!(comma, twice);

    let gone = dir.path().join("none.png");
    let bad = render_cmd(
        dir.path(),
        &[
            led().to_str().unwrap(),
            "--highlight",
            "J1,Q9,net:nope,limit:lid",
            "--out",
            gone.to_str().unwrap(),
        ],
    );
    assert_eq!(bad.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&bad.stderr);
    for id in ["`Q9`", "`net:nope`", "`limit:lid`"] {
        assert!(stderr.contains(id), "{stderr}");
    }
    assert!(stderr.contains("power_in"), "{stderr}");
    assert!(
        bad.stdout.is_empty() && !gone.exists(),
        "an error writes nothing"
    );
}

#[test]
fn a_limit_and_a_net_can_be_highlighted() {
    let dir = netsmith_testkit::TempDir::new("render-highlight-limit");
    let lidded = fixtures().join("height_limits.nsm");
    let out = dir.path().join("l.png");
    let args = ["--size", "256x160"];
    let plain = picture(&lidded, &out, &args);
    let limit = picture(
        &lidded,
        &out,
        &[&args[..], &["--highlight", "limit:lid"]].concat(),
    );
    assert_ne!(plain, limit);
    let net = picture(
        &led(),
        &out,
        &[&args[..], &["--highlight", "net:VCC"]].concat(),
    );
    assert_eq!(png_size(&net), (256, 160));
}

#[test]
fn a_section_cuts_the_picture_and_is_checked() {
    let dir = netsmith_testkit::TempDir::new("render-section");
    let out = dir.path().join("s.png");
    let args = ["--size", "256x160", "--view", "iso"];
    let whole = picture(&led(), &out, &args);
    // The LED board is 14 mm long: a cut at x = 7 leaves half of it.
    let cut = picture(&led(), &out, &[&args[..], &["--section", "x=7"]].concat());
    assert_ne!(whole, cut);
    assert_eq!(png_size(&cut), (256, 160));
    // The same cut, as a number with a sign and a fraction, on each axis.
    for section in ["y=3.5", "z=-0.8", "z=2.5"] {
        let png = picture(&led(), &out, &[&args[..], &["--section", section]].concat());
        assert_ne!(png, whole, "{section}");
    }
    for bad in ["x", "w=3", "x=", "x=abc", "x=1,y=2", "x=NaN"] {
        let result = render_cmd(
            dir.path(),
            &[led().to_str().unwrap(), "--section", bad, "--out", "no.png"],
        );
        assert_eq!(result.status.code(), Some(2), "{bad}");
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains("--section"), "{bad}: {stderr}");
    }
    assert!(!dir.path().join("no.png").exists());
}

/// The section of `scene` across y = `y` seen from the front through a camera `half_height` mm
/// tall, centred on (`x`, `z`) in 800x500 pixels, caps on; written into
/// `NETSMITH_RENDER3D_OUT` when it is set, to look at.
fn section_picture(
    scene: &netsmith_3d::Scene,
    name: &str,
    y: f64,
    (x, z, half_height): (f64, f64, f64),
) -> netsmith_3d::Image {
    use netsmith_3d::{Axis, Camera, Lens, RenderOptions, Section, render_meshes};
    let meshes = netsmith_3d::meshes_of_scene_section(scene, Axis::Y, y);
    let options = RenderOptions {
        width: 800,
        height: 500,
        section: Some(Section {
            axis: Axis::Y,
            at: y,
            keep_below: false,
        }),
        ..RenderOptions::default()
    };
    let camera = Camera {
        eye: [x, -100.0, z],
        target: [x, 0.0, z],
        up: [0.0, 0.0, 1.0],
        lens: Lens::Orthographic { half_height },
    };
    let image = render_meshes(&meshes, &camera, &options);
    if let Some(out) = std::env::var_os("NETSMITH_RENDER3D_OUT") {
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(Path::new(&out).join(format!("{name}.png")), image.to_png()).unwrap();
    }
    image
}

#[test]
fn a_section_of_the_four_layer_board_shows_its_stackup_in_bands_in_order_top_to_bottom() {
    use netsmith_3d::{Material, Rgba, Theme};
    use netsmith_pcb::stackup::LayerKind;
    let scene = scene_of("examples/four_layer.nsm");
    // Across J1 (a header 8.9 mm tall; the layout file has it 3.25 mm from the top edge of the
    // 7 mm board, so at y = 3.75 mm from the bottom edge): a section through a part.
    let y = 3.75;
    assert!(scene.bodies.iter().any(|b| b.refdes == "J1"
        && (b.corners.iter().any(|c| c.y < y) && b.corners.iter().any(|c| c.y > y))));
    // Near the board's left edge (x = 0), 0.9 mm either side of the middle of the stackup: the
    // picture is 444 pixels to the mm, the thinnest layer (the mask, 10 um) four pixels.
    let (x, z, half) = (1.2, -0.8, 0.9);
    let image = section_picture(&scene, "four_layer-section-y", y, (x, z, half));
    let again = section_picture(&scene, "four_layer-section-y-again", y, (x, z, half));
    assert_eq!(image, again, "the same bytes every time");

    let layers = &scene.board.layers;
    let palette = Theme::Light.palette();
    // Every cap is lit as the same face, so the colours of the bands are the palette's times one
    // factor: take it from the middle of the core.
    let row_of = |zz: f64| ((z + half - zz) / (2.0 * half) * 500.0).floor() as usize;
    // Column 250 is x = 0.86 mm: both inner planes have copper there (they are poured, with
    // clearance round the pad at the right of the picture).
    let column = 250;
    let core = layers.iter().find(|l| l.kind == LayerKind::Core).unwrap();
    let probe = image.pixel(column, row_of(0.5 * (core.z_top + core.z_bottom)));
    let want = palette.colour(Material::Substrate);
    let factor = f64::from(probe[1]) / f64::from(want[1]);
    let lit = |c: Rgba| [0, 1, 2].map(|k| (f64::from(c[k]) * factor).min(255.0));
    let near = |p: Rgba, c: Rgba| {
        let c = lit(c);
        (0..3).all(|k| (f64::from(p[k]) - c[k]).abs() <= 8.0)
    };
    let mut seen: Vec<&str> = Vec::new();
    for l in layers {
        let p = image.pixel(column, row_of(0.5 * (l.z_top + l.z_bottom)));
        let name = match l.kind {
            LayerKind::Mask => {
                assert!(
                    near(p, palette.colour(Material::MaskTop))
                        || near(p, palette.colour(Material::MaskBottom)),
                    "{}: {p:?}",
                    l.name
                );
                "mask"
            }
            LayerKind::Core => {
                assert!(
                    near(p, palette.colour(Material::Substrate)),
                    "{}: {p:?}",
                    l.name
                );
                "core"
            }
            LayerKind::Prepreg => {
                assert!(
                    near(p, palette.colour(Material::Prepreg)),
                    "{}: {p:?}",
                    l.name
                );
                "prepreg"
            }
            LayerKind::Copper => {
                // Copper where the layer has copper at this cut, the insulator it is
                // embedded in where it has none: the two inner planes are poured across.
                assert!(
                    near(p, palette.colour(Material::Copper))
                        || near(p, palette.colour(Material::Prepreg))
                        || near(p, palette.colour(Material::MaskTop))
                        || near(p, palette.colour(Material::MaskBottom)),
                    "{}: {p:?}",
                    l.name
                );
                if l.name.starts_with("In") {
                    assert!(
                        near(p, palette.colour(Material::Copper)),
                        "{}: {p:?}",
                        l.name
                    );
                }
                "copper"
            }
        };
        seen.push(name);
    }
    assert_eq!(
        seen,
        [
            "mask", "copper", "prepreg", "copper", "core", "copper", "prepreg", "copper", "mask"
        ]
    );
    // Above the board's top face at this x there is nothing but background; the part's body is
    // cut solid where the plane crosses it (the picture's middle column of the header).
    assert_eq!(image.pixel(200, 10), palette.background);
    let body = section_picture(&scene, "four_layer-section-body", y, (13.0, 1.0, 1.5));
    let top = body.pixel(400, 40);
    assert_ne!(top, palette.background, "a part's body is cut solid");
}

#[test]
fn the_cli_fills_a_section_and_no_caps_leaves_it_open() {
    let dir = netsmith_testkit::TempDir::new("render-caps");
    let src = repo().join("examples/four_layer.nsm");
    let args = ["--view", "right", "--section", "x=13", "--size", "400x300"];
    let capped = picture(&src, &dir.path().join("a.png"), &args);
    let open = picture(
        &src,
        &dir.path().join("b.png"),
        &[&args[..], &["--no-caps"]].concat(),
    );
    assert_ne!(capped, open);
    assert_eq!(
        capped,
        picture(&src, &dir.path().join("c.png"), &args),
        "the same bytes twice"
    );
}

#[test]
fn a_size_out_of_bounds_or_a_wrong_view_is_a_usage_error() {
    let dir = netsmith_testkit::TempDir::new("render-usage");
    for (args, word) in [
        (&["--size", "63x200"][..], "--size"),
        (&["--size", "9000x200"], "--size"),
        (&["--size", "8192x8192"], "--size"),
        (&["--size", "800"], "--size"),
        (&["--size", "axb"], "--size"),
        (&["--view", "sideways"], "iso-bottom"),
        (&["--theme", "blue"], "--theme"),
        (&["--ortho", "--perspective"], "--ortho"),
        (&["--glb", "x"], "--glb"),
    ] {
        let result = render_cmd(dir.path(), &[&[led().to_str().unwrap()][..], args].concat());
        assert_eq!(result.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(word), "{args:?}: {stderr}");
    }
    // The smallest and a large picture are fine.
    let small = picture(&led(), &dir.path().join("a.png"), &["--size", "64x64"]);
    assert_eq!(png_size(&small), (64, 64));
}

#[test]
fn a_file_of_several_boards_needs_board_and_a_board_that_does_not_build_is_exit_one() {
    let dir = netsmith_testkit::TempDir::new("render-boards");
    let two = repo().join("examples/two_boards.nsm");
    let result = render_cmd(dir.path(), &[two.to_str().unwrap(), "--out", "x.png"]);
    assert_eq!(result.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("--board"), "{stderr}");
    assert!(stderr.contains("TwoBoards/led"), "{stderr}");
    let out = dir.path().join("one.png");
    let png = picture(
        &two,
        &out,
        &["--board", "TwoBoards/led", "--size", "128x80"],
    );
    assert_eq!(png_size(&png), (128, 80));
    // The default file of a board of a system has no slash in it.
    let named = render_cmd(
        dir.path(),
        &[
            two.to_str().unwrap(),
            "--board",
            "TwoBoards/led",
            "--view",
            "top",
            "--size",
            "64x64",
        ],
    );
    assert!(named.status.success(), "{:?}", named.stderr);
    assert_eq!(
        String::from_utf8_lossy(&named.stdout).trim(),
        "TwoBoards-led-top.png"
    );
    assert!(dir.path().join("TwoBoards-led-top.png").exists());

    let missing = render_cmd(dir.path(), &["no_such_file.nsm"]);
    assert_eq!(missing.status.code(), Some(2));
}

#[test]
fn render_is_listed_in_the_help() {
    let out = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["render", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for word in [
        "netsmith render",
        "--highlight",
        "--section",
        "--xray",
        "--perspective",
    ] {
        assert!(text.contains(word), "{word}");
    }
}

/// A board with one part and nothing to route, so that the copper `fab` draws is the copper
/// `board3d` draws.
const NOTHING_TO_ROUTE: &str = "import std::parts::connector::Header1x2\nimport std::mfg::jlcpcb::TwoLayer\n\nboard Lonely {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .h: Header1x2\n}\n";

/// `netsmith fab <args>` in `dir`, with KiCad switched off: the package needs none (ADR 0097).
fn fab(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("fab")
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

#[test]
fn fab_writes_the_scene_as_a_glb_beside_the_notes_and_not_in_the_gerber_zip() {
    let dir = netsmith_testkit::TempDir::new("fab-glb-default");
    std::fs::write(dir.join("a.nsm"), NOTHING_TO_ROUTE).unwrap();
    let run = fab(&dir, &["a.nsm", "--out", "out"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let package = dir.join("out/Lonely");
    let glb = std::fs::read(package.join("Lonely.glb")).expect("fab writes <board>.glb");
    assert!(package.join("fab-notes.txt").is_file());

    // The bytes are those of `board3d --glb` for the board, placed as fab placed it (this board
    // has nothing to route, so routed and unrouted copper are the same).
    let by_board3d = dir.join("b.glb");
    let run = board3d(&dir, &["a.nsm", "--glb", by_board3d.to_str().unwrap()]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(glb, std::fs::read(&by_board3d).unwrap());

    // It is not for the board house: the Gerber zip has no `.glb`.
    let zip = std::fs::File::open(package.join("Lonely-gerbers.zip")).unwrap();
    let archive = zip::ZipArchive::new(zip).unwrap();
    assert!(
        archive.file_names().all(|n| !n.ends_with(".glb")),
        "{:?}",
        archive.file_names().collect::<Vec<_>>()
    );
}

#[test]
fn fab_draws_the_routed_copper_in_its_glb() {
    let examples = repo().join("examples");
    let dir = netsmith_testkit::TempDir::new("fab-glb-routed");
    let run = fab(
        &examples,
        &["led.nsm", "--out", dir.to_str().unwrap(), "--locked"],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let routed = std::fs::read(dir.join("LedExample/LedExample.glb")).unwrap();
    let unrouted = dir.join("unrouted.glb");
    let run = board3d(&examples, &["led.nsm", "--glb", unrouted.to_str().unwrap()]);
    assert!(run.status.success());
    let unrouted = std::fs::read(&unrouted).unwrap();
    assert_eq!(&routed[..4], b"glTF");
    assert!(
        routed.len() > unrouted.len(),
        "the tracks are in the routed scene: {} bytes against {}",
        routed.len(),
        unrouted.len()
    );
    let (a, b) = (glb_json(&routed), glb_json(&unrouted));
    assert_eq!(
        glb_node(&a, "board")["extras"]["netsmith"]["board"],
        glb_node(&b, "board")["extras"]["netsmith"]["board"]
    );
}

#[test]
fn no_glb_and_the_manifest_key_leave_the_scene_out_and_remove_an_old_one() {
    let dir = netsmith_testkit::TempDir::new("fab-glb-off");
    std::fs::write(dir.join("a.nsm"), NOTHING_TO_ROUTE).unwrap();
    let glb = dir.join("out/Lonely/Lonely.glb");
    let ok = |run: Output| {
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    };
    ok(fab(&dir, &["a.nsm", "--out", "out"]));
    assert!(glb.is_file());

    // `--no-glb` leaves it out, and the one of the earlier run is removed so that the folder
    // never holds the scene of another layout.
    ok(fab(&dir, &["a.nsm", "--out", "out", "--no-glb"]));
    assert!(!glb.exists());
    assert!(dir.join("out/Lonely/fab-notes.txt").is_file());

    // `glb = false` in netsmith.toml does the same for the project, in `fab` and in an order.
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.nsm"), NOTHING_TO_ROUTE).unwrap();
    std::fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"p\"\nglb = false\n\n[order.one.boards]\nLonely = 1\n",
    )
    .unwrap();
    ok(fab(&dir, &["--out", "kept-out"]));
    assert!(!dir.join("kept-out/Lonely/Lonely.glb").exists());
    assert!(dir.join("kept-out/Lonely/Lonely.kicad_pcb").is_file());
    ok(fab(&dir, &["--order", "one", "--out", "order-out"]));
    let written: Vec<_> = walk(&dir.join("order-out"));
    assert!(written.iter().any(|f| f.ends_with("Lonely.kicad_pcb")));
    assert!(!written.iter().any(|f| f.ends_with(".glb")), "{written:?}");

    // Without the key the order has it.
    std::fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"p\"\n\n[order.one.boards]\nLonely = 1\n",
    )
    .unwrap();
    ok(fab(&dir, &["--order", "one", "--out", "order-on"]));
    assert!(
        walk(&dir.join("order-on"))
            .iter()
            .any(|f| f.ends_with("Lonely.glb"))
    );
    ok(fab(
        &dir,
        &["--order", "one", "--out", "order-flag", "--no-glb"],
    ));
    assert!(
        !walk(&dir.join("order-flag"))
            .iter()
            .any(|f| f.ends_with(".glb"))
    );
}

#[test]
fn fab_writes_the_board_as_step_beside_the_glb_and_not_in_the_gerber_zip() {
    let dir = netsmith_testkit::TempDir::new("fab-step-default");
    std::fs::write(dir.join("a.nsm"), NOTHING_TO_ROUTE).unwrap();
    let run = fab(&dir, &["a.nsm", "--out", "out"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let package = dir.join("out/Lonely");
    let step = std::fs::read(package.join("Lonely.step")).expect("fab writes <board>.step");
    assert!(step.starts_with(b"ISO-10303-21;"));
    assert!(package.join("Lonely.glb").is_file());

    // The bytes are those of `board3d --step` for the board, placed as fab placed it.
    let by_board3d = dir.join("b.step");
    let run = board3d(&dir, &["a.nsm", "--step", by_board3d.to_str().unwrap()]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(step, std::fs::read(&by_board3d).unwrap());

    // It is not for the board house: the Gerber zip has no `.step`.
    let zip = std::fs::File::open(package.join("Lonely-gerbers.zip")).unwrap();
    let archive = zip::ZipArchive::new(zip).unwrap();
    assert!(
        archive.file_names().all(|n| !n.ends_with(".step")),
        "{:?}",
        archive.file_names().collect::<Vec<_>>()
    );
}

#[test]
fn the_step_of_a_routed_board_is_the_step_board3d_writes_for_it() {
    // The STEP holds no copper, so the routing does not change it: a board that fab routes has
    // the bytes of `board3d --step`, which routes nothing.
    let examples = repo().join("examples");
    let dir = netsmith_testkit::TempDir::new("fab-step-routed");
    let run = fab(
        &examples,
        &["led.nsm", "--out", dir.to_str().unwrap(), "--locked"],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let routed = std::fs::read(dir.join("LedExample/LedExample.step")).unwrap();
    let unrouted = dir.join("unrouted.step");
    let run = board3d(
        &examples,
        &["led.nsm", "--step", unrouted.to_str().unwrap()],
    );
    assert!(run.status.success());
    assert_eq!(routed, std::fs::read(&unrouted).unwrap());
}

#[test]
fn no_step_and_the_manifest_key_leave_the_step_out_and_remove_an_old_one() {
    let dir = netsmith_testkit::TempDir::new("fab-step-off");
    std::fs::write(dir.join("a.nsm"), NOTHING_TO_ROUTE).unwrap();
    let step = dir.join("out/Lonely/Lonely.step");
    let ok = |run: Output| {
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    };
    ok(fab(&dir, &["a.nsm", "--out", "out"]));
    assert!(step.is_file());

    // `--no-step` leaves it out, removes the one of the earlier run, and keeps the `.glb`.
    ok(fab(&dir, &["a.nsm", "--out", "out", "--no-step"]));
    assert!(!step.exists());
    assert!(dir.join("out/Lonely/Lonely.glb").is_file());
    assert!(dir.join("out/Lonely/fab-notes.txt").is_file());

    // `--no-glb` does not touch the STEP.
    ok(fab(&dir, &["a.nsm", "--out", "out", "--no-glb"]));
    assert!(step.is_file());
    assert!(!dir.join("out/Lonely/Lonely.glb").exists());

    // `step = false` in netsmith.toml does the same for the project, in `fab` and in an order.
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.nsm"), NOTHING_TO_ROUTE).unwrap();
    std::fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"p\"\nstep = false\n\n[order.one.boards]\nLonely = 1\n",
    )
    .unwrap();
    ok(fab(&dir, &["--out", "kept-out"]));
    assert!(!dir.join("kept-out/Lonely/Lonely.step").exists());
    assert!(dir.join("kept-out/Lonely/Lonely.glb").is_file());
    ok(fab(&dir, &["--order", "one", "--out", "order-out"]));
    let written = walk(&dir.join("order-out"));
    assert!(written.iter().any(|f| f.ends_with("Lonely.kicad_pcb")));
    assert!(!written.iter().any(|f| f.ends_with(".step")), "{written:?}");

    // Without the key the order has it, and the flag leaves it out.
    std::fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"p\"\n\n[order.one.boards]\nLonely = 1\n",
    )
    .unwrap();
    ok(fab(&dir, &["--order", "one", "--out", "order-on"]));
    assert!(
        walk(&dir.join("order-on"))
            .iter()
            .any(|f| f.ends_with("Lonely.step"))
    );
    ok(fab(
        &dir,
        &["--order", "one", "--out", "order-flag", "--no-step"],
    ));
    assert!(
        !walk(&dir.join("order-flag"))
            .iter()
            .any(|f| f.ends_with(".step"))
    );
}

#[test]
fn a_manifest_step_that_is_not_a_boolean_is_an_error() {
    let dir = netsmith_testkit::TempDir::new("fab-step-bad-key");
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.nsm"), NOTHING_TO_ROUTE).unwrap();
    std::fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"p\"\nstep = \"no\"\n",
    )
    .unwrap();
    let run = fab(&dir, &["--out", "out"]);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("step"));
}

/// Every file under `dir`, as paths.
fn walk(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path.display().to_string());
        }
    }
    found
}

#[test]
fn a_manifest_glb_that_is_not_a_boolean_is_an_error() {
    let dir = netsmith_testkit::TempDir::new("fab-glb-bad-key");
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.nsm"), NOTHING_TO_ROUTE).unwrap();
    std::fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"p\"\nglb = \"no\"\n",
    )
    .unwrap();
    let run = fab(&dir, &["--out", "out"]);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("glb"));
}

/// The layers of every board's stackup add up to within the fabricator's tolerance of the
/// thickness it chose (1.6 mm nominal; JLCPCB's four-layer stack is 1.6062 mm with the mask).
#[test]
fn the_stackup_adds_up_to_the_nominal_thickness_within_the_fabricators_tolerance() {
    let examples = repo().join("examples");
    for file in ["led.nsm", "four_layer.nsm"] {
        let (report, _) = json(&examples, &[file, "--json"]);
        let board = &report["board"];
        let nominal = board["thickness"].as_f64().unwrap();
        let sum: f64 = board["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["thickness"].as_f64().unwrap())
            .sum();
        assert!(
            (board["stack_thickness"].as_f64().unwrap() - sum).abs() < 1e-9,
            "{file}: stack_thickness is the sum of the layers"
        );
        let tolerance = netsmith_pcb::stackup::thickness_tolerance(nominal);
        assert!(
            (sum - nominal).abs() <= tolerance,
            "{file}: the layers add up to {sum} mm, more than {tolerance} mm from {nominal} mm"
        );
    }
}

/// The system scene of the two-board example (ADR 0110): the LED board is placed on the power
/// board by the mate of its socket with the power board's header, at the mated height the two
/// seats give, and each member's report is the one `--board TwoBoards/member` prints.
#[test]
fn the_two_boards_are_mated_at_the_height_their_seats_give() {
    let examples = repo().join("examples");
    let (r, text) = json(
        &examples,
        &["two_boards.nsm", "--board", "TwoBoards", "--json"],
    );
    assert_eq!(r["schema"], 1);
    assert_eq!(r["system"], "TwoBoards");
    assert_eq!(r["base"], "power");
    let mate = &r["mates"][0];
    assert_eq!(mate["a"], "power.power_out");
    assert_eq!(mate["b"], "led.power_in");
    assert_eq!(mate["contacts"], 2);
    assert_eq!(mate["mated_height"]["min"], 10.65);
    assert_eq!(mate["mated_height"]["max"], 11.35);
    assert_eq!(mate["seats"]["a"]["min"], 2.3);
    assert_eq!(mate["seats"]["b"]["max"], 8.65);
    assert_eq!(mate["residual"], 0.0);
    assert_eq!(mate["gap"], 10.65);
    let members = r["members"].as_array().unwrap();
    assert_eq!(members.len(), 2);
    assert_eq!(members[0]["member"], "power");
    assert_eq!(members[0]["placed"]["base"], true);
    assert_eq!(
        members[0]["motion"]["offset"],
        serde_json::json!([0.0, 0.0, 0.0])
    );
    let led = &members[1];
    assert_eq!(led["member"], "led");
    assert_eq!(led["board"], "TwoBoards/led");
    assert_eq!(led["placed"]["mate"], 0);
    // Both connectors stand on their boards' top faces, so the LED board hangs face down, its
    // top face at the least mated height.
    assert_eq!(led["motion"]["flip"], true);
    assert_eq!(led["motion"]["matrix"][2][3], 10.65);
    assert_eq!(led["motion"]["matrix"][2][2], -1.0);
    // The member's report is its own command's.
    let (alone, _) = json(
        &examples,
        &["two_boards.nsm", "--board", "TwoBoards/led", "--json"],
    );
    assert_eq!(led["report"], alone);
    // Nothing collides, and nothing of unknown height faces the other board.
    let findings = r["findings"].as_array().unwrap();
    assert!(
        findings.iter().all(|f| f["severity"] != "error"),
        "{findings:?}"
    );
    // The same bytes each run.
    let (_, again) = json(
        &examples,
        &["two_boards.nsm", "--board", "TwoBoards", "--json"],
    );
    assert_eq!(text, again);
    // The summary says how each member is placed.
    let out = board3d(&examples, &["two_boards.nsm", "--board", "TwoBoards"]);
    let summary = String::from_utf8(out.stdout).unwrap();
    assert!(
        summary
            .contains("member `led`: placed by the mate of `power.power_out` and `led.power_in`"),
        "{summary}"
    );
    assert!(
        summary.contains("mated height 10.65 to 11.35 mm"),
        "{summary}"
    );
}

/// A point of the scene (mm, z up) as glTF writes it (m, y up).
fn gltf_point(p: [f64; 3]) -> [f64; 3] {
    [p[0] * 0.001, p[2] * 0.001, -p[1] * 0.001]
}

/// The system scene's `.glb` (ADR 0110): the root `system`, one node per member carrying its
/// motion as a matrix in glTF's frame, and under it the board's subtree with every name starting
/// with the member. The STEP has a product per member, placed by a transformation. Both are the
/// same bytes each run.
#[test]
fn the_system_is_written_as_glb_and_step_with_a_node_and_a_product_per_member() {
    let examples = repo().join("examples");
    let dir = netsmith_testkit::TempDir::new("system-glb");
    let (glb, step) = (dir.join("s.glb"), dir.join("s.step"));
    let write = |glb: &Path, step: &Path| {
        let run = board3d(
            &examples,
            &[
                "two_boards.nsm",
                "--board",
                "TwoBoards",
                "--glb",
                glb.to_str().unwrap(),
                "--step",
                step.to_str().unwrap(),
            ],
        );
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        // A file asked for, and nothing printed.
        assert!(run.stdout.is_empty());
    };
    write(&glb, &step);
    let bytes = std::fs::read(&glb).unwrap();
    let doc = glb_json(&bytes);
    let nodes = doc["nodes"].as_array().unwrap();
    let root = &nodes[doc["scenes"][0]["nodes"][0].as_u64().unwrap() as usize];
    assert_eq!(root["name"], "system");
    assert_eq!(root["extras"]["netsmith"]["kind"], "system");
    assert_eq!(root["extras"]["netsmith"]["base"], "power");
    assert_eq!(
        root["extras"]["netsmith"]["mates"][0]["mated_height_mm"]["min"],
        10.65
    );
    let members: Vec<&serde_json::Value> = root["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| &nodes[i.as_u64().unwrap() as usize])
        .collect();
    assert_eq!(
        members
            .iter()
            .map(|m| m["name"].clone())
            .collect::<Vec<_>>(),
        ["power", "led"]
    );
    assert_eq!(members[1]["extras"]["netsmith"]["kind"], "member");
    assert_eq!(members[1]["extras"]["netsmith"]["flip"], true);
    // The matrix is the motion in glTF's frame: a point of the LED board's own frame, moved by
    // the node's matrix, is where the motion puts it, written as glTF writes a point.
    let m: Vec<f64> = members[1]["matrix"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let apply = |p: [f64; 3]| {
        [0, 1, 2].map(|r| (0..3).map(|c| m[c * 4 + r] * p[c]).sum::<f64>() + m[12 + r])
    };
    // The motion is a half turn with a flip, offset (25, 2.1, 10.65) mm: (x, y, z) goes to
    // (25 - x, 2.1 + y, 10.65 - z).
    for p in [[0.0, 0.0, 0.0], [13.0, 8.0, -1.6], [7.5, 2.75, 0.0]] {
        let want = gltf_point([25.0 - p[0], 2.1 + p[1], 10.65 - p[2]]);
        let got = apply(gltf_point(p));
        for k in 0..3 {
            assert!(
                (got[k] - want[k]).abs() < 1e-6,
                "{p:?}: {got:?} != {want:?}"
            );
        }
    }
    // Under the member, its board's subtree as the board alone has it, every name prefixed.
    let board = &nodes[members[1]["children"][0].as_u64().unwrap() as usize];
    assert_eq!(board["name"], "led/board");
    assert_eq!(board["extras"]["netsmith"]["board"], "TwoBoards/led");
    let part = glb_node(&doc, "led/J1");
    assert_eq!(part["extras"]["netsmith"]["member"], "led");
    assert_eq!(part["extras"]["netsmith"]["path"], "power_in");
    assert!(glb_node(&doc, "power/J2")["extras"]["netsmith"]["member"] == "power");

    let text = std::fs::read_to_string(&step).unwrap();
    for product in [
        "PRODUCT('TwoBoards','TwoBoards'",
        "PRODUCT('TwoBoards/power','TwoBoards/power'",
        "PRODUCT('TwoBoards/led','TwoBoards/led'",
        "PRODUCT('led/power_in','J1'",
        "PRODUCT('power/board','board'",
    ] {
        assert!(text.contains(product), "no {product}");
    }
    // One transformation per member, and one per part and board inside each.
    let placements = text.matches("ITEM_DEFINED_TRANSFORMATION").count();
    let parts = text.matches("NEXT_ASSEMBLY_USAGE_OCCURRENCE").count();
    assert_eq!(placements, parts);
    assert!(text.contains("NEXT_ASSEMBLY_USAGE_OCCURRENCE('TwoBoards/led','led'"));

    // The same bytes each run.
    let (glb2, step2) = (dir.join("t.glb"), dir.join("t.step"));
    write(&glb2, &step2);
    assert_eq!(bytes, std::fs::read(&glb2).unwrap());
    assert_eq!(text, std::fs::read_to_string(&step2).unwrap());
}

/// `render --board System` draws the system scene, the same bytes each run, and lights a part
/// named `member/<id>` (a net also `net:member/<name>`); a member the system does not have is an
/// error that names its members.
#[test]
fn render_draws_the_system_and_lights_a_part_of_a_member() {
    let examples = repo().join("examples");
    let dir = netsmith_testkit::TempDir::new("system-render");
    let draw = |name: &str, extra: &[&str]| {
        let out = dir.join(name);
        let mut args = vec![
            "render",
            "two_boards.nsm",
            "--board",
            "TwoBoards",
            "--size",
            "320x200",
            "--out",
            out.to_str().unwrap(),
        ];
        args.extend_from_slice(extra);
        let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
            .args(&args)
            .current_dir(&examples)
            .env("NETSMITH_NO_KICAD", "1")
            .output()
            .unwrap();
        (run, out)
    };
    let (run, a) = draw("a.png", &[]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let (_, b) = draw("b.png", &[]);
    assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    let (run, lit) = draw("lit.png", &["--highlight", "led/J1"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_ne!(std::fs::read(&a).unwrap(), std::fs::read(&lit).unwrap());
    // A member's net, named either way: `led/net:GND`, or `net:led/GND` as the editor names it.
    let (run, net) = draw("net.png", &["--highlight", "led/net:GND"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let (run, same) = draw("same.png", &["--highlight", "net:led/GND"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_ne!(std::fs::read(&a).unwrap(), std::fs::read(&net).unwrap());
    assert_eq!(std::fs::read(&net).unwrap(), std::fs::read(&same).unwrap());
    let (run, _) = draw("bad.png", &["--highlight", "net:lamp/GND"]);
    assert_eq!(run.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&run.stderr).contains("no member `lamp`"));
    let (run, _) = draw("bad.png", &["--highlight", "lamp/J1"]);
    assert_eq!(run.status.code(), Some(2));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains("no member `lamp`") && err.contains("power, led"),
        "{err}"
    );
}

/// `fab` of a system writes the system's `.glb` and `.step` into the system's folder, from its
/// boards as routed (so a via is a hole in the STEP's board, which `board3d` does not route), the
/// same bytes each run and the same products as `board3d --step`; `--no-glb` and `--no-step`
/// leave them out and remove an earlier run's.
#[test]
fn fab_writes_the_system_scene_beside_its_boards_and_the_switches_leave_it_out() {
    let examples = repo().join("examples");
    let dir = netsmith_testkit::TempDir::new("system-fab");
    let out = dir.join("out");
    let made = |extra: &[&str]| {
        let mut args = vec!["two_boards.nsm", "--out", out.to_str().unwrap(), "--locked"];
        args.extend_from_slice(extra);
        let run = fab(&examples, &args);
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
    };
    made(&[]);
    let folder = out.join("TwoBoards");
    let glb = std::fs::read(folder.join("TwoBoards.glb")).expect("fab writes the system's .glb");
    assert_eq!(glb_json(&glb)["nodes"][0]["name"], "system");
    let step = std::fs::read_to_string(folder.join("TwoBoards.step")).expect("and its .step");
    made(&[]);
    assert_eq!(glb, std::fs::read(folder.join("TwoBoards.glb")).unwrap());
    assert_eq!(
        step,
        std::fs::read_to_string(folder.join("TwoBoards.step")).unwrap()
    );

    let by_board3d = dir.join("b.step");
    let run = board3d(
        &examples,
        &[
            "two_boards.nsm",
            "--board",
            "TwoBoards",
            "--step",
            by_board3d.to_str().unwrap(),
        ],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let products = |text: &str| -> Vec<String> {
        text.lines()
            .filter_map(|l| l.split_once("=PRODUCT(").map(|(_, p)| p.to_string()))
            .collect()
    };
    assert_eq!(
        products(&step),
        products(&std::fs::read_to_string(&by_board3d).unwrap())
    );

    made(&["--no-glb", "--no-step"]);
    assert!(!folder.join("TwoBoards.glb").exists());
    assert!(!folder.join("TwoBoards.step").exists());
}

/// A header too tall for the stack reaches into the board mated on it: `system_collision`, an
/// error that stops `fab` whatever `--allow-drc-violations` says, and no system file is written.
#[test]
fn a_header_taller_than_the_mated_height_stops_fab_of_the_system() {
    let dir = netsmith_testkit::TempDir::new("system-collision");
    let source = std::fs::read_to_string(repo().join("examples/two_boards.nsm"))
        .unwrap()
        .replacen(
            "    .power_out: Header1x2\n",
            "    .power_out: Header1x2 { .height &= 12mm }\n",
            1,
        );
    std::fs::write(dir.join("c.nsm"), source).unwrap();
    let report = board3d(&dir, &["c.nsm", "--board", "TwoBoards", "--json"]);
    assert!(report.status.success());
    let r: serde_json::Value = serde_json::from_slice(&report.stdout).unwrap();
    let f = &r["findings"][0];
    assert_eq!(f["code"], "system_collision");
    assert_eq!(f["severity"], "error");
    assert_eq!(f["parts"], serde_json::json!(["power/power_out", "led/"]));
    assert_eq!(
        (f["z_bottom"].as_f64(), f["z_top"].as_f64()),
        (Some(10.65), Some(12.0))
    );

    let run = fab(&dir, &["c.nsm", "--out", "out", "--allow-drc-violations"]);
    assert_eq!(run.status.code(), Some(1));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains("error: TwoBoards: system_collision: the body of J2 (`power/power_out`) and the board `led`")
            && err.contains("does not waive it"),
        "{err}"
    );
    assert!(!dir.join("out/TwoBoards/TwoBoards.glb").exists());
}
