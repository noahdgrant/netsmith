//! The power-up order of a board (issue #1267): `sequence { .a.vout; then 6ms; .b.vout }` is held
//! against when each rail comes up and each enable pin goes high, from the regulators'
//! `.enable_delay` and `.soft_start_time` and an RC on an enable pin.

use netsmith_check::{Checked, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    for (name, path, text) in [
        (
            "capacitor",
            "data/jlcpcb/catalog/capacitor.csv",
            include_str!("../../../data/jlcpcb/catalog/capacitor.csv"),
        ),
        (
            "resistor",
            "data/jlcpcb/catalog/resistor.csv",
            include_str!("../../../data/jlcpcb/catalog/resistor.csv"),
        ),
    ] {
        c.add_file(name, path, text).unwrap();
    }
    c
}

/// A regulator with an enable pin, whose delays a board states.
const REG: &str = "\
import std::parts::passive::{Capacitor, Resistor}
import std::sources::Rail

part Reg {
    .designator &= \"U\"
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    pins {
        .vin.pos  power \"VIN\"  \"1\"
        .vin.neg  power \"GND\"  \"2\"
        .vout.pos power \"VOUT\" \"3\"
        .en       input \"EN\"   \"4\"
    }
    .enable_delay: Time
    .enable_delay ?= 0s
    .soft_start_time: Time
    .enable_threshold: Voltage
    .vin.current == -.vout.current
    dc linear_regulator .vin .vout
}
";

/// Regulator `.a` from the input, enabled with it, and `.b` from `.a`'s output with `b_en` on
/// its enable, and `sequence`.
fn board(b_en: &str, sequence: &str) -> String {
    format!(
        "{REG}\n\
         board B {{\n    \
         .input: Rail {{ .voltage &= 5V +/- 5%; .pos.net &= \"VIN\"; .neg.net &= \"GND\" }}\n    \
         .a: Reg {{ .enable_delay &= 100us..300us; .soft_start_time &= 1ms..2ms; .enable_threshold &= 1V..1.2V }}\n    \
         .b: Reg {{ .soft_start_time &= 500us..1ms; .enable_threshold &= 1V..1.2V }}\n    \
         .a.vout.voltage &= 3.3V +/- 3%\n    \
         .b.vout.voltage &= 1.8V +/- 3%\n    \
         .load: Rail {{ .current &= 1mA..10mA }}\n    \
         .b.vout ~ .load\n    \
         .input ~ .a.vin\n    \
         .a.vout ~ .b.vin\n    \
         .a.en ~ .input.pos\n\
         {b_en}\n\
         {sequence}\n}}\n"
    )
}

fn solved(src: &str) -> Solution {
    let checked: Checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("a board");
    solve(board, &catalogs())
}

fn sequence_messages(src: &str) -> (Vec<String>, Vec<String>) {
    let s = solved(src);
    let faults = s
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .filter(|m| m.contains("`sequence`"))
        .collect();
    let notes = s
        .not_evaluated
        .iter()
        .map(|n| n.message.clone())
        .filter(|m| m.contains("`sequence`"))
        .collect();
    (faults, notes)
}

/// The enable of `.b` from `.a`'s output through 100 kohm, held by `cap` to ground.
fn rc(cap: &str) -> String {
    format!(
        "    .r: Resistor {{ .resistance &= 100kohm +/- 1% }}\n    \
         .c: Capacitor {{ .capacitance &= {cap} +/- 10%; .max_voltage >= 6.3V }}\n    \
         .r.a ~ .a.vout.pos\n    \
         .r.b ~ .b.en\n    \
         .c.a ~ .b.en\n    \
         .c.b ~ .input.neg"
    )
}

#[test]
fn an_order_the_regulators_keep_says_nothing() {
    // `.a` is up by 2.3 ms at the latest. An RC of 100 kohm and 1 uF on `.b.en` (a time
    // constant of 89 ms at the least) holds `.b` off for about 31 ms from the start of `.a`'s
    // rise, well past the 6 ms after `.a` is up.
    let (faults, notes) = sequence_messages(&board(
        &rc("1uF"),
        "    sequence { .a.vout; then 6ms; .b.vout }",
    ));
    assert!(faults.is_empty() && notes.is_empty(), "{faults:#?} {notes:#?}");
}

#[test]
fn an_order_that_is_not_kept_names_the_two_events_and_the_margin() {
    let (faults, _) = sequence_messages(&board(
        "    .b.en ~ .a.vout.pos",
        "    sequence { .a.vout; then 6ms; .b.vout }",
    ));
    assert_eq!(faults.len(), 1, "{faults:#?}");
    let m = &faults[0];
    for part in [
        "`b.vout` comes at least 6ms after `a.vout`",
        "`a.vout` is up by 2.3ms at the latest",
        "`b.vout` is up from 600us at the earliest",
        "the margin is -7.7ms, 7.7ms short",
    ] {
        assert!(m.contains(part), "`{part}` is not in `{m}`");
    }
}

#[test]
fn an_rc_on_an_enable_pin_delays_it_by_its_time_constant_to_the_threshold() {
    // 100 kohm and 10 nF: at the least 99 kohm * 9 nF * ln(3.399 / (3.399 - 1)) = 0.31 ms
    // after `.a` starts to rise (100 us), so `b.en` goes high from about 410 us: before
    // `.a` is up, and a `then 6ms` after `.a.vout` is not kept.
    let (faults, _) = sequence_messages(&board(
        &rc("10nF"),
        "    sequence { .a.vout; then 6ms; .b.en }",
    ));
    assert_eq!(faults.len(), 1, "{faults:#?}");
    assert!(
        faults[0].contains("`b.en` goes high from")
            && faults[0].contains("held by")
            && faults[0].contains("short"),
        "{}",
        faults[0]
    );
}

#[test]
fn after_orders_the_event_above_it_after_the_one_it_names() {
    let (faults, _) = sequence_messages(&board(
        "    .b.en ~ .a.vout.pos",
        "    sequence { .b.vout; after .a.vout }",
    ));
    // `.b` follows `.a`'s output, but rises faster: it may be up before `.a` is.
    assert_eq!(faults.len(), 1, "{faults:#?}");
    assert!(
        faults[0].contains("`b.vout` comes at or after `a.vout`")
            && faults[0].contains("before it"),
        "{}",
        faults[0]
    );
}

#[test]
fn an_enable_that_a_gpio_drives_is_not_evaluated_and_says_why() {
    let gpio = "\
part Mcu {
    .designator &= \"U\"
    .power: Rail
    pins {
        .power.pos power  \"VDD\" \"1\"
        .power.neg power  \"VSS\" \"2\"
        .pa1       output \"PA1\" \"3\"
    }
}
";
    let src = board(
        "    .mcu: Mcu\n    .mcu.power ~ .a.vout\n    .mcu.pa1 ~ .b.en",
        "    sequence { .a.vout; then 6ms; .b.vout }",
    )
    .replacen("board B", &format!("{gpio}\nboard B"), 1);
    let (faults, notes) = sequence_messages(&src);
    assert!(faults.is_empty(), "{faults:#?}");
    assert_eq!(notes.len(), 1, "{notes:#?}");
    assert!(
        notes[0].contains("is driven by `mcu.pa1`"),
        "{}",
        notes[0]
    );
}

#[test]
fn a_regulator_that_states_no_soft_start_leaves_the_latest_time_open() {
    let src = board(
        "    .b.en ~ .a.vout.pos",
        "    sequence { .a.vout; then 6ms; .b.vout }",
    )
    .replace(".soft_start_time &= 1ms..2ms; ", "");
    let (faults, notes) = sequence_messages(&src);
    assert!(faults.is_empty(), "{faults:#?}");
    assert_eq!(notes.len(), 1, "{notes:#?}");
    assert!(
        notes[0].contains("`a` states no `.soft_start_time`"),
        "{}",
        notes[0]
    );
}

#[test]
fn a_load_switch_drives_its_output_from_its_enable_and_leaves_its_latest_time_open() {
    let switch = "\
part Sw {
    .designator &= \"U\"
    .in: Rail
    .out: Rail
    .in.neg ~ .out.neg
    pins {
        .in.pos  power \"IN\"  \"1\"
        .in.neg  power \"GND\" \"2\"
        .out.pos power \"OUT\" \"3\"
        .enable  input \"EN\"  \"4\"
    }
    .on_resistance: Resistance
    .on_resistance &= 50mohm..80mohm
    .in.current == -.out.current
    .out.voltage == .in.voltage - .on_resistance * (-.out.current)
    dc load_switch .in .out .enable .on_resistance
}
";
    let with_switch = |sequence: &str| {
        board(
            "    .b.en ~ .a.vout.pos\n    .sw: Sw\n    .sw.in ~ .a.vout\n    .sw.enable ~ .b.vout.pos\n    .sensor: Rail { .current &= 1mA..5mA }\n    .sw.out ~ .sensor",
            sequence,
        )
        .replacen("board B", &format!("{switch}\nboard B"), 1)
    };
    // The switch turns on once `.b` is up, from 600 us at the earliest: after `.a.vout`
    // starts, but not 6 ms after it is up.
    let (faults, notes) = sequence_messages(&with_switch("    sequence { .a.vout; then 6ms; .sw.out }"));
    assert!(notes.is_empty(), "{notes:#?}");
    assert_eq!(faults.len(), 1, "{faults:#?}");
    assert!(
        faults[0].contains("`sw` starts when its input is up and `sw.enable` is high"),
        "{}",
        faults[0]
    );
    // As the earlier event its latest time is not known.
    let (faults, notes) = sequence_messages(&with_switch("    sequence { .sw.out; then 1ms; .b.vout }"));
    assert!(faults.is_empty(), "{faults:#?}");
    assert!(
        notes.len() == 1 && notes[0].contains("the turn-on time of the load switch `sw` is not modelled"),
        "{notes:#?}"
    );
}
