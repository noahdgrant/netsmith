//! The SVG.

mod common;

use netsmith_kicad::{Mapping, SymbolLib, testlib};
use netsmith_view::render_svg;

fn svg(d: &netsmith_core::Design) -> String {
    render_svg(
        d,
        &Mapping::standard(),
        &SymbolLib::at(testlib::sample_symbols()),
    )
    .unwrap()
}

fn count(doc: &str, needle: &str) -> usize {
    doc.matches(needle).count()
}

#[test]
fn the_same_design_gives_identical_bytes() {
    for d in common::all() {
        assert_eq!(svg(&d), svg(&d), "{}", d.name);
    }
}

#[test]
fn the_svg_is_well_formed() {
    for d in common::all() {
        let doc = svg(&d);
        assert!(doc.starts_with("<svg "), "{}", d.name);
        assert!(doc.trim_end().ends_with("</svg>"));
        common::balanced(&doc).unwrap_or_else(|e| panic!("{}: {e}", d.name));
    }
}

#[test]
fn each_example_has_its_parts_pins_and_nets() {
    for d in common::all() {
        let doc = svg(&d);
        assert_eq!(
            count(&doc, "<g class=\"part\""),
            d.parts.len(),
            "{}",
            d.name
        );
        for p in &d.parts {
            assert!(doc.contains(&format!("data-ref=\"{}\"", p.refdes)));
        }
        let pins: usize = d.nets.iter().map(|n| n.pins.len()).sum();
        assert_eq!(count(&doc, "<g class=\"pin\""), pins, "{}", d.name);
        for n in &d.nets {
            assert!(
                doc.contains(&format!("<g class=\"wire\" data-net=\"{}\">", n.name)),
                "{}",
                n.name
            );
            for p in &n.pins {
                let tag = format!(
                    "<g class=\"pin\" data-ref=\"{}\" data-pin=\"{}\" data-net=\"{}\">",
                    p.refdes, p.pin, n.name
                );
                assert!(doc.contains(&tag), "{tag}");
            }
        }
        assert_eq!(count(&doc, "<style>"), 1);
    }
}

#[test]
fn symbols_are_drawn_from_the_library() {
    let doc = svg(&common::regulator());
    // The regulator's body is a filled rectangle with named pins; the capacitor is two
    // plates; the header has two small rectangles inside its body.
    assert!(doc.contains("class=\"pin-name\""));
    assert!(doc.contains(">VI<") && doc.contains(">VO<") && doc.contains(">GND<"));
    assert!(doc.contains("f-background"));
    let led = svg(&common::led());
    assert!(
        led.contains("<polyline class=\"s f-none\""),
        "the LED is polylines"
    );
}

#[test]
fn names_are_on_symbols_and_flags_and_a_generated_name_is_never_shown() {
    use netsmith_view::{Rails, render_svg_with};
    let d = common::led();
    let mut rails = Rails::default();
    rails.ground.insert("GND".into());
    rails.positive.insert("VCC".into());
    // A generated name is not a rail, whatever the checker's balances say.
    rails.positive.insert("net-led.anode".into());
    let doc = render_svg_with(
        &d,
        &rails,
        &netsmith_kicad::Mapping::standard(),
        &netsmith_kicad::SymbolLib::at(netsmith_kicad::testlib::sample_symbols()),
    )
    .unwrap();
    assert!(
        doc.contains(">VCC</text>"),
        "the supply symbol has its name"
    );
    assert!(!doc.contains("net-led.anode</text>"));
    assert!(!doc.contains("class=\"power supply\" data-net=\"net-led.anode\""));
    // With no rails told, a net is only wires.
    let plain = svg(&d);
    assert!(!plain.contains("class=\"net-label\""));
}

#[test]
fn text_from_the_design_is_escaped() {
    let d = common::design(
        "A<B>",
        &[("R1", "Resistor", "1k & \"co\"", Some("0402"))],
        &[("N&1", &["R1.1"]), ("N2", &["R1.2"])],
    );
    let doc = svg(&d);
    assert!(!doc.contains("1k & "));
    assert!(doc.contains("1k &amp; &quot;co&quot;"));
    assert!(doc.contains("data-net=\"N&amp;1\""));
    common::balanced(&doc).unwrap();
}

#[test]
fn an_unknown_part_degrades_to_a_labelled_box_instead_of_failing() {
    let d = common::design(
        "Odd",
        &[("X1", "Gadget", "42", None)],
        &[("A", &["X1.1"]), ("B", &["X1.2"])],
    );
    let doc = svg(&d);
    assert!(doc.contains("<g class=\"part missing\" data-ref=\"X1\""));
    assert!(doc.contains(">Gadget<"), "the kind is written in the box");
    assert!(doc.contains("No symbol:"), "the tooltip says why");
    assert_eq!(count(&doc, "<g class=\"pin\""), 2);
    common::balanced(&doc).unwrap();
}

#[test]
fn a_part_that_lacks_a_pad_the_design_uses_is_a_box_too() {
    let d = common::design(
        "Odd",
        &[("R1", "Resistor", "1k", Some("0402"))],
        &[("A", &["R1.1"]), ("B", &["R1.7"])],
    );
    let doc = svg(&d);
    assert!(doc.contains("part missing"));
    assert!(doc.contains("has no pin `7`"));
}

#[test]
fn an_inconsistent_design_is_an_error() {
    let d = common::design(
        "Bad",
        &[("R1", "Resistor", "1k", Some("0402"))],
        &[("A", &["R9.1"])],
    );
    let e = render_svg(
        &d,
        &Mapping::standard(),
        &SymbolLib::at(testlib::sample_symbols()),
    )
    .unwrap_err();
    assert!(e.to_string().contains("R9"), "{e}");
}

#[test]
fn the_view_box_matches_the_data_bounds() {
    let doc = svg(&common::led());
    let grab = |key: &str| {
        let at = doc.find(key).unwrap() + key.len();
        doc[at..].split('"').next().unwrap().to_string()
    };
    assert_eq!(grab("viewBox=\""), grab("data-bounds=\""));
}
