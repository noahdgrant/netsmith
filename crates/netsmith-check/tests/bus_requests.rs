//! `.dbg: .u.Uart`: a request for the end of a bus (#788). The tool picks one peripheral of the
//! part that is an end of the bus and a pin for each of its lines, keeps them in the pin file, and
//! the request joins as that peripheral does (ADR 0129). The parts are invented for these tests.

use std::collections::BTreeSet;

use netsmith_check::*;

const PARTS: &str = r#"
import std::bus::{Spi, SpiRole, Uart}
import std::sources::Rail

/// Zmcu, an invented processor: two UARTs and an SPI master on any pin, of which one UART at a
/// time is in use, and a UART that names no bus.
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

    pins {
        .vdd.pos power         "VDD"   "1"
        .vdd.neg power         "VSS"   "2"
        .p00     bidirectional "P0.00" "3"  fn APP_GPIO
        .p01     bidirectional "P0.01" "4"  fn APP_GPIO
        .p02     bidirectional "P0.02" "5"  fn APP_GPIO
        .p03     bidirectional "P0.03" "6"  fn APP_GPIO
        .p04     bidirectional "P0.04" "7"  fn APP_GPIO
    }
}

/// Zplain, an invented processor whose UART names no bus.
part Zplain {
    .vdd: Rail
    peripheral UARTE0 on APP_GPIO { UARTE0_TX:output UARTE0_RX:input }
    pins {
        .vdd.pos power         "VDD"   "1"
        .vdd.neg power         "VSS"   "2"
        .p00     bidirectional "P0.00" "3"  fn APP_GPIO
        .p01     bidirectional "P0.01" "4"  fn APP_GPIO
    }
}

/// Zlink, an invented UART peer.
part Zlink {
    .vdd: Rail
    .uart: Uart
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .uart.tx  output "TX"  "3"
        .uart.rx  input  "RX"  "4"
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
"#;

const SUPPLY: &str = "    .supply: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }\n";

/// A board with `parts` (each on the supply) and `body`.
fn board(parts: &[(&str, &str)], body: &str) -> String {
    let mut decl = String::new();
    for (name, ty) in parts {
        decl.push_str(&format!("    .{name}: {ty}\n    .supply ~ .{name}.vdd\n"));
    }
    format!("{PARTS}\nboard B {{\n{SUPPLY}{decl}{body}\n}}\n")
}

fn run(src: &str, file: Option<&str>) -> Checked {
    let loader = match file {
        Some(text) => Loader::std().with_pins("main.nsm", "main.pins.toml", text),
        None => Loader::std(),
    };
    check("main.nsm", src, &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// The nets of board `B`, each as the paths of its terminals.
fn nets(c: &Checked) -> BTreeSet<BTreeSet<String>> {
    let m = &c.board("B").expect("the board is there").model;
    m.nets()
        .iter()
        .map(|n| n.terminals.iter().map(|&t| m.path_of(t)).collect())
        .collect()
}

/// The entry of the request `key` in the pin file the run would write.
fn bus_entry(c: &Checked, key: &str) -> Option<BusEntry> {
    c.pins.file.buses_of("B")?.get(key).cloned()
}

fn uart_entry(peripheral: &str, rx: &str, tx: &str, lock: bool) -> String {
    format!(
        "version = 1\n\n[B.pins.dbg]\nperipheral = \"{peripheral}\"\npins = {{ rx = \"{rx}\", tx = \"{tx}\" }}\n{}source = \"hand\"\n",
        if lock { "lock = true\n" } else { "" }
    )
}

const LINK: [(&str, &str); 2] = [("u", "Zmcu"), ("link", "Zlink")];

#[test]
fn a_request_for_the_end_of_a_uart_gets_the_first_peripheral_and_joins_as_it() {
    let c = run(
        &board(&LINK, "    .dbg: .u.Uart\n    .dbg ~ .link.uart\n"),
        None,
    );
    assert!(c.is_ok(), "{}", c.render());
    let by_hand = run(
        &board(
            &LINK,
            "    .u.p00.function &= UARTE0_RX\n    .u.p01.function &= UARTE0_TX\n    .u.UARTE0 ~ .link.uart\n",
        ),
        None,
    );
    assert!(by_hand.is_ok(), "{}", by_hand.render());
    // `rx` comes before `tx`, so `rx` gets `p00`; the join crosses the lines as `~` does.
    assert_eq!(nets(&c), nets(&by_hand));
    let e = bus_entry(&c, "dbg").expect("an entry");
    assert_eq!(e.peripheral, "UARTE0");
    assert_eq!(
        e.pins.iter().map(|(l, p)| format!("{l}={p}")).collect::<Vec<_>>(),
        ["rx=p00", "tx=p01"]
    );
    assert_eq!(
        c.pins.file.to_text(),
        "version = 1\n\n[B.pins.dbg]\nperipheral = \"UARTE0\"\npins = { rx = \"p00\", tx = \"p01\" }\nsource = \"auto\"\n"
    );
    assert_eq!(c.pins.changes, ["B.dbg: new, `UARTE0` with rx `p00`, tx `p01`"]);
    // The lines are in the model's assignments and in what the optimizer may move.
    let m = &c.board("B").unwrap().model;
    let keys: Vec<&str> = m.pin_assignments.iter().map(|a| a.key.as_str()).collect();
    assert_eq!(keys, ["dbg.rx", "dbg.tx"]);
    let space = &c.pins.spaces["B"][0];
    let line = space.requests.iter().find(|r| r.key == "dbg.tx").unwrap();
    assert_eq!(line.capability, "UARTE0_TX");
    assert_eq!(
        line.bus,
        Some(BusLine {
            request: "dbg".into(),
            peripheral: "UARTE0".into(),
            line: "tx".into()
        })
    );
}

#[test]
fn the_request_may_come_before_the_part_it_asks() {
    let src = format!(
        "{PARTS}\nboard B {{\n    .dbg: .u.Uart\n{SUPPLY}    .u: Zmcu\n    .supply ~ .u.vdd\n    .link: Zlink\n    .supply ~ .link.vdd\n    .dbg ~ .link.uart\n}}\n"
    );
    let c = run(&src, None);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(bus_entry(&c, "dbg").unwrap().peripheral, "UARTE0");
}

#[test]
fn a_peripheral_that_the_source_uses_is_not_given() {
    // The source puts UARTE0's TX on `p04`, so UARTE0 is in use: the request gets UARTE1.
    let c = run(
        &board(
            &LINK,
            "    .u.p04.function &= UARTE0_TX\n    .dbg: .u.Uart\n    .dbg ~ .link.uart\n",
        ),
        None,
    );
    let e = bus_entry(&c, "dbg").expect("an entry");
    assert_eq!(e.peripheral, "UARTE1");
    assert_eq!(e.pins.values().cloned().collect::<Vec<_>>(), ["p00", "p01"]);
}

#[test]
fn two_requests_get_two_peripherals_and_a_request_for_a_pin_keeps_its_own() {
    let parts = [("u", "Zmcu"), ("a", "Zlink"), ("b", "Zlink")];
    let c = run(
        &board(
            &parts,
            "    .one: .u.Uart\n    .two: .u.Uart\n    .one ~ .a.uart\n    .two ~ .b.uart\n",
        ),
        None,
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(bus_entry(&c, "one").unwrap().peripheral, "UARTE0");
    assert_eq!(bus_entry(&c, "two").unwrap().peripheral, "UARTE1");

    // A request for a line of UARTE0 by name keeps UARTE0 from the request for the bus.
    let c = run(
        &board(&LINK, "    .tx: .u.UARTE0_TX\n    .dbg: .u.Uart\n    .dbg ~ .link.uart\n"),
        None,
    );
    assert_eq!(bus_entry(&c, "dbg").unwrap().peripheral, "UARTE1");
    let m = &c.board("B").unwrap().model;
    let tx = m.pin_assignments.iter().find(|a| a.key == "tx").unwrap();
    assert_eq!(tx.function, "UARTE0_TX");
}

#[test]
fn an_entry_that_holds_is_kept_and_one_that_does_not_is_picked_again() {
    let src = board(&LINK, "    .dbg: .u.Uart\n    .dbg ~ .link.uart\n");
    let c = run(&src, Some(&uart_entry("UARTE1", "p03", "p02", false)));
    assert!(c.is_ok(), "{}", c.render());
    assert!(c.pins.changes.is_empty(), "{:?}", c.pins.changes);
    let e = bus_entry(&c, "dbg").unwrap();
    assert_eq!(e.peripheral, "UARTE1");
    assert_eq!(e.pins["tx"], "p02");

    // A peripheral the part does not have: a note, and the request is served again.
    let c = run(&src, Some(&uart_entry("UARTE7", "p03", "p02", false)));
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(bus_entry(&c, "dbg").unwrap().peripheral, "UARTE0");
    assert!(
        c.pins.findings.iter().any(|f| f.contains(
            "`dbg` was `UARTE7` of `u`, but `u` (`Zmcu`) has no peripheral `UARTE7` that is an end of a `Uart`; its ends are `UARTE0` and `UARTE1`; a peripheral is picked again"
        )),
        "{:?}",
        c.pins.findings
    );
}

#[test]
fn a_locked_entry_that_does_not_hold_is_an_error() {
    // The source chooses a function on `p02`, so the entry's `tx` pin is the source's.
    let src = board(
        &LINK,
        "    .u.p02.function &= APP_GPIO\n    .dbg: .u.Uart\n    .dbg ~ .link.uart\n",
    );
    let c = run(&src, Some(&uart_entry("UARTE1", "p03", "p02", true)));
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.starts_with(
            "`dbg` is locked to `UARTE1` of `u` in the pin file, but `u.p02` is held by the source"
        )),
        "{m:#?}"
    );
}

#[test]
fn a_part_with_no_end_of_the_bus_is_named() {
    let parts = [("u", "Zplain"), ("link", "Zlink")];
    let c = run(&board(&parts, "    .dbg: .u.Uart\n"), None);
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.starts_with(
            "`dbg` asks `u` (`Zplain`) for the end of a `Uart`, but no peripheral of `Zplain` is one"
        )),
        "{m:#?}"
    );
}

#[test]
fn the_request_is_joined_alone() {
    let c = run(
        &board(&LINK, "    .dbg: .u.Uart\n    .dbg.tx ~ .link.uart.rx\n"),
        None,
    );
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.starts_with(
            "`.dbg` asks `u` for the end of a `Uart`, which is joined alone with `~`"
        )),
        "{m:#?}"
    );
    // The other side must be an interface of its bus.
    let c = run(
        &board(&[("u", "Zmcu"), ("flash", "Zflash")], "    .dbg: .u.Uart\n    .dbg ~ .flash.spi\n"),
        None,
    );
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.starts_with(
            "`~` joins `.dbg`, an end of a `Uart`, to an interface of that kind or to another such peripheral; `.flash.spi` is a `Spi`"
        )),
        "{m:#?}"
    );
}

#[test]
fn an_spi_master_gets_a_pin_for_every_line_it_lists() {
    // `miso` and `cs` are optional for a master in `Spi`; the request still gets a pin for each
    // line the peripheral names.
    let parts = [("u", "Zmcu"), ("flash", "Zflash")];
    let c = run(&board(&parts, "    .spi: .u.Spi
    .spi ~ .flash.spi
"), None);
    assert!(c.is_ok(), "{}", c.render());
    let e = bus_entry(&c, "spi").unwrap();
    assert_eq!(e.peripheral, "SPIM0");
    let lines: Vec<String> = e.pins.iter().map(|(l, p)| format!("{l}={p}")).collect();
    assert_eq!(lines, ["cs=p00", "miso=p01", "mosi=p02", "sclk=p03"]);
    assert!(nets(&c).contains(&BTreeSet::from(["flash.spi.cs".to_string(), "u.p00".to_string()])));
}

#[test]
fn a_stale_entry_is_dropped_with_a_note() {
    let src = board(&[("u", "Zmcu")], "");
    let c = run(&src, Some(&uart_entry("UARTE1", "p03", "p02", false)));
    assert!(c.is_ok(), "{}", c.render());
    assert!(c.pins.file.is_empty());
    assert!(
        c.pins
            .findings
            .iter()
            .any(|f| f.contains("the entry `dbg` (`UARTE1`) is stale: `B` has no request `dbg`")),
        "{:?}",
        c.pins.findings
    );
    assert_eq!(c.pins.changes, ["B.dbg: removed, it was `UARTE1`"]);
}

#[test]
fn the_pin_file_reads_back_what_it_writes_and_refuses_a_bad_entry() {
    let text = "version = 1\n\n[B.pins.dbg]\nperipheral = \"UARTE1\"\npins = { rx = \"p01\", tx = \"p00\" }\nlock = true\nsource = \"hand\"\n\n[B.pins.led]\npin = \"p02\"\nfunction = \"GPIO\"\nsource = \"auto\"\n";
    let file = PinFile::parse(text).expect("it reads");
    assert_eq!(file.to_text(), text);
    let e = &file.buses_of("B").unwrap()["dbg"];
    assert!(e.lock);
    assert_eq!(e.pins["tx"], "p00");

    let bad = "version = 1\n\n[B.pins.dbg]\nperipheral = \"UARTE1\"\npin = \"p00\"\nsource = \"hand\"\n";
    let errors = PinFile::parse(bad).unwrap_err();
    let m: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
    assert_eq!(
        m,
        ["unknown key `pin` in `B.pins.dbg`; an entry of a request for the end of a bus has `peripheral`, `pins`, `lock` and `source`"]
    );
}
