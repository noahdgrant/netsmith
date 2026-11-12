//! A push-pull `output` on the net of a rail is a short (#461), and any other kind may be tied to
//! a rail: a GPIO tied to ground is an ordinary circuit.
//!
//! The parts are invented for these tests.

use netsmith_check::*;

const PARTS: &str = r#"
import std::sources::Rail

part Zdrv {
    pins {
        .out output "OUT" "1"
    }
}

/// A part whose pin of each kind may be tied.
part Zmcu {
    pins {
        .pa9  bidirectional "PA9"  "1"
        .pa10 input         "PA10" "2"
        .sel  tri_state     "SEL"  "3"
        .flag open_drain    "FLAG" "4"
        .cfg  passive       "CFG"  "5"
    }
}
"#;

fn run(body: &str) -> Checked {
    check(
        "test.nsm",
        &format!("{PARTS}\nboard B {{\n    .gnd: Rail {{ .neg.net &= \"GND\" }}\n{body}\n}}\n"),
        &Loader::std(),
    )
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn an_output_tied_to_a_rail_is_a_short() {
    let c = run("    .d: Zdrv\n    .d.out ~ .gnd.neg");
    assert_eq!(
        messages(&c),
        [
            "`d.out` (`Zdrv`) is an output on net `GND`, the net of rail `gnd` (`gnd.neg`): a push-pull output drives the rail to its other level, which is a short; give it a net of its own, or, if its datasheet says the pin may be tied, write the kind it says (`bidirectional`, `open_drain` or `passive`)"
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_pin_of_any_other_kind_may_be_tied_to_a_rail() {
    let c = run(
        "    .u: Zmcu\n    .u.pa9 ~ .gnd.neg\n    .u.pa10 ~ .gnd.neg\n    .u.sel ~ .gnd.neg\n    .u.flag ~ .gnd.neg\n    .u.cfg ~ .gnd.neg",
    );
    assert!(
        messages(&c)
            .iter()
            .all(|m| !m.contains("is an output on net")),
        "{}",
        c.render()
    );
}
