//! Satellites on one rail share a wire and a symbol (`engine::bridge`): two pull-ups of a bus,
//! one a few steps above the other, are joined to one `+3V3` symbol and not drawn with one
//! each, as in a hand drawing. Nothing is crossed or run along to do it.

use crate::common;

use netsmith_core::Design;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_testkit::TempDir;
use netsmith_view::check::check;
use netsmith_view::layout::{Layout, PowerKind};
use netsmith_view::metrics::measure;
use netsmith_view::{Rails, layout_rails};

fn pin(kind: &str, x: f64, y: f64, angle: u32, name: &str, number: u32) -> String {
    format!(
        "(pin {kind} line (at {x} {y} {angle}) (length 2.54) \
         (name \"{name}\" (effects (font (size 1.27 1.27)))) \
         (number \"{number}\" (effects (font (size 1.27 1.27)))))"
    )
}

/// `Bus`: a chip with its two bus pins `SDA` and `SCL` on the right, 7.62 mm apart, and the
/// supply and the ground on the left.
fn library() -> String {
    let mut pins = String::new();
    pins += &pin("bidirectional", 12.7, 3.81, 180, "SDA", 1);
    pins += &pin("input", 12.7, -3.81, 180, "SCL", 2);
    pins += &pin("power_in", -12.7, 2.54, 0, "VDD", 3);
    pins += &pin("power_in", -12.7, -2.54, 0, "GND", 4);
    format!(
        "(kicad_symbol_lib (symbol \"Bus\" (property \"Reference\" \"U\" (at 0 8 0)) \
         (symbol \"Bus_0_1\" (rectangle (start -10.16 6.35) (end 10.16 -6.35) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Bus_1_1\" {pins})))"
    )
}

struct Drawn {
    design: Design,
    layout: Layout,
    _dir: TempDir,
}

fn draw() -> Drawn {
    let dir = TempDir::new("view-rails");
    let file = dir.path().join("rails.kicad_sym");
    std::fs::write(&file, library()).unwrap();
    let rows = "[[part]]\nkind = \"Bus\"\npackage = \"B\"\nsymbol = \"rails:Bus\"\nfootprint = \"rails:F\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(netsmith_testkit::sample_symbols()).with_library("rails", file);
    let design = common::design(
        "Pullups",
        &[
            ("U1", "Bus", "CHIP", Some("B")),
            ("R1", "Resistor", "4k7", Some("0402")),
            ("R2", "Resistor", "4k7", Some("0402")),
        ],
        &[
            ("+3V3", &["U1.3", "R1.1", "R2.1"]),
            ("GND", &["U1.4"]),
            ("SDA", &["U1.1", "R1.2"]),
            ("SCL", &["U1.2", "R2.2"]),
        ],
    );
    let mut rails = Rails::default();
    rails.positive.insert("+3V3".into());
    rails.ground.insert("GND".into());
    let layout = layout_rails(&design, &rails, &mapping, &lib).unwrap();
    Drawn {
        design,
        layout,
        _dir: dir,
    }
}

#[test]
fn two_pull_ups_on_one_rail_share_one_wire_and_one_symbol() {
    let d = draw();
    let plus = d.design.nets.iter().position(|n| n.name == "+3V3").unwrap();
    // The chip's own supply pin has a symbol; the two pull-ups share one between them (three
    // symbols when each had its own).
    let symbols: Vec<_> = d
        .layout
        .symbols
        .iter()
        .filter(|s| s.net == plus && s.kind == PowerKind::Supply)
        .collect();
    assert_eq!(symbols.len(), 2, "{symbols:?}");
    // Nothing is open: both pull-ups' supply pins are wired to the one symbol they share.
    let found = check(&d.design, &d.layout);
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(measure(&d.layout).total.crossings, 0);
}
