//! A project with libraries of its own, shared by the tests that build it.

#![allow(dead_code)]

use std::fs;
use std::path::Path;

use netsmith::Libraries;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_pcb::FootprintLibrary;

/// An invented dual op-amp-like part in a project library `acme`: two amplifiers and a
/// supply unit, with two pins left open.
pub const DUAL_BOARD: &str = "\
import std::parts::connector::Header1x2

part Dual {
    pins {
        .out1 output \"OUT\" \"1\"
        .inn1 input \"-\" \"2\" optional
        .inp1 input \"+\" \"3\" optional
        .vn power \"V-\" \"4\"
        .inp2 input \"+\" \"5\" optional
        .inn2 input \"-\" \"6\" optional
        .out2 output \"OUT\" \"7\"
        .vp power \"V+\" \"8\"
    }
}

board B {
    .u: Dual
    .j1: Header1x2
    .j2: Header1x2
    .u.out1 ~ .u.inn2
    .u.out2 ~ .j1.p1
    .u.vp ~ .j2.p1
    .u.vn ~ .j2.p2
    .j1.p2 ~ .u.vn
}
";

/// A part whose symbol has a ground pin shared by both of its units.
pub const SHARED_BOARD: &str = "\
import std::parts::connector::Header1x2

part Shared {
    pins {
        .a input \"A\" \"1\" optional
        .b output \"B\" \"2\"
        .c input \"C\" \"3\" optional
        .d output \"D\" \"4\"
        .gnd power \"GND\" \"9\"
    }
}

board B {
    .u: Shared
    .j: Header1x2
    .u.b ~ .u.c
    .u.d ~ .j.p1
    .u.gnd ~ .j.p2
}
";

/// The `kicad.toml` that maps `Dual` to `acme:Dual` and lists the library `acme`.
pub const KICAD_TOML: &str = r#"
[[library]]
name = "acme"
symbols = "libs/acme.kicad_sym"
footprints = "libs/acme.pretty"

[[part]]
kind = "Dual"
package = "DIP-8"
default = true
symbol = "acme:Dual"
footprint = "acme:DIP-8"

[[part]]
kind = "Shared"
package = "DIP-8"
default = true
symbol = "acme:Shared"
footprint = "acme:Shared-5"
"#;

const DIP8: &str = "(footprint \"DIP-8\" (layer \"F.Cu\") \
    (property \"Reference\" \"REF**\" (at 0 -6 0) (layer \"F.SilkS\") (effects (font (size 1 1)))) \
    (attr through_hole) \
    (pad \"1\" thru_hole rect (at -3.81 -3.81) (size 1.6 1.6) (drill 0.8) (layers \"*.Cu\" \"*.Mask\")) \
    (pad \"2\" thru_hole oval (at -3.81 -1.27) (size 1.6 1.6) (drill 0.8) (layers \"*.Cu\" \"*.Mask\")) \
    (pad \"3\" thru_hole oval (at -3.81 1.27) (size 1.6 1.6) (drill 0.8) (layers \"*.Cu\" \"*.Mask\")) \
    (pad \"4\" thru_hole oval (at -3.81 3.81) (size 1.6 1.6) (drill 0.8) (layers \"*.Cu\" \"*.Mask\")) \
    (pad \"5\" thru_hole oval (at 3.81 3.81) (size 1.6 1.6) (drill 0.8) (layers \"*.Cu\" \"*.Mask\")) \
    (pad \"6\" thru_hole oval (at 3.81 1.27) (size 1.6 1.6) (drill 0.8) (layers \"*.Cu\" \"*.Mask\")) \
    (pad \"7\" thru_hole oval (at 3.81 -1.27) (size 1.6 1.6) (drill 0.8) (layers \"*.Cu\" \"*.Mask\")) \
    (pad \"8\" thru_hole oval (at 3.81 -3.81) (size 1.6 1.6) (drill 0.8) (layers \"*.Cu\" \"*.Mask\")) \
    (fp_rect (start -5 -5) (end 5 5) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))";

/// The footprint of `Shared`: the pads its symbol has.
const SHARED5: &str = "(footprint \"Shared-5\" (layer \"F.Cu\") \
    (property \"Reference\" \"REF**\" (at 0 -4 0) (layer \"F.SilkS\") (effects (font (size 1 1)))) \
    (attr smd) \
    (pad \"1\" smd rect (at -2 -1.5) (size 1.6 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"2\" smd rect (at 2 -1.5) (size 1.6 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"3\" smd rect (at -2 0.5) (size 1.6 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"4\" smd rect (at 2 0.5) (size 1.6 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"9\" smd rect (at 0 2.5) (size 1.6 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (fp_rect (start -4 -3) (end 4 4) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))";

/// A project folder at `dir` with `kicad.toml` and its library `libs/acme.*`, and the
/// libraries that build from it: our sample KiCad libraries plus the project's.
pub fn project_libraries(dir: &Path) -> Libraries {
    fs::create_dir_all(dir.join("libs/acme.pretty")).unwrap();
    fs::write(dir.join("kicad.toml"), KICAD_TOML).unwrap();
    fs::write(dir.join("libs/acme.kicad_sym"), netsmith_testkit::acme()).unwrap();
    fs::write(dir.join("libs/acme.pretty/DIP-8.kicad_mod"), DIP8).unwrap();
    fs::write(dir.join("libs/acme.pretty/Shared-5.kicad_mod"), SHARED5).unwrap();
    Libraries::with_mapping(
        Mapping::for_project(Some(dir)).unwrap_or_else(|e| panic!("{e}")),
        SymbolLib::at(netsmith_testkit::sample_symbols()),
        FootprintLibrary::at(netsmith_testkit::sample_footprints()),
    )
}

/// An invented six-pin IC, `Chip`, in a library of its own, `tiny`: input, output and ground
/// pins, an enable, and two pins that must stay open.
pub const CHIP_PINS: [(&str, &str, &str); 6] = [
    ("power_in", "VIN", "1"),
    ("power_out", "VOUT", "2"),
    ("input", "EN", "3"),
    ("no_connect", "NC", "4"),
    ("no_connect", "NC", "5"),
    ("power_in", "GND", "6"),
];

/// The part `Chip` as a pin table, agreeing with [`CHIP_PINS`] and `chip_footprint(6)`.
pub const CHIP_PART: &str = "\
part Chip {
    pins {
        .vin power \"VIN\" \"1\"
        .vout power \"VOUT\" \"2\"
        .en input \"EN\" \"3\"
        .nc1 no_connect \"NC\" \"4\"
        .nc2 no_connect \"NC\" \"5\"
        .gnd power \"GND\" \"6\"
    }
}
";

/// A board using `Chip`: the supply on a header, the output on another.
pub const CHIP_BOARD: &str = "\
import std::parts::connector::Header1x2

board B {
    .u: Chip
    .in: Header1x2
    .out: Header1x2
    .u.vin ~ .in.p1
    .u.en ~ .in.p1
    .u.gnd ~ .in.p2
    .u.vout ~ .out.p1
}
";

/// A board using `Chip` on rails: the output rail has no other pin on its net than the
/// chip's.
pub const CHIP_RAIL_BOARD: &str = "\
import std::sources::Rail

board B {
    .u: Chip
    .vin: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .vout: Rail {
        .pos.net &= \"VOUT\"
        .neg.net &= \"GND\"
    }
    .u.vin ~ .vin.pos
    .u.en ~ .vin.pos
    .u.gnd ~ .vin.neg
    .u.vout ~ .vout.pos
}
";

/// The source of a board using `Chip`, with the part's pin table `part`.
pub fn chip_source(part: &str) -> String {
    format!("{part}\n{CHIP_BOARD}")
}

/// `Chip` and a board `board` that uses it.
pub fn chip_source_of(board: &str) -> String {
    format!("{CHIP_PART}\n{board}")
}

/// The symbol `tiny:Chip` with the given pins `(electrical type, name, number)`.
pub fn chip_symbol(pins: &[(&str, &str, &str)]) -> String {
    let font = "(effects (font (size 1.27 1.27)))";
    let prop = |name: &str, value: &str, y: f64| {
        format!("(property \"{name}\" \"{value}\" (at 0 {y} 0) {font})")
    };
    let pins: String = pins
        .iter()
        .enumerate()
        .map(|(i, (kind, name, number))| {
            let y = 5.08 - 2.54 * i as f64;
            format!(
                "(pin {kind} line (at -7.62 {y} 0) (length 2.54) (name \"{name}\" {font}) \
                 (number \"{number}\" {font})) "
            )
        })
        .collect();
    format!(
        "(kicad_symbol_lib (version 20241209) (generator \"netsmith-test\") \
         (symbol \"Chip\" (exclude_from_sim no) (in_bom yes) (on_board yes) {} {} {} {} {} \
         (symbol \"Chip_0_1\" (rectangle (start -5.08 7.62) (end 5.08 -7.62) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Chip_1_1\" {pins})))\n",
        prop("Reference", "U", 8.89),
        prop("Value", "Chip", -8.89),
        prop("Footprint", "", 0.0),
        prop("Datasheet", "~", 0.0),
        prop("Description", "", 0.0),
    )
}

/// The footprint `tiny:SOT-6` with pads numbered `numbers`.
pub fn chip_footprint(numbers: &[&str]) -> String {
    let pads: String = numbers
        .iter()
        .enumerate()
        .map(|(i, n)| {
            format!(
                "(pad \"{n}\" smd rect (at {} {}) (size 1 0.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) ",
                if i < 3 { -1.5 } else { 1.5 },
                (i % 3) as f64 * 1.0 - 1.0
            )
        })
        .collect();
    format!(
        "(footprint \"SOT-6\" (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 -3 0) (layer \"F.SilkS\") (effects (font (size 1 1)))) \
         (attr smd) {pads} \
         (fp_rect (start -2.5 -2) (end 2.5 2) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))"
    )
}

/// A project folder at `dir` that maps `Chip` to the given symbol and footprint files'
/// text, and the libraries that build from it.
pub fn chip_libraries(dir: &Path, symbol: &str, footprint: &str) -> Libraries {
    fs::create_dir_all(dir.join("libs/tiny.pretty")).unwrap();
    fs::write(
        dir.join("kicad.toml"),
        "[[library]]\nname = \"tiny\"\nsymbols = \"libs/tiny.kicad_sym\"\n\
         footprints = \"libs/tiny.pretty\"\n\n\
         [[part]]\nkind = \"Chip\"\npackage = \"SOT-6\"\ndefault = true\n\
         symbol = \"tiny:Chip\"\nfootprint = \"tiny:SOT-6\"\n",
    )
    .unwrap();
    fs::write(dir.join("libs/tiny.kicad_sym"), symbol).unwrap();
    fs::write(dir.join("libs/tiny.pretty/SOT-6.kicad_mod"), footprint).unwrap();
    Libraries::with_mapping(
        Mapping::for_project(Some(dir)).unwrap_or_else(|e| panic!("{e}")),
        SymbolLib::at(netsmith_testkit::sample_symbols()),
        FootprintLibrary::at(netsmith_testkit::sample_footprints()),
    )
}

pub mod endpoints;
pub mod port;
