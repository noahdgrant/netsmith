//! `netsmith project fmt` (issue #1103): the files of a project, `--check`, a file that does not
//! parse, and that a formatted board is the same design (`board compare`). The layout itself is
//! tested in `crates/netsmith-lang/tests/format.rs`.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

/// A board written in another layout than the formatter's.
const SOURCE: &str = "import std::sources::Rail
import std::parts::passive::{Resistor,Led}

board Lamp {
  .supply:Rail { .voltage &= 3.3V+/-5% ; .pos.net &= \"3V3\" }
  .r: Resistor {.resistance &= 330ohm +/- 5%}
  .led : Led   // the lamp
  .supply.pos ~> .r ~> .led ~> .supply.neg
}
";

/// `SOURCE` formatted.
const FORMATTED: &str = "import std::parts::passive::{Led, Resistor}
import std::sources::Rail

board Lamp {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= \"3V3\" }
    .r: Resistor { .resistance &= 330ohm +/- 5% }
    .led: Led  // the lamp
    .supply.pos ~> .r ~> .led ~> .supply.neg
}
";

/// A module of the project, in another layout.
const MODULE: &str = "//! A module.\nimport std::sources::Rail\n/// A rail.\nmodule Supply {\n.rail: Rail\n}\n";

/// A project with `SOURCE` as its board, `MODULE` under `src/parts`, and a physical file.
fn project(label: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src/parts")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"lamp\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), SOURCE).unwrap();
    fs::write(root.join("src/parts/supply.nsm"), MODULE).unwrap();
    fs::write(
        root.join("src/main.physical.nsm"),
        "physical for Lamp {\n.r.place.x &= 5mm\n}\n",
    )
    .unwrap();
    root
}

fn netsmith(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap();
    (
        run.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&run.stdout).into_owned(),
        String::from_utf8_lossy(&run.stderr).into_owned(),
    )
}

#[test]
fn check_lists_the_files_and_writes_nothing_and_fmt_writes_them() {
    let root = project("fmt-project");
    let (code, out, err) = netsmith(&root, &["project", "fmt", "--check"]);
    assert_eq!(code, 1, "{out}{err}");
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "src/main.nsm",
            "src/main.physical.nsm",
            "src/parts/supply.nsm"
        ],
        "{err}"
    );
    assert!(err.contains("not formatted"), "{err}");
    assert_eq!(fs::read_to_string(root.join("src/main.nsm")).unwrap(), SOURCE);

    let (code, out, err) = netsmith(&root, &["project", "fmt"]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(out.lines().count(), 3, "{out}");
    assert_eq!(fs::read_to_string(root.join("src/main.nsm")).unwrap(), FORMATTED);
    assert_eq!(
        fs::read_to_string(root.join("src/parts/supply.nsm")).unwrap(),
        "//! A module.\n\nimport std::sources::Rail\n\n/// A rail.\nmodule Supply {\n    .rail: Rail\n}\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("src/main.physical.nsm")).unwrap(),
        "physical for Lamp {\n    .r.place.x &= 5mm\n}\n"
    );

    let (code, out, err) = netsmith(&root, &["project", "fmt", "--check"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.is_empty(), "{out}");
}

#[test]
fn a_named_file_is_formatted_alone() {
    let root = project("fmt-named");
    let (code, out, err) = netsmith(&root.join("src"), &["project", "fmt", "main.nsm"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(out, "main.nsm\n");
    assert_eq!(fs::read_to_string(root.join("src/main.nsm")).unwrap(), FORMATTED);
    assert_eq!(
        fs::read_to_string(root.join("src/parts/supply.nsm")).unwrap(),
        MODULE
    );
}

#[test]
fn a_file_that_does_not_parse_is_left_and_named_and_the_others_are_formatted() {
    let root = project("fmt-broken");
    let broken = "board B {\n    .r: R /* no */\n}\n";
    fs::write(root.join("src/broken.nsm"), broken).unwrap();
    let (code, _, err) = netsmith(&root, &["project", "fmt", "src/broken.nsm", "src/main.nsm"]);
    assert_eq!(code, 2, "{err}");
    assert!(
        err.contains("src/broken.nsm:2:11: block comments do not exist"),
        "{err}"
    );
    assert_eq!(fs::read_to_string(root.join("src/broken.nsm")).unwrap(), broken);
    assert_eq!(fs::read_to_string(root.join("src/main.nsm")).unwrap(), FORMATTED);
}

/// The owner's check (issue #1103): the board before and after the formatter is one design.
#[test]
fn the_formatted_board_is_the_same_design() {
    let root = TempDir::new("fmt-compare");
    fs::write(root.join("before.nsm"), SOURCE).unwrap();
    fs::write(root.join("after.nsm"), SOURCE).unwrap();
    let (code, _, err) = netsmith(&root, &["project", "fmt", "after.nsm"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(fs::read_to_string(root.join("after.nsm")).unwrap(), FORMATTED);
    let (code, out, err) = netsmith(&root, &["board", "compare", "before.nsm", "after.nsm"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("match"), "{out}");
}

/// `project init` writes a project the formatter leaves as it is.
#[test]
fn a_new_project_is_formatted() {
    let root = TempDir::new("fmt-init");
    let (code, _, err) = netsmith(&root, &["project", "init", "--name", "lamp"]);
    assert_eq!(code, 0, "{err}");
    let (code, out, err) = netsmith(&root, &["project", "fmt", "--check"]);
    assert_eq!(code, 0, "{out}{err}");
}
