//! The crate's seams with the files it reads: a KiCad symbol library committed in `fixtures/`
//! (and so the same on every machine, with no KiCad installed), and the standard mapping against
//! our own sample libraries.

use netsmith_kicad::{Mapping, PinType, SymbolLib, symbol_in, symbol_names_in};

fn mini() -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/mini.kicad_sym",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn a_symbol_library_file_lists_its_symbols_in_the_files_order() {
    assert_eq!(
        symbol_names_in(&mini(), "mini.kicad_sym").unwrap(),
        ["Mini", "Other"]
    );
}

#[test]
fn a_symbol_of_the_file_has_its_pins_with_their_numbers_types_and_places() {
    let text = mini();
    let two_pin = symbol_in(&text, "mini.kicad_sym", "Mini").unwrap();
    assert_eq!(two_pin.pin_numbers(), ["1", "2"]);
    assert_eq!(two_pin.pin_type("1"), Some(PinType::Passive));
    assert_eq!(
        two_pin.pin_geometry("1").map(|p| (p.x, p.y)),
        Some((0.0, 3.81))
    );
    assert_eq!(
        two_pin.pin_geometry("2").map(|p| (p.x, p.y)),
        Some((0.0, -3.81))
    );
    let other = symbol_in(&text, "mini.kicad_sym", "Other").unwrap();
    assert_eq!(other.pin_type("7"), Some(PinType::Input));
}

#[test]
fn a_symbol_the_file_does_not_have_is_an_error_that_names_the_ones_it_has() {
    let e = symbol_in(&mini(), "mini.kicad_sym", "Missing").unwrap_err();
    assert!(e.contains("mini.kicad_sym has no symbol `Missing`"), "{e}");
    assert!(e.contains("Mini, Other"), "{e}");
}

#[test]
fn the_standard_mapping_names_a_symbol_and_a_footprint_that_our_sample_libraries_have() {
    let mapped = Mapping::standard()
        .map("Resistor", Some("0402"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(mapped.footprint, "Resistor_SMD:R_0402_1005Metric");
    let lib_id = mapped.lib_id.expect("a resistor has a KiCad symbol");
    assert_eq!(lib_id, "Device:R");
    let symbols = SymbolLib::at(netsmith_testkit::sample_symbols());
    let symbol = symbols.symbol(&lib_id).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(symbol.pin_numbers(), ["1", "2"]);
}
