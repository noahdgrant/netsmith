//! A circle of values that stays open, with a check inside it that cannot be made, is a
//! finding and not only a note (ADR 0057). Every value here is invented.

use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_solve::Catalogs;

fn build(src: &str, catalogs: &Catalogs) -> Result<Vec<netsmith::Built>, String> {
    build_with(
        "circle.nsm",
        src,
        &Loader::std(),
        catalogs,
        &Libraries::sample(),
    )
}

/// A constant-power load behind a series resistor, with the resistor's own rating check
/// (`drop * current <= max_power`), and `extra` in the board.
fn rated(extra: &str) -> String {
    format!(
        r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

board Circle {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 9V..16V
        .pos.net &= "VBATT"
        .neg.net &= "GND"
    }}
    .r1: Resistor {{ .resistance &= 10ohm +/- 1% }}
    .load: Rail {{
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
    }}
    .supply.neg ~ .load.neg
    .supply.pos ~> .r1 ~> .load.pos
    .load.voltage == .supply.voltage - .r1.drop
    .load.current == 0.5W / .load.voltage{extra}
}}
"#
    )
}

/// The same circle through a part that has no rating, so that no check reads it.
fn unrated() -> String {
    r#"import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

part Link {
    .designator &= "R"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    bridge .a ~> .b
}

board Circle {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 9V..16V
        .pos.net &= "VBATT"
        .neg.net &= "GND"
    }
    .r1: Link { .resistance &= 10ohm +/- 1% }
    .load: Rail {
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
    }
    .supply.neg ~ .load.neg
    .supply.pos ~> .r1 ~> .load.pos
    .load.voltage == .supply.voltage - .r1.drop
    .load.current == 0.5W / .load.voltage
}
"#
    .to_string()
}

#[test]
fn a_circle_with_a_check_inside_it_warns_and_names_the_check_and_a_start() {
    let boards = build(&rated(""), &Catalogs::jlcpcb()).unwrap();
    let warnings = boards[0].guideline_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let w = &warnings[0];
    assert!(w.starts_with("Circle: open circle guideline: these values depend on each other and none has a value: `load.current`, `load.voltage`"), "{w}");
    assert!(
        w.contains("`(r1.drop * r1.current) <= r1.max_power`"),
        "{w}"
    );
    assert!(
        w.contains("are not evaluated, and the parts they would judge are chosen without them"),
        "{w}"
    );
    assert!(
        w.contains("state a range on one of them with `&=`, for example `load.voltage`"),
        "{w}"
    );
    // The note is still there.
    assert!(
        boards[0]
            .notes
            .iter()
            .any(|n| n.contains("depend on each other"))
    );
}

#[test]
fn a_strict_build_refuses_it_with_the_line() {
    let strict = Catalogs::jlcpcb().with_strict_guidelines(true);
    let Err(e) = build(&rated(""), &strict) else {
        panic!("a strict build must stop at an open circle with a check in it");
    };
    assert!(e.contains("error: Circle: open circle guideline:"), "{e}");
    assert!(e.contains("--> circle.nsm:"), "{e}");
}

#[test]
fn a_started_circle_has_no_finding_and_its_checks_are_made() {
    let src = rated("\n    .load.voltage &= 5V..16V");
    let boards = build(&src, &Catalogs::jlcpcb().with_strict_guidelines(true)).unwrap();
    assert!(boards[0].guidelines.is_empty());
}

#[test]
fn a_circle_with_no_check_inside_it_is_not_refused() {
    let strict = Catalogs::jlcpcb().with_strict_guidelines(true);
    // No KiCad symbol of ours draws the invented part, so the sources are built without them.
    let built = netsmith::outcome_with("circle.nsm", &unrated(), &Loader::std(), &strict, None)
        .unwrap_or_else(|e| panic!("{}", netsmith::render_problems(&e)));
    assert!(built.boards[0].guidelines.is_empty());
    // It is still named in a note: nothing in it passes.
    assert!(
        built.boards[0]
            .notes
            .iter()
            .any(|n| n.contains("depend on each other"))
    );
}

#[test]
fn a_circle_that_also_waits_on_an_unstated_value_says_which() {
    // The load also draws `.extra`, which nothing states: a range on the circle alone would
    // not start it.
    let src = rated("\n    .extra: Current")
        .replace("0.5W / .load.voltage", "0.5W / .load.voltage + .extra");
    let boards = build(&src, &Catalogs::jlcpcb()).unwrap();
    let warnings = boards[0].guideline_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("a range on them alone would not start them, because they also read `extra`, which nothing states: state it first"),
        "{}",
        warnings[0]
    );
}

#[test]
fn a_circle_that_waits_on_a_value_that_waits_on_an_unstated_one_names_the_unstated_one() {
    // The load draws `.extra`, which is worked out from `.other`, which nothing states: the
    // circle reads a value that something sets, and it is that something that waits.
    let src = rated("\n    .extra: Current\n    .other: Current\n    .extra == .other * 2")
        .replace("0.5W / .load.voltage", "0.5W / .load.voltage + .extra");
    let boards = build(&src, &Catalogs::jlcpcb()).unwrap();
    let warnings = boards[0].guideline_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("a range on them alone would not start them, because they also read `other`, which nothing states: state it first"),
        "{}",
        warnings[0]
    );
    assert!(!warnings[0].contains("read ,"), "{}", warnings[0]);
}

#[test]
fn a_long_list_of_waited_on_values_is_cut_like_the_circle_s() {
    // Twelve unstated currents, summed into the load's draw.
    let mut extra = String::from("\n    .total: Current\n");
    let mut sum = Vec::new();
    for n in 0..12 {
        extra.push_str(&format!("    .i{n:02}: Current\n"));
        sum.push(format!(".i{n:02}"));
    }
    extra.push_str(&format!("    .total == {}", sum.join(" + ")));
    let src = rated(&extra).replace("0.5W / .load.voltage", "0.5W / .load.voltage + .total");
    let boards = build(&src, &Catalogs::jlcpcb()).unwrap();
    let warnings = boards[0].guideline_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("because they also read `i00`, `i01`, `i02`, `i03`, `i04`, `i05`, `i06`, `i07`, `i08`, `i09` and 2 more, which nothing states: state them first"),
        "{}",
        warnings[0]
    );
}

#[test]
fn a_circle_that_waits_on_another_circle_names_the_value_it_reads_from_it() {
    // `.extra` is part of a circle of its own (`.extra` and `.back` set each other), and the
    // load reads it: nothing in either can be set first, so the value it reads is named.
    let src = rated(
        "\n    .extra: Current\n    .back: Current\n    .extra == .back * 2\n    .back == .extra / 2",
    )
    .replace("0.5W / .load.voltage", "0.5W / .load.voltage + .extra");
    let boards = build(&src, &Catalogs::jlcpcb()).unwrap();
    let warnings = boards[0].guideline_warnings();
    assert!(!warnings.is_empty(), "{warnings:?}");
    for w in &warnings {
        assert!(!w.contains("read ,"), "{w}");
    }
    assert!(
        warnings
            .iter()
            .any(|w| w
                .contains("because they also read `extra`, which nothing states: state it first")),
        "{warnings:?}"
    );
}

// ---- a circle through the currents of a net (issue #18) ---------------------------------------

/// A divider (two resistors, a limit on its draw) behind a resistor in series, on a supply of
/// 14 V to 22 V; `extra` goes into the board.
fn behind(extra: &str) -> String {
    format!(
        r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

module Divider {{
    .vin: Rail
    .mid: Pin
    .max_draw: Current
    .top: Resistor
    .bottom: Resistor
    .vin.pos ~> .top ~> .mid
    .mid ~> .bottom ~> .vin.neg
    .top.current == .vin.voltage / (.top.resistance + .bottom.resistance)
    .bottom.current == .top.current
    .top.current <= .max_draw
}}

board Behind {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 14V..22V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .series: Resistor {{ .resistance &= 68ohm +/- 1% }}
    .after: Rail {{
        .pos.net &= "AFTER"
        .neg.net &= "GND"
    }}
    .supply.neg ~ .after.neg
    .supply.pos ~> .series ~> .after.pos
    .after.voltage == .supply.voltage - .series.drop
    .divider: Divider {{ .max_draw &= 40uA }}
    .divider.mid.net &= "MID"
    .after ~ .divider.vin{extra}
}}
"#
    )
}

#[test]
fn a_circle_through_a_net_with_two_unknown_currents_warns_and_says_what_to_state() {
    let boards = build(&behind(""), &Catalogs::jlcpcb()).unwrap();
    let warnings = boards[0].guideline_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let w = &warnings[0];
    assert!(
        w.starts_with("Behind: open circle guideline: these values depend on each other and none has a value: `after.current`, `after.voltage`, `divider.top.current`"),
        "{w}"
    );
    // The check the pick would have needed comes before the library's ratings.
    assert!(
        w.contains("so the checks `divider.top.current <= divider.max_draw`, "),
        "{w}"
    );
    assert!(
        w.contains("are not evaluated, and the parts they would judge are chosen without them; net `AFTER` carries 3 currents (`after.current`, `divider.vin.current`, `series.current`), none of which has a value, and the solver works a part's current out only from a net of two currents (ADR 0026)"),
        "{w}"
    );
    assert!(
        w.contains("state a range on `series.current` (the part's current: the most and the least that the net's loads draw through it), or on a voltage in the circle, for example `after.voltage`; if nothing else draws from the net, `after.current &= 0A` makes it a circle of equations, which a voltage starts; the checks are then made against what you state"),
        "{w}"
    );
    // The pick is made as it was: without the limit.
    let parts: Vec<String> = boards[0]
        .design
        .parts
        .iter()
        .map(|p| p.value.clone())
        .collect();
    assert_eq!(
        parts.iter().filter(|v| v.as_str() == "330").count(),
        2,
        "{parts:?}"
    );
}

#[test]
fn a_strict_build_refuses_a_circle_through_a_net() {
    let strict = Catalogs::jlcpcb().with_strict_guidelines(true);
    let Err(e) = build(&behind(""), &strict) else {
        panic!("a strict build must stop at it");
    };
    assert!(e.contains("error: Behind: open circle guideline:"), "{e}");
    assert!(e.contains("net `AFTER` carries 3 currents"), "{e}");
}

#[test]
fn a_start_ends_the_finding_and_the_limit_is_made_part_of_the_pick() {
    for start in [
        "\n    .divider.vin.voltage &= 0V..22V",
        "\n    .series.current &= 0A..4mA",
    ] {
        let strict = Catalogs::jlcpcb().with_strict_guidelines(true);
        let boards = build(&behind(start), &strict).unwrap_or_else(|e| panic!("{start}: {e}"));
        assert!(boards[0].guidelines.is_empty(), "{start}");
        let parts: Vec<String> = boards[0]
            .design
            .parts
            .iter()
            .map(|p| p.value.clone())
            .collect();
        // 22 V over the divider's two resistors has to stay under 40 uA: not 330 and 330.
        assert_eq!(
            parts.iter().filter(|v| v.as_str() == "330").count(),
            1,
            "{start}: {parts:?}"
        );
    }
}

#[test]
fn nothing_else_on_the_net_leaves_an_ordinary_circle() {
    let boards = build(&behind("\n    .after.current &= 0A"), &Catalogs::jlcpcb()).unwrap();
    let warnings = boards[0].guideline_warnings();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].contains("state a range on one of them with `&=`, for example `after.voltage`"),
        "{}",
        warnings[0]
    );
    assert!(
        !warnings[0].contains("net `AFTER` carries"),
        "{}",
        warnings[0]
    );
}

#[test]
fn no_check_that_reads_the_circle_no_finding() {
    // The same shape through parts that have no rating, and no limit on the draw: no check
    // that could be made is waiting.
    let src = r#"import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

part Link {
    .designator &= "R"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    bridge .a ~> .b
}

module Divider {
    .vin: Rail
    .mid: Pin
    .top: Link { .resistance &= 100kohm +/- 1% }
    .bottom: Link { .resistance &= 33kohm +/- 1% }
    .vin.pos ~> .top ~> .mid
    .mid ~> .bottom ~> .vin.neg
    .top.current == .vin.voltage / (.top.resistance + .bottom.resistance)
    .bottom.current == .top.current
}

board Behind {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 14V..22V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .series: Link { .resistance &= 68ohm +/- 1% }
    .after: Rail {
        .pos.net &= "AFTER"
        .neg.net &= "GND"
    }
    .supply.neg ~ .after.neg
    .supply.pos ~> .series ~> .after.pos
    .after.voltage == .supply.voltage - .series.drop
    .divider: Divider
    .divider.mid.net &= "MID"
    .after ~ .divider.vin
}
"#;
    let strict = Catalogs::jlcpcb().with_strict_guidelines(true);
    let built = netsmith::outcome_with("behind.nsm", src, &Loader::std(), &strict, None)
        .unwrap_or_else(|e| panic!("{}", netsmith::render_problems(&e)));
    assert!(built.boards[0].guidelines.is_empty());
    // One bound on the divider's current is all it takes to make the same board a finding.
    let limited = src.replace(
        ".bottom.current == .top.current",
        ".bottom.current == .top.current\n    .top.current <= 40uA",
    );
    let Err(e) = netsmith::outcome_with("behind.nsm", &limited, &Loader::std(), &strict, None)
    else {
        panic!("a bound on the divider's current must make it a finding");
    };
    assert!(netsmith::render_problems(&e).contains("net `AFTER` carries 3 currents"));
}
