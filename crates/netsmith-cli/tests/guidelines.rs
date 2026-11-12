//! Guideline findings (ADR 0036, ADR 0030): a warning by default, an error with
//! `guidelines = "error"` in `netsmith.toml` or `--strict` for one run, shown by the
//! command line, the fab notes and the editor.

use netsmith_kicad::testlib::TempDir;
use std::{fs, path::Path, process::Command};

/// A 2 A supply and load on a `5V` / `GND` pair, with the rail drawn `width` wide.
fn board(width: &str) -> String {
    format!(
        "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Heater {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .load: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= 0A..2A
        .min_track &= {width}..1m
    }}
    .source: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= -2A..0A
    }}
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}}
"
    )
}

fn dir(name: &str) -> TempDir {
    let dir = TempDir::new(&format!("guidelines-test-{name}"));
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

fn project(name: &str, manifest_line: &str, width: &str) -> TempDir {
    let root = dir(name);
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"heater\"\n{manifest_line}\n"),
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), board(width)).unwrap();
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
fn the_command_line_warns_by_default_and_refuses_with_strict_or_the_manifest_key() {
    let root = project("cli", "", "0.2mm");
    let run = netsmith(&root, &["build"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{said}");
    assert!(
        said.contains("warning: Heater: IPC-2221 guideline: net `5V` carries up to 2 A"),
        "{said}"
    );

    // `--strict` is for one run: the next run without it warns again.
    let run = netsmith(&root, &["build", "--strict"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success(), "{said}");
    assert!(
        said.contains("error: Heater: IPC-2221 guideline: net `5V`"),
        "{said}"
    );
    assert!(netsmith(&root, &["build"]).status.success());

    let root = project("cli-error", "guidelines = \"error\"", "0.2mm");
    let run = netsmith(&root, &["build"]);
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("IPC-2221 guideline"));

    // A board that meets the guideline builds under either.
    let root = project("cli-ok", "guidelines = \"error\"", "1mm");
    let run = netsmith(&root, &["build"]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(!String::from_utf8_lossy(&run.stderr).contains("guideline"));
}

#[test]
fn fab_puts_the_finding_in_the_notes_for_the_person_ordering() {
    let root = project("fab", "", "0.2mm");
    let run = netsmith(&root, &["fab", "--allow-drc-violations"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(
        run.status.success(),
        "{said}\n{}",
        String::from_utf8_lossy(&run.stdout)
    );
    let notes = fs::read_to_string(root.join("fab/Heater/fab-notes.txt")).unwrap();
    assert!(
        notes.contains("warning: Heater: IPC-2221 guideline: net `5V` carries up to 2 A"),
        "{notes}"
    );
    // With `--strict` nothing is made.
    let run = netsmith(
        &root,
        &[
            "fab",
            "--allow-drc-violations",
            "--strict",
            "--out",
            "strict",
        ],
    );
    assert!(!run.status.success());
    assert!(!root.join("strict").exists());
}

// ---- the router sizes every net (issue #4, ADR 0036 amendment) -----------------------

/// The heater with no `.min_track` anywhere, so that the width is the router's to work out.
fn unstated_project(name: &str, manifest_line: &str) -> TempDir {
    let root = dir(name);
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"heater\"\n{manifest_line}\n"),
    )
    .unwrap();
    let src = board("0.2mm").replace("        .min_track &= 0.2mm..1m\n", "");
    assert!(!src.contains("min_track"));
    fs::write(root.join("src/main.nsm"), src).unwrap();
    root
}

#[test]
fn a_board_that_states_no_width_is_not_warned_and_is_routed_at_the_width_the_current_needs() {
    // Under `--strict` too: there is nothing to refuse.
    let root = unstated_project("sized", "");
    let run = netsmith(&root, &["fab", "--allow-drc-violations", "--strict"]);
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{said}");
    // (The line that says the pick lock was written names the project's folder.)
    let findings: Vec<&str> = said.lines().filter(|l| !l.starts_with("wrote ")).collect();
    assert!(!findings.iter().any(|l| l.contains("guideline")), "{said}");
    let notes = fs::read_to_string(root.join("fab/Heater/fab-notes.txt")).unwrap();
    assert!(!notes.contains("IPC-2221"), "{notes}");
    // The board file draws 5V and GND at 0.98 mm (2 A needs 0.7814 mm in 1 oz at 10 K, and
    // 0.9768 mm for the narrowest track JLCPCB's ±20 % tolerance makes of it).
    let pcb = fs::read_to_string(root.join("fab/Heater/Heater.kicad_pcb")).unwrap();
    assert!(
        pcb.contains("(width 0.98)"),
        "no track of 0.98 mm in the board"
    );
    let pro = fs::read_to_string(root.join("fab/Heater/Heater.kicad_pro")).unwrap();
    assert!(pro.contains("Track_0.98mm"), "a net class of 0.98 mm");
}

// ---- a joined bus interface that states no role (ADR 0064, stage 5) ----------------------
