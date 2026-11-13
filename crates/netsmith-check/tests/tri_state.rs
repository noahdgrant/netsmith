//! The kind `tri_state` (ADR 0064): an output that drives only while its part is selected,
//! so that several of them may share a line.
//!
//! The parts are invented for these tests.

use netsmith_check::*;

const PARTS: &str = r#"
/// A slave whose data out is `tri_state`, with the datasheet page in the comment.
part Zrom {
    pins {
        // Page 8: DO is high impedance while /CS is high.
        .do  tri_state "DO"  "1"
        .sel input     "CS"  "2" optional
    }
}

/// A slave whose datasheet draws DO as an output and says no more.
part Zgyro {
    pins {
        .do  output "DO" "1"
    }
}

part Zsink {
    pins {
        .din input "DIN" "1"
    }
}

part Zbidir {
    pins {
        .io bidirectional "IO" "1"
    }
}

part Zmcu {
    pins {
        .p1 bidirectional "P1" "1" fn GPIO:bidirectional DO:tri_state UART_TX:output
        .p2 bidirectional "P2" "2" fn GPIO:bidirectional DO:tri_state UART_TX:output
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

#[test]
fn two_tri_state_pins_share_a_net() {
    let c = run("    .a: Zrom\n    .b: Zrom\n    .s: Zsink\n    .a.do ~ .b.do\n    .a.do ~ .s.din");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn an_output_among_tri_state_pins_is_refused_and_says_what_to_read() {
    let c =
        run("    .a: Zrom\n    .b: Zgyro\n    .s: Zsink\n    .a.do ~ .b.do\n    .a.do ~ .s.din");
    assert_eq!(
        messages(&c),
        [
            "net `net-a.do` has 2 drivers and `b.do` is not `tri_state`: `a.do` (`Zrom`, a `tri_state` output) and `b.do` (`Zgyro`, an output); it would drive the net while another driver on it is selected; the datasheet of `Zgyro` must say that the pin is high impedance while its part is not selected (then write `tri_state` for it, with the page), or give `b.do` a net of its own"
        ],
        "{}",
        c.render()
    );
}

#[test]
fn two_outputs_keep_the_message_they_had() {
    let c = run("    .a: Zgyro\n    .b: Zgyro\n    .a.do ~ .b.do");
    assert_eq!(
        messages(&c),
        ["net `net-a.do` has two outputs: `a.do`, `b.do`"]
    );
}

#[test]
fn a_bidirectional_pin_beside_a_tri_state_pin_is_accepted() {
    let c = run("    .a: Zrom\n    .b: Zbidir\n    .a.do ~ .b.io");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_tri_state_pin_that_drives_a_receiver_alone_is_accepted() {
    let c = run("    .a: Zrom\n    .s: Zsink\n    .a.do ~ .s.din");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_tri_state_pin_may_stay_open_like_an_output() {
    let c = run("    .a: Zrom\n    .s: Zsink\n    .s.din ~ .s.din");
    // `.a.do` is not connected and `.a.sel` is optional; only the lone `din` is refused.
    let m = messages(&c);
    assert!(
        m.iter().all(|m| !m.contains("`a.do`")),
        "a tri_state pin is optional: {m:?}"
    );
}

#[test]
fn a_tri_state_function_kind_is_chosen_on_a_gpio() {
    // Two pins chosen as a `tri_state` function share a net; a transmitter beside one does not.
    let c = run(
        "    .a: Zmcu\n    .b: Zmcu\n    .a.p1 ~ .b.p1\n    .a.p1.function &= DO\n    .b.p1.function &= DO",
    );
    assert!(c.is_ok(), "{}", c.render());
    let c = run(
        "    .a: Zmcu\n    .b: Zmcu\n    .a.p1 ~ .b.p1\n    .a.p1.function &= DO\n    .b.p1.function &= UART_TX",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].starts_with("net `net-a.p1` has 2 drivers and `b.p1` is not `tri_state`"),
        "{}",
        m[0]
    );
}

#[test]
fn optional_is_useless_on_a_tri_state_row_and_required_is_not() {
    let c = check(
        "test.nsm",
        "part P {\n    pins {\n        .a tri_state \"A\" \"1\" optional\n    }\n}\n",
        &Loader::std(),
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("`.a` is a tri_state output and may float already"),
        "{}",
        m[0]
    );
    let c = check(
        "test.nsm",
        "part P {\n    pins {\n        .a tri_state \"A\" \"1\" required\n    }\n}\n",
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
}
