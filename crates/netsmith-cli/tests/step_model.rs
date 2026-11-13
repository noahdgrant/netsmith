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
