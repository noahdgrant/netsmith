//! `peripheral` and `shared` in a part (ADR 0062): which functions belong together, which
//! hardware they share, and what the checker says to a board that uses two peripherals of
//! one shared resource.
//!
//! `Zmcu` is invented for these tests: a part with four serial units, each of which runs as
//! a UART, an SPI master or an I2C master, one at a time. It is not a real part and is not in
//! the standard library.

use netsmith_check::*;

/// The peripherals of `Zmcu`: four serial units, each of which runs as a UART, an SPI master
/// or an I2C master.
const PERIPHERALS: &str = r#"
    peripheral UARTE0 { UARTE0_TX:output UARTE0_RX:input }
    peripheral SPIM0 { SPIM0_SCK:output SPIM0_MOSI:output }
    peripheral TWIM0 { TWIM0_SCL:bidirectional TWIM0_SDA:bidirectional }
    peripheral UARTE1 { UARTE1_TX:output UARTE1_RX:input }
    peripheral SPIM1 { SPIM1_SCK:output SPIM1_MOSI:output }
    peripheral TWIM1 { TWIM1_SCL:bidirectional TWIM1_SDA:bidirectional }
    peripheral UARTE2 { UARTE2_TX:output UARTE2_RX:input }
    peripheral SPIM2 { SPIM2_SCK:output SPIM2_MOSI:output }
    peripheral TWIM2 { TWIM2_SCL:bidirectional TWIM2_SDA:bidirectional }
    peripheral UARTE3 { UARTE3_TX:output UARTE3_RX:input }
    peripheral SPIM3 { SPIM3_SCK:output SPIM3_MOSI:output }
    peripheral TWIM3 { TWIM3_SCL:bidirectional TWIM3_SDA:bidirectional }
"#;

const SHARES: &str = r#"
    shared SERIAL0 { UARTE0 SPIM0 TWIM0 }
    shared SERIAL1 { UARTE1 SPIM1 TWIM1 }
    shared SERIAL2 { UARTE2 SPIM2 TWIM2 }
    shared SERIAL3 { UARTE3 SPIM3 TWIM3 }
"#;

/// Every function of the peripherals, as a pin row lists them.
fn functions() -> String {
    PERIPHERALS
        .split_whitespace()
        .filter(|w| w.contains('_'))
        .map(|w| w.split(':').next().unwrap())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A part with ten pins that each list every function (a part whose peripherals are on
/// any pin writes that once; ADR 0062 stage 2), the peripherals and the shared resources.
fn part_with(peripherals: &str, shares: &str) -> String {
    let rows: String = (0..10)
        .map(|n| {
            format!(
                "        .p{n:02} bidirectional \"P0.{n:02}\" \"{}\" fn GPIO {}\n",
                n + 1,
                functions()
            )
        })
        .collect();
    format!("part Zmcu {{{peripherals}{shares}\n    pins {{\n{rows}    }}\n}}\n{PROBE}")
}

/// Something with a pin for each of the part's, to put a net on.
const PROBE: &str = r#"
part Probe {
    pins {
        .a0 passive "A0" "1"
        .a1 passive "A1" "2"
        .a2 passive "A2" "3"
        .a3 passive "A3" "4"
        .a4 passive "A4" "5"
        .a5 passive "A5" "6"
        .a6 passive "A6" "7"
        .a7 passive "A7" "8"
        .a8 passive "A8" "9"
        .a9 passive "A9" "10"
    }
}
"#;

fn zmcu() -> String {
    part_with(PERIPHERALS, SHARES)
}

/// A board with a `Zmcu` and a probe, and the statements that connect and choose.
fn run(statements: &str) -> Checked {
    run_with(&part_with(PERIPHERALS, SHARES), statements)
}

fn run_with(part: &str, statements: &str) -> Checked {
    check(
        "main.nsm",
        &format!("{part}\nboard B {{\n    .u: Zmcu\n    .probe: Probe\n{statements}\n}}\n"),
        &Loader::std(),
    )
}

/// `pin` of the part on `probe` pin `n`, chosen as `function`: two statements.
fn choose(pin: u32, function: &str) -> String {
    format!("    .u.p{pin:02} ~ .probe.a{pin}\n    .u.p{pin:02}.function &= \"{function}\"\n")
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn only(c: &Checked, message: &str) {
    assert_eq!(messages(c), [message], "{}", c.render());
}

/// The line of the source where the diagnostic points, trimmed.
fn line_of(c: &Checked, i: usize) -> String {
    let d = &c.diagnostics[i];
    let (line, _) = d.line_col(&c.sources);
    c.sources
        .get(d.source)
        .text
        .lines()
        .nth(line - 1)
        .unwrap()
        .trim()
        .to_string()
}

/// A console on unit 0, a display on unit 1, a sensor on unit 2 and an EEPROM on unit 3.
fn four_interfaces() -> String {
    [
        choose(0, "UARTE0_TX"),
        choose(1, "UARTE0_RX"),
        choose(2, "SPIM1_SCK"),
        choose(3, "SPIM1_MOSI"),
        choose(4, "TWIM2_SCL"),
        choose(5, "TWIM2_SDA"),
        choose(6, "TWIM3_SCL"),
        choose(7, "TWIM3_SDA"),
    ]
    .concat()
}

#[test]
fn one_peripheral_on_each_unit_is_fine() {
    let c = run(&four_interfaces());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_fifth_interface_on_four_units_names_both_uses_and_says_none_is_free() {
    // A second console on unit 1, which already runs the display.
    let c = run(&format!(
        "{}{}{}",
        four_interfaces(),
        choose(8, "UARTE1_TX"),
        choose(9, "UARTE1_RX")
    ));
    only(
        &c,
        "`u` (`Zmcu`) uses SPIM1 and UARTE1, which share `SERIAL1` and run one at a time: SPIM1 on `u.p02` as SPIM1_SCK (main.nsm:57) and on `u.p03` as SPIM1_MOSI (main.nsm:59), UARTE1 on `u.p08` as UARTE1_TX (main.nsm:69) and on `u.p09` as UARTE1_RX (main.nsm:71); the other shared resources of `Zmcu` are in use as well, `SERIAL0` by UARTE0, `SERIAL2` by TWIM2 and `SERIAL3` by TWIM3, so none is free",
    );
    // The error is at the statement of the use that goes over, the second in path order.
    assert_eq!(line_of(&c, 0), ".u.p08.function &= \"UARTE1_TX\"");
}

#[test]
fn the_message_names_the_shared_resources_that_have_nothing_in_use() {
    // Units 2 and 3 are free: the author can move one of the two uses there.
    let c = run(&format!(
        "{}{}{}",
        choose(0, "UARTE0_TX"),
        choose(2, "SPIM1_SCK"),
        choose(8, "UARTE1_TX"),
    ));
    only(
        &c,
        "`u` (`Zmcu`) uses SPIM1 and UARTE1, which share `SERIAL1` and run one at a time: SPIM1 on `u.p02` as SPIM1_SCK (main.nsm:55), UARTE1 on `u.p08` as UARTE1_TX (main.nsm:57); the other shared resources of `Zmcu`: `SERIAL0` by UARTE0, `SERIAL2` has nothing in use and `SERIAL3` has nothing in use",
    );
}

#[test]
fn two_functions_of_one_peripheral_are_one_use() {
    let c = run(&format!(
        "{}{}{}",
        choose(0, "UARTE0_TX"),
        choose(1, "UARTE0_RX"),
        choose(2, "SPIM1_SCK")
    ));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_peripheral_in_two_shared_resources_is_judged_in_both() {
    let part = zmcu().replace(
        "    shared SERIAL0 {",
        "    shared PAIR { UARTE0 UARTE1 }\n    shared SERIAL0 {",
    );
    let c = run_with(
        &part,
        &format!("{}{}", choose(0, "UARTE0_TX"), choose(8, "UARTE1_TX")),
    );
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{}", c.render());
    assert!(
        msgs[0].starts_with(
            "`u` (`Zmcu`) uses UARTE0 and UARTE1, which share `PAIR` and run one at a time: "
        ),
        "{msgs:#?}"
    );
}

#[test]
fn a_function_that_no_peripheral_lists_is_not_a_use() {
    // GPIO is on every row and belongs to no peripheral.
    let c = run(&format!("{}{}", choose(0, "UARTE0_TX"), choose(2, "GPIO")));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_function_chosen_on_an_unconnected_pin_is_a_use() {
    let c = run(&format!(
        "{}    .u.p02.function &= \"SPIM0_SCK\"\n",
        choose(0, "UARTE0_TX")
    ));
    only(
        &c,
        "`u` (`Zmcu`) uses UARTE0 and SPIM0, which share `SERIAL0` and run one at a time: UARTE0 on `u.p00` as UARTE0_TX (main.nsm:53), SPIM0 on `u.p02` as SPIM0_SCK (main.nsm:54); the other shared resources of `Zmcu`: `SERIAL1` has nothing in use, `SERIAL2` has nothing in use and `SERIAL3` has nothing in use",
    );
}

#[test]
fn a_function_a_peripheral_lists_takes_its_kind_from_the_peripheral() {
    // UARTE0_TX is an output of the peripheral, though its rows write no kind: two of them
    // on one net are two outputs.
    let c = run(
        "    .u.p00 ~ .u.p08\n    .u.p00.function &= \"UARTE0_TX\"\n    .u.p08.function &= \"UARTE3_TX\"\n",
    );
    only(
        &c,
        "net `net-u.p00` has two outputs: `u.p00` (`Zmcu` function `UARTE0_TX`, an output), `u.p08` (`Zmcu` function `UARTE3_TX`, an output)",
    );
}

#[test]
fn a_part_in_a_module_is_judged_with_its_path() {
    let src = format!(
        "{}
module Hub {{
    .mcu: Zmcu
    .probe: Probe
    .mcu.p00 ~ .probe.a0
    .mcu.p08 ~ .probe.a8
    .mcu.p00.function &= \"UARTE0_TX\"
    .mcu.p08.function &= \"UARTE0_RX\"
    .mcu.p02 ~ .probe.a2
    .mcu.p02.function &= \"SPIM0_SCK\"
}}
board B {{
    .hub: Hub
}}
",
        zmcu()
    );
    let c = check("main.nsm", &src, &Loader::std());
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{}", c.render());
    assert!(
        msgs[0].starts_with(
            "`hub.mcu` (`Zmcu`) uses UARTE0 and SPIM0, which share `SERIAL0` and run one at a time: UARTE0 on `hub.mcu.p00` as UARTE0_TX"
        ),
        "{msgs:#?}"
    );
}

#[test]
fn the_order_of_the_statements_does_not_matter() {
    // The same part with its statements the other way round: the shared resources first,
    // then the table, then the peripherals.
    const FORWARD: &str = "
part Zmcu {
    peripheral UARTE0 { UARTE0_TX:output }
    peripheral SPIM0 { SPIM0_SCK:output }
    shared SERIAL0 { UARTE0 SPIM0 }
    pins {
        .p00 bidirectional \"P0.00\" \"1\" fn UARTE0_TX
        .p02 bidirectional \"P0.02\" \"3\" fn SPIM0_SCK
    }
}
part Probe {
    pins {
        .a0 passive \"A0\" \"1\"
        .a2 passive \"A2\" \"3\"
    }
}
";
    const BACKWARD: &str = "
part Zmcu {
    shared SERIAL0 { SPIM0 UARTE0 }
    pins {
        .p02 bidirectional \"P0.02\" \"3\" fn SPIM0_SCK
        .p00 bidirectional \"P0.00\" \"1\" fn UARTE0_TX
    }
    peripheral SPIM0 { SPIM0_SCK:output }
    peripheral UARTE0 { UARTE0_TX:output }
}
part Probe {
    pins {
        .a2 passive \"A2\" \"3\"
        .a0 passive \"A0\" \"1\"
    }
}
";
    let statements = format!("{}{}", choose(0, "UARTE0_TX"), choose(2, "SPIM0_SCK"));
    let a = run_with(FORWARD, &statements);
    let b = run_with(BACKWARD, &statements);
    let message =
        "`u` (`Zmcu`) uses UARTE0 and SPIM0, which share `SERIAL0` and run one at a time: ";
    for c in [&a, &b] {
        let msgs = messages(c);
        assert_eq!(msgs.len(), 1, "{}", c.render());
        assert!(msgs[0].starts_with(message), "{msgs:#?}");
    }
    // The use that is over is the same in both: the second in path order.
    assert_eq!(line_of(&a, 0), line_of(&b, 0));
}

// ---- The mistakes of a definition ----

#[test]
fn a_member_that_is_not_a_peripheral_names_the_nearest() {
    let part = zmcu().replace(
        "    shared SERIAL1 { UARTE1 SPIM1 TWIM1 }",
        "    shared SERIAL1 { UARTE1 SPIM9 TWIM1 }",
    );
    let c = run_with(&part, "");
    only(
        &c,
        "`SPIM9` in `shared SERIAL1` is not a peripheral, a function or a pin of `Zmcu`; did you mean `SPIM0`? the peripherals are UARTE0, SPIM0, TWIM0, UARTE1, SPIM1, TWIM1, ...",
    );
    assert_eq!(line_of(&c, 0), "shared SERIAL1 { UARTE1 SPIM9 TWIM1 }");
}

#[test]
fn a_part_with_no_peripherals_says_so_to_a_member() {
    let c = check(
        "main.nsm",
        "part P {\n    shared S { UARTE0 }\n    pins { .a passive \"A\" \"1\" }\n}\n",
        &Loader::std(),
    );
    only(
        &c,
        "`UARTE0` in `shared S` is not a peripheral, a function or a pin of `P`; `P` has no peripherals, so declare one with `peripheral NAME { FUNCTION ... }`",
    );
}

#[test]
fn a_function_that_no_row_lists_is_on_no_pin() {
    let part = zmcu().replace(
        "peripheral TWIM3 { TWIM3_SCL:bidirectional TWIM3_SDA:bidirectional }",
        "peripheral TWIM3 { TWIM3_SCL:bidirectional TWIM3_SDA:bidirectional TWIM3_SMBA:input }",
    );
    let c = run_with(&part, "");
    only(
        &c,
        "`peripheral TWIM3` lists `TWIM3_SMBA`, but no row of any `pins` table of `Zmcu` lists it, so no pin can be it; list it as `fn TWIM3_SMBA` on the pins that carry it",
    );
}

#[test]
fn a_function_belongs_to_one_peripheral() {
    let part = zmcu().replace(
        "peripheral TWIM3 { TWIM3_SCL:bidirectional TWIM3_SDA:bidirectional }",
        "peripheral TWIM3 { TWIM3_SCL:bidirectional TWIM3_SDA:bidirectional UARTE0_TX }",
    );
    let c = run_with(&part, "");
    only(
        &c,
        "function `UARTE0_TX` is in `peripheral UARTE0` and in `peripheral TWIM3`; a function belongs to one peripheral",
    );
}

#[test]
fn a_row_may_not_write_a_kind_for_a_function_its_peripheral_gives_one() {
    let part = zmcu().replace(
        "\"P0.00\" \"1\" fn GPIO UARTE0_TX",
        "\"P0.00\" \"1\" fn GPIO UARTE0_TX:output",
    );
    let c = run_with(&part, "");
    only(
        &c,
        "`.p00` lists `UARTE0_TX:output`, a function of `peripheral UARTE0`, which gives it its kind; write `UARTE0_TX` without a kind",
    );
}

#[test]
fn names_are_declared_once() {
    let part = zmcu()
        .replace(
            "    shared SERIAL3 { UARTE3 SPIM3 TWIM3 }",
            "    shared SERIAL3 { UARTE3 SPIM3 TWIM3 UARTE3 }\n    shared SERIAL3 { UARTE3 }\n    peripheral SPIM3 { SPIM3_NSS }",
        )
        .replace("\"P0.00\" \"1\" fn GPIO", "\"P0.00\" \"1\" fn GPIO SPIM3");
    let c = run_with(&part, "");
    let msgs = messages(&c);
    assert!(
        msgs.contains(&"`SPIM3` in `shared SERIAL3` is listed twice".to_string())
            || msgs.contains(&"`UARTE3` is listed twice in `shared SERIAL3`".to_string()),
        "{msgs:#?}"
    );
    assert!(
        msgs.contains(
            &"`shared SERIAL3` is declared twice in `Zmcu`; one statement lists all of its members"
                .to_string()
        ),
        "{msgs:#?}"
    );
    assert!(
        msgs.contains(&"`peripheral SPIM3` is declared twice in `Zmcu`; one statement lists all of its functions".to_string()),
        "{msgs:#?}"
    );
    assert!(
        msgs.iter().any(|m| m.starts_with(
            "`SPIM3` is a peripheral of `Zmcu` and also a function listed by `.p00`;"
        )),
        "{msgs:#?}"
    );
}

#[test]
fn a_peripheral_and_a_function_have_different_names() {
    let part = zmcu().replace(
        "peripheral TWIM3 { TWIM3_SCL:bidirectional TWIM3_SDA:bidirectional }",
        "peripheral TWIM3 { TWIM3_SCL:bidirectional TWIM3_SDA:bidirectional }\n    peripheral GPIO { GPIO_ALT }",
    );
    let c = run_with(&part, "");
    let msgs = messages(&c);
    assert!(
        msgs.contains(&"`GPIO` is a peripheral of `Zmcu` and also a function listed by `.p00`, `.p01`, `.p02`, `.p03`, `.p04`, `.p05`, `.p06`, `.p07`, `.p08` and `.p09`; a peripheral and a function have different names, so give one of them another".to_string()),
        "{msgs:#?}"
    );
}

#[test]
fn only_a_part_holds_a_peripheral_or_a_shared_resource() {
    for (holder, what, statement) in [
        ("module", "peripheral", "peripheral A { A_TX }"),
        ("module", "shared", "shared S { A }"),
        ("board", "peripheral", "peripheral A { A_TX }"),
        ("interface", "shared", "shared S { A }"),
    ] {
        let src = format!("{holder} X {{\n    {statement}\n}}\n");
        let c = check("main.nsm", &src, &Loader::std());
        only(
            &c,
            &format!("`{what}` can only appear directly inside a part"),
        );
    }
}

#[test]
fn a_generic_refuses_them_and_a_provider_does_not_take_them_in() {
    let src = "\
generic Mcu {
    pins { .vdd passive }
    peripheral UART { UART_TX:output }
    shared SERIAL { UART }
}
part Zmcu provides Mcu {
    pins { .vdd passive \"VDD\" \"1\" fn UART_TX }
}
";
    let c = check("main.nsm", src, &Loader::std());
    let msgs = messages(&c);
    assert_eq!(
        msgs,
        [
            "a generic states what every provider has; a peripheral is a fact of one part's datasheet, so `peripheral` goes in a part that provides it",
            "a generic states what every provider has; a shared resource is a fact of one part's datasheet, so `shared` goes in a part that provides it",
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_provider_may_state_its_own() {
    let src = "\
generic Mcu {
    pins { .vdd passive }
}
part Zmcu provides Mcu {
    peripheral UART0 { UART0_TX:output }
    peripheral SPI0 { SPI0_SCK:output }
    shared UNIT0 { UART0 SPI0 }
    pins {
        .vdd passive \"VDD\" \"1\"
        .p0 bidirectional \"P0\" \"2\" fn UART0_TX SPI0_SCK
        .p1 bidirectional \"P1\" \"3\" fn UART0_TX SPI0_SCK
    }
}
part Probe {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
}
board B {
    .u: Zmcu
    .probe: Probe
    .u.p0 ~ .probe.a
    .u.p1 ~ .probe.b
    .u.p0.function &= \"UART0_TX\"
    .u.p1.function &= \"SPI0_SCK\"
}
";
    let c = check("main.nsm", src, &Loader::std());
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{}", c.render());
    assert!(
        msgs[0].starts_with(
            "`u` (`Zmcu`) uses UART0 and SPI0, which share `UNIT0` and run one at a time"
        ),
        "{msgs:#?}"
    );
}
