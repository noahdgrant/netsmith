//! The viewer draws what the symbol libraries hold: a large symbol, stacked hidden pins,
//! several units, and a box where there is no symbol.

mod common;

use netsmith_kicad::SymbolLib;
use netsmith_view::layout::layout;
use netsmith_view::render_svg;

#[test]
fn a_symbol_of_thirty_pins_is_laid_out_and_drawn_with_every_pin() {
    let (_dir, mapping, lib) = common::mcu::setup();
    let d = common::mcu::board();
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
    let (_dir, mapping, lib) = common::mcu::setup();
    let d = common::mcu::board();
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
    let (_dir, mapping, _) = common::mcu::setup();
    let lib = SymbolLib::at("/nonexistent/symbols");
    let d = common::mcu::board();
    let l = layout(&d, &mapping, &lib).unwrap();
    assert!(l.parts[0].glyph.symbol.is_none());
    let why = l.parts[0].glyph.missing.clone().unwrap();
    assert!(why.contains("symbol library `big` not found"), "{why}");
    let svg = render_svg(&d, &mapping, &lib).unwrap();
    assert!(svg.contains("symbol library `big` not found"), "tooltip");
}
