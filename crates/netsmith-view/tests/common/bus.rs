//! A processor and a modem joined by a UART, with the interface instances each declares: the
//! members of one interface are the nets a bus is drawn for. The modem calls its lines by its
//! own names, so its `tx` is the processor's `rx`, and its pins are in another order.

#![allow(dead_code)]

use netsmith_core::{Design, InterfaceInstance, InterfaceMember};
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
        modem += &pin("bidirectional", -12.7, 5.08 - 2.54 * i as f64, 0, name, i as u32 + 1);
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
        parts.push((format!("C{}", c + 1), "Capacitor", "100n", format!("mcu.c{c}")));
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
                if *k == "Capacitor" { Some("0402") } else { Some("P") },
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
