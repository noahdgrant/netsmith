//! The time a charger takes to charge the storage on its output, against the connection's
//! `.charge_time_limit`, and what it takes from its supply while it charges, against the
//! supply's `.max_peak_current` (issue #1264). Every row and the charger here are invented: no
//! real cell or supercapacitor has a recorded source yet (`data/jlcpcb/README.md`).

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const CELL_HEAD: &str = "mpn,chemistry,capacity,internal_resistance,charge_temperature,discharge_temperature,float_voltage,max_charge_current,package,source\n";
/// A lithium-ion cell of 1000 mAh (3.6 kC), charged to 4.2 V +/- 50 mV, at most 500 mA.
const CELL: &str = "ZZC-1000,LiIon,3.6kC,150mohm,0degC..45degC,-20degC..60degC,4.2V +/- 50mV,500mA,JST_PH_S2B-PH-K_1x02_P2.00mm_Horizontal,https://example.invalid/zzc-1000.pdf\n";
const SUPERCAP_HEAD: &str = "mpn,capacitance,esr,dc_esr,leakage_current,max_voltage,cells,soldering,package,height_growth,source\n";
/// Two cells with no tap, 0.5 F +/- 20%, leaking up to 6 uA, 5.5 V.
const SUPERCAP: &str = "ZZS-0F5,0.5F +/- 20%,120mohm,200mohm,6uA,5.5V,2,Hand,Radial_D16.0mm_P7.50mm,1mm,https://example.invalid/zzs-0f5.pdf\n";

fn catalogs() -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    crate::offered::add(&mut c, "cell", "catalogs/cell.csv", &format!("{CELL_HEAD}{CELL}"))
        .unwrap();
    crate::offered::add(&mut c, 
        "supercap",
        "catalogs/supercap.csv",
        &format!("{SUPERCAP_HEAD}{SUPERCAP}"),
    )
    .unwrap();
    c
}

/// An invented charger IC that regulates at `float` and charges at what `current` states (the
/// equation of its programming pin, or a limit).
fn charger(float: &str, current: &str) -> String {
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
    {current}
    .float_voltage &= {float}
}}
"
    )
}

/// The fast charge current of the programming pin: 1000 V over its resistor.
const PROG: &str = ".charge_current == 1000V / .rprog";
const CELL_FLOAT: &str = "4.168V..4.232V";

/// A board with a 5 V supply (`supply` is what else its braces say), the charger and its 2.2 kohm
/// programming resistor, and `lines`.
fn board(charger: &str, supply: &str, lines: &str) -> String {
    format!(
        "import std::parts::passive::{{Resistor, Supercap}}
import std::parts::regulator::Charger
import std::sources::{{Cell, Rail}}

{charger}
board B {{
    .usb: Rail {{ .voltage &= 5V +/- 5%; .pos.net &= \"VBUS\"{supply} }}
    .chg: ZzCharger
    .rprog: Resistor {{ .resistance &= 2.2kohm +/- 1% }}
    .chg.rprog == .rprog.resistance
    .usb ~ .chg.vin
    .chg.prog ~> .rprog ~> .usb.neg
{lines}
}}
"
    )
}

/// The cell on the charger's output, and the limit `limit` on the connection.
fn cell(limit: &str) -> String {
    format!(
        "    .bat: Cell
    .bat.terminals.voltage &= 3V..4.2V
    .bat.terminals.pos.net &= \"VBAT\"
    .chg.out ~ .bat.terminals
    .bat.terminals.charge_time_limit &= {limit}"
    )
}

/// The supercapacitor across the charger's output, charged from 0 V to 4.8 V, and `limit`.
fn supercap(limit: &str) -> String {
    format!(
        "    .chg.out.voltage &= 0V..4.8V
    .chg.out.pos.net &= \"VCAP\"
    .sc: Supercap
    .chg.out.pos ~> .sc ~> .chg.out.neg
    .chg.out.charge_time_limit &= {limit}"
    )
}

fn solved(src: &str) -> Solution {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &catalogs())
}

fn errors(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated
        .iter()
        .map(|n| n.message.clone())
        .filter(|m| m.contains("charge time") || m.contains("while it charges"))
        .collect()
}

#[test]
fn a_cell_charged_within_its_limit_passes() {
    // 3.6 kC at as little as 1000 V / 2.222 kohm, 450 mA, is 8 ks: within three hours.
    let s = solved(&board(&charger(CELL_FLOAT, PROG), "", &cell("..10.8ks")));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(notes(&s).is_empty(), "{:?}", notes(&s));
}

#[test]
fn a_cell_that_takes_longer_than_its_limit_is_refused_with_the_time_and_the_current_it_needs() {
    let s = solved(&board(&charger(CELL_FLOAT, PROG), "", &cell("..7.2ks")));
    let e = errors(&s);
    assert_eq!(e.len(), 1, "{e:?}");
    for part in [
        "`chg` takes up to 7.999ks (2.22 h) to charge the cell `bat`",
        "at as little as 450mA (`chg.charge_current`, 450mA to 459.1mA)",
        "longer than the 7.2ks of `chg.out.charge_time_limit`",
        "needs at least 500mA of charge current",
    ] {
        assert!(e[0].contains(part), "`{part}` is not in `{}`", e[0]);
    }
}

#[test]
fn a_stated_charge_current_is_read_as_the_programming_pin_s() {
    let s = solved(&board(
        &charger(CELL_FLOAT, ".charge_current &= 200mA +/- 10%"),
        "",
        &cell("..10.8ks"),
    ));
    let e = errors(&s);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("at as little as 180mA"), "{}", e[0]);
}

#[test]
fn a_supercapacitor_is_charged_over_the_rail_s_window_to_the_charger_s_float_voltage() {
    // The most of 0.5 F +/- 20%, 0.6 F, from 0 V to 4.75 V (the least float voltage, below the
    // rail's 4.8 V) is 2.85 C; at 450 mA less 6 uA, 6.33 s.
    let s = solved(&board(&charger("4.75V..4.85V", PROG), "", &supercap("..10s")));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    let s = solved(&board(&charger("4.75V..4.85V", PROG), "", &supercap("..5s")));
    let e = errors(&s);
    assert_eq!(e.len(), 1, "{e:?}");
    for part in [
        "600mF across the rail (`sc`) from 0V to 4.75V",
        "2.85C at as little as 450mA",
        "less the 6uA the storage leaks",
    ] {
        assert!(e[0].contains(part), "`{part}` is not in `{}`", e[0]);
    }
}

#[test]
fn a_charge_current_below_the_leakage_never_finishes() {
    let s = solved(&board(
        &charger("4.75V..4.85V", ".charge_current &= 5uA"),
        "",
        &supercap("..1ks"),
    ));
    let e = errors(&s);
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("`chg` never finishes charging"), "{}", e[0]);
    assert!(e[0].contains("leaks up to 6uA"), "{}", e[0]);
}

#[test]
fn no_limit_is_no_check() {
    let lines = "    .bat: Cell
    .bat.terminals.voltage &= 3V..4.2V
    .bat.terminals.pos.net &= \"VBAT\"
    .chg.out ~ .bat.terminals";
    let s = solved(&board(
        &charger(CELL_FLOAT, ".charge_current &= 1mA"),
        "",
        lines,
    ));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(notes(&s).is_empty(), "{:?}", notes(&s));
}

#[test]
fn a_limit_with_nothing_to_charge_is_listed_as_not_evaluated() {
    let lines = "    .chg.out.voltage &= 0V..4.2V
    .load: Rail { .current &= 0mA..1mA }
    .chg.out ~ .load
    .chg.out.charge_time_limit &= ..1ks";
    let s = solved(&board(&charger(CELL_FLOAT, PROG), "", lines));
    let n = notes(&s);
    assert_eq!(n.len(), 1, "{n:?}");
    assert!(n[0].contains("there is nothing to charge"), "{}", n[0]);
}

#[test]
fn the_charge_current_and_the_other_loads_within_the_supply_s_limit_pass() {
    let s = solved(&board(
        &charger(CELL_FLOAT, PROG),
        "; .max_peak_current &= 600mA",
        &format!(
            "{}\n    .mcu: Rail {{ .current &= 10mA..100mA }}\n    .usb ~ .mcu",
            cell("..10.8ks")
        ),
    ));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(notes(&s).is_empty(), "{:?}", notes(&s));
}

#[test]
fn the_charge_current_and_the_other_loads_above_the_supply_s_limit_are_refused() {
    let s = solved(&board(
        &charger(CELL_FLOAT, PROG),
        "; .max_peak_current &= 500mA",
        &format!(
            "{}\n    .mcu: Rail {{ .current &= 10mA..100mA }}\n    .usb ~ .mcu",
            cell("..10.8ks")
        ),
    ));
    let e = errors(&s);
    assert_eq!(e.len(), 1, "{e:?}");
    for part in [
        "while `chg` charges it takes up to 459.1mA from `chg.vin`",
        "the other loads on the net draw up to 100mA",
        "the source gives up to 559.1mA, above the 500mA it can give (`chg.vin.max_peak_current`)",
    ] {
        assert!(e[0].contains(part), "`{part}` is not in `{}`", e[0]);
    }
}

#[test]
fn a_supply_that_states_no_limit_is_not_judged() {
    let s = solved(&board(
        &charger(CELL_FLOAT, PROG),
        "",
        &format!(
            "{}\n    .mcu: Rail {{ .current &= 10mA..2A }}\n    .usb ~ .mcu",
            cell("..10.8ks")
        ),
    ));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert!(notes(&s).is_empty(), "{:?}", notes(&s));
}

#[test]
fn a_charger_that_states_its_input_current_is_read_by_it() {
    // A switching charger that takes 300 mA from its input to give 459 mA: with the MCU's
    // 100 mA, within the supply's 500 mA.
    let current = format!("{PROG}\n    .vin.current &= 250mA..300mA");
    let s = solved(&board(
        &charger(CELL_FLOAT, &current),
        "; .max_peak_current &= 500mA",
        &format!(
            "{}\n    .mcu: Rail {{ .current &= 10mA..100mA }}\n    .usb ~ .mcu",
            cell("..10.8ks")
        ),
    ));
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    let s = solved(&board(
        &charger(CELL_FLOAT, &current),
        "; .max_peak_current &= 350mA",
        &format!(
            "{}\n    .mcu: Rail {{ .current &= 10mA..100mA }}\n    .usb ~ .mcu",
            cell("..10.8ks")
        ),
    ));
    let e: Vec<String> = errors(&s)
        .into_iter()
        .filter(|e| e.contains("while `chg` charges"))
        .collect();
    assert_eq!(e.len(), 1, "{:?}", errors(&s));
    assert!(
        e[0].contains("takes up to 300mA from `chg.vin` (`chg.vin.current`, 250mA to 300mA)"),
        "{}",
        e[0]
    );
}
