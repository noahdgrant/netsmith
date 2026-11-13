//! A symbol `Row` of `n` pins along its top edge and one `Col` of `n` pins down its left edge,
//! for tests that need a part with all its pins on one side.

#![allow(dead_code)]

use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_testkit::TempDir;

/// A symbol `Row` of `n` pins along its top edge, 2.54 mm apart, and one `Col` of `n` pins
/// down its left edge.
pub fn library(n: u32) -> String {
    let (mut top, mut left) = (String::new(), String::new());
    for i in 0..n {
        let x = 2.54 * f64::from(i);
        top += &format!(
            "(pin passive line (at {x} 5.08 270) (length 2.54) \
             (name \"T{k}\" (effects (font (size 1.27 1.27)))) \
             (number \"{k}\" (effects (font (size 1.27 1.27))))) ",
            k = i + 1
        );
        left += &format!(
            "(pin passive line (at -5.08 {y} 0) (length 2.54) \
             (name \"L{k}\" (effects (font (size 1.27 1.27)))) \
             (number \"{k}\" (effects (font (size 1.27 1.27))))) ",
            y = -2.54 * f64::from(i),
            k = i + 1
        );
    }
    let w = 2.54 * f64::from(n);
    let h = 2.54 * f64::from(n);
    format!(
        "(kicad_symbol_lib \
         (symbol \"Row\" (property \"Reference\" \"J\" (at 0 0 0)) \
         (symbol \"Row_0_1\" (rectangle (start -1.27 2.54) (end {w} -2.54) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Row_1_1\" {top})) \
         (symbol \"Col\" (property \"Reference\" \"U\" (at 0 0 0)) \
         (symbol \"Col_0_1\" (rectangle (start -2.54 2.54) (end 5.08 {bottom}) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Col_1_1\" {left})))",
        bottom = -h
    )
}

pub fn setup(n: u32) -> (TempDir, Mapping, SymbolLib) {
    let dir = TempDir::new("view-flags");
    let file = dir.path().join("flags.kicad_sym");
    std::fs::write(&file, library(n)).unwrap();
    let rows = "[[part]]\nkind = \"Row\"\npackage = \"P\"\nsymbol = \"flags:Row\"\nfootprint = \"flags:F\"\n\
                [[part]]\nkind = \"Col\"\npackage = \"P\"\nsymbol = \"flags:Col\"\nfootprint = \"flags:F\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(netsmith_testkit::sample_symbols()).with_library("flags", file);
    (dir, mapping, lib)
}
