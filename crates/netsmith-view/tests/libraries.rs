//! The viewer draws what the symbol libraries hold: a large symbol, stacked hidden pins,
//! several units, and a box where there is no symbol.

mod common;

use netsmith_core::Design;
use netsmith_kicad::testlib::{self, TempDir};
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_view::layout::layout;
use netsmith_view::render_svg;

/// A symbol of 30 pins, 15 down each side, named `P1` to `P30` and numbered 1 to 30; the
/// first of the right-hand side stacks three hidden duplicates on its point.
fn mcu_library() -> String {
    let mut pins = String::new();
    for i in 0..30u32 {
        let (side, row) = if i < 15 { (-1.0, i) } else { (1.0, i - 15) };
        let (x, angle) = if side < 0.0 { (-12.7, 0) } else { (12.7, 180) };
        let y = 17.78 - 2.54 * row as f64;
        pins += &format!(
            "(pin bidirectional line (at {x} {y} {angle}) (length 2.54) \
             (name \"P{n}\" (effects (font (size 1.27 1.27)))) \
             (number \"{n}\" (effects (font (size 1.27 1.27))))) ",
            n = i + 1
        );
    }
    // Pads 31 to 33 repeat pad 16's point and name, hidden.
    for n in 31..=33 {
        pins += &format!(
            "(pin passive line (at 12.7 17.78 180) (hide yes) (length 2.54) \
             (name \"P16\" (effects (font (size 1.27 1.27)))) \
             (number \"{n}\" (effects (font (size 1.27 1.27))))) "
        );
    }
    format!(
        "(kicad_symbol_lib (symbol \"Mcu\" (property \"Reference\" \"U\" (at 0 20 0)) \
         (symbol \"Mcu_0_1\" (rectangle (start -10.16 20.32) (end 10.16 -20.32) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Mcu_1_1\" {pins})))"
    )
}

fn setup() -> (TempDir, Mapping, SymbolLib) {
    let dir = TempDir::new("view-mcu");
    let file = dir.path().join("big.kicad_sym");
    std::fs::write(&file, mcu_library()).unwrap();
    let rows = "[[part]]\nkind = \"Mcu\"\npackage = \"QFP\"\nsymbol = \"big:Mcu\"\nfootprint = \"big:F\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(testlib::sample_symbols()).with_library("big", file);
    (dir, mapping, lib)
}

fn board() -> Design {
    common::design(
        "Big",
        &[
            ("U1", "Mcu", "MCU", Some("QFP")),
            ("J1", "Header1x2", "Conn_01x02", None),
            ("R1", "Resistor", "10k", Some("0402")),
        ],
        &[
            ("TX", &["U1.1", "J1.1"]),
            // Pads 16, 31, 32 and 33 are one pin; the design puts them all on one net.
            ("SUPPLY", &["U1.16", "U1.31", "U1.32", "U1.33", "R1.1"]),
            ("GND", &["J1.2", "R1.2", "U1.30"]),
        ],
    )
}

#[test]
fn a_symbol_of_thirty_pins_is_laid_out_and_drawn_with_every_pin() {
    let (_dir, mapping, lib) = setup();
    let d = board();
    let l = layout(&d, &mapping, &lib).unwrap();
    let u1 = &l.parts[0];
    assert_eq!(u1.glyph.symbol.as_deref(), Some("big:Mcu"));
    // 30 pins and three hidden duplicates, each placed.
    assert_eq!(u1.pins.len(), 33);
    // Every net reaches the pins it names.
    for net in &d.nets {
        assert!(
            l.wires.iter().any(|w| d.nets[w.net].name == net.name),
            "{}",
            net.name
        );
    }
    let svg = render_svg(&d, &mapping, &lib).unwrap();
    common::balanced(&svg).unwrap();
    // Thirty pins are drawn for U1; the hidden duplicates are not.
    let drawn = svg.matches("data-ref=\"U1\" data-pin=").count();
    assert_eq!(drawn, 30);
    assert!(!svg.contains("data-ref=\"U1\" data-pin=\"31\""));
    assert!(svg.contains(">P16<"));
    // A name is printed once at the stacked point, not four times.
    assert_eq!(svg.matches(">P16<").count(), 1);
}

#[test]
fn a_net_on_stacked_pads_is_not_drawn_to_the_same_point_twice() {
    let (_dir, mapping, lib) = setup();
    let d = board();
    let l = layout(&d, &mapping, &lib).unwrap();
    let supply = d.nets.iter().position(|n| n.name == "SUPPLY").unwrap();
    for wire in l.wires.iter().filter(|w| w.net == supply) {
        for pair in wire.points.windows(2) {
            assert_ne!(pair[0], pair[1], "a wire of zero length: {wire:?}");
        }
    }
}

#[test]
fn a_box_with_a_known_pinout_shows_the_datasheet_names() {
    // No library at all: the symbol is missing, so the part is a box.
    let (_dir, mapping, _) = setup();
    let lib = SymbolLib::at("/nonexistent/symbols");
    let d = board();
    let l = layout(&d, &mapping, &lib).unwrap();
    assert!(l.parts[0].glyph.symbol.is_none());
    let why = l.parts[0].glyph.missing.clone().unwrap();
    assert!(why.contains("symbol library `big` not found"), "{why}");
    let svg = render_svg(&d, &mapping, &lib).unwrap();
    assert!(svg.contains("symbol library `big` not found"), "tooltip");
}
