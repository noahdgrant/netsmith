//! A board's case model on disk (ADR 0109), as `check` and `fab` report it: a file that is not
//! there leaves the fit unchecked, a warning, and an error from `fab` when constraints are strict.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

/// A project with the LED board in `src/main.nsm` and `case` in the `case { }` section of its
/// physical file.
fn project(label: &str, case: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    fs::write(
        root.join("src/main.physical.nsm"),
        format!("import std::pcb::CaseModel\n\nphysical for LedExample {{\n    case {{\n{case}\n    }}\n}}\n"),
    )
    .unwrap();
    root
}

const SHELL: &str = "        .shell: CaseModel {\n            .file &= \"../case/box.step\"\n            .z &= -1.6mm\n            .rx &= 180\n        }";

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        )
        .output()
        .unwrap()
}

#[test]
fn check_warns_of_a_missing_case_model() {
    let root = project("case-check-missing", SHELL);
    let run = netsmith(&root, &["check", "src/main.nsm"]);
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{err}");
    assert!(
        err.contains("warning: LedExample: `shell.file` names `case/box.step`")
            && err.contains("is not checked"),
        "{err}"
    );
}

#[test]
fn fab_with_strict_constraints_refuses_a_missing_case_model() {
    let root = project("case-fab-strict", SHELL);
    let run = netsmith(
        &root,
        &[
            "fab",
            "src/main.nsm",
            "--constraints",
            "error",
            "--out",
            "fab",
        ],
    );
    let err = String::from_utf8_lossy(&run.stderr);
    assert_eq!(run.status.code(), Some(1), "{err}");
    assert!(
        err.contains("error: LedExample: `shell.file` names `case/box.step`")
            && err.contains("this is an error because constraints are strict"),
        "{err}"
    );
    assert!(!root.join("fab").exists(), "nothing is written");
}
