//! The analysis of a board's `scenario` (issue #1258): with the input's supply removed, a backup
//! source behind a diode reaches a regulator's input backwards through its body diode, the
//! regulators on that input forwards, and the loads and sense nets behind them. `Reg`, `Load`
//! and `Det` are invented; the diode is the standard Schottky.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = "\
import std::logic::Logic
import std::parts::diode::Schottky
import std::parts::passive::Resistor
import std::sources::{Rail, Reverse, ReversePath}

part Reg {
    .designator &= \"U\"
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    pins {
        .vin.pos  power \"VIN\"  \"1\"
        .vin.neg  power \"GND\"  \"2\"
        .vout.pos power \"VOUT\" \"3\"
    }
    .dropout: Voltage
    .dropout ?= 0.2V
    .reverse: Reverse
    .vin.current == -.vout.current
    dc linear_regulator .vin .vout
}

/// Runs from 3 V to 3.6 V.
part Load {
    .designator &= \"U\"
    .vdd: Rail
    .vdd.voltage >= 3V
    .vdd.voltage <= 3.6V
    .vdd.current &= 1mA..2mA
    pins {
        .vdd.pos power \"VDD\" \"1\"
        .vdd.neg power \"GND\" \"2\"
    }
}

/// Reads a high from 70 % of its supply and a low up to 30 %.
part Det {
    .designator &= \"U\"
    .vdd: Rail
    .vdd.current &= 1mA..2mA
    .det: Logic
    .det.reference ~ .vdd
    .det.reference.current &= 0A
    .det.vih &= 0.7 * .vdd.voltage
    .det.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos  power \"VDD\" \"1\"
        .vdd.neg  power \"GND\" \"2\"
        .det.line input \"DET\" \"3\"
    }
}
";

/// A 5 V input into `.a` (3.3 V) and `.b` (2.5 V); a 3.6 V to 4.2 V cell behind a Schottky on
/// the 3.3 V rail; `.det` on the 3.3 V rail senses the input through a divider.
fn board(extra: &str, scenario: &str) -> String {
    format!(
        "{PARTS}\n\
         board B {{\n    \
         .usb: Rail {{ .voltage &= 5V +/- 5%; .pos.net &= \"VBUS\" }}\n    \
         .cell: Rail {{ .voltage &= 3.6V..4.2V; .pos.net &= \"VBAT\" }}\n    \
         .cell.neg ~ .usb.neg\n    \
         .a: Reg\n    \
         .b: Reg\n    \
         .a.vout.voltage &= 3.3V +/- 3%\n    \
         .b.vout.voltage &= 2.5V +/- 3%\n    \
         .usb ~ .a.vin\n    \
         .usb ~ .b.vin\n    \
         .a.vout.pos.net &= \"3V3\"\n    \
         .b.vout.pos.net &= \"2V5\"\n    \
         .d: Schottky {{ .current &= 0A..100mA }}\n    \
         .cell.pos ~> .d ~> .a.vout.pos\n    \
         .det: Det\n    \
         .det.vdd ~ .a.vout\n    \
         .late: Load\n    \
         .late.vdd ~ .b.vout\n    \
         .top: Resistor {{ .resistance &= 100kohm +/- 1% }}\n    \
         .bottom: Resistor {{ .resistance &= 100kohm +/- 1% }}\n    \
         .usb.pos ~> .top ~> .det.det.line\n    \
         .det.det.line ~> .bottom ~> .usb.neg\n\
         {extra}\n    \
         {scenario}\n}}\n"
    )
}

const BACKUP: &str = "scenario backup { remove .usb; source .cell; sense .det.det.line }";

fn solved(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let board = c.standalone_boards().next().expect("a board");
    solve(board, &Catalogs::jlcpcb())
}

/// The scenario's faults and its notes.
fn scenario(src: &str) -> (Vec<String>, Vec<String>) {
    let s = solved(src);
    let faults = s
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .filter(|m| m.contains("scenario"))
        .collect();
    let notes = s
        .not_evaluated
        .iter()
        .map(|n| n.message.clone())
        .filter(|m| m.contains("scenario"))
        .collect();
    (faults, notes)
}

fn the_note(notes: &[String]) -> &str {
    notes
        .iter()
        .find(|n| n.starts_with("scenario `backup`"))
        .unwrap_or_else(|| panic!("{notes:#?}"))
}

#[test]
fn a_body_diode_feeds_the_input_from_behind_and_the_regulators_on_it_forwards() {
    let (faults, notes) = scenario(&board("", BACKUP));
    let note = the_note(&notes);
    assert!(note.contains("with `.usb` removed"), "{note}");
    assert!(
        note.contains("reaches `.a` backwards through its body diode (`.a.vin` at"),
        "{note}"
    );
    assert!(note.contains("drives `.b` (`.b.vout` at"), "{note}");
    assert!(
        note.contains("; it powers `.det.vdd` at 3V..3.6V (which states no supply range) and `.late.vdd` at"),
        "{note}"
    );
    assert!(note.contains("`.det.vdd` at") && note.contains("`.late.vdd` at"), "{note}");
    // The input sits a body diode below the 3.3 V rail, and the divider halves it: a level
    // that the detector reads as neither low nor high.
    assert!(
        faults.iter().any(|f| f.contains("`.det.det.line` settles at")
            && f.contains("which the pin reads as neither low")),
        "{faults:#?}"
    );
    // `.b` follows its input down, below what `.late` runs from.
    assert!(
        faults.iter().any(|f| f.contains("`.late.vdd` is at")
            && f.contains("`.late` needs 3V..3.6V on it")),
        "{faults:#?}"
    );
}

#[test]
fn a_regulator_that_blocks_keeps_the_input_off() {
    let (faults, notes) = scenario(&board(
        "    .a.reverse.path &= ReversePath::Blocking",
        BACKUP,
    ));
    assert!(faults.is_empty(), "{faults:#?}");
    let note = the_note(&notes);
    assert!(note.contains("is blocked by `.a`"), "{note}");
    assert!(!note.contains("drives `.b`"), "{note}");
    assert!(!note.contains("`.late.vdd`"), "{note}");
    assert!(
        note.contains("`.det.det.line` settles at 0V, read as low by the pin"),
        "{note}"
    );
}

#[test]
fn a_reverse_limit_below_what_the_input_draws_leaves_its_voltage_open() {
    let (faults, notes) = scenario(&board(
        "    .a.reverse.max_current &= 1uA",
        BACKUP,
    ));
    let note = the_note(&notes);
    assert!(
        note.contains("draw more than its reverse limit, so `.a.vin` may be at any voltage"),
        "{note}"
    );
    // The input may be anywhere from 0 V, so `.b`'s output may be too.
    assert!(
        faults
            .iter()
            .any(|f| f.contains("`.late.vdd` is at 0V..")),
        "{faults:#?}"
    );
}

#[test]
fn a_sense_net_that_floats_is_not_judged() {
    // With no resistor to ground and the input blocked, the line hangs from an input that is
    // off and joined by no resistor to anything that is on.
    let src = board("    .a.reverse.path &= ReversePath::Blocking", BACKUP)
        .replace("    .det.det.line ~> .bottom ~> .usb.neg\n", "")
        .replace("    .bottom: Resistor { .resistance &= 100kohm +/- 1% }\n", "");
    let (faults, notes) = scenario(&src);
    assert!(faults.is_empty(), "{faults:#?}");
    assert!(
        notes.iter().any(|n| n
            == "in the scenario `backup`, where `.det.det.line` settles is not judged: no resistor joins it to a net that is on or to ground, so it floats"),
        "{notes:#?}"
    );
}

#[test]
fn a_rail_that_a_regulator_drives_is_not_a_supply() {
    let (faults, _) = scenario(&board(
        "",
        "scenario backup { remove .a.vout; source .cell }",
    ));
    assert!(
        faults.iter().any(|f| f.contains(
            "`.a.vout` is not a supply, so it cannot be removed: `.a` drives it"
        )),
        "{faults:#?}"
    );
}
