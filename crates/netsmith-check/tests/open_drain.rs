//! The kind `open_drain` (#460): an output that only pulls its line low or lets it go, so that
//! several may share a line, and whose high comes from a pull-up.
//!
//! The parts are invented for these tests.

use netsmith_check::*;

const PARTS: &str = r#"
import std::parts::passive::Resistor
import std::sources::Rail

/// A comparator whose output is open drain.
part Zcmp {
    pins {
        .out open_drain "OUT" "1"
    }
}

/// A part whose datasheet draws the pin as a push-pull output.
part Zdrv {
    pins {
        .out output "OUT" "1"
    }
}

/// A slave whose data out is high impedance while it is not selected.
part Zrom {
    pins {
        .do tri_state "DO" "1"
    }
}

part Zsink {
    pins {
        .din input "DIN" "1"
    }
}

part Zgpio {
    pins {
        .io bidirectional "IO" "1"
    }
}
"#;

fn run(body: &str) -> Checked {
    check(
        "test.nsm",
        &format!("{PARTS}\nboard B {{\n{body}\n}}\n"),
        &Loader::std(),
    )
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// Two open-drain outputs and a reader on one line, pulled up to `.v3`.
const PULLED: &str = "    .v3: Rail\n    .r: Resistor\n    .a: Zcmp\n    .b: Zcmp\n    .s: Zsink\n    .a.out ~ .b.out\n    .a.out ~ .s.din\n    .a.out ~ .r.a\n    .r.b ~ .v3.pos";

#[test]
fn two_open_drain_outputs_with_a_pull_up_to_a_rail_share_a_net() {
    let c = run(PULLED);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_pull_up_to_a_pin_that_drives_high_counts() {
    // A GPIO takes the line high through a series resistor, as on the charger's /TCPP_EN.
    let c = run(
        "    .r: Resistor\n    .a: Zcmp\n    .g: Zgpio\n    .s: Zsink\n    .a.out ~ .s.din\n    .a.out ~ .r.a\n    .r.b ~ .g.io",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn an_open_drain_line_with_no_pull_up_is_refused_and_says_what_to_add() {
    let c =
        run("    .a: Zcmp\n    .b: Zcmp\n    .s: Zsink\n    .a.out ~ .b.out\n    .a.out ~ .s.din");
    assert_eq!(
        messages(&c),
        [
            "net `net-a.out` has open_drain outputs (`a.out`, `b.out`) and no pull-up: nothing takes the line high when they let go of it; join a resistor from the net to a rail's `pos`, or to a pin that drives the line high"
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_resistor_to_ground_is_not_a_pull_up() {
    let c = run(
        "    .v3: Rail\n    .r: Resistor\n    .a: Zcmp\n    .s: Zsink\n    .a.out ~ .s.din\n    .a.out ~ .r.a\n    .r.b ~ .v3.neg",
    );
    assert_eq!(
        messages(&c),
        [
            "net `net-a.out` has an open_drain output (`a.out`) and no pull-up: nothing takes the line high when it lets go of it; join a resistor from the net to a rail's `pos`, or to a pin that drives the line high"
        ],
        "{}",
        c.render()
    );
}

#[test]
fn an_open_drain_and_a_push_pull_output_on_one_net_are_refused() {
    let c = run(&format!("{PULLED}\n    .d: Zdrv\n    .a.out ~ .d.out"));
    assert_eq!(
        messages(&c),
        [
            "net `net-a.out` has open_drain outputs (`a.out`, `b.out`) and a push-pull driver: `d.out` (`Zdrv`, an output); it drives the line high while an open_drain pin pulls it low; give `d.out` a net of its own, or write `open_drain` for it if the datasheet says the pin is open drain"
        ],
        "{}",
        c.render()
    );
}

#[test]
fn an_open_drain_and_a_tri_state_output_on_one_net_are_refused() {
    let c = run(&format!("{PULLED}\n    .m: Zrom\n    .a.out ~ .m.do"));
    assert_eq!(
        messages(&c),
        [
            "net `net-a.out` has open_drain outputs (`a.out`, `b.out`) and a push-pull driver: `m.do` (`Zrom`, a tri_state output); it drives the line high while an open_drain pin pulls it low; give `m.do` a net of its own, or write `open_drain` for it if the datasheet says the pin is open drain"
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_bidirectional_pin_beside_an_open_drain_output_is_accepted() {
    let c = run(&format!("{PULLED}\n    .g: Zgpio\n    .a.out ~ .g.io"));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn an_open_drain_output_may_float() {
    let c = run("    .a: Zcmp");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn optional_is_refused_on_an_open_drain_output_and_required_is_accepted() {
    let src = |word: &str| {
        format!(
            "part Z {{\n    pins {{\n        .out open_drain \"OUT\" \"1\" {word}\n    }}\n}}\nboard B {{\n    .z: Z\n}}\n"
        )
    };
    let c = check("test.nsm", &src("optional"), &Loader::std());
    assert_eq!(
        messages(&c),
        [
            "`optional` is for a power or input pin, which must be connected unless it says so; `.out` is an open_drain output and may float already"
        ],
        "{}",
        c.render()
    );
    let c = check("test.nsm", &src("required"), &Loader::std());
    assert!(
        messages(&c)
            .iter()
            .all(|m| !m.contains("`required` is for")),
        "{}",
        c.render()
    );
}
