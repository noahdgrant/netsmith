//! The weights of the router's quality objective as keys of `netsmith.toml` (issue 1536):
//! `board fab` routes with them and says so, a flag wins over its key, and a key out of its
//! range is an error that names it.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

fn project(label: &str, keys: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"led\"\n{keys}"),
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    root
}

fn netsmith(cwd: &Path, args: &[&str]) -> (bool, String) {
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap();
    (
        run.status.success(),
        String::from_utf8_lossy(&run.stderr).to_string(),
    )
}

/// The line `fab` tells of the weights it routes with, if it tells one.
fn weights_line(stderr: &str) -> Option<&str> {
    stderr.lines().find(|l| l.contains("weights of the quality objective"))
}

#[test]
fn fab_routes_with_the_keys_and_a_flag_wins_over_its_key() {
    let root = project(
        "routing-weights-keys",
        "via_weight = 40\nlayer_change_weight = 2\n",
    );
    let (ok, err) = netsmith(&root, &["board", "fab", "--out", "out"]);
    assert!(ok, "{err}");
    let line = weights_line(&err).unwrap_or_else(|| panic!("no weights line: {err}"));
    assert!(
        line.contains("length 1 of the router's own, via 40 mm, layer change 2 mm"),
        "{line}"
    );
    let (ok, err) = netsmith(
        &root,
        &["board", "fab", "--out", "out", "--via-weight", "8", "--length-weight", "2"],
    );
    assert!(ok, "{err}");
    let line = weights_line(&err).unwrap_or_else(|| panic!("no weights line: {err}"));
    assert!(
        line.contains("length 2 of the router's own, via 8 mm, layer change 2 mm"),
        "{line}"
    );
}

#[test]
fn a_project_with_no_weights_routes_as_the_router_always_has() {
    let root = project("routing-weights-none", "");
    let (ok, err) = netsmith(&root, &["board", "fab", "--out", "out"]);
    assert!(ok, "{err}");
    assert_eq!(weights_line(&err), None, "{err}");
}

#[test]
fn a_key_out_of_its_range_stops_the_run_and_names_the_weight() {
    let root = project("routing-weights-range", "via_weight = 5000\n");
    let (ok, err) = netsmith(&root, &["board", "fab", "--out", "out"]);
    assert!(!ok);
    assert!(
        err.contains("netsmith.toml") && err.contains("the via weight is 5000"),
        "{err}"
    );
}
