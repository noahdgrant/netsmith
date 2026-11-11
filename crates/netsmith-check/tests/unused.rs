//! `unused .processor.usb`: a board states that an interface is not used (ADR 0083). The
//! limits the part states for it, the sources its pins need and the connection rules of its
//! pins do not apply, and the board may not also use it.
//!
//! The parts are invented for these tests.

use netsmith_check::*;

const PARTS: &str = r#"
import std::bus::Usb2
import std::sources::Rail

/// Zproc, an invented processor: a supply, and a USB port that its datasheet limits to 4.35 V
/// to 5.5 V on VBUS whether or not a board uses it.
part Zproc {
    .vdd: Rail
    .usb: Usb2
    .usb.vbus.neg ~ .vdd.neg
    .usb.vbus.voltage >= 4.35V
    .usb.vbus.voltage <= 5.5V

    pins {
        .vdd.pos      power         "VDD"  "1"
        .vdd.neg      power         "VSS"  "2"
        .usb.vbus.pos power         "VBUS" "3"
        .usb.dp       bidirectional "DP"   "4"
        .usb.dm       bidirectional "DM"   "5" fn GPIO USB_DM
    }
}

/// Zpad, an invented test pad.
part Zpad {
    pins {
        .p passive "P" "1"
    }
}

/// Zjack, an invented receptacle that brings a USB port out.
part Zjack {
    .usb: Usb2

    pins {
        .usb.vbus.pos power         "VBUS" "1" optional
        .usb.vbus.neg power         "GND"  "2"
        .usb.dp       bidirectional "D+"   "3"
        .usb.dm       bidirectional "D-"   "4"
    }
}
"#;

fn run(src: &str) -> Checked {
    check("test.nsm", src, &Loader::std())
}

fn board(body: &str) -> String {
    format!(
        "{PARTS}\nboard B {{\n    .supply: Rail {{\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }}\n    .proc: Zproc\n    .supply ~ .proc.vdd\n{body}}}\n"
    )
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The relations of the board `B` that read `needle`, as text.
fn reading(c: &Checked, needle: &str) -> Vec<String> {
    let m = &c.board("B").unwrap().model;
    m.relations()
        .iter()
        .map(|r| {
            format!(
                "{} {} {}",
                m.show_expr(&r.lhs),
                r.op.symbol(),
                m.show_expr(&r.rhs)
            )
        })
        .filter(|t| t.contains(needle))
        .collect()
}

/// The nets of the board `B` that a balance is made for.
fn balanced(c: &Checked) -> Vec<String> {
    let m = &c.board("B").unwrap().model;
    m.balances().iter().map(|b| b.net.clone()).collect()
}

#[test]
fn without_the_statement_the_part_states_its_limits_and_the_pin_has_a_balance() {
    let c = run(&board(""));
    assert!(
        messages(&c).iter().all(|m| !m.contains("stated `unused`")),
        "{}",
        c.render()
    );
    assert_eq!(
        reading(&c, "proc.usb.vbus.voltage"),
        [
            "proc.usb.vbus.voltage >= 4.35V",
            "proc.usb.vbus.voltage <= 5.5V"
        ]
    );
    assert!(
        balanced(&c).iter().any(|n| n.contains("proc.usb.vbus")),
        "{:?}",
        balanced(&c)
    );
    assert!(c.board("B").unwrap().model.unused().is_empty());
}

#[test]
fn the_statement_takes_the_limits_and_the_balance_out_and_its_pins_need_no_connection() {
    let c = run(&board("    unused .proc.usb\n"));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(reading(&c, "proc.usb.vbus"), Vec::<String>::new());
    assert!(
        balanced(&c).iter().all(|n| !n.contains("proc.usb")),
        "{:?}",
        balanced(&c)
    );
    let m = &c.board("B").unwrap().model;
    assert_eq!(m.unused().len(), 1);
    let pin = m.inst_at("proc.usb.vbus.pos").unwrap();
    assert!(m.unused_by(pin).is_some());
    assert!(m.unused_by(m.inst_at("proc.vdd.pos").unwrap()).is_none());
}

#[test]
fn a_pin_that_must_be_connected_needs_no_connection_when_its_interface_is_unused() {
    // Without the statement the power pin of the port must be connected: nothing is on its net.
    let without = run(&board(""));
    assert!(
        messages(&without)
            .iter()
            .any(|m| m.contains("`proc.usb.vbus.pos` is a power pin that must be connected")),
        "{}",
        without.render()
    );
    let with = run(&board("    unused .proc.usb\n"));
    assert!(with.is_ok(), "{}", with.render());
}

#[test]
fn the_statement_reads_the_same_wherever_it_stands() {
    let before = run(&format!(
        "{PARTS}\nboard B {{\n    unused .proc.usb\n    .proc: Zproc\n    .supply: Rail\n    .supply ~ .proc.vdd\n}}\n"
    ));
    let after = run(&format!(
        "{PARTS}\nboard B {{\n    .proc: Zproc\n    .supply: Rail\n    .supply ~ .proc.vdd\n    unused .proc.usb\n}}\n"
    ));
    let inside = run(&format!(
        "{PARTS}\nboard B {{\n    .proc: Zproc {{\n        unused .usb\n    }}\n    .supply: Rail\n    .supply ~ .proc.vdd\n}}\n"
    ));
    for c in [&before, &after, &inside] {
        assert!(c.is_ok(), "{}", c.render());
        assert_eq!(c.board("B").unwrap().model.unused().len(), 1);
        assert_eq!(reading(c, "proc.usb.vbus"), Vec::<String>::new());
    }
}

#[test]
fn a_relation_that_reads_other_values_too_is_kept() {
    let src = board("    unused .proc.usb\n    .proc.usb.vbus.voltage >= .proc.vdd.voltage\n");
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        reading(&c, "proc.usb.vbus"),
        ["proc.usb.vbus.voltage >= proc.vdd.voltage"]
    );
}

#[test]
fn joining_the_interface_contradicts_the_statement() {
    let src = board(
        "    .jack: Zjack\n    .jack.usb.vbus.neg ~ .supply.neg\n    .proc.usb ~ .jack.usb\n    unused .proc.usb\n",
    );
    let c = run(&src);
    let said = messages(&c);
    assert!(
        said.iter().any(
            |m| m.contains("`proc.usb.dm` is connected here to `jack.usb.dm`")
                && m.contains("but `proc.usb` is stated `unused` at test.nsm:")
                && m.contains("remove the connection, or remove `unused`")
        ),
        "{}",
        c.render()
    );
    // The error points at the connection; the message names the line of the statement.
    let at = c
        .diagnostics
        .iter()
        .find(|d| d.message.contains("connected here"))
        .unwrap();
    assert!(
        c.sources.get(at.source).text[at.span.start..at.span.end].contains(".proc.usb"),
        "{}",
        c.render()
    );
    // One statement joins three pins and is told once.
    assert_eq!(
        messages(&c)
            .iter()
            .filter(|m| m.contains("connected here"))
            .count(),
        1,
        "{}",
        c.render()
    );
}

#[test]
fn a_pin_joined_to_a_pin_of_another_part_contradicts_the_statement() {
    let src = board(
        "    .jack: Zjack\n    .jack.usb.vbus.neg ~ .supply.neg\n    .proc.usb.dp ~ .jack.usb.dp\n    unused .proc.usb\n",
    );
    let c = run(&src);
    assert!(
        messages(&c).iter().any(|m| m
            .contains("`proc.usb.dp` is connected here to `jack.usb.dp` (net `net-jack.usb.dp`)")),
        "{}",
        c.render()
    );
}

#[test]
fn a_test_pad_and_a_ground_do_not_use_the_interface() {
    let src = board(
        "    .tp: Zpad\n    .tp.p ~ .proc.usb.dp\n    .proc.usb.vbus.pos.net &= \"VBUS_TP\"\n    .tp2: Zpad\n    .tp2.p.net &= \"VBUS_TP\"\n    .proc.usb.vbus.neg ~ .supply.neg\n    unused .proc.usb\n",
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    // The pads are on their nets, and nothing says the nets have no source.
    let m = &c.board("B").unwrap().model;
    assert!(m.net_of(m.inst_at("proc.usb.vbus.pos").unwrap()).is_some());
    assert!(
        balanced(&c).iter().all(|n| !n.contains("VBUS_TP")),
        "{:?}",
        balanced(&c)
    );
}

#[test]
fn a_function_chosen_on_a_pin_contradicts_the_statement() {
    let src = board("    .proc.usb.dm.function &= \"USB_DM\"\n    unused .proc.usb\n");
    let c = run(&src);
    assert!(
        messages(&c).iter().any(|m| m.contains(
            "`proc.usb.dm` is given a function here, but `proc.usb` is stated `unused` at test.nsm:"
        )),
        "{}",
        c.render()
    );
}

#[test]
fn only_an_interface_of_a_board_can_be_stated_unused() {
    for (body, needle) in [
        (
            "    unused .proc.vdd.pos\n",
            "is a pin, and `unused` names an interface",
        ),
        (
            "    unused .proc\n",
            "is a `Zproc`, and `unused` names an interface",
        ),
        (
            "    unused .proc.usb.vbus.voltage\n",
            "is a value, and `unused` names an interface",
        ),
    ] {
        let c = run(&board(body));
        assert!(
            messages(&c).iter().any(|m| m.contains(needle)),
            "{body}: {}",
            c.render()
        );
    }
    let c = run(&board("    unused .proc.usb\n    unused .proc.usb\n"));
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("`proc.usb` is already stated `unused` at test.nsm:")),
        "{}",
        c.render()
    );
    // A part, a module or a system cannot state that a function of its own is unused.
    let c = run(&format!(
        "{PARTS}\nmodule M {{\n    .u: Zproc\n    unused .u.usb\n}}\n"
    ));
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("`unused` is a statement of a board")),
        "{}",
        c.render()
    );
}

#[test]
fn an_interface_of_a_module_is_unused_when_the_modules_own_wiring_reaches_it() {
    // The module wires its USB port to the part inside it; the board names the nets of the
    // pads and brings them out to test pads, and says nothing uses the port.
    let src = format!(
        "{PARTS}\nmodule Zmodem {{\n    .usb: Usb2\n    .u: Zjack\n    .usb ~ .u.usb\n}}\n\nboard B {{\n    .supply: Rail {{\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }}\n    .modem: Zmodem\n    .tp_vbus: Zpad\n    .tp_dp: Zpad\n    .modem.usb.vbus.pos.net &= \"MODEM_VBUS\"\n    .tp_vbus.p.net &= \"MODEM_VBUS\"\n    .modem.usb.dp ~ .tp_dp.p\n    unused .modem.usb\n}}\n"
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    // The power view reads the balances of the board's own scope: none is for the port.
    let on_board = |c: &Checked| -> Vec<String> {
        let m = &c.board("B").unwrap().model;
        m.balances()
            .iter()
            .filter(|b| b.scope == m.root)
            .map(|b| b.net.clone())
            .collect()
    };
    assert!(
        on_board(&c)
            .iter()
            .all(|n| n != "MODEM_VBUS" && !n.contains("modem.usb")),
        "{:?}",
        on_board(&c)
    );
    // Without the statement the module's rail is a net that nothing supplies.
    let without = run(&src.replace("    unused .modem.usb\n", ""));
    assert!(
        on_board(&without).iter().any(|n| n == "MODEM_VBUS"),
        "{:?}",
        on_board(&without)
    );
}

#[test]
fn a_rail_of_the_board_that_supplies_the_port_contradicts_the_statement() {
    let src = board(
        "    .vbus: Rail {\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"VBUS\"\n        .neg.net &= \"GND\"\n    }\n    .vbus ~ .proc.usb.vbus\n    unused .proc.usb\n",
    );
    let c = run(&src);
    assert!(
        messages(&c).iter().any(|m| m
            .contains("`proc.usb.vbus` is connected here to `vbus`, but `proc.usb` is stated `unused` at test.nsm:")),
        "{}",
        c.render()
    );
    // The error is on the line of the join.
    let at = c
        .diagnostics
        .iter()
        .find(|d| d.message.contains("connected here"))
        .unwrap();
    assert_eq!(
        &c.sources.get(at.source).text[at.span.start..at.span.end],
        ".proc.usb.vbus"
    );
}

// A series passive (a fuse, a resistor, a ferrite bead) from the interface to the rest of the
// board is a use: the part still feeds the board from the interface.

/// Zload, an invented load with a power input, so a net has a pin that is not a passive one.
const LOAD: &str = r#"
import std::parts::passive::{Capacitor, FerriteBead, Fuse, Resistor}

part Zload {
    pins {
        .in power "IN" "1"
    }
}
"#;

fn series(body: &str) -> Checked {
    run(&format!("{LOAD}{}", board(body)))
}

#[test]
fn a_fuse_from_the_interface_to_a_load_is_a_use() {
    let c = series(
        "    .f: Fuse\n    .load: Zload\n    .proc.usb.vbus.pos ~> .f ~> .load.in\n    unused .proc.usb\n",
    );
    let said = messages(&c);
    assert!(
        said.iter().any(|m| m.contains(
            "`proc.usb.vbus.pos` is connected here through `f` (a `Fuse`) to `load.in` (net `"
        ) && m.contains("but `proc.usb` is stated `unused` at test.nsm:")
            && m.contains("remove the connection, or remove `unused`")),
        "{}",
        c.render()
    );
    // The error is on the chain that joins them.
    let at = c
        .diagnostics
        .iter()
        .find(|d| d.message.contains("through `f`"))
        .unwrap();
    assert!(
        c.sources.get(at.source).text[at.span.start..at.span.end].contains(".proc.usb.vbus.pos"),
        "{}",
        c.render()
    );
}

#[test]
fn a_fuse_joined_with_pin_connections_is_a_use() {
    let c = series(
        "    .f: Fuse\n    .load: Zload\n    .f.a ~ .proc.usb.vbus.pos\n    .f.b ~ .load.in\n    unused .proc.usb\n",
    );
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("through `f` (a `Fuse`) to `load.in`")),
        "{}",
        c.render()
    );
}

#[test]
fn a_chain_of_a_bead_and_a_resistor_is_a_use() {
    let c = series(
        "    .fb: FerriteBead\n    .r: Resistor\n    .load: Zload\n    .proc.usb.vbus.pos ~> .fb ~> .r ~> .load.in\n    unused .proc.usb\n",
    );
    assert!(
        messages(&c).iter().any(|m| m.contains(
            "`proc.usb.vbus.pos` is connected here through `fb` (a `FerriteBead`) and `r` (a `Resistor`) to `load.in`"
        )),
        "{}",
        c.render()
    );
}

#[test]
fn a_resistor_on_a_signal_pin_to_a_load_is_a_use() {
    let c = series(
        "    .r: Resistor\n    .load: Zload\n    .proc.usb.dp ~> .r ~> .load.in\n    unused .proc.usb\n",
    );
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("`proc.usb.dp` is connected here through `r` (a `Resistor`)")),
        "{}",
        c.render()
    );
}

#[test]
fn a_resistor_that_leads_only_to_test_pads_is_no_use() {
    let c = series(
        "    .r: Resistor\n    .fb: FerriteBead\n    .r2: Resistor\n    .tp: Zpad\n    .tp2: Zpad\n    .proc.usb.dp ~> .r ~> .tp.p\n    .proc.usb.vbus.pos ~> .fb ~> .r2 ~> .tp2.p\n    unused .proc.usb\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_series_part_back_to_ground_is_no_use() {
    // A pull-down to the return proves nothing: every ground is one net.
    let c =
        series("    .r: Resistor\n    .proc.usb.dp ~> .r ~> .supply.neg\n    unused .proc.usb\n");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_pull_up_to_the_supply_is_no_use() {
    // The owner's decision of 2026-11-10: a pull to either rail of the board proves nothing,
    // although the supply's net has pins that are not passive (the processor's `vdd`).
    let c =
        series("    .r: Resistor\n    .proc.usb.dp ~> .r ~> .supply.pos\n    unused .proc.usb\n");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_fuse_and_a_bead_to_the_supply_are_no_use() {
    let c = series(
        "    .f: Fuse\n    .fb: FerriteBead\n    .r: Resistor\n    .proc.usb.vbus.pos ~> .f ~> .supply.pos\n    .proc.usb.dp ~> .fb ~> .r ~> .supply.pos\n    unused .proc.usb\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

// The owner's decision of 2026-11-10 (item U3): only a rail that nothing on the board sources is
// a rail for a pull. A rail that a regulator makes, or that the board sources with `dc source`
// or an output pin, is fed from the board, and a path to it from the interface is a use.

/// Zldo, an invented regulator that makes a rail; Zsrc, an invented part that sources one with
/// `dc source`; Zcharger, an invented part with an output pin on a rail.
const SOURCES: &str = r#"
part Zldo {
    .vin: Rail
    .vout: Rail
    .vin.neg ~ .vout.neg
    .vin.current == -.vout.current
    dc linear_regulator .vin .vout

    pins {
        .vin.pos  power "IN"  "1"
        .vin.neg  power "GND" "2"
        .vout.pos power "OUT" "3"
    }
}

part Zsrc {
    .out: Rail
    dc source .out

    pins {
        .out.pos power "P" "1"
        .out.neg power "N" "2"
    }
}

part Zcharger {
    pins {
        .out output "OUT" "1"
    }
}
"#;

#[test]
fn a_path_to_a_rail_that_a_regulator_makes_is_a_use() {
    // `.rail` is the output of the regulator on the board: the interface's pin feeds the board.
    let c = run(&format!(
        "{SOURCES}{LOAD}{}",
        board(
            "    .ldo: Zldo\n    .rail: Rail {\n        .voltage &= 1.8V +/- 5%\n        .pos.net &= \"1V8\"\n        .neg.net &= \"GND\"\n    }\n    .supply ~ .ldo.vin\n    .rail ~ .ldo.vout\n    .f: Fuse\n    .proc.usb.vbus.pos ~> .f ~> .rail.pos\n    unused .proc.usb\n"
        )
    ));
    assert!(
        messages(&c).iter().any(|m| m.contains(
            "`proc.usb.vbus.pos` is connected here through `f` (a `Fuse`) to `ldo.vout.pos`"
        )),
        "{}",
        c.render()
    );
}

#[test]
fn a_path_to_a_rail_that_the_board_sources_with_dc_source_is_a_use() {
    let c = run(&format!(
        "{SOURCES}{LOAD}{}",
        board(
            "    .src: Zsrc\n    .rail: Rail {\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"5V\"\n        .neg.net &= \"GND\"\n    }\n    .rail ~ .src.out\n    .r: Resistor\n    .proc.usb.dp ~> .r ~> .rail.pos\n    unused .proc.usb\n"
        )
    ));
    assert!(
        messages(&c).iter().any(|m| m.contains(
            "`proc.usb.dp` is connected here through `r` (a `Resistor`) to `src.out.pos`"
        )),
        "{}",
        c.render()
    );
}

// The owner's decision of 2026-11-11: the rule counts any `dc source` on the net of a board's
// rail, also the `dc source .vbus` that a std connector states. So a supply joined to such a
// connector has a source on the board, and a pull from an unused interface to it is a use.

#[test]
fn a_pull_to_a_supply_with_no_source_on_the_board_is_allowed() {
    // The supply's source is outside the board: it is the rail for a pull.
    let c =
        series("    .r: Resistor\n    .proc.usb.dp ~> .r ~> .supply.pos\n    unused .proc.usb\n");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_pull_to_a_supply_joined_to_a_connector_that_states_dc_source_is_a_use() {
    // `UsbCPower` states `dc source .vbus`: with the supply joined to it, the board has a source
    // for the supply, which is no longer the rail for a pull.
    let c = run(&format!(
        "import std::parts::connector::UsbCPower\n{LOAD}{}",
        board(
            "    .conn: UsbCPower\n    .supply ~ .conn.vbus\n    .r: Resistor\n    .proc.usb.dp ~> .r ~> .supply.pos\n    unused .proc.usb\n"
        )
    ));
    assert_eq!(
        messages(&c),
        [
            "`proc.usb.dp` is connected here through `r` (a `Resistor`) to `conn.vbus.pos` (net `3V3`), but `proc.usb` is stated `unused` at test.nsm:63: the interface is either used or not; remove the connection, or remove `unused`"
        ],
        "{}",
        c.render()
    );
}

#[test]
fn a_path_to_a_rail_with_an_output_pin_on_it_is_a_use() {
    let c = run(&format!(
        "{SOURCES}{LOAD}{}",
        board(
            "    .charger: Zcharger\n    .rail: Rail {\n        .voltage &= 4.2V +/- 5%\n        .pos.net &= \"VBAT\"\n        .neg.net &= \"GND\"\n    }\n    .rail.pos ~ .charger.out\n    .r: Resistor\n    .proc.usb.dp ~> .r ~> .rail.pos\n    unused .proc.usb\n"
        )
    ));
    assert!(
        messages(&c).iter().any(|m| m.contains(
            "`proc.usb.dp` is connected here through `r` (a `Resistor`) to `charger.out`"
        )),
        "{}",
        c.render()
    );
}

#[test]
fn a_fuse_to_an_intermediate_rail_then_to_a_load_pin_is_a_use() {
    // The rail is made by a regulator and the fuse leads on to a load.
    let c = run(&format!(
        "{SOURCES}{LOAD}{}",
        board(
            "    .ldo: Zldo\n    .rail: Rail {\n        .voltage &= 1.8V +/- 5%\n        .pos.net &= \"1V8\"\n        .neg.net &= \"GND\"\n    }\n    .supply ~ .ldo.vin\n    .rail ~ .ldo.vout\n    .f: Fuse\n    .load: Zload\n    .proc.usb.vbus.pos ~> .f ~> .load.in\n    unused .proc.usb\n"
        )
    ));
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("through `f` (a `Fuse`) to `load.in`")),
        "{}",
        c.render()
    );
}

#[test]
fn a_pull_to_the_supply_is_no_use_when_a_regulator_on_the_board_makes_another_rail() {
    // The supply is still the rail that nothing on the board sources, though it feeds a
    // regulator; the return still counts.
    let c = run(&format!(
        "{SOURCES}{LOAD}{}",
        board(
            "    .ldo: Zldo\n    .rail: Rail {\n        .voltage &= 1.8V +/- 5%\n        .pos.net &= \"1V8\"\n        .neg.net &= \"GND\"\n    }\n    .supply ~ .ldo.vin\n    .rail ~ .ldo.vout\n    .r: Resistor\n    .r2: Resistor\n    .proc.usb.dp ~> .r ~> .supply.pos\n    .proc.usb.dp ~> .r2 ~> .supply.neg\n    unused .proc.usb\n"
        )
    ));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_pull_up_whose_pin_also_reaches_a_load_through_another_path_is_a_use() {
    let c = series(
        "    .r: Resistor\n    .r2: Resistor\n    .load: Zload\n    .proc.usb.dp ~> .r ~> .supply.pos\n    .proc.usb.dp ~> .r2 ~> .load.in\n    unused .proc.usb\n",
    );
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("through `r2` (a `Resistor`) to `load.in`")),
        "{}",
        c.render()
    );
}

#[test]
fn a_pull_up_whose_pin_is_also_joined_to_a_load_is_a_use() {
    let c = series(
        "    .r: Resistor\n    .load: Zload\n    .proc.usb.dp ~> .r ~> .supply.pos\n    .proc.usb.dp ~ .load.in\n    unused .proc.usb\n",
    );
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("`proc.usb.dp` is connected here to `load.in`")),
        "{}",
        c.render()
    );
}

#[test]
fn a_series_part_to_a_load_on_the_supply_net_through_a_fuse_is_still_a_use() {
    // A fuse from the interface to a net that has a load but no rail pin is a use, whatever
    // else is pulled to the supply.
    let c = series(
        "    .f: Fuse\n    .r: Resistor\n    .load: Zload\n    .proc.usb.vbus.pos ~> .f ~> .load.in\n    .proc.usb.dp ~> .r ~> .supply.pos\n    unused .proc.usb\n",
    );
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("through `f` (a `Fuse`) to `load.in`")),
        "{}",
        c.render()
    );
    assert!(
        messages(&c).iter().all(|m| !m.contains("through `r`")),
        "{}",
        c.render()
    );
}

#[test]
fn a_series_part_between_two_unused_interfaces_is_no_use() {
    let c = run(&format!(
        "{LOAD}{PARTS}\nboard B {{\n    .supply: Rail {{\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }}\n    .proc: Zproc\n    .jack: Zjack\n    .supply ~ .proc.vdd\n    .jack.usb.vbus.neg ~ .supply.neg\n    .r: Resistor\n    .proc.usb.dp ~> .r ~> .jack.usb.dp\n    unused .proc.usb\n    unused .jack.usb\n}}\n"
    ));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_series_part_between_one_unused_interface_and_a_used_one_is_a_use() {
    let c = run(&format!(
        "{LOAD}{PARTS}\nboard B {{\n    .supply: Rail {{\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }}\n    .proc: Zproc\n    .jack: Zjack\n    .supply ~ .proc.vdd\n    .jack.usb.vbus.neg ~ .supply.neg\n    .r: Resistor\n    .proc.usb.dp ~> .r ~> .jack.usb.dp\n    unused .proc.usb\n}}\n"
    ));
    assert!(
        messages(&c).iter().any(|m| m.contains(
            "`proc.usb.dp` is connected here through `r` (a `Resistor`) to `jack.usb.dp`"
        )),
        "{}",
        c.render()
    );
}

#[test]
fn a_capacitor_in_series_is_not_read_as_a_use() {
    // The narrowest reading: a fuse, a resistor and a ferrite bead. A series capacitor (or an
    // inductor, a diode) is a question for the owner, and the checker does not follow it.
    let c = series(
        "    .c: Capacitor\n    .load: Zload\n    .proc.usb.dp ~> .c ~> .load.in\n    unused .proc.usb\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_series_part_inside_the_module_that_holds_the_interface_is_no_use() {
    // The module's own wiring is not the board's use of the interface.
    let src = format!(
        "{LOAD}{PARTS}\nmodule Zmodem {{\n    .usb: Usb2\n    .u: Zjack\n    .r: Resistor\n    .load: Zload\n    .usb ~ .u.usb\n    .u.usb.dp ~> .r ~> .load.in\n}}\n\nboard B {{\n    .supply: Rail {{\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }}\n    .modem: Zmodem\n    unused .modem.usb\n}}\n"
    );
    let c = run(&src);
    assert!(
        messages(&c).iter().all(|m| !m.contains("through `r`")),
        "{}",
        c.render()
    );
}

// A source that is itself in an interface stated `unused` sources nothing: the board says that
// nothing uses it. So a pull to a rail that only such a pin or such a `dc source` feeds is still
// a pull to a rail nothing on the board sources, and is no use, though a load is on its net.

/// Zhub, an invented part with an interface whose `vbus` pin is an output, and Zpwr, one whose
/// rail (`.pwr`) a `dc source` gives.
const HUB: &str = r#"
part Zhub {
    .aux: Usb2

    pins {
        .aux.vbus.pos output        "OUT"  "1"
        .aux.vbus.neg power         "GND"  "2"
        .aux.dp       bidirectional "DP"   "3"
        .aux.dm       bidirectional "DM"   "4"
    }
}

part Zpwr {
    .pwr: Rail
    dc source .pwr

    pins {
        .pwr.pos power "P" "1"
        .pwr.neg power "N" "2"
    }
}
"#;

#[test]
fn an_output_pin_in_an_interface_stated_unused_does_not_source_the_rail() {
    let c = run(&format!(
        "{HUB}{LOAD}{}",
        board(
            "    .hub: Zhub\n    .rail: Rail {\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"5V\"\n        .neg.net &= \"GND\"\n    }\n    .rail.pos ~ .hub.aux.vbus.pos\n    .load: Zload\n    .rail.pos ~ .load.in\n    .r: Resistor\n    .proc.usb.dp ~> .r ~> .rail.pos\n    unused .proc.usb\n    unused .hub.aux\n"
        )
    ));
    let said = messages(&c);
    // The one thing the board gets wrong is the hub's connection to a load while the interface
    // is stated `unused`; the processor's pull is not made a use by a source that is not one.
    assert!(!c.is_ok(), "{}", c.render());
    assert_eq!(said.len(), 1, "{}", c.render());
    assert!(
        said[0].contains("`hub.aux.vbus.pos` is connected here to `load.in`")
            && said[0].contains("`hub.aux` is stated `unused`"),
        "{}",
        c.render()
    );
    assert!(
        !said
            .iter()
            .any(|m| m.contains("`proc.usb.dp` is connected")),
        "{}",
        c.render()
    );
}

#[test]
fn a_dc_source_rail_in_an_interface_stated_unused_does_not_source_the_rail() {
    let c = run(&format!(
        "{HUB}{LOAD}{}",
        board(
            "    .hub: Zpwr\n    .rail: Rail {\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"5V\"\n        .neg.net &= \"GND\"\n    }\n    .rail ~ .hub.pwr\n    .load: Zload\n    .rail.pos ~ .load.in\n    .r: Resistor\n    .proc.usb.dp ~> .r ~> .rail.pos\n    unused .proc.usb\n    unused .hub.pwr\n"
        )
    ));
    let said = messages(&c);
    assert!(
        said.iter()
            .any(|m| m.contains("`hub.pwr") && m.contains("is stated `unused`")),
        "{}",
        c.render()
    );
    assert!(!c.is_ok(), "{}", c.render());
    assert_eq!(said.len(), 1, "{}", c.render());
    assert!(
        !said
            .iter()
            .any(|m| m.contains("`proc.usb.dp` is connected")),
        "{}",
        c.render()
    );
}

/// Ztri, an invented part with a tri-state pin on a rail.
const TRI: &str = r#"
part Ztri {
    pins {
        .out tri_state "OUT" "1"
    }
}
"#;

#[test]
fn a_path_to_a_rail_with_a_tri_state_pin_on_it_is_a_use() {
    let c = run(&format!(
        "{TRI}{LOAD}{}",
        board(
            "    .t: Ztri\n    .rail: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"BUS\"\n        .neg.net &= \"GND\"\n    }\n    .rail.pos ~ .t.out\n    .r: Resistor\n    .proc.usb.dp ~> .r ~> .rail.pos\n    unused .proc.usb\n"
        )
    ));
    assert!(
        messages(&c)
            .iter()
            .any(|m| m
                .contains("`proc.usb.dp` is connected here through `r` (a `Resistor`) to `t.out`")),
        "{}",
        c.render()
    );
}
