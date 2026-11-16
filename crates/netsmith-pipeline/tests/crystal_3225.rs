//! The four-pad 3225 crystal (issue #845): the solver offers YXC's YSX321SL rows, the part's
//! `.case` pin is the two case pads (2 and 4) of the footprint, and a build needs no KiCad
//! library for it: the footprint is generated and the symbol is drawn from the pin table.

use netsmith_check::Loader;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_pcb::FootprintLibrary;
use netsmith_pipeline::{Libraries, build_with};
use netsmith_solve::Catalogs;

/// Libraries that read nothing from a folder: whatever they answer is made by netsmith.
fn bare() -> Libraries {
    Libraries::with_mapping(
        Mapping::standard(),
        SymbolLib::at("/nonexistent/symbols").with_generators(),
        FootprintLibrary::at("/nonexistent/footprints").with_generators(),
    )
}

/// A 25 MHz crystal across a resistor on a 3.3 V supply, with `extra` in the crystal's body and
/// `case` after it (a statement or nothing).
fn board(extra: &str, case: &str) -> String {
    format!(
        r#"import std::parts::passive::{{Crystal, Resistor}}
import std::sources::Rail

board B {{
    .supply: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= "VCC"
    }}
    .xtal: Crystal {{
        .frequency &= 25MHz +/- 30ppm
        {extra}
    }}
    .r: Resistor {{}}
    .supply.pos ~ .xtal.a
    .xtal.b ~ .r.a
    .r.b ~ .supply.neg
    {case}
}}
"#
    )
}

fn build(src: &str) -> Result<netsmith_pipeline::Built, String> {
    build_with("t.nsm", src, &Loader::std(), &Catalogs::jlcpcb(), &bare())
        .map(|mut boards| boards.remove(0))
}

#[test]
fn a_25_mhz_crystal_is_the_basic_3225_row_with_its_case_pads_on_ground() {
    let b = build(&board("", ".xtal.case ~ .supply.neg")).unwrap_or_else(|e| panic!("{e}"));
    let y = b
        .sourcing
        .iter()
        .find(|s| s.lcsc.as_deref() == Some("C9006"))
        .unwrap_or_else(|| panic!("the Basic 3225 row is picked: {:#?}", b.sourcing));
    assert_eq!(y.mpn.as_deref(), Some("X322525MOB4SI"));
    assert!(
        b.schematic
            .contains("Crystal:Crystal_SMD_3225-4Pin_3.2x2.5mm"),
        "the footprint is the four-pad 3225"
    );
    // Pads 2 and 4 are on the ground net with the resistor's other end.
    let ground = b
        .report
        .lines()
        .find(|l| l.contains("Y1.2") && l.contains("Y1.4"))
        .unwrap_or_else(|| panic!("no net has both case pads: {}", b.report));
    assert!(ground.contains("R1."), "{ground}");
}

#[test]
fn the_case_pin_may_be_left_open() {
    let b = build(&board("", "")).unwrap_or_else(|e| panic!("{e}"));
    assert!(b.sourcing.iter().any(|s| s.lcsc.as_deref() == Some("C9006")));
}

#[test]
fn a_two_pad_package_has_no_case_pin() {
    let e = build(&board(".package &= \"5032\"", ".xtal.case ~ .supply.neg"))
        .err()
        .expect("a 5032 crystal has no case pin");
    assert!(e.contains("case"), "{e}");
}

#[test]
fn a_two_pad_package_still_builds_without_the_case_pin() {
    let b = build(&board(".package &= \"5032\"", "")).unwrap_or_else(|e| panic!("{e}"));
    assert!(b.schematic.contains("Crystal_SMD_5032-2Pin_5.0x3.2mm"));
}
