//! The STEP model of a part on disk (`.model`, ADR 0108), as `check` reports it: a file that is
//! not there is a warning, and a path outside the project stops the file.

use std::{fs, path::Path, process::Command};

use netsmith_kicad::testlib::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

/// A project with the LED board in `src/main.nsm` and `physical` as its physical file.
fn project(label: &str, physical: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    fs::write(
        root.join("src/main.physical.nsm"),
        format!("physical for LedExample {{\n{physical}\n}}\n"),
    )
    .unwrap();
    root
}

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env(
            "NETSMITH_KICAD_SYMBOLS",
            netsmith_kicad::testlib::sample_symbols(),
        )
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_kicad::testlib::sample_footprints(),
        )
        .output()
        .unwrap()
}

#[test]
fn check_warns_of_a_missing_model_and_refuses_one_outside_the_project() {
    let root = project(
        "model-check-missing",
        "    .power_in.model.file &= \"../models/header.step\"",
    );
    let run = netsmith(&root, &["check", "src/main.nsm"]);
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{err}");
    assert!(
        err.contains("warning: LedExample: `power_in.model.file` names `models/header.step`"),
        "{err}"
    );

    let root = project(
        "model-check-outside",
        "    .power_in.model.file &= \"../../header.step\"",
    );
    let run = netsmith(&root, &["check", "src/main.nsm"]);
    let err = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "{err}");
    assert!(
        err.contains("error: LedExample: `power_in.model.file` is `../../header.step`"),
        "{err}"
    );
}

const BLOCKS: &[u8] = include_bytes!("../../netsmith-3d/tests/models/blocks.step");

/// `board3d --json` of `root`'s board, its stderr, and the parsed report.
fn board3d(root: &Path, extra: &[&str]) -> (String, serde_json::Value) {
    let mut args = vec!["board3d", "src/main.nsm", "--json"];
    args.extend(extra);
    let run = netsmith(root, &args);
    let err = String::from_utf8_lossy(&run.stderr).into_owned();
    assert!(run.status.success(), "{err}");
    let report = serde_json::from_slice(&run.stdout).unwrap_or_else(|e| panic!("{e}: {err}"));
    (err, report)
}

fn part<'a>(report: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    report["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["path"] == path)
        .unwrap_or_else(|| panic!("no part {path}"))
}

#[test]
fn board3d_lists_the_model_a_part_is_drawn_from_and_the_glb_records_its_sha256() {
    let root = project(
        "model-board3d",
        "    .power_in.model.file &= \"../models/blocks.step\"\n    .power_in.model.rz &= 90",
    );
    fs::create_dir_all(root.join("models")).unwrap();
    fs::write(root.join("models/blocks.step"), BLOCKS).unwrap();
    let sha = netsmith::models::sha256(BLOCKS);
    let (err, report) = board3d(&root, &["--glb", "out.glb"]);
    assert!(!err.contains("warning"), "{err}");
    let j1 = part(&report, "power_in");
    assert_eq!(j1["model"]["file"], "../models/blocks.step");
    assert_eq!(j1["model"]["sha256"], sha.as_str());
    assert_eq!(j1["model"]["top"], 0.6);
    assert_eq!(report["warnings"], serde_json::json!([]));
    // Nothing that is measured moves: the height and its provenance are the part's own.
    let (_, plain) = board3d(&project("model-board3d-plain", ""), &[]);
    for key in ["height", "provenance", "z_top", "body"] {
        assert_eq!(j1[key], part(&plain, "power_in")[key], "{key}");
    }
    let glb = fs::read(root.join("out.glb")).unwrap();
    let doc = netsmith_3d::gltf::json_chunk(&glb).unwrap();
    let node = doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| {
            n["extras"]["netsmith"]["path"] == "power_in"
                && n["extras"]["netsmith"]["kind"] == "part"
        })
        .unwrap();
    assert_eq!(node["extras"]["netsmith"]["model"]["sha256"], sha.as_str());
}

#[test]
fn a_model_that_cannot_be_read_or_stands_too_tall_is_a_warning() {
    let root = project(
        "model-board3d-bad",
        "    .power_in.model.file &= \"../models/bad.step\"",
    );
    fs::create_dir_all(root.join("models")).unwrap();
    fs::write(root.join("models/bad.step"), "not a step file").unwrap();
    let (err, report) = board3d(&root, &[]);
    assert!(
        err.contains("`power_in.model.file` names `../models/bad.step` (src/main.physical.nsm:2), which netsmith cannot read as a STEP model (not a STEP file"),
        "{err}"
    );
    assert!(part(&report, "power_in").get("model").is_none());
    assert_eq!(report["warnings"].as_array().unwrap().len(), 1);

    let root = project(
        "model-board3d-tall",
        "    .power_in.model.file &= \"../models/blocks.step\"\n    .power_in.model.z &= 20mm",
    );
    fs::create_dir_all(root.join("models")).unwrap();
    fs::write(root.join("models/blocks.step"), BLOCKS).unwrap();
    let (err, report) = board3d(&root, &[]);
    assert!(
        err.contains("warning: LedExample: the model of `power_in`"),
        "{err}"
    );
    assert!(err.contains("`.power_in.height &= 20.60mm`"), "{err}");
    assert_eq!(part(&report, "power_in")["model"]["top"], 20.6);
}

#[test]
fn a_member_of_a_system_is_drawn_from_its_model_in_the_system_scene() {
    let root = TempDir::new("model-system");
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for f in ["two_boards.nsm", "two_boards.lock.toml"] {
        fs::copy(examples.join(f), root.join(f)).unwrap();
    }
    fs::write(
        root.join("two_boards.physical.nsm"),
        "physical for LedBoard {\n    .power_in.model.file &= \"blocks.step\"\n}\n",
    )
    .unwrap();
    fs::write(root.join("blocks.step"), BLOCKS).unwrap();
    let run = netsmith(
        &root,
        &[
            "board3d",
            "two_boards.nsm",
            "--board",
            "TwoBoards",
            "--json",
        ],
    );
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{err}");
    let r: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let led = r["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["member"] == "led")
        .unwrap();
    let sha = netsmith::models::sha256(BLOCKS);
    assert_eq!(
        part(&led["report"], "power_in")["model"]["sha256"],
        sha.as_str()
    ); // The system's .glb and STEP draw it too.
    let run = netsmith(
        &root,
        &[
            "board3d",
            "two_boards.nsm",
            "--board",
            "TwoBoards",
            "--glb",
            "s.glb",
            "--step",
            "s.step",
        ],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let doc = netsmith_3d::gltf::json_chunk(&fs::read(root.join("s.glb")).unwrap()).unwrap();
    assert!(
        doc["materials"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["name"] == "model:#d91a1a")
    );
    let step = fs::read_to_string(root.join("s.step")).unwrap();
    assert!(step.contains(&format!("drawn from blocks.step (sha256 {sha})")));
}

#[test]
fn fab_draws_the_model_in_the_glb_and_the_step_and_names_it_in_the_notes() {
    let root = project(
        "model-fab",
        "    .power_in.model.file &= \"../models/blocks.step\"",
    );
    fs::create_dir_all(root.join("models")).unwrap();
    fs::write(root.join("models/blocks.step"), BLOCKS).unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["fab", "src/main.nsm", "--out", "out"])
        .current_dir(&root)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{err}");
    let package = root.join("out/LedExample");
    let notes = fs::read_to_string(package.join("fab-notes.txt")).unwrap();
    let sha = netsmith::models::sha256(BLOCKS);
    assert!(
        notes.contains(&format!(
            "note: drawn from a STEP model in the .glb and the .step (never measured: heights are the parts' own): J1 (`power_in`) from `../models/blocks.step` (sha256 {sha})"
        )),
        "{notes}"
    );
    let step = fs::read_to_string(package.join("LedExample.step")).unwrap();
    assert!(step.contains(&format!("drawn from ../models/blocks.step (sha256 {sha})")));
    assert!(step.contains("PRODUCT('power_in:body','body'"));
    let glb = fs::read(package.join("LedExample.glb")).unwrap();
    let doc = netsmith_3d::gltf::json_chunk(&glb).unwrap();
    let names: Vec<&str> = doc["materials"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"model:#d91a1a"), "{names:?}");
}
