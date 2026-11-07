//! A processor and a modem joined by a UART, with the interface instances each declares: the
//! members of one interface are the nets a bus is drawn for. The modem calls its lines by its
//! own names, so its `tx` is the processor's `rx`, and its pins are in another order.

#![allow(dead_code)]

use netsmith_core::{Design, InterfaceInstance, InterfaceMember, PartPin, PinKind};
use netsmith_kicad::testlib::{self, TempDir};
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_view::{Group, Rails};

fn pin(kind: &str, x: f64, y: f64, angle: u32, name: &str, number: u32) -> String {
    format!(
        "(pin {kind} line (at {x} {y} {angle}) (length 2.54) \
         (name \"{name}\" (effects (font (size 1.27 1.27)))) \
         (number \"{number}\" (effects (font (size 1.27 1.27)))))"
    )
}

/// `Cpu`: the lines of the UART down the right, a free pin among them, the supply on the left.
/// `Modem`: the same lines down the left in another order, the supply on the right.
fn library() -> String {
    let mut cpu = String::new();
    for (i, (name, kind)) in [
        ("TX", "bidirectional"),
        ("RX", "bidirectional"),
        ("GPIO", "bidirectional"),
        ("RTS", "bidirectional"),
        ("CTS", "bidirectional"),
    ]
    .into_iter()
    .enumerate()
    {
        cpu += &pin(kind, 12.7, 5.08 - 2.54 * i as f64, 180, name, i as u32 + 1);
    }
    cpu += &pin("power_in", -12.7, 2.54, 0, "VCC", 6);
    cpu += &pin("power_in", -12.7, -2.54, 0, "GND", 7);
    let mut modem = String::new();
    for (i, name) in ["RX", "TX", "CTS", "RTS", "WAKE"].into_iter().enumerate() {
        modem += &pin(
            "bidirectional",
            -12.7,
            5.08 - 2.54 * i as f64,
            0,
            name,
            i as u32 + 1,
        );
    }
    modem += &pin("power_in", 12.7, 2.54, 180, "VCC", 6);
    modem += &pin("power_in", 12.7, -2.54, 180, "GND", 7);
    let symbol = |name: &str, pins: &str| {
        format!(
            "(symbol \"{name}\" (property \"Reference\" \"U\" (at 0 12 0)) \
             (symbol \"{name}_0_1\" (rectangle (start -10.16 10.16) (end 10.16 -10.16) \
             (stroke (width 0.254) (type default)) (fill (type background)))) \
             (symbol \"{name}_1_1\" {pins}))"
        )
    };
    format!(
        "(kicad_symbol_lib {} {})",
        symbol("Cpu", &cpu),
        symbol("Modem", &modem)
    )
}

pub fn setup() -> (TempDir, Mapping, SymbolLib) {
    let dir = TempDir::new("view-bus");
    let file = dir.path().join("bus.kicad_sym");
    std::fs::write(&file, library()).unwrap();
    let rows = "[[part]]\nkind = \"Cpu\"\npackage = \"P\"\nsymbol = \"bus:Cpu\"\nfootprint = \"bus:F\"\n\
                [[part]]\nkind = \"Modem\"\npackage = \"P\"\nsymbol = \"bus:Modem\"\nfootprint = \"bus:F\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(testlib::sample_symbols()).with_library("bus", file);
    (dir, mapping, lib)
}

fn instance(path: &str, members: &[(&str, &str)]) -> InterfaceInstance {
    InterfaceInstance {
        path: path.into(),
        type_name: "Uart".into(),
        members: members
            .iter()
            .map(|&(name, net)| InterfaceMember {
                name: name.into(),
                net: net.into(),
            })
            .collect(),
    }
}

/// The processor `U1` and the modem `U2` with `lines` of the UART between them, in the order
/// `tx rx rts cts`, and `caps` capacitors on the supply in each of the two modules `mcu` and
/// `modem`. With 8 capacitors there are more than 16 parts, so the board is drawn as two
/// sheets.
pub fn board(lines: usize, caps: usize) -> (Design, Vec<Group>) {
    let mut parts: Vec<(String, &str, &str, String)> = Vec::new();
    parts.push(("U1".into(), "Cpu", "Cpu", "mcu.u".into()));
    parts.push(("U2".into(), "Modem", "Modem", "modem.u".into()));
    for c in 0..caps {
        parts.push((
            format!("C{}", c + 1),
            "Capacitor",
            "100n",
            format!("mcu.c{c}"),
        ));
    }
    for c in 0..caps {
        parts.push((
            format!("C{}", caps + c + 1),
            "Capacitor",
            "100n",
            format!("modem.c{c}"),
        ));
    }
    let parts_ref: Vec<(&str, &str, &str, Option<&str>)> = parts
        .iter()
        .map(|(r, k, v, _)| {
            (
                r.as_str(),
                *k,
                *v,
                if *k == "Capacitor" {
                    Some("0402")
                } else {
                    Some("P")
                },
            )
        })
        .collect();
    let lines_all = [
        ("UART_TX", "U1.1", "U2.2"),
        ("UART_RX", "U1.2", "U2.1"),
        ("UART_RTS", "U1.4", "U2.3"),
        ("UART_CTS", "U1.5", "U2.4"),
    ];
    let mut owned: Vec<(String, Vec<String>)> = lines_all[..lines]
        .iter()
        .map(|&(n, a, b)| (n.to_string(), vec![a.to_string(), b.to_string()]))
        .collect();
    owned.push(("WAKE".into(), vec!["U1.3".into(), "U2.5".into()]));
    let mut vcc = vec!["U1.6".to_string(), "U2.6".to_string()];
    let mut gnd = vec!["U1.7".to_string(), "U2.7".to_string()];
    for c in 0..caps * 2 {
        vcc.push(format!("C{}.1", c + 1));
        gnd.push(format!("C{}.2", c + 1));
    }
    owned.push(("VCC".into(), vcc));
    owned.push(("GND".into(), gnd));
    let nets: Vec<(&str, Vec<&str>)> = owned
        .iter()
        .map(|(n, p)| (n.as_str(), p.iter().map(String::as_str).collect()))
        .collect();
    let nets_ref: Vec<(&str, &[&str])> = nets.iter().map(|(n, p)| (*n, p.as_slice())).collect();
    let mut design = super::design("UartBoard", &parts_ref, &nets_ref);
    for (p, (_, _, _, path)) in design.parts.iter_mut().zip(&parts) {
        p.path = path.clone();
    }
    let ours: [(&str, &str); 4] = [
        ("tx", "UART_TX"),
        ("rx", "UART_RX"),
        ("rts", "UART_RTS"),
        ("cts", "UART_CTS"),
    ];
    let theirs: [(&str, &str); 4] = [
        ("tx", "UART_RX"),
        ("rx", "UART_TX"),
        ("rts", "UART_CTS"),
        ("cts", "UART_RTS"),
    ];
    design.interfaces = vec![
        // Listed out of order: the drawing is keyed by path.
        instance("modem.uart", &theirs[..lines]),
        instance("mcu.uart0", &ours[..lines]),
    ];
    let groups = vec![
        Group {
            name: "mcu".into(),
            detail: "Cpu".into(),
            parts: std::iter::once(0).chain(2..2 + caps).collect(),
        },
        Group {
            name: "modem".into(),
            detail: "Modem".into(),
            parts: std::iter::once(1).chain(2 + caps..2 + 2 * caps).collect(),
        },
    ];
    (design, groups)
}

pub fn rails() -> Rails {
    let mut r = Rails::default();
    r.ground.insert("GND".into());
    r.positive.insert("VCC".into());
    r
}

fn table_pin(name: &str, pad: &str, kind: PinKind) -> PartPin {
    PartPin {
        name: name.into(),
        datasheet: name.to_uppercase(),
        kind,
        pads: vec![pad.into()],
        function: None,
    }
}

/// The UART board with the pin tables of its two parts, as the language gives them: the
/// processor's `tx` and `rts` are outputs and its `rx` and `cts` inputs, the modem's pins the
/// other way round on the same lines, the wake line is bidirectional at both.
pub fn board_directed(lines: usize, caps: usize) -> (Design, Vec<Group>) {
    let (mut design, groups) = board(lines, caps);
    design.parts[0].pins = vec![
        table_pin("uart0.tx", "1", PinKind::Output),
        table_pin("uart0.rx", "2", PinKind::Input),
        table_pin("wake", "3", PinKind::Bidirectional),
        table_pin("uart0.rts", "4", PinKind::Output),
        table_pin("uart0.cts", "5", PinKind::Input),
        table_pin("vcc", "6", PinKind::Power),
        table_pin("gnd", "7", PinKind::Power),
    ];
    design.parts[1].pins = vec![
        table_pin("uart.rx", "1", PinKind::Output),
        table_pin("uart.tx", "2", PinKind::Input),
        table_pin("uart.cts", "3", PinKind::Input),
        table_pin("uart.rts", "4", PinKind::Output),
        table_pin("wake", "5", PinKind::Bidirectional),
        table_pin("vcc", "6", PinKind::Power),
        table_pin("gnd", "7", PinKind::Power),
    ];
    (design, groups)
}

fn typed(path: &str, type_name: &str, members: &[(&str, &str)]) -> InterfaceInstance {
    InterfaceInstance {
        type_name: type_name.into(),
        ..instance(path, members)
    }
}

/// The UART board with its four lines declared as one interface that nests another: each
/// side has `link` (a `Link` with the `irq` line) with `data` (an `Spi`, three lines) inside
/// it and a nested `Rail` for the supply, the way an interface that is made of others lists
/// its instances. Both sides call the lines the same.
pub fn board_nested(caps: usize) -> (Design, Vec<Group>) {
    let (mut design, groups) = board(4, caps);
    let side = |module: &str| {
        vec![
            typed(&format!("{module}.link"), "Link", &[("irq", "UART_RTS")]),
            typed(
                &format!("{module}.link.data"),
                "Spi",
                &[
                    ("sclk", "UART_TX"),
                    ("mosi", "UART_RX"),
                    ("miso", "UART_CTS"),
                ],
            ),
            typed(
                &format!("{module}.link.power"),
                "Rail",
                &[("pos", "VCC"), ("neg", "GND")],
            ),
        ]
    };
    design.interfaces = side("modem").into_iter().chain(side("mcu")).collect();
    (design, groups)
}

/// A processor `U1` and `devices` sensors on one SPI bus: the clock, `mosi` and `miso` (the
/// first `lines` of them) are one net each with a pin on every part, and the first sensor has
/// a select line of its own. Every part declares the bus as `spi` (the lines are named alike
/// at both ends). With `caps` capacitors on the supply (split between the processor's module
/// `mcu` and a module `bank` that holds the second half of the sensors) the board is more than
/// 16 parts and is drawn as two sheets.
pub fn board_shared(devices: usize, lines: usize, caps: usize) -> (Design, Vec<Group>) {
    let mut parts: Vec<(String, &str, &str, String)> =
        vec![("U1".into(), "Cpu", "Cpu", "mcu.u".into())];
    for d in 0..devices {
        // The first half of the sensors with the processor, the rest in the other module.
        let module = if d < devices.div_ceil(2) {
            "mcu"
        } else {
            "bank"
        };
        let path = if module == "mcu" {
            format!("sensor{}.u", d + 1)
        } else {
            format!("bank.sensor{}.u", d + 1)
        };
        parts.push((format!("U{}", d + 2), "Modem", "Modem", path));
    }
    for c in 0..caps {
        let module = if c < caps / 2 { "mcu" } else { "bank" };
        parts.push((
            format!("C{}", c + 1),
            "Capacitor",
            "100n",
            format!("{module}.c{c}"),
        ));
    }
    let parts_ref: Vec<(&str, &str, &str, Option<&str>)> = parts
        .iter()
        .map(|(r, k, v, _)| {
            (
                r.as_str(),
                *k,
                *v,
                if *k == "Capacitor" {
                    Some("0402")
                } else {
                    Some("P")
                },
            )
        })
        .collect();
    let shared = [("SCLK", "1"), ("MOSI", "2"), ("MISO", "4")];
    let mut owned: Vec<(String, Vec<String>)> = Vec::new();
    for &(net, pin) in &shared[..lines] {
        let mut pins = vec![format!("U1.{pin}")];
        for d in 0..devices {
            pins.push(format!("U{}.{pin}", d + 2));
        }
        owned.push((net.to_string(), pins));
    }
    owned.push(("CS1".into(), vec!["U1.3".into(), "U2.5".into()]));
    let mut vcc: Vec<String> = (1..=devices + 1).map(|u| format!("U{u}.6")).collect();
    let mut gnd: Vec<String> = (1..=devices + 1).map(|u| format!("U{u}.7")).collect();
    for c in 0..caps {
        vcc.push(format!("C{}.1", c + 1));
        gnd.push(format!("C{}.2", c + 1));
    }
    owned.push(("VCC".into(), vcc));
    owned.push(("GND".into(), gnd));
    let nets: Vec<(&str, Vec<&str>)> = owned
        .iter()
        .map(|(n, p)| (n.as_str(), p.iter().map(String::as_str).collect()))
        .collect();
    let nets_ref: Vec<(&str, &[&str])> = nets.iter().map(|(n, p)| (*n, p.as_slice())).collect();
    let mut design = super::design("SharedBus", &parts_ref, &nets_ref);
    for (p, (_, _, _, path)) in design.parts.iter_mut().zip(&parts) {
        p.path = path.clone();
    }
    let members: Vec<(&str, &str)> =
        [("sclk", "SCLK"), ("mosi", "MOSI"), ("miso", "MISO")][..lines].to_vec();
    design.interfaces = std::iter::once(typed("mcu.spi", "Spi", &members))
        .chain((0..devices).map(|d| {
            let owner = if d < devices.div_ceil(2) { "" } else { "bank." };
            typed(&format!("{owner}sensor{}.spi", d + 1), "Spi", &members)
        }))
        .collect();
    let in_module = |module: &str, first: bool| -> Vec<usize> {
        let mut v = Vec::new();
        if first {
            v.push(0);
        }
        for d in 0..devices {
            if (d < devices.div_ceil(2)) == (module == "mcu") {
                v.push(d + 1);
            }
        }
        for c in 0..caps {
            if (c < caps / 2) == (module == "mcu") {
                v.push(devices + 1 + c);
            }
        }
        v
    };
    let groups = vec![
        Group {
            name: "mcu".into(),
            detail: "Cpu and sensors".into(),
            parts: in_module("mcu", true),
        },
        Group {
            name: "bank".into(),
            detail: "More sensors".into(),
            parts: in_module("bank", false),
        },
    ];
    (design, groups)
}
