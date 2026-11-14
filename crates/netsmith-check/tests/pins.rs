//! Pin tables, ties and the faults of a board's pins (ADR 0016).
//!
//! `Ldo9` is invented for these tests: an illustrative regulator in two packages. It is
//! not a real part and is not in the standard library.

use netsmith_check::*;

/// Two packages with different pin sets: the DFN has a sense pin and an exposed pad, the
/// SOT-23-5 has neither.
const LDO9: &str = r#"
part Ldo9 {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg

    .dropout:     Voltage
    .max_current: Current
    .vin.voltage >= .vout.voltage + .dropout
    .vin.current == -.vout.current
    -.vout.current <= .max_current

    pins "DFN-8-EP" {
        //  pin        kind        name     pads
        .vout.pos      power       "OUT"    "1" "2"
        .sense         input       "SENSE"  "3"
        .vin.neg       power       "GND"    "4" "9"
        .nc            no_connect  "NC"     "5"
        .en            input       "EN"     "6"
        .vin.pos       power       "IN"     "7" "8"
    }
    pins "SOT-23-5" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .en            input       "EN"     "3"
        .nc            no_connect  "NC"     "4"
        .vout.pos      power       "OUT"    "5"
    }
    tie .sense ~ .vout.pos

    .package: Package
    catalog ldo9 { .vout.voltage, .dropout, .max_current, .package }
}
"#;

fn run(src: &str) -> Checked {
    check("test.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// A board with an `Ldo9` called `u` in a package, plus `extra`.
fn ldo_board(package: &str, connections: &str, extra: &str) -> String {
    format!(
        "import std::sources::Rail\n{LDO9}\nboard B {{\n    \
         .input: Rail {{ .voltage &= 5V +/- 5%\n .pos.net &= \"VIN\"\n .neg.net &= \"GND\" }}\n    \
         .u: Ldo9 {{ .package &= \"{package}\"\n .vout.voltage &= 3.3V +/- 5% }}\n    \
         .output: Rail {{ .pos.net &= \"3V3\"\n .neg.net &= \"GND\"\n .current &= 0mA..50mA }}\n    \
         .input ~ .u.vin\n    .u.vout ~ .output\n{connections}\n{extra}\n}}\n"
    )
}

const FULL_DFN: &str = "    .u.sense ~ .u.vout.pos\n    .u.en ~ .u.vin.pos\n";

/// Exactly one diagnostic, and it is `message`.
fn only(c: &Checked, message: &str) {
    assert_eq!(messages(c), [message], "{}", c.render());
}

#[test]
fn a_board_that_connects_the_pins_as_the_part_says_checks_clean() {
    let c = run(&ldo_board("DFN-8-EP", FULL_DFN, ""));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn the_pin_table_is_in_the_model_per_package() {
    let c = run(&ldo_board("DFN-8-EP", FULL_DFN, ""));
    let m = &c.board("B").unwrap().model;
    let u = m.inst_at("u").unwrap();
    let tables = &m.instance(u).tables;
    assert_eq!(
        tables
            .iter()
            .map(|t| t.package.as_deref())
            .collect::<Vec<_>>(),
        [Some("DFN-8-EP"), Some("SOT-23-5")]
    );
    let sense = m.inst_at("u.sense").unwrap();
    let row = m.pin_row(sense).expect("the DFN has a sense pin");
    assert_eq!(
        (row.kind, row.name.as_str(), row.pads.clone()),
        (PinKind::Input, "SENSE", vec!["3".to_string()])
    );
    assert_eq!(m.packages_with(sense), ["DFN-8-EP"]);
    // The package that was asked for decides the pads: the output has two.
    let out = m.pin_row(m.inst_at("u.vout.pos").unwrap()).unwrap();
    assert_eq!(out.pads, ["1", "2"]);
    // `vout.neg` has no row; the part joins it to `vin.neg`.
    let vout_neg = m.inst_at("u.vout.neg").unwrap();
    assert!(m.pin_row(vout_neg).is_none() && m.is_alias(vout_neg));
    assert!(!m.is_alias(m.inst_at("u.vin.neg").unwrap()));
    assert_eq!(m.instance(u).ties.len(), 1);
}

#[test]
fn a_required_pin_with_nothing_on_its_net_is_refused() {
    let c = run(&ldo_board("DFN-8-EP", "    .u.sense ~ .u.vout.pos\n", ""));
    only(
        &c,
        "`u.en` is an input that must be connected (pad 6, `EN`), but nothing else is on its net",
    );
    let (line, _) = c.diagnostics[0].line_col(&c.sources);
    let text = &c.sources.get(c.diagnostics[0].source).text;
    assert!(
        text.lines().nth(line - 1).unwrap().contains(".u: Ldo9"),
        "points at the board's declaration of the part"
    );
}

#[test]
fn a_pin_wired_only_to_its_own_part_is_still_unconnected() {
    // `en` is on a net with the part's own `vin.pos`, which is not another part.
    let c = run(&ldo_board(
        "DFN-8-EP",
        "    .u.sense ~ .u.vout.pos\n    .u.en ~ .u.nc\n",
        "",
    ));
    let msgs = messages(&c);
    assert!(
        msgs.iter().any(|m| m == "`u.en` is an input that must be connected (pad 6, `EN`), but nothing else is on its net"),
        "{msgs:#?}"
    );
}

#[test]
fn a_power_pin_must_be_connected_too() {
    let src = ldo_board("DFN-8-EP", FULL_DFN, "").replace("    .u.vout ~ .output\n", "");
    let c = run(&src);
    let msgs = messages(&c);
    assert!(
        msgs.contains(&"`u.vout.pos` is a power pin that must be connected (pads 1, 2, `OUT`), but nothing else is on its net".to_string()),
        "{msgs:#?}"
    );
}

#[test]
fn a_tie_the_board_does_not_make_is_refused() {
    // The sense pin is connected, but not to the output.
    let c = run(&ldo_board(
        "DFN-8-EP",
        "    .u.sense ~ .u.vin.pos\n    .u.en ~ .u.vin.pos\n",
        "",
    ));
    only(
        &c,
        "`u.sense` must be on the same net as `u.vout.pos` (`Ldo9` ties them), but `u.sense` is on net `VIN` with `input.pos`, `u.en`, `u.vin.pos`",
    );
    // It points at the `tie` line of the part.
    let text = &c.sources.get(c.diagnostics[0].source).text;
    let (line, _) = c.diagnostics[0].line_col(&c.sources);
    assert!(text.lines().nth(line - 1).unwrap().contains("tie .sense"));
}

#[test]
fn a_tie_with_nothing_connected_says_so() {
    let c = run(&ldo_board("DFN-8-EP", "    .u.en ~ .u.vin.pos\n", ""));
    let msgs = messages(&c);
    assert!(
        msgs.contains(&"`u.sense` must be on the same net as `u.vout.pos` (`Ldo9` ties them), but nothing is connected to `u.sense`".to_string()),
        "{msgs:#?}"
    );
}

#[test]
fn a_no_connect_pin_must_stay_alone() {
    let c = run(&ldo_board(
        "DFN-8-EP",
        &format!("{FULL_DFN}    .u.nc ~ .u.vout.pos\n"),
        "",
    ));
    only(
        &c,
        "`u.nc` is a no_connect pin (pad 5, `NC`) but it is on net `3V3` with `output.pos`, `u.sense`, `u.vout.pos`",
    );
}

#[test]
fn a_package_without_a_pin_does_not_tie_it_and_does_not_ask_for_it() {
    // The SOT-23-5 has no sense pin: the tie is skipped there, and the board says nothing
    // about a pin it does not have.
    let c = run(&ldo_board("SOT-23-5", "    .u.en ~ .u.vin.pos\n", ""));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_board_that_connects_a_pin_the_package_lacks_is_refused() {
    let c = run(&ldo_board("SOT-23-5", FULL_DFN, ""));
    only(
        &c,
        "`u.sense` (`SENSE`) does not exist in package `SOT-23-5` of `Ldo9`, but the board connects it; the packages that have it: `DFN-8-EP`",
    );
}

#[test]
fn a_package_with_no_table_is_named_with_the_ones_there_are() {
    let c = run(&ldo_board("QFN-9", FULL_DFN, ""));
    only(
        &c,
        "`u` is a `Ldo9` in package `QFN-9`, which has no pin table; its tables are for `DFN-8-EP`, `SOT-23-5`",
    );
}

#[test]
fn a_package_the_solver_will_choose_is_not_judged_before_it_has() {
    // No package yet: nothing that depends on it is said at check time.
    let src = ldo_board("DFN-8-EP", "    .u.sense ~ .u.vin.pos\n", "")
        .replace(".package &= \"DFN-8-EP\"\n", "");
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
}

const TX: &str = r#"
part Tx {
    pins {
        .tx output "TX" "1"
    }
}
"#;

#[test]
fn two_outputs_on_one_net_are_refused() {
    let c = run(&format!(
        "{TX}\nboard B {{\n    .a: Tx\n    .b: Tx\n    .a.tx ~ .b.tx\n}}\n"
    ));
    only(&c, "net `net-a.tx` has two outputs: `a.tx`, `b.tx`");
}

#[test]
fn an_output_may_share_a_net_with_a_bidirectional_pin_and_inputs() {
    let src = r#"
part Tx {
    pins {
        .tx output "TX" "1"
    }
}
part Bus {
    pins {
        .io bidirectional "IO" "1"
    }
}
part Rx {
    pins {
        .rx input "RX" "1"
    }
}
board B {
    .a: Tx
    .b: Bus
    .c: Rx
    .a.tx ~ .b.io ~ .c.rx
}
"#;
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn outputs_joined_inside_a_part_are_one_output() {
    let src = r#"
part Tx2 {
    pins {
        .tx output "TX" "1"
        .tx_b output "TX" "2"
    }
    .tx ~ .tx_b
}
board B {
    .a: Tx2
    .a.tx ~ .a.tx_b
}
"#;
    // Two pads of one output; joining them in the part is allowed, and so is the board
    // naming them.
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
}

const MCU: &str = r#"
part Mcu32 {
    pins {
        .pa2   bidirectional "PA2"   "12" fn GPIO UART1_TX SPI1_SCK
        .pa3   bidirectional "PA3"   "13" fn GPIO UART1_RX
        .pa4   bidirectional "PA4"   "16" fn GPIO
        .pb6   bidirectional "PB6"   "14" fn GPIO UART1_TX
        .boot0 input         "BOOT0" "31" optional
        .led   output        "LED"   "7"  required
    }
}
part Peer {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
        .c passive "C" "3"
    }
}
"#;

fn mcu_board(statements: &str) -> Checked {
    run(&format!(
        "{MCU}\nboard B {{\n    .u: Mcu32\n    .p: Peer\n    .u.led ~ .p.a\n{statements}\n}}\n"
    ))
}

#[test]
fn a_connected_pin_with_several_functions_needs_one_chosen() {
    let c = mcu_board("    .u.pa2 ~ .p.b");
    only(
        &c,
        "`u.pa2` has several functions (GPIO, UART1_TX, SPI1_SCK) and is connected: choose one with `.u.pa2.function &= GPIO`",
    );
    // An unconnected pin needs no function, and a pin with one needs none.
    let c = mcu_board("    .u.pa4 ~ .p.b");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_chosen_function_is_recorded_on_the_pin() {
    let c = mcu_board("    .u.pa2 ~ .p.b\n    .u.pa2.function &= UART1_TX");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let pa2 = m.inst_at("u.pa2").unwrap();
    assert_eq!(m.function_of(pa2).as_deref(), Some("UART1_TX"));
}

#[test]
fn a_function_the_pin_does_not_offer_is_refused() {
    let c = mcu_board("    .u.pa2 ~ .p.b\n    .u.pa2.function &= I2C1_SDA");
    only(
        &c,
        "`u.pa2` cannot be `I2C1_SDA`; its functions are GPIO, UART1_TX, SPI1_SCK",
    );
    let text = &c.sources.get(c.diagnostics[0].source).text;
    let (line, _) = c.diagnostics[0].line_col(&c.sources);
    assert!(text.lines().nth(line - 1).unwrap().contains("I2C1_SDA"));
}

#[test]
fn a_pin_with_no_functions_offers_none() {
    let c = mcu_board("    .u.boot0 ~ .p.b\n    .u.boot0.function &= GPIO");
    only(
        &c,
        "`u.boot0` cannot be `GPIO`; it has no functions to choose from",
    );
}

#[test]
fn one_function_cannot_be_chosen_on_two_pins_of_a_part() {
    let c = mcu_board(
        "    .u.pa2 ~ .p.b\n    .u.pb6 ~ .p.c\n    .u.pa2.function &= UART1_TX\n    .u.pb6.function &= UART1_TX",
    );
    only(
        &c,
        "function `UART1_TX` is chosen on both `u.pa2` and `u.pb6`",
    );
}

#[test]
fn optional_and_required_move_a_pin_from_its_kinds_default() {
    // `boot0` is an input marked optional: it may float. `led` is an output marked required.
    let c = mcu_board("");
    assert!(c.is_ok(), "{}", c.render());
    let src = format!("{MCU}\nboard B {{\n    .u: Mcu32\n    .p: Peer\n}}\n");
    let c = run(&src);
    only(
        &c,
        "`u.led` is an output that must be connected (pad 7, `LED`), but nothing else is on its net",
    );
}

// ---- the tables themselves -------------------------------------------------------

fn part_error(body: &str, message: &str) {
    let c = run(&format!(
        "import std::sources::Rail\npart P {{\n{body}\n}}\n"
    ));
    assert!(
        messages(&c).iter().any(|m| m == message),
        "wanted `{message}`, got {:#?}",
        messages(&c)
    );
}

#[test]
fn a_pin_has_one_kind_and_one_name_in_every_package() {
    part_error(
        "    .package: Package\n    pins \"A\" {\n        .x input \"X\" \"1\"\n    }\n    pins \"B\" {\n        .x output \"X\" \"1\"\n    }",
        "`.x` is an output named `X` in `pins \"B\"` but an input named `X` in `pins \"A\"`; a pin has one kind, one datasheet name and one set of absolute limits in every package",
    );
    part_error(
        "    .package: Package\n    pins \"A\" {\n        .x input \"X\" \"1\"\n    }\n    pins \"B\" {\n        .x input \"Y\" \"1\"\n    }",
        "`.x` is an input named `Y` in `pins \"B\"` but an input named `X` in `pins \"A\"`; a pin has one kind, one datasheet name and one set of absolute limits in every package",
    );
    part_error(
        "    .package: Package\n    pins \"A\" {\n        .x input \"X\" \"1\" optional\n    }\n    pins \"B\" {\n        .x input \"X\" \"1\"\n    }",
        "`.x` is an input named `X` in `pins \"B\"` but an input named `X` (optional) in `pins \"A\"`; a pin has one kind, one datasheet name and one set of absolute limits in every package",
    );
}

#[test]
fn pads_and_functions_may_differ_by_package() {
    let c = run(
        "part P {\n    .package: Package\n    pins \"A\" {\n        .x input \"X\" \"1\" fn A B\n    }\n    pins \"B\" {\n        .x input \"X\" \"7\" \"8\" fn A\n    }\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn tables_are_one_unnamed_or_one_for_each_package() {
    part_error(
        "    pins {\n        .x input \"X\" \"1\"\n    }\n    pins {\n        .y input \"Y\" \"2\"\n    }",
        "this part already has a `pins` table for every package; two would give a pin two answers",
    );
    part_error(
        "    .package: Package\n    pins \"A\" {\n        .x input \"X\" \"1\"\n    }\n    pins \"A\" {\n        .x input \"X\" \"1\"\n    }",
        "this part already has a `pins` table for package `A`",
    );
    part_error(
        "    .package: Package\n    pins {\n        .x input \"X\" \"1\"\n    }\n    pins \"A\" {\n        .x input \"X\" \"1\"\n    }",
        "this part has a `pins` table for every package; a table for one package would give its pins a second answer, so remove one of them",
    );
    part_error(
        "    .package: Package\n    pins \"A\" {\n        .x input \"X\" \"1\"\n    }\n    pins {\n        .x input \"X\" \"1\"\n    }",
        "this part has `pins` tables for packages; an unnamed table would apply to all of them, so give this one a package or remove the others",
    );
}

#[test]
fn a_table_for_a_package_needs_a_package_field() {
    part_error(
        "    pins \"A\" {\n        .x input \"X\" \"1\"\n    }",
        "`pins \"A\"` names a package, but this part has no `.package: Package` to hold it",
    );
}

#[test]
fn a_row_names_a_pin_of_the_part() {
    part_error(
        "    .v: Rail\n    pins {\n        .v input \"V\" \"1\"\n    }",
        "`.v` is a `Rail`, not a pin; name one of its pins, such as `.v.pos`",
    );
    part_error(
        "    .v: Voltage\n    pins {\n        .v input \"V\" \"1\"\n    }",
        "`.v` is a value, not a pin",
    );
    part_error(
        "    pins {\n        .x input \"X\" \"1\"\n        .x input \"X\" \"2\"\n    }",
        "`.x` is listed twice in this pin table; one row gives all of its pads",
    );
    part_error(
        "    .x: Pin\n    pins {\n        .x input \"X\" \"1\"\n    }",
        "`x` is already declared in this block",
    );
    part_error(
        "    pins {\n        .v.pos input \"V\" \"1\"\n    }",
        "`v` is not a member of this P",
    );
}

#[test]
fn a_row_does_not_repeat_a_pad_or_a_function() {
    part_error(
        "    pins {\n        .x input \"X\" \"1\" \"1\"\n    }",
        "pad \"1\" is listed twice for `.x`",
    );
    part_error(
        "    pins {\n        .x input \"X\" \"1\" fn A A\n    }",
        "function `A` is listed twice for `.x`",
    );
}

#[test]
fn optional_and_required_say_something_the_kind_does_not() {
    part_error(
        "    pins {\n        .x output \"X\" \"1\" optional\n    }",
        "`optional` is for a power or input pin, which must be connected unless it says so; `.x` is an output and may float already",
    );
    part_error(
        "    pins {\n        .x input \"X\" \"1\" required\n    }",
        "`required` is for an output, bidirectional or passive pin, which may float unless it says so; `.x` is an input and must be connected already",
    );
    part_error(
        "    pins {\n        .x no_connect \"X\" \"1\" optional\n    }",
        "a no_connect pin must stay unconnected, so it is neither optional nor required",
    );
}

#[test]
fn pins_and_tie_belong_in_a_part() {
    let c = run("module M {\n    pins {\n        .x input \"X\" \"1\"\n    }\n}\n");
    assert!(
        messages(&c).contains(&"`pins` can only appear directly inside a part".to_string()),
        "{:#?}",
        messages(&c)
    );
    let c = run("part P {\n    .a: Pin\n}\nboard B {\n    .p: P\n    tie .p.a ~ .p.a\n}\n");
    assert!(
        messages(&c).contains(&"`tie` can only appear directly inside a part".to_string()),
        "{:#?}",
        messages(&c)
    );
}

#[test]
fn a_tie_joins_two_different_pins_once() {
    part_error(
        "    .a: Pin\n    tie .a ~ .a",
        "a tie needs two different pins",
    );
    part_error(
        "    .a: Pin\n    .b: Pin\n    tie .a ~ .b\n    tie .b ~ .a",
        "this tie is already stated",
    );
    part_error(
        "    .a: Pin\n    .v: Voltage\n    tie .a ~ .v",
        "`.v` is a value, not a pin",
    );
}

#[test]
fn a_pin_is_declared_once_by_its_table_and_not_set_by_pad() {
    // `.pad` is gone: the table is the one way to give a pad.
    let c = run("part P {\n    .a: Pin { .pad &= \"1\" }\n}\n");
    assert!(
        messages(&c).contains(&"`pad` is not a member of this Pin".to_string()),
        "{:#?}",
        messages(&c)
    );
}

// ---- catalogs keyed by a column ------------------------------------------------------

#[test]
fn a_catalog_may_name_the_column_that_picks_a_row() {
    let c = run(
        "part P {\n    .package: Package\n    catalog mcu by .mpn { .mpn, .package }\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    part_error(
        "    .package: Package\n    catalog mcu by .mpn { .package }",
        "the key `.mpn` of a catalog must be one of its columns",
    );
    part_error(
        "    .r: Rail\n    catalog mcu by .r { .mpn }",
        "`.r` is not a plain value, so it cannot be a key",
    );
}

// ---- several packages share a table, and the absolute maximum in the row (ADR 0124) ----

/// A regulator with one table for two packages and the absolute maximum of its enable pin in
/// the row, measured from the table's one rail ground, `.vin.neg`.
const SHARED: &str = r#"
part Shared {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current

    pins "SOT-23-5" "SOT-223-5" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .en            input       "EN"     "3" abs_max 6V
        .fb            input       "FB"     "4" optional abs_max 2V from .vin.neg
        .vout.pos      power       "OUT"    "5"
    }

    .package: Package
}
"#;

fn shared_board(package: &str) -> Checked {
    run(&format!(
        "import std::sources::Rail\n{SHARED}\nboard B {{\n    \
         .input: Rail {{ .voltage &= 5V\n .pos.net &= \"VIN\"\n .neg.net &= \"GND\" }}\n    \
         .u: Shared {{ .package &= \"{package}\" }}\n    \
         .output: Rail {{ .pos.net &= \"3V3\"\n .neg.net &= \"GND\"\n .current &= 0mA..50mA }}\n    \
         .input ~ .u.vin\n    .u.vout ~ .output\n    .u.en ~ .u.vin.pos\n}}\n"
    ))
}

#[test]
fn a_shared_table_is_the_table_of_each_of_its_packages() {
    for package in ["SOT-23-5", "SOT-223-5"] {
        let c = shared_board(package);
        assert!(c.is_ok(), "{}", c.render());
        let m = &c.board("B").unwrap().model;
        let u = m.inst_at("u").unwrap();
        let tables = &m.instance(u).tables;
        assert_eq!(
            tables
                .iter()
                .map(|t| t.package.as_deref())
                .collect::<Vec<_>>(),
            [Some("SOT-23-5"), Some("SOT-223-5")]
        );
        assert_eq!(tables[0].rows, tables[1].rows);
        let en = m.inst_at("u.en").unwrap();
        assert_eq!(m.pin_row(en).unwrap().pads, ["3"]);
    }
}

#[test]
fn a_rows_abs_max_narrows_the_pin_and_names_its_ground() {
    let c = shared_board("SOT-23-5");
    let m = &c.board("B").unwrap().model;
    match m.domain_at("u.en.abs_max") {
        Some(Domain::Quantity(q)) => assert_eq!((q.lo(), q.hi()), (6.0, 6.0)),
        other => panic!("{other:?}"),
    }
    let ground = m.inst_at("u.vin.neg").unwrap();
    // No `from`: the table's one rail ground.
    assert_eq!(m.limit_reference(m.inst_at("u.en").unwrap()), Some(ground));
    // `from` names it.
    assert_eq!(m.limit_reference(m.inst_at("u.fb").unwrap()), Some(ground));
    let row = m.pin_row(m.inst_at("u.fb").unwrap()).unwrap();
    assert_eq!(row.limits.as_ref().unwrap().text, "abs_max 2V from .vin.neg");
    // A pin with no limit in its row has no reference.
    assert_eq!(m.limit_reference(m.inst_at("u.vout.pos").unwrap()), None);
}

#[test]
fn a_package_is_listed_once_in_a_header() {
    part_error(
        "    .package: Package\n    pins \"A\" \"B\" \"A\" {\n        .x input \"X\" \"1\"\n    }",
        "package \"A\" is listed twice in this `pins` header",
    );
    // A package of a shared header that another table has.
    part_error(
        "    .package: Package\n    pins \"A\" \"B\" {\n        .x input \"X\" \"1\"\n    }\n    pins \"B\" {\n        .x input \"X\" \"1\"\n    }",
        "this part already has a `pins` table for package `B`",
    );
    part_error(
        "    pins \"A\" \"B\" {\n        .x input \"X\" \"1\"\n    }",
        "`pins \"B\"` names a package, but this part has no `.package: Package` to hold it",
    );
}

#[test]
fn from_names_a_pin_of_the_part() {
    let c = run(
        "import std::sources::Rail\npart P {\n    .vin: Rail\n    pins {\n        .vin.pos power \"IN\" \"1\"\n        .vin.neg power \"GND\" \"2\"\n        .en input \"EN\" \"3\" abs_max 6V from .gnd\n    }\n}\n",
    );
    assert_eq!(messages(&c), ["`gnd` is not a member of this P"]);
    part_error(
        "    .vin: Rail\n    .limit: Voltage\n    pins {\n        .vin.pos power \"IN\" \"1\"\n        .vin.neg power \"GND\" \"2\"\n        .en input \"EN\" \"3\" abs_max 6V from .limit\n    }",
        "`.limit` is a value, not a pin",
    );
    part_error(
        "    .vin: Rail\n    pins {\n        .vin.pos power \"IN\" \"1\"\n        .vin.neg power \"GND\" \"2\"\n        .en input \"EN\" \"3\" abs_max 6V from .en\n    }",
        "`.en` cannot be measured from itself; name its ground",
    );
}

#[test]
fn a_limit_with_no_from_needs_one_rail_ground_in_its_table() {
    part_error(
        "    pins {\n        .gnd power \"GND\" \"1\"\n        .en input \"EN\" \"2\" abs_max 6V\n    }",
        "the limits of `.en` are measured from the part's ground, the `neg` of its rail, and this table has no rail's `neg` to be it; name the pin they are measured from, such as `abs_max 65V from .gnd`",
    );
    part_error(
        "    .a: Rail\n    .b: Rail\n    pins {\n        .a.pos power \"A\" \"1\"\n        .a.neg power \"AGND\" \"2\"\n        .b.pos power \"B\" \"3\"\n        .b.neg power \"BGND\" \"4\"\n        .en input \"EN\" \"5\" abs_max 6V\n    }",
        "the limits of `.en` are measured from the part's ground, the `neg` of its rail, and this table has several, `.a.neg` and `.b.neg`; name the pin they are measured from, such as `abs_max 65V from .gnd`",
    );
    // `from` settles it.
    let c = run(
        "import std::sources::Rail\npart P {\n    pins {\n        .gnd power \"GND\" \"1\"\n        .en input \"EN\" \"2\" abs_max 6V from .gnd\n    }\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_logic_line_takes_its_ground_from_its_logic() {
    let src = "import std::sources::Rail\nimport std::logic::Logic\npart P {\n    .vcc: Rail\n    .a: Logic\n    .a.reference ~ .vcc\n    pins {\n        .vcc.pos power \"VCC\" \"1\"\n        .vcc.neg power \"GND\" \"2\"\n        .a.line input \"A\" \"3\" abs_max 6.5V\n    }\n}\n";
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
    let c = run(&src.replace("abs_max 6.5V", "abs_max 6.5V from .vcc.neg"));
    assert_eq!(
        messages(&c),
        ["`.a.line` is measured from the ground its `Logic` or its bus's `levels` give, the `neg` of their reference rail; leave out `from`"]
    );
}

#[test]
fn a_pin_has_one_absolute_maximum_in_every_package() {
    part_error(
        "    .vin: Rail\n    .package: Package\n    pins \"A\" {\n        .vin.neg power \"GND\" \"1\"\n        .x input \"X\" \"2\" abs_max 6V\n    }\n    pins \"B\" {\n        .vin.neg power \"GND\" \"1\"\n        .x input \"X\" \"2\" abs_max 5V\n    }",
        "`.x` is an input named `X` with `abs_max 5V` in `pins \"B\"` but an input named `X` with `abs_max 6V` in `pins \"A\"`; a pin has one kind, one datasheet name and one set of absolute limits in every package",
    );
    // The same limit in two tables is one narrowing.
    let c = run(
        "import std::sources::Rail\npart P {\n    .vin: Rail\n    .package: Package\n    pins \"A\" {\n        .vin.neg power \"GND\" \"1\"\n        .x input \"X\" \"2\" abs_max 6V\n    }\n    pins \"B\" {\n        .vin.neg power \"GND\" \"3\"\n        .x input \"X\" \"4\" abs_max 6V\n    }\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

// ---- the limits are written only in the row (ADR 0126) ----------------------------

/// A part with a `Logic` input whose limits are in its row.
const LOGIC_ROW: &str = "import std::sources::Rail\nimport std::logic::Logic\npart P {\n    .vcc: Rail\n    .a: Logic\n    .a.reference ~ .vcc\n    pins {\n        .vcc.pos power \"VCC\" \"1\"\n        .vcc.neg power \"GND\" \"2\"\n        .a.line input \"A\" \"3\" optional abs_min -0.5V abs_max 6.5V\n    }\n}\n";

#[test]
fn a_rows_limits_narrow_both_fields_of_the_pin_and_name_the_ground_of_its_logic() {
    let src = format!(
        "{LOGIC_ROW}\nboard B {{\n    .u: P {{ .vcc.voltage &= 3V }}\n    .s: Rail {{ .voltage &= 3V\n .pos.net &= \"V\"\n .neg.net &= \"G\" }}\n    .s ~ .u.vcc\n}}\n"
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    match m.domain_at("u.a.line.abs_min") {
        Some(Domain::Quantity(q)) => assert_eq!((q.lo(), q.hi()), (-0.5, -0.5)),
        other => panic!("{other:?}"),
    }
    match m.domain_at("u.a.line.abs_max") {
        Some(Domain::Quantity(q)) => assert_eq!((q.lo(), q.hi()), (6.5, 6.5)),
        other => panic!("{other:?}"),
    }
    let row = m.pin_row(m.inst_at("u.a.line").unwrap()).unwrap();
    assert_eq!(
        row.limits.as_ref().unwrap().text,
        "abs_min -0.5V abs_max 6.5V"
    );
    // The ground is the `Logic`'s, which the solver reads: the row names none.
    assert_eq!(m.limit_reference(m.inst_at("u.a.line").unwrap()), None);
}

#[test]
fn an_abs_min_alone_is_a_row_too_and_needs_the_ground_like_an_abs_max() {
    let c = run(
        "import std::sources::Rail\npart P {\n    .vin: Rail\n    pins {\n        .vin.pos power \"IN\" \"1\"\n        .vin.neg power \"GND\" \"2\"\n        .en input \"EN\" \"3\" abs_min -0.3V\n    }\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    part_error(
        "    pins {\n        .gnd power \"GND\" \"1\"\n        .en input \"EN\" \"2\" abs_min -0.3V\n    }",
        "the limits of `.en` are measured from the part's ground, the `neg` of its rail, and this table has no rail's `neg` to be it; name the pin they are measured from, such as `abs_max 65V from .gnd`",
    );
}

#[test]
fn a_statement_of_a_pins_limit_is_refused_with_the_row_that_says_it() {
    // A part's own statement, on a `Logic` line, on a bus pin and on a plain pin.
    let on_line = LOGIC_ROW
        .replace(
            " abs_min -0.5V abs_max .vcc.voltage + 0.5V",
            "",
        )
        .replace(
            "    pins {",
            "    .a.line.abs_max &= 6.5V\n    pins {",
        );
    assert_eq!(
        messages(&run(&on_line)),
        ["a pin's absolute maximum is written in its pin row, not with `&=`: end the row of `.a.line` in the `pins` table of its part with `abs_max 6.5V`, such as `.a.line input \"NAME\" \"3\" abs_max 6.5V` (ADR 0126)"]
    );
    let on_min = on_line.replace(".a.line.abs_max &= 6.5V", ".a.line.abs_min &= -0.5V");
    assert_eq!(
        messages(&run(&on_min)),
        ["a pin's absolute minimum is written in its pin row, not with `&=`: end the row of `.a.line` in the `pins` table of its part with `abs_min -0.5V`, such as `.a.line input \"NAME\" \"3\" abs_min -0.5V` (ADR 0126)"]
    );
    part_error(
        "    .vin: Rail\n    .en: Pin\n    .en.abs_max &= 6V\n    pins {\n        .vin.pos power \"IN\" \"1\"\n        .vin.neg power \"GND\" \"2\"\n        .en input \"EN\" \"3\"\n    }",
        "a pin's absolute maximum is written in its pin row, not with `&=`: end the row of `.en` in the `pins` table of its part with `abs_max 6V`, such as `.en input \"NAME\" \"3\" abs_max 6V` (ADR 0126)",
    );
}

#[test]
fn a_board_cannot_state_a_pins_limit_either_and_a_default_is_refused_too() {
    let src = format!(
        "{LOGIC_ROW}\nboard B {{\n    .u: P {{ .a.line.abs_max &= 2V }}\n    .v: P {{ .a.line.abs_min ?= 0V }}\n}}\n"
    );
    let msgs = messages(&run(&src));
    assert!(
        msgs.iter().any(|m| m.starts_with("a pin's absolute maximum is written in its pin row, not with `&=`: end the row of `.a.line`")),
        "{msgs:#?}"
    );
    assert!(
        msgs.iter().any(|m| m.starts_with("a pin's absolute minimum is written in its pin row, not with `?=`")),
        "{msgs:#?}"
    );
}
