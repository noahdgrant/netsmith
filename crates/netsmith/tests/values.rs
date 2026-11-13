//! A pick that nothing limits and a value that nothing states are findings, not silence (ADR
//! 0065): a warning by default, an error with `--strict`. Every value here is invented.

use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_solve::Catalogs;

fn build(src: &str, catalogs: &Catalogs) -> Result<Vec<netsmith::Built>, String> {
    build_with(
        "values.nsm",
        src,
        &Loader::std(),
        catalogs,
        &Libraries::sample(),
    )
}

fn warnings(src: &str) -> Vec<String> {
    build(src, &Catalogs::jlcpcb())
        .unwrap_or_else(|e| panic!("{e}"))
        .iter()
        .flat_map(|b| b.guideline_warnings())
        .collect()
}

const PLAIN: &str = r#"import std::parts::passive::Resistor
import std::sources::Rail

module Lamp {
    .supply: Rail
    .balance_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .top.resistance &= .balance_resistance
    .bottom.resistance &= .balance_resistance
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
}

board Plain {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .lamp: Lamp
    .supply ~ .lamp.supply
    .pull: Resistor
    .supply.pos ~> .pull ~> .supply.neg
}
"#;

#[test]
fn a_board_that_states_nothing_gets_a_finding_for_each_cause() {
    let w = warnings(PLAIN);
    assert_eq!(
        w,
        [
            "Plain: unconstrained pick guideline: `lamp.top` and `lamp.bottom` (Resistor) were chosen with nothing limiting their resistance, so the solver took the first row that fits, 330Ω 1% 0402 C25104 for each; both are held to `lamp.balance_resistance` (`&=` at values.nsm:9 and 10), which nothing states: state it, `.lamp.balance_resistance &= 4.7kohm +/- 5%`, or say that any value will do, `.lamp.balance_resistance &= 10kohm..100kohm`",
            "Plain: unconstrained pick guideline: `pull` (Resistor) was chosen with nothing limiting its resistance, so the solver took the first row that fits, 330Ω 1% 0402 C25104: state it, `.pull.resistance &= 4.7kohm +/- 5%`, or bound a value that it decides (a current or a voltage the design checks), or say that any value will do, `.pull.resistance &= 10kohm..100kohm`",
        ]
    );
}

#[test]
fn what_the_message_says_to_write_ends_the_finding() {
    let src = PLAIN
        .replace(
            "    .pull: Resistor\n",
            "    .pull: Resistor { .resistance &= 4.7kohm +/- 5% }\n    .lamp.balance_resistance &= 10kohm..100kohm\n",
        );
    assert_eq!(warnings(&src), Vec::<String>::new());
    // And so does a bound that the pick honours.
    let bounded = PLAIN
        .replace(
            "    .pull: Resistor\n",
            "    .pull: Resistor { .resistance &= 4.7kohm +/- 5% }\n    .lamp.balance_resistance >= 20kohm\n",
        );
    assert_eq!(warnings(&bounded), Vec::<String>::new());
}

#[test]
fn strict_makes_each_an_error_at_the_part() {
    let strict = Catalogs::jlcpcb().with_strict_guidelines(true);
    let Err(e) = build(PLAIN, &strict) else {
        panic!("a strict build must stop at an unconstrained pick");
    };
    assert_eq!(
        e.matches("error: Plain: unconstrained pick guideline:")
            .count(),
        2,
        "{e}"
    );
    assert!(e.contains("--> values.nsm:18:5"), "the part's line: {e}");
    assert!(
        e.contains("--> values.nsm:7:5"),
        "the first part of the module: {e}"
    );
}

/// Two boards that each state nothing for their resistor, in a system.
const SYSTEM: &str = r#"import std::parts::passive::Resistor
import std::sources::Rail

interface Setting {
    .value: Resistance
}

board A {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .setting: Setting
    .r: Resistor
    .r.resistance &= .setting.value
    .supply.pos ~> .r ~> .supply.neg
}

board B {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .setting: Setting
    .other: Resistor
    .supply.pos ~> .other ~> .supply.neg
}

system Both {
    .a: A
    .b: B
    .a.setting ~ .b.setting
}
"#;

#[test]
fn a_board_of_a_system_is_judged_on_its_own_parts() {
    let boards = build(SYSTEM, &Catalogs::jlcpcb()).unwrap();
    let all: Vec<String> = boards.iter().flat_map(|b| b.guideline_warnings()).collect();
    // `a`'s resistor is held to a port that nothing states; `b`'s is alone.
    assert_eq!(all.len(), 2, "{all:#?}");
    assert!(
        all[0].starts_with("Both/a: unconstrained pick guideline: `r` (Resistor) was chosen with nothing limiting its resistance"),
        "{}",
        all[0]
    );
    assert!(
        all[1].starts_with("Both/b: unconstrained pick guideline: `other` (Resistor) was chosen"),
        "{}",
        all[1]
    );
}

#[test]
fn a_value_that_the_other_board_states_through_a_joined_port_limits_the_pick() {
    // The system names nothing but joins: the value is stated in board `B`, and the join carries
    // it into `A`.
    let stated = SYSTEM.replace(
        "    .other: Resistor\n",
        "    .other: Resistor { .resistance &= 1kohm +/- 5% }\n    .setting.value &= 4.7kohm +/- 5%\n",
    );
    let boards = build(&stated, &Catalogs::jlcpcb()).unwrap();
    let all: Vec<String> = boards.iter().flat_map(|b| b.guideline_warnings()).collect();
    assert_eq!(all, Vec::<String>::new());
}

const RECEIVER: &str = r#"module Receiver {
    .line: Pin
    .vih: Voltage
    .vil: Voltage
    .driver_high: Voltage
    .driver_high >= .vih
    .driver_high <= 5.5V
    .vil &= 0.3V
}

board Probe {
    .receiver: Receiver
}
"#;

#[test]
fn values_that_checks_wait_for_are_a_finding_that_names_the_checks() {
    assert_eq!(
        warnings(RECEIVER),
        [
            "Probe: unstated value guideline: `receiver.vih` and `receiver.driver_high` are declared by `Receiver` and nothing states or works them out, so the checks `receiver.driver_high >= receiver.vih` and `receiver.driver_high <= 5.5V` are not evaluated; state them, for example `.receiver.vih &= 3.3V +/- 5%`"
        ]
    );
    let stated = RECEIVER.replace(
        "    .receiver: Receiver\n",
        "    .receiver: Receiver { .vih &= 2V; .driver_high &= 3.3V +/- 5% }\n",
    );
    assert_eq!(warnings(&stated), Vec::<String>::new());
}

#[test]
fn a_strict_build_refuses_an_unstated_value_at_the_module() {
    let strict = Catalogs::jlcpcb().with_strict_guidelines(true);
    let Err(e) = build(RECEIVER, &strict) else {
        panic!("a strict build must stop at an unstated value");
    };
    assert!(e.contains("error: Probe: unstated value guideline:"), "{e}");
    assert!(e.contains("--> values.nsm:"), "{e}");
}

#[test]
fn the_examples_state_what_they_need_and_get_neither_finding() {
    for (name, src) in [
        ("led", include_str!("../../../examples/led.nsm")),
        (
            "linear_regulator",
            include_str!("../../../examples/linear_regulator.nsm"),
        ),
        (
            "status_indicator",
            include_str!("../../../examples/status_indicator.nsm"),
        ),
        (
            "four_layer",
            include_str!("../../../examples/four_layer.nsm"),
        ),
        ("six_layer", include_str!("../../../examples/six_layer.nsm")),
        ("ten_layer", include_str!("../../../examples/ten_layer.nsm")),
        (
            "two_boards",
            include_str!("../../../examples/two_boards.nsm"),
        ),
    ] {
        let boards = build_with(
            &format!("{name}.nsm"),
            src,
            &Loader::std(),
            &Catalogs::jlcpcb(),
            &Libraries::sample(),
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        for b in &boards {
            let found: Vec<String> = b
                .guideline_warnings()
                .into_iter()
                .filter(|w| w.contains("unconstrained pick") || w.contains("unstated value"))
                .collect();
            assert!(found.is_empty(), "{name}: {found:#?}");
        }
    }
}

const DIVIDER: &str = r#"import std::parts::passive::Resistor
import std::sources::Rail

module Divider {
    .supply: Rail
    .total_resistance: Resistance
    .top: Resistor
    .bottom: Resistor
    .total_resistance == .top.resistance + .bottom.resistance
    .supply.pos ~> .top ~> .bottom ~> .supply.neg
    .out: Voltage
    .out == .supply.voltage * .bottom.resistance / .total_resistance
    .out >= 1.5V
    .out <= 1.8V
}

board RatioOnly {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .divider: Divider
    .divider.supply ~ .supply
}
"#;

#[test]
fn a_divider_whose_size_nobody_states_is_one_finding_that_says_the_ratio_is_limited() {
    assert_eq!(
        warnings(DIVIDER),
        [
            "RatioOnly: unconstrained pick guideline: `divider.top` and `divider.bottom` (Resistor) were chosen with nothing limiting their resistance in size, so the solver took the first row that fits, 330Ω 1% 0402 C25104 for each; their resistance is limited as a ratio, not in size, by `.out >= 1.5V` and `.out <= 1.8V` (values.nsm:13 and 14), which hold whatever the size is: state one of them, `.divider.top.resistance &= 4.7kohm +/- 5%`, or bound a value that changes with the size, not only with the ratio (a current or a voltage the design checks), or say that any value will do, `.divider.top.resistance &= 10kohm..100kohm`",
        ]
    );
    // Stating the total ends it.
    let stated = DIVIDER.replace(
        "    .divider.supply ~ .supply\n",
        "    .divider.supply ~ .supply\n    .divider.total_resistance &= 10kohm..200kohm\n",
    );
    assert_eq!(warnings(&stated), Vec::<String>::new());
}
