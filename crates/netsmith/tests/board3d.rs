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
    assert_eq!(r["schema"], 1);
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
