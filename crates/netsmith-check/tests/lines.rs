//! The lines and roles of the standard buses (ADR 0064): the table of a part against its role,
//! one line on one net, the crossing of a UART, the limit of masters, the select of one slave,
//! the lines a role requires, and the nets that a bus rule reports.
//!
//! The parts are invented for these tests. A comment on a `tri_state` row stands for the page
//! of the datasheet that says it.

use netsmith_check::*;

const PARTS: &str = r#"
import std::bus::{I2c, I2cRole, Spi, SpiRole, Uart}
import std::sources::Rail

/// Zcpu, an invented processor: one SPI master, a second select on a plain pin, and a UART.
part Zcpu {
    .vdd: Rail
    .spi: Spi
    .uart: Uart
    .spi.role &= SpiRole::Master

    pins {
        .vdd.pos  power         "VDD"  "1"
        .vdd.neg  power         "VSS"  "2"
        .spi.sclk output        "SCK"  "3"
        .spi.mosi output        "MOSI" "4"
        .spi.miso input         "MISO" "5"
        .spi.cs   output        "CS0"  "6"
        .sel1     bidirectional "IO7"  "7"
        .uart.tx  output        "TXD"  "8"
        .uart.rx  input         "RXD"  "9" optional
    }
}

/// Zflash, an invented serial flash.
part Zflash {
    .vdd: Rail
    .spi: Spi
    .spi.role &= SpiRole::Slave

    pins {
        .vdd.pos  power     "VCC" "8"
        .vdd.neg  power     "GND" "4"
        .spi.sclk input     "CLK" "6"
        .spi.mosi input     "DI"  "5"
        // Page 8: DO is high impedance while /CS is high.
        .spi.miso tri_state "DO"  "2"
        .spi.cs   input     "/CS" "1"
    }
}

/// Zimu, an invented IMU.
part Zimu {
    .vdd: Rail
    .spi: Spi
    .spi.role &= SpiRole::Slave

    pins {
        .vdd.pos  power     "VDD" "8"
        .vdd.neg  power     "GND" "6"
        .spi.sclk input     "SCL" "1"
        .spi.mosi input     "SDI" "2"
        // Table 14: SDO is high impedance while CS is high.
        .spi.miso tri_state "SDO" "3"
        .spi.cs   input     "CS"  "4"
    }
}

/// Zgyro, an invented gyroscope whose datasheet draws SDO as an output and says no more.
part Zgyro {
    .vdd: Rail
    .spi: Spi
    .spi.role &= SpiRole::Slave

    pins {
        .vdd.pos  power  "VDD" "8"
        .vdd.neg  power  "GND" "6"
        .spi.sclk input  "SCL" "1"
        .spi.mosi input  "SDI" "2"
        .spi.miso output "SDO" "3"
        .spi.cs   input  "CS"  "4"
    }
}

/// Zold, an invented sensor from before roles: no role, every SPI pin `bidirectional`.
part Zold {
    .vdd: Rail
    .spi: Spi

    pins {
        .vdd.pos  power         "VDD" "8"
        .vdd.neg  power         "GND" "6"
        .spi.sclk bidirectional "SCL" "1"
        .spi.mosi bidirectional "SDI" "2"
        .spi.miso bidirectional "SDO" "3"
        .spi.cs   bidirectional "CS"  "4"
    }
}

/// Zlink, an invented UART bridge with flow control.
part Zlink {
    .vdd: Rail
    .uart: Uart

    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .uart.tx  output "TX"  "3"
        .uart.rx  input  "RX"  "4"
        .uart.rts output "RTS" "5"
        .uart.cts input  "CTS" "6"
    }
}

/// Zbare, an invented UART part with no flow control: no rows for `rts` and `cts`.
part Zbare {
    .vdd: Rail
    .uart: Uart

    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .uart.tx  output "TX"  "3"
        .uart.rx  input  "RX"  "4"
    }
}
"#;

const SUPPLY: &str = "    .supply: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }\n";

fn run(src: &str) -> Checked {
    check("main.nsm", src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// A board with `parts` (`.cpu: Zcpu`, one per line, each on the supply) and `joins`.
fn board(parts: &[(&str, &str)], joins: &str) -> String {
    let mut body = String::new();
    for (name, ty) in parts {
        body.push_str(&format!("    .{name}: {ty}\n    .supply ~ .{name}.vdd\n"));
    }
    format!("{PARTS}\nboard B {{\n{SUPPLY}{body}{joins}\n}}\n")
}

const SENSORS: &[(&str, &str)] = &[
    ("cpu", "Zcpu"),
    ("flash", "Zflash"),
    ("imu", "Zimu"),
    ("link", "Zlink"),
];

/// The board of the guide: the flash joins as a whole interface, the IMU shares `sclk`, `mosi`
/// and `miso` and has the select of its own on `sel1`, and the UART joins as a whole.
const GOOD: &str = "    .cpu.spi ~ .flash.spi\n    .cpu.spi.sclk ~ .imu.spi.sclk\n    .cpu.spi.mosi ~ .imu.spi.mosi\n    .cpu.spi.miso ~ .imu.spi.miso\n    .cpu.sel1 ~ .imu.spi.cs\n    .cpu.uart ~ .link.uart\n";

fn good(replacing: &[(&str, &str)]) -> String {
    let mut joins = GOOD.to_string();
    for (from, to) in replacing {
        assert!(joins.contains(from), "`{from}` is not in the joins");
        joins = joins.replace(from, to);
    }
    board(SENSORS, &joins)
}

fn only(c: &Checked, message: &str) {
    assert_eq!(messages(c), [message], "{}", c.render());
}

#[test]
fn the_board_that_joins_the_buses_as_they_want_checks_clean() {
    let c = run(&good(&[]));
    assert!(c.is_ok(), "{}", c.render());
}

// ---- message 1: one line on one net -------------------------------------------------

#[test]
fn mosi_joined_to_miso_is_one_line_of_each_on_a_net() {
    let c = run(&good(&[
        (
            ".cpu.spi.mosi ~ .imu.spi.mosi",
            ".cpu.spi.mosi ~ .imu.spi.miso",
        ),
        (
            ".cpu.spi.miso ~ .imu.spi.miso",
            ".cpu.spi.miso ~ .imu.spi.mosi",
        ),
    ]));
    let m = messages(&c);
    assert_eq!(m.len(), 2, "{}", c.render());
    assert_eq!(
        m[0],
        "net `net-cpu.spi.miso` joins two different lines of a `Spi`: `miso` (`cpu.spi.miso`, `flash.spi.miso`) and `mosi` (`imu.spi.mosi`); a `Spi` joins `miso` to `miso` and `mosi` to `mosi`, and `imu.spi.mosi` is on the net of `miso`"
    );
    assert!(m[1].starts_with("net `net-cpu.spi.mosi` joins two different lines of a `Spi`: `miso` (`imu.spi.miso`) and `mosi` (`cpu.spi.mosi`, `flash.spi.mosi`)"), "{}", m[1]);
    // The rules of a net say nothing more about the nets a bus rule has reported.
    assert!(
        m.iter()
            .all(|m| !m.contains("two outputs") && !m.contains("only inputs"))
    );
}

#[test]
fn the_line_rule_needs_no_role_and_no_net_name() {
    // Parts with no role at all, on two nets that no statement names.
    let src = board(
        &[("a", "Zold"), ("b", "Zold")],
        "    .a.spi.sclk ~ .b.spi.sclk\n    .a.spi.mosi ~ .b.spi.miso\n    .a.spi.miso ~ .b.spi.cs\n    .a.spi.cs ~ .b.spi.mosi\n",
    );
    let m = messages(&run(&src));
    assert_eq!(m.len(), 3, "{m:#?}");
    assert!(
        m.iter()
            .all(|m| m.contains("joins two different lines of a `Spi`")),
        "{m:#?}"
    );
}

#[test]
fn a_net_of_two_interfaces_says_so() {
    let src = board(
        &[("cpu", "Zcpu"), ("flash", "Zflash")],
        "    .cpu.spi.sclk ~ .flash.spi.sclk\n    .cpu.uart.tx ~ .cpu.spi.mosi\n",
    );
    let m = messages(&run(&src));
    assert!(
        m.contains(&"net `net-cpu.spi.mosi` joins lines of different interfaces: `Uart` (`cpu.uart.tx`) and `Spi` (`cpu.spi.mosi`); each interface joins its own lines to the same lines of another one".to_string()),
        "{m:#?}"
    );
}

// ---- message 2: the limit ---------------------------------------------------------

#[test]
fn two_masters_on_one_bus_are_one_message_and_the_nets_say_no_more() {
    let src = board(
        &[
            ("cpu", "Zcpu"),
            ("flash", "Zflash"),
            ("imu", "Zimu"),
            ("host", "Zcpu"),
        ],
        "    .cpu.spi ~ .flash.spi\n    .cpu.spi.sclk ~ .imu.spi.sclk\n    .cpu.spi.mosi ~ .imu.spi.mosi\n    .cpu.spi.miso ~ .imu.spi.miso\n    .cpu.sel1 ~ .imu.spi.cs\n    .host.spi ~ .flash.spi\n",
    );
    let c = run(&src);
    only(
        &c,
        "`cpu.spi` and `host.spi` are both `SpiRole::Master` on one `Spi` bus, and `Spi` allows 1 `Master` on a bus: they are on one bus through `net-cpu.spi.cs` (`cpu.spi.cs`, `flash.spi.cs` and `host.spi.cs`); the slaves on it are `flash.spi` and `imu.spi`; give `host.spi` a bus of its own",
    );
}

#[test]
fn two_masters_that_share_no_net_are_on_one_bus_through_a_slave() {
    // The masters meet only on the flash: one on its clock, the other on its data in.
    let src = board(
        &[("cpu", "Zcpu"), ("host", "Zcpu"), ("flash", "Zflash")],
        "    .cpu.spi.sclk ~ .flash.spi.sclk\n    .host.spi.mosi ~ .flash.spi.mosi\n",
    );
    let m = messages(&run(&src));
    assert!(
        m.iter().any(|m| m.starts_with("`cpu.spi` and `host.spi` are both `SpiRole::Master` on one `Spi` bus, and `Spi` allows 1 `Master` on a bus: they are on one bus through the nets `net-cpu.spi.sclk` and `net-flash.spi.mosi`, which the other ends of the bus join; the slave on it is `flash.spi`; give `host.spi` a bus of its own")),
        "{m:#?}"
    );
}

// ---- message 3: a UART's lines cross ---------------------------------------------

#[test]
fn a_tx_joined_to_a_tx_and_an_rx_to_an_rx_are_found_with_the_way_to_write_them() {
    let src = board(
        &[("cpu", "Zcpu"), ("link", "Zlink")],
        "    .cpu.uart.tx ~ .link.uart.tx\n    .cpu.uart.rx ~ .link.uart.rx\n    .cpu.uart.rx ~ .link.uart.rx\n",
    );
    let m = messages(&run(&src));
    assert_eq!(
        m,
        [
            "net `net-cpu.uart.rx` joins `rx` to `rx` (`cpu.uart.rx`, `link.uart.rx`): `rx` crosses `tx` in a `Uart`, so it is joined to the other end's `tx`; write `.cpu.uart.rx ~ .link.uart.tx`, or join the two interfaces with `.cpu.uart ~ .link.uart`, which crosses `tx` to `rx` and `rts` to `cts`",
            "net `net-cpu.uart.tx` joins `tx` to `tx` (`cpu.uart.tx`, `link.uart.tx`): `tx` crosses `rx` in a `Uart`, so it is joined to the other end's `rx`; write `.cpu.uart.tx ~ .link.uart.rx`, or join the two interfaces with `.cpu.uart ~ .link.uart`, which crosses `tx` to `rx` and `rts` to `cts`",
        ]
    );
}

#[test]
fn a_third_end_of_a_uart_is_over_its_limit() {
    let src = board(
        &[("a", "Zlink"), ("b", "Zlink"), ("c", "Zlink")],
        "    .a.uart.tx ~ .b.uart.rx\n    .a.uart.tx ~ .c.uart.rx\n    .b.uart.tx ~ .a.uart.rx\n    .c.uart.tx ~ .a.uart.rx\n",
    );
    let m = messages(&run(&src));
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].starts_with("`a.uart`, `b.uart` and `c.uart` are 3 ends of one `Uart` bus, and a `Uart` has 2 ends on a bus: they are on one bus through `net-a.uart.rx`")
            && m[0].ends_with("give `c.uart` a bus of its own"),
        "{}",
        m[0]
    );
}

#[test]
fn joining_two_uarts_with_a_tilde_crosses_tx_to_rx_and_rts_to_cts() {
    let src = board(&[("a", "Zlink"), ("b", "Zlink")], "    .a.uart ~ .b.uart\n");
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    let net_of = |pin: &str| {
        let id = model.inst_at(pin).expect(pin);
        let mut names: Vec<String> = model
            .net_of(id)
            .expect("on a net")
            .terminals
            .iter()
            .map(|&t| model.path_of(t))
            .collect();
        names.sort();
        names
    };
    assert_eq!(net_of("a.uart.tx"), ["a.uart.tx", "b.uart.rx"]);
    assert_eq!(net_of("a.uart.rx"), ["a.uart.rx", "b.uart.tx"]);
    assert_eq!(net_of("a.uart.rts"), ["a.uart.rts", "b.uart.cts"]);
    assert_eq!(net_of("a.uart.cts"), ["a.uart.cts", "b.uart.rts"]);
}

#[test]
fn two_uarts_without_flow_control_join_with_a_tilde_and_leave_rts_and_cts_alone() {
    let src = board(&[("a", "Zbare"), ("b", "Zbare")], "    .a.uart ~ .b.uart\n");
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_uart_joined_pin_by_pin_crossed_is_as_good_as_one_joined_with_a_tilde() {
    let src = board(
        &[("a", "Zbare"), ("b", "Zbare")],
        "    .a.uart.tx ~ .b.uart.rx\n    .b.uart.tx ~ .a.uart.rx\n",
    );
    assert!(run(&src).is_ok());
}

// ---- messages 5 and 6 of the shared line: tri_state with the roles -------------------

const SHARED_MISO: &str = "    .cpu.spi ~ .a.spi\n    .cpu.spi.sclk ~ .b.spi.sclk\n    .cpu.spi.mosi ~ .b.spi.mosi\n    .cpu.spi.miso ~ .b.spi.miso\n    .cpu.sel1 ~ .b.spi.cs\n";

#[test]
fn two_slaves_whose_miso_is_an_output_get_the_advice_about_tri_state() {
    let c = run(&board(
        &[("cpu", "Zcpu"), ("a", "Zgyro"), ("b", "Zgyro")],
        SHARED_MISO,
    ));
    only(
        &c,
        "net `net-a.spi.miso` has two outputs: `a.spi.miso`, `b.spi.miso`; a line that several parts share needs every driver to be `tri_state`",
    );
}

#[test]
fn a_slave_with_an_output_beside_a_tri_state_slave_is_message_5() {
    let c = run(&board(
        &[("cpu", "Zcpu"), ("a", "Zimu"), ("b", "Zgyro")],
        SHARED_MISO,
    ));
    only(
        &c,
        "net `net-a.spi.miso` has 2 drivers and `b.spi.miso` is not `tri_state`: `a.spi.miso` (`Zimu`, a `tri_state` output) and `b.spi.miso` (`Zgyro`, an output); it would drive the net while another driver on it is selected; the datasheet of `Zgyro` must say that the pin is high impedance while its part is not selected (then write `tri_state` for it, with the page), or give `b.spi.miso` a net of its own",
    );
}

#[test]
fn two_tri_state_slaves_share_a_miso() {
    let c = run(&board(
        &[("cpu", "Zcpu"), ("a", "Zimu"), ("b", "Zflash")],
        SHARED_MISO,
    ));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn two_outputs_of_parts_with_no_role_keep_the_message_they_had() {
    let c = run(&board(
        &[("a", "Zlink"), ("b", "Zlink")],
        "    .a.uart.tx ~ .a.uart.rx\n    .b.uart.tx ~ .a.uart.rx\n",
    ));
    // A pin by pin join of two `tx`: the `Uart` rule says it, the net rules say no more.
    let m = messages(&c);
    assert!(m.iter().all(|m| !m.contains("tri_state")), "{m:#?}");
}

// ---- message 7: the select of a slave ------------------------------------------

#[test]
fn the_selects_of_two_slaves_on_one_net_are_refused() {
    let c = run(&good(&[(
        ".cpu.sel1 ~ .imu.spi.cs",
        ".imu.spi.cs ~ .flash.spi.cs",
    )]));
    only(
        &c,
        "net `net-cpu.spi.cs` carries the `cs` of two slaves, `flash.spi.cs` and `imu.spi.cs`: `Spi` gives each slave a `cs` of its own (`cs` is `single` for `SpiRole::Slave`); join `.imu.spi.cs` to a pin of its own",
    );
}

#[test]
fn a_masters_select_with_one_slaves_on_a_net_is_accepted() {
    // `GOOD` already has the master's `cs` with the flash's `cs`.
    assert!(run(&good(&[])).is_ok());
}

// ---- message 8: the table against the role ---------------------------------------

#[test]
fn a_row_that_contradicts_the_role_is_refused_with_the_datasheet_name() {
    let src = good(&[]).replace(
        "        .spi.mosi input     \"SDI\" \"2\"\n        // Table 14",
        "        .spi.mosi output    \"SDI\" \"2\"\n        // Table 14",
    );
    let c = run(&src);
    let m = messages(&c);
    assert_eq!(
        m[0],
        "`imu.spi.mosi` is an output in the table of `Zimu`, but `SpiRole::Slave` makes `mosi` an input of `Spi`; either the datasheet's `SDI` pin is not an output, or the role is the other one"
    );
}

#[test]
fn bidirectional_passive_and_a_tri_state_output_agree_with_a_role() {
    // The flash states `tri_state` for an `output` line, and `bidirectional` and `passive`
    // contradict nothing.
    let src = board(
        &[("cpu", "Zcpu"), ("old", "Zold")],
        "    .cpu.spi ~ .old.spi\n",
    )
    .replace(
        ".spi: Spi\n\n    pins {\n        .vdd.pos  power         \"VDD\" \"8\"",
        ".spi: Spi\n    .spi.role &= SpiRole::Slave\n\n    pins {\n        .vdd.pos  power         \"VDD\" \"8\"",
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
}

// ---- message 9: a line the role requires ---------------------------------------

#[test]
fn a_slaves_data_out_left_open_is_refused_when_the_bus_is_used() {
    let c = run(&good(&[(
        ".cpu.spi.miso ~ .imu.spi.miso",
        ".cpu.spi.sclk ~ .imu.spi.sclk",
    )]));
    // `miso` of the IMU is on no net but the master's `miso` is open: the slave's is required.
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m == "`imu.spi.miso` is a tri_state output that must be connected (pad 3, `SDO`): `SpiRole::Slave` of `Spi` requires it, and nothing else is on its net"),
        "{m:#?}"
    );
}

#[test]
fn an_interface_that_no_line_of_is_connected_is_left_alone() {
    let src = board(
        &[("cpu", "Zcpu"), ("flash", "Zflash")],
        "    .cpu.uart.tx ~ .cpu.uart.tx\n",
    );
    let m = messages(&run(&src));
    // The flash's bus is not used, so its required `miso` is not asked for (its other pins
    // are inputs that must be connected, as they always were).
    assert!(m.iter().all(|m| !m.contains("requires it")), "{m:#?}");
}

// ---- parts that state no role --------------------------------------------------

#[test]
fn a_part_that_states_no_role_is_not_judged_by_the_table_and_the_limit() {
    // Two role-less sensors and a master: the table and the limit say nothing about the
    // sensors; their `bidirectional` pins on one net are what they were.
    let src = board(
        &[("cpu", "Zcpu"), ("a", "Zold"), ("b", "Zold")],
        "    .cpu.spi ~ .a.spi\n    .a.spi ~ .b.spi\n",
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_bus_where_one_part_states_its_role_and_another_does_not_gets_a_note() {
    let src = board(
        &[("cpu", "Zcpu"), ("old", "Zold")],
        "    .cpu.spi ~ .old.spi\n",
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    let notes: Vec<String> = unstated_roles(model).into_iter().map(|(_, m)| m).collect();
    assert_eq!(
        notes,
        [
            "not evaluated: the role of `old.spi` is not stated, so the limit and the table of `old` are not judged"
        ]
    );
    // A bus on which no part states a role gets no note.
    let src = board(&[("a", "Zold"), ("b", "Zold")], "    .a.spi ~ .b.spi\n");
    let c = run(&src);
    assert!(unstated_roles(&c.board("B").unwrap().model).is_empty());
}

#[test]
fn an_interface_a_part_declares_and_the_board_never_joins_is_left_alone() {
    let src = board(&[("old", "Zold")], "");
    let c = run(&src);
    let m = messages(&c);
    assert!(m.iter().all(|m| !m.contains("role")), "{m:#?}");
    assert!(unstated_roles(&c.board("B").unwrap().model).is_empty());
}

// ---- a bidirectional row against a role (the guideline) -------------------------------

#[test]
fn a_bidirectional_row_where_the_role_gives_a_direction_is_advice_and_not_an_error() {
    let src = board(&[("cpu", "Zcpu"), ("old", "Zold")], "    .cpu.spi ~ .old.spi\n").replace(
        ".spi: Spi\n\n    pins {\n        .vdd.pos  power         \"VDD\" \"8\"",
        ".spi: Spi\n    .spi.role &= SpiRole::Slave\n\n    pins {\n        .vdd.pos  power         \"VDD\" \"8\"",
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    let rows = bidirectional_rows(&c.board("B").unwrap().model);
    let shown: Vec<String> = rows.into_iter().map(|(_, m)| m).collect();
    assert_eq!(shown.len(), 4, "{shown:#?}");
    assert!(
        shown.contains(&"`old.spi.miso` is `bidirectional` in the table of `Zold`, and `SpiRole::Slave` makes `miso` an output: write `output` or `tri_state`, as the datasheet says, unless the datasheet says the pin also receives".to_string()),
        "{shown:#?}"
    );
}

// ---- a joined interface that states no role (the guideline of stage 5) ------------------

fn unstated(c: &Checked) -> Vec<String> {
    joined_without_role(&c.board("B").unwrap().model)
        .into_iter()
        .map(|(_, m)| m)
        .collect()
}

#[test]
fn a_joined_interface_that_states_no_role_is_advice_and_not_an_error() {
    let src = board(
        &[("cpu", "Zcpu"), ("old", "Zold")],
        "    .cpu.spi ~ .old.spi\n",
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        unstated(&c),
        [
            "`old.spi` is joined to another end of a `Spi` bus and states no role, so the limit and the table of `old` are not judged: write `.spi.role &=` `SpiRole::Master` or `SpiRole::Slave`, as its datasheet says"
        ]
    );
}

#[test]
fn two_joined_interfaces_that_state_no_role_are_each_advised() {
    let src = board(&[("a", "Zold"), ("b", "Zold")], "    .a.spi ~ .b.spi\n");
    let c = run(&src);
    let found = unstated(&c);
    assert_eq!(found.len(), 2, "{found:#?}");
    assert!(found[0].starts_with("`a.spi` is joined"), "{found:#?}");
    assert!(found[1].starts_with("`b.spi` is joined"), "{found:#?}");
}

#[test]
fn an_interface_joined_pin_by_pin_is_joined_all_the_same() {
    let src = board(
        &[("cpu", "Zcpu"), ("old", "Zold")],
        "    .cpu.spi.sclk ~ .old.spi.sclk\n",
    );
    assert_eq!(unstated(&run(&src)).len(), 1);
}

#[test]
fn an_interface_alone_on_its_bus_a_bus_with_no_roles_and_a_stated_role_are_not_advised() {
    // Alone: no net joins it to another end.
    let c = run(&board(&[("old", "Zold")], ""));
    assert!(unstated(&c).is_empty());
    // A `Uart` has no roles to state.
    let c = run(&good(&[]));
    assert!(unstated(&c).is_empty(), "{:#?}", unstated(&c));
    // Every end states its role.
    let c = run(&board(
        &[("cpu", "Zcpu"), ("flash", "Zflash")],
        "    .cpu.spi ~ .flash.spi\n",
    ));
    assert!(unstated(&c).is_empty());
}

// ---- the role is not a value that a bus shares --------------------------------------

#[test]
fn a_master_joined_to_a_slave_with_a_tilde_builds_because_the_role_is_not_equated() {
    let c = run(&good(&[]));
    assert!(c.is_ok(), "{}", c.render());
}

// ---- order of statements never matters ----------------------------------------------

#[test]
fn the_order_of_the_joins_changes_nothing() {
    let reversed: String = {
        let mut lines: Vec<&str> = GOOD.lines().collect();
        lines.reverse();
        lines.join("\n") + "\n"
    };
    let bad_a = good(&[(
        ".cpu.spi.mosi ~ .imu.spi.mosi",
        ".cpu.spi.mosi ~ .imu.spi.miso",
    )]);
    let mut bad_b = board(SENSORS, &reversed);
    bad_b = bad_b.replace(
        ".cpu.spi.mosi ~ .imu.spi.mosi",
        ".cpu.spi.mosi ~ .imu.spi.miso",
    );
    let (a, b) = (messages(&run(&bad_a)), messages(&run(&bad_b)));
    assert!(!a.is_empty());
    assert_eq!(a, b);
}

#[test]
fn a_role_stated_by_the_board_gives_the_same_messages_as_one_the_part_states() {
    let stated_by_part = good(&[]);
    let by_board = stated_by_part
        .replace("    .spi.role &= SpiRole::Master\n", "")
        .replace(
            "    .cpu: Zcpu\n",
            "    .cpu: Zcpu\n    .cpu.spi.role &= SpiRole::Master\n",
        );
    let (a, b) = (run(&stated_by_part), run(&by_board));
    assert_eq!(messages(&a), messages(&b));
}

// ---- modules and generics ------------------------------------------------------------

#[test]
fn a_module_that_forwards_a_bus_is_a_way_through() {
    let src = format!(
        "{PARTS}\nmodule Flash {{\n    .supply: Rail\n    .bus: Spi\n    .chip: Zflash\n    .supply ~ .chip.vdd\n    .bus ~ .chip.spi\n    .bus.role == .chip.spi.role\n}}\n\nboard B {{\n{SUPPLY}    .cpu: Zcpu\n    .supply ~ .cpu.vdd\n    .f: Flash\n    .supply ~ .f.supply\n    .cpu.spi ~ .f.bus\n}}\n"
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_generic_may_state_the_role_and_a_tri_state_contract_row() {
    let src = format!(
        "{PARTS}\ngeneric SerialFlash {{\n    .vdd: Rail\n    .spi: Spi\n    .spi.role &= SpiRole::Slave\n    pins {{\n        .spi.miso tri_state\n    }}\n}}\n\npart Zrom provides SerialFlash {{\n    pins {{\n        .vdd.pos  power     \"VCC\" \"8\"\n        .vdd.neg  power     \"GND\" \"4\"\n        .spi.sclk input     \"CLK\" \"6\"\n        .spi.mosi input     \"DI\"  \"5\"\n        .spi.miso tri_state \"DO\"  \"2\"\n        .spi.cs   input     \"/CS\" \"1\"\n    }}\n}}\n\nboard B {{\n{SUPPLY}    .cpu: Zcpu\n    .supply ~ .cpu.vdd\n    .mem: SerialFlash\n    .supply ~ .mem.vdd\n    .cpu.spi ~ .mem.spi\n}}\n"
    );
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    // The role of the generic reached the use.
    assert_eq!(
        model.domain_at("mem.spi.role"),
        Some(&Domain::Text("Slave".to_string()))
    );
}

// ---- the declaration -----------------------------------------------------------------

fn lines_error(items: &str) -> Vec<String> {
    let src = format!(
        "enum ZRole {{ A, B }}\ninterface Zbus {{\n    .role: ZRole\n    .x: Pin\n    .y: Pin\n{items}}}\npart P {{\n    .bus: Zbus\n    pins {{\n        .bus.x input \"X\" \"1\"\n    }}\n}}\nboard Bd {{\n    .p: P\n}}\n"
    );
    messages(&run(&src))
}

#[test]
fn lines_belong_directly_in_an_interface() {
    let c = run("part P {\n    lines {\n        .a input\n    }\n}\n");
    only(
        &c,
        "`lines` can only appear directly inside an interface: it states the roles of a bus",
    );
}

#[test]
fn a_role_must_be_a_variant_of_the_enum_of_the_interfaces_role() {
    let m = lines_error("    lines ZRole::C {\n        .x input\n    }\n");
    assert!(m[0].starts_with("`ZRole` has no variant `C`"), "{m:#?}");
    let m = lines_error(
        "    lines ZRole::A {\n        .x input\n    }\n    lines {\n        .y input\n    }\n",
    );
    assert!(
        m.iter()
            .any(|m| m.contains("each `lines` names the role it is for, such as `lines ZRole::A`")),
        "{m:#?}"
    );
}

#[test]
fn a_row_names_a_pin_of_the_interface_once() {
    let m = lines_error("    lines ZRole::A {\n        .z input\n    }\n");
    assert!(
        m[0].contains("`.z` is not a pin of this interface; its pins are `.x` and `.y`"),
        "{m:#?}"
    );
    let m = lines_error("    lines ZRole::A {\n        .x input\n        .x output\n    }\n");
    assert!(
        m.iter().any(|m| m.contains("`.x` is listed twice")),
        "{m:#?}"
    );
    let m = lines_error("    lines ZRole::A {\n        .x power\n    }\n");
    assert!(
        m.iter().any(|m| m.contains("a line of a bus is a signal")),
        "{m:#?}"
    );
}

#[test]
fn a_role_has_one_table_and_a_crossing_is_one_pair() {
    let m = lines_error(
        "    lines ZRole::A {\n        .x input\n    }\n    lines ZRole::A {\n        .y input\n    }\n",
    );
    assert!(
        m.iter()
            .any(|m| m.contains("`ZRole::A` already has a `lines` table")),
        "{m:#?}"
    );
    let m = lines_error(
        "    lines ZRole::A {\n        .x output crosses .y\n        .y input crosses .x\n    }\n",
    );
    assert!(
        m.iter().all(|m| !m.contains("crosses")),
        "the same pair written twice is accepted: {m:#?}"
    );
    let m = lines_error("    lines ZRole::A {\n        .x output crosses .x\n    }\n");
    assert!(
        m.iter().any(|m| m.contains("cannot cross itself")),
        "{m:#?}"
    );
}

#[test]
fn an_interface_with_no_role_member_takes_lines_with_no_role_only() {
    let src = "interface Zbus {\n    .x: Pin\n    lines ZRole::A {\n        .x input\n    }\n}\nenum ZRole { A }\nboard B {\n    .b: Zbus\n}\n";
    let m = messages(&run(src));
    assert!(
        m.iter()
            .any(|m| m.contains("has no `.role` to hold a role")),
        "{m:#?}"
    );
}

// ---- the I2C roles ---------------------------------------------------------------------

#[test]
fn i2c_lines_are_bidirectional_for_a_controller_and_a_target() {
    let src = format!(
        "{PARTS}\npart Zctl {{\n    .vdd: Rail\n    .i2c: I2c\n    .i2c.role &= I2cRole::Controller\n    pins {{\n        .vdd.pos power \"VDD\" \"1\"\n        .vdd.neg power \"GND\" \"2\"\n        .i2c.sda bidirectional \"SDA\" \"3\"\n        .i2c.scl output \"SCL\" \"4\"\n    }}\n}}\n\npart Ztgt {{\n    .vdd: Rail\n    .i2c: I2c\n    .i2c.role &= I2cRole::Target\n    pins {{\n        .vdd.pos power \"VDD\" \"1\"\n        .vdd.neg power \"GND\" \"2\"\n        .i2c.sda bidirectional \"SDA\" \"3\"\n        .i2c.scl input \"SCL\" \"4\"\n    }}\n}}\n\nboard B {{\n{SUPPLY}    .c: Zctl\n    .t: Ztgt\n    .supply ~ .c.vdd\n    .supply ~ .t.vdd\n    .c.i2c ~ .t.i2c\n    .r: std::bus::I2cPullups {{ .resistance &= 4.7kohm +/- 5% }}\n    .supply ~ .r.supply\n    .c.i2c ~ .r.bus\n}}\n"
    );
    let src = src
        .replace(
            "import std::bus::{I2c, I2cRole, Spi, SpiRole, Uart}",
            "import std::bus::{I2c, I2cPullups, I2cRole, Spi, SpiRole, Uart}",
        )
        .replace("std::bus::I2cPullups {", "I2cPullups {");
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
}
