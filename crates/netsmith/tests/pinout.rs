//! A part's pin table must agree with the symbol and the footprint it is exported with
//! (ADR 0016, decision 8): export stops, and the error names both sides. The libraries
//! here are invented, in a temporary project.

use netsmith::build_with;
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
