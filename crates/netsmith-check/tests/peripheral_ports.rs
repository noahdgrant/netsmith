//! `.u.UARTE0 ~ .link.uart`: a peripheral that is an end of a bus, joined as a port (ADR 0129).
//! The parts are invented for these tests.

use std::collections::BTreeSet;

use netsmith_check::*;

const PARTS: &str = r#"
import std::bus::{I2c, I2cRole, Spi, SpiRole, Uart}
import std::sources::Rail

/// Zmcu, an invented processor: two UARTs and an SPI master on any pin, an I2C controller on
/// two pins of its own, and a UART that names no bus.
part Zmcu {
    .vdd: Rail

    peripheral UARTE0 on APP_GPIO as Uart {
        UARTE0_TX:output as tx
        UARTE0_RX:input  as rx
    }
    peripheral UARTE1 on APP_GPIO as Uart {
        UARTE1_TX:output as tx
        UARTE1_RX:input  as rx
    }
    peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master {
        SPIM0_SCK:output  as sclk
        SPIM0_MOSI:output as mosi
        SPIM0_MISO:input  as miso
        SPIM0_CSN:output  as cs
    }
    peripheral TWI0 as I2c I2cRole::Controller {
        TWI0_SCL as scl
        TWI0_SDA as sda
    }
    peripheral PLAIN on APP_GPIO { PLAIN_TX:output PLAIN_RX:input }

    pins {
        .vdd.pos power         "VDD"   "1"
        .vdd.neg power         "VSS"   "2"
        .p00     bidirectional "P0.00" "3"  fn APP_GPIO
        .p01     bidirectional "P0.01" "4"  fn APP_GPIO
        .p02     bidirectional "P0.02" "5"  fn APP_GPIO
        .p03     bidirectional "P0.03" "6"  fn APP_GPIO
        .p04     bidirectional "P0.04" "7"  fn APP_GPIO TWI0_SCL
        .p05     bidirectional "P0.05" "8"  fn APP_GPIO TWI0_SDA
        .p06     bidirectional "P0.06" "9"  fn APP_GPIO TWI0_SCL
    }
}

/// Zlink, an invented UART peer with flow control.
part Zlink {
    .vdd: Rail
    .uart: Uart
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .uart.tx  output "TX"  "3"
        .uart.rx  input  "RX"  "4"
        .uart.rts output "RTS" "5"
        .uart.cts input  "CTS" "6" optional
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

/// Zrom, an invented I2C target.
part Zrom {
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    pins {
        .vdd.pos power         "VCC" "8"
        .vdd.neg power         "GND" "4"
        .i2c.sda bidirectional "SDA" "5"
        .i2c.scl input         "SCL" "6"
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

/// A board with `parts` (each on the supply) and `body`.
fn board(parts: &[(&str, &str)], body: &str) -> String {
    let mut decl = String::new();
    for (name, ty) in parts {
        decl.push_str(&format!("    .{name}: {ty}\n    .supply ~ .{name}.vdd\n"));
    }
    format!("{PARTS}\nboard B {{\n{SUPPLY}{decl}{body}\n}}\n")
}

/// The nets of board `B`, each as the paths of its terminals.
fn nets(c: &Checked) -> BTreeSet<BTreeSet<String>> {
    let m = &c.board("B").expect("the board is there").model;
    m.nets()
        .iter()
        .map(|n| n.terminals.iter().map(|&t| m.path_of(t)).collect())
        .collect()
}

/// The names of the nets of board `B`.
fn net_names(c: &Checked) -> BTreeSet<String> {
    let m = &c.board("B").expect("the board is there").model;
    m.nets().iter().map(|n| n.name.clone()).collect()
}

fn same_nets(by_port: &str, by_hand: &str) {
    let a = run(by_port);
    assert!(a.is_ok(), "{}", a.render());
    let b = run(by_hand);
    assert!(b.is_ok(), "{}", b.render());
    assert_eq!(nets(&a), nets(&b));
    assert_eq!(net_names(&a), net_names(&b));
}

const UART_FUNCTIONS: &str =
    "    .u.p00.function &= UARTE0_TX\n    .u.p01.function &= UARTE0_RX\n";

#[test]
fn a_uart_peripheral_joined_to_a_uart_crosses_its_lines_as_the_pins_did() {
    let parts = [("u", "Zmcu"), ("link", "Zlink")];
    same_nets(
        &board(
            &parts,
            &format!("{UART_FUNCTIONS}    .u.UARTE0 ~ .link.uart\n"),
        ),
        &board(
            &parts,
            &format!("{UART_FUNCTIONS}    .u.p00 ~ .link.uart.rx\n    .u.p01 ~ .link.uart.tx\n"),
        ),
    );
}

#[test]
fn the_join_reads_the_functions_wherever_they_are_stated() {
    // The functions after the join: the pins are joined once every statement is read.
    let parts = [("u", "Zmcu"), ("link", "Zlink")];
    same_nets(
        &board(
            &parts,
            &format!("    .link.uart ~ .u.UARTE0\n{UART_FUNCTIONS}"),
        ),
        &board(
            &parts,
            &format!("{UART_FUNCTIONS}    .u.p00 ~ .link.uart.rx\n    .u.p01 ~ .link.uart.tx\n"),
        ),
    );
}

#[test]
fn an_i2c_controller_on_its_own_pins_joins_the_targets_bus_and_the_pull_ups_rule_reads_it() {
    let parts = [("u", "Zmcu"), ("rom", "Zrom")];
    let body = "    .u.p04.function &= TWI0_SCL\n    .u.p05.function &= TWI0_SDA\n    .u.TWI0 ~ .rom.i2c\n";
    let m = messages(&run(&board(&parts, body)));
    assert!(
        m.iter()
            .any(|m| m.starts_with("net `net-rom.i2c.scl` carries the scl line of an I2C bus")),
        "the bus has no pull-up and the rule says so: {m:#?}"
    );
    let with_pullups = format!(
        "{body}    .pullups: I2cPullups {{ .resistance &= 4.7kohm +/- 5% }}\n    .pullups.supply ~ .supply\n    .pullups.bus.levels.reference ~ .supply\n    .pullups.bus ~ .rom.i2c\n"
    );
    let src = board(&parts, &with_pullups)
        .replace("import std::sources::Rail", "import std::sources::Rail\nimport std::bus::I2cPullups");
    let c = run(&src);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn an_spi_masters_optional_select_left_unchosen_joins_nothing() {
    // The select reaches the flash by another way, so the board does not choose SPIM0_CSN, and
    // the master's `cs` is optional in `Spi`: the three other lines join, the select is the
    // board's.
    let parts = [("u", "Zmcu"), ("flash", "Zflash")];
    let functions = "    .u.p00.function &= SPIM0_SCK\n    .u.p01.function &= SPIM0_MOSI\n    .u.p02.function &= SPIM0_MISO\n    .u.p03.function &= APP_GPIO\n    .u.p03 ~ .flash.spi.cs\n";
    same_nets(
        &board(&parts, &format!("{functions}    .u.SPIM0 ~ .flash.spi\n")),
        &board(
            &parts,
            &format!("{functions}    .u.p00 ~ .flash.spi.sclk\n    .u.p01 ~ .flash.spi.mosi\n    .u.p02 ~ .flash.spi.miso\n"),
        ),
    );
}

#[test]
fn a_line_whose_function_no_pin_has_chosen_is_refused_with_the_pins_that_offer_it() {
    let parts = [("u", "Zmcu"), ("link", "Zlink")];
    let m = messages(&run(&board(
        &parts,
        "    .u.p00.function &= UARTE0_TX\n    .u.UARTE0 ~ .link.uart\n",
    )));
    assert!(
        m.contains(&"`.u.UARTE0 ~ .link.uart` joins the line `rx` of `UARTE0`, but no pin of `u` is `UARTE0_RX`: choose it on a pin that offers it and has no function chosen, `u.p01`, `u.p02`, `u.p03`, `u.p04` and 2 more (`.u.p01.function &= UARTE0_RX`)".to_string()),
        "{m:#?}"
    );
    // A function on one pin names that pin.
    let parts = [("u", "Zmcu"), ("rom", "Zrom")];
    let m = messages(&run(&board(
        &parts,
        "    .u.p04.function &= TWI0_SCL\n    .u.TWI0 ~ .rom.i2c\n",
    )));
    assert!(
        m.contains(&"`.u.TWI0 ~ .rom.i2c` joins the line `sda` of `TWI0`, but no pin of `u` is `TWI0_SDA`: choose it on the pin that carries it, `.u.p05.function &= TWI0_SDA`".to_string()),
        "{m:#?}"
    );
}

#[test]
fn two_uart_peripherals_join_each_other_and_cross() {
    let parts = [("u", "Zmcu"), ("v", "Zmcu")];
    let functions = "    .u.p00.function &= UARTE0_TX\n    .u.p01.function &= UARTE0_RX\n    .v.p02.function &= UARTE1_TX\n    .v.p03.function &= UARTE1_RX\n";
    same_nets(
        &board(&parts, &format!("{functions}    .u.UARTE0 ~ .v.UARTE1\n")),
        &board(
            &parts,
            &format!("{functions}    .u.p00 ~ .v.p03\n    .u.p01 ~ .v.p02\n"),
        ),
    );
}

#[test]
fn the_other_side_must_be_an_end_of_the_same_bus() {
    let parts = [("u", "Zmcu"), ("flash", "Zflash"), ("link", "Zlink")];
    let m = messages(&run(&board(
        &parts,
        &format!("{UART_FUNCTIONS}    .u.UARTE0 ~ .flash.spi\n"),
    )));
    assert!(
        m.contains(&"`~` joins the peripheral `UARTE0`, an end of a `Uart`, to an interface of that kind or to another such peripheral; `.flash.spi` is a `Spi`".to_string()),
        "{m:#?}"
    );
    let m = messages(&run(&board(
        &parts,
        &format!("{UART_FUNCTIONS}    .u.UARTE0 ~ .link.uart.rx\n"),
    )));
    assert!(
        m.contains(&"`~` joins the peripheral `UARTE0`, an end of a `Uart`, to an interface of that kind or to another such peripheral; `.link.uart.rx` is a pin".to_string()),
        "{m:#?}"
    );
    let m = messages(&run(&board(
        &parts,
        &format!("{UART_FUNCTIONS}    .u.UARTE0 ~ .u.SPIM0\n"),
    )));
    assert!(
        m.contains(&"cannot connect `.u.UARTE0`, an end of a `Uart`, to `.u.SPIM0`, an end of a `Spi`".to_string()),
        "{m:#?}"
    );
}

#[test]
fn a_peripheral_that_names_no_bus_is_not_a_port() {
    let parts = [("u", "Zmcu"), ("link", "Zlink")];
    let m = messages(&run(&board(&parts, "    .u.PLAIN ~ .link.uart\n")));
    assert!(
        m.contains(&"`PLAIN` is a peripheral of `Zmcu` that names no bus, so `~` cannot join it as one; the part says which bus it is an end of with `peripheral PLAIN as Uart { ... }` (ADR 0064), or the board joins its pins".to_string()),
        "{m:#?}"
    );
}

#[test]
fn a_peripheral_in_a_chain_or_a_value_is_no_member_and_the_message_says_how_it_joins() {
    let parts = [("u", "Zmcu"), ("link", "Zlink"), ("other", "Zlink")];
    let m = messages(&run(&board(
        &parts,
        &format!("{UART_FUNCTIONS}    .u.UARTE0 ~ .link.uart ~ .other.uart\n"),
    )));
    assert!(
        m.contains(&"`UARTE0` is a peripheral of `Zmcu`, not a member: a peripheral that is an end of a bus is joined alone, `~` with an interface of its bus (`.u.UARTE0 ~ .other.bus`, ADR 0129), and its pins are chosen with `.function &=`".to_string()),
        "{m:#?}"
    );
}
