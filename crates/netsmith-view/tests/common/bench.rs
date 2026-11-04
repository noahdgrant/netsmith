//! An invented board of five modules that has the shapes a schematic view must draw well: a
//! patch antenna with ten pins, nine of them ground; a regulator with its capacitors; a
//! divider and an amplifier whose pins are on rails and on nets that go to other sheets; a
//! microcontroller whose symbol has two units and ninety-seven pins; and a row of lights and
//! a connector. None of it is a real design.

#![allow(dead_code)]

use netsmith_core::{Design, Net, Part, PinRef};
use netsmith_kicad::testlib::{self, TempDir};
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_view::{Group, Rails};

pub struct Bench {
    /// Keeps the symbol libraries until the test ends.
    pub dir: TempDir,
    pub design: Design,
    pub groups: Vec<Group>,
    pub rails: Rails,
    pub mapping: Mapping,
    pub lib: SymbolLib,
}

fn pin(kind: &str, x: f64, y: f64, angle: u32, name: &str, number: &str) -> String {
    format!(
        "(pin {kind} line (at {x} {y} {angle}) (length 2.54) \
         (name \"{name}\" (effects (font (size 1.27 1.27)))) \
         (number \"{number}\" (effects (font (size 1.27 1.27)))))"
    )
}

/// `Patch`: ten pins along the bottom, pin 1 the feed, the other nine ground.
fn patch() -> String {
    let mut pins = String::new();
    for i in 0..10u32 {
        let x = -11.43 + 2.54 * f64::from(i);
        let (name, kind) = if i == 0 {
            ("RF", "passive")
        } else {
            ("GND", "passive")
        };
        pins += &pin(kind, x, -5.08, 90, name, &(i + 1).to_string());
    }
    format!(
        "(symbol \"Patch\" (property \"Reference\" \"U\" (at 0 5 0)) \
         (symbol \"Patch_0_1\" (rectangle (start -12.7 2.54) (end 12.7 -2.54) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Patch_1_1\" {pins}))"
    )
}

/// `Mcu`: unit 1 has 64 pins (32 on each side), unit 2 is drawn 40 mm to the right and has
/// the 24 grounds, six supplies and three pins that connect to nothing.
fn mcu() -> String {
    let mut one = String::new();
    for i in 0..64u32 {
        let (side, row) = if i < 32 { (-1.0, i) } else { (1.0, i - 32) };
        let x = side * 15.24;
        let y = 40.64 - 2.54 * f64::from(row);
        let angle = if side < 0.0 { 0 } else { 180 };
        let port = i / 16;
        one += &pin(
            "bidirectional",
            x,
            y,
            angle,
            &format!("P{port}.{:02}", i % 16),
            &(i + 1).to_string(),
        );
    }
    let mut two = String::new();
    for i in 0..24u32 {
        two += &pin(
            "power_in",
            40.0 + 2.54 * f64::from(i) - 30.0,
            -50.8,
            90,
            "GND",
            &(101 + i).to_string(),
        );
    }
    for i in 0..6u32 {
        two += &pin(
            "power_in",
            40.0 + 2.54 * f64::from(i),
            50.8,
            270,
            "VDD",
            &(131 + i).to_string(),
        );
    }
    for i in 0..3u32 {
        two += &pin(
            "no_connect",
            60.0 + 2.54 * f64::from(i),
            -50.8,
            90,
            "NC",
            &(141 + i).to_string(),
        );
    }
    format!(
        "(symbol \"Mcu\" (property \"Reference\" \"U\" (at 0 45 0)) \
         (symbol \"Mcu_0_1\" (rectangle (start -12.7 43.18) (end 12.7 -43.18) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Mcu_1_1\" {one}) \
         (symbol \"Mcu_2_1\" (rectangle (start 27.94 48.26) (end 70.0 -48.26) \
         (stroke (width 0.254) (type default)) (fill (type background))) {two}))"
    )
}

/// `Buck`: a regulator IC: supplies and enable on the left, bootstrap, switch, feedback and
/// the good flag on the right, three grounds below.
fn buck() -> String {
    let left = [
        ("power_in", "VIN1", "1", 12.7),
        ("power_in", "VIN2", "2", 10.16),
        ("input", "EN", "3", 7.62),
        ("input", "RT", "4", 5.08),
    ];
    let right = [
        ("passive", "BST", "13", 12.7),
        ("power_out", "SW", "10", 10.16),
        ("power_out", "VCC", "1A", 7.62),
        ("input", "FB", "6", 5.08),
        ("output", "PGOOD", "5", 2.54),
    ];
    let mut pins = String::new();
    for (kind, name, number, y) in left {
        pins += &pin(kind, -12.7, y, 0, name, number);
    }
    for (kind, name, number, y) in right {
        pins += &pin(kind, 12.7, y, 180, name, number);
    }
    for (i, number) in ["9", "11", "3A"].iter().enumerate() {
        pins += &pin(
            "power_in",
            -2.54 + 2.54 * i as f64,
            -17.78,
            90,
            "GND",
            number,
        );
    }
    format!(
        "(symbol \"Buck\" (property \"Reference\" \"U\" (at 0 15 0)) \
         (symbol \"Buck_0_1\" (rectangle (start -10.16 15.24) (end 10.16 -15.24) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Buck_1_1\" {pins}))"
    )
}

/// `L` and `Crystal`: two-terminal parts the sample libraries do not have.
fn two_terminal(name: &str, half_width: f64) -> String {
    format!(
        "(symbol \"{name}\" (property \"Reference\" \"X\" (at 2 0 0)) \
         (symbol \"{name}_0_1\" (rectangle (start -{half_width} 2.54) (end {half_width} -2.54) \
         (stroke (width 0.254) (type default)) (fill (type none)))) \
         (symbol \"{name}_1_1\" {} {}))",
        pin("passive", 0.0, 3.81, 270, "~", "1"),
        pin("passive", 0.0, -3.81, 90, "~", "2")
    )
}

pub fn bench() -> Bench {
    let dir = TempDir::new("view-bench");
    let file = dir.path().join("bench.kicad_sym");
    std::fs::write(
        &file,
        format!(
            "(kicad_symbol_lib {} {} {} {} {})",
            patch(),
            mcu(),
            buck(),
            two_terminal("L", 1.0),
            two_terminal("Crystal", 1.0)
        ),
    )
    .unwrap();
    let acme = dir.path().join("acme.kicad_sym");
    std::fs::write(&acme, testlib::acme()).unwrap();
    let rows = "[[part]]\nkind = \"Patch\"\npackage = \"P\"\nsymbol = \"bench:Patch\"\nfootprint = \"bench:F\"\n\
                [[part]]\nkind = \"Mcu\"\npackage = \"P\"\nsymbol = \"bench:Mcu\"\nfootprint = \"bench:F\"\n\
                [[part]]\nkind = \"Buck\"\npackage = \"P\"\nsymbol = \"bench:Buck\"\nfootprint = \"bench:F\"\n\
                [[part]]\nkind = \"Inductor\"\npackage = \"IND\"\nsymbol = \"bench:L\"\nfootprint = \"bench:F\"\n\
                [[part]]\nkind = \"Crystal\"\npackage = \"XTAL\"\nsymbol = \"bench:Crystal\"\nfootprint = \"bench:F\"\n\
                [[part]]\nkind = \"OpAmp\"\npackage = \"P\"\nsymbol = \"acme:Dual\"\nfootprint = \"bench:F\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(testlib::sample_symbols())
        .with_library("bench", file)
        .with_library("acme", acme);

    let parts: Vec<(&str, &str, &str, Option<&str>)> = vec![
        // gnss
        ("U2", "Patch", "PatchAnt", Some("P")),
        ("J3", "Header1x2", "Coax", None),
        // ldo
        ("U4", "LinearRegulator", "LDO", Some("SOT-223")),
        ("C2", "Capacitor", "1u", Some("0402")),
        ("C3", "Capacitor", "1u", Some("0402")),
        // adc
        ("R1", "Resistor", "1M2", Some("0402")),
        ("R2", "Resistor", "330k", Some("0402")),
        ("R3", "Resistor", "10k", Some("0402")),
        ("C1", "Capacitor", "100n", Some("0402")),
        ("U1", "OpAmp", "Dual", Some("P")),
        // mcu
        ("U5", "Mcu", "Mcu97", Some("P")),
        ("C4", "Capacitor", "10u", Some("0402")),
        ("C5", "Capacitor", "100n", Some("0402")),
        // io
        ("J7", "Header1x2", "Uart", None),
        ("J8", "Header1x2", "Power", None),
        ("R4", "Resistor", "1k", Some("0402")),
        ("D1", "Led", "LED", None),
        ("R5", "Resistor", "1k", Some("0402")),
        ("D2", "Led", "LED", None),
        ("R6", "Resistor", "100k", Some("0402")),
        ("R7", "Resistor", "22k", Some("0402")),
        // buck
        ("U7", "Buck", "Buck1V8", Some("P")),
        ("C6", "Capacitor", "10u", Some("0402")),
        ("C7", "Capacitor", "100n", Some("0402")),
        ("L1", "Inductor", "4u7", Some("IND")),
        ("C8", "Capacitor", "22u", Some("0402")),
        ("R8", "Resistor", "100k", Some("0402")),
        ("R9", "Resistor", "33k", Some("0402")),
        ("R10", "Resistor", "10k", Some("0402")),
        ("R11", "Resistor", "10k", Some("0402")),
        ("C9", "Capacitor", "1u", Some("0402")),
        // the crystal of the microcontroller
        ("Y1", "Crystal", "32k768", Some("XTAL")),
        // modem
        ("J10", "Header1x4", "Uart", None),
        ("J11", "Header1x4", "Control", None),
        ("R20", "Resistor", "10k", Some("0402")),
        ("R21", "Resistor", "10k", Some("0402")),
        ("R22", "Resistor", "100", Some("0402")),
        ("R23", "Resistor", "10k", Some("0402")),
        ("R24", "Resistor", "10k", Some("0402")),
        ("R25", "Resistor", "1k", Some("0402")),
        ("D3", "Led", "LED", None),
        ("C20", "Capacitor", "10u", Some("0402")),
        ("C21", "Capacitor", "100n", Some("0402")),
        ("C22", "Capacitor", "100n", Some("0402")),
        ("C23", "Capacitor", "10n", Some("0402")),
        ("C24", "Capacitor", "10n", Some("0402")),
        ("R26", "Resistor", "4k7", Some("0402")),
        ("R27", "Resistor", "4k7", Some("0402")),
        ("R28", "Resistor", "47k", Some("0402")),
        ("R29", "Resistor", "47k", Some("0402")),
        ("C25", "Capacitor", "10n", Some("0402")),
        ("C26", "Capacitor", "10n", Some("0402")),
    ];
    let nets: Vec<(&str, Vec<String>)> = {
        let mut v: Vec<(&str, Vec<String>)> = Vec::new();
        let s = |l: &[&str]| l.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let mut gnd = s(&[
            "J3.2", "U4.1", "C2.2", "C3.2", "R2.2", "C1.2", "U1.4", "C4.2", "C5.2", "D1.1", "D2.1",
            "R6.2", "R7.2", "J7.2", "J8.2", "C6.2", "C8.2", "R9.2", "R10.2", "C9.2", "J11.2",
            "C20.2", "C21.2", "C22.2", "D3.1", "C23.2", "C24.2", "C25.2", "C26.2",
        ]);
        gnd.extend(["U7.9", "U7.11", "U7.3A"].map(String::from));
        gnd.extend((2..=10).map(|i| format!("U2.{i}")));
        gnd.extend((101..125).map(|i| format!("U5.{i}")));
        v.push(("GND", gnd));
        v.push(("ANT", s(&["U2.1", "J3.1"])));
        v.push(("+3V8", s(&["U4.3", "C2.1", "R1.1"])));
        let mut v33 = s(&["U4.2", "C3.1", "U1.8", "C4.1", "C5.1", "R6.1", "J8.1"]);
        v33.extend((131..137).map(|i| format!("U5.{i}")));
        v33.extend(
            [
                "R11.2", "J11.1", "C20.1", "C21.1", "C22.1", "R20.2", "R23.2", "R26.2", "R27.2",
                "R28.2", "R29.2",
            ]
            .map(String::from),
        );
        v.push(("+3V3", v33));
        v.push(("+5V_in", s(&["U7.1", "U7.2", "U7.3", "C6.1"])));
        v.push(("+1V8", s(&["L1.2", "C8.1", "R8.1"])));
        v.push(("net-buck.sw", s(&["U7.10", "C7.2", "L1.1"])));
        v.push(("net-buck.bst", s(&["U7.13", "C7.1"])));
        v.push(("net-buck.fb", s(&["U7.6", "R8.2", "R9.1"])));
        v.push(("net-buck.rt", s(&["U7.4", "R10.1"])));
        v.push(("net-buck.vcc", s(&["U7.1A", "C9.1"])));
        v.push(("PGOOD", s(&["U7.5", "R11.1", "U5.6"])));
        v.push(("net-mcu.xl1", s(&["U5.15", "Y1.1"])));
        v.push(("net-mcu.xl2", s(&["U5.16", "Y1.2"])));
        v.push(("M_TX", s(&["J10.1", "R20.1", "U5.7"])));
        v.push(("M_RX", s(&["J10.2", "R21.1", "U5.8"])));
        v.push(("net-modem.rts", s(&["J10.3", "R22.1"])));
        v.push(("M_RTS", s(&["R22.2", "J11.4", "U5.9"])));
        v.push(("M_CTS", s(&["J10.4", "R23.1", "U5.10"])));
        v.push(("M_STAT", s(&["J11.3", "R25.1", "R24.1"])));
        v.push(("net-modem.led", s(&["R25.2", "D3.2"])));
        v.push(("net-modem.a", s(&["R26.1", "C23.1"])));
        v.push(("net-modem.b", s(&["R27.1", "C24.1"])));
        v.push(("net-modem.c", s(&["R28.1", "C25.1"])));
        v.push(("net-modem.d", s(&["R29.1", "C26.1"])));
        v.push(("BATT_DIV", s(&["R1.2", "R2.1", "U1.3"])));
        v.push(("net-adc.amp", s(&["U1.1", "R3.1", "U1.2"])));
        v.push(("BATT_ADC", s(&["R3.2", "C1.1", "U5.2"])));
        v.push(("LED1", s(&["U5.3", "R4.1"])));
        v.push(("net-io.d1", s(&["R4.2", "D1.2"])));
        v.push(("LED2", s(&["U5.4", "R5.1"])));
        v.push(("net-io.d2", s(&["R5.2", "D2.2"])));
        v.push(("UART_TX", s(&["U5.5", "J7.1", "R7.1"])));
        v
    };
    // R5 has no ground of its own; D2's cathode goes to ground above.
    let design = Design {
        name: "Bench".into(),
        process: None,
        features: Default::default(),
        parts: parts
            .iter()
            .map(|&(refdes, kind, value, package)| Part {
                refdes: refdes.into(),
                kind: kind.into(),
                value: value.into(),
                package: package.map(str::to_string),
                path: String::new(),
                pins: Vec::new(),
            })
            .collect(),
        nets: nets
            .iter()
            .map(|(name, pins)| Net {
                name: (*name).into(),
                pins: pins
                    .iter()
                    .map(|p| {
                        let (refdes, pin) = p.split_once('.').unwrap();
                        PinRef {
                            refdes: refdes.into(),
                            pin: pin.into(),
                        }
                    })
                    .collect(),
            })
            .collect(),
    };
    let group = |name: &str, detail: &str, refs: &[&str]| Group {
        name: name.into(),
        detail: detail.into(),
        parts: refs
            .iter()
            .map(|r| design.parts.iter().position(|p| p.refdes == *r).unwrap())
            .collect(),
    };
    let groups = vec![
        group("gnss", "Gnss", &["U2", "J3"]),
        group("ldo", "Ldo3V3", &["U4", "C2", "C3"]),
        group("adc", "AdcMonitor", &["R1", "R2", "R3", "C1", "U1"]),
        group("mcu", "Mcu", &["U5", "C4", "C5", "Y1"]),
        group(
            "buck",
            "Buck1V8",
            &["U7", "C6", "C7", "L1", "C8", "R8", "R9", "R10", "R11", "C9"],
        ),
        group(
            "modem",
            "Modem",
            &[
                "J10", "J11", "R20", "R21", "R22", "R23", "R24", "R25", "D3", "C20", "C21", "C22",
                "C23", "C24", "R26", "R27", "R28", "R29", "C25", "C26",
            ],
        ),
        group(
            "io",
            "Indicators",
            &["J7", "J8", "R4", "D1", "R5", "D2", "R6", "R7"],
        ),
    ];
    let mut rails = Rails::default();
    rails.ground.insert("GND".into());
    rails.positive.insert("+3V8".into());
    rails.positive.insert("+3V3".into());
    rails.positive.insert("+5V_in".into());
    rails.positive.insert("+1V8".into());
    Bench {
        dir,
        design,
        groups,
        rails,
        mapping,
        lib,
    }
}
