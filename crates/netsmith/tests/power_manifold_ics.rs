//! `examples/power_manifold_charger/`: the three hard ICs of the Power Manifold charger module,
//! U1 (STM32G071C8T6), U3 (TPS55288RPMR) and U5 (TCPP02-M18): their pin tables, their footprints
//! (written from the makers' land patterns, in `libs/power_manifold_ics.pretty`) and their
//! catalog rows. None is a must-match part of the parity checker: they build, are sourced and
//! are compared with the reference board's pads here, and the tests say how close they are.
//!
//! A fixture board of the test's own (never the project's board) builds with the parts and no
//! KiCad install.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use netsmith::project::Target;
use netsmith::{Libraries, build_with};
use netsmith_core::PinKind;
use netsmith_kicad::install::Search;
use netsmith_pcb::Footprint;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/power_manifold_charger")
}

fn read(path: &str) -> String {
    fs::read_to_string(project().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn parsed(text: &str) -> toml::Table {
    text.parse::<toml::Table>()
        .unwrap_or_else(|e| panic!("the table is not TOML: {e}"))
}

fn number(table: &toml::Table, key: &str) -> f64 {
    match &table[key] {
        toml::Value::Float(f) => *f,
        toml::Value::Integer(i) => *i as f64,
        other => panic!("{key} is {other:?}, not a number"),
    }
}

/// A small board of our own, never built by the project: the three parts with the pins the
/// datasheets require joined to headers, a supply and a rail. The nets and the names are the
/// datasheets', nothing says they are the charger's.
const FIXTURE: &str = r#"
import src::stm32g071c8::Stm32G071C8
import src::tcpp02_m18::Tcpp02M18
import src::tps55288::Tps55288
import std::parts::connector::{Header1x2, Header1x4, Header1x8}
import std::sources::Rail

board IcsFixture {
    .rail: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..100mA
    }
    .usb: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VBUS"
        .neg.net &= "GND"
    }

    .u1: Stm32G071C8 { .mpn &= "STM32G071C8T6" }
    .u3: Tps55288 { .mpn &= "TPS55288RPMR" }
    .u5: Tcpp02M18 { .mpn &= "TCPP02-M18" }

    // U1: the three supply pads of the reference board are one rail.
    .rail ~ .u1.vdd
    .rail.pos ~ .u1.vbat
    .rail.pos ~ .u1.vrefp
    .mcu: Header1x8
    .u1.pf2 ~ .mcu.p1
    .u1.pa13 ~ .mcu.p2
    .u1.pa14 ~ .mcu.p3
    .u1.pb6 ~ .mcu.p4
    .u1.pb7 ~ .mcu.p5

    // U3: the input rail, the grounds and the inputs.
    .usb ~ .u3.vin
    .usb.neg ~ .u3.agnd
    .conv: Header1x8
    .u3.en ~ .conv.p1
    .u3.scl ~ .conv.p2
    .u3.sda ~ .conv.p3
    .u3.dith ~ .conv.p4
    .u3.mode ~ .conv.p5
    .u3.comp ~ .conv.p6
    .u3.isp ~ .conv.p7
    .u3.isn ~ .conv.p8
    .out: Header1x4
    .u3.vout.pos ~ .out.p1
    .u3.sw1 ~ .out.p2
    .u3.sw2 ~ .out.p3
    .u3.boot1 ~ .out.p4

    // U5: the supply and the inputs.
    .usb ~ .u5.vcc
    .prot: Header1x8
    .u5.en ~ .prot.p1
    .u5.src ~ .prot.p2
    .u5.vbusc ~ .prot.p3
    .u5.isense ~ .prot.p4
    .u5.i2c_add ~ .prot.p5
    .u5.cc1 ~ .prot.p6
    .u5.cc1c ~ .prot.p7
    .u5.gate ~ .prot.p8
}
"#;

/// A machine with no KiCad install, and the project's own `kicad.toml` and libraries.
fn libraries() -> Libraries {
    let nowhere = netsmith_testkit::TempDir::new("power-manifold-ics-no-kicad");
    Libraries::for_project_by(Some(&project()), &Search::under(nowhere.path()))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn built() -> netsmith::Built {
    let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
    build_with(
        "ics.nsm",
        FIXTURE,
        &target.loader().unwrap(),
        &target.catalogs().unwrap(),
        &libraries(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
}

/// A part's pins as the design has them: the datasheet's name, the kind and the pads, in order.
fn pins_of(built: &netsmith::Built, kind: &str) -> Vec<(String, PinKind, Vec<String>)> {
    part_of(built, kind)
        .pins
        .iter()
        .map(|p| (p.datasheet.clone(), p.kind, p.pads.clone()))
        .collect()
}

fn part_of<'a>(built: &'a netsmith::Built, kind: &str) -> &'a netsmith_core::Part {
    built
        .design
        .parts
        .iter()
        .find(|p| p.kind == kind)
        .unwrap_or_else(|| panic!("the board has no {kind}"))
}

/// The footprint a part is exported with, loaded as the build loads it.
fn footprint_of(built: &netsmith::Built, kind: &str) -> Footprint {
    let part = part_of(built, kind);
    let libraries = libraries();
    let mapped = libraries
        .mapping
        .map(&part.kind, part.package.as_deref())
        .unwrap_or_else(|e| panic!("{e}"));
    libraries
        .footprints
        .load(&mapped.footprint)
        .unwrap_or_else(|e| panic!("{}: {e}", mapped.footprint))
}

fn pad_numbers(pins: &[(String, PinKind, Vec<String>)]) -> BTreeSet<String> {
    pins.iter().flat_map(|p| p.2.iter().cloned()).collect()
}

fn numbered(footprint: &Footprint) -> BTreeSet<String> {
    footprint
        .pad_numbers()
        .into_iter()
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect()
}

/// A reference pad: its number, the box it covers in the footprint's frame as (min x, min y,
/// max x, max y), and its net.
struct RefPad {
    number: String,
    bounds: [f64; 4],
    net: String,
}

impl RefPad {
    fn centre(&self) -> (f64, f64) {
        (
            (self.bounds[0] + self.bounds[2]) / 2.0,
            (self.bounds[1] + self.bounds[3]) / 2.0,
        )
    }
    fn size(&self) -> (f64, f64) {
        (
            self.bounds[2] - self.bounds[0],
            self.bounds[3] - self.bounds[1],
        )
    }
}

/// The half of the width of the stroke of the reference's custom pads' polygons (0.1 mm): the
/// table's `extent` of a custom pad includes it, the polygon's own corners do not.
const REFERENCE_STROKE: f64 = 0.05;

/// The reference's pads of one footprint. A pad's file angle includes the footprint's rotation:
/// a pad a quarter turn from the footprint's own frame has its size the other way round. A custom
/// pad is its polygon's box (the table's `extent`, which counts the stroke, less the stroke).
fn reference_pads(reference: &str) -> Vec<RefPad> {
    let table = parsed(&read("reference/placements.toml"));
    let part = table["part"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_table().unwrap())
        .find(|p| p["reference"].as_str() == Some(reference))
        .unwrap_or_else(|| panic!("the reference has no {reference}"));
    let rotation = number(part, "rotation");
    part["pads"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            let p = p.as_table().unwrap();
            let f = |v: &toml::Value| v.as_float().or_else(|| v.as_integer().map(|i| i as f64));
            let (x, y) = (number(p, "x"), number(p, "y"));
            let turn = p.get("angle").map_or(0.0, |_| number(p, "angle")) - rotation;
            let bounds = if let Some(extent) = p.get("extent") {
                assert!(
                    turn.rem_euclid(360.0).abs() < 1e-6,
                    "a custom pad is turned"
                );
                let e: Vec<f64> = extent
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| f(v).unwrap())
                    .collect();
                [
                    x + e[0] + REFERENCE_STROKE,
                    y + e[1] + REFERENCE_STROKE,
                    x + e[2] - REFERENCE_STROKE,
                    y + e[3] - REFERENCE_STROKE,
                ]
            } else {
                let size = p["size"].as_array().unwrap();
                let (mut w, mut h) = (f(&size[0]).unwrap(), f(&size[1]).unwrap());
                if (turn.rem_euclid(180.0) - 90.0).abs() < 1e-6 {
                    std::mem::swap(&mut w, &mut h);
                }
                [x - w / 2.0, y - h / 2.0, x + w / 2.0, y + h / 2.0]
            };
            RefPad {
                number: p["number"].as_str().unwrap().to_string(),
                bounds,
                net: p["net"].as_str().unwrap().to_string(),
            }
        })
        .collect()
}

/// How close the numbered pads of `footprint` are to the reference's: for each reference pad
/// the pad of its number (each used once) nearest to it, as (number, distance between the
/// centres of the boxes, largest difference of a side of the box). The two have the same
/// numbers, each once or as often as the other.
fn compare(footprint: &Footprint, reference: &str) -> Vec<(String, f64, f64)> {
    let wanted = reference_pads(reference);
    let own: Vec<_> = footprint
        .pads
        .iter()
        .filter(|p| !p.number.is_empty())
        .collect();
    assert_eq!(own.len(), wanted.len(), "{reference}: the number of pads");
    let mut used = vec![false; own.len()];
    let mut out = Vec::new();
    for r in &wanted {
        let (cx, cy) = r.centre();
        let (i, d) = own
            .iter()
            .enumerate()
            .filter(|(i, p)| !used[*i] && p.number == r.number)
            .map(|(i, p)| {
                let b = p.bounds();
                let c = ((b.min.x + b.max.x) / 2.0, (b.min.y + b.max.y) / 2.0);
                (i, ((c.0 - cx).powi(2) + (c.1 - cy).powi(2)).sqrt())
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap_or_else(|| panic!("{reference}: no pad {} left", r.number));
        used[i] = true;
        let b = own[i].bounds();
        let (rw, rh) = r.size();
        let size = ((b.max.x - b.min.x) - rw)
            .abs()
            .max(((b.max.y - b.min.y) - rh).abs());
        out.push((r.number.clone(), d, size));
    }
    out
}

fn farthest(rows: &[(String, f64, f64)]) -> f64 {
    rows.iter().map(|r| r.1).fold(0.0, f64::max)
}

fn largest(rows: &[(String, f64, f64)]) -> f64 {
    rows.iter().map(|r| r.2).fold(0.0, f64::max)
}

#[test]
fn the_fixture_builds_with_no_kicad_and_every_part_is_sourced() {
    let target = Target::resolve_from(&project(), None).unwrap_or_else(|e| panic!("{e}"));
    assert!(target.catalogs().is_ok(), "the project's catalogs load");
    let built = built();
    // The build ran `validate_pinout` on each part (a pad of the pin table that the footprint
    // lacks, or a footprint pad that no pin names, stops it).
    assert!(!built.schematic.is_empty());
    for (kind, lcsc, mpn, package, library) in [
        (
            "Stm32G071C8",
            "C529341",
            "STM32G071C8T6",
            "LQFP-48(7x7)",
            "Extended",
        ),
        (
            "Tps55288",
            "C2864583",
            "TPS55288RPMR",
            "VQFN-26-HR(3.5x4)",
            "Extended",
        ),
        (
            "Tcpp02M18",
            "C3662929",
            "TCPP02-M18",
            "QFN-18 (3.5x3.5)",
            "Extended",
        ),
    ] {
        let row = built
            .sourcing
            .iter()
            .find(|s| s.kind == kind)
            .unwrap_or_else(|| panic!("no sourcing row of {kind}"));
        assert_eq!(row.lcsc.as_deref(), Some(lcsc), "{kind}");
        assert_eq!(row.mpn.as_deref(), Some(mpn), "{kind}");
        assert_eq!(row.package.as_deref(), Some(package), "{kind}");
        assert_eq!(format!("{:?}", row.library.unwrap()), library, "{kind}");
    }
}

#[test]
fn the_symbols_are_drawn_from_the_pin_tables() {
    let built = built();
    for (kind, package) in [
        ("Stm32G071C8", "LQFP-48(7x7)"),
        ("Tps55288", "VQFN-26-HR(3.5x4)"),
        ("Tcpp02M18", "QFN-18 (3.5x3.5)"),
    ] {
        let mapped = libraries().mapping.map(kind, Some(package)).unwrap();
        assert_eq!(mapped.lib_id, None, "{kind}: no KiCad symbol is named");
        let name = format!(
            "netsmith:{}",
            netsmith_kicad::generate::symbol_name(kind, Some(package))
        );
        assert!(
            built.schematic.contains(&format!("(lib_id \"{name}")),
            "no instance of the symbol `{name}`"
        );
        for (datasheet, _, pads) in pins_of(&built, kind) {
            assert!(
                built.schematic.contains(&format!("(name \"{datasheet}\"")),
                "no pin {datasheet}"
            );
            for pad in pads {
                assert!(
                    built.schematic.contains(&format!("(number \"{pad}\"")),
                    "{kind}: no pad {pad}"
                );
            }
        }
    }
}

// ---- U1, the microcontroller ----

/// The pin names of the LQFP48 column of DS12232 Rev 3, Figure 5 and Table 12, by pad number.
const STM32_NAMES: [&str; 48] = [
    "PC13",
    "PC14-OSC32_IN",
    "PC15-OSC32_OUT",
    "VBAT",
    "VREF+",
    "VDD/VDDA",
    "VSS/VSSA",
    "PF0-OSC_IN",
    "PF1-OSC_OUT",
    "PF2-NRST",
    "PA0",
    "PA1",
    "PA2",
    "PA3",
    "PA4",
    "PA5",
    "PA6",
    "PA7",
    "PB0",
    "PB1",
    "PB2",
    "PB10",
    "PB11",
    "PB12",
    "PB13",
    "PB14",
    "PB15",
    "PA8",
    "PA9",
    "PC6",
    "PC7",
    "PA10",
    "PA11 [PA9]",
    "PA12 [PA10]",
    "PA13",
    "PA14-BOOT0",
    "PA15",
    "PD0",
    "PD1",
    "PD2",
    "PD3",
    "PB3",
    "PB4",
    "PB5",
    "PB6",
    "PB7",
    "PB8",
    "PB9",
];

#[test]
fn the_microcontroller_has_the_forty_eight_pins_of_the_datasheet() {
    let built = built();
    let pins = pins_of(&built, "Stm32G071C8");
    let want: BTreeSet<String> = (1..=48).map(|n| n.to_string()).collect();
    assert_eq!(pad_numbers(&pins), want, "pads 1 to 48, each in one pin");
    let total: usize = pins.iter().map(|p| p.2.len()).sum();
    assert_eq!(total, 48, "no pad is on two pins");
    for (i, name) in STM32_NAMES.iter().enumerate() {
        let pad = (i + 1).to_string();
        let pin = pins
            .iter()
            .find(|p| p.2.contains(&pad))
            .unwrap_or_else(|| panic!("pad {pad}"));
        assert_eq!(pin.0, *name, "pad {pad}");
        // Table 12: the four `S` pins are supplies, every other pin is `I/O`.
        let supply = matches!(pad.as_str(), "4" | "5" | "6" | "7");
        assert_eq!(
            pin.1,
            if supply {
                PinKind::Power
            } else {
                PinKind::Bidirectional
            },
            "pad {pad} {name}"
        );
    }
}

#[test]
fn the_microcontroller_supply_is_the_datasheets_window() {
    let built = built();
    let part = part_of(&built, "Stm32G071C8");
    let on = |name: &str| {
        part.pins
            .iter()
            .find(|p| p.datasheet == name)
            .unwrap_or_else(|| panic!("no pin {name}"))
    };
    // The rail `.vdd` is the `VDD/VDDA` and `VSS/VSSA` pads (Table 21: VDD 1.7 V to 3.6 V); the
    // battery and reference pins are plain pins of the part, not of the rail.
    assert_eq!(on("VDD/VDDA").pads, ["6"]);
    assert_eq!(on("VSS/VSSA").pads, ["7"]);
    assert_eq!(on("VBAT").pads, ["4"]);
    assert_eq!(on("VREF+").pads, ["5"]);
    let source = read("src/stm32g071c8.nsm");
    assert!(source.contains(".vdd.voltage >= 1.7V") && source.contains(".vdd.voltage <= 3.6V"));
}

#[test]
fn the_microcontroller_pads_agree_with_the_nets_of_the_reference_u1() {
    let built = built();
    let pins = pins_of(&built, "Stm32G071C8");
    let name_of = |pad: &str| {
        pins.iter()
            .find(|p| p.2.iter().any(|x| x == pad))
            .unwrap()
            .0
            .clone()
    };
    // The name that KiCad's symbol gave a pin is in the net name of a pin left open or only
    // on a network: `unconnected-(U1-<name>-Pad<n>)` and `Net-(U1-<name>)`. The symbol writes
    // the remappable pins as `PA11[PA9]` (the datasheet's figure has a space).
    let squash = |s: &str| s.replace(' ', "");
    let (mut open, mut network, mut named) = (0, 0, 0);
    for pad in reference_pads("U1") {
        let pin = name_of(&pad.number);
        if let Some(rest) = pad.net.strip_prefix("unconnected-(U1-") {
            let name = rest.rsplit_once("-Pad").unwrap().0;
            assert_eq!(
                squash(name),
                squash(&pin),
                "pad {}: {}",
                pad.number,
                pad.net
            );
            open += 1;
        } else if let Some(rest) = pad.net.strip_prefix("Net-(U1-") {
            let name = rest.strip_suffix(')').unwrap();
            assert_eq!(name, pin, "pad {}: {}", pad.number, pad.net);
            network += 1;
        } else {
            // The pads on the board's named nets. The functions are the datasheet's alternate
            // functions (Tables 13 and 14, printed pages 48 and 49): USART2_TX and USART2_RX
            // on PA2 and PA3 (AF1), I2C1_SCL and I2C1_SDA on PB6 and PB7 (AF6), I2C2_SCL and
            // I2C2_SDA on PB10 and PB11 (AF6), SWDIO and SWCLK on PA13 and PA14 (AF0).
            let want = match pad.net.as_str() {
                "+3.3V" => match pad.number.as_str() {
                    "4" => "VBAT",
                    "5" => "VREF+",
                    _ => "VDD/VDDA",
                },
                "GND" => match pad.number.as_str() {
                    "7" => "VSS/VSSA",
                    // The board ties PA9 and PA10 to ground (they are the pads of the dead
                    // battery function of the USB Type-C PD pins, FT_d, Table 12).
                    "29" => "PA9",
                    "32" => "PA10",
                    other => panic!("pad {other} is on GND"),
                },
                "/~{RST}" => "PF2-NRST",
                "/USART_TX" => "PA2",
                "/USART_RX" => "PA3",
                "/USB_SCL" => "PB10",
                "/USB_SDA" => "PB11",
                "/SCL" => "PB6",
                "/SDA" => "PB7",
                "/SWDIO" => "PA13",
                "/SWCLK" => "PA14-BOOT0",
                // The names the board gives the other lines say nothing about the pin; the
                // number is the datasheet's: the analog inputs (PA0, PA1, PA4, PB0, PB1, PB2),
                // the I/O pads PB12, PB13, PB15, PA8, PB4, PB5 and PB8.
                "/VBUS_SENSE" => "PA0",
                "/IANA" => "PA1",
                "/OVP_REF" => "PA4",
                "/NTC_CONV" => "PB0",
                "/VOUT_SENSE" => "PB1",
                "/NTC_PLUG" => "PB2",
                "/CONVERTER_FLT" => "PB12",
                "/~{FLG}" => "PB13",
                "/UCPD_CC2" => "PB15",
                "/UCPD_CC1" => "PA8",
                "/TCPP_EN" => "PB4",
                "/~{ALERT}" => "PB5",
                "/EN" => "PB8",
                other => panic!("pad {}: net {other}", pad.number),
            };
            assert_eq!(pin, want, "pad {} is on {}", pad.number, pad.net);
            named += 1;
        }
    }
    assert_eq!(
        (open, network, named),
        (18, 2, 28),
        "18 pads left open, 2 on a network, 28 on named nets"
    );
}

#[test]
fn the_microcontroller_footprint_is_sts_land_pattern_and_says_how_it_differs_from_the_reference() {
    let built = built();
    let footprint = footprint_of(&built, "Stm32G071C8");
    assert_eq!(footprint.name, "LQFP-48_7x7mm_P0.5mm_ST_DS12232");
    assert_eq!(footprint.pads.len(), 48);
    assert_eq!(
        numbered(&footprint),
        pad_numbers(&pins_of(&built, "Stm32G071C8"))
    );
    // DS12232 Rev 3, Figure 39 (printed page 114): 1.20 x 0.30 mm pads on a 0.50 mm pitch, 9.70
    // mm from outer edge to outer edge, 5.80 mm along a side from outer edge to outer edge (12
    // pads: 11 pitches and one pad width), 7.30 mm from inner edge to inner edge.
    let pad = |n: &str| footprint.pads.iter().find(|p| p.number == n).unwrap();
    for n in 1..=48 {
        let p = pad(&n.to_string());
        let (long, short) = if (13..=24).contains(&n) || n >= 37 {
            (p.width, p.height)
        } else {
            (p.height, p.width)
        };
        assert_eq!((long, short), (1.2, 0.3), "pad {n}");
    }
    let outer = |a: &str, b: &str, side: bool| {
        let (a, b) = (pad(a).bounds(), pad(b).bounds());
        if side {
            (b.max.x - a.min.x).abs()
        } else {
            (b.max.y - a.min.y).abs()
        }
    };
    assert!((outer("48", "13", true) - 9.7).abs() < 1e-9, "9.70 across");
    assert!((outer("36", "1", false) - 9.7).abs() < 1e-9, "9.70 down");
    let (first, last) = (pad("1").bounds(), pad("12").bounds());
    assert!((last.max.x - first.min.x - 5.8).abs() < 1e-9, "5.80 along");
    let (left, right) = (pad("37").bounds(), pad("24").bounds());
    assert!(
        (right.min.x - left.max.x - 7.3).abs() < 1e-9,
        "7.30 between"
    );
    // Pad 1 is at the bottom left and the numbers run counter-clockwise, as the datasheet's
    // figure and the reference have them.
    assert_eq!((pad("1").at.x, pad("1").at.y), (-2.75, 4.25));
    assert_eq!((pad("13").at.x, pad("13").at.y), (4.25, 2.75));
    assert_eq!((pad("25").at.x, pad("25").at.y), (2.75, -4.25));
    assert_eq!((pad("37").at.x, pad("37").at.y), (-4.25, -2.75));
    // The reference's pads (1.50 x 0.27 mm): the same numbers and centres, a pad 0.30 mm longer
    // than ST's and 0.03 mm narrower. The footprint is ST's and not the reference's.
    let rows = compare(&footprint, "U1");
    assert!(farthest(&rows) <= 0.01, "centres: {}", farthest(&rows));
    for (n, _, size) in &rows {
        assert!(
            (size - 0.3).abs() < 0.01,
            "pad {n}: the sides differ by {size}"
        );
    }
}

// ---- U3, the converter ----

#[test]
fn the_converter_has_the_twenty_six_pins_of_the_datasheet() {
    let built = built();
    let pins = pins_of(&built, "Tps55288");
    let want: BTreeSet<String> = (1..=26).map(|n| n.to_string()).collect();
    assert_eq!(pad_numbers(&pins), want);
    let total: usize = pins.iter().map(|p| p.2.len()).sum();
    assert_eq!(total, 26, "no pad is on two pins");
    let pin = |name: &str| {
        pins.iter()
            .find(|p| p.0 == name)
            .unwrap_or_else(|| panic!("no pin {name}"))
    };
    // Table 5-1 (printed pages 3 and 4): the pads, the names and the I/O column.
    for (name, pads, kind) in [
        ("DR1L", vec!["1"], PinKind::Output),
        ("DR1H", vec!["2"], PinKind::Output),
        ("VIN", vec!["3"], PinKind::Power),
        ("EN/UVLO", vec!["4"], PinKind::Input),
        ("SCL", vec!["5"], PinKind::Input),
        ("SDA", vec!["6"], PinKind::Bidirectional),
        ("DITH/SYNC", vec!["7"], PinKind::Input),
        ("FSW", vec!["8"], PinKind::Output),
        ("PGND", vec!["9", "24"], PinKind::Power),
        ("AGND", vec!["10"], PinKind::Power),
        ("VOUT", vec!["11", "26"], PinKind::Power),
        ("ISP", vec!["12"], PinKind::Input),
        ("ISN", vec!["13"], PinKind::Input),
        ("FB/INT", vec!["14"], PinKind::Bidirectional),
        ("MODE", vec!["15"], PinKind::Input),
        ("CDC", vec!["16"], PinKind::Output),
        ("ILIM", vec!["17"], PinKind::Output),
        ("COMP", vec!["18"], PinKind::Input),
        ("VCC", vec!["19"], PinKind::Output),
        // The table prints I for SW1, SW2 and BOOT1 and O for BOOT2: the power stage's nodes
        // and the bootstrap capacitors' pins are `passive` here (the part file says why).
        ("BOOT2", vec!["20"], PinKind::Passive),
        ("SW2", vec!["21", "25"], PinKind::Passive),
        ("BOOT1", vec!["22"], PinKind::Passive),
        ("SW1", vec!["23"], PinKind::Passive),
    ] {
        let p = pin(name);
        assert_eq!(
            (&p.2, p.1),
            (&pads.iter().map(|s| s.to_string()).collect(), kind),
            "{name}"
        );
    }
}

#[test]
fn the_converter_pads_agree_with_the_nets_of_the_reference_u3() {
    let built = built();
    let pins = pins_of(&built, "Tps55288");
    let name_of = |pad: &str| {
        pins.iter()
            .find(|p| p.2.iter().any(|x| x == pad))
            .unwrap()
            .0
            .clone()
    };
    // KiCad's names for the pins, in the nets that have none of the board's own: the symbol
    // writes `DITH/SYNC` as `DITH{slash}SYNC`.
    let (mut open, mut network, mut named) = (0, 0, 0);
    for pad in reference_pads("U3") {
        let pin = name_of(&pad.number);
        if let Some(rest) = pad.net.strip_prefix("unconnected-(U3-") {
            let name = rest.rsplit_once("-Pad").unwrap().0;
            assert_eq!(name, pin, "pad {}", pad.number);
            open += 1;
        } else if let Some(rest) = pad.net.strip_prefix("Net-(U3-") {
            let name = rest.strip_suffix(')').unwrap().replace("{slash}", "/");
            assert_eq!(name, pin, "pad {}: {}", pad.number, pad.net);
            network += 1;
        } else {
            let want = match pad.net.as_str() {
                "/DR1L" => "DR1L",
                "/DR1H" => "DR1H",
                "/EMI_VIN" => "VIN",
                "/EN" => "EN/UVLO",
                "/USB_SCL" => "SCL",
                "/USB_SDA" => "SDA",
                "GND" => match pad.number.as_str() {
                    "9" | "24" => "PGND",
                    "10" => "AGND",
                    other => panic!("pad {other} is on GND"),
                },
                "/VOUT_CONV" => "VOUT",
                "/VOUT" => "ISN",
                "/CONVERTER_FLT" => "FB/INT",
                "/SW1" => "SW1",
                "/SW2" => "SW2",
                other => panic!("pad {}: net {other}", pad.number),
            };
            // Pad 12, ISP, is on the same net as VOUT (`/VOUT_CONV`); pad 13, ISN, is on `/VOUT`
            // (the other side of the sense resistor).
            let want = if pad.number == "12" { "ISP" } else { want };
            assert_eq!(pin, want, "pad {} is on {}", pad.number, pad.net);
            named += 1;
        }
    }
    assert_eq!(
        (open, network, named),
        (1, 8, 17),
        "1 pad left open, 8 on a network, 17 on named nets"
    );
}

#[test]
fn the_converter_footprint_is_tis_land_pattern_and_the_references_pads() {
    let built = built();
    let footprint = footprint_of(&built, "Tps55288");
    assert_eq!(footprint.name, "VQFN-HR-26_3.5x4mm_P0.5mm_TI_RPM0026A");
    assert_eq!(footprint.pads.len(), 26);
    assert_eq!(
        numbered(&footprint),
        pad_numbers(&pins_of(&built, "Tps55288"))
    );
    // TI's example board layout (RPM0026A, drawing 4224618/A): the middle pads are 0.375 x
    // 1.425 mm, 0.625 mm apart; the lead pads on the sides 0.25 mm wide, 0.5 mm apart.
    let pad = |n: &str| footprint.pads.iter().find(|p| p.number == n).unwrap();
    for n in ["24", "25", "26"] {
        assert_eq!((pad(n).width, pad(n).height), (0.375, 1.425), "pad {n}");
    }
    assert!((pad("25").at.x - pad("24").at.x - 0.625).abs() < 1e-9);
    assert!((pad("26").at.x - pad("25").at.x - 0.625).abs() < 1e-9);
    for (a, b) in [("2", "3"), ("3", "4"), ("4", "5"), ("5", "6")] {
        assert!((pad(b).at.y - pad(a).at.y - 0.5).abs() < 1e-9, "{a} {b}");
    }
    // The four corner pads are L shaped: a bar of 0.4 x 0.575 mm (the anchor) and an arm, and
    // each covers 0.425 + 0.4 + 0.375 = 1.2 mm along the side and 0.575 mm across it.
    for n in ["1", "7", "13", "19"] {
        let b = pad(n).bounds();
        assert!(
            ((b.max.x - b.min.x) - 0.775).abs() < 1e-9
                && ((b.max.y - b.min.y) - 0.575).abs() < 1e-9,
            "pad {n}: {b:?}"
        );
    }
    // The reference's pads (an import whose lead pads and corner polygons differ from TI's
    // example by a few hundredths): every pad's box within 0.05 mm of the reference's. The
    // largest difference of a centre is 0.041 mm (the corner pads and pads 11 and 12) and of a
    // side 0.025 mm (the corner pads).
    let rows = compare(&footprint, "U3");
    assert!(farthest(&rows) <= 0.05, "centres: {rows:?}");
    assert!(largest(&rows) <= 0.05, "sizes: {rows:?}");
}

// ---- U5, the USB Type-C protection ----

#[test]
fn the_protection_ic_has_the_eighteen_pins_and_the_exposed_pad_of_the_datasheet() {
    let built = built();
    let pins = pins_of(&built, "Tcpp02M18");
    let want: BTreeSet<String> = (1..=19).map(|n| n.to_string()).collect();
    assert_eq!(
        pad_numbers(&pins),
        want,
        "pads 1 to 18 and the exposed pad 19"
    );
    let total: usize = pins.iter().map(|p| p.2.len()).sum();
    assert_eq!(total, 19, "no pad is on two pins");
    let pin = |name: &str| {
        pins.iter()
            .find(|p| p.0 == name)
            .unwrap_or_else(|| panic!("no pin {name}"))
    };
    // Table 1 (printed page 2): the pads, the names and the type column.
    for (name, pads, kind) in [
        ("EN", vec!["1"], PinKind::Input),
        ("CC1", vec!["2"], PinKind::Bidirectional),
        ("VCC_VCONN", vec!["3"], PinKind::Power),
        ("CC2", vec!["4"], PinKind::Bidirectional),
        ("IANA", vec!["5"], PinKind::Output),
        ("GATE", vec!["6"], PinKind::Output),
        ("SRC", vec!["7"], PinKind::Input),
        ("VBUSc", vec!["8"], PinKind::Input),
        ("Isense", vec!["9"], PinKind::Input),
        ("GND", vec!["10", "14", "19"], PinKind::Power),
        ("CC2c", vec!["11"], PinKind::Bidirectional),
        ("CBIAS", vec!["12"], PinKind::Output),
        ("CC1c", vec!["13"], PinKind::Bidirectional),
        ("I2C_ADD", vec!["15"], PinKind::Input),
        ("SDA", vec!["16"], PinKind::Bidirectional),
        ("SCL", vec!["17"], PinKind::Bidirectional),
        ("FLGn", vec!["18"], PinKind::Output),
    ] {
        let p = pin(name);
        assert_eq!(
            (&p.2, p.1),
            (&pads.iter().map(|s| s.to_string()).collect(), kind),
            "{name}"
        );
    }
}

#[test]
fn the_protection_ic_pads_agree_with_the_nets_of_the_reference_u5() {
    let built = built();
    let pins = pins_of(&built, "Tcpp02M18");
    let name_of = |pad: &str| {
        pins.iter()
            .find(|p| p.2.iter().any(|x| x == pad))
            .unwrap()
            .0
            .clone()
    };
    let mut named = 0;
    for pad in reference_pads("U5") {
        let pin = name_of(&pad.number);
        // The reference's nets, by what the datasheet says each pin is: `TCPP_EN` the enable,
        // `UCPD_CC1` and `UCPD_CC2` the controller side's CC lines, `+5V` the supply, `IANA`
        // the current monitor, `VBUS_GATE`, `VBUS_SRC`, `VBUS` and `VBUSf` the gate, the
        // source, the connector side and the sense pins, `CC1` and `CC2` the connector side,
        // `Net-(U5-CBIAS)` the capacitor, `USB_SDA`, `USB_SCL` and `~{FLG}` the I2C pins and
        // the flag. Pads 10, 14, 15 and 19 are on `GND` (the I2C address is 0).
        let want = match (pad.net.as_str(), pad.number.as_str()) {
            ("/TCPP_EN", _) => "EN",
            ("/UCPD_CC1", _) => "CC1",
            ("+5V", _) => "VCC_VCONN",
            ("/UCPD_CC2", _) => "CC2",
            ("/IANA", _) => "IANA",
            ("/VBUS_GATE", _) => "GATE",
            ("/VBUS_SRC", _) => "SRC",
            ("/VBUS", _) => "VBUSc",
            ("/VBUSf", _) => "Isense",
            ("/CC2", _) => "CC2c",
            ("Net-(U5-CBIAS)", _) => "CBIAS",
            ("/CC1", _) => "CC1c",
            ("/USB_SDA", _) => "SDA",
            ("/USB_SCL", _) => "SCL",
            ("/~{FLG}", _) => "FLGn",
            ("GND", "15") => "I2C_ADD",
            ("GND", _) => "GND",
            (other, n) => panic!("pad {n}: net {other}"),
        };
        assert_eq!(pin, want, "pad {} is on {}", pad.number, pad.net);
        named += 1;
    }
    assert_eq!(named, 19, "every pad of the reference was compared");
}

#[test]
fn the_protection_ic_footprint_is_sts_land_pattern_and_says_how_it_differs_from_the_reference() {
    let built = built();
    let footprint = footprint_of(&built, "Tcpp02M18");
    assert_eq!(footprint.name, "QFN-18-1EP_3.5x3.5mm_P0.5mm_EP2.2x2.2mm_ST");
    assert_eq!(footprint.pads.len(), 19);
    assert_eq!(
        numbered(&footprint),
        pad_numbers(&pins_of(&built, "Tcpp02M18"))
    );
    // DS13787 Rev 4, Figure 12 (printed page 20): lead pads 0.50 x 0.30 mm on a 0.50 mm pitch,
    // 3.60 mm from outer edge to outer edge each way, the exposed pad 2.20 x 2.20 mm with a
    // gap of 0.20 mm to the lead pads.
    let pad = |n: &str| footprint.pads.iter().find(|p| p.number == n).unwrap();
    for n in 1..=18 {
        let p = pad(&n.to_string());
        let (long, short) = if (1..=5).contains(&n) || (10..=14).contains(&n) {
            (p.width, p.height)
        } else {
            (p.height, p.width)
        };
        assert_eq!((long, short), (0.5, 0.3), "pad {n}");
    }
    assert_eq!((pad("19").width, pad("19").height), (2.2, 2.2));
    let across = pad("13").bounds().max.x - pad("1").bounds().min.x;
    let down = pad("9").bounds().max.y - pad("18").bounds().min.y;
    assert!((across - 3.6).abs() < 1e-9 && (down - 3.6).abs() < 1e-9);
    let gap = pad("10").bounds().min.x - pad("19").bounds().max.x;
    assert!((gap - 0.2).abs() < 1e-9, "the gap is {gap}");
    // Pad 1 is at the top of the left side, the numbers run counter-clockwise.
    assert_eq!((pad("1").at.x, pad("1").at.y), (-1.55, -1.0));
    assert_eq!((pad("6").at.x, pad("6").at.y), (-0.75, 1.55));
    assert_eq!((pad("10").at.x, pad("10").at.y), (1.55, 1.0));
    assert_eq!((pad("15").at.x, pad("15").at.y), (0.75, -1.55));
    // The reference's pads: the same numbers, the exposed pad the same (2.2 mm), the lead pads
    // 0.79 x 0.28 mm centred 1.71 mm from the middle: 0.16 mm farther out and 0.29 mm longer
    // than ST's. The footprint is ST's and not the reference's.
    let rows = compare(&footprint, "U5");
    for (n, far, size) in &rows {
        if n == "19" {
            assert!(
                *far <= 0.01 && *size <= 0.01,
                "the exposed pad: {far} {size}"
            );
        } else {
            assert!((far - 0.16).abs() < 0.01, "pad {n}: centres {far} mm apart");
            assert!(
                (size - 0.29).abs() < 0.02,
                "pad {n}: sides differ by {size}"
            );
        }
    }
}
