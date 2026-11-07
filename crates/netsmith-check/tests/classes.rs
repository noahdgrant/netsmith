//! `peripheral NAME on CLASS { ... }` (ADR 0062, stage 2): a peripheral whose lines go on any
//! pin of a class, so that the pin table has one row for each pin and not one for each pin and
//! each peripheral. A core that owns a pin is a class too.
//!
//! `Zmcu` is invented for these tests: two cores, four serial units and a high-speed SPI on
//! dedicated pins. It is not a real part and is not in the standard library.

use netsmith_check::*;

/// The part of the ADR's example, without a supply: twelve pins, whose rows list the classes of
/// the cores that may own them, and the four pins of the high-speed SPI list its functions.
const ZMCU: &str = r#"
part Zmcu {
    peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }
    peripheral SPIM0  on APP_GPIO { SPIM0_SCK:output SPIM0_MOSI:output SPIM0_MISO:input }
    peripheral TWIM0  on APP_GPIO { TWIM0_SCL:bidirectional TWIM0_SDA:bidirectional }
    peripheral UARTE1 on APP_GPIO { UARTE1_TX:output UARTE1_RX:input }
    peripheral SPIM1  on APP_GPIO { SPIM1_SCK:output SPIM1_MOSI:output SPIM1_MISO:input }
    peripheral TWIM1  on APP_GPIO { TWIM1_SCL:bidirectional TWIM1_SDA:bidirectional }
    peripheral UARTE2 on APP_GPIO { UARTE2_TX:output UARTE2_RX:input }
    peripheral SPIM2  on APP_GPIO { SPIM2_SCK:output SPIM2_MOSI:output SPIM2_MISO:input }
    peripheral TWIM2  on APP_GPIO { TWIM2_SCL:bidirectional TWIM2_SDA:bidirectional }
    peripheral UARTE3 on APP_GPIO { UARTE3_TX:output UARTE3_RX:input }
    peripheral SPIM3  on APP_GPIO { SPIM3_SCK:output SPIM3_MOSI:output SPIM3_MISO:input }
    peripheral TWIM3  on APP_GPIO { TWIM3_SCL:bidirectional TWIM3_SDA:bidirectional }
    peripheral NETUART on NET_GPIO { NETUART_TX:output NETUART_RX:input }
    peripheral HSSPI { HSSPI_SCK:output HSSPI_MOSI:output HSSPI_MISO:input HSSPI_CSN:output }

    shared SERIAL0 { UARTE0 SPIM0 TWIM0 }
    shared SERIAL1 { UARTE1 SPIM1 TWIM1 }
    shared SERIAL2 { UARTE2 SPIM2 TWIM2 }
    shared SERIAL3 { UARTE3 SPIM3 TWIM3 }

    pins {
        .p00 bidirectional "P0.00" "1"  fn APP_GPIO NET_GPIO
        .p01 bidirectional "P0.01" "2"  fn APP_GPIO NET_GPIO
        .p02 bidirectional "P0.02" "3"  fn APP_GPIO NET_GPIO
        .p03 bidirectional "P0.03" "4"  fn APP_GPIO NET_GPIO
        .p04 bidirectional "P0.04" "5"  fn APP_GPIO NET_GPIO
        .p05 bidirectional "P0.05" "6"  fn APP_GPIO NET_GPIO
        .p06 bidirectional "P0.06" "7"  fn APP_GPIO
        .p07 bidirectional "P0.07" "8"  fn NET_GPIO
        .p08 bidirectional "P0.08" "9"  fn APP_GPIO NET_GPIO HSSPI_SCK
        .p09 bidirectional "P0.09" "10" fn APP_GPIO NET_GPIO HSSPI_MOSI
        .p10 bidirectional "P0.10" "11" fn APP_GPIO NET_GPIO HSSPI_MISO
        .p11 bidirectional "P0.11" "12" fn APP_GPIO NET_GPIO HSSPI_CSN
    }
}

/// Something with a pin for each of the part's, to put a net on.
part Probe {
    pins {
        .a00 passive "A00" "1"
        .a01 passive "A01" "2"
        .a02 passive "A02" "3"
        .a03 passive "A03" "4"
        .a04 passive "A04" "5"
        .a05 passive "A05" "6"
        .a06 passive "A06" "7"
        .a07 passive "A07" "8"
        .a08 passive "A08" "9"
        .a09 passive "A09" "10"
        .a10 passive "A10" "11"
        .a11 passive "A11" "12"
    }
}
"#;

fn run_with(part: &str, statements: &str) -> Checked {
    check(
        "main.nsm",
        &format!("{part}\nboard B {{\n    .u: Zmcu\n    .probe: Probe\n{statements}\n}}\n"),
        &Loader::std(),
    )
}

fn run(statements: &str) -> Checked {
    run_with(ZMCU, statements)
}

/// Pin `n` of the part, connected to the probe and chosen as `function`.
fn choose(n: u32, function: &str) -> String {
    format!("    .u.p{n:02} ~ .probe.a{n:02}\n    .u.p{n:02}.function &= \"{function}\"\n")
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn only(c: &Checked, message: &str) {
    assert_eq!(messages(c), [message], "{}", c.render());
}

/// The peripherals of a pin whose row lists both classes, as the messages list them.
const LINES: &str =
    "UARTE0, SPIM0, TWIM0, UARTE1, SPIM1, TWIM1, UARTE2, SPIM2, TWIM2, UARTE3, SPIM3, TWIM3";

#[test]
fn a_class_offers_the_functions_of_its_peripherals_on_every_pin_that_lists_it() {
    // No row lists UARTE0_TX, and any pin of the class can be it.
    for pin in [0, 1, 6] {
        let c = run(&choose(pin, "UARTE0_TX"));
        assert!(c.is_ok(), "pin {pin}: {}", c.render());
    }
    let c = run(&format!(
        "{}{}{}{}",
        choose(0, "UARTE0_TX"),
        choose(1, "UARTE0_RX"),
        choose(2, "TWIM1_SCL"),
        choose(3, "TWIM1_SDA"),
    ));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn the_class_and_a_dedicated_function_are_functions_of_the_row_too() {
    let c = run(&format!(
        "{}{}",
        choose(0, "APP_GPIO"),
        choose(8, "HSSPI_SCK")
    ));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_dedicated_function_is_offered_only_on_the_pins_whose_rows_list_it() {
    let c = run(&choose(2, "HSSPI_SCK"));
    only(
        &c,
        &format!(
            "`u.p02` cannot be `HSSPI_SCK`; that function is offered only on `u.p08`; `u.p02` offers APP_GPIO, NET_GPIO and the lines of the peripherals on those pins: {LINES}, NETUART"
        ),
    );
    // The error points at the statement that chose the function.
    let d = &c.diagnostics[0];
    let (line, _) = d.line_col(&c.sources);
    let text = &c.sources.get(d.source).text;
    assert_eq!(
        text.lines().nth(line - 1).unwrap().trim(),
        ".u.p02.function &= \"HSSPI_SCK\""
    );
}

#[test]
fn a_name_that_no_pin_offers_names_the_nearest_function() {
    let c = run(&choose(0, "UARTE0_TXD"));
    only(
        &c,
        &format!(
            "`u.p00` cannot be `UARTE0_TXD`; did you mean `UARTE0_TX`? `u.p00` offers APP_GPIO, NET_GPIO and the lines of the peripherals on those pins: {LINES}, NETUART"
        ),
    );
}

#[test]
fn a_core_owns_a_pin_through_the_classes_its_row_lists() {
    // `.p06` lists only the application core and `.p07` only the network core.
    let c = run(&choose(6, "NETUART_TX"));
    only(
        &c,
        &format!(
            "`u.p06` cannot be `NETUART_TX`; that function is offered on every pin that lists `NET_GPIO`; `u.p06` offers APP_GPIO and the lines of the peripherals on those pins: {LINES}"
        ),
    );
    let c = run(&choose(7, "UARTE0_TX"));
    only(
        &c,
        "`u.p07` cannot be `UARTE0_TX`; that function is offered on every pin that lists `APP_GPIO`; `u.p07` offers NET_GPIO and the lines of the peripherals on those pins: NETUART",
    );
    // And each core's own peripheral goes on its own pin.
    let c = run(&format!(
        "{}{}",
        choose(6, "UARTE0_TX"),
        choose(7, "NETUART_RX")
    ));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_pin_that_lists_only_a_class_still_needs_a_choice_when_it_is_connected() {
    // Its peripherals' lines are functions of it, so there is more than one.
    let c = run("    .u.p06 ~ .probe.a06\n");
    only(
        &c,
        "`u.p06` has several functions (APP_GPIO and the lines of the peripherals on those pins) and is connected: choose one with `.u.p06.function &= \"APP_GPIO\"`",
    );
    // A pin that nothing is connected to needs none.
    let c = run("");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_class_function_takes_its_kind_from_the_peripheral() {
    // Two transmitters on one net are two outputs, though no row writes a kind.
    let c = run(
        "    .u.p00 ~ .u.p01\n    .u.p00.function &= \"UARTE0_TX\"\n    .u.p01.function &= \"UARTE1_TX\"\n",
    );
    only(
        &c,
        "net `net-u.p00` has two outputs: `u.p00` (`Zmcu` function `UARTE0_TX`, an output), `u.p01` (`Zmcu` function `UARTE1_TX`, an output)",
    );
}

#[test]
fn the_shared_rule_counts_functions_a_class_offers() {
    // A second console on a unit that already runs a display: five interfaces, four units.
    let c = run(&[
        choose(0, "UARTE0_TX"),
        choose(1, "UARTE0_RX"),
        choose(2, "SPIM1_SCK"),
        choose(3, "SPIM1_MOSI"),
        choose(4, "TWIM2_SCL"),
        choose(5, "TWIM2_SDA"),
        choose(6, "TWIM3_SCL"),
        choose(8, "TWIM3_SDA"),
        choose(9, "UARTE1_TX"),
        choose(10, "UARTE1_RX"),
    ]
    .concat());
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{}", c.render());
    assert!(
        msgs[0].starts_with(
            "`u` (`Zmcu`) uses SPIM1 and UARTE1, which share `SERIAL1` and run one at a time: SPIM1 on `u.p02` as SPIM1_SCK ("
        ) && msgs[0].ends_with(
            "the other shared resources of `Zmcu` are in use as well, `SERIAL0` by UARTE0, `SERIAL2` by TWIM2 and `SERIAL3` by TWIM3, so none is free"
        ),
        "{msgs:#?}"
    );
}

#[test]
fn the_order_of_the_statements_does_not_matter() {
    // The shared resources first, then the table with its rows the other way round, then the
    // peripherals: the same part.
    let lines: Vec<&str> = ZMCU.lines().collect();
    let start = lines.iter().position(|l| *l == "part Zmcu {").unwrap();
    let end = start + lines[start..].iter().position(|l| *l == "}").unwrap();
    let body = &lines[start + 1..end];
    let starting = |word: &'static str| {
        body.iter()
            .copied()
            .filter(move |l| l.trim_start().starts_with(word))
            .collect::<Vec<&str>>()
    };
    let table = body.iter().position(|l| l.trim() == "pins {").unwrap();
    let mut rows: Vec<&str> = body[table + 1..body.len() - 1].to_vec();
    rows.reverse();
    let part = format!(
        "part Zmcu {{\n{}\n    pins {{\n{}\n    }}\n{}\n}}\n{}\n",
        starting("shared").join("\n"),
        rows.join("\n"),
        starting("peripheral").join("\n"),
        lines[end + 1..].join("\n"),
    );
    let statements = choose(2, "HSSPI_SCK");
    let a = run(&statements);
    let b = run_with(&part, &statements);
    assert_eq!(messages(&a).len(), 1, "{}", a.render());
    assert_eq!(messages(&a), messages(&b), "{}", b.render());
}

#[test]
fn a_function_forwarded_through_a_module_is_checked_against_the_class() {
    let src = format!(
        "{ZMCU}
module Hub {{
    .scl: Pin
    .mcu: Zmcu
    .scl ~ .mcu.p07
    .mcu.p07.function == .scl.function
}}
"
    );
    // The network core's pin cannot be a peripheral of the application core: the error is at
    // the board's statement.
    let bad = format!(
        "{src}
board C {{
    .hub: Hub
    .hub.scl.function &= \"UARTE0_TX\"
}}
"
    );
    let c = check("main.nsm", &bad, &Loader::std());
    let msgs = messages(&c);
    assert_eq!(msgs.len(), 1, "{}", c.render());
    assert!(
        msgs[0].starts_with("`hub.mcu.p07` cannot be `UARTE0_TX`; that function is offered on every pin that lists `APP_GPIO`"),
        "{msgs:#?}"
    );
    let good = format!(
        "{src}
board C {{
    .hub: Hub
    .hub.scl.function &= \"NETUART_TX\"
}}
"
    );
    let c = check("main.nsm", &good, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

// ---- The mistakes of a definition ----

#[test]
fn a_class_that_no_row_lists_is_refused_with_the_functions_rows_do_list() {
    let part = ZMCU.replace("peripheral UARTE0 on APP_GPIO", "peripheral UARTE0 on GPIO");
    let c = run_with(&part, "");
    only(
        &c,
        "`peripheral UARTE0 on GPIO`: no row of any `pins` table of `Zmcu` lists the function `GPIO`, so UARTE0 is offered on no pin; the functions that rows list are APP_GPIO, NET_GPIO, HSSPI_SCK, HSSPI_MOSI, HSSPI_MISO and HSSPI_CSN",
    );
}

#[test]
fn a_row_may_not_list_what_the_class_already_offers() {
    let part = ZMCU.replace(
        ".p00 bidirectional \"P0.00\" \"1\"  fn APP_GPIO NET_GPIO",
        ".p00 bidirectional \"P0.00\" \"1\"  fn APP_GPIO NET_GPIO UARTE0_TX",
    );
    let c = run_with(&part, "");
    only(
        &c,
        "`.p00` lists `UARTE0_TX`, which `peripheral UARTE0` already offers on every pin that lists `APP_GPIO`, as `.p00` does; remove it from the row",
    );
}

#[test]
fn a_pin_may_list_a_function_of_a_peripheral_whose_class_it_does_not_list() {
    // `.p07` lists only the network core's class, and a function of an application-core
    // peripheral on its row is a dedicated route to that function: allowed.
    let part = ZMCU.replace(
        ".p07 bidirectional \"P0.07\" \"8\"  fn NET_GPIO",
        ".p07 bidirectional \"P0.07\" \"8\"  fn NET_GPIO UARTE0_TX",
    );
    let c = run_with(&part, &choose(7, "UARTE0_TX"));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_function_of_a_peripheral_on_a_class_needs_no_row() {
    // Only a peripheral with no class must have each function listed by a row.
    let part = ZMCU.replace(
        "peripheral HSSPI { HSSPI_SCK:output HSSPI_MOSI:output HSSPI_MISO:input HSSPI_CSN:output }",
        "peripheral HSSPI { HSSPI_SCK:output HSSPI_MOSI:output HSSPI_MISO:input HSSPI_CSN:output HSSPI_DCX:output }",
    );
    let c = run_with(&part, "");
    only(
        &c,
        "`peripheral HSSPI` lists `HSSPI_DCX`, but no row of any `pins` table of `Zmcu` lists it, so no pin can be it; list it as `fn HSSPI_DCX` on the pins that carry it",
    );
}
