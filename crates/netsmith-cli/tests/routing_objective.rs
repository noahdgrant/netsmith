//! The placer's objective for routing (issue 1544) as a key of `netsmith.toml`
//! (`routing_objective`) and a flag (`--routing-objective`): the key places the board with the
//! objective, the flag wins over it, and a project that states none places as it always has.
//!
//! The board is the linear regulator, whose input power chain the objective places shorter
//! (`crates/netsmith-placer/src/anneal/routing.rs`); `view pcb` draws the placement without
//! routing, so the picture's text says where the parts went.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const SOURCE: &str = include_str!("../../../examples/linear_regulator.nsm");
const LOCK: &str = include_str!("../../../examples/linear_regulator.lock.toml");

fn project(label: &str, keys: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"regulator\"\n{keys}"),
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), SOURCE).unwrap();
    fs::write(root.join("src/main.lock.toml"), LOCK).unwrap();
    root
}

fn netsmith(cwd: &Path, args: &[&str]) -> (bool, String) {
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap();
    (
        run.status.success(),
        String::from_utf8_lossy(&run.stderr).to_string(),
    )
}

/// The placement `view pcb` draws, as the SVG's text, with `extra` flags.
fn placed(root: &Path, extra: &[&str]) -> String {
    let mut args = vec![
        "view",
        "pcb",
        "--format",
        "svg",
        "--picture",
        "placed.svg",
        "--place-seeds",
        "1",
    ];
    args.extend_from_slice(extra);
    let (ok, err) = netsmith(root, &args);
    assert!(ok, "{err}");
    fs::read_to_string(root.join("placed.svg")).unwrap()
}

#[test]
fn the_key_places_with_the_objective_and_a_flag_wins_over_it() {
    let off = placed(&project("objective-none", ""), &[]);
    let zero = placed(&project("objective-zero", "routing_objective = 0\n"), &[]);
    assert_eq!(off, zero, "a key of 0 places as a project with none does");

    let keyed_root = project("objective-key", "routing_objective = 1\n");
    let keyed = placed(&keyed_root, &[]);
    assert_ne!(
        off, keyed,
        "the key changes where the parts of the power chain go"
    );

    let flagged = placed(&project("objective-flag", ""), &["--routing-objective", "1"]);
    assert_eq!(flagged, keyed, "the flag states what the key does");

    let overridden = placed(&keyed_root, &["--routing-objective", "0"]);
    assert_eq!(overridden, off, "a flag of 0 turns the key off");
}

#[test]
fn a_key_or_flag_out_of_its_range_is_an_error_that_names_it() {
    let root = project("objective-range", "routing_objective = 11\n");
    let (ok, err) = netsmith(&root, &["board", "check"]);
    assert!(!ok);
    assert!(
        err.contains("netsmith.toml: routing_objective = 11 must be from 0"),
        "{err}"
    );
    let root = project("objective-flag-range", "");
    let (ok, err) = netsmith(
        &root,
        &["view", "pcb", "--routing-objective=-2", "--format", "svg"],
    );
    assert!(!ok);
    assert!(err.contains("--routing-objective"), "{err}");
}
