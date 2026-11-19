//! A part's pin table must agree with the symbol and the footprint it is exported with
//! (ADR 0016, decision 8): export stops, and the error names both sides. The libraries
//! here are invented, in a temporary project.

use netsmith_pipeline::build_with;
use netsmith_check::Loader;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

use crate::common;

const PADS: [&str; 6] = ["1", "2", "3", "4", "5", "6"];

/// Build the `Chip` board with the part's pin table `part`, the symbol's pins and the
/// footprint's pads; the error text if it does not.
fn build(part: &str, symbol: &[(&str, &str, &str)], pads: &[&str]) -> Result<(), String> {
    let dir = TempDir::new("pinout");
    let libraries = common::chip_libraries(
        dir.path(),
        &common::chip_symbol(symbol),
        &common::chip_footprint(pads),
    );
    build_with(
        "t.nsm",
        &common::chip_source(part),
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .map(|_| ())
}

fn with_pin(
    index: usize,
    pin: (&'static str, &'static str, &'static str),
) -> Vec<(&'static str, &'static str, &'static str)> {
    let mut pins = common::CHIP_PINS.to_vec();
    pins[index] = pin;
    pins
}

#[test]
fn a_pin_table_that_agrees_with_the_symbol_and_the_footprint_builds() {
    build(common::CHIP_PART, &common::CHIP_PINS, &PADS).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_pin_the_symbol_names_otherwise_stops_the_build_naming_both() {
    let err = build(
        common::CHIP_PART,
        &with_pin(2, ("input", "ENABLE", "3")),
        &PADS,
    )
    .unwrap_err();
    assert!(
        err.contains(
            "error: B: U1 (Chip): pad 3 is the input pin `en` (`EN`) in the \
             part, but pin 3 of the symbol `tiny:Chip` is named `ENABLE`"
        ),
        "{err}"
    );
}

#[test]
fn a_pin_of_another_kind_than_the_symbols_stops_the_build_naming_both() {
    let err = build(
        common::CHIP_PART,
        &with_pin(2, ("output", "EN", "3")),
        &PADS,
    )
    .unwrap_err();
    assert!(
        err.contains(
            "pad 3 is the input pin `en` (`EN`) in the part, but pin 3 of the symbol \
             `tiny:Chip` has the electrical type `output`, which does not suit a input pin"
        ),
        "{err}"
    );
}

#[test]
fn a_pad_the_footprint_lacks_stops_the_build_naming_both() {
    let err = build(common::CHIP_PART, &common::CHIP_PINS, &PADS[..5]).unwrap_err();
    assert!(
        err.contains(
            "the pinout has pad 6 (the power pin `gnd` (`GND`)) but the footprint \
             `tiny:SOT-6` has pads 1, 2, 3, 4, 5 only"
        ),
        "{err}"
    );
}

#[test]
fn a_pad_the_part_lacks_stops_the_build_naming_both() {
    let mut pads = PADS.to_vec();
    pads.push("7");
    let err = build(common::CHIP_PART, &common::CHIP_PINS, &pads).unwrap_err();
    assert!(
        err.contains(
            "the footprint `tiny:SOT-6` has pad 7, which no pin of the part has; give it to a \
             pin in the pinout"
        ),
        "{err}"
    );
}

#[test]
fn every_disagreement_is_reported_not_only_the_first() {
    let err = build(
        common::CHIP_PART,
        &with_pin(2, ("input", "ENABLE", "3")),
        &PADS[..5],
    )
    .unwrap_err();
    assert!(
        err.contains("named `ENABLE`") && err.contains("has pads 1, 2, 3, 4, 5 only"),
        "{err}"
    );
}

/// An invented 3.3 V regulator that provides `LinearRegulator`, in a package `SOT-6` of
/// three pads, and a board that draws up to 100 mA from it (issue 751).
const LDO3: &str = "\
import std::parts::regulator::LinearRegulator
import std::sources::Rail

/// An invented 3.3 V regulator.
part Ldo3 provides LinearRegulator {
    pins {
        .vin.pos  power \"IN\"  \"1\"
        .vin.neg  power \"GND\" \"2\"
        .vout.pos power \"OUT\" \"3\"
    }
    .vout.voltage &= 3.3V +/- 3%
    .dropout &= 300mV
    .max_current &= 500mA
    .max_power &= 1W
    catalog ldo3 { .mpn, .package }
}

board Repro {
    .input: Rail {
        .voltage &= 4.5V..5.5V
        .pos.net &= \"VIN\"
    }
    .reg: Ldo3
    .input ~ .reg.vin
    .load: Rail {
        .current &= 0A..100mA
        .pos.net &= \"V3V3\"
    }
    .reg.vout ~ .load
}
";

const LDO3_CSV: &str = "\
mpn,name,package,library,source
LDO3-33,LDO3 3.3V,SOT-6,Extended,https://example.invalid/ldo3
";

/// Build `LDO3` with `Ldo3` drawn with a library symbol whose output pin (pad 3) has the
/// electrical type `output`, as many library regulator symbols type it.
fn build_ldo3(source: &str) -> Result<(), String> {
    let dir = TempDir::new("pinout-ldo3");
    let mut libraries = common::chip_libraries(
        dir.path(),
        &common::chip_symbol(&[
            ("power_in", "IN", "1"),
            ("power_in", "GND", "2"),
            ("output", "OUT", "3"),
        ]),
        &common::chip_footprint(&["1", "2", "3"]),
    );
    // The project's mapping names `Chip`; map `Ldo3` to the same symbol and footprint.
    let toml = std::fs::read_to_string(dir.path().join("kicad.toml"))
        .unwrap()
        .replace("kind = \"Chip\"", "kind = \"Ldo3\"");
    std::fs::write(dir.path().join("kicad.toml"), toml).unwrap();
    libraries.mapping =
        netsmith_kicad::Mapping::for_project(Some(dir.path())).unwrap_or_else(|e| panic!("{e}"));
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file("ldo3", "catalogs/ldo3.csv", LDO3_CSV)
        .unwrap();
    build_with("t.nsm", source, &Loader::std(), &catalogs, &libraries).map(|_| ())
}

#[test]
fn a_regulators_output_drawn_with_a_symbol_typed_output_builds() {
    // The pin table says `power` (the generic's kind) and the symbol `output`: the solved
    // design says the pin gives power, so the symbol is used as it is.
    build_ldo3(LDO3).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn an_output_symbol_pin_on_a_power_pin_that_gives_no_power_stops_the_build() {
    // The same symbol pin on the input, which takes power, does not suit it.
    let swapped = LDO3
        .replace("\"IN\"  \"1\"", "\"IN\"  \"3\"")
        .replace("\"OUT\" \"3\"", "\"OUT\" \"1\"");
    let err = build_ldo3(&swapped).unwrap_err();
    assert!(
        err.contains(
            "pad 3 is the power pin `vin.pos` (`IN`) in the part, but pin 3 of the symbol \
             `tiny:Chip` has the electrical type `output`, which does not suit a power pin; a \
             power pin suits `output` when the solved design says it gives power"
        ),
        "{err}"
    );
}

/// Build the `Chip` board with a `table` on its row of `kicad.toml` (issue #1281); the error
/// text if it does not build.
fn build_with_table(table: &str) -> Result<(), String> {
    let dir = TempDir::new("pinout-table");
    common::chip_libraries(
        dir.path(),
        &common::chip_symbol(&common::CHIP_PINS),
        &common::chip_footprint(&PADS),
    );
    let file = dir.path().join("kicad.toml");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, format!("{text}table = \"{table}\"\n")).unwrap();
    let libraries = netsmith_pipeline::Libraries::with_mapping(
        netsmith_kicad::Mapping::for_project(Some(dir.path())).unwrap_or_else(|e| panic!("{e}")),
        netsmith_kicad::SymbolLib::at(netsmith_testkit::sample_symbols()),
        netsmith_pcb::FootprintLibrary::at(netsmith_testkit::sample_footprints()),
    );
    build_with(
        "t.nsm",
        &common::chip_source(common::CHIP_PART),
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .map(|_| ())
}

#[test]
fn a_footprint_that_agrees_with_the_table_of_its_row_builds() {
    // The chip's footprint: two columns of three pads, 1 mm apart.
    build_with_table("pads=6, pitch=1mm").unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_footprint_that_disagrees_with_the_table_of_its_row_stops_the_build_naming_both() {
    let err = build_with_table("pads=8, pitch=0.95mm, body=1.6x2.9mm").unwrap_err();
    assert!(
        err.contains(
            "error: B: U1 (Chip): the footprint `tiny:SOT-6` has 6 pads, but the table \
             `pads=8, pitch=0.95mm, body=1.6x2.9mm` of kicad.toml line 11 says 8"
        ),
        "{err}"
    );
    assert!(err.contains("has its pads 1mm apart"), "{err}");
    // The footprint draws nothing on `F.Fab`.
    assert!(err.contains("draws no body on its fabrication layer"), "{err}");
}
