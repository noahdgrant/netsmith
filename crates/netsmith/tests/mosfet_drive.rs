//! A MOSFET's on-resistance is guaranteed at a gate-source voltage (`.on_resistance_vgs`, ADR
//! 0092): the gate drive the design gives the part must always be at least it. The standard
//! rows state it from the datasheets; a project's row may leave it out. Every project row
//! and number below is invented.

use netsmith::{Outcome, Problem, outcome_with};
use netsmith_check::Loader;
use netsmith_solve::Catalogs;

/// An invented 100 V N-channel row that states no test voltage, as a project's own row.
const ZN_BLANK: &str = "\
mpn,name,max_drain_source_voltage,max_drain_current,max_gate_source_voltage,gate_threshold,on_resistance,package,library,source
ZN-100,ZN-100 100V 2A N-MOSFET,100V,2A,20V,1V..2V,0.2ohm,SOT-23,Extended,https://example.invalid/zn-100
";

/// An invented 100 V N-channel row that states it (a header with the column).
const ZN_STATED: &str = "\
mpn,name,max_drain_source_voltage,max_drain_current,max_gate_source_voltage,gate_threshold,on_resistance,on_resistance_vgs,package,library,source
ZN-100,ZN-100 100V 2A N-MOSFET,100V,2A,20V,1V..2V,0.2ohm,4.5V,SOT-23,Extended,https://example.invalid/zn-100
";

/// An invented 30 V P-channel row that states it.
const ZP_STATED: &str = "\
mpn,name,max_drain_source_voltage,max_drain_current,max_gate_source_voltage,gate_threshold,on_resistance,on_resistance_vgs,package,library,source
ZP-30,ZP-30 30V 4A P-MOSFET,30V,4A,12V,0.4V..1.3V,0.07ohm,4.5V,SOT-23,Extended,https://example.invalid/zp-30
";

fn world(files: &[(&str, &str)]) -> (Loader, Catalogs) {
    let loader = Loader::std();
    let mut catalogs = Catalogs::jlcpcb();
    for (name, rows) in files {
        catalogs
            .add_file(name, &format!("catalogs/{name}.csv"), rows)
            .unwrap();
    }
    (loader, catalogs)
}

fn build(w: &(Loader, Catalogs), source: &str) -> Result<Outcome, Vec<Problem>> {
    outcome_with("main.nsm", source, &w.0, &w.1, None)
}

fn fails(w: &(Loader, Catalogs), source: &str) -> String {
    match build(w, source) {
        Ok(_) => panic!("this should not build"),
        Err(problems) => problems
            .iter()
            .map(|p| p.message.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn q_mpn(o: &Outcome) -> Option<&str> {
    o.boards[0]
        .sourcing
        .iter()
        .find(|s| s.refdes == "Q1")
        .and_then(|s| s.mpn.as_deref())
}

/// A low-side switch on a 12 V rail, its gate driven from a logic rail of `drive`; `extra` is
/// more statements of the part.
fn low_side(supply: &str, drive: &str, extra: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\nimport std::parts::transistor::NMosfet\nimport std::sources::Rail\n\n\
board Switch {{\n    .supply: Rail {{\n        .voltage &= {supply}\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n\
    .logic: Rail {{\n        .voltage &= {drive}\n        .pos.net &= \"LOGIC\"\n        .neg.net &= \"GND\"\n    }}\n\
    .q: NMosfet {{\n        .drain_source_voltage &= supply.voltage\n        .gate_source_voltage &= logic.voltage\n{extra}    }}\n\
    .load: Resistor {{ .resistance &= 1Mohm +/- 1% }}\n\
    .q.gate ~ .logic.pos\n    .q.source ~ .supply.neg\n    .q.drain ~ .load.a\n    .load.b ~ .supply.pos\n}}\n"
    )
}

fn high_side(drive: &str, extra: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\nimport std::parts::transistor::PMosfet\nimport std::sources::Rail\n\n\
board HighSide {{\n    .supply: Rail {{\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n\
    .q: PMosfet {{\n        .drain_source_voltage &= supply.voltage\n        .gate_source_voltage &= {drive}\n        .current &= 0A..1A\n{extra}    }}\n\
    .load: Resistor {{ .resistance &= 100kohm +/- 1% }}\n\
    .q.gate ~ .supply.neg\n    .q.source ~ .supply.pos\n    .q.drain ~ .load.a\n    .load.b ~ .supply.neg\n}}\n"
    )
}

#[test]
fn the_standard_rows_state_the_voltage_their_on_resistance_is_guaranteed_at() {
    let csv = include_str!("../../../data/jlcpcb/catalog/n_mosfet.csv");
    let mut lines = csv.lines();
    let header: Vec<&str> = lines.next().unwrap().split(',').collect();
    assert!(header.contains(&"on_resistance_vgs"), "{header:?}");
    // 2N7002: 7 ohm at VGS = 5 V (JSCJ datasheet, p.2); AO3400A: 48 mohm at 2.5 V (AOS Rev 3, p.2).
    assert!(csv.contains(",7ohm,5V,SOT-23,"), "{csv}");
    assert!(csv.contains(",0.048ohm,2.5V,SOT-23,"), "{csv}");
}

#[test]
fn a_3v3_drive_is_given_the_row_guaranteed_at_2v5_and_not_the_one_guaranteed_at_5v() {
    let w = world(&[]);
    // 5.7 V to 6.3 V is above the 5 V the 2N7002's 7 ohm holds at: the first row serves.
    let o = build(&w, &low_side("12V +/- 5%", "6V +/- 5%", "")).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(q_mpn(&o), Some("2N7002"));
    // 3.3 V drive: 3.135 V to 3.465 V is below the 5 V the 2N7002's 7 ohm holds at, so the
    // pick moves to the AO3400A, guaranteed at 2.5 V.
    let o =
        build(&w, &low_side("12V +/- 5%", "3.3V +/- 5%", "")).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(q_mpn(&o), Some("AO3400A"));
}

#[test]
fn a_stated_test_voltage_is_checked_against_the_drive_for_either_polarity() {
    let w = world(&[]);
    for (board, drive) in [
        (
            low_side("12V +/- 5%", "3.3V", ".on_resistance_vgs &= 5V\n"),
            "3.3V",
        ),
        (high_side("3.3V", ".on_resistance_vgs &= 5V\n"), "3.3V"),
    ] {
        let m = fails(&w, &board);
        assert!(
            m.contains("q.gate_source_voltage must always be at least q.on_resistance_vgs")
                && m.contains(drive)
                && m.contains("5V"),
            "{m}"
        );
    }
    // A drive at the test voltage, or above, holds.
    build(
        &w,
        &low_side("12V +/- 5%", "5V", ".on_resistance_vgs &= 5V\n"),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
}

#[test]
fn a_row_with_no_test_voltage_is_offered_and_its_check_is_not_evaluated() {
    // 80 V is beyond the 60 V of both standard rows, so only the project's row can serve.
    let w = world(&[("n_mosfet", ZN_BLANK)]);
    let o =
        build(&w, &low_side("80V +/- 5%", "3.3V +/- 5%", "")).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(q_mpn(&o), Some("ZN-100"));
    // The same row with a stated 4.5 V is refused for a 3.3 V drive and picked for 5 V.
    let w = world(&[("n_mosfet", ZN_STATED)]);
    let m = fails(&w, &low_side("80V +/- 5%", "3.3V +/- 5%", ""));
    assert!(m.contains("no `n_mosfet` candidate satisfies `q`"), "{m}");
    let o = build(&w, &low_side("80V +/- 5%", "5V +/- 5%", "")).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(q_mpn(&o), Some("ZN-100"));
    // A test voltage the design states is checked against the drive before any row is tried:
    // the message gives both numbers.
    let m = fails(
        &world(&[]),
        &low_side(
            "12V +/- 5%",
            "3.3V +/- 5%",
            ".on_resistance_vgs &= 4V..5V\n",
        ),
    );
    assert!(m.contains("3.135V..3.465V") && m.contains("4V..5V"), "{m}");
}

#[test]
fn a_requirement_on_the_test_voltage_offers_only_the_rows_that_state_one() {
    let w = world(&[("n_mosfet", ZN_BLANK)]);
    // Nothing in the design asks: the row without a voltage serves an 80 V rail.
    build(&w, &low_side("80V +/- 5%", "5V +/- 5%", "")).unwrap_or_else(|e| panic!("{e:?}"));
    // The design asks for a voltage of 5 V at most: the row does not record one.
    let m = fails(
        &w,
        &low_side("80V +/- 5%", "5V +/- 5%", ".on_resistance_vgs <= 5V\n"),
    );
    assert!(m.contains("no `n_mosfet` candidate satisfies `q`"), "{m}");
    // The same requirement picks the AO3400A on a 12 V rail whose drive is 3.3 V.
    let o = build(
        &w,
        &low_side("12V +/- 5%", "3.3V +/- 5%", ".on_resistance_vgs <= 3V\n"),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(q_mpn(&o), Some("AO3400A"));
}

#[test]
fn a_p_channel_row_that_states_its_test_voltage_is_checked_too() {
    let w = world(&[("p_mosfet", ZP_STATED)]);
    let o = build(&w, &high_side("5V +/- 5%", "")).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(q_mpn(&o), Some("ZP-30"));
    let m = fails(&w, &high_side("3.3V +/- 5%", ""));
    assert!(
        m.contains("no `p_mosfet` candidate satisfies `q`")
            && m.contains("ZP-30 30V 4A P-MOSFET")
            && m.contains("q.gate_source_voltage must always be at least q.on_resistance_vgs")
            && m.contains("3.135V..3.465V")
            && m.contains("4.5V"),
        "{m}"
    );
}
