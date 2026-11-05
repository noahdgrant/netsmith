//! A symbol of thirty pins with hidden duplicates, and a small board that uses it.

#![allow(dead_code)]

use netsmith_core::Design;
use netsmith_kicad::testlib::{self, TempDir};
use netsmith_kicad::{Mapping, SymbolLib};

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

pub fn setup() -> (TempDir, Mapping, SymbolLib) {
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

pub fn board() -> Design {
    super::design(
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
