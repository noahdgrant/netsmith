//! A system of two boards: values and currents cross the join, and a join that cannot hold
//! is reported with both boards, the port and the numbers.

use netsmith_check::{Checked, Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve_system};

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

fn run(src: &str) -> (Checked, Solution) {
    let c = check("two_boards.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    let s = solve_system(&c.systems[0], &Catalogs::jlcpcb());
    (c, s)
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

/// The example with the LED board's LED run at `current` whatever its resistor says: the
/// formula that ties the current to the resistor is dropped, so the board draws what is
/// stated and the parts picked do not matter.
fn led_draws(current: &str) -> String {
    changed(
        ".led.current == (.input.voltage - .led.forward_voltage) / .r1.resistance\n",
        "",
    )
    .replace(".current &= 5mA..20mA", &format!(".current &= {current}"))
}

/// The line of the example that joins the two boards: the two connectors are mated, and the
/// mate derives the join of the ports they carry (ADR 0040).
const MATE: &str = "    .power.power_out.mating ~ .led.power_in.mating\n";

/// The example with one line of it changed.
fn changed(from: &str, to: &str) -> String {
    assert!(TWO_BOARDS.contains(from), "`{from}` is not in the example");
    TWO_BOARDS.replacen(from, to, 1)
}

#[test]
fn the_example_solves_and_the_led_boards_voltage_comes_from_the_regulator_board() {
    let (_, s) = run(TWO_BOARDS);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    // The LED board states no voltage; it takes the regulator board's 3.3 V +/- 5%.
    assert!(close(
        range(&s, "led.input.voltage"),
        (3.3 * 0.95, 3.3 * 1.05)
    ));
    assert!(close(
        range(&s, "power.output.voltage"),
        (3.3 * 0.95, 3.3 * 1.05)
    ));
    // Parts are picked on both boards.
    let picked: Vec<String> = s.picks.iter().map(|p| s.model.path_of(p.part)).collect();
    for part in ["power.stage.regulator", "led.led", "led.r1"] {
        assert!(picked.contains(&part.to_string()), "{picked:?}");
    }
}

#[test]
fn what_the_led_board_draws_is_within_what_the_regulator_board_is_built_to_give() {
    let (_, s) = run(TWO_BOARDS);
    // The LED board asks for 5 to 20 mA; its resistor lets 7.28 to 16.8 mA through, and that
    // is what the input draws. The regulator board's output is written as what the outside
    // draws, 0 to 50 mA, and stays as its board wrote it.
    assert!(close(
        range(&s, "led.input.current"),
        (-0.016818181818181822, -0.007277227722772274)
    ));
    assert!(close(range(&s, "power.output.current"), (0.0, 0.05)));
    // ...and the regulator behind the output is checked against it as before.
    assert!(close(range(&s, "power.input.current"), (-0.05, 0.0)));
}

#[test]
fn a_board_that_draws_more_than_the_other_is_built_to_give_names_both_boards_and_the_numbers() {
    let (_, s) = run(&led_draws("150mA"));
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    let m = &msgs[0];
    for needle in [
        "led.input ~ power.output",
        "`power.output.current` (board `PowerBoard`)",
        "0A..50mA",
        "`led.input.current` (board `LedBoard`)",
        "-150mA",
    ] {
        assert!(m.contains(needle), "`{needle}` is not in: {m}");
    }
    assert!(m.contains("cannot add up to zero"), "{m}");
}

#[test]
fn a_draw_just_over_the_envelope_is_the_regulator_boards_to_refuse() {
    // 40 to 60 mA overlaps the envelope of 0 to 50 mA, so the currents can add up, but the
    // board may be asked for more than it was built for.
    let (_, s) = run(&led_draws("40mA..60mA"));
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    let m = &msgs[0];
    assert!(
        m.contains("asks more of a port than its board allows")
            && m.contains("`power.output.current` (board `PowerBoard`) is 0A..50mA")
            && m.contains("`led.input.current` (board `LedBoard`) is -60mA..-40mA")
            && m.contains("draw 40mA..60mA"),
        "{m}"
    );
}

#[test]
fn a_draw_the_regulator_board_never_stated_is_checked_by_its_regulator() {
    // The regulator board states no limit on its output, so the LED board's draw is what it
    // has to give, and the regulator in it can give 100 mA at most.
    let src = led_draws("150mA").replacen(".current &= 0mA..50mA\n", "\n", 1);
    let (_, s) = run(&src);
    let msgs = messages(&s);
    assert!(
        msgs.iter()
            .any(|m| m.contains("150mA") && m.contains("100mA")),
        "{msgs:#?}"
    );
}

#[test]
fn the_regulator_board_checks_the_draw_it_is_given_without_a_statement_of_its_own() {
    let src = changed(".current &= 0mA..50mA\n", "\n");
    let (_, s) = run(&src);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    // The output is what the LED board draws: the 7.28 to 16.8 mA its resistor lets through,
    // not the 5 to 20 mA window the LED states, which the output was left at while it
    // went stale.
    let drawn = (0.007277227722772274, 0.016818181818181822);
    assert!(close(range(&s, "power.output.current"), drawn));
    assert!(close(
        range(&s, "power.input.current"),
        (-drawn.1, -drawn.0)
    ));
}

#[test]
fn ports_whose_voltages_cannot_meet_are_refused_naming_both_boards_and_values() {
    let src = changed(
        "    .input: Rail {\n        .pos.net &= \"3V3\"",
        "    .input: Rail {\n        .voltage &= 5V\n        .pos.net &= \"3V3\"",
    );
    let (_, s) = run(&src);
    let msgs = messages(&s);
    let m = &msgs[0];
    assert!(
        m.contains("connected values disagree")
            && m.contains("`power.output.voltage` is 3.135V..3.465V")
            && m.contains("`led.input.voltage` is 5V"),
        "{m}"
    );
}

#[test]
fn a_wider_voltage_on_one_side_is_narrowed_to_what_the_other_gives() {
    let src = changed(
        "    .input: Rail {\n        .pos.net &= \"3V3\"",
        "    .input: Rail {\n        .voltage &= 3V..4V\n        .pos.net &= \"3V3\"",
    );
    let (_, s) = run(&src);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "led.input.voltage"), (3.135, 3.465)));
}

#[test]
fn an_unjoined_system_leaves_each_boards_ports_alone() {
    let src = changed(MATE, "");
    let (_, s) = run(&src);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(range(&s, "led.input.current"), (-0.02, -0.005)));
    assert!(matches!(
        s.model.domain_at("led.input.voltage"),
        Some(Domain::Unset)
    ));
}

#[test]
fn the_same_board_twice_is_two_boards() {
    let src = TWO_BOARDS.replace(
        MATE,
        "    .led2: LedBoard\n    .power.output ~ .led.input\n    .power.output ~ .led2.input\n",
    );
    let (_, s) = run(&src);
    // Two LEDs on one output draw 14.6 to 33.6 mA between them, within the 50 mA.
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert!(close(
        range(&s, "led2.input.current"),
        (-0.016818181818181822, -0.007277227722772274)
    ));
}

#[test]
fn many_boards_on_one_output_are_added_together() {
    // Three LED boards at up to 16.8 mA each (what their resistors allow, inside the 20 mA the
    // board asks for) is 50.5 mA: more than the 50 mA of the output.
    // One connector mates with one other, so the boards share the output by port joins.
    let src = TWO_BOARDS.replace(
        MATE,
        "    .led2: LedBoard\n    .led3: LedBoard\n    .power.output ~ .led.input\n    .power.output ~ .led2.input\n    .power.output ~ .led3.input\n",
    );
    let (_, s) = run(&src);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].contains("asks more of a port than its board allows")
            && msgs[0].contains("draw 21.83mA..50.46mA"),
        "{}",
        msgs[0]
    );
}

#[test]
fn the_led_boards_input_current_is_the_current_of_its_led_chain() {
    let (_, s) = run(TWO_BOARDS);
    assert!(
        !TWO_BOARDS.contains(".input.current ="),
        "the board states no draw of its own"
    );
    // The resistor and the LED pass one current, and the input gives exactly that.
    // The stated window is 5..20 mA; the resistor that was picked narrows it.
    let led = range(&s, "led.led.current");
    assert!(close(led, (0.007277227722772274, 0.016818181818181822)));
    assert!(close(range(&s, "led.r1.current"), led));
    assert!(close(range(&s, "led.input.current"), (-led.1, -led.0)));
    // Through the join it reaches the regulator board, which is checked against it.
    assert!(close(range(&s, "power.input.current"), (-0.05, 0.0)));
}

#[test]
fn an_led_that_draws_more_than_the_regulator_board_gives_is_refused_at_the_join() {
    // Said by the LED's own current, with nothing stated on the input.
    let (_, s) = run(&led_draws("80mA"));
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    for needle in [
        "led.input ~ power.output",
        "`power.output.current` (board `PowerBoard`) is 0A..50mA",
        "`led.input.current` (board `LedBoard`) is -80mA",
        "cannot add up to zero",
        "-80mA..-30mA",
    ] {
        assert!(
            msgs[0].contains(needle),
            "`{needle}` is not in: {}",
            msgs[0]
        );
    }
}
