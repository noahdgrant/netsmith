//! A charger joined to a rechargeable cell (issue #1262): `std::sources::Cell` and
//! `std::parts::regulator::Charger`, held against each other after the solve. Every row and the
//! charger here are invented: no real cell has a recorded source yet (`data/jlcpcb/README.md`),
//! so the standard catalog `cell` is empty.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const HEAD: &str = "mpn,chemistry,capacity,internal_resistance,charge_temperature,discharge_temperature,float_voltage,max_charge_current,package,library,source\n";

/// A lithium-ion cell of 1000 mAh, charged to 4.2 V +/- 50 mV between 0 and 45 degC, at most
/// 500 mA.
const CELL: &str = "ZZC-1000,LiIon,3.6kC,150mohm,0degC..45degC,-20degC..60degC,4.2V +/- 50mV,500mA,JST_PH_S2B-PH-K_1x02_P2.00mm_Horizontal,Extended,https://example.invalid/zzc-1000.pdf\n";

fn catalogs() -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    c.add_file("cell", "catalogs/cell.csv", &format!("{HEAD}{CELL}"))
        .unwrap();
    c
}

/// An invented charger IC whose fast charge current is `1000 V / RPROG`, with the float voltage
/// and the thermistor window given (`window` is the narrowing line, or empty for none).
fn charger(float: &str, window: &str) -> String {
    format!(
        "/// An invented charger.
part ZzCharger provides Charger {{
    .package: Package
    .package &= \"SOT-23-5\"
    pins \"SOT-23-5\" {{
        .out.pos power \"VBAT\" \"3\"
        .vin.neg power \"VSS\"  \"2\"
        .vin.pos power \"VDD\"  \"4\"
        .prog    input \"PROG\" \"5\"
    }}
    .rprog: Resistance
    .charge_current == 1000V / .rprog
    .float_voltage &= {float}
    {window}
}}
"
    )
}

fn source(charger: &str, board: &str) -> String {
    format!(
        "import std::parts::passive::Resistor
import std::parts::regulator::Charger
import std::sources::{{Cell, Rail}}

{charger}
board B {{
    .usb: Rail {{ .voltage &= 5V +/- 5%; .pos.net &= \"VBUS\" }}
    .bat: Cell
    .bat.terminals.voltage &= 3V..4.2V
    .bat.terminals.pos.net &= \"VBAT\"
{board}
}}
"
    )
}

/// The charger on the board, its programming resistor `rprog` and its output joined to the
/// cell.
fn joined(rprog: &str) -> String {
    format!(
        "    .chg: ZzCharger
    .rprog: Resistor {{ .resistance &= {rprog} }}
    .chg.rprog == .rprog.resistance
    .usb ~ .chg.vin
    .chg.out ~ .bat.terminals
    .chg.prog ~> .rprog ~> .usb.neg"
    )
}

const MATCHED_FLOAT: &str = "4.168V..4.232V";
const MATCHED_WINDOW: &str = ".thermistor_window &= 0degC..45degC";

fn solved(src: &str) -> Solution {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &catalogs())
}

fn errors(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn charge_errors(s: &Solution) -> Vec<String> {
    errors(s)
        .into_iter()
        .filter(|e| e.contains("charges the cell"))
        .collect()
}

fn charge_notes(s: &Solution) -> Vec<String> {
    s.not_evaluated
        .iter()
        .map(|n| n.message.clone())
        .filter(|m| m.contains("is not judged against the cell"))
        .collect()
}

#[test]
fn a_charger_that_matches_its_cell_passes() {
    // 1000 V over 2.2 kohm at 1% is up to 459 mA.
    let s = solved(&source(
        &charger(MATCHED_FLOAT, MATCHED_WINDOW),
        &joined("2.2kohm +/- 1%"),
    ));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(charge_notes(&s).is_empty(), "{:?}", charge_notes(&s));
    assert_eq!(s.picks.iter().filter(|p| p.candidate == "ZZC-1000").count(), 1);
}

#[test]
fn a_float_voltage_above_the_cells_is_refused_with_both_ranges() {
    let s = solved(&source(
        &charger("4.3V..4.4V", MATCHED_WINDOW),
        &joined("2.2kohm +/- 1%"),
    ));
    let e = charge_errors(&s);
    assert_eq!(e.len(), 1, "{:?}", errors(&s));
    assert!(
        e[0].contains("`chg` charges the cell `bat` to 4.3V to 4.4V (`chg.float_voltage`), not within the 4.15V to 4.25V the cell is charged to (`bat.float_voltage`): 4.4V is above 4.25V"),
        "{}",
        e[0]
    );
}

#[test]
fn a_float_voltage_below_the_cells_is_refused() {
    let s = solved(&source(
        &charger("4.0V..4.1V", MATCHED_WINDOW),
        &joined("2.2kohm +/- 1%"),
    ));
    let e = charge_errors(&s);
    assert_eq!(e.len(), 1, "{:?}", errors(&s));
    assert!(e[0].contains("4V is below 4.15V"), "{}", e[0]);
}

#[test]
fn a_thermistor_window_past_the_cells_charge_window_is_refused() {
    let s = solved(&source(
        &charger(MATCHED_FLOAT, ".thermistor_window &= -10degC..50degC"),
        &joined("2.2kohm +/- 1%"),
    ));
    let e = charge_errors(&s);
    assert_eq!(e.len(), 1, "{:?}", errors(&s));
    assert!(
        e[0].contains("`chg` charges the cell `bat` between -10degC to 50degC (`chg.thermistor_window`), not within the 0degC to 45degC the cell may be charged in (`bat.charge_temperature`): 50degC is above 45degC and -10degC is below 0degC"),
        "{}",
        e[0]
    );
}

#[test]
fn a_charger_with_no_thermistor_is_not_judged_on_the_window() {
    let s = solved(&source(&charger(MATCHED_FLOAT, ""), &joined("2.2kohm +/- 1%")));
    assert!(charge_errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(charge_notes(&s).is_empty(), "{:?}", charge_notes(&s));
}

#[test]
fn a_charge_current_above_the_cells_limit_is_refused_at_its_most() {
    // 1000 V over 1 kohm at 1% is up to 1.0101 A.
    let s = solved(&source(
        &charger(MATCHED_FLOAT, MATCHED_WINDOW),
        &joined("1kohm +/- 1%"),
    ));
    let e = charge_errors(&s);
    assert_eq!(e.len(), 1, "{:?}", errors(&s));
    assert!(e[0].contains("`chg` charges the cell `bat` at up to 1.01A (`chg.charge_current`, 990.1mA to 1.01A)"), "{}", e[0]);
    assert!(
        e[0].contains("above the 500mA the cell allows (`bat.max_charge_current`)"),
        "{}",
        e[0]
    );
}

#[test]
fn the_resistors_tolerance_counts_against_the_limit() {
    // 2 kohm is 500 mA, the cell's limit, but at 1% it is up to 505 mA.
    let s = solved(&source(
        &charger(MATCHED_FLOAT, MATCHED_WINDOW),
        &joined("2kohm +/- 1%"),
    ));
    let e = charge_errors(&s);
    assert_eq!(e.len(), 1, "{:?}", errors(&s));
    assert!(e[0].contains("above the 500mA"), "{}", e[0]);
}

#[test]
fn a_charger_joined_to_no_cell_is_not_judged() {
    let board = "    .chg: ZzCharger
    .rprog: Resistor { .resistance &= 1kohm +/- 1% }
    .chg.rprog == .rprog.resistance
    .usb ~ .chg.vin
    .other: Rail { .voltage &= 4.4V; .pos.net &= \"VOUT\"; .current &= 0mA..10mA }
    .chg.out ~ .other
    .chg.prog ~> .rprog ~> .usb.neg
    .load: Rail { .current &= 0mA..10mA }
    .bat.terminals ~ .load";
    let s = solved(&source(&charger("4.3V..4.4V", ""), board));
    assert!(charge_errors(&s).is_empty(), "{:?}", errors(&s));
}

#[test]
fn a_charger_inside_a_module_is_judged_through_its_rail() {
    let src = format!(
        "import std::parts::passive::Resistor
import std::parts::regulator::Charger
import std::sources::{{Cell, Rail}}

{}
module ChargeStage {{
    .vin: Rail
    .vbat: Rail
    .chg: ZzCharger
    .rprog: Resistor {{ .resistance &= 1kohm +/- 1% }}
    .chg.rprog == .rprog.resistance
    .vin ~ .chg.vin
    .chg.out ~ .vbat
    .chg.prog ~> .rprog ~> .vin.neg
}}

board B {{
    .usb: Rail {{ .voltage &= 5V +/- 5%; .pos.net &= \"VBUS\" }}
    .bat: Cell
    .bat.terminals.voltage &= 3V..4.2V
    .bat.terminals.pos.net &= \"VBAT\"
    .stage: ChargeStage
    .usb ~ .stage.vin
    .stage.vbat ~ .bat.terminals
}}
",
        charger(MATCHED_FLOAT, MATCHED_WINDOW)
    );
    let s = solved(&src);
    let e = charge_errors(&s);
    assert_eq!(e.len(), 1, "{:?}", errors(&s));
    assert!(e[0].contains("`stage.chg` charges the cell `bat` at up to"), "{}", e[0]);
}

#[test]
fn a_charge_current_with_no_value_is_listed_as_not_evaluated() {
    let board = "    .chg: ZzCharger
    .usb ~ .chg.vin
    .chg.out ~ .bat.terminals
    .chg.prog ~ .usb.neg";
    let s = solved(&source(&charger(MATCHED_FLOAT, MATCHED_WINDOW), board));
    let notes = charge_notes(&s);
    assert_eq!(notes.len(), 1, "{:?}", s.not_evaluated);
    assert!(
        notes[0].contains("the charger `chg` is not judged against the cell `bat` it charges in full: `chg.charge_current` has no value"),
        "{}",
        notes[0]
    );
}

#[test]
fn the_charge_time_limit_is_a_field_of_the_connection() {
    // The two rails joined with `~` share it: a limit stated on the cell's side and a bound on
    // the charger's that it breaks contradict.
    let lines = format!(
        "{}\n    .bat.terminals.charge_time_limit &= 3.6ks..10.8ks\n    .chg.out.charge_time_limit >= 14.4ks",
        joined("2.2kohm +/- 1%")
    );
    let c = check(
        "t.nsm",
        &source(&charger(MATCHED_FLOAT, MATCHED_WINDOW), &lines),
        &Loader::std(),
    );
    let s = if c.is_ok() {
        errors(&solve(&c.boards[0], &catalogs())).join("\n")
    } else {
        c.render()
    };
    assert!(
        s.contains("chg.out.charge_time_limit must always be at least 14.4ks, but 3.6ks..10.8ks is not always above 14.4ks"),
        "{s}"
    );
}
