//! The search leaves out the candidates of a part that a trial would refuse, without trying
//! each of them (issue #184), and chooses what it always chose.
//!
//! Each test solves one board twice, with the narrowing off (every candidate is tried: the
//! reference) and on, and compares what the solver says: the picks (with how many candidates
//! each rejected before it), the messages, and what is left unresolved. With the narrowing on,
//! the audit is also on: every candidate that is left out is tried, and the solver stops if a
//! trial accepts one, so a candidate that was dropped wrongly fails the test and not only a
//! pick that happened to change. A board that a search does not solve is searched again with
//! every candidate tried, so its message, its trials and its work are those of the reference.
//! Every part and number here is invented.

use netsmith_check::{Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, Solution, Value, solve};

fn resistor(ohms: f64, percent: f64) -> Candidate {
    let label = format!("{ohms}");
    let center = Quantity::literal(ohms, Some("ohm")).unwrap().quantity;
    let tolerance = Quantity::literal(percent, Some("%")).unwrap().quantity;
    Candidate {
        name: format!("{label} {percent}%"),
        label,
        columns: vec![
            (
                "resistance".into(),
                Value::Quantity(Quantity::tolerance(&center, &tolerance, true).unwrap()),
            ),
            ("package".into(), Value::Text("0402".into())),
            (
                "max_power".into(),
                Value::Quantity(Quantity::literal(1.0, Some("kW")).unwrap().quantity),
            ),
        ],
        lcsc: None,
        mpn: None,
        library: None,
        assembly: None,
        height: None,
        temp_min: None,
        temp_max: None,
        generic: None,
    }
}

/// Resistors of 100, 200, ... 4000 ohms with a tolerance of 0.1 %.
fn tenths() -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates: (1..=40).map(|i| resistor(100.0 * i as f64, 0.1)).collect(),
    });
    c
}

fn run(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("narrowing.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// What was picked, with how many candidates each part rejected first.
fn picks(s: &Solution) -> Vec<(String, String, usize, u32)> {
    s.picks
        .iter()
        .map(|p| (p.catalog.clone(), p.candidate.clone(), p.rejected, p.count))
        .collect()
}

/// The board solved without the narrowing and with it (and the audit), the same everywhere
/// but the effort.
fn both(src: &str, catalogs: Catalogs) -> (Solution, Solution) {
    let reference = run(src, &catalogs.clone().with_narrowing(false));
    let narrowed = run(
        src,
        &catalogs.with_narrowing(true).with_narrowing_audit(true),
    );
    assert_eq!(picks(&narrowed), picks(&reference), "the picks");
    assert_eq!(messages(&narrowed), messages(&reference), "the messages");
    assert_eq!(
        narrowed.unresolved.len(),
        reference.unresolved.len(),
        "what is left unresolved"
    );
    assert_eq!(narrowed.deferred, reference.deferred, "what is deferred");
    (narrowed, reference)
}

#[test]
fn the_second_resistor_of_a_feedback_divider_is_found_without_trying_every_row() {
    // The pair is searched as a pair: once the first resistor is chosen, nearly every row of
    // the second is refused for the output window or the power rating.
    let src = "import std::parts::regulator::FeedbackDivider

board B {
    .divider: FeedbackDivider {
        .vout &= 3.8V +/- 3%
        .vref &= 1.0V +/- 1%
    }
}
";
    let (narrowed, reference) = both(src, Catalogs::jlcpcb());
    assert_eq!(narrowed.picks.len(), 2, "{:?}", picks(&narrowed));
    assert!(messages(&narrowed).is_empty(), "{:?}", messages(&narrowed));
    assert!(
        narrowed.trials * 4 < reference.trials,
        "{} trials against {}",
        narrowed.trials,
        reference.trials
    );
}

#[test]
fn a_divider_in_a_buck_stage_with_its_frequency_resistor_is_chosen_as_before() {
    // The stage of a real kind of board: the divider of a stage and the resistor that sets the
    // frequency, among the parts of a group of ten.
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file(
            "zbk2",
            "catalogs/zbk2.csv",
            "mpn,name,label,package,source\nZBK2-A,ZBK2-A invented buck IC,ZBK2,SOT-23-6,invented for a test\n",
        )
        .unwrap();
    let (narrowed, reference) = both(&BUCK.replace(".peak &= 75V", ".peak &= 30V"), catalogs);
    assert_eq!(narrowed.picks.len(), 10, "{:?}", picks(&narrowed));
    assert!(messages(&narrowed).is_empty(), "{:?}", messages(&narrowed));
    assert!(
        narrowed.trials * 4 < reference.trials,
        "{} trials against {}",
        narrowed.trials,
        reference.trials
    );
}

#[test]
fn a_frequency_resistor_whose_window_holds_for_a_few_rows_is_found_without_trying_the_rest() {
    // f = 46.5 MHz * 1 kohm / (R + 5.2 kohm) must stay in 1.0 .. 1.1 MHz: a few rows of 40.
    let mut catalogs = Catalogs::empty();
    catalogs.insert(Catalog {
        name: "resistor".into(),
        candidates: (1..=40).map(|i| resistor(1500.0 * i as f64, 0.1)).collect(),
    });
    let src = "import std::parts::passive::Resistor

board F {
    .rt: Resistor
    .fsw: Frequency
    .fsw == 46.5MHz * 1kohm / (.rt.resistance + 5.2kohm)
    .fsw &= 1.0MHz..1.1MHz
}
";
    let (narrowed, reference) = both(src, catalogs);
    assert_eq!(narrowed.picks.len(), 1, "{:?}", picks(&narrowed));
    assert!(messages(&narrowed).is_empty(), "{:?}", messages(&narrowed));
    assert!(
        narrowed.trials < reference.trials,
        "{} trials against {}",
        narrowed.trials,
        reference.trials
    );
}

const PAIR: &str = "import std::parts::passive::Resistor

board Pair {
    .a: Resistor
    .b: Resistor
    .total: Resistance
    .total == .a.resistance + .b.resistance
    .total >= 7.9kohm
    .total <= 8.2kohm
}
";

#[test]
fn a_first_pick_that_leaves_the_second_nothing_is_undone_as_before() {
    // Only the pair 4000 + 4000 reaches the window: each of the 39 smaller first picks leaves
    // the second pick an empty interval, and the search backs up to the first.
    let (narrowed, reference) = both(PAIR, tenths());
    assert_eq!(
        picks(&narrowed)
            .iter()
            .map(|p| p.1.clone())
            .collect::<Vec<_>>(),
        vec!["4000 0.1%", "4000 0.1%"]
    );
    assert!(
        narrowed.trials * 10 < reference.trials,
        "{} trials against {}",
        narrowed.trials,
        reference.trials
    );
}

#[test]
fn a_pair_with_no_solution_is_searched_as_it_was_and_says_what_it_said() {
    // Nothing reaches 9 kohm. The narrowing leaves candidates out, finds no pick, and the group
    // is searched again with every candidate tried: the message, the trials and the work are
    // those of the reference.
    let src = PAIR
        .replace("7.9kohm", "9kohm")
        .replace("8.2kohm", "9.5kohm");
    let (narrowed, reference) = both(&src, tenths());
    assert_eq!(messages(&narrowed).len(), 1, "{:?}", messages(&narrowed));
    assert_eq!(narrowed.trials, reference.trials);
    assert_eq!(narrowed.work, reference.work);
}

#[test]
fn a_relation_that_is_not_monotone_still_finds_the_row_in_the_middle() {
    // x = t (1 - t) with t = b / 4 kohm has its peak at 2 kohm, so the rows that give x >= 0.24
    // are a band in the middle of the catalog and not an end of it. A block of rows that
    // straddles the peak is not refused, because the evaluation of its hull gives a range that
    // reaches the bound; only blocks whose whole range is below it are left out.
    let src = "import std::parts::passive::Resistor

board Peak {
    .b: Resistor
    .x: Number
    .x == .b.resistance / 4kohm * (1 - .b.resistance / 4kohm)
    .x >= 0.24
}
";
    let (narrowed, reference) = both(src, tenths());
    assert_eq!(narrowed.picks.len(), 1);
    let chosen = &narrowed.picks[0].candidate;
    let ohms: f64 = chosen.split_whitespace().next().unwrap().parse().unwrap();
    assert!((1600.0..=2400.0).contains(&ohms), "{chosen}");
    assert_eq!(narrowed.picks[0].rejected, reference.picks[0].rejected);
    assert!(narrowed.trials <= reference.trials);
}

#[test]
fn a_bound_on_a_single_column_that_the_design_wrote_filters_as_the_columns_do() {
    // `.a.resistance >= 3kohm` is a range on the column that `fits` applies, so only rows at or
    // above it are offered; with the pair's window the same pair as above is found.
    let src = PAIR.replace(
        ".total <= 8.2kohm",
        ".total <= 8.2kohm\n    .a.resistance >= 3kohm",
    );
    let (narrowed, _) = both(&src, tenths());
    assert_eq!(
        picks(&narrowed)
            .iter()
            .map(|p| p.1.clone())
            .collect::<Vec<_>>(),
        vec!["4000 0.1%", "4000 0.1%"]
    );
}

/// A buck stage and its controller, as in `joint_bank.rs`, with an invented IC.
const BUCK: &str = r#"import std::parts::passive::{Capacitor, Resistor, FerriteBead}
import std::parts::regulator::{Buck, BuckStage}
import std::sources::Rail

part Zbk2 {
    .vin: Rail
    .vin.voltage >= 3V
    .vin.voltage <= 65V
    .vin_abs_max: Voltage
    .vin_abs_max &= 80V
    .vin.peak <= .vin_abs_max
    .buck: Buck {
        .vref &= 0.8V +/- 1.5%
        .min_input_voltage &= 3V
        .max_input_voltage &= 65V
        .max_output_current &= 3A
        .efficiency &= 0.85..0.95
        .min_input_capacitance &= 2.2uF
        .min_output_capacitance &= 10uF
    }
    .rt_resistance: Resistance
    .fsw_nominal: Frequency
    .fsw_nominal == 46.5MHz * 1kohm / (.rt_resistance + 5.2kohm)
    .fsw_tolerance: Number
    .fsw_tolerance &= 0.925..1.075
    .fsw: Frequency
    .fsw == .fsw_nominal * .fsw_tolerance
    .buck.fsw == .fsw
    .buck.min_inductance == (2 * .buck.sw.voltage - .vin.voltage) / (2.5 * .fsw * 1A)
    .buck.sw.neg ~ .vin.neg
    .vin.current == -.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency)
    pins {
        .vin.pos       power   "VIN"  "1"
        .vin.neg       power   "GND"  "2"
        .buck.sw.pos   power   "SW"   "3"
        .buck.fb       input   "FB"   "4"
        .en            input   "EN"   "5"
        .rt            passive "RT"   "6"
    }
    .mpn: String
    .package: Package
    catalog zbk2 { .mpn, .package }
}

module Zbk2Buck {
    .vin: Rail
    .vout: Rail
    .fsw: Frequency
    .fsw ?= 0.8MHz..1.2MHz
    .controller: Zbk2
    .stage: BuckStage
    .frequency_resistor: Resistor
    .vin1_cap: Capacitor { .capacitance &= 1uF +/- 20% }
    .vin2_cap: Capacitor { .capacitance &= 1uF +/- 20% }

    .controller.vin ~ .vin
    .stage.vin ~ .vin
    .stage.vout ~ .vout
    .stage.buck ~ .controller.buck
    .controller.en ~ .vin.pos
    .controller.rt ~> .frequency_resistor ~> .vin.neg
    .controller.rt_resistance == .frequency_resistor.resistance
    .controller.fsw == .fsw
    .vin.pos ~> .vin1_cap ~> .vin.neg
    .vin.pos ~> .vin2_cap ~> .vin.neg
    .vin1_cap.max_voltage >= .vin.voltage * 2
    .vin2_cap.max_voltage >= .vin.voltage * 2
}

board Converter {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .peak &= 75V
        .pos.net &= "VBAT"
        .neg.net &= "GND"
    }
    .output: Rail {
        .pos.net &= "1V8"
        .neg.net &= "GND"
        .current &= 0mA..1A
        .voltage &= 1.8V +/- 3%
        .peak &= 1.9V
    }
    .filter: FerriteBead
    .reg: Zbk2Buck {
        .vin.pos.net &= "VIN"
        .vin.voltage &= 4.5V..5.3V
    }
    .supply.pos ~> .filter ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vin.voltage == .supply.voltage - .filter.drop
    .reg.vin.peak == .supply.peak
    .filter.current == .reg.vin.current
    .reg.vout ~ .output
}
"#;

/// A timing aid, not a check (a time is not asserted): run it with `cargo test -p netsmith-solve
/// --test main narrowing:: -- --ignored --nocapture` to see the buck stage and the divider alone,
/// with the narrowing off and on.
#[test]
#[ignore]
fn timing_of_the_divider_and_the_buck_stage() {
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file(
            "zbk2",
            "catalogs/zbk2.csv",
            "mpn,name,label,package,source\nZBK2-A,ZBK2-A invented buck IC,ZBK2,SOT-23-6,invented for a test\n",
        )
        .unwrap();
    let src = BUCK.replace(".peak &= 75V", ".peak &= 30V");
    for on in [false, true, false, true] {
        let start = std::time::Instant::now();
        let s = run(&src, &catalogs.clone().with_narrowing(on));
        eprintln!(
            "narrowing {on}: {:?}, {} trials, {} work",
            start.elapsed(),
            s.trials,
            s.work
        );
    }
}
