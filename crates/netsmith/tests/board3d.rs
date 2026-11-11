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
