//! `netsmith fab` and a part whose catalog row has no supplier number (no `lcsc`). It
//! refuses, all such parts in one message, before it places or routes anything, and the
//! message says what to write and where. The boards here are invented.

use netsmith::Libraries;
use netsmith::fab::fab;
use netsmith_kicad::testlib::TempDir;
use netsmith_solve::Catalogs;
use std::fs;

/// Two resistors of a value that only the project's own row has, and the row has a
/// manufacturer's number and a source but no `lcsc`.
const ROWS: &str = "lcsc,mpn,resistance,max_power,package,library,source\n\
    ,ZZR-1234,1.234kohm +/- 0.1%,100mW,0402,,https://example.invalid/zzr-1234\n";

const FIRST_AND_SECOND: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Invented {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .first: Resistor { .resistance &= 1.23kohm..1.24kohm }
    .second: Resistor { .resistance &= 1.23kohm..1.24kohm }
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .supply.pos ~> .first ~> .second ~> .supply.neg
}
"#;

const IN_A_MODULE: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Pair {
    .supply: Rail
    .a: Resistor { .resistance &= 1.23kohm..1.24kohm }
    .b: Resistor { .resistance &= 1.23kohm..1.24kohm }
    .supply.pos ~> .a ~> .b ~> .supply.neg
}

board Invented {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .pair: Pair
    .supply ~ .pair.supply
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}
"#;

fn catalogs() -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    c.add_file("resistor", "catalogs/resistor.csv", ROWS)
        .unwrap();
    c
}

fn run(source: &str, dir: &std::path::Path) -> Result<Vec<netsmith::fab::Fabricated>, String> {
    fab(
        "invented.nsm",
        source,
        dir,
        &catalogs(),
        &Libraries::standard(),
        None,
    )
}

#[test]
fn every_part_with_no_part_number_is_named_together_with_what_to_write() {
    let dir = TempDir::unused("fab-no-part-number");
    let err = run(FIRST_AND_SECOND, &dir).expect_err("no LCSC number must stop the package");
    // Both parts are in one message, each with its path, and what the row says.
    for want in [
        "R1",
        "R2",
        "`first`",
        "`second`",
        "ZZR-1234",
        "no LCSC part number",
    ] {
        assert!(err.contains(want), "{want}: {err}");
    }
    // What works today: the board says who solders each part, in the file that declares it.
    assert!(
        err.contains(".first.assembly &= \"hand\"") && err.contains(".second.assembly &= \"hand\""),
        "{err}"
    );
    assert!(err.contains("invented.nsm"), "{err}");
    assert!(!err.contains("--allow-missing-lcsc"), "{err}");
    assert!(!err.contains("Options::"), "{err}");
    // Nothing was placed, routed or written.
    assert!(!dir.join("Invented").exists(), "files were written");
}

#[test]
fn marking_the_parts_hand_soldered_gets_past_it_and_the_notes_say_so() {
    let dir = TempDir::unused("fab-no-part-number-hand");
    let source = FIRST_AND_SECOND
        .replace(
            ".first: Resistor { .resistance &= 1.23kohm..1.24kohm }",
            ".first: Resistor { .resistance &= 1.23kohm..1.24kohm\n .assembly &= \"hand\" }",
        )
        .replace(
            ".second: Resistor { .resistance &= 1.23kohm..1.24kohm }",
            ".second: Resistor { .resistance &= 1.23kohm..1.24kohm\n .assembly &= \"hand\" }",
        );
    let made = run(&source, &dir).unwrap_or_else(|e| panic!("{e}"));
    let notes = fs::read_to_string(dir.join("Invented").join("fab-notes.txt")).unwrap();
    assert!(notes.contains("hand-solder: J1, R1, R2"), "{notes}");
    assert!(
        notes.contains("the board house does not assemble"),
        "{notes}"
    );
    let bom = fs::read_to_string(dir.join("Invented").join("bom.csv")).unwrap();
    assert!(!bom.contains("R1") && !bom.contains("R2"), "{bom}");
    assert!(made[0].notes.contains("hand-solder"), "{}", made[0].notes);
}

#[test]
fn a_part_inside_a_module_is_named_by_path_with_the_ways_the_module_allows() {
    let dir = TempDir::unused("fab-no-part-number-module");
    let err = run(IN_A_MODULE, &dir).expect_err("no LCSC number must stop the package");
    for want in ["`pair.a`", "`pair.b`", "R1", "R2", "ZZR-1234"] {
        assert!(err.contains(want), "{want}: {err}");
    }
    // The board cannot reach `pair.a.assembly`; it can only set a value the module offers.
    assert!(
        !err.contains(".pair.a.assembly &="),
        "the board cannot write that: {err}"
    );
    assert!(
        err.contains(".pair.assembly_mode &= \"hand\"") && err.contains("Pair"),
        "{err}"
    );
    assert!(err.contains("`.a.assembly &= \"hand\"`"), "{err}");
    assert!(!err.contains("--allow-missing-lcsc"), "{err}");
}

#[test]
fn a_part_that_is_not_assembled_by_the_board_house_needs_no_number() {
    // The row's own `assembly` column is the third way: the pick carries it.
    let dir = TempDir::unused("fab-no-part-number-row");
    let mut c = Catalogs::jlcpcb();
    c.add_file(
        "resistor",
        "catalogs/resistor.csv",
        "lcsc,mpn,resistance,max_power,package,library,assembly,source\n\
         ,ZZR-1234,1.234kohm +/- 0.1%,100mW,0402,,hand,https://example.invalid/zzr-1234\n",
    )
    .unwrap();
    let made = fab(
        "invented.nsm",
        FIRST_AND_SECOND,
        &dir,
        &c,
        &Libraries::standard(),
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(made[0].notes.contains("hand-solder: J1, R1, R2"));
}

#[test]
fn the_module_way_the_message_names_gets_past_it() {
    let dir = TempDir::unused("fab-no-part-number-offer");
    let source = IN_A_MODULE
        .replace(
            "    .supply: Rail\n    .a:",
            "    .supply: Rail\n    .assembly_mode: String\n    .a:",
        )
        .replace(
            "    .supply.pos ~> .a ~> .b ~> .supply.neg\n}",
            "    .a.assembly &= .assembly_mode\n    .b.assembly &= .assembly_mode\n    .supply.pos ~> .a ~> .b ~> .supply.neg\n}",
        )
        .replace(
            "    .pair: Pair\n",
            "    .pair: Pair { .assembly_mode &= \"hand\" }\n",
        );
    assert_ne!(source, IN_A_MODULE);
    let made = run(&source, &dir).unwrap_or_else(|e| panic!("{e}"));
    assert!(made[0].notes.contains("hand-solder: J1, R1, R2"));
}
