//! `netsmith part from-symbol` (#453): a draft part file from a KiCad symbol's pins.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn charger_schematic() -> String {
    repo()
        .join("examples/power_manifold_charger/reference/charger-module.kicad_sch")
        .display()
        .to_string()
}

fn draft(file: &str, symbol: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["part", "from-symbol", file, symbol])
        .output()
        .unwrap()
}

fn stdout(run: &std::process::Output) -> String {
    String::from_utf8_lossy(&run.stdout).to_string()
}

#[test]
fn a_schematics_symbol_gives_a_row_for_each_pin_with_its_kind() {
    let run = draft(&charger_schematic(), "TLV7022DDFR");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let text = stdout(&run);
    assert!(
        text.starts_with("// DRAFT written by `netsmith part from-symbol`"),
        "{text}"
    );
    assert!(
        text.contains("check every row against the datasheet's pin\n// table")
            || text.contains("Check every row against the datasheet's pin"),
        "{text}"
    );
    assert!(text.contains("part Tlv7022ddfr {"), "{text}");
    assert!(text.contains(".designator &= \"U\""), "{text}");
    // The symbol types the outputs open collector and the supplies power; it leaves the inputs
    // unspecified, so they are written TODO_KIND and listed.
    for row in [
        ".outa    open_drain \"OUTA\" \"1\"",
        ".vee     power      \"VEE\" \"4\"",
        ".ina_neg TODO_KIND  \"INA-\" \"2\"",
        ".ina_pos TODO_KIND  \"INA+\" \"3\"",
    ] {
        assert!(text.contains(row), "{row}:\n{text}");
    }
    assert!(
        text.contains("//   `.ina_neg` (pad 2): the symbol types it `unspecified`"),
        "{text}"
    );
    // Eight rows, one for each pad, in the order of the pads.
    let rows: Vec<&str> = text
        .lines()
        .filter(|l| l.trim_start().starts_with('.') && !l.contains("designator"))
        .collect();
    assert_eq!(rows.len(), 8, "{text}");
}

#[test]
fn the_draft_does_not_parse_until_each_undecided_kind_is_written() {
    let text = stdout(&draft(&charger_schematic(), "TLV7022DDFR"));
    let e = netsmith_lang::parse(&text).unwrap_err();
    assert!(format!("{e:?}").contains("TODO_KIND"), "{e:?}");
    let decided = text.replace("TODO_KIND", "input   ");
    netsmith_lang::parse(&decided).unwrap_or_else(|e| panic!("{e:?}\n{decided}"));
}

#[test]
fn a_symbol_library_is_read_too_and_a_name_in_two_units_is_a_pin_of_each() {
    let lib = repo()
        .join("examples/schematic_bench/libs/bench.kicad_sym")
        .display()
        .to_string();
    let run = draft(&lib, "DualOpAmp");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let text = stdout(&run);
    assert!(text.contains("part Dualopamp {"), "{text}");
    assert!(
        text.contains("// The symbol's types decided the kind of every pin."),
        "{text}"
    );
    // The `-` input of each half is a pin of its own, never one pin with two pads.
    assert!(text.contains(".pin_neg_a input  \"-\" \"2\"\n"), "{text}");
    assert!(text.contains(".pin_neg_b input  \"-\" \"6\"\n"), "{text}");
    netsmith_lang::parse(&text).unwrap_or_else(|e| panic!("{e:?}\n{text}"));
}

#[test]
fn a_symbol_the_file_does_not_have_is_exit_two_with_its_names() {
    let run = draft(&charger_schematic(), "NoSuchPart");
    assert_eq!(run.status.code(), Some(2));
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(
        said.contains("has no symbol `NoSuchPart`; it has ")
            && said.contains("Local Library:TLV7022DDFR"),
        "{said}"
    );
}
