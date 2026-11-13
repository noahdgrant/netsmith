//! What is written at a pin: the names of pins side by side on a top or bottom edge, nearer
//! than the names are wide, are written upward (as KiCad does) and never over each other;
//! a pin on a side edge, or alone, is written across.

use crate::common;

use netsmith_core::Design;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_testkit::TempDir;
use netsmith_view::geom::{Dir, Orient, Pt};
use netsmith_view::glyph::{Glyph, used_pads};

/// A symbol with `VDD` and `VDDIO` side by side at the bottom (pads 101 and 102, 2.54 apart),
/// one `EN` alone at the top (pad 1), and `A` and `B` on the left (pads 2 and 3).
fn library() -> String {
    let pin = |kind: &str, x: f64, y: f64, angle: u32, name: &str, number: &str| {
        format!(
            "(pin {kind} line (at {x} {y} {angle}) (length 2.54) \
             (name \"{name}\" (effects (font (size 1.27 1.27)))) \
             (number \"{number}\" (effects (font (size 1.27 1.27))))) "
        )
    };
    let pins = [
        pin("power_in", -1.27, -7.62, 90, "VDD", "101"),
        pin("power_in", 1.27, -7.62, 90, "VDDIO", "102"),
        pin("input", 0.0, 7.62, 270, "EN", "1"),
        pin("input", -10.16, 1.27, 0, "A", "2"),
        pin("input", -10.16, -1.27, 0, "B", "3"),
    ]
    .concat();
    format!(
        "(kicad_symbol_lib (symbol \"Ic\" (property \"Reference\" \"U\" (at 0 8 0)) \
         (symbol \"Ic_0_1\" (rectangle (start -7.62 5.08) (end 7.62 -5.08) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Ic_1_1\" {pins})))"
    )
}

fn glyph() -> (TempDir, Glyph, Design) {
    let dir = TempDir::new("view-pintexts");
    let file = dir.path().join("t.kicad_sym");
    std::fs::write(&file, library()).unwrap();
    let rows =
        "[[part]]\nkind = \"Ic\"\npackage = \"QFP\"\nsymbol = \"t:Ic\"\nfootprint = \"t:F\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(netsmith_testkit::sample_symbols()).with_library("t", file);
    let d = common::design(
        "Pins",
        &[("U1", "Ic", "IC", Some("QFP"))],
        &[
            ("A", &["U1.101", "U1.102"]),
            ("B", &["U1.1", "U1.2", "U1.3"]),
        ],
    );
    let g = Glyph::for_part(&d.parts[0], &used_pads(&d)[0], None, &mapping, &lib);
    assert_eq!(g.symbol.as_deref(), Some("t:Ic"));
    (dir, g, d)
}

fn texts_of(g: &Glyph, pad: &str) -> (netsmith_view::layout::Text, netsmith_view::layout::Text) {
    let pin = g.pin(pad).unwrap();
    let toward = Orient::IDENTITY.dir(pin.toward_body);
    let (number, name) = g.pin_texts(pin, Pt::new(pin.at.0, -pin.at.1), toward);
    (number.unwrap(), name.unwrap())
}

#[test]
fn names_of_pins_side_by_side_on_an_edge_are_written_upward() {
    // `VDD` and `VDDIO` are 2.54 mm apart and 2.1 and 3.1 mm wide.
    let (_dir, g, _) = glyph();
    for pad in ["101", "102"] {
        let (number, name) = texts_of(&g, pad);
        assert!(
            name.vertical,
            "the name of {pad} is written across: {name:?}"
        );
        // The numbers (three digits, 1.7 mm) fit across.
        assert!(!number.vertical, "the number of {pad}: {number:?}");
    }
}

#[test]
fn the_texts_of_a_crowded_edge_do_not_overlap() {
    let (_dir, g, _) = glyph();
    let boxes: Vec<_> = ["101", "102"]
        .iter()
        .flat_map(|pad| {
            let (n, m) = texts_of(&g, pad);
            [n.extent_at(g.pin_text), m.extent_at(g.pin_text)]
        })
        .collect();
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            let apart = a.max.x <= b.min.x
                || b.max.x <= a.min.x
                || a.max.y <= b.min.y
                || b.max.y <= a.min.y;
            assert!(apart, "{a:?} overlaps {b:?}");
        }
    }
}

#[test]
fn a_pin_alone_on_an_edge_and_pins_on_a_side_are_written_across() {
    let (_dir, g, _) = glyph();
    for pad in ["1", "2", "3"] {
        let (number, name) = texts_of(&g, pad);
        assert!(!name.vertical && !number.vertical, "pin {pad}");
    }
    // The toward directions: the bottom pins point up into the body.
    let pin = g.pin("101").unwrap();
    assert_eq!(Orient::IDENTITY.dir(pin.toward_body), Dir::Up);
}
