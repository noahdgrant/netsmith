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
        serde_json::json!({"known": 2, "total": 3, "stated": 2, "catalog": 0})
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
fn scene_of(source: &str) -> netsmith_pcb::scene3d::Scene {
    let path = repo().join(source);
    let target =
        netsmith::project::Target::resolve_from(&repo(), Some(path.to_str().unwrap())).unwrap();
    let file = &target.files[0];
    let text = std::fs::read_to_string(&file.path).unwrap();
    let catalogs = target.catalogs().unwrap();
    let (held, _) = netsmith::pick_lock::open(&file.path, &catalogs, false).unwrap();
    let nowhere = netsmith_kicad::testlib::TempDir::new("render-no-kicad");
    let search = netsmith_kicad::install::Search::under(nowhere.path()).with_no_kicad();
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
fn render(scene: &netsmith_pcb::scene3d::Scene, name: &str, dir: &Path) -> Vec<u8> {
    use netsmith_pcb::raster3d::{RenderOptions, View, render_view};
    let meshes = netsmith_pcb::mesh::of_scene(scene);
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
    use netsmith_pcb::mesh::{Material, of_scene};
    let scene = scene_of("examples/led.nsm");
    let meshes = of_scene(&scene);
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
    let dir = netsmith_kicad::testlib::TempDir::new("render-led");
    let a = render(&scene, "led", dir.path());
    let b = render(&scene_of("examples/led.nsm"), "led", dir.path());
    assert_eq!(a, b);
    assert_eq!(of_scene(&scene), of_scene(&scene_of("examples/led.nsm")));
}

#[test]
fn the_four_layer_example_draws_its_copper_mask_and_silk_per_net() {
    use netsmith_pcb::mesh::{Material, Mesh, of_scene};
    let scene = scene_of("examples/four_layer.nsm");
    let meshes = of_scene(&scene);
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
        .map(|n| n.unwrap_or(netsmith_pcb::mesh::NO_NET).to_string())
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
    use netsmith_pcb::mesh::{Material, of_scene};
    let scene = scene_of("examples/four_layer.nsm");
    let meshes = of_scene(&scene);
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
        let bb = netsmith_pcb::mesh::bounds_of(&mine).unwrap();
        let (lo, hi) = b.z_range();
        assert!(
            (bb.min[2] - lo.unwrap()).abs() < 1e-9 && (bb.max[2] - hi.unwrap()).abs() < 1e-9,
            "{} ({h} mm): {bb:?}",
            b.path
        );
    }
    let dir = netsmith_kicad::testlib::TempDir::new("render-four-layer");
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
fn a_limit_a_part_breaks_is_drawn_as_broken() {
    use netsmith_pcb::mesh::{Material, of_scene};
    let scene = scene_of("scripts/editor-verify-js/fixtures/height_limits.nsm");
    let meshes = of_scene(&scene);
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
    let dir = netsmith_kicad::testlib::TempDir::new("render-lidded");
    render(&scene, "lidded", dir.path());
}

/// The JSON chunk of a `.glb` file.
fn glb_json(glb: &[u8]) -> serde_json::Value {
    netsmith_pcb::gltf::json_chunk(glb).expect("a GLB file")
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
    let out = netsmith_kicad::testlib::TempDir::new("board3d-glb");
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
    assert!(String::from_utf8_lossy(&run.stderr).contains("--glb needs a file"));
}

#[test]
fn an_unknown_height_is_flat_in_the_glb_and_the_limit_it_breaks_is_marked() {
    let out = netsmith_kicad::testlib::TempDir::new("board3d-glb-lid");
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
    let dir = netsmith_kicad::testlib::TempDir::new("board3d-editor-glb");
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
