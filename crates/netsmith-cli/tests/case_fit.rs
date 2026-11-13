//! A board's fit in its case models (ADR 0109), as `board3d` and `fab` report it, on the
//! editor's fixture `case_fit.nsm`: a 40 by 24 mm board in a box of hand-written STEP files
//! (`case_fit/base.step`, a floor, four walls and a boss under `hole1`; `case_fit/lid.step`).
//! `tall` is 9 mm under a lid at 8 mm, `bare` has no height, `edge` stands 0.83 mm from the
//! right wall, and `hole2` has no boss.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_kicad::testlib::TempDir;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/editor-verify-js/fixtures")
}

/// The fixture in a folder of its own, with `edit` applied to its physical file.
fn project(label: &str, edit: impl Fn(String) -> String) -> TempDir {
    let dir = TempDir::new(label);
    for name in ["case_fit.nsm", "case_fit.pcb.toml"] {
        std::fs::copy(fixtures().join(name), dir.join(name)).unwrap();
    }
    std::fs::create_dir_all(dir.join("case_fit")).unwrap();
    for name in ["base.step", "lid.step", "open.step"] {
        std::fs::copy(
            fixtures().join("case_fit").join(name),
            dir.join("case_fit").join(name),
        )
        .unwrap();
    }
    let physical = std::fs::read_to_string(fixtures().join("case_fit.physical.nsm")).unwrap();
    std::fs::write(dir.join("case_fit.physical.nsm"), edit(physical)).unwrap();
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn report(dir: &Path) -> serde_json::Value {
    let run = netsmith(dir, &["board3d", "case_fit.nsm", "--json"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    serde_json::from_slice(&run.stdout).unwrap()
}

fn findings(r: &serde_json::Value) -> Vec<(String, String, String)> {
    r["case_findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["kind"].as_str().unwrap().to_string(),
                f["severity"].as_str().unwrap().to_string(),
                f["path"]
                    .as_str()
                    .unwrap_or(f["subject"].as_str().unwrap())
                    .to_string(),
            )
        })
        .collect()
}

fn s(a: &str, b: &str, c: &str) -> (String, String, String) {
    (a.into(), b.into(), c.into())
}

#[test]
fn board3d_lists_the_case_models_and_every_finding_with_its_numbers() {
    let dir = project("case-fit-board3d", |p| p);
    let r = report(&dir);
    let models = r["case_models"].as_array().unwrap();
    assert_eq!(models.len(), 2);
    let base = &models[0];
    assert_eq!(base["name"], "base");
    assert_eq!(base["file"], "case_fit/base.step");
    assert_eq!(base["status"], "read");
    assert_eq!(base["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(base["offset"], serde_json::json!([-2.3, -2.3, -6.0]));
    assert_eq!(base["clearance"], 1.0, "the physical file narrows it");
    assert_eq!(base["reach"], 1.5, "std's default");
    assert_eq!(base["triangles"], 72);
    let floor = &base["solids"][0];
    assert_eq!(floor["name"], "floor");
    assert_eq!(floor["closed"], true);
    assert_eq!(floor["min"], serde_json::json!([-2.3, -2.3, -8.0]));
    assert_eq!(models[1]["name"], "lid");
    assert_eq!(models[1]["clearance"], 0.5);

    assert_eq!(
        findings(&r),
        [
            s("case_unchecked", "warning", "bare"),
            s("case_clearance", "warning", "edge"),
            s("case_collision", "error", "tall"),
            s("case_reach", "warning", "edge"),
            s("case_boss", "warning", "hole2"),
        ]
    );
    let all = r["case_findings"].as_array().unwrap();
    assert_eq!(all[0]["strict_error"], true);
    assert_eq!(all[1]["gap"], 0.83);
    assert_eq!(all[1]["limit"], 1.0);
    assert_eq!(all[1]["solid"], "wall_right");
    let tall = &all[2];
    assert_eq!(tall["depth"], 1.0);
    assert_eq!(tall["case"], "lid");
    assert_eq!(tall["refdes"], "J1");
    assert_eq!(tall["at"][2], 8.0);
    assert!(
        tall["message"]
            .as_str()
            .unwrap()
            .contains("is inside the solid `lid` of the case model `lid` by 1.00 mm"),
        "{tall}"
    );
    assert_eq!(all[3]["edge"], "right");
    assert_eq!(all[3]["distance"], 0.83);
    assert_eq!(all[4]["case"], "base, lid");

    // The same bytes for the same source, and the summary names them.
    let again = netsmith(&dir, &["board3d", "case_fit.nsm", "--json"]);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&again.stdout).unwrap(),
        r
    );
    let summary = netsmith(&dir, &["board3d", "case_fit.nsm"]);
    let text = String::from_utf8_lossy(&summary.stdout);
    assert!(
        text.contains("case model `base` (`case_fit/base.step`): 6 solid(s), 72 triangles"),
        "{text}"
    );
    assert!(
        text.contains("error: case_collision: J1 (`tall`)"),
        "{text}"
    );
}

#[test]
fn fab_refuses_a_collision_with_the_case_whatever_the_flags_say() {
    let dir = project("case-fit-fab-collision", |p| p);
    for flags in [
        &["--allow-drc-violations"][..],
        &["--constraints", "warn", "--allow-drc-violations"][..],
    ] {
        let mut args = vec!["fab", "case_fit.nsm", "--out", "fab"];
        args.extend_from_slice(flags);
        let run = netsmith(&dir, &args);
        let err = String::from_utf8_lossy(&run.stderr);
        assert_eq!(run.status.code(), Some(1), "{flags:?}: {err}");
        assert!(
            err.contains("error: Cased: case_collision: J1 (`tall`), 9.00 mm tall (stated), is inside the solid `lid`")
                && err.contains("neither `--allow-drc-violations` nor `--constraints warn` waives it"),
            "{err}"
        );
        assert!(!dir.join("fab/Cased/fab-notes.txt").exists());
    }
}

#[test]
fn fab_writes_the_case_findings_in_the_notes_and_strict_constraints_refuse_an_unchecked_fit() {
    let lower = |p: String| p.replace(".tall.height &= 9mm", ".tall.height &= 5mm");
    let dir = project("case-fit-fab-notes", lower);
    let run = netsmith(&dir, &["fab", "case_fit.nsm", "--out", "fab"]);
    let out =
        String::from_utf8_lossy(&run.stdout).to_string() + &String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{out}");
    assert!(
        out.contains("Cased: warning: case_boss: the mounting hole H2 (`hole2`)"),
        "{out}"
    );
    let notes = std::fs::read_to_string(dir.join("fab/Cased/fab-notes.txt")).unwrap();
    for line in [
        "note: case model `base` (`case_fit/base.step`, SHA-256 ",
        "note: case model `lid` (`case_fit/lid.step`, SHA-256 ",
        "warning: case_unchecked: J3 (`bare`) has no height and stands under the solid `lid`",
        "warning: case_clearance: J2 (`edge`) is 0.83 mm from the solid `wall_right`",
        "warning: case_reach: J2 (`edge`), a connector at the board's right edge",
        "warning: case_boss: the mounting hole H2 (`hole2`)",
    ] {
        assert!(notes.contains(line), "{line}\n{notes}");
    }
    // The STEP export leaves the case out (the owner's choice).
    let step = std::fs::read_to_string(dir.join("fab/Cased/Cased.step")).unwrap();
    assert!(!step.contains("'lid'") && !step.contains("wall_right"));

    let dir = project("case-fit-fab-strict", lower);
    let run = netsmith(
        &dir,
        &[
            "fab",
            "case_fit.nsm",
            "--out",
            "fab",
            "--constraints",
            "error",
        ],
    );
    let err = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "{err}");
    assert!(
        err.contains("error: Cased: case_unchecked: J3 (`bare`) has no height")
            && err.contains("this is an error because constraints are strict"),
        "{err}"
    );
}

#[test]
fn a_missing_case_file_is_reported_and_the_rest_is_checked() {
    let dir = project("case-fit-missing", |p| {
        p.replace("case_fit/lid.step", "case_fit/no_lid.step")
    });
    let r = report(&dir);
    assert_eq!(r["case_models"][1]["status"], "missing");
    assert!(r["case_models"][1]["sha256"].is_null());
    let f = &r["case_findings"][0];
    assert_eq!(f["kind"], "case_unchecked");
    assert_eq!(f["subject"], "model");
    assert_eq!(f["case"], "lid");
    assert!(f["message"].as_str().unwrap().contains(
        "there is no such file, so the board's fit in its case model `lid` is not checked"
    ));
    // With no lid, the tall header and the one of unknown height are under nothing.
    let kinds = findings(&r);
    assert!(!kinds.iter().any(|k| k.0 == "case_collision"), "{kinds:?}");
}

#[test]
fn an_open_shell_is_measured_by_its_surface_and_says_it_gets_no_inside_test() {
    let dir = project("case-fit-open", |p| {
        p.replace("case_fit/lid.step", "case_fit/open.step")
    });
    let r = report(&dir);
    let solids = &r["case_models"][1]["solids"];
    assert_eq!(solids[0]["name"], "shelf");
    assert_eq!(solids[0]["closed"], false);
    let f = &r["case_findings"][0];
    assert_eq!(f["kind"], "case_unchecked");
    assert_eq!(f["subject"], "solid");
    assert_eq!(f["solid"], "shelf");
    assert!(
        f["message"]
            .as_str()
            .unwrap()
            .contains("is not a closed shell")
    );
    // Its surface is still there: the tall header reaches through its underside.
    assert!(findings(&r).contains(&s("case_collision", "error", "tall")));
}

#[test]
fn a_case_file_the_reader_cannot_read_stops_fab() {
    let dir = project("case-fit-unreadable", |p| {
        p.replace(".tall.height &= 9mm", ".tall.height &= 5mm")
    });
    std::fs::write(dir.join("case_fit/lid.step"), "not a STEP file\n").unwrap();
    let r = report(&dir);
    assert_eq!(r["case_models"][1]["status"], "unreadable");
    assert_eq!(r["case_findings"][0]["severity"], "error");
    let run = netsmith(&dir, &["fab", "case_fit.nsm", "--out", "fab"]);
    let err = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "{err}");
    assert!(
        err.contains("netsmith's STEP reader cannot read it")
            && err.contains("a case model netsmith cannot read stops `fab`"),
        "{err}"
    );
}

#[test]
fn the_glb_holds_the_case_and_render_lights_it() {
    let dir = project("case-fit-glb", |p| p);
    let glb = dir.join("cased.glb");
    let run = netsmith(
        &dir,
        &["board3d", "case_fit.nsm", "--glb", glb.to_str().unwrap()],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let doc = netsmith_3d::gltf::json_chunk(&std::fs::read(&glb).unwrap()).unwrap();
    let nodes = doc["nodes"].as_array().unwrap();
    let node = |name: &str| {
        nodes
            .iter()
            .find(|n| n["name"] == name)
            .unwrap_or_else(|| panic!("no node `{name}`"))
    };
    let lid = &node("case:lid")["extras"]["netsmith"];
    assert_eq!(lid["kind"], "case");
    assert_eq!(lid["file"], "case_fit/lid.step");
    assert_eq!(lid["sha256"].as_str().unwrap().len(), 64);
    let boss = &node("case:base:boss");
    assert_eq!(boss["extras"]["netsmith"]["kind"], "case_solid");
    assert_eq!(boss["extras"]["netsmith"]["closed"], true);
    let mesh = &doc["meshes"][boss["mesh"].as_u64().unwrap() as usize];
    let material = mesh["primitives"][0]["material"].as_u64().unwrap() as usize;
    assert_eq!(doc["materials"][material]["name"], "case");
    assert_eq!(doc["materials"][material]["alphaMode"], "BLEND");
    // The root lists the fit's findings, with the solid's node.
    let findings = node("board")["extras"]["netsmith"]["findings"]
        .as_array()
        .unwrap()
        .clone();
    let collision = findings
        .iter()
        .find(|f| f["code"] == "case_collision")
        .unwrap();
    assert_eq!(collision["solid"], "case:lid:lid");
    assert_eq!(collision["parts"], serde_json::json!(["tall"]));
    assert_eq!(collision["severity"], "error");

    // `render` draws it, and lights it by `case:<name>`; a name the board has not is refused
    // with the list.
    let png = dir.join("lid.png");
    let run = netsmith(
        &dir,
        &[
            "render",
            "case_fit.nsm",
            "--size",
            "200x120",
            "--highlight",
            "case:lid",
            "--out",
            png.to_str().unwrap(),
        ],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
    let run = netsmith(
        &dir,
        &["render", "case_fit.nsm", "--highlight", "case:nope"],
    );
    assert_eq!(run.status.code(), Some(2));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains("the case models: case:base, case:lid"),
        "{err}"
    );
}

#[test]
fn a_members_case_model_is_its_own_boards_and_not_the_systems() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = TempDir::new("case-fit-system");
    for name in ["two_boards.nsm", "two_boards.lock.toml"] {
        std::fs::copy(repo.join("examples").join(name), dir.join(name)).unwrap();
    }
    std::fs::copy(fixtures().join("case_fit/lid.step"), dir.join("lid.step")).unwrap();
    std::fs::write(
        dir.join("two_boards.physical.nsm"),
        "import std::pcb::CaseModel\n\nphysical for LedBoard {\n    case {\n        .lid: CaseModel {\n            .file &= \"lid.step\"\n            .z &= 8mm\n        }\n    }\n}\n",
    )
    .unwrap();
    let names = |board: &str| -> Vec<String> {
        let glb = dir.join("out.glb");
        let run = netsmith(
            &dir,
            &[
                "board3d",
                "two_boards.nsm",
                "--board",
                board,
                "--glb",
                glb.to_str().unwrap(),
            ],
        );
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let doc = netsmith_3d::gltf::json_chunk(&std::fs::read(&glb).unwrap()).unwrap();
        doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|n| n["name"].as_str().map(str::to_string))
            .collect()
    };
    // The board alone has its case; the system scene (ADR 0110) draws and checks none.
    assert!(names("TwoBoards/led").iter().any(|n| n == "case:lid"));
    assert!(
        !names("TwoBoards").iter().any(|n| n.contains("case:")),
        "a case round one board of a system is not the system's"
    );
}
