//! `netsmith build --emit-solved <file>` writes the values the solve left each member of each
//! board with (issue #744), and nothing when a board fails.

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
    // The inductor carries the load's current, at most 50 mA.
    let current = member(&boards[0], "reg.inductor.current");
    assert_eq!(current["unit"], "A");
    let range = current["range"].as_array().unwrap();
    let (lo, hi) = (range[0].as_f64().unwrap(), range[1].as_f64().unwrap());
    assert!((0.005..0.006).contains(&lo) && hi == 0.05, "{current}");
    assert_eq!(current["shown"].as_str().unwrap().split("..").nth(1), Some("50mA"));
    // The row it was picked as gives its inductance.
    assert_eq!(member(&boards[0], "reg.inductor.inductance")["unit"], "H");
    // Text is a value.
    assert!(member(&boards[0], "reg.inductor.package")["value"].is_string());
    // The same board is the same bytes.
    assert!(build(&dir, "buck.nsm", &emit).status.success());
    assert_eq!(fs::read_to_string(&emit).unwrap(), text);
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
