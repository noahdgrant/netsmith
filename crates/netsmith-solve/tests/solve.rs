//! The solver on the example boards and on small boards that break in particular ways.

use netsmith_check::{Checked, Domain, FieldKind, Loader, check};
use netsmith_quantity::{Dimension, Quantity};
use netsmith_solve::{Candidate, Catalog, Catalogs, Derating, Library, Solution, Value, solve};

/// The LED board as it was written when only the sample catalogs existed, with the LED's
/// forward voltage stated by the design. These tests are about the algorithm, so they keep
/// using the sample catalogs and this source. `examples/led.nsm` is solved with the JLCPCB
/// catalogs, at the end of this file.
const LED: &str = r#"import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

board LedExample {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }

    .led: Led {
        .forward_voltage &= 1.8V..2.2V
        .current         &= 8mA..12mA
    }

    .r1: Resistor {}

    /// The supply comes in through this header.
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2

    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance

    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
"#;
const LED_EXAMPLE: &str = include_str!("../../../examples/led.nsm");
const STATUS_EXAMPLE: &str = include_str!("../../../examples/status_indicator.nsm");
const REGULATOR: &str = include_str!("../../../examples/linear_regulator.nsm");

fn run_with(src: &str, catalogs: &Catalogs) -> (Checked, Solution) {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    let s = solve(&c.boards[0], catalogs);
    (c, s)
}

fn run(src: &str) -> (Checked, Solution) {
    run_with(src, &Catalogs::sample())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> (f64, f64) {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("`{path}` is {other:?}"),
    }
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1e-30);
    near(a.0, b.0) && near(a.1, b.1)
}

fn picked(s: &Solution) -> Vec<(String, String)> {
    s.picks
        .iter()
        .map(|p| (s.model.path_of(p.part), p.label.clone()))
        .collect()
}

// ---- the LED board ------------------------------------------------------------

#[test]
fn the_led_board_gets_a_300_ohm_resistor() {
    let (_, s) = run(LED);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s), [("r1".to_string(), "300".to_string())]);
    assert_eq!(s.picks[0].catalog, "resistor");
    assert!(close(range(&s, "r1.resistance"), (297.0, 303.0)));
    assert_eq!(
        s.model.domain_at("r1.package"),
        Some(&Domain::Text("0402".into()))
    );
}

#[test]
fn every_smaller_resistor_was_tried_and_rejected() {
    let (_, s) = run(LED);
    // 24 values per decade: 1 ohm to 91 ohm is 48, then 100 to 270 is 11 more.
    assert_eq!(s.picks[0].rejected, 59);
}

#[test]
fn the_chosen_resistor_keeps_the_led_current_in_range_in_the_worst_case() {
    let (_, s) = run(LED);
    let q = |p: &str| match s.model.domain_at(p) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("{p}: {other:?}"),
    };
    let current = q("supply.voltage")
        .sub(&q("led.forward_voltage"))
        .unwrap()
        .div(&q("r1.resistance"))
        .unwrap();
    assert!(q("led.current").encloses(&current).unwrap(), "{current}");
    // 2.55 V over 303 ohm at the least, 3.45 V over 297 ohm at the most.
    assert!(
        close((current.lo(), current.hi()), (2.55 / 303.0, 3.45 / 297.0)),
        "{current}"
    );
}

#[test]
fn a_part_with_no_catalog_is_reported_not_guessed() {
    let (_, s) = run(LED);
    // The sample catalogs have no `header` one either, so the connector is left alone too.
    let led: Vec<_> = s
        .unresolved
        .iter()
        .filter(|u| s.model.path_of(u.part) == "led")
        .collect();
    assert_eq!(led.len(), 1);
    assert!(led[0].reason.contains("`led` catalog"), "{}", led[0].reason);
}

#[test]
fn what_cannot_be_evaluated_is_deferred() {
    // The resistor's drop needs its through-current. The chain passes one current, so it is
    // the LED's, and the drop is worked out once a resistor is chosen. The current is the one
    // the chosen resistor lets through (the equation narrows the stated 8..12 mA window to
    // it), not the window.
    let (_, s) = run(LED);
    assert!(close(
        range(&s, "r1.current"),
        (0.008415841584158413, 0.01161616161616162)
    ));
    assert!(s.deferred.is_empty(), "{:?}", s.deferred);
    assert!(close(range(&s, "led.drop"), (1.8, 2.2)));

    // Without a resistor to choose from, its resistance is unknown and so is its drop.
    let (_, s) = run_with(LED, &Catalogs::default());
    let waiting: Vec<String> = s
        .deferred
        .iter()
        .map(|&i| s.model.show_expr(&s.model.relations()[i].lhs))
        .collect();
    assert!(waiting.contains(&"r1.drop".to_string()), "{waiting:?}");
    assert!(close(range(&s, "led.drop"), (1.8, 2.2)));
}

#[test]
fn solving_leaves_the_checked_board_alone() {
    let c = check("led.nsm", LED, &Loader::std());
    let before = c.boards[0].model.domain_at("r1.resistance").cloned();
    let _ = solve(&c.boards[0], &Catalogs::sample());
    assert_eq!(before, Some(Domain::Unset));
    assert_eq!(
        c.boards[0].model.domain_at("r1.resistance"),
        Some(&Domain::Unset)
    );
}

#[test]
fn the_order_of_declarations_does_not_change_the_pick() {
    let reordered = "\
import std::parts::passive::{Resistor, Led}
import std::sources::Rail

board LedExample {
    .led.current == (.supply.voltage - .led.forward_voltage) / .r1.resistance
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
    .r1: Resistor {}
    .led: Led {
        .current         &= 8mA..12mA
        .forward_voltage &= 1.8V..2.2V
    }
    .supply: Rail {
        .neg.net &= \"GND\"
        .pos.net &= \"VCC\"
        .voltage &= 5V +/- 5%
    }
}
";
    let (_, a) = run(LED);
    let (_, b) = run(reordered);
    assert_eq!(picked(&a), picked(&b));
    assert_eq!(
        a.model.domain_at("r1.resistance"),
        b.model.domain_at("r1.resistance")
    );
}

// ---- when nothing works ---------------------------------------------------------

#[test]
fn an_impossible_requirement_names_the_closest_candidate_and_the_numbers() {
    // A 0.2 mA window is narrower than 1% resistors and a 5% supply can hold.
    let tight = LED.replace(
        ".current         &= 8mA..12mA",
        ".current         &= 9.9mA..10.1mA",
    );
    let (c, s) = run(&tight);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("no `resistor` candidate satisfies `r1`"),
        "{}",
        msgs[0]
    );
    assert!(msgs[0].contains("closest"), "{}", msgs[0]);
    assert!(msgs[0].contains("mA"), "{}", msgs[0]);
    assert!(
        msgs[0].contains("`led.current` must stay within 9.9mA..10.1mA"),
        "{}",
        msgs[0]
    );
    // The message points at the equation the design wrote.
    let rendered = s.diagnostics[0].render(&c.sources);
    assert!(rendered.contains(".led.current == "), "{rendered}");
    // And no part was half-chosen.
    assert!(s.picks.is_empty());
    assert_eq!(s.model.domain_at("r1.resistance"), Some(&Domain::Unset));
}

#[test]
fn a_catalog_with_nothing_that_fits_says_what_was_asked_for() {
    let src = "\
import std::parts::passive::Resistor
import std::sources::Rail

board B {
    .s: Rail { .voltage &= 5V }
    .r: Resistor { .resistance &= s.voltage / 1mA }
}
";
    let (_, s) = run(src);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("the `resistor` catalog has nothing for `r`"),
        "{}",
        msgs[0]
    );
    assert!(msgs[0].contains("resistance within"), "{}", msgs[0]);
}

#[test]
fn a_catalog_with_nothing_for_a_part_points_at_the_line_that_declares_it() {
    let src = "\
import std::parts::passive::Resistor
import std::sources::Rail

board B {
    .s: Rail { .voltage &= 5V }
    .r: Resistor { .resistance &= s.voltage / 1mA }
}
";
    let (c, s) = run(src);
    let d = &s.diagnostics[0];
    let source = c.sources.get(d.source);
    assert_eq!(
        source.name, "test.nsm",
        "not the standard library's definition"
    );
    assert_eq!(&source.text[d.span.start..d.span.end], ".r: Resistor");
}

#[test]
fn a_regulator_output_no_part_offers_is_blamed_on_the_regulator_not_the_capacitors() {
    let src = REGULATOR.replace(
        ".vout.voltage &= 3.3V +/- 5%",
        ".vout.voltage &= 3.0V +/- 2%",
    );
    let (c, s) = run_with(&src, &Catalogs::jlcpcb());
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("the `ldo` catalog has nothing for `stage.regulator`"),
        "{}",
        msgs[0]
    );
    assert!(
        msgs[0].contains("vout.voltage within 2.94V..3.06V"),
        "{}",
        msgs[0]
    );
    assert!(!msgs[0].contains("capacitance"), "{}", msgs[0]);
    let d = &s.diagnostics[0];
    let source = c.sources.get(d.source);
    assert_eq!(
        &source.text[d.span.start..d.span.end],
        ".regulator:  LinearRegulator"
    );
    // The input capacitor reads only the range the board states for the input, so nothing the
    // regulator is decides it: it is chosen (ADR 0057). The output capacitor's voltage is
    // the regulator's, so it is left with it, as before. (A connector's pick is its own.)
    let left: Vec<_> = s
        .unresolved
        .iter()
        .map(|u| s.model.path_of(u.part))
        .collect();
    assert_eq!(left, ["stage.output_cap", "stage.regulator"]);
    assert!(
        s.picks
            .iter()
            .any(|p| s.model.path_of(p.part) == "stage.input_cap")
    );
}

// ---- the catalog's order is the policy -------------------------------------------

fn resistor(label: &str, ohms: f64) -> Candidate {
    let center = Quantity::literal(ohms, Some("ohm")).unwrap().quantity;
    let one_percent = Quantity::literal(1.0, Some("%")).unwrap().quantity;
    Candidate {
        name: format!("{label} 1%"),
        label: label.to_string(),
        columns: vec![
            (
                "resistance".into(),
                Value::Quantity(Quantity::tolerance(&center, &one_percent, true).unwrap()),
            ),
            ("package".into(), Value::Text("0402".into())),
            // Far above anything a test draws: the power check is not what these tests are about.
            (
                "max_power".into(),
                Value::Quantity(Quantity::literal(1.0, Some("kW")).unwrap().quantity),
            ),
        ],
        lcsc: None,
        mpn: None,
        library: None,
        assembly: None,
        generic: None,
    }
}

fn resistor_in(label: &str, ohms: f64, package: &str, lcsc: &str) -> Candidate {
    let mut c = resistor(label, ohms);
    c.columns[1] = ("package".into(), Value::Text(package.into()));
    c.lcsc = Some(lcsc.into());
    c
}

fn catalogs_with(candidates: Vec<Candidate>) -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "resistor".into(),
        candidates,
    });
    c
}

#[test]
fn the_first_candidate_that_works_wins_so_catalog_order_is_the_policy() {
    // Both 295 and 300 ohm keep the LED in range. Whichever the catalog lists first is used.
    let (_, cheap_first) = run_with(
        LED,
        &catalogs_with(vec![resistor("295", 295.0), resistor("300", 300.0)]),
    );
    let (_, other_first) = run_with(
        LED,
        &catalogs_with(vec![resistor("300", 300.0), resistor("295", 295.0)]),
    );
    assert_eq!(
        picked(&cheap_first),
        [("r1".to_string(), "295".to_string())]
    );
    assert_eq!(
        picked(&other_first),
        [("r1".to_string(), "300".to_string())]
    );
    assert_eq!(cheap_first.picks[0].rejected, 0);
}

const FULLY_NARROWED: &str = "\
import std::parts::passive::Resistor

board B {
    .r: Resistor {
        .resistance &= 226kohm +/- 1%
        .package &= \"0805\"
    }
}
";

#[test]
fn a_part_whose_every_column_is_narrowed_is_still_sourced_from_the_catalog() {
    // The 0402 row has the value but not the package; the first row that satisfies both
    // narrowings is the pick, as for a part with unset columns.
    let (_, s) = run_with(
        FULLY_NARROWED,
        &catalogs_with(vec![
            resistor_in("226k", 226e3, "0402", "C1"),
            resistor_in("226k", 226e3, "0805", "C2"),
            resistor_in("226k", 226e3, "0805", "C3"),
        ]),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks.len(), 1);
    assert_eq!(s.picks[0].lcsc.as_deref(), Some("C2"));
}

#[test]
fn a_fully_narrowed_part_that_no_row_fits_is_an_error_naming_what_it_needs() {
    let (_, s) = run_with(
        FULLY_NARROWED,
        &catalogs_with(vec![
            resistor_in("226k", 226e3, "0402", "C1"),
            resistor_in("100k", 100e3, "0805", "C2"),
        ]),
    );
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].starts_with("the `resistor` catalog has nothing for `r`, which needs"),
        "{}",
        msgs[0]
    );
    assert!(msgs[0].contains("resistance within"), "{}", msgs[0]);
    assert!(msgs[0].contains("package within"), "{}", msgs[0]);
    assert!(s.picks.is_empty());
}

#[test]
fn a_fully_narrowed_part_still_obeys_the_relations_and_leaves_other_picks_alone() {
    // The 226k part is fully narrowed; the second resistor has an unset value and must
    // always be above it, so the 100k row and the 226k row (whose range overlaps) are out.
    let src = "\
import std::parts::passive::Resistor

board B {
    .a: Resistor {
        .resistance &= 226kohm +/- 1%
        .package &= \"0805\"
    }
    .b: Resistor {}
    .b.resistance >= .a.resistance
}
";
    let rows = vec![
        resistor_in("100k", 100e3, "0402", "C1"),
        resistor_in("226k", 226e3, "0805", "C2"),
        resistor_in("300k", 300e3, "0402", "C3"),
    ];
    let (_, s) = run_with(src, &catalogs_with(rows));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(
        picked(&s),
        [
            ("a".to_string(), "226k".to_string()),
            ("b".to_string(), "300k".to_string())
        ]
    );
}

#[test]
fn candidates_that_break_the_requirement_are_skipped_whatever_their_order() {
    // 220 ohm is listed first but lets 15.8 mA through the LED.
    let (_, s) = run_with(
        LED,
        &catalogs_with(vec![resistor("220", 220.0), resistor("300", 300.0)]),
    );
    assert_eq!(picked(&s), [("r1".to_string(), "300".to_string())]);
    assert_eq!(s.picks[0].rejected, 1);
}

// ---- propagation and the regulator ------------------------------------------------

#[test]
fn the_regulator_stage_is_solved_from_the_voltage_the_board_asks_for() {
    let (_, s) = run(REGULATOR);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(
        picked(&s),
        [
            ("stage.input_cap".to_string(), "1.5u".to_string()),
            ("stage.output_cap".to_string(), "6.8u".to_string()),
            ("stage.regulator".to_string(), "LDO-3.3V".to_string()),
        ]
    );
    // The board set the stage's rail; the equality carried it into the regulator.
    assert!(close(
        range(&s, "stage.regulator.vout.voltage"),
        (3.234, 3.366)
    ));
    assert!(close(
        range(&s, "stage.regulator.vin.voltage"),
        (4.75, 5.25)
    ));
    assert!(close(range(&s, "stage.regulator.dropout"), (1.1, 1.1)));
    assert!(close(
        range(&s, "stage.input_cap.max_voltage"),
        (16.0, 16.0)
    ));
    assert!(s.deferred.is_empty(), "the headroom check was evaluated");
}

#[test]
fn a_regulator_without_headroom_fails_with_the_numbers() {
    let low = REGULATOR.replace(".voltage &= 5V +/- 5%", ".voltage &= 3.5V +/- 5%");
    let (_, s) = run(&low);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    // The regulator is the cause whatever the capacitors are, and the message says so alone
    // (ADR 0057); it used to name every part searched with it.
    assert!(
        msgs[0].contains(
            "no `ldo` candidate satisfies `stage.regulator`: the closest of the 1 that fit"
        ),
        "{}",
        msgs[0]
    );
    assert!(msgs[0].contains("must always be at least"), "{}", msgs[0]);
    assert!(msgs[0].contains("3.325V..3.675V"), "{}", msgs[0]);
}

#[test]
fn connected_rails_that_disagree_are_reported() {
    let src = "\
import std::sources::Rail

board B {
    .a: Rail { .voltage &= 5V }
    .b: Rail { .voltage &= 3V }
    .a ~ .b
}
";
    let (_, s) = run(src);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(msgs[0].contains("connected values disagree"), "{}", msgs[0]);
}

#[test]
fn a_value_flows_through_a_chain_of_connected_rails() {
    let src = "\
import std::sources::Rail

board B {
    .a: Rail { .voltage &= 5V }
    .b: Rail
    .c: Rail
    .a ~ .b
    .b ~ .c
}
";
    let (_, s) = run(src);
    assert!(s.diagnostics.is_empty());
    assert!(close(range(&s, "c.voltage"), (5.0, 5.0)));
}

#[test]
fn solving_twice_gives_the_same_answer() {
    let (_, a) = run(REGULATOR);
    let (_, b) = run(REGULATOR);
    assert_eq!(picked(&a), picked(&b));
    assert_eq!(a.deferred, b.deferred);
}

// ---- parts that depend on each other -------------------------------------------------

const DIVIDER: &str = "\
import std::parts::passive::Resistor
import std::sources::Rail

board Divider {
    .supply: Rail { .voltage &= 5V +/- 1% }
    .out: Rail { .voltage &= 3.3V +/- 5% }

    .r_top: Resistor
    .r_bottom: Resistor

    .supply.pos ~> .r_top ~> .out.pos
    .out.pos ~> .r_bottom ~> .supply.neg

    .out.voltage == .supply.voltage * .r_bottom.resistance / (.r_top.resistance + .r_bottom.resistance)
}
";

#[test]
fn two_unknown_resistors_in_one_relation_are_solved_together() {
    let (_, s) = run(DIVIDER);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks.len(), 2);

    // Whatever was chosen really does hold the output inside its limits, checked from the
    // formula's exact corners, not by the solver's own arithmetic.
    let (lo, hi) = divider_true_range(&s);
    let (limit_lo, limit_hi) = range(&s, "out.voltage");
    assert!(
        limit_lo <= lo && hi <= limit_hi,
        "{lo}..{hi} is outside {limit_lo}..{limit_hi}"
    );
}

/// The exact range of `supply * r_bottom / (r_top + r_bottom)`: it rises with the supply and
/// with `r_bottom` and falls with `r_top`, so each end is at a corner.
fn divider_true_range(s: &Solution) -> (f64, f64) {
    let (vs, rb, rt) = (
        range(s, "supply.voltage"),
        range(s, "r_bottom.resistance"),
        range(s, "r_top.resistance"),
    );
    (vs.0 * rb.0 / (rt.1 + rb.0), vs.1 * rb.1 / (rt.0 + rb.1))
}

#[test]
fn the_first_part_is_reconsidered_when_the_second_has_nothing_that_works() {
    let (_, s) = run(DIVIDER);
    // `r_bottom` is picked first. 1 ohm cannot work with any `r_top`, so it was backed out
    // of and a later candidate used.
    let bottom = s
        .picks
        .iter()
        .find(|p| s.model.path_of(p.part) == "r_bottom")
        .unwrap();
    assert!(
        bottom.rejected > 0,
        "r_bottom was accepted without being checked"
    );
}

#[test]
fn a_pair_with_no_working_combination_is_reported_together_and_leaves_nothing_chosen() {
    // The output cannot be higher than the supply.
    let src = DIVIDER.replace(
        ".out: Rail { .voltage &= 3.3V +/- 5% }",
        ".out: Rail { .voltage &= 6V +/- 1% }",
    );
    let (_, s) = run(&src);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("no combination of candidates satisfies `r_bottom` and `r_top` together"),
        "{}",
        msgs[0]
    );
    assert!(msgs[0].contains("r_bottom = "), "{}", msgs[0]);
    assert!(s.picks.is_empty());
    assert_eq!(
        s.model.domain_at("r_bottom.resistance"),
        Some(&Domain::Unset)
    );
    assert_eq!(s.unresolved.len(), 2);
}

#[test]
fn a_part_with_no_valid_choice_does_not_undo_parts_unrelated_to_it() {
    // `r_ok` and `r_bad` share no relation, so `r_bad` having no candidate leaves `r_ok` picked.
    let src = "\
import std::parts::passive::Resistor

board B {
    .r_ok: Resistor { .resistance &= 1kohm +/- 1% }
    .r_bad: Resistor { .resistance &= 1ohm +/- 0.0001% }
}
";
    let (_, s) = run(src);
    assert_eq!(messages(&s).len(), 1, "{:?}", messages(&s));
    let picked: Vec<_> = s.picks.iter().map(|p| s.model.path_of(p.part)).collect();
    assert_eq!(picked, ["r_ok"]);
}

// ---- a capacitor must be rated for its rail --------------------------------------------

#[test]
fn the_regulator_stage_picks_capacitors_rated_for_their_rails() {
    let (_, s) = run_with(REGULATOR, &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    // 5.25 V * 1.5 on the input and 3.465 V * 1.5 on the output.
    assert!(range(&s, "stage.input_cap.max_voltage").0 >= 5.25 * 1.5);
    assert!(range(&s, "stage.output_cap.max_voltage").0 >= 3.465 * 1.5);
    assert!(s.deferred.is_empty(), "the rating rules were evaluated");
}

#[test]
fn a_capacitor_rated_too_low_for_its_rail_is_refused_with_the_numbers() {
    // Five times the rail is more than the sample capacitors, all 16 V, are rated for.
    let strict = REGULATOR.replace(".vout.voltage * 1.5", ".vout.voltage * 5");
    let (_, s) = run(&strict);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(msgs[0].contains("`stage.output_cap`"), "{}", msgs[0]);
    assert!(msgs[0].contains("must always be at least"), "{}", msgs[0]);
    assert!(msgs[0].contains("17.3"), "{}", msgs[0]);
}

// ---- the examples with real parts ---------------------------------------------------

/// `path: LCSC number, Basic or Extended` for every pick but a connector's: the `header`
/// catalog rates a connector's contacts (ADR 0040) and has its own test.
fn sourced(s: &Solution) -> Vec<(String, String, Library)> {
    s.picks
        .iter()
        .filter(|p| p.catalog != "header")
        .map(|p| {
            (
                s.model.path_of(p.part),
                p.lcsc.clone().expect("a real part has an LCSC number"),
                p.library.expect("a real part has a library"),
            )
        })
        .collect()
}

fn real(src: &str) -> (Checked, Solution) {
    let (c, s) = run_with(src, &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    (c, s)
}

fn entry(path: &str, lcsc: &str, library: Library) -> (String, String, Library) {
    (path.to_string(), lcsc.to_string(), library)
}

#[test]
fn the_led_example_gets_the_basic_red_led_and_the_basic_330_ohm_resistor() {
    let (_, s) = real(LED_EXAMPLE);
    assert_eq!(
        sourced(&s),
        [
            entry("led", "C2286", Library::Basic),
            entry("r1", "C25104", Library::Basic)
        ]
    );
    assert_eq!(picked(&s)[1].1, "330");
    assert_eq!(s.picks[1].mpn.as_deref(), Some("0402WGF3300TCE"));
    // The LED's range came from the catalog, and the current stays inside 5..15 mA.
    assert!(close(range(&s, "led.forward_voltage"), (1.8, 2.4)));
    let (lo, hi) = range(&s, "led.current");
    assert!(lo >= 0.005 && hi <= 0.015, "{lo}..{hi}");
}

#[test]
fn changing_only_the_catalog_order_changes_the_resistor_pick() {
    // Ordered by value alone, as the sample catalog is, the 240 ohm resistor (Extended)
    // is the first that keeps the LED in range. The design and the parts are the same.
    let mut by_value = Catalogs::jlcpcb();
    let kinds = [
        (
            "resistance".to_string(),
            FieldKind::Quantity(Dimension::RESISTANCE),
        ),
        (
            "max_power".to_string(),
            FieldKind::Quantity(Dimension::POWER),
        ),
        ("package".to_string(), FieldKind::Text),
        (
            "temperature_coefficient".to_string(),
            FieldKind::Quantity(Dimension::NONE),
        ),
    ];
    let mut resistors = by_value.resolve("resistor", &kinds, &[]).unwrap().unwrap();
    resistors.candidates.sort_by(|a, b| {
        let ohms = |c: &Candidate| match c.column("resistance") {
            Some(Value::Quantity(q)) => q.mid(),
            _ => panic!(),
        };
        ohms(a).total_cmp(&ohms(b))
    });
    by_value.insert(resistors);

    let (_, s) = run_with(LED_EXAMPLE, &by_value);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(
        sourced(&s),
        [
            entry("led", "C2286", Library::Basic),
            entry("r1", "C25094", Library::Extended)
        ]
    );
    assert!(s.picks[1].rejected > 0, "100 ohm was tried and rejected");
}

#[test]
fn a_design_can_ask_for_a_package_and_gets_a_part_of_it() {
    // Several packages hold most values, and the cheapest Basic part wins, so a design that
    // needs one says so.
    for package in ["0402", "0603"] {
        let src = LED_EXAMPLE.replace(
            ".r1: Resistor {}",
            &format!(".r1: Resistor {{ .package &= \"{package}\" }}"),
        );
        let (_, s) = real(&src);
        let r1 = s
            .picks
            .iter()
            .find(|p| s.model.path_of(p.part) == "r1")
            .unwrap();
        assert!(r1.candidate.contains(package), "{}", r1.candidate);
    }
}

#[test]
fn the_status_indicator_gets_the_green_led_a_1k_resistor_and_a_100n_capacitor() {
    let (_, s) = real(STATUS_EXAMPLE);
    assert_eq!(
        sourced(&s),
        [
            entry("status_indicator.bypass", "C1525", Library::Basic),
            entry("status_indicator.current_limit", "C11702", Library::Basic),
            entry("status_indicator.led", "C125094", Library::Extended),
        ]
    );
}

#[test]
fn the_regulator_example_gets_the_xc6206_and_two_basic_capacitors() {
    let (_, s) = real(REGULATOR);
    assert_eq!(
        sourced(&s),
        [
            entry("stage.input_cap", "C315248", Library::Extended),
            entry("stage.output_cap", "C59461", Library::Basic),
            entry("stage.regulator", "C5446", Library::Basic),
        ]
    );
    let ldo = s.picks.iter().find(|p| p.catalog == "ldo").unwrap();
    assert_eq!(ldo.mpn.as_deref(), Some("XC6206P332MR-G"));
    assert!(close(
        range(&s, "stage.regulator.vout.voltage"),
        (3.135, 3.465)
    ));
    assert!(s.deferred.is_empty(), "the headroom check was evaluated");
}

#[test]
fn the_regulator_must_still_have_headroom_with_the_real_dropout() {
    // 3.3 V in leaves no room for 3.135 V out plus 0.68 V of dropout.
    let low = REGULATOR.replace(".voltage &= 5V +/- 5%", ".voltage &= 3.8V +/- 5%");
    let (_, s) = run_with(&low, &Catalogs::jlcpcb());
    assert!(
        messages(&s)
            .iter()
            .any(|m| m.contains("always be at least")),
        "{:?}",
        messages(&s)
    );
}

// ---- effective capacitance -------------------------------------------------------------

/// A 5 V +/- 5% rail with one capacitor across it, stating `need`, rated for one and a half
/// times the rail. The capacitor's bias comes from the rail it sits across.
fn bypass(need: &str) -> String {
    bypass_on("5V +/- 5%", need, "")
}

fn bypass_on(rail: &str, need: &str, extra: &str) -> String {
    format!(
        "import std::parts::passive::Capacitor
import std::sources::Rail

board B {{
    .rail: Rail {{ .voltage &= {rail} }}
    .c: Capacitor {{ {need} }}
    {extra}
    .c.max_voltage >= .rail.voltage * 1.5
    .rail.pos ~> .c ~> .rail.neg
}}
"
    )
}

/// The same capacitor, but between the rail and the positive pin of another rail, so it is
/// across no rail.
fn loose(need: &str, extra: &str) -> String {
    format!(
        "import std::parts::passive::Capacitor
import std::sources::Rail

board B {{
    .rail: Rail {{ .voltage &= 5V +/- 5% }}
    .aux: Rail {{ .voltage &= 3V +/- 5% }}
    .c: Capacitor {{ {need} }}
    {extra}
    .rail.pos ~> .c ~> .aux.pos
}}
"
    )
}

/// A capacitor of `farads` +/- 10% rated `rated` V that keeps what `curve` says.
fn capacitor(label: &str, farads: f64, rated: f64, curve: &[(f64, f64)]) -> Candidate {
    let center = Quantity::literal(farads, Some("F")).unwrap().quantity;
    let ten_percent = Quantity::literal(10.0, Some("%")).unwrap().quantity;
    let marked = Quantity::tolerance(&center, &ten_percent, true).unwrap();
    Candidate {
        name: format!("{label} cap"),
        label: label.to_string(),
        columns: vec![
            ("capacitance".into(), Value::Quantity(marked)),
            (
                "effective_capacitance".into(),
                Value::Derated(Derating {
                    marked,
                    against: "bias".into(),
                    curve: curve.to_vec(),
                }),
            ),
            (
                "max_voltage".into(),
                Value::Quantity(Quantity::literal(rated, Some("V")).unwrap().quantity),
            ),
            ("package".into(), Value::Text("0402".into())),
        ],
        lcsc: None,
        mpn: None,
        library: None,
        assembly: None,
        generic: None,
    }
}

fn capacitors_only(candidates: Vec<Candidate>) -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "capacitor".into(),
        candidates,
    });
    c
}

/// Half left at 10 V, falling in a straight line.
const HALF_AT_10V: [(f64, f64); 2] = [(0.0, 1.0), (10.0, 0.5)];

#[test]
fn the_bias_of_a_capacitor_across_a_rail_is_that_rails_voltage() {
    let catalogs = capacitors_only(vec![capacitor("2.2u", 2.2e-6, 16.0, &HALF_AT_10V)]);
    let (_, s) = run_with(&bypass(".effective_capacitance >= 1uF"), &catalogs);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "c.bias"), (4.75, 5.25)));
}

#[test]
fn the_requirement_applies_to_the_effective_capacitance_not_the_marked_value() {
    // At 4.75..5.25 V the curve keeps 0.7375..0.7625, so 1 uF +/- 10% is worth
    // 0.9 * 0.7375 = 664 nF at the least, short of 1 uF. The 2.2 uF is not.
    let catalogs = capacitors_only(vec![
        capacitor("1u", 1e-6, 16.0, &HALF_AT_10V),
        capacitor("2.2u", 2.2e-6, 16.0, &HALF_AT_10V),
    ]);
    let (_, s) = run_with(&bypass(".effective_capacitance >= 1uF"), &catalogs);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s), [("c".to_string(), "2.2u".to_string())]);
    assert_eq!(s.picks[0].rejected, 1);
    // The marked value keeps its tolerance and nothing else; the effective one has the
    // tolerance and the bias in the worst case.
    assert!(close(range(&s, "c.capacitance"), (1.98e-6, 2.42e-6)));
    let (lo, hi) = range(&s, "c.effective_capacitance");
    assert!((lo - 1.98e-6 * 0.7375).abs() < 1e-12, "{lo}");
    assert!((hi - 2.42e-6 * 0.7625).abs() < 1e-12, "{hi}");
}

#[test]
fn a_window_on_the_effective_capacitance_is_a_window_on_the_derated_value() {
    // `&=` narrows what is left after derating, so a part whose derated range pokes out of
    // the window does not fit, and one inside it does.
    let catalogs = capacitors_only(vec![
        capacitor("1u", 1e-6, 16.0, &HALF_AT_10V),
        capacitor("2.2u", 2.2e-6, 16.0, &HALF_AT_10V),
    ]);
    let (_, s) = run_with(&bypass(".effective_capacitance &= 1.4uF..1.9uF"), &catalogs);
    assert_eq!(picked(&s), [("c".to_string(), "2.2u".to_string())]);
}

#[test]
fn a_capacitor_with_no_curve_is_refused_when_the_design_asks_for_effective_capacitance() {
    let catalogs = capacitors_only(vec![capacitor("22u", 22e-6, 16.0, &[])]);
    let (_, s) = run_with(&bypass(".effective_capacitance >= 1uF"), &catalogs);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("`c.effective_capacitance` is unknown")
            && msgs[0].contains("no derating data for 1"),
        "{}",
        msgs[0]
    );
    assert!(s.picks.is_empty());
}

#[test]
fn a_capacitor_with_no_curve_is_fine_when_the_design_only_names_its_marked_value() {
    let catalogs = capacitors_only(vec![capacitor("22u", 22e-6, 16.0, &[])]);
    let (_, s) = run_with(&bypass(".capacitance &= 22uF +/- 10%"), &catalogs);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s), [("c".to_string(), "22u".to_string())]);
    assert_eq!(
        s.model.domain_at("c.effective_capacitance"),
        Some(&Domain::Unset),
        "nothing was asked, so nothing was worked out"
    );
}

#[test]
fn a_capacitor_across_no_rail_has_no_effective_capacitance_to_check() {
    let catalogs = capacitors_only(vec![capacitor("2.2u", 2.2e-6, 16.0, &HALF_AT_10V)]);
    let (_, s) = run_with(&loose(".effective_capacitance >= 1uF", ""), &catalogs);
    assert_eq!(s.model.domain_at("c.bias"), Some(&Domain::Unset));
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("`c.bias` is not known")
            && msgs[0].contains("`c.effective_capacitance`")
            && msgs[0].contains("place `c` between a rail's pos and neg")
            && msgs[0].contains("or state `c.bias`"),
        "{}",
        msgs[0]
    );
}

#[test]
fn a_capacitor_across_no_rail_is_fine_when_it_has_no_effective_requirement() {
    let catalogs = capacitors_only(vec![capacitor("2.2u", 2.2e-6, 16.0, &[])]);
    let (_, s) = run_with(&loose(".capacitance &= 2.2uF +/- 10%", ""), &catalogs);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s), [("c".to_string(), "2.2u".to_string())]);
}

#[test]
fn a_capacitor_across_no_rail_works_when_the_design_states_its_bias() {
    let catalogs = capacitors_only(vec![capacitor("2.2u", 2.2e-6, 16.0, &HALF_AT_10V)]);
    let (_, s) = run_with(
        &loose(".effective_capacitance >= 1uF", ".c.bias &= 2V..3V"),
        &catalogs,
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(picked(&s), [("c".to_string(), "2.2u".to_string())]);
}

#[test]
fn a_stated_bias_that_disagrees_with_the_rail_is_a_contradiction() {
    let catalogs = capacitors_only(vec![capacitor("2.2u", 2.2e-6, 16.0, &HALF_AT_10V)]);
    let (_, s) = run_with(
        &bypass_on(
            "5V +/- 5%",
            ".effective_capacitance >= 1uF",
            ".c.bias &= 3V",
        ),
        &catalogs,
    );
    let msgs = messages(&s);
    assert!(
        msgs.iter()
            .any(|m| m.contains("`c.bias` must stay within 3V")
                && m.contains("rail.voltage works out to 4.75V..5.25V")),
        "{msgs:#?}"
    );
}

#[test]
fn a_stated_bias_that_agrees_with_the_rail_is_fine() {
    let catalogs = capacitors_only(vec![capacitor("2.2u", 2.2e-6, 16.0, &HALF_AT_10V)]);
    let (_, s) = run_with(
        &bypass_on(
            "5V +/- 5%",
            ".effective_capacitance >= 1uF",
            ".c.bias == .rail.voltage",
        ),
        &catalogs,
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
}

#[test]
fn a_bias_past_the_end_of_the_curve_is_refused_with_both_voltages() {
    let catalogs = capacitors_only(vec![capacitor(
        "2.2u",
        2.2e-6,
        16.0,
        &[(0.0, 1.0), (4.0, 0.6)],
    )]);
    let (_, s) = run_with(&bypass(".effective_capacitance >= 1uF"), &catalogs);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("data only up to 4 V, but the bias reaches 5.25 V, for 1"),
        "{}",
        msgs[0]
    );
}

#[test]
fn an_effective_capacitor_that_falls_short_is_reported_with_the_numbers() {
    let catalogs = capacitors_only(vec![capacitor("1u", 1e-6, 16.0, &HALF_AT_10V)]);
    let (_, s) = run_with(&bypass(".effective_capacitance >= 1uF"), &catalogs);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("c.effective_capacitance must always be at least 1uF")
            && msgs[0].contains("is not always above 1uF"),
        "{}",
        msgs[0]
    );
    assert!(msgs[0].contains("663.7nF..838.8nF"), "{}", msgs[0]);
}

#[test]
fn the_real_catalog_leaves_the_1u_25v_part_too_small_and_a_curveless_part_unused() {
    // The only 22 uF part rated for 5 V is the Murata one, which has no curve, and the 10 uF
    // 10 V part keeps about 1.8 uF at 5 V. Nothing rated for 5 V keeps 5 uF. The parts with
    // no curve (the Murata, the other makers', the Samsung class II parts of 2.2 uF and more
    // measured at 1 Vrms) are counted, not used.
    let (_, s) = run_with(
        &bypass(".effective_capacitance >= 5uF"),
        &Catalogs::jlcpcb(),
    );
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    // A row whose curve cell has no points.
    let without = include_str!("../../../data/jlcpcb/catalog/capacitor.csv")
        .lines()
        .filter(|l| l.contains(",derate capacitance by bias:,"))
        .count();
    assert!(
        msgs[0].contains("`c.effective_capacitance` is unknown")
            && msgs[0].contains(&format!("no derating data for {without}")),
        "{}",
        msgs[0]
    );
}

#[test]
fn the_real_catalog_picks_the_cheapest_part_whose_effective_value_is_enough() {
    // 1 uF at 5 V: the Basic 4.7 uF 10 V part keeps only about 0.8 uF at the least and the
    // Basic 1 uF 25 V part about 0.5 uF, so the Extended 10 uF 10 V part is the first that works.
    let (_, s) = run_with(
        &bypass(".effective_capacitance >= 1uF"),
        &Catalogs::jlcpcb(),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].lcsc.as_deref(), Some("C315248"));
    assert!(range(&s, "c.effective_capacitance").0 >= 1e-6);
}

#[test]
fn a_4u7_part_passes_as_a_marked_value_but_fails_as_an_effective_one_at_3v3() {
    // The Basic 4.7 uF 10 V X5R part is marked 4.7 uF +/- 20%, and keeps about a third of it
    // at 3.3 V. Asked for as marked it is the pick; asked for as 4.7 uF effective it is not.
    let rail = "3.3V +/- 5%";
    let (_, s) = run_with(
        &bypass_on(rail, ".capacitance &= 4.7uF +/- 20%", ""),
        &Catalogs::jlcpcb(),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].lcsc.as_deref(), Some("C23733"));
    let (lo, _) = range(&s, "c.capacitance");
    assert!(lo >= 3.7e-6, "{lo}");

    let (_, s) = run_with(
        &bypass_on(rail, ".effective_capacitance >= 4.7uF", ""),
        &Catalogs::jlcpcb(),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_ne!(s.picks[0].lcsc.as_deref(), Some("C23733"));
    assert!(range(&s, "c.effective_capacitance").0 >= 4.7e-6);
}

#[test]
fn the_4u7_part_alone_is_reported_with_its_effective_range_at_3v3() {
    let (_, s) = run_with(
        &bypass_on("3.3V +/- 5%", ".effective_capacitance >= 4.7uF", ""),
        &capacitors_only(vec![capacitor(
            "4.7u",
            4.7e-6,
            10.0,
            &[(0.0, 1.0), (10.0, 0.2)],
        )]),
    );
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("c.effective_capacitance must always be at least 4.7uF")
            && msgs[0].contains("is not always above 4.7uF"),
        "{}",
        msgs[0]
    );
}

// ---- a variable that appears twice -------------------------------------------------

fn rhs_of(s: &Solution, lhs: &str) -> netsmith_check::RExpr {
    s.model
        .relations()
        .iter()
        .find(|r| s.model.show_expr(&r.lhs) == lhs)
        .unwrap_or_else(|| panic!("no relation for {lhs}"))
        .rhs
        .clone()
}

fn value(s: &Solution, e: &netsmith_check::RExpr) -> (f64, f64) {
    match netsmith_solve::eval(&s.model, e).unwrap() {
        netsmith_solve::Val::Q(q) => (q.lo(), q.hi()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_divider_is_evaluated_to_its_true_range_not_a_wider_one() {
    let (_, s) = run(DIVIDER);
    let got = value(&s, &rhs_of(&s, "out.voltage"));
    let want = divider_true_range(&s);
    // Equal to the corners but for the outward rounding of the arithmetic.
    assert!(
        close(got, want) || ((got.0 - want.0).abs() < 1e-9 && (got.1 - want.1).abs() < 1e-9),
        "{got:?} vs {want:?}"
    );
}

#[test]
fn a_divider_only_the_true_range_can_prove_is_accepted() {
    // Naive interval arithmetic spreads this divider over about +/-3%, which is just outside
    // a +/-3% limit; its true spread is about +/-1.7%. It used to be rejected.
    let src = DIVIDER.replace(
        ".out: Rail { .voltage &= 3.3V +/- 5% }",
        ".out: Rail { .voltage &= 3.3V +/- 3% }",
    );
    let (_, s) = run(&src);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks.len(), 2);
    let (lo, hi) = divider_true_range(&s);
    assert!(lo >= 3.3 * 0.97 && hi <= 3.3 * 1.03, "{lo}..{hi}");
}

#[test]
fn a_formula_that_is_not_monotone_is_cut_into_pieces() {
    // x * x over -1..2 is 0..4. Interval arithmetic alone says -2..4.
    let src = "\
import std::sources::Rail

board B {
    .a: Rail { .voltage &= -1V..2V }
    .b: Rail
    .b.voltage == .a.voltage * .a.voltage / 1V
}
";
    let (_, s) = run(src);
    let (lo, hi) = range(&s, "b.voltage");
    assert!((4.0..4.0001).contains(&hi), "the top is exact: {hi}");
    assert!(
        lo >= -0.1,
        "far tighter than -2, though not exactly 0: {lo}"
    );
}

#[test]
fn a_variable_subtracted_from_itself_is_zero_not_a_wide_range() {
    let src = "\
import std::sources::Rail

board B {
    .a: Rail { .voltage &= 1V..5V }
    .b: Rail
    .b.voltage == .a.voltage - .a.voltage
}
";
    let (_, s) = run(src);
    let (lo, hi) = range(&s, "b.voltage");
    assert!(lo.abs() < 1e-9 && hi.abs() < 1e-9, "{lo}..{hi}");
}

#[test]
fn an_equation_gives_a_field_with_no_limit_the_computed_value() {
    let src = "\
import std::sources::Rail

board B {
    .a: Rail { .voltage &= 3V..4V }
    .b: Rail
    .c: Rail
    .b.voltage == .a.voltage * 2
    .c.voltage == 5V
}
";
    let (_, s) = run(src);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    let (lo, hi) = range(&s, "b.voltage");
    assert!(
        (lo - 6.0).abs() < 1e-9 && (hi - 8.0).abs() < 1e-9,
        "{lo}..{hi}"
    );
    assert_eq!(range(&s, "c.voltage"), (5.0, 5.0));
}

#[test]
fn an_equation_must_stay_within_the_range_the_field_already_has() {
    let src = "\
import std::sources::Rail

board B {
    .a: Rail { .voltage &= 3V..4V }
    .b: Rail { .voltage &= 5V..7V }
    .b.voltage == .a.voltage * 2
}
";
    let (_, s) = run(src);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:?}");
    assert!(
        msgs[0].contains("`b.voltage` must stay within 5V..7V") && msgs[0].contains("6V..8V"),
        "{}",
        msgs[0]
    );
}

// ---- load current ---------------------------------------------------------------------

/// The regulator example with a different load on its output.
fn with_load(load: &str) -> String {
    REGULATOR.replace(".current &= 0mA..50mA", &format!(".current &= {load}"))
}

#[test]
fn a_load_on_the_output_flows_back_through_the_regulator_to_the_input() {
    let (_, s) = run(REGULATOR);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    // The output rail draws 0..50 mA, so what supplies it gives that much (negative)...
    assert!(close(range(&s, "stage.vout.current"), (-0.05, 0.0)));
    assert!(close(
        range(&s, "stage.regulator.vout.current"),
        (-0.05, 0.0)
    ));
    // ...the regulator draws the same from its input (no quiescent current)...
    assert!(close(range(&s, "stage.regulator.vin.current"), (0.0, 0.05)));
    assert!(close(range(&s, "stage.vin.current"), (0.0, 0.05)));
    // ...and the board's input supplies it, which is a negative draw.
    assert!(close(range(&s, "input.current"), (-0.05, 0.0)));
}

#[test]
fn a_load_within_the_regulators_limits_works_with_the_real_part() {
    let (_, s) = real(&with_load("0mA..100mA"));
    assert_eq!(
        sourced(&s)
            .iter()
            .find(|(p, ..)| p == "stage.regulator")
            .map(|(_, l, _)| l.as_str()),
        Some("C5446")
    );
}

#[test]
fn a_load_beyond_the_regulators_current_names_the_current_and_the_limit() {
    let (_, s) = run_with(&with_load("0mA..150mA"), &Catalogs::jlcpcb());
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    let m = &msgs[0];
    assert!(
        m.contains("no `ldo` candidate satisfies `stage.regulator`")
            && m.contains(
                "-stage.regulator.vout.current must always be at most stage.regulator.max_current"
            )
            && m.contains("0A..150mA")
            && m.contains("100mA"),
        "{m}"
    );
    assert!(
        s.unresolved
            .iter()
            .any(|u| s.model.path_of(u.part) == "stage.regulator")
    );
}

#[test]
fn a_load_the_regulator_cannot_survive_the_heat_of_is_refused_with_the_power() {
    // 6 V in, 3.3 V out: 2.9 V across the regulator. 100 mA of that is 0.29 W against the
    // 250 mW of the package, though 100 mA is within its current rating.
    let hot = with_load("0mA..100mA").replace("5V +/- 5%", "6V");
    let (_, s) = run_with(&hot, &Catalogs::jlcpcb());
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("must always be at most stage.regulator.max_power")
            && msgs[0].contains("250mW"),
        "{}",
        msgs[0]
    );
    // The same board at half the load is fine.
    let (_, ok) = real(&with_load("0mA..50mA").replace("5V +/- 5%", "6V"));
    assert!(ok.diagnostics.is_empty());
}

#[test]
fn a_load_with_nothing_to_supply_it_is_reported() {
    let src = "\
import std::sources::Rail

board B {
    .out: Rail {
        .pos.net &= \"3V3\"
        .current &= 10mA..20mA
    }
}
";
    let (_, s) = run(src);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("the currents on net `3V3` do not add up")
            && msgs[0].contains("`out.current` is 10mA..20mA"),
        "{}",
        msgs[0]
    );
}

#[test]
fn a_supply_that_feeds_only_parts_is_taken_as_stated() {
    // The load is a part, which does not add to a net's sum yet, so there is no rail to
    // answer for the current the input gives; it stands as written.
    let src = "\
import std::sources::Rail

board B {
    .input: Rail {
        .pos.net &= \"V\"
        .current &= -20mA..-5mA
    }
}
";
    let (_, s) = run(src);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "input.current"), (-0.02, -0.005)));
}

#[test]
fn a_rail_that_feeds_several_loads_gives_the_sum_of_them() {
    let src = "\
import std::sources::Rail

board B {
    .supply: Rail { .pos.net &= \"V\" }
    .a: Rail { .current &= 10mA..20mA }
    .b: Rail { .current &= 5mA..8mA }
    .supply.pos ~ .a.pos
    .supply.pos ~ .b.pos
}
";
    let (_, s) = run(src);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "supply.current"), (-0.028, -0.015)));
    // Neither load was equated with the supply or with the other.
    assert!(close(range(&s, "a.current"), (0.01, 0.02)));
    assert!(close(range(&s, "b.current"), (0.005, 0.008)));
}

#[test]
fn two_supplies_that_cannot_cover_the_load_are_reported() {
    let src = "\
import std::sources::Rail

board B {
    .a: Rail { .current &= -10mA..-5mA }
    .b: Rail { .current &= -2mA..-1mA }
    .load: Rail { .current &= 20mA..30mA }
    .a.pos ~ .b.pos
    .a.pos ~ .load.pos
}
";
    let (_, s) = run(src);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("`a.current` is -10mA..-5mA")
            && msgs[0].contains("must add up to 5mA..10mA")
            && msgs[0].contains("only give 18mA..29mA"),
        "{}",
        msgs[0]
    );
}

#[test]
fn the_currents_of_connected_rails_are_not_equated() {
    let (c, _) = run(REGULATOR);
    let m = &c.board("RegulatorExample").unwrap().model;
    assert!(
        m.equalities()
            .iter()
            .all(|&(a, _)| !m.path_of_field(a).ends_with(".current"))
    );
    assert!(!m.balances().is_empty());
}

// ---- parts add to the net's current (ADR 0014) -----------------------------------------

/// The regulator example with `count` branches on its output, each a resistor and an LED in
/// series between the output rail's pins. `drawing` replaces what the outside is said to
/// draw.
fn with_branches(count: usize, resistor: &str, drawing: &str) -> String {
    let branches: String = (1..=count)
        .map(|i| {
            format!(
                "    .b{i}_r: Resistor {{ .resistance &= {resistor} }}
    .b{i}_led: Led {{ .forward_voltage &= 1.8V..2.4V }}
    .b{i}_led.current == (.output.voltage - .b{i}_led.forward_voltage) / .b{i}_r.resistance
    .output.pos ~> .b{i}_r ~> .b{i}_led ~> .output.neg
"
            )
        })
        .collect();
    with_load(drawing)
        .replace(
            "    .input ~ .stage.vin\n",
            &format!("{branches}    .input ~ .stage.vin\n"),
        )
        .replace(
            "import std::parts::passive::Capacitor",
            "import std::parts::passive::{Capacitor, Resistor, Led}",
        )
}

fn with_branch(resistor: &str, drawing: &str) -> String {
    with_branches(1, resistor, drawing)
}

#[test]
fn a_branch_of_parts_on_the_output_is_drawn_from_the_regulator() {
    let (_, s) = real(&with_branch("100ohm +/- 1%", "0mA..50mA"));
    // 3.3 V +/- 5% across 100 ohm +/- 1% and a 1.8 V to 2.4 V LED: about 7 to 17 mA. The
    // resistor and the LED pass the same current.
    let (lo, hi) = (3.135_f64 - 2.4, 3.465_f64 - 1.8);
    let branch = (lo / 101.0, hi / 99.0);
    assert!(close(range(&s, "b1_led.current"), branch));
    assert!(close(range(&s, "b1_r.current"), branch));
    // The output rail's 0 to 50 mA and the branch both come out of the regulator.
    assert!(close(
        range(&s, "stage.regulator.vout.current"),
        (-0.05 - branch.1, -branch.0)
    ));
    assert!(close(
        range(&s, "input.current"),
        (-0.05 - branch.1, -branch.0)
    ));
}

#[test]
fn an_led_branch_counts_against_the_regulators_current() {
    // 90 mA on the output alone is within the regulator's 100 mA; with the branch on the
    // same net it is not.
    let (_, ok) = real(&with_load("0mA..90mA"));
    assert!(ok.diagnostics.is_empty());
    let (_, s) = run_with(
        &with_branch("100ohm +/- 1%", "0mA..90mA"),
        &Catalogs::jlcpcb(),
    );
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    let m = &msgs[0];
    // The limit, the regulator's current, and the terms that make it up on the net.
    for needle in [
        "-stage.regulator.vout.current must always be at most stage.regulator.max_current",
        "100mA",
        "net `3V3`: b1_r.current (9.257mA..15.81mA) + output.current (0A..90mA)",
        "stage.vout.current (-105.9mA..-9.257mA)",
    ] {
        assert!(m.contains(needle), "`{needle}` is not in: {m}");
    }
}

#[test]
fn parts_alone_can_overload_the_regulator() {
    // No load is stated on the output. Three branches of 47 ohm draw about 17 to 36 mA each,
    // up to 107 mA together, which is more than the regulator gives. (With 100 ohm it took
    // six branches, and every one of the six LEDs that fit has its own copy of each of the
    // other five, which is more combinations than the search will try before giving up.)
    let (_, s) = run_with(
        &with_branches(3, "47ohm +/- 1%", "0mA"),
        &Catalogs::jlcpcb(),
    );
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    for needle in [
        "stage.regulator.max_current",
        "100mA",
        "b1_r.current",
        "b3_r.current",
    ] {
        assert!(
            msgs[0].contains(needle),
            "`{needle}` is not in: {}",
            msgs[0]
        );
    }
    // Two of them are within it.
    let (_, ok) = real(&with_branches(2, "47ohm +/- 1%", "0mA"));
    assert!(ok.diagnostics.is_empty());
}

#[test]
fn candidates_the_design_cannot_tell_apart_are_searched_once() {
    // 600 copies of the same 1k resistor that differ only in their package. Nothing relates
    // the package, so one failed copy stands for all of them. Searched copy by copy, the
    // pair would take 600 x 600 = 360,000 combinations, more than the search will try, and
    // the message would be "gave up" instead of the reason.
    let one_k = resistor("1k", 1000.0);
    let copies: Vec<Candidate> = (0..600)
        .map(|i| {
            let mut c = one_k.clone();
            c.name = format!("1k copy {i}");
            c.columns[1] = ("package".into(), Value::Text(format!("P{i}")));
            c
        })
        .collect();
    // The output cannot be higher than the supply, whatever the resistors are.
    let src = DIVIDER.replace(
        ".out: Rail { .voltage &= 3.3V +/- 5% }",
        ".out: Rail { .voltage &= 6V +/- 1% }",
    );
    let (_, s) = run_with(&src, &catalogs_with(copies));
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("no combination of candidates satisfies `r_bottom` and `r_top` together"),
        "{}",
        msgs[0]
    );
    assert!(!msgs[0].contains("gave up"), "{}", msgs[0]);
}

/// A board with a 5 V supply that is solved with the sample catalogs; `body` is the rest.
fn on_supply(body: &str) -> String {
    format!(
        "import std::parts::passive::{{Resistor, Led, Capacitor}}\nimport std::sources::Rail\n\nboard B {{\n    .supply: Rail {{\n        .voltage &= 5V\n        .pos.net &= \"V\"\n        .neg.net &= \"GND\"\n    }}\n{body}}}\n"
    )
}

#[test]
fn the_supply_gives_what_the_chain_draws_without_the_board_saying_so() {
    let (_, s) = run(&on_supply(
        "    .r: Resistor\n    .led: Led { .current &= 5mA..20mA }\n    .supply.pos ~> .r ~> .led ~> .supply.neg\n",
    ));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "r.current"), (0.005, 0.02)));
    assert!(close(range(&s, "supply.current"), (-0.02, -0.005)));
}

#[test]
fn a_series_chain_that_passes_two_currents_is_refused_with_both() {
    let (_, s) = run(&on_supply(
        "    .r: Resistor { .current &= 1mA..2mA }\n    .led: Led { .current &= 5mA..20mA }\n    .supply.pos ~> .r ~> .led ~> .supply.neg\n",
    ));
    let msgs = messages(&s);
    assert!(
        msgs[0].contains("`r.current` is 1mA..2mA but `led.current` is 5mA..20mA"),
        "{msgs:#?}"
    );
    // The fault is not said again for the net the current comes back on.
    assert!(!msgs.iter().any(|m| m.contains("net `GND`")), "{msgs:#?}");
}

#[test]
fn a_supply_that_cannot_give_what_a_chain_draws_is_reported_once() {
    // The input says it supplies 50 mA at most; the chain draws 100 mA. The fault shows on
    // the supply net and again on the return net, and is said once.
    let (_, s) = run(&on_supply(
        "    .supply.current &= -50mA..0mA\n    .r: Resistor\n    .led: Led { .current &= 100mA }\n    .supply.pos ~> .r ~> .led ~> .supply.neg\n",
    ));
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    for needle in [
        "the currents on net `V` do not add up",
        "`r.current` is 100mA",
        "`supply.current` is -50mA..0A",
    ] {
        assert!(
            msgs[0].contains(needle),
            "`{needle}` is not in: {}",
            msgs[0]
        );
    }
}

#[test]
fn a_capacitor_in_a_chain_adds_nothing_to_the_sum() {
    // The rail's own current is all there is on the net: nothing is drawn by the capacitor,
    // so a stated load is all the supply gives.
    let (_, s) = run(&on_supply(
        "    .load: Rail { .current &= 10mA..20mA }\n    .bypass: Capacitor\n    .supply.pos ~ .load.pos\n    .supply.pos ~> .bypass ~> .supply.neg\n",
    ));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "supply.current"), (-0.02, -0.01)));
}

#[test]
fn a_part_with_a_rail_of_its_own_adds_what_it_draws() {
    let src = "\
import std::sources::Rail

part Mcu {
    .vdd: Rail
}

board B {
    .supply: Rail {
        .pos.net &= \"V\"
        .neg.net &= \"GND\"
    }
    .mcu: Mcu { .vdd.current &= 10mA..30mA }
    .supply ~ .mcu.vdd
}
";
    let (_, s) = run(src);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "supply.current"), (-0.03, -0.01)));
}

#[test]
fn what_a_tapped_chain_gives_back_is_not_checked_but_what_it_draws_is_counted() {
    // r1 and r2 form a divider with a load on the middle, so their currents differ. The
    // supply is charged with r1's, and nothing is claimed about the return.
    let (_, s) = run(&on_supply(
        "    .r1: Resistor { .current &= 2mA..3mA }\n    .r2: Resistor { .current &= 1mA..2mA }\n    .load: Resistor\n    .supply.pos ~> .r1 ~> .r2 ~> .supply.neg\n    .r1.b ~ .load.a\n    .load.b ~ .supply.neg\n",
    ));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "supply.current"), (-0.003, -0.002)));
}

// ---- resistor power -------------------------------------------------------------

/// A resistor on its own, with the current through it stated.
fn alone(extra: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\n\nboard Alone {{\n    .r: Resistor {{\n{extra}    }}\n}}\n"
    )
}

#[test]
fn a_10_ohm_resistor_carrying_1_a_fails_every_part_the_catalog_rates_at_250_mw_or_less() {
    let src = alone("        .resistance &= 10ohm +/- 1%\n        .current &= 1A\n");
    let (_, s) = run_with(&src, &Catalogs::jlcpcb());
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    for needle in [
        "no `resistor` candidate satisfies `r`",
        "10Ω 1% 0805 C317221",
        "r.drop * r.current",
        "must always be at most r.max_power",
        "9.9W..10.1W is not always below 250mW",
    ] {
        assert!(
            msgs[0].contains(needle),
            "`{needle}` is not in: {}",
            msgs[0]
        );
    }
    assert!(s.picks.is_empty());
}

#[test]
fn a_resistor_within_its_rating_is_picked_and_one_over_it_is_skipped_for_the_next() {
    // Up to 49 mA through up to 101 ohm is 243 mW: over the 0402 part's 62.5 mW and the 0603
    // part's 100 mW, so the 250 mW 0805 part is the first that holds.
    let src = alone("        .resistance &= 100ohm +/- 1%\n        .current &= 45mA..49mA\n");
    let (_, s) = run_with(&src, &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.model.domain_at("r.package"), Some(&text("0805")));
    assert!(close(range(&s, "r.max_power"), (0.25, 0.25)));
    assert!(s.picks[0].rejected > 0);
}

fn text(t: &str) -> Domain {
    Domain::Text(t.to_string())
}

#[test]
fn a_resistor_whose_current_nothing_states_has_no_power_to_check() {
    let src = alone("        .resistance &= 10ohm +/- 1%\n");
    let (_, s) = run_with(&src, &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(
        s.deferred.iter().any(|&i| {
            s.model
                .show_expr(&s.model.relations()[i].lhs)
                .contains("r.drop")
        }),
        "the rating is listed as not evaluated"
    );
}

#[test]
fn a_row_with_no_rating_is_never_offered_and_the_message_counts_them() {
    let mut c = Catalogs::empty();
    c.add_file(
        "resistor",
        "catalogs/resistor.csv",
        "mpn,resistance,max_power,package,source\n\
         ZZ-1,316ohm +/- 1%,,0402,https://example.invalid/zz\n",
    )
    .unwrap();
    let src = alone("        .resistance &= 316ohm +/- 1%\n");
    let (_, s) = run_with(&src, &c);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("the `resistor` catalog has nothing for `r`")
            && msgs[0].contains("rows with an empty cell are never offered")
            && msgs[0].contains("1 for `max_power`"),
        "{}",
        msgs[0]
    );
}

// ---- a balance solves rails and verifies parts (ADR 0026) -------------------------------

/// The regulator example with a second load on the output rail and a divider of two
/// resistors from the rail to ground whose middle goes to a header pin (so the chain is
/// tapped, and nothing says what current the divider passes). The stage is said to supply
/// `supply` at most, which is what makes the regulator's supplied current known before the
/// divider's.
fn with_divider(supply: &str, r_top: &str) -> String {
    with_load("0mA..60mA")
        .replace(
            "    .input ~ .stage.vin\n",
            &format!(
                "    .stage.vout.current &= {supply}
    .other: Rail {{
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
        .current &= 0mA..20mA
    }}
    .output.pos ~ .other.pos
    .output.neg ~ .other.neg
    .r_top: Resistor {{ .resistance &= {r_top} }}
    .r_bottom: Resistor {{ .resistance &= 10kohm +/- 1% }}
    .sense: Header1x2
    .output.pos ~> .r_top ~> .r_bottom ~> .output.neg
    .r_top.b ~ .sense.p1
    .output.neg ~ .sense.p2
    .input ~ .stage.vin\n"
            ),
        )
        .replace(
            "import std::parts::passive::Capacitor",
            "import std::parts::passive::{Capacitor, Resistor}",
        )
}

#[test]
fn a_divider_whose_current_nothing_determines_is_not_evaluated_and_does_not_fail() {
    // The regulator supplies 0 to 100 mA and the loads take 0 to 80 mA, so what is left for
    // the head of the divider is -80 mA to 100 mA by subtraction. At 10 kohm that is up to 100
    // W, which is no property of the resistor: nothing says what it carries.
    let (_, s) = real(&with_divider("-100mA..0mA", "10kohm +/- 1%"));
    assert!(
        s.model
            .domain_at("r_top.current")
            .is_none_or(|d| *d == Domain::Unset)
    );
    let shown: Vec<String> = s
        .deferred
        .iter()
        .map(|&i| s.model.show_expr(&s.model.relations()[i].lhs))
        .collect();
    // The sense header is on the divider's middle, where a connector can put any voltage, so
    // Ohm's law has no range there and gives no bound: the resistors still wait (the Ohm bound
    // is pass-only and needs a range at both ends).
    for resistor in ["r_top", "r_bottom"] {
        assert!(
            shown
                .iter()
                .any(|e| e.contains(&format!("{resistor}.drop"))),
            "{resistor}'s power is listed as not evaluated: {shown:?}"
        );
    }
    // The rails are still solved: what the regulator supplies is what the input gives.
    assert!(close(range(&s, "input.current"), (-0.1, 0.0)));
}

#[test]
fn a_resistor_whose_current_is_determined_still_fails_its_power_with_the_numbers() {
    // 10 ohm across a 5 V rail passes 0.5 A and dissipates about 2.5 W; no catalog row rated
    // at 250 mW or less will do, and the message names the numbers.
    let src = on_supply(
        "    .r: Resistor { .resistance &= 10ohm +/- 1% }\n    .r.current == .supply.voltage / .r.resistance\n    .supply.pos ~> .r ~> .supply.neg\n",
    );
    let (_, s) = run_with(&src, &Catalogs::jlcpcb());
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    for needle in [
        "no `resistor` candidate satisfies `r`",
        "r.drop * r.current",
        "must always be at most r.max_power",
        "is not always below 250mW",
    ] {
        assert!(
            msgs[0].contains(needle),
            "`{needle}` is not in: {}",
            msgs[0]
        );
    }
}

// ---- a balance is complete or it is not used for parts (ADR 0027) -------------------------

/// A step-down stage in outline: an inductor from the input to the output joined with `~`
/// (so what it carries is not modelled), and a 100 kohm divider from the output to ground
/// in a chain. The output net of the stage has the divider's first resistor, the stage's
/// output port and the inductor's pin.
const OUTLINE_STAGE: &str = r#"import std::parts::passive::{Inductor, Resistor}
import std::sources::Rail

module Stage {
    .vin: Rail
    .vout: Rail
    .l: Inductor { .inductance &= 2.2uH +/- 20% }
    .r_top: Resistor { .resistance &= 100kohm +/- 1% }
    .r_bottom: Resistor { .resistance &= 100kohm +/- 1% }

    .vin.pos ~ .l.a
    .l.b ~ .vout.pos
    .vin.neg ~ .vout.neg
    .vout.pos ~> .r_top ~> .r_bottom ~> .vout.neg
}

board Supply {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .stage: Stage { .vout.voltage &= 3.3V +/- 3% }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0A..2A
    }
    .input ~ .stage.vin
    .stage.vout ~ .output
}
"#;

#[test]
fn a_part_current_is_not_set_from_a_balance_with_an_unmodelled_pin() {
    // The balance on the stage's output has two terms (the divider's first resistor and the
    // port, 0 to 2 A) and the inductor's pin, which is nobody's term. Setting the resistor to
    // the port's whole current gave 2 A through 100 kohm and a power of 404 kW.
    let (_, s) = real(OUTLINE_STAGE);
    assert!(
        s.model
            .domain_at("stage.r_top.current")
            .is_none_or(|d| *d == Domain::Unset)
    );
    let shown: Vec<String> = s
        .deferred
        .iter()
        .map(|&i| s.model.show_expr(&s.model.relations()[i].lhs))
        .collect();
    // Ohm's law bounds each resistor of the divider (3.399 V over 99 kohm) and the bound proves
    // their power checks, so they no longer wait (the Ohm bound, pass-only). The inductor, which
    // Ohm's law does not bound, still waits.
    for resistor in ["r_top", "r_bottom"] {
        assert!(
            !shown
                .iter()
                .any(|e| e.contains(&format!("stage.{resistor}."))),
            "{resistor}'s power is proven by the bound: {shown:?}"
        );
    }
    assert!(
        shown.iter().any(|e| e.contains("stage.l.drop")),
        "the inductor's relations wait: {shown:?}"
    );
    // The port's current is a rail's and is solved as before.
    assert!(close(range(&s, "stage.vout.current"), (-2.0, 0.0)));
    // The balance says which pin is the reason.
    let b = s
        .model
        .balances()
        .iter()
        .find(|b| b.net == "3V3" && s.model.path_of(b.scope) == "stage")
        .expect("the stage's output balance");
    let pins: Vec<String> = b.unmodelled.iter().map(|&p| s.model.path_of(p)).collect();
    assert_eq!(pins, ["stage.l.b"]);
}

#[test]
fn an_incomplete_balance_does_not_report_a_part_as_not_adding_up() {
    // The reverse: the divider passes 16.5 uA by Ohm's law and the board takes nothing from
    // the output. By the two terms alone the net does not add up, but the inductor's pin is
    // on it and supplies the divider, so there is nothing to contradict.
    let src = OUTLINE_STAGE
        .replace(".current &= 0A..2A", ".current &= 0A")
        .replace(
            "    .vin.pos ~ .l.a\n",
            "    .r_top.current == .vout.voltage / (.r_top.resistance + .r_bottom.resistance)\n    .vin.pos ~ .l.a\n",
        );
    let (_, s) = real(&src);
    assert!(close(range(&s, "stage.vout.current"), (0.0, 0.0)));
    let amps = range(&s, "stage.r_top.current");
    assert!(amps.0 > 1.5e-5 && amps.1 < 1.8e-5, "{amps:?}");
}

#[test]
fn rails_alone_are_still_verified_on_a_net_with_an_unmodelled_pin() {
    // Two rails that cannot add up are an error whatever else is on the net.
    let (_, s) = run(&on_supply(
        "    .load: Rail { .pos.net &= \"V\"\n .neg.net &= \"GND\"\n .current &= 50mA..60mA }\n    \
         .supply.current &= -10mA..0mA\n    \
         .supply.pos ~ .load.pos\n    .supply.neg ~ .load.neg\n    \
         .loose: Resistor\n    .supply.pos ~ .loose.a\n    .supply.neg ~ .loose.b\n",
    ));
    let msgs = messages(&s);
    assert!(
        msgs.iter().any(|m| m.contains("do not add up")
            && m.contains("50mA..60mA")
            && m.contains("-10mA..0A")),
        "{msgs:#?}"
    );
}

/// A board with a resistor feeding an IC invented for these tests: its supply is a rail
/// that states what it draws, and its enable pin is tied to the same net. `extra` goes
/// in the board.
fn enable_on_the_chain(draw: &str, extra: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\nimport std::sources::Rail\n\n\
         part Ic {{\n    .vdd: Rail\n    pins {{\n        .vdd.pos   power  \"VDD\" \"1\"\n        \
         .vdd.neg   power  \"GND\" \"2\"\n        .en        input  \"EN\"  \"3\" optional\n    }}\n}}\n\n\
         part NoRail {{\n    pins {{\n        .en  input  \"EN\"  \"1\" optional\n    }}\n}}\n\n\
         board B {{\n    .supply: Rail {{\n        .voltage &= 5V\n        .pos.net &= \"V\"\n        \
         .neg.net &= \"GND\"\n    }}\n    .r: Resistor {{ .resistance &= 100ohm +/- 1% }}\n    \
         .ic: Ic {{ .vdd.current &= {draw} }}\n    .supply.pos ~> .r ~> .ic.vdd.pos\n    \
         .ic.vdd.neg ~ .supply.neg\n    .ic.en ~ .ic.vdd.pos\n{extra}}}\n"
    )
}

#[test]
fn an_enable_pin_on_the_chains_net_does_not_hide_the_chain_current() {
    // The enable pin leaks and is ignored (ADR 0027); the IC's supply is its rail. The net
    // after the resistor is complete, the resistor carries what the IC draws, and the supply
    // is worked out from it.
    let (c, s) = run_with(&enable_on_the_chain("10mA..20mA", ""), &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(c.boards[0].model.balances().iter().all(|b| b.is_complete()));
    assert!(close(range(&s, "r.current"), (0.01, 0.02)));
    assert!(close(range(&s, "supply.current"), (-0.02, -0.01)));
}

#[test]
fn a_power_failure_after_such_a_chain_is_caught_with_numbers() {
    // 0.4 to 0.5 A through 100 ohm is 16 to 25 W: the resistor's power fails, which was
    // "not evaluated" while the enable pin kept the net incomplete.
    let (_, s) = run_with(
        &enable_on_the_chain("400mA..500mA", ""),
        &Catalogs::jlcpcb(),
    );
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("r.drop * r.current")
            && msgs[0].contains("must always be at most r.max_power")
            && msgs[0].contains("is not always below"),
        "{}",
        msgs[0]
    );
}

#[test]
fn an_input_pin_of_a_part_with_no_rail_still_keeps_the_chain_current_unset() {
    let (c, s) = run_with(
        &enable_on_the_chain(
            "10mA..20mA",
            "    .other: NoRail\n    .other.en ~ .ic.vdd.pos\n",
        ),
        &Catalogs::jlcpcb(),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    let incomplete: Vec<String> = c.boards[0]
        .model
        .balances()
        .iter()
        .filter(|b| !b.is_complete())
        .flat_map(|b| b.unmodelled.iter().map(|&p| s.model.path_of(p)))
        .collect();
    assert_eq!(incomplete, ["other.en"]);
    assert!(
        s.model
            .domain_at("r.current")
            .is_none_or(|d| *d == Domain::Unset)
    );
}

#[test]
fn a_complete_net_still_sets_a_part_from_two_terms_and_a_determined_overload_fails() {
    let board = |rated: &str| {
        format!(
            "import std::parts::passive::Fuse\nimport std::sources::Rail\n\nboard B {{\n    .supply: Rail {{\n        .voltage &= 12V\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n    .load: Rail {{\n        .current &= 100mA..600mA\n        .pos.net &= \"VF\"\n        .neg.net &= \"GND\"\n    }}\n    .fuse: Fuse {{ .rated_current &= {rated} }}\n    .supply.pos ~> .fuse ~> .load.pos\n    .supply.neg ~ .load.neg\n}}\n"
        )
    };
    let (c, s) = run_with(&board("1A"), &Catalogs::sample());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(
        c.boards[0].model.balances().iter().all(|b| b.is_complete()),
        "every pin on the fuse board is a term or draws nothing"
    );
    assert!(close(range(&s, "fuse.current"), (0.1, 0.6)));
    let (_, s) = run_with(&board("500mA"), &Catalogs::sample());
    let msgs = messages(&s);
    assert!(
        msgs.iter().any(|m| m.contains("fuse.current")
            && m.contains("100mA..600mA")
            && m.contains("500mA")),
        "{msgs:#?}"
    );
}

// ---- an equation narrows a field that has a range (ADR 0025) ----------------------

/// The LED example on a 12 V supply, the resistor held to `package`.
fn led_on_12v(package: &str) -> String {
    LED_EXAMPLE.replace("5V +/- 5%", "12V +/- 5%").replace(
        ".r1: Resistor {}",
        &format!(".r1: Resistor {{ .package &= \"{package}\" }}"),
    )
}

#[test]
fn the_leds_stated_window_becomes_the_current_the_resistor_lets_through() {
    let (_, s) = real(LED_EXAMPLE);
    // 330 ohm +/- 1%, 4.75 V to 5.25 V, a 1.8 V to 2.4 V LED: 7.05 to 10.56 mA, inside the
    // stated 5 to 15 mA, and that is what every field that follows the current now holds.
    let current = range(&s, "led.current");
    assert!(
        (current.0 - 0.00705).abs() < 1e-5 && (current.1 - 0.01056).abs() < 1e-5,
        "{current:?}"
    );
    assert!(close(range(&s, "r1.current"), current));
    assert!(close(range(&s, "supply.current"), (-current.1, -current.0)));
    // The 0402 part's 62.5 mW holds: the power is about 36 mW, not the 88 mW that the top of
    // the window would give.
    let drop = range(&s, "r1.drop");
    let power = drop.1 * current.1;
    assert!(power > 0.035 && power < 0.038, "{power}");
    assert_eq!(picked(&s)[1].1, "330");
    assert_eq!(s.picks[1].lcsc.as_deref(), Some("C25104"));
}

#[test]
fn a_candidate_that_failed_the_power_check_at_the_top_of_the_window_passes_at_its_own_current() {
    // Without the narrowing the 0402 330 ohm part was checked at 15 mA x 5.9 V = 88 mW against
    // its 62.5 mW and refused; at the 7 to 10.6 mA it carries it is about 36 mW, so it is
    // the first candidate that holds, with nothing rejected before it. A design may still ask
    // for the larger part.
    let with_package = |package: &str| {
        LED_EXAMPLE.replace(
            ".r1: Resistor {}",
            &format!(".r1: Resistor {{ .package &= \"{package}\" }}"),
        )
    };
    let (_, s) = real(&with_package("0603"));
    assert_eq!(picked(&s)[1].1, "390", "a 0603 part when it is asked for");
    let (_, s) = real(&with_package("0402"));
    assert_eq!(picked(&s)[1].1, "330");
    assert_eq!(s.picks[1].rejected, 0, "{:?}", s.picks[1]);
}

#[test]
fn a_candidate_whose_own_current_is_still_too_much_fails_with_the_numbers() {
    // 12 V +/- 5% through an LED: every 0402 value that keeps the current in 5 to 15 mA
    // still dissipates more than 62.5 mW at the current it carries.
    let (_, s) = run_with(&led_on_12v("0402"), &Catalogs::jlcpcb());
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    for needle in [
        "no combination of candidates satisfies `led` and `r1` together",
        "r1.max_power",
        "is not always below 62.5mW",
    ] {
        assert!(
            msgs[0].contains(needle),
            "`{needle}` is not in: {}",
            msgs[0]
        );
    }
    // The same board takes the 0603 part (100 mW) when the package is free.
    let (_, s) = real(&LED_EXAMPLE.replace("5V +/- 5%", "12V +/- 5%"));
    assert!(
        close(range(&s, "r1.max_power"), (0.1, 0.1)),
        "100 mW or 250 mW"
    );
}

#[test]
fn an_equation_that_does_not_fit_the_window_is_reported_against_the_window_as_written() {
    // 330 ohm gives 7.05 to 10.56 mA: partly outside 5 to 8 mA, and wholly outside 20 to 30 mA.
    let fixed = |window: &str| {
        LED_EXAMPLE
            .replace(
                "        .current &= 5mA..15mA",
                &format!("        .forward_voltage &= 1.8V..2.4V\n        .current &= {window}"),
            )
            .replace(
                ".r1: Resistor {}",
                ".r1: Resistor { .resistance &= 330ohm +/- 1% }",
            )
    };
    for (window, needle) in [("5mA..8mA", "5mA..8mA"), ("20mA..30mA", "20mA..30mA")] {
        let (_, s) = run_with(&fixed(window), &Catalogs::jlcpcb());
        let msgs = messages(&s);
        assert!(
            msgs.iter().any(
                |m| m.contains(&format!("`led.current` must stay within {needle}"))
                    && m.contains("works out to 7.05")
            ),
            "{window}: {msgs:#?}"
        );
    }
}

/// Two rails and an equation or two between their voltages.
fn rails(equations: &str) -> String {
    format!(
        "import std::sources::Rail\n\nboard B {{\n    .a: Rail {{ .voltage &= 3V..4V }}\n    .b: Rail {{ .voltage &= 3.4V..3.6V }}\n    .c: Rail {{ .voltage &= 3.2V..3.8V }}\n{equations}}}\n"
    )
}

#[test]
fn only_the_one_equation_on_a_field_narrows_it() {
    let (_, s) = run(&rails("    .a.voltage == .b.voltage\n"));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "a.voltage"), (3.4, 3.6)));
    // Two equations are two requirements on the range as written: neither narrows it, so
    // each is checked against 3 V to 4 V.
    let (_, s) = run(&rails(
        "    .a.voltage == .b.voltage\n    .a.voltage == .c.voltage\n",
    ));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "a.voltage"), (3.0, 4.0)));
}

#[test]
fn an_equation_on_a_field_with_a_range_is_still_a_contradiction_when_nothing_is_left() {
    let (_, s) = run(&rails("    .a.voltage == .b.voltage + 1V\n"));
    let msgs = messages(&s);
    assert!(
        msgs.iter()
            .any(|m| m.contains("`a.voltage` must stay within 3V..4V") && m.contains("4.4V..4.6V")),
        "{msgs:#?}"
    );
}

// ---- a stated current is a budget (ADR 0038) -------------------------------------------

/// A sensor board invented for these tests: a monitor module whose supply port feeds a
/// resistor divider of two `ohms` resistors. The divider's current follows from its own
/// equation; the board states what the monitor may draw as a budget.
fn monitor_board(budget: &str, ohms: &str) -> String {
    format!(
        r#"import std::parts::passive::Resistor
import std::sources::Rail

module Monitor {{
    .vin: Rail
    .r_top: Resistor {{ .resistance &= {ohms} +/- 1% }}
    .r_bottom: Resistor {{ .resistance &= {ohms} +/- 1% }}
    .r_top.current == .vin.voltage / (.r_top.resistance + .r_bottom.resistance)
    .vin.pos ~> .r_top ~> .r_bottom ~> .vin.neg
}}

board Sensor {{
    .supply: Rail {{
        .voltage &= 3.3V +/- 3%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }}
    .monitor: Monitor {{ .vin.current &= {budget} }}
    .supply ~ .monitor.vin
}}
"#
    )
}

#[test]
fn a_budget_wider_than_what_the_circuit_draws_is_met_and_narrowed_to_it() {
    // Two 100 kohm resistors on 3.2 V to 3.4 V draw 15.84 uA to 17.17 uA, inside the 20 uA
    // the monitor may draw. This was refused: the stated range had to be enclosed by what
    // the divider alone could give.
    let (_, s) = run(&monitor_board("0A..20uA", "100kohm"));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    let drawn = range(&s, "monitor.r_top.current");
    assert!(drawn.0 > 1.58e-5 && drawn.1 < 1.72e-5, "{drawn:?}");
    // The port holds what the circuit carries, not the allowance, and the supply follows it.
    assert!(close(range(&s, "monitor.vin.current"), drawn));
    assert!(close(range(&s, "supply.current"), (-drawn.1, -drawn.0)));
}

#[test]
fn the_range_the_build_prints_is_accepted_when_it_is_stated() {
    let (_, s) = run(&monitor_board("15.84uA..17.17uA", "100kohm"));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
}

#[test]
fn a_circuit_that_draws_more_than_its_budget_is_refused_with_both_ranges() {
    let (_, s) = run(&monitor_board("0A..10uA", "100kohm"));
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    for needle in [
        "the currents on net `3V3` do not add up",
        "`-monitor.vin.current` must stay within -10uA..0A",
        "the others on it work out to -17.17uA..-15.84uA",
        "`monitor.r_top.current` is 15.84uA..17.17uA",
    ] {
        assert!(
            msgs[0].contains(needle),
            "`{needle}` is not in: {}",
            msgs[0]
        );
    }
    // The stated range is not narrowed by what does not fit it.
    assert!(close(range(&s, "monitor.vin.current"), (0.0, 1e-5)));
}

#[test]
fn a_budget_a_determined_chain_exceeds_fails_whatever_the_overlap() {
    // Two 20 ohm resistors on 3.2 V to 3.4 V draw 79.2 mA to 85.8 mA, which a
    // budget of 50 mA does not hold, and one of 90 mA does.
    let (_, s) = run(&monitor_board("0mA..50mA", "20ohm"));
    let msgs = messages(&s);
    assert!(
        msgs.iter().any(
            |m| m.contains("`-monitor.vin.current` must stay within -50mA..0A")
                && m.contains("work out to -85.84mA..-79.23mA")
        ),
        "{msgs:#?}"
    );
    let (_, ok) = run(&monitor_board("0mA..90mA", "20ohm"));
    assert!(ok.diagnostics.is_empty(), "{:?}", messages(&ok));
}

#[test]
fn a_supply_may_state_more_than_the_load_takes() {
    // Both are stated: the supply's range is what it may give, the load's what it may
    // take, and they only have to be able to reach zero together.
    let two = |supply: &str, load: &str| {
        format!(
            "import std::sources::Rail\n\nboard B {{\n    .supply: Rail {{ .current &= {supply} }}\n    .load: Rail {{ .current &= {load} }}\n    .supply.pos ~ .load.pos\n}}\n"
        )
    };
    let (_, s) = run(&two("-100mA..0mA", "0mA..50mA"));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    // A supply that can give 5 to 10 mA against a load of 20 to 30 mA cannot.
    let (_, s) = run(&two("-10mA..-5mA", "20mA..30mA"));
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("`load.current` is 20mA..30mA")
            && msgs[0].contains("only give -10mA..-5mA"),
        "{}",
        msgs[0]
    );
}

#[test]
fn a_derived_current_is_worked_out_again_from_every_net_it_is_on() {
    // The stage's input is on two nets: the board's (the input supplies 5 to 15 mA) and
    // the regulator's (it draws what the 10 to 20 mA load takes). The two ranges overlap
    // without either lying inside the other, which left the first one worked out in place
    // and the build refused. The current is what both say.
    let src = REGULATOR
        .replace(".current &= 0mA..50mA", ".current &= 10mA..20mA")
        .replace(
            ".voltage &= 5V +/- 5%\n        .pos.net &= \"VIN\"",
            ".voltage &= 5V +/- 5%\n        .current &= -15mA..-5mA\n        .pos.net &= \"VIN\"",
        );
    assert_ne!(src, REGULATOR);
    let (_, s) = run(&src);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "stage.vin.current"), (0.01, 0.015)));
    // The regulator's equation (its input draws what its output supplies, 10 to 20 mA) and
    // the net it is on (5 to 15 mA) both hold.
    assert!(close(
        range(&s, "stage.regulator.vin.current"),
        (0.01, 0.015)
    ));
    // An input that gives 1 to 4 mA shares nothing with a load of 10 to 20 mA.
    let (_, s) = run(&src.replace("-15mA..-5mA", "-4mA..-1mA"));
    assert!(!s.diagnostics.is_empty());
}
