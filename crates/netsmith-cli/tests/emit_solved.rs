//! `netsmith build --emit-solved <file>` writes the values the solve left each member of each
//! board with (issue #744), and nothing when a board fails before the solve; a board whose solve
//! fails is written with the rows its group passed over (issue 1008).

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

/// The converter of issue #744: a buck with a 5 mA to 50 mA load.
const BUCK: &str = r#"import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

board Repro {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 24V
        .pos.net &= "VIN"
    }
    .reg: Buck<Lmr16006y> {
        .vout.pos.net &= "V3V3"
        .vout.voltage &= 3.3V +/- 5%
    }
    .input ~ .reg.vin
    .load: Rail { .current &= 5mA..50mA }
    .reg.vout ~ .load
}
"#;

fn build(cwd: &Path, file: &str, emit: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["build", file, "--out", "out", "--emit-solved"])
        .arg(emit)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

/// The member of `board` at `path`.
fn member<'a>(board: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    board["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["path"] == path)
        .unwrap_or_else(|| panic!("no member `{path}` in {board:#}"))
}

#[test]
fn the_current_range_of_a_part_is_written_with_its_unit() {
    let dir = TempDir::new("emit-solved");
    fs::write(dir.join("buck.nsm"), BUCK).unwrap();
    let emit = dir.join("buck.solved.json");
    let run = build(&dir, "buck.nsm", &emit);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(
        said.contains(&format!("wrote {}", emit.display())),
        "{said}"
    );
    let text = fs::read_to_string(&emit).unwrap();
    let boards: serde_json::Value = serde_json::from_str(&text).unwrap();
    let boards = boards.as_array().unwrap();
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0]["board"], "Repro");
    // The inductor carries the load's current, at most 50 mA, and the feedback divider's few microamps.
    let current = member(&boards[0], "reg.inductor.current");
    assert_eq!(current["unit"], "A");
    let range = current["range"].as_array().unwrap();
    let (lo, hi) = (range[0].as_f64().unwrap(), range[1].as_f64().unwrap());
    assert!(
        (0.005..0.006).contains(&lo) && (0.05..0.0501).contains(&hi),
        "{current}"
    );
    assert!(
        current["shown"].as_str().unwrap().split("..").nth(1).unwrap().starts_with("50."),
        "{current}"
    );
    // The row it was picked as gives its inductance.
    assert_eq!(member(&boards[0], "reg.inductor.inductance")["unit"], "H");
    // Text is a value.
    assert!(member(&boards[0], "reg.inductor.package")["value"].is_string());
    // The same board is the same bytes. The first build wrote the pick lock, which the
    // second holds the parts to (each pick then says so), so the third is compared with it.
    assert!(build(&dir, "buck.nsm", &emit).status.success());
    let held = fs::read_to_string(&emit).unwrap();
    assert!(build(&dir, "buck.nsm", &emit).status.success());
    assert_eq!(fs::read_to_string(&emit).unwrap(), held);
}

#[test]
fn nothing_is_written_when_a_board_fails() {
    let dir = TempDir::new("emit-solved-fails");
    fs::write(
        dir.join("broken.nsm"),
        "board Broken {\n    .r1: NoSuchPart\n}\n",
    )
    .unwrap();
    let emit = dir.join("broken.solved.json");
    let run = build(&dir, "broken.nsm", &emit);
    assert!(!run.status.success());
    assert!(!emit.exists());
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(said.contains("is not written, as a board failed"), "{said}");
}

/// The pick of `board` of the part at `path`.
fn pick<'a>(board: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    board["picks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["path"] == path)
        .unwrap_or_else(|| panic!("no pick of `{path}` in {board:#}"))
}

#[test]
fn a_pick_says_why_the_rows_before_it_were_passed_over() {
    let dir = TempDir::new("emit-solved-picks");
    fs::write(dir.join("buck.nsm"), BUCK).unwrap();
    let emit = dir.join("buck.solved.json");
    let run = build(&dir, "buck.nsm", &emit);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let boards: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&emit).unwrap()).unwrap();
    let inductor = pick(&boards[0], "reg.inductor");
    assert_eq!(inductor["catalog"], "inductor");
    assert!(inductor.get("held").is_none(), "{inductor:#}");
    // A row before it was refused by the converter's least inductance, with where that is.
    let passed = inductor["passed"].as_array().unwrap();
    let refused = passed
        .iter()
        .find(|r| r["why"] == "refused")
        .unwrap_or_else(|| panic!("no refused row in {inductor:#}"));
    assert!(
        refused["message"]
            .as_str()
            .unwrap()
            .contains("min_inductance"),
        "{refused:#}"
    );
    assert!(
        refused["at"]
            .as_str()
            .unwrap()
            .starts_with("std/parts/regulator.nsm:"),
        "{refused:#}"
    );
    // A pull-up with a stated value passes over the rows that do not fit it.
    let pullup = pick(&boards[0], "reg.ic.shdn_pullup");
    let unfit = &pullup["passed"][0];
    assert_eq!(unfit["why"], "unfit", "{pullup:#}");
    assert!(
        unfit["message"]
            .as_str()
            .unwrap()
            .contains("`reg.ic.shdn_pullup.resistance` is"),
        "{unfit:#}"
    );

    // The first build wrote the pick lock, which now holds each part to its row.
    assert!(dir.join("buck.lock.toml").exists());
    assert!(build(&dir, "buck.nsm", &emit).status.success());
    let boards: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&emit).unwrap()).unwrap();
    let inductor = pick(&boards[0], "reg.inductor");
    assert!(
        inductor["held"]
            .as_str()
            .unwrap()
            .starts_with("the pick lock"),
        "{inductor:#}"
    );
    assert_eq!(inductor["passed"], serde_json::json!([]));
}

/// A resistor that no row of the catalog is at least a thousand gigaohms.
const NO_ROW: &str = "import std::parts::passive::Resistor

board Failing {
    .r: Resistor
    .r.resistance >= 1000Gohm
}
";

#[test]
fn a_board_whose_group_fails_is_written_with_the_rows_of_its_first_turn() {
    let dir = TempDir::new("emit-solved-failed");
    fs::write(dir.join("failing.nsm"), NO_ROW).unwrap();
    let emit = dir.join("failing.solved.json");
    let run = build(&dir, "failing.nsm", &emit);
    assert!(!run.status.success());
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(said.contains("no `resistor` candidate satisfies `r`"), "{said}");
    let boards: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&emit).unwrap()).unwrap();
    assert_eq!(boards.as_array().unwrap().len(), 1);
    let board = &boards[0];
    assert_eq!(board["board"], "Failing");
    assert_eq!(board["failed"], true);
    assert_eq!(board["picks"], serde_json::json!([]));
    // The members the solve got to are there.
    assert!(member(board, "r.designator")["value"].is_string());
    let part = &board["unpicked"][0];
    assert_eq!(part["path"], "r");
    assert_eq!(part["catalog"], "resistor");
    assert!(part.get("with").is_none(), "{part:#}");
    // Every row of the catalog was refused by the relation, with where it is.
    let passed = part["passed"].as_array().unwrap();
    assert!(passed.len() > 100, "{}", passed.len());
    assert!(passed.iter().all(|r| r["why"] == "refused"
        && r["at"] == "failing.nsm:5"
        && r["message"].as_str().unwrap().contains("r.resistance")));
    // The file is written for a board that builds as it was, with no `failed` key.
    fs::write(dir.join("ok.nsm"), BUCK).unwrap();
    assert!(build(&dir, "ok.nsm", &emit).status.success());
    let ok: serde_json::Value = serde_json::from_str(&fs::read_to_string(&emit).unwrap()).unwrap();
    assert!(ok[0].get("failed").is_none() && ok[0].get("unpicked").is_none());
}
