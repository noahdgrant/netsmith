//! `AIN1 of ANALOG` in a pin row (#787): the function is the channel of the class `ANALOG` on
//! that pin. A request for the class gets a pin as any request does, and the pin file records
//! the channel of the pin it got, which is what firmware needs.
//!
//! `Zmcu6` is invented for these tests; it is not a real part and is not in the standard library.

use netsmith_check::*;

const ZMCU6: &str = r#"
part Zmcu6 {
    pins {
        .p00 bidirectional "P0.00" "3" fn GPIO class
        .p02 bidirectional "P0.02" "5" fn GPIO class ANALOG class AIN0:input of ANALOG
        .p03 bidirectional "P0.03" "6" fn GPIO class ANALOG class AIN1:input of ANALOG
    }
}

/// Something with a pin for each of the part's, to put a net on.
part Probe {
    pins {
        .a00 passive "A00" "1"
        .a01 passive "A01" "2"
    }
}
"#;

const TWO_SENSORS: &str =
    "    .a: .u.ANALOG\n    .b: .u.ANALOG\n    .a ~ .probe.a00\n    .b ~ .probe.a01\n";

fn run(statements: &str, file: Option<&str>) -> Checked {
    let loader = match file {
        Some(text) => Loader::std().with_pins("main.nsm", "main.pins.toml", text),
        None => Loader::std(),
    };
    let source = format!("{ZMCU6}\nboard B {{\n    .u: Zmcu6\n    .probe: Probe\n{statements}\n}}\n");
    check("main.nsm", &source, &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// A part on its own, for the mistakes in its table.
fn part_messages(rows: &str) -> Vec<String> {
    messages(&check(
        "main.nsm",
        &format!("part P {{\n    pins {{\n{rows}\n    }}\n}}\n"),
        &Loader::std(),
    ))
}

fn file(entries: &[(&str, &str, &str)]) -> String {
    let mut out = String::from("version = 1\n");
    for (key, pin, function) in entries {
        out.push_str(&format!(
            "\n[B.pins.{key}]\npin = \"{pin}\"\nfunction = \"{function}\"\nsource = \"hand\"\n"
        ));
    }
    out
}

#[test]
fn the_pin_file_records_the_channel_of_the_pin_a_request_for_the_class_got() {
    let c = run(TWO_SENSORS, None);
    assert!(c.is_ok(), "{}", c.render());
    // The board chose the class on the pins; the file says which channel each one is.
    let model = &c.board("B").unwrap().model;
    for a in &model.pin_assignments {
        assert_eq!(model.function_of(a.pin).as_deref(), Some("ANALOG"));
    }
    let text = c.pins.file.to_text();
    assert!(
        text.contains("[B.pins.a]\npin = \"p02\"\nfunction = \"AIN0\""),
        "{text}"
    );
    assert!(
        text.contains("[B.pins.b]\npin = \"p03\"\nfunction = \"AIN1\""),
        "{text}"
    );
    // What the optimizer knows: the capability, and the channel on each pin it may move to.
    let space = &c.pins.spaces["B"][0];
    let a = &space.requests[0];
    assert_eq!(
        (a.capability.as_str(), a.function_on("p02"), a.function_on("p03")),
        ("ANALOG", "AIN0", "AIN1")
    );
    // The file that holds is read back unchanged.
    let again = run(TWO_SENSORS, Some(&text));
    assert!(again.is_ok(), "{}", again.render());
    assert!(!again.pins.changed(), "{:?}", again.pins.changes);
}

#[test]
fn an_entry_on_another_pin_of_the_class_holds_and_records_that_pins_channel() {
    // `a` was moved to `p03` without its function line, and `b` is written with the class.
    let text = file(&[("a", "p03", "AIN0"), ("b", "p02", "ANALOG")]);
    let c = run(TWO_SENSORS, Some(&text));
    assert!(c.is_ok(), "{}", c.render());
    assert!(c.pins.findings.is_empty(), "{:?}", c.pins.findings);
    let out = c.pins.file.to_text();
    assert!(
        out.contains("[B.pins.a]\npin = \"p03\"\nfunction = \"AIN1\""),
        "{out}"
    );
    assert!(
        out.contains("[B.pins.b]\npin = \"p02\"\nfunction = \"AIN0\""),
        "{out}"
    );
}

#[test]
fn an_entry_for_a_channel_of_another_class_does_not_hold() {
    let text = file(&[("a", "p02", "GPIO")]);
    let c = run("    .a: .u.ANALOG\n    .a ~ .probe.a00\n", Some(&text));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        c.pins.findings,
        ["main.pins.toml:3: `a` was `u.p02`, but the entry is for GPIO and the request asks for ANALOG; a pin is picked again"]
    );
}

#[test]
fn a_request_for_a_class_with_no_channel_records_the_class() {
    let c = run("    .s: .u.GPIO\n    .s ~ .probe.a00\n", None);
    assert!(c.is_ok(), "{}", c.render());
    let text = c.pins.file.to_text();
    assert!(
        text.contains("[B.pins.s]\npin = \"p00\"\nfunction = \"GPIO\""),
        "{text}"
    );
}

#[test]
fn of_names_a_class_of_the_same_row() {
    assert_eq!(
        part_messages("        .p0 bidirectional \"P0\" \"1\" fn GPIO class AIN0 of ANALOG"),
        ["`.p0` writes `AIN0 of ANALOG`, but the row does not list `ANALOG`; `of` names a class of the same row, as in `fn GPIO class ANALOG class AIN1 of ANALOG`"]
    );
    assert_eq!(
        part_messages("        .p0 bidirectional \"P0\" \"1\" fn ANALOG AIN0 of ANALOG"),
        ["`.p0` writes `AIN0 of ANALOG`, but `ANALOG` is not a class; mark it `ANALOG class` on every row that lists it, or remove `of ANALOG`"]
    );
}

#[test]
fn a_channel_is_no_class() {
    assert_eq!(
        part_messages(
            "        .p0 bidirectional \"P0\" \"1\" fn ANALOG class AIN0 class of ANALOG"
        ),
        ["`.p0` marks `AIN0` as a class and as the channel of `ANALOG`; a channel is one function on one pin, so remove `class` or `of ANALOG`"]
    );
}

#[test]
fn a_pin_is_one_channel_of_one_class() {
    assert_eq!(
        part_messages(
            "        .p0 bidirectional \"P0\" \"1\" fn ANALOG class AIN0 of ANALOG AIN8 of ANALOG"
        ),
        ["`.p0` has two channels of `ANALOG`, `AIN0` and `AIN8`; a pin is one channel of a class"]
    );
    assert_eq!(
        part_messages(
            "        .p0 bidirectional \"P0\" \"1\" fn ANALOG class COMP class AIN0 of ANALOG CMP0 of COMP"
        ),
        ["`.p0` is a channel of two classes, `AIN0 of ANALOG` and `CMP0 of COMP`; a pin is the channel of one class"]
    );
}
