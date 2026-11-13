//! The bus clause of a peripheral (ADR 0064, stage 4): the master of an any-pin processor is an
//! end of a bus, so the rules of the bus read it: a line on a line, the limit of masters, the
//! select of one slave, the crossing of a UART.
//!
//! The parts are invented for these tests.

use netsmith_check::*;

const PARTS: &str = r#"
import std::bus::{Spi, SpiRole, Uart}
import std::sources::Rail

/// Zmcu, an invented processor whose serial units reach any pin of the application core.
part Zmcu {
    .vdd: Rail

    peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master {
        SPIM0_SCK:output  as sclk
        SPIM0_MOSI:output as mosi
        SPIM0_MISO:input  as miso
    }
    peripheral SPIM1 on APP_GPIO as Spi SpiRole::Master {
        SPIM1_SCK:output  as sclk
        SPIM1_MOSI:output as mosi
        SPIM1_MISO:input  as miso
    }
    peripheral SPIS0 on APP_GPIO as Spi SpiRole::Slave {
        SPIS0_SCK:input       as sclk
        SPIS0_MOSI:input      as mosi
        SPIS0_MISO:tri_state  as miso
        SPIS0_CSN:input       as cs
    }
    peripheral UARTE0 on APP_GPIO as Uart {
        UARTE0_TX:output as tx
        UARTE0_RX:input  as rx
    }
    peripheral UARTE1 on APP_GPIO as Uart {
        UARTE1_TX:output as tx
        UARTE1_RX:input  as rx
    }
    peripheral UARTE2 on APP_GPIO as Uart {
        UARTE2_TX:output as tx
        UARTE2_RX:input  as rx
    }

    pins {
        .vdd.pos power         "VDD"   "1"
        .vdd.neg power         "VSS"   "2"
        .p00     bidirectional "P0.00" "3"  fn APP_GPIO
        .p01     bidirectional "P0.01" "4"  fn APP_GPIO
        .p02     bidirectional "P0.02" "5"  fn APP_GPIO
        .p03     bidirectional "P0.03" "6"  fn APP_GPIO
        .p04     bidirectional "P0.04" "7"  fn APP_GPIO
        .p05     bidirectional "P0.05" "8"  fn APP_GPIO
        .p06     bidirectional "P0.06" "9"  fn APP_GPIO
        .p07     bidirectional "P0.07" "10" fn APP_GPIO
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

/// Zhost, an invented processor with a fixed SPI master and a UART.
part Zhost {
    .vdd: Rail
    .spi: Spi
    .uart: Uart
    .spi.role &= SpiRole::Master

    pins {
        .vdd.pos  power  "VDD"  "1"
        .vdd.neg  power  "VSS"  "2"
        .spi.sclk output "SCK"  "3"
        .spi.mosi output "MOSI" "4"
        .spi.miso input  "MISO" "5"
        .spi.cs   output "CS"   "6"
        .uart.tx  output "TXD"  "8"
        .uart.rx  input  "RXD"  "9" optional
    }
}

/// Zlink, an invented UART peer.
part Zlink {
    .vdd: Rail
    .uart: Uart
    pins {
        .vdd.pos power  "VDD" "1"
        .vdd.neg power  "GND" "2"
        .uart.tx output "TX"  "3"
        .uart.rx input  "RX"  "4"
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

/// The master of the any-pin processor on SPIM0, joined to a flash the way a board does it: the
/// functions are chosen on pins, and the pins are joined one at a time.
const MASTER: &str = "    .u.p03.function &= APP_GPIO\n    .u.p00.function &= SPIM0_SCK\n    .u.p01.function &= SPIM0_MOSI\n    .u.p02.function &= SPIM0_MISO\n    .u.p00 ~ .flash.spi.sclk\n    .u.p01 ~ .flash.spi.mosi\n    .u.p02 ~ .flash.spi.miso\n    .u.p03 ~ .flash.spi.cs\n";

fn with_master(replacing: &[(&str, &str)]) -> String {
    let mut body = MASTER.to_string();
    for (from, to) in replacing {
        assert!(body.contains(from), "`{from}` is not in the body");
        body = body.replace(from, to);
    }
    board(&[("u", "Zmcu"), ("flash", "Zflash")], &body)
}

#[test]
fn an_any_pin_master_joined_to_a_slave_checks_clean() {
    let c = run(&with_master(&[]));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn mosi_of_the_any_pin_master_on_the_miso_of_a_slave_is_found() {
    // The master's `mosi` pin and `miso` pin swapped on the slave's `miso` and `mosi`.
    let c = run(&with_master(&[
        (".u.p01 ~ .flash.spi.mosi", ".u.p01 ~ .flash.spi.miso"),
        (".u.p02 ~ .flash.spi.miso", ".u.p02 ~ .flash.spi.mosi"),
    ]));
    let m = messages(&c);
    assert_eq!(m.len(), 2, "{}", c.render());
    assert_eq!(
        m[0],
        "net `net-flash.spi.miso` joins two different lines of a `Spi`: `miso` (`flash.spi.miso`) and `mosi` (`u.p01`); a `Spi` joins `miso` to `miso` and `mosi` to `mosi`, and `u.p01` is on the net of `miso`"
    );
}

#[test]
fn the_master_function_chosen_on_a_pin_is_the_statement_that_puts_it_on_the_bus() {
    // The same joins with no function chosen: the pins are plain pins, and no rule reads them
    // as a bus. (The processor's pins that have several functions need one chosen.)
    let src = with_master(&[]).replace("    .u.p00.function &= SPIM0_SCK\n    .u.p01.function &= SPIM0_MOSI\n    .u.p02.function &= SPIM0_MISO\n", "    .u.p00.function &= APP_GPIO\n    .u.p01.function &= APP_GPIO\n    .u.p02.function &= APP_GPIO\n");
    let c = run(&src);
    let m = messages(&c);
    assert!(
        m.iter().all(|m| !m.contains("Spi")),
        "plain pins are not a bus: {m:#?}"
    );
}

#[test]
fn two_masters_one_of_them_a_peripheral_are_one_message_naming_both() {
    let src = board(
        &[("u", "Zmcu"), ("host", "Zhost"), ("flash", "Zflash")],
        &format!("{MASTER}    .host.spi ~ .flash.spi\n"),
    );
    let c = run(&src);
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{}", c.render());
    assert!(
        m[0].starts_with("`host.spi` and `u` (`SPIM0`) are both `SpiRole::Master` on one `Spi` bus, and `Spi` allows 1 `Master` on a bus: they are on one bus through `net-"),
        "{}",
        m[0]
    );
    assert!(
        m[0].ends_with("the slave on it is `flash.spi`; give `u` (`SPIM0`) a bus of its own"),
        "{}",
        m[0]
    );
}

#[test]
fn two_peripherals_of_one_part_on_one_bus_are_two_masters() {
    // SPIM0 and SPIM1 both on the flash's lines.
    let src = board(
        &[("u", "Zmcu"), ("flash", "Zflash")],
        &format!("{MASTER}    .u.p04.function &= SPIM1_SCK\n    .u.p04 ~ .flash.spi.sclk\n"),
    );
    let m = messages(&run(&src));
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("`u` (`SPIM0`) and `u` (`SPIM1`) are both `SpiRole::Master`"),
        "{}",
        m[0]
    );
}

#[test]
fn the_selects_of_two_slave_units_on_one_net_are_refused() {
    let src = board(
        &[("u", "Zmcu"), ("v", "Zmcu")],
        "    .u.p00.function &= SPIS0_CSN\n    .v.p00.function &= SPIS0_CSN\n    .u.p00 ~ .v.p00\n",
    );
    let m = messages(&run(&src));
    assert!(
        m.iter()
            .any(|m| m.contains("carries the `cs` of two slaves, `u.p00` and `v.p00`")),
        "{m:#?}"
    );
}

#[test]
fn a_slave_units_data_out_left_open_is_refused_once_the_unit_is_used() {
    let src = board(
        &[("u", "Zmcu"), ("host", "Zhost")],
        "    .u.p00.function &= SPIS0_SCK\n    .u.p01.function &= SPIS0_MOSI\n    .u.p02.function &= SPIS0_MISO\n    .u.p00 ~ .host.spi.sclk\n    .u.p01 ~ .host.spi.mosi\n    .host.uart.rx ~ .host.uart.rx\n",
    );
    let m = messages(&run(&src));
    assert!(
        m.iter().any(
            |m| m.contains("`u.p02` is a bidirectional pin that must be connected")
                && m.contains("`SpiRole::Slave` of `Spi` requires it")
        ),
        "{m:#?}"
    );
}

#[test]
fn the_uart_units_cross_and_a_tx_on_a_tx_is_found() {
    let ok = board(
        &[("u", "Zmcu"), ("link", "Zlink")],
        "    .u.p00.function &= UARTE0_TX\n    .u.p01.function &= UARTE0_RX\n    .u.p00 ~ .link.uart.rx\n    .u.p01 ~ .link.uart.tx\n",
    );
    let c = run(&ok);
    assert!(c.is_ok(), "{}", c.render());
    let bad = ok
        .replace(".u.p00 ~ .link.uart.rx", ".u.p00 ~ .link.uart.tx")
        .replace(".u.p01 ~ .link.uart.tx", ".u.p01 ~ .link.uart.rx");
    let m = messages(&run(&bad));
    assert_eq!(m.len(), 2, "{m:#?}");
    assert!(
        m[1].starts_with("net `net-link.uart.tx` joins `tx` to `tx` (`link.uart.tx`, `u.p00`): `tx` crosses `rx` in a `Uart`, so it is joined to the other end's `rx`; write `.link.uart.tx ~ .<the other end>.rx`"),
        "{}",
        m[1]
    );
}

#[test]
fn a_third_uart_end_is_over_the_limit_whether_it_is_an_interface_or_a_peripheral() {
    let src = board(
        &[("u", "Zmcu"), ("link", "Zlink"), ("host", "Zhost")],
        "    .u.p00.function &= UARTE0_TX\n    .u.p01.function &= UARTE0_RX\n    .u.p00 ~ .link.uart.rx\n    .u.p01 ~ .link.uart.tx\n    .host.uart.rx ~ .link.uart.tx\n    .host.spi.sclk ~ .host.spi.sclk\n",
    );
    let m = messages(&run(&src));
    assert!(
        m.iter()
            .any(|m| m.contains("are 3 ends of one `Uart` bus") && m.contains("`u` (`UARTE0`)")),
        "{m:#?}"
    );
}

#[test]
fn the_order_of_the_statements_changes_nothing() {
    let forward = with_master(&[(".u.p01 ~ .flash.spi.mosi", ".u.p01 ~ .flash.spi.miso")]);
    let mut lines: Vec<&str> = MASTER.lines().collect();
    lines.reverse();
    let reversed = lines
        .join("\n")
        .replace(".u.p01 ~ .flash.spi.mosi", ".u.p01 ~ .flash.spi.miso")
        + "\n";
    let backward = board(&[("u", "Zmcu"), ("flash", "Zflash")], &reversed);
    let (a, b) = (messages(&run(&forward)), messages(&run(&backward)));
    assert!(!a.is_empty());
    assert_eq!(a, b);
}

// ---- the definition ------------------------------------------------------------------

fn part_error(peripheral: &str) -> Vec<String> {
    let src = format!(
        "import std::bus::{{Spi, SpiRole, Uart}}\nimport std::sources::Rail\npart P {{\n    {peripheral}\n    pins {{\n        .p0 bidirectional \"P0\" \"1\" fn APP_GPIO\n    }}\n}}\nboard B {{\n    .p: P\n}}\n"
    );
    messages(&run(&src))
}

#[test]
fn a_function_names_a_line_only_in_a_peripheral_that_is_an_end_of_a_bus() {
    let m = part_error("peripheral X on APP_GPIO { X_SCK:output as sclk }");
    assert!(
        m[0].contains("names the line `sclk` of a bus, but `peripheral X` is not an end of one"),
        "{m:#?}"
    );
}

#[test]
fn the_bus_must_be_an_interface_with_lines_and_a_role_when_it_has_roles() {
    let m = part_error("peripheral X on APP_GPIO as P { X_A:input }");
    assert!(m.iter().any(|m| m.contains("`as` names the interface that the peripheral is an end of, such as `Spi`; `P` is not one")), "{m:#?}");
    let m = part_error("peripheral X on APP_GPIO as Rail { X_A:input }");
    assert!(
        m.iter().any(|m| m.contains(
            "`Rail` has no `lines`, so it has no roles or lines for a peripheral to be one of"
        )),
        "{m:#?}"
    );
    let m = part_error("peripheral X on APP_GPIO as Spi { X_A:input as sclk }");
    assert!(m.iter().any(|m| m.contains("`Spi` has roles (`SpiRole`): say which one the peripheral is, such as `as Spi SpiRole::Master`")), "{m:#?}");
    let m = part_error("peripheral X on APP_GPIO as Uart SpiRole::Master { X_A:input as rx }");
    assert!(
        m.iter().any(|m| m.contains("`Uart` has no roles")),
        "{m:#?}"
    );
    let m = part_error("peripheral X on APP_GPIO as Spi SpiRole::Boss { X_A:input as sclk }");
    assert!(
        m.iter()
            .any(|m| m.contains("`SpiRole` has no variant `Boss`")),
        "{m:#?}"
    );
}

#[test]
fn a_line_is_one_the_role_has_once_and_its_kind_agrees() {
    let m = part_error("peripheral X on APP_GPIO as Spi SpiRole::Master { X_A:output as sck }");
    assert!(m.iter().any(|m| m.contains("`Spi` has no line `sck` for `SpiRole::Master`; its lines are `sclk`, `mosi`, `miso` and `cs`; did you mean `sclk`?")), "{m:#?}");
    let m = part_error(
        "peripheral X on APP_GPIO as Spi SpiRole::Master { X_A:output as sclk X_B:output as sclk }",
    );
    assert!(
        m.iter()
            .any(|m| m.contains("`sclk` is already the function that is `sclk`")),
        "{m:#?}"
    );
    let m = part_error("peripheral X on APP_GPIO as Spi SpiRole::Master { X_MISO:output as miso }");
    assert!(
        m.iter().any(|m| m == "`X_MISO` is an output in `peripheral X`, but `SpiRole::Master` makes `miso` an input of `Spi`"),
        "{m:#?}"
    );
}

#[test]
fn a_function_with_no_line_is_not_on_the_bus() {
    let m = part_error(
        "peripheral X on APP_GPIO as Spi SpiRole::Master { X_SCK:output as sclk X_DCX:output }",
    );
    assert!(m.is_empty(), "{m:#?}");
}

#[test]
fn a_generic_refuses_the_statement_as_before() {
    let src = "import std::bus::{Spi, SpiRole}\ngeneric G {\n    peripheral X as Spi SpiRole::Master { X_SCK:output as sclk }\n}\nboard B {\n}\n";
    let c = run(src);
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("a generic states what every provider has")),
        "{}",
        c.render()
    );
}

// ---- the advice about a function that states no kind (message 10) ----------------------

const BARE: &str = r#"
import std::bus::{Spi, SpiRole}
import std::sources::Rail

/// Zbare, an invented processor whose functions state no kind of their own.
part Zbare {
    .vdd: Rail
    peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master {
        SPIM0_SCK  as sclk
        SPIM0_MOSI:output as mosi
        SPIM0_MISO as miso
    }
    pins {
        .vdd.pos power         "VDD"   "1"
        .vdd.neg power         "VSS"   "2"
        .p00     bidirectional "P0.00" "3" fn APP_GPIO
        .p01     bidirectional "P0.01" "4" fn APP_GPIO
        .p02     bidirectional "P0.02" "5" fn APP_GPIO
    }
}
board B {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Zbare
    .supply ~ .u.vdd
    .u.p00.function &= SPIM0_SCK
    .u.p01.function &= SPIM0_MOSI
    .u.p02.function &= SPIM0_MISO
}
"#;

#[test]
fn a_function_that_states_no_kind_on_a_bidirectional_pin_is_advised_to_say_it() {
    let c = run(BARE);
    let rows: Vec<String> = bidirectional_rows(&c.board("B").unwrap().model)
        .into_iter()
        .map(|(_, m)| m)
        .collect();
    assert_eq!(
        rows,
        [
            "`u.p00` is `bidirectional` in the table of `Zbare`, and its function `SPIM0_SCK` states no kind of its own, and `SpiRole::Master` makes `sclk` an output: write `SPIM0_SCK:output` or `SPIM0_SCK:tri_state`, as the datasheet says, unless the datasheet says the pin also receives",
            "`u.p02` is `bidirectional` in the table of `Zbare`, and its function `SPIM0_MISO` states no kind of its own, and `SpiRole::Master` makes `miso` an input: write `SPIM0_MISO:input`, as the datasheet says, unless the datasheet says the pin also drives",
        ]
    );
}
