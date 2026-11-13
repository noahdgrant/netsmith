//! A chain of series parts between two main parts (a fuse and a reverse-polarity diode between a
//! vehicle connector and a header, a TVS diode on the far end) is drawn as one row from the part
//! the power comes from, on the left, to the part it goes to, on the right: no power symbol joins
//! the pieces, the parts are turned along the row, and the end that has the clamp on it is the
//! load.

use crate::common;

use netsmith_core::Design;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_testkit::TempDir;
use netsmith_view::check::check;
use netsmith_view::geom::Pt;
use netsmith_view::layout::{Layout, PowerKind};
use netsmith_view::metrics::measure;
use netsmith_view::{Rails, layout_rails};

const STEP: f64 = 1.27;

fn pin(kind: &str, x: f64, y: f64, angle: u32, name: &str, number: u32) -> String {
    format!(
        "(pin {kind} line (at {x} {y} {angle}) (length 2.54) \
         (name \"{name}\" (effects (font (size 1.27 1.27)))) \
         (number \"{number}\" (effects (font (size 1.27 1.27)))))"
    )
}

/// `Obd`: sixteen pins, the first eight on the left and the last eight on the right, as the
/// vehicle connector of J1962 is usually drawn. `Series2`: two pins at the ends of a short body.
fn library() -> String {
    let mut obd = String::new();
    for i in 0..8u32 {
        let y = 8.89 - 2.54 * f64::from(i);
        obd += &pin("passive", -12.7, y, 0, &format!("P{}", i + 1), i + 1);
        obd += &pin("passive", 12.7, y, 180, &format!("P{}", i + 9), i + 9);
    }
    format!(
        "(kicad_symbol_lib (symbol \"Obd\" (property \"Reference\" \"J\" (at 0 12 0)) \
         (symbol \"Obd_0_1\" (rectangle (start -10.16 10.16) (end 10.16 -10.16) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Obd_1_1\" {obd})) \
         (symbol \"Series2\" (property \"Reference\" \"X\" (at 2 0 0)) \
         (symbol \"Series2_0_1\" (rectangle (start -1.0 2.54) (end 1.0 -2.54) \
         (stroke (width 0.254) (type default)) (fill (type none)))) \
         (symbol \"Series2_1_1\" {} {})))",
        pin("passive", 0.0, 3.81, 270, "~", 1),
        pin("passive", 0.0, -3.81, 90, "~", 2)
    )
}

fn setup() -> (TempDir, Mapping, SymbolLib) {
    let dir = TempDir::new("view-link");
    let file = dir.path().join("link.kicad_sym");
    std::fs::write(&file, library()).unwrap();
    let mut rows = String::new();
    for (kind, package, symbol) in [
        ("Obd", "J", "Obd"),
        ("LkFuse", "P", "Series2"),
        ("LkDiode", "P", "Series2"),
        ("LkTvs", "P", "Series2"),
    ] {
        rows += &format!(
            "[[part]]\nkind = \"{kind}\"\npackage = \"{package}\"\nsymbol = \"link:{symbol}\"\nfootprint = \"link:F\"\n"
        );
    }
    let mapping = Mapping::standard()
        .merged(Mapping::parse(&rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(netsmith_testkit::sample_symbols()).with_library("link", file);
    (dir, mapping, lib)
}

/// A vehicle's input protection: a vehicle connector `J2` (battery power on pin 16, ground on 4 and 5), a
/// fuse `F1` and a diode `D1` in series from it, a TVS diode `D2` on the far node, and a header
/// `J1` that takes the protected supply. The header is listed first, as it was.
fn guard() -> Design {
    common::design(
        "VehicleInput",
        &[
            ("J1", "Header1x2", "Conn_01x02", None),
            ("J2", "Obd", "OBDII", Some("J")),
            ("F1", "LkFuse", "3A", Some("P")),
            ("D1", "LkDiode", "SS3H10", Some("P")),
            ("D2", "LkTvs", "SMDJ36A", Some("P")),
        ],
        &[
            ("+VBATT", &["J2.16", "F1.1"]),
            ("FUSED", &["F1.2", "D1.1"]),
            ("+VBATT_SAFE", &["D1.2", "D2.1", "J1.1"]),
            ("GND", &["J2.4", "J2.5", "D2.2", "J1.2"]),
        ],
    )
}

fn rails() -> Rails {
    let mut r = Rails::default();
    r.ground.insert("GND".into());
    r.positive.insert("+VBATT".into());
    r.positive.insert("+VBATT_SAFE".into());
    r
}

struct Drawn {
    design: Design,
    layout: Layout,
    _dir: TempDir,
}

fn draw(design: Design) -> Drawn {
    let (dir, mapping, lib) = setup();
    let layout = layout_rails(&design, &rails(), &mapping, &lib).unwrap();
    if let Ok(path) = std::env::var("NETSMITH_LINK_SVG") {
        std::fs::write(path, netsmith_view::render_layout(&design, &layout)).unwrap();
    }
    Drawn {
        design,
        layout,
        _dir: dir,
    }
}

fn part(d: &Design, refdes: &str) -> usize {
    d.parts.iter().position(|p| p.refdes == refdes).unwrap()
}

fn pin_at(d: &Drawn, refdes: &str, pad: &str) -> Pt {
    d.layout.parts[part(&d.design, refdes)]
        .pins
        .iter()
        .find(|p| p.id == pad)
        .unwrap()
        .at
}

/// Both pins of a two-pin part are on one horizontal line: its y, or `None`.
fn row_of(d: &Drawn, refdes: &str) -> Option<f64> {
    let (a, b) = (pin_at(d, refdes, "1"), pin_at(d, refdes, "2"));
    ((a.y - b.y).abs() < 1e-6).then_some(a.y)
}

fn names(d: &Drawn, c: &[usize]) -> Vec<String> {
    c.iter()
        .map(|&p| d.design.parts[p].refdes.clone())
        .collect()
}

#[test]
fn the_chain_between_two_connectors_is_one_row_from_the_source_to_the_load() {
    let d = draw(guard());
    assert_eq!(d.layout.spines.len(), 1, "{:?}", d.layout.spines);
    let mark = &d.layout.spines[0];
    // The vehicle connector has no clamp on its end: it is the source, and the chain runs from it.
    assert_eq!(d.design.parts[mark.ic].refdes, "J2");
    assert_eq!(names(&d, &mark.chains[0]), ["F1", "D1"]);

    // The connector, the fuse, the diode, the header: left to right, on one line.
    let row = pin_at(&d, "J2", "16").y;
    for r in ["F1", "D1"] {
        assert_eq!(row_of(&d, r), Some(row), "{r} is not on the row");
    }
    assert_eq!(
        pin_at(&d, "J1", "1").y,
        row,
        "the header is not level with the row"
    );
    let x = |r: &str, pad: &str| pin_at(&d, r, pad).x;
    assert!(
        x("J2", "16") < x("F1", "1") && x("F1", "2") < x("D1", "1") && x("D1", "2") < x("J1", "1")
    );

    let m = &measure(&d.layout).sheets[0];
    assert_eq!(m.spine_bends, 0, "{m:?}");
    assert_eq!(m.detached_chain, 0);
    assert_eq!(m.crossings, 0, "{m:?}");
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn the_pins_that_face_the_row_are_the_ones_on_it() {
    let d = draw(guard());
    // The source's battery pin is on its right edge; the load's first pin on its left.
    let j2 = &d.layout.parts[part(&d.design, "J2")];
    let p16 = pin_at(&d, "J2", "16");
    assert!(
        j2.pins
            .iter()
            .filter(|p| p.id != "16")
            .all(|p| p.at.x <= p16.x + 1e-6 || p.id == "9")
    );
    assert!(pin_at(&d, "J1", "1").x > p16.x);
}

#[test]
fn the_source_end_has_no_symbol_and_the_load_end_has_the_one() {
    let d = draw(guard());
    let net = |name: &str| d.design.nets.iter().position(|n| n.name == name).unwrap();
    let supplies: Vec<usize> = d
        .layout
        .symbols
        .iter()
        .filter(|s| s.kind == PowerKind::Supply)
        .map(|s| s.net)
        .collect();
    assert_eq!(supplies, [net("+VBATT_SAFE")], "{supplies:?}");
}

#[test]
fn the_clamp_hangs_from_the_load_end_with_a_ground_symbol_of_its_own() {
    let d = draw(guard());
    let top = pin_at(&d, "D2", "1");
    let row = pin_at(&d, "J2", "16").y;
    assert!(
        (top.y - row - 2.0 * STEP).abs() < 1e-6,
        "D2 is {} steps under the row",
        (top.y - row) / STEP
    );
    // Straight under the node it serves: the diode's far pin and the header's pin are on it.
    assert!(top.x > pin_at(&d, "D1", "2").x && top.x < pin_at(&d, "J1", "1").x);
}

#[test]
fn a_design_listed_the_other_way_round_is_drawn_the_same() {
    let a = draw(guard());
    let mut flipped = guard();
    flipped.parts.reverse();
    let b = draw(flipped);
    let at = |d: &Drawn, r: &str| pin_at(d, r, "1");
    for r in ["J1", "J2", "F1", "D1", "D2"] {
        assert_eq!(at(&a, r), at(&b, r), "{r}");
    }
}

/// With no clamp on either end the chain stays as it was found, and the sheet still reads from the
/// connector to the header.
#[test]
fn a_chain_with_nothing_on_either_end_is_still_one_row() {
    let mut design = guard();
    design.parts.retain(|p| p.refdes != "D2");
    for n in &mut design.nets {
        n.pins.retain(|p| p.refdes != "D2");
    }
    let d = draw(design);
    assert_eq!(d.layout.spines.len(), 1);
    let row = pin_at(&d, "J2", "16").y;
    assert_eq!(row_of(&d, "F1"), Some(row));
    assert_eq!(pin_at(&d, "J1", "1").y, row);
}

/// A rail that feeds several branches is no chain: two pull-ups on one rail make no link.
#[test]
fn a_rail_with_several_parts_on_it_is_no_link() {
    let design = common::design(
        "Branches",
        &[
            ("J1", "Header1x2", "Conn_01x02", None),
            ("J2", "Obd", "OBDII", Some("J")),
            ("F1", "LkFuse", "3A", Some("P")),
            ("F2", "LkFuse", "3A", Some("P")),
        ],
        &[
            ("+VBATT", &["J2.16", "F1.1", "F2.1"]),
            ("A", &["F1.2", "J1.1"]),
            ("B", &["F2.2", "J2.1"]),
            ("GND", &["J2.4", "J1.2"]),
        ],
    );
    let d = draw(design);
    assert!(d.layout.spines.is_empty(), "{:?}", d.layout.spines);
}

/// A resistor from a rail to a signal net (a pull-up to a pin of another main part) carries no
/// power from one rail to another: it is a part beside a pin, and no link.
#[test]
fn a_pull_up_from_a_rail_to_a_signal_net_is_no_link() {
    let design = common::design(
        "PullUp",
        &[
            ("J1", "Header1x2", "Conn_01x02", None),
            ("J2", "Obd", "OBDII", Some("J")),
            ("F1", "LkFuse", "10k", Some("P")),
        ],
        &[
            ("+VBATT", &["J2.16", "F1.1"]),
            ("SENSE", &["F1.2", "J1.1"]),
            ("GND", &["J2.4", "J1.2"]),
        ],
    );
    let d = draw(design);
    assert!(d.layout.spines.is_empty(), "{:?}", d.layout.spines);
}

/// A header whose supply pin is wired straight to a series part: the two are close, the net joins nothing else, and a
/// symbol would only read as a port to somewhere. The net is the wire, with its name on a short stem.
fn straight() -> Design {
    common::design(
        "Straight",
        &[
            ("J1", "Header1x2", "Conn_01x02", None),
            ("R1", "LkFuse", "330", Some("P")),
            ("D1", "LkDiode", "LED", Some("P")),
        ],
        &[
            ("+3V3", &["J1.1", "R1.1"]),
            ("SIG", &["R1.2", "D1.1"]),
            ("GND", &["D1.2", "J1.2"]),
        ],
    )
}

fn rails_3v3() -> Rails {
    let mut r = Rails::default();
    r.ground.insert("GND".into());
    r.positive.insert("+3V3".into());
    r
}

fn draw_with(design: Design, rails: &Rails) -> Drawn {
    let (dir, mapping, lib) = setup();
    let layout = layout_rails(&design, rails, &mapping, &lib).unwrap();
    if let Ok(path) = std::env::var("NETSMITH_LINK_SVG") {
        std::fs::write(path, netsmith_view::render_layout(&design, &layout)).unwrap();
    }
    Drawn {
        design,
        layout,
        _dir: dir,
    }
}

#[test]
fn a_supply_wired_straight_to_a_series_part_has_a_name_and_no_symbol() {
    let d = draw_with(straight(), &rails_3v3());
    let net = d.design.nets.iter().position(|n| n.name == "+3V3").unwrap();
    assert!(
        d.layout
            .symbols
            .iter()
            .all(|s| !(s.net == net && s.kind == PowerKind::Supply)),
        "a supply symbol on a net the wire joins already"
    );
    let labels: Vec<&str> = d
        .layout
        .labels
        .iter()
        .filter(|l| l.net == net)
        .map(|l| l.text.text.as_str())
        .collect();
    assert_eq!(labels, ["+3V3"], "the name of the rail, once");
    // The two pins are joined by wire, and the drawing is the netlist.
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_supply_with_a_part_to_ground_on_it_keeps_its_symbol() {
    let mut design = straight();
    design.parts.push(netsmith_core::Part {
        provides: Vec::new(),
        declared_kind: netsmith_testkit::std_kind("LkTvs").and_then(netsmith_core::PartKind::from_word),
        refdes: "D2".into(),
        kind: "LkTvs".into(),
        value: "TVS".into(),
        package: Some("P".into()),
        path: String::new(),
        pins: Vec::new(),
    });
    for n in &mut design.nets {
        let pin = |pin: &str| netsmith_core::PinRef {
            refdes: "D2".into(),
            pin: pin.into(),
        };
        if n.name == "+3V3" {
            n.pins.push(pin("1"));
        } else if n.name == "GND" {
            n.pins.push(pin("2"));
        }
    }
    let d = draw_with(design, &rails_3v3());
    let net = d.design.nets.iter().position(|n| n.name == "+3V3").unwrap();
    assert!(
        d.layout
            .symbols
            .iter()
            .any(|s| s.net == net && s.kind == PowerKind::Supply),
        "a rail with a shunt on it is drawn with its symbol"
    );
    assert!(d.layout.labels.iter().all(|l| l.net != net));
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn a_supply_with_one_pin_on_the_sheet_keeps_its_symbol() {
    let design = common::design(
        "OnePin",
        &[
            ("J1", "Header1x2", "Conn_01x02", None),
            ("D1", "LkDiode", "LED", Some("P")),
        ],
        &[
            ("+3V3", &["J1.1"]),
            ("SIG", &["D1.1"]),
            ("GND", &["D1.2", "J1.2"]),
        ],
    );
    let d = draw_with(design, &rails_3v3());
    let net = d.design.nets.iter().position(|n| n.name == "+3V3").unwrap();
    assert!(
        d.layout
            .symbols
            .iter()
            .any(|s| s.net == net && s.kind == PowerKind::Supply)
    );
}
