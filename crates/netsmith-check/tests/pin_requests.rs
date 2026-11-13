//! Pin requests (ADR 0101): `.status: .u.GPIO` asks a part for any pin that offers GPIO, and the
//! tool picks it, keeps it in the pin file, and runs the usual checks on the result.
//!
//! `Zmcu4` is invented for these tests; it is not a real part and is not in the standard library.

use netsmith_check::*;

const ZMCU4: &str = r#"
part Zmcu4 {
    peripheral UARTE0 on GPIO { UARTE0_TX:output UARTE0_RX:input }
    peripheral UARTE1 on GPIO { UARTE1_TX:output UARTE1_RX:input }
    shared UARTS limit 1 { UARTE0 UARTE1 }
    pins {
        .p00 bidirectional "P0.00" "3" fn GPIO
        .p01 bidirectional "P0.01" "4" fn GPIO
        .p02 bidirectional "P0.02" "5" fn GPIO ANALOG AIN0
        .p03 bidirectional "P0.03" "6" fn GPIO ANALOG AIN1
    }
}

/// Something with a pin for each of the part's, to put a net on.
part Probe {
    pins {
        .a00 passive "A00" "1"
        .a01 passive "A01" "2"
        .a02 passive "A02" "3"
        .a03 passive "A03" "4"
    }
}
"#;

fn source(statements: &str) -> String {
    format!("{ZMCU4}\nboard B {{\n    .u: Zmcu4\n    .probe: Probe\n{statements}\n}}\n")
}

fn run(statements: &str, file: Option<&str>) -> Checked {
    let loader = match file {
        Some(text) => Loader::std().with_pins("main.nsm", "main.pins.toml", text),
        None => Loader::std(),
    };
    check("main.nsm", &source(statements), &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The pins the requests got, as `request -> pin path in the part`.
fn pins(c: &Checked) -> Vec<(String, String)> {
    let model = &c.board("B").expect("board B").model;
    model
        .pin_assignments
        .iter()
        .map(|a| (a.key.clone(), model.path_below(a.pin, a.part)))
        .collect()
}

fn entry(pin: &str, function: &str, lock: bool) -> String {
    format!(
        "pin = \"{pin}\"\nfunction = \"{function}\"\n{}source = \"hand\"\n",
        if lock { "lock = true\n" } else { "" }
    )
}

fn file(entries: &[(&str, String)]) -> String {
    let mut out = String::from("version = 1\n");
    for (key, body) in entries {
        out.push_str(&format!("\n[B.pins.{key}]\n{body}"));
    }
    out
}

fn pair(a: &str, b: &str) -> (String, String) {
    (a.to_string(), b.to_string())
}

#[test]
fn a_request_gets_the_first_pin_that_offers_the_capability() {
    let c = run("    .status: .u.GPIO\n    .status ~ .probe.a00\n", None);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("status", "p00")]);
    // The pin is on the net of the request, with the function chosen.
    let model = &c.board("B").unwrap().model;
    let net = model
        .nets()
        .iter()
        .find(|n| n.terminals.iter().any(|&t| model.path_of(t) == "probe.a00"))
        .expect("a net");
    assert!(net.terminals.iter().any(|&t| model.path_of(t) == "u.p00"));
    let pin = model.pin_assignments[0].pin;
    assert_eq!(model.function_of(pin).as_deref(), Some("GPIO"));
}

#[test]
fn requests_are_served_in_the_order_of_their_paths_and_a_named_pin_stays() {
    // `a` comes before `b`: the order of the lines never matters. `p00` is the source's.
    let c = run(
        "    .b: .u.GPIO\n    .a: .u.GPIO\n    .u.p00 ~ .probe.a00\n    .u.p00.function &= GPIO\n    .a ~ .probe.a01\n    .b ~ .probe.a02\n",
        None,
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("a", "p01"), pair("b", "p02")]);
}

#[test]
fn the_tie_is_broken_by_the_pins_path_in_byte_order() {
    let part = r#"
part Zbyte {
    pins {
        .p0_2 bidirectional "P0.2" "1" fn GPIO
        .p0_10 bidirectional "P0.10" "2" fn GPIO
    }
}
board B {
    .u: Zbyte
    .status: .u.GPIO
}
"#;
    let c = check("main.nsm", part, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    // In byte order `p0_10` comes before `p0_2`, whatever the order of the rows.
    assert_eq!(
        model.path_below(model.pin_assignments[0].pin, model.pin_assignments[0].part),
        "p0_10"
    );
}

#[test]
fn the_function_is_a_peripherals_line_on_any_pin_of_its_class() {
    let c = run("    .tx: .u.UARTE1_TX\n    .tx ~ .probe.a00\n", None);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("tx", "p00")]);
    let model = &c.board("B").unwrap().model;
    assert_eq!(
        model.function_of(model.pin_assignments[0].pin).as_deref(),
        Some("UARTE1_TX")
    );
}

#[test]
fn a_class_may_be_chosen_on_two_pins_and_another_function_by_hand_may_not() {
    let c = run(
        "    .a: .u.GPIO\n    .b: .u.GPIO\n    .a ~ .probe.a00\n    .b ~ .probe.a01\n",
        None,
    );
    assert!(c.is_ok(), "{}", c.render());
    // A hand-written choice of GPIO on two pins is the same thing, not an error.
    let c = run(
        "    .u.p00.function &= GPIO\n    .u.p01.function &= GPIO\n",
        None,
    );
    assert!(c.is_ok(), "{}", c.render());
    // Two pins the source gives a function by hand are still two uses, class or not.
    let c = run(
        "    .u.p02.function &= ANALOG\n    .u.p03.function &= ANALOG\n",
        None,
    );
    assert_eq!(
        messages(&c),
        ["function `ANALOG` is chosen on both `u.p02` and `u.p03`"]
    );
}

#[test]
fn a_locked_pin_is_kept_and_the_file_does_not_change() {
    let text = file(&[("status", entry("p03", "GPIO", true))]);
    let c = run(
        "    .status: .u.GPIO\n    .status ~ .probe.a00\n",
        Some(&text),
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("status", "p03")]);
    assert!(!c.pins.changed(), "{:?}", c.pins.changes);
    assert!(c.pins.findings.is_empty());
}

#[test]
fn an_entry_that_holds_is_kept_when_an_earlier_pin_is_free() {
    // `p00` is free and sorts first, but the entry holds, so the pin does not move.
    let text = file(&[("status", entry("p02", "GPIO", false))]);
    let c = run(
        "    .status: .u.GPIO\n    .status ~ .probe.a00\n",
        Some(&text),
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("status", "p02")]);
    assert!(!c.pins.changed());
}

#[test]
fn a_new_request_goes_round_the_pins_the_entries_hold() {
    let text = file(&[("a", entry("p00", "GPIO", true))]);
    let c = run(
        "    .a: .u.GPIO\n    .b: .u.GPIO\n    .a ~ .probe.a00\n    .b ~ .probe.a01\n",
        Some(&text),
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("a", "p00"), pair("b", "p01")]);
    assert_eq!(c.pins.changes, ["B.b: new, pin `p01` (GPIO)"]);
    assert_eq!(
        c.pins.file.board("B").unwrap()["b"].source,
        EntrySource::Auto
    );
}

#[test]
fn the_file_is_the_same_text_on_every_run() {
    let statements = "    .b: .u.GPIO\n    .a: .u.ANALOG\n    .tx: .u.UARTE0_TX\n";
    let first = run(statements, None);
    assert!(first.is_ok(), "{}", first.render());
    let text = first.pins.file.to_text();
    assert_eq!(PinFile::parse(&text).unwrap(), first.pins.file);
    // Run again with the file the first run wrote: nothing changes, the text is the same.
    let second = run(statements, Some(&text));
    assert!(second.is_ok(), "{}", second.render());
    assert!(!second.pins.changed(), "{:?}", second.pins.changes);
    assert_eq!(second.pins.file.to_text(), text);
    assert_eq!(pins(&first), pins(&second));
    // And another run with no file at all picks the same pins.
    assert_eq!(pins(&run(statements, None)), pins(&first));
    assert!(
        text.starts_with(
            "version = 1\n\n[B.pins.a]\npin = \"p02\"\nfunction = \"ANALOG\"\nsource = \"auto\"\n"
        ),
        "{text}"
    );
}

#[test]
fn a_pin_stays_when_the_design_gains_a_request_that_sorts_first() {
    let first = run("    .m: .u.GPIO\n", None);
    assert_eq!(pins(&first), [pair("m", "p00")]);
    let text = first.pins.file.to_text();
    let second = run("    .a: .u.GPIO\n    .m: .u.GPIO\n", Some(&text));
    assert!(second.is_ok(), "{}", second.render());
    assert_eq!(pins(&second), [pair("a", "p01"), pair("m", "p00")]);
}

#[test]
fn a_locked_entry_that_no_longer_holds_is_an_error_at_its_line() {
    // The source now connects `p02` itself.
    let text = file(&[("status", entry("p02", "GPIO", true))]);
    let c = run(
        "    .status: .u.GPIO\n    .u.p02 ~ .probe.a02\n    .u.p02.function &= GPIO\n",
        Some(&text),
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{}", c.render());
    assert!(m[0].starts_with("`status` is locked to `u.p02` in the pin file, but `u.p02` is held by the source (connected to `probe.a02`, function GPIO chosen at main.nsm:"), "{}", m[0]);
    assert!(
        m[0].ends_with("); change the source, or unlock the entry so that a pin is picked again"),
        "{}",
        m[0]
    );
    let d = &c.diagnostics[0];
    assert_eq!(
        c.sources.place(d.source, d.span).as_deref(),
        Some("main.pins.toml:3")
    );
}

#[test]
fn an_unlocked_entry_that_no_longer_holds_is_a_finding_and_the_pin_is_picked_again() {
    let text = file(&[("status", entry("p02", "GPIO", false))]);
    let c = run(
        "    .status: .u.GPIO\n    .u.p02 ~ .probe.a02\n    .u.p02.function &= GPIO\n",
        Some(&text),
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("status", "p00")]);
    assert_eq!(c.pins.findings.len(), 1);
    assert!(c.pins.findings[0].starts_with("main.pins.toml:3: `status` was `u.p02`, but `u.p02` is held by the source (connected to `probe.a02`, function GPIO chosen at main.nsm:"), "{:?}", c.pins.findings);
    assert_eq!(c.pins.changes, ["B.status: pin `p02` becomes `p00`"]);
}

#[test]
fn an_entry_for_another_function_does_not_hold() {
    let text = file(&[("status", entry("p02", "GPIO", false))]);
    let c = run("    .status: .u.ANALOG\n", Some(&text));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("status", "p02")]);
    assert_eq!(
        c.pins.findings,
        [
            "main.pins.toml:3: `status` was `u.p02`, but the entry is for GPIO and the request asks for ANALOG; a pin is picked again"
        ]
    );
}

#[test]
fn a_stale_entry_is_a_finding_when_unlocked_and_an_error_when_locked() {
    let unlocked = file(&[
        ("gone", entry("p03", "GPIO", false)),
        ("status", entry("p00", "GPIO", false)),
    ]);
    let c = run("    .status: .u.GPIO\n", Some(&unlocked));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        c.pins.findings,
        [
            "main.pins.toml:3: the entry `gone` (pin `p03`) is stale: `B` has no request `gone`; the file drops it"
        ]
    );
    assert_eq!(c.pins.changes, ["B.gone: removed, it was pin `p03`"]);
    assert!(!c.pins.file.board("B").unwrap().contains_key("gone"));

    let locked = file(&[("gone", entry("p03", "GPIO", true))]);
    let c = run("    .status: .u.GPIO\n", Some(&locked));
    assert_eq!(
        messages(&c),
        [
            "`gone` is locked to pin `p03` in the table of `B`, but `B` has no request `gone`; delete the entry to let the file drop it"
        ]
    );
}

#[test]
fn a_table_for_a_board_the_source_lacks_is_stale_too() {
    let text =
        "version = 1\n\n[Other.pins.x]\npin = \"p00\"\nfunction = \"GPIO\"\nsource = \"auto\"\n";
    let c = run("    .status: .u.GPIO\n", Some(text));
    assert_eq!(
        c.pins.findings,
        [
            "main.pins.toml:3: the entry `x` (pin `p00`) is stale: the source has no board or system `Other`; the file drops it"
        ]
    );
}

#[test]
fn no_pin_offers_the_capability() {
    let c = run("    .vbat: .u.PWM\n", None);
    assert_eq!(
        messages(&c),
        [
            "`vbat` asks for PWM on `u` (`Zmcu4`), and no pin of `Zmcu4` offers it; the functions its rows and peripherals offer are GPIO, ANALOG, AIN0, AIN1, UARTE0_TX, UARTE0_RX, UARTE1_TX and UARTE1_RX"
        ]
    );
}

#[test]
fn a_request_names_a_part_that_is_there() {
    let c = run("    .vbat: .v.GPIO\n", None);
    assert_eq!(
        messages(&c),
        [
            "`vbat` asks `v` for GPIO, but `v` is not a part declared here; the parts here are `u` and `probe`"
        ]
    );
    let c = run("    .vbat: .u.p00.GPIO\n", None);
    assert_eq!(messages(&c).len(), 1, "{}", c.render());
}

#[test]
fn every_pin_that_offers_it_is_taken() {
    // Both ANALOG pins are the source's and an entry's.
    let text = file(&[("x", entry("p03", "ANALOG", true))]);
    let c = run(
        "    .vbat: .u.ANALOG\n    .x: .u.ANALOG\n    .u.p02 ~ .probe.a02\n    .u.p02.function &= GPIO\n",
        Some(&text),
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{}", c.render());
    assert!(m[0].starts_with("`vbat` asks for ANALOG on `u`, and every pin that offers it is taken: `u.p02` by the source (connected to `probe.a02`, function GPIO chosen at main.nsm:"), "{}", m[0]);
    assert!(
        m[0].ends_with("), `u.p03` by `x` (locked, main.pins.toml:3)"),
        "{}",
        m[0]
    );
}

#[test]
fn the_message_names_the_move_that_would_free_a_pin() {
    let text = file(&[("x", entry("p03", "ANALOG", false))]);
    let c = run(
        "    .vbat: .u.ANALOG\n    .x: .u.ANALOG\n    .u.p02 ~ .probe.a02\n    .u.p02.function &= GPIO\n",
        Some(&text),
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{}", c.render());
    // `x` is `ANALOG`, which only `p02` and `p03` offer, so it has nowhere to go: no move.
    assert!(!m[0].contains("moving"), "{}", m[0]);

    // A GPIO entry on a pin that the new request needs can move to a free pin.
    let text = file(&[("x", entry("p02", "GPIO", false))]);
    let c = run(
        "    .vbat: .u.ANALOG\n    .x: .u.GPIO\n    .u.p03 ~ .probe.a03\n    .u.p03.function &= GPIO\n",
        Some(&text),
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{}", c.render());
    assert!(
        m[0].ends_with("moving `x` to `u.p00` would free `u.p02`; run with the entry removed from the pin file, or move it there, and run again"),
        "{}",
        m[0]
    );
}

#[test]
fn shared_hardware_limits_the_choice() {
    // UARTE0 is in use through a function the source chose, so UARTE1 cannot be asked for.
    let c = run(
        "    .u.p00 ~ .probe.a00\n    .u.p00.function &= UARTE0_TX\n    .tx: .u.UARTE1_TX\n",
        None,
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{}", c.render());
    assert!(
        m[0].starts_with("`tx` asks for UARTE1_TX on `u`, and no free pin can serve it: `UARTS` allows 1 of UARTE0 and UARTE1 at a time: UARTE0 is used on `u.p00`"),
        "{}",
        m[0]
    );
}

#[test]
fn two_requests_for_one_unit_of_a_pair_go_to_the_same_unit_only_if_the_limit_allows() {
    // Two lines of one peripheral are one use: both fit.
    let c = run("    .tx: .u.UARTE0_TX\n    .rx: .u.UARTE0_RX\n", None);
    assert!(c.is_ok(), "{}", c.render());
    // Two peripherals are two uses: the second is refused.
    let c = run("    .tx: .u.UARTE0_TX\n    .tx2: .u.UARTE1_TX\n", None);
    assert_eq!(messages(&c).len(), 1, "{}", c.render());
    assert!(messages(&c)[0].starts_with("`tx2` asks for UARTE1_TX on `u`"));
}

#[test]
fn more_requests_than_pins_say_who_got_them() {
    let c = run(
        "    .a: .u.ANALOG\n    .b: .u.ANALOG\n    .c: .u.ANALOG\n",
        None,
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{}", c.render());
    assert!(m.contains(&"`c` asks for ANALOG on `u`, and every pin that offers it is taken: `u.p02` by `a` (new), `u.p03` by `b` (new)".to_string()), "{m:?}");
    // ANALOG is on two pins, so the two requests that were served share nothing: the check has
    // one message, for the request that has no pin left.
}

#[test]
fn requests_for_a_function_that_several_pins_offer_each_get_a_pin() {
    let statements =
        "    .a: .u.ANALOG\n    .b: .u.ANALOG\n    .a ~ .probe.a00\n    .b ~ .probe.a01\n";
    let c = run(statements, None);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("a", "p02"), pair("b", "p03")]);
    let model = &c.board("B").unwrap().model;
    for a in &model.pin_assignments {
        assert_eq!(model.function_of(a.pin).as_deref(), Some("ANALOG"));
    }
    // The pin file records the function that was asked for, one entry for each request.
    let text = c.pins.file.to_text();
    assert!(
        text.contains("[B.pins.a]\npin = \"p02\"\nfunction = \"ANALOG\""),
        "{text}"
    );
    assert!(
        text.contains("[B.pins.b]\npin = \"p03\"\nfunction = \"ANALOG\""),
        "{text}"
    );
    // The same source gives the same answer, and the file that holds is read back unchanged.
    let again = run(statements, Some(&text));
    assert!(again.is_ok(), "{}", again.render());
    assert_eq!(pins(&again), pins(&c));
    assert!(!again.pins.changed(), "{:?}", again.pins.changes);
    assert_eq!(again.pins.file.to_text(), text);
    // Locked entries (the locked path) keep each request on its own pin, even in the other order.
    let locked = file(&[
        ("a", entry("p03", "ANALOG", true)),
        ("b", entry("p02", "ANALOG", true)),
    ]);
    let kept = run(statements, Some(&locked));
    assert!(kept.is_ok(), "{}", kept.render());
    assert_eq!(pins(&kept), [pair("a", "p03"), pair("b", "p02")]);
    assert!(!kept.pins.changed());
}

#[test]
fn two_requests_for_a_line_of_a_peripheral_are_still_refused() {
    // A line of a peripheral is on one pin at a time, though every GPIO could carry it.
    let c = run("    .a: .u.UARTE1_TX\n    .b: .u.UARTE1_TX\n", None);
    assert_eq!(
        messages(&c),
        [
            "function `UARTE1_TX` is chosen on both `u.p00` and `u.p01`; the requests `a` and `b` ask for a line of a peripheral, which one pin has"
        ]
    );
}

#[test]
fn a_request_goes_round_a_pin_the_source_chose_for_the_same_function() {
    // `p02` is the source's ANALOG; the request gets the other pin and the check has no fault.
    let c = run(
        "    .u.p02 ~ .probe.a00\n    .u.p02.function &= ANALOG\n    .a: .u.ANALOG\n    .a ~ .probe.a01\n",
        None,
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("a", "p03")]);
}

#[test]
fn two_requests_for_a_function_that_one_pin_offers_are_still_refused() {
    // AIN0 is listed on `p02` only: the second request has no pin left.
    let c = run("    .a: .u.AIN0\n    .b: .u.AIN0\n", None);
    assert_eq!(
        messages(&c),
        ["`b` asks for AIN0 on `u`, and every pin that offers it is taken: `u.p02` by `a` (new)"]
    );
}

/// Zuart, an invented part whose rows list one line of a UART (`UART1_TX`) on two pins by hand:
/// no peripheral lists it and no row marks it a `class`.
const ZUART: &str = r#"
part Zuart {
    pins {
        .p00 bidirectional "P0.00" "1" fn GPIO UART1_TX:output
        .p01 bidirectional "P0.01" "2" fn GPIO UART1_TX:output
    }
}
"#;

#[test]
fn requests_for_a_row_listed_line_that_no_peripheral_lists_each_get_a_pin() {
    // Pins today's behaviour (an open question for the owner, in the pull request that added
    // this test): the exemption of requests applies to every function that is not a line of a
    // peripheral, and `UART1_TX` here is only listed by rows. Two requests for it are accepted
    // and each gets its own pin, as two for `ANALOG` do; two pins the source chooses it on by
    // hand are refused.
    let text = |statements: &str| format!("{ZUART}\nboard B {{\n    .u: Zuart\n{statements}\n}}\n");
    let c = check(
        "main.nsm",
        &text("    .a: .u.UART1_TX\n    .b: .u.UART1_TX\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("a", "p00"), pair("b", "p01")]);
    // By hand it is refused, as it was.
    let by_hand = check(
        "main.nsm",
        &text("    .u.p00.function &= UART1_TX\n    .u.p01.function &= UART1_TX\n"),
        &Loader::std(),
    );
    assert!(
        messages(&by_hand)
            .iter()
            .any(|m| m.contains("function `UART1_TX` is chosen on both `u.p00` and `u.p01`")),
        "{}",
        by_hand.render()
    );
}

#[test]
fn a_direction_that_disagrees_with_the_net_is_refused() {
    // Two transmitters on one net: the usual rule for two outputs, on the result.
    let c = run(
        "    .a: .u.UARTE0_TX\n    .b: .u.UARTE0_TX\n    .a ~ .probe.a00\n    .b ~ .probe.a00\n",
        None,
    );
    assert!(
        messages(&c).iter().any(|m| m.contains("has two outputs")),
        "{}",
        c.render()
    );
}

#[test]
fn a_pin_of_a_part_with_a_package_needs_the_package() {
    let part = r#"
part Zpkg {
    pins "A" { .p00 bidirectional "P0" "1" fn GPIO }
    pins "B" { .p00 bidirectional "P0" "A1" fn GPIO }
    .package: Package
}
board B {
    .u: Zpkg
    .status: .u.GPIO
}
"#;
    let c = check("main.nsm", part, &Loader::std());
    assert_eq!(
        messages(&c),
        [
            "`status` asks `u` for GPIO, but the pins of `Zpkg` depend on its package, which the board does not state; state it (`.u.package &= ...`), one of `A` and `B`"
        ]
    );
}

#[test]
fn a_request_is_not_made_in_a_part() {
    let part = "part P {\n    .x: .y.GPIO\n}\n";
    let c = check("main.nsm", part, &Loader::std());
    assert_eq!(messages(&c).len(), 1, "{}", c.render());
    assert!(messages(&c)[0].starts_with("a pin request goes in a board or a module"));
}

#[test]
fn a_request_in_a_module_is_keyed_by_its_path() {
    let src = format!(
        "{ZMCU4}\nmodule Hub {{\n    .u: Zmcu4\n    .scl: .u.GPIO\n}}\nboard B {{\n    .hub: Hub\n}}\n"
    );
    let c = check("main.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        c.pins.file.board("B").unwrap().keys().collect::<Vec<_>>(),
        ["hub.scl"]
    );
    assert!(c.pins.file.to_text().contains("[B.pins.\"hub.scl\"]"));
    // The module read on its own writes no table.
    assert!(c.pins.file.board("Hub").is_none());
}

#[test]
fn the_pin_file_says_what_is_wrong_with_it() {
    let bad = "version = 2\n\n[B.pins.x]\npin = \"p00\"\nfunction = \"GPIO\"\ncolour = \"red\"\n";
    let errors = PinFile::parse(bad).unwrap_err();
    let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "`version` is a whole number, and this reads only version 1",
            "unknown key `colour` in `B.pins.x`; the keys are `pin`, `function`, `lock` and `source`",
        ]
    );
    // Through the checker: at the file's line.
    let c = run("    .status: .u.GPIO\n", Some(bad));
    assert_eq!(c.diagnostics.len(), 2, "{}", c.render());
    let d = &c.diagnostics[0];
    assert_eq!(
        c.sources.place(d.source, d.span).as_deref(),
        Some("main.pins.toml:1")
    );
}

#[test]
fn the_pin_space_says_which_pins_each_request_may_have_and_what_a_choice_must_keep() {
    // `.tx` asks for UARTE0's line, `.rx` for UARTE1's: the part lets one of the two be in use.
    let c = run(
        "    .tx: .u.UARTE0_TX\n    .rx: .u.UARTE0_RX\n    .a: .u.GPIO\n    .a ~ .probe.a00\n    .tx ~ .probe.a01\n    .rx ~ .probe.a02\n    .u.p03.function &= AIN1\n",
        None,
    );
    assert!(c.is_ok(), "{}", c.render());
    let space = &c.pins.spaces["B"];
    assert_eq!(space.len(), 1);
    let part = &space[0];
    assert_eq!(part.path, "u");
    assert_eq!(
        part.requests
            .iter()
            .map(|r| r.key.as_str())
            .collect::<Vec<_>>(),
        ["a", "rx", "tx"],
        "in the byte order of the requests"
    );
    // The pin the source chose a function on is the source's: no request may have it.
    assert!(part.taken.contains("p03"));
    let a = part.requests.iter().find(|r| r.key == "a").unwrap();
    assert_eq!(a.offers, ["p00", "p01", "p02"]);
    // Any distinct pins that offer the functions are allowed ...
    assert!(part.allows(&[("a", "p02"), ("tx", "p00"), ("rx", "p01")]));
    // ... but not a pin twice, a pin the source holds, or a pin that does not offer it.
    assert!(!part.allows(&[("a", "p00"), ("tx", "p00"), ("rx", "p01")]));
    assert!(!part.allows(&[("a", "p03"), ("tx", "p00"), ("rx", "p01")]));
    assert!(!part.allows(&[("a", "p00"), ("tx", "p01"), ("rx", "p09")]));
}

#[test]
fn a_pin_space_keeps_the_shared_limits() {
    // Two pins of the part that may not both be in use: a choice that uses both is not allowed.
    let text = r#"
part Zpair {
    peripheral UARTE0 on GPIO { UARTE0_TX:output UARTE0_RX:input }
    shared PAIR limit 1 { .p00 .p01 }
    pins {
        .p00 bidirectional "P0.00" "1" fn GPIO
        .p01 bidirectional "P0.01" "2" fn GPIO
        .p02 bidirectional "P0.02" "3" fn GPIO
    }
}

part Probe {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
}

board B {
    .u: Zpair
    .probe: Probe
    .x: .u.GPIO
    .y: .u.GPIO
    .x ~ .probe.a
    .y ~ .probe.b
}
"#;
    let c = check("main.nsm", text, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let part = &c.pins.spaces["B"][0];
    assert!(part.allows(&[("x", "p00"), ("y", "p02")]));
    assert!(part.allows(&[("x", "p02"), ("y", "p01")]));
    assert!(!part.allows(&[("x", "p00"), ("y", "p01")]));
}

#[test]
fn a_locked_entry_wins_the_pin_over_an_unlocked_entry_with_an_earlier_key() {
    // `a` sorts before `b` and holds `p02` without a lock; the user then locks `b` to `p02`.
    // The lock wins: `b` keeps `p02` and `a` is told and picked again.
    let text = file(&[
        ("a", entry("p02", "GPIO", false)),
        ("b", entry("p02", "GPIO", true)),
    ]);
    let c = run(
        "    .a: .u.GPIO\n    .b: .u.GPIO\n    .a ~ .probe.a00\n    .b ~ .probe.a01\n",
        Some(&text),
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(pins(&c), [pair("a", "p00"), pair("b", "p02")]);
    assert_eq!(c.pins.findings.len(), 1, "{:?}", c.pins.findings);
    assert!(
        c.pins.findings[0].contains("`a` was `u.p02`, but `u.p02` is held by `b` (locked"),
        "{:?}",
        c.pins.findings
    );
    assert_eq!(c.pins.changes, ["B.a: pin `p02` becomes `p00`"]);
    // The locked entry is as the file has it.
    let b = &c.pins.file.board("B").unwrap()["b"];
    assert!(b.lock);
    assert_eq!(b.pin, "p02");
}

#[test]
fn two_locked_entries_on_one_pin_still_go_in_key_order() {
    let text = file(&[
        ("a", entry("p02", "GPIO", true)),
        ("b", entry("p02", "GPIO", true)),
    ]);
    let c = run(
        "    .a: .u.GPIO\n    .b: .u.GPIO\n    .a ~ .probe.a00\n    .b ~ .probe.a01\n",
        Some(&text),
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{}", c.render());
    assert!(
        m[0].starts_with(
            "`b` is locked to `u.p02` in the pin file, but `u.p02` is held by `a` (locked"
        ),
        "{}",
        m[0]
    );
}

#[test]
fn a_request_that_gets_no_pin_is_listed_with_the_message_of_the_check() {
    // `nope` asks for a function nothing offers; `ghost` names a part that is not there;
    // `ok` gets its pin. The two that failed are listed, in the byte order of their paths, each
    // with the words the check reported at it, and the part they asked for as written.
    let c = run(
        "    .ok: .u.GPIO\n    .nope: .u.GPIOO\n    .ghost: .mcu.GPIO\n    .ok ~ .probe.a00\n    .nope ~ .probe.a01\n    .ghost ~ .probe.a02\n",
        None,
    );
    let unserved = &c.pins.unserved["B"];
    let keys: Vec<(&str, &str, &str)> = unserved
        .iter()
        .map(|u| (u.key.as_str(), u.part.as_str(), u.capability.as_str()))
        .collect();
    assert_eq!(
        keys,
        [("ghost", "mcu", "GPIO"), ("nope", "u", "GPIOO")],
        "{unserved:?}"
    );
    let said = messages(&c);
    for u in unserved {
        assert!(
            !u.message.is_empty() && said.contains(&u.message),
            "{} says `{}`, the check said {said:?}",
            u.key,
            u.message
        );
    }
    assert!(
        unserved[1]
            .message
            .contains("`nope` asks for GPIOO on `u` (`Zmcu4`), and no pin of `Zmcu4` offers it")
    );
    assert!(
        unserved[0]
            .message
            .contains("`mcu` is not a part declared here")
    );
    // The request that got a pin is not listed, and a board whose requests all got one lists none.
    assert!(unserved.iter().all(|u| u.key != "ok"));
    let all_served = run("    .ok: .u.GPIO\n    .ok ~ .probe.a00\n", None);
    assert!(all_served.is_ok(), "{:?}", messages(&all_served));
    assert!(all_served.pins.unserved.is_empty());
}

#[test]
fn a_request_in_a_module_names_its_part_by_the_path_from_the_board() {
    let source = format!(
        "{ZMCU4}\nmodule Hub {{\n    .u: Zmcu4\n    .scl: .u.GPIO\n    .sda: .u.GPIOO\n    .scl ~ .u.p00\n    .sda ~ .u.p01\n}}\nboard B {{\n    .hub: Hub\n}}\n"
    );
    let c = check("main.nsm", &source, &Loader::std());
    let unserved = c
        .pins
        .unserved
        .get("B")
        .map(Vec::as_slice)
        .unwrap_or_default();
    assert!(
        unserved
            .iter()
            .any(|u| u.key == "hub.sda" && u.part == "hub.u"),
        "{unserved:?} {:?}",
        messages(&c)
    );
}
