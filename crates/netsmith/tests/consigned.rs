//! A consigned part (ADR 0083): the owner buys it and ships it to the board
//! house, which places and solders it. It is in `cpl.csv` and `bom.csv`, needs no `lcsc`, is
//! printed as `consigned` by `build`, and neither the stock note (#202) nor the unresolved
//! generic warning (#208) is made for it. The boards and the parts are invented.

use netsmith::fab::fab;
use netsmith::{Libraries, build, outcome_with};
use netsmith_check::Loader;
use netsmith_kicad::testlib::TempDir;
use netsmith_solve::Catalogs;
use std::fs;

/// A part that the project's own row has with no `lcsc` and a stock of 0, as a processor that
/// the supplier does not stock and the owner buys elsewhere.
const ROWS: &str = "lcsc,mpn,resistance,max_power,package,library,stock,source\n\
    ,ZZR-1234,1.234kohm +/- 0.1%,100mW,0402,,0,https://example.invalid/zzr-1234\n";

/// A row with a number and a stock of 0, for the stock note.
const ROWS_WITH_NUMBER: &str = "lcsc,mpn,resistance,max_power,package,library,stock,source\n\
    C900001,ZZR-1234,1.234kohm +/- 0.1%,100mW,0402,Extended,0,https://example.invalid/zzr-1234\n";

fn board(mode: &str) -> String {
    format!(
        r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Pair {{
    .supply: Rail
    .assembly_mode: AssemblyMode
    .a: Resistor {{ .resistance &= 1.23kohm..1.24kohm }}
    .b: Resistor {{ .resistance &= 1.23kohm..1.24kohm }}
    .a.assembly &= .assembly_mode
    .supply.pos ~> .a ~> .b ~> .supply.neg
}}

board Invented {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .supply: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }}
    .pair: Pair {{
{mode}
    }}
    .supply ~ .pair.supply
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}}
"#
    )
}

const CONSIGNED: &str = "        .assembly_mode &= AssemblyMode::Consigned";

fn catalogs(rows: &str) -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    c.add_file("resistor", "catalogs/resistor.csv", rows)
        .unwrap();
    c
}

// ---- the statement ----

#[test]
fn a_part_the_board_marks_consigned_is_assembled_by_the_board_house_with_no_number_needed() {
    // `.b` is a part of the module that the board cannot reach: give it to the hand by the
    // module's own value, and `.a` is consigned. Here both are stated in one definition.
    let src = board("        .assembly_mode &= AssemblyMode::Consigned").replace(
        ".a.assembly &= .assembly_mode\n",
        ".a.assembly &= .assembly_mode\n    .b.assembly &= AssemblyMode::Hand\n",
    );
    let outcome = outcome_with("invented.nsm", &src, &Loader::std(), &catalogs(ROWS), None)
        .unwrap_or_else(|e| panic!("{}", netsmith::render_problems(&e)));
    let b = &outcome.boards[0];
    let consigned: Vec<_> = b
        .sourcing
        .iter()
        .filter(|s| s.assembly == netsmith_design::Assembly::Consigned)
        .collect();
    assert_eq!(consigned.len(), 1, "{:#?}", b.sourcing);
    assert!(consigned[0].assembled() && !consigned[0].needs_part_number());
    assert_eq!(consigned[0].mpn.as_deref(), Some("ZZR-1234"));
    assert_eq!(consigned[0].lcsc, None);
}

// ---- the build ----

#[test]
fn the_report_prints_a_consigned_part_as_consigned() {
    // The report names each part's footprint, which needs KiCad's libraries (the `kicad` job).
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    let src = board("        .assembly_mode &= AssemblyMode::Consigned").replace(
        ".a.assembly &= .assembly_mode\n",
        ".a.assembly &= .assembly_mode\n    .b.assembly &= AssemblyMode::Hand\n",
    );
    let boards = build("invented.nsm", &src, &catalogs(ROWS)).unwrap_or_else(|e| panic!("{e}"));
    let line = boards[0]
        .report
        .lines()
        .find(|l| l.trim_start().starts_with("R1"))
        .unwrap_or_else(|| panic!("{}", boards[0].report));
    assert!(line.trim_end().ends_with("consigned"), "{line}");
    assert!(boards[0].report.contains("hand-soldered"));
}

// ---- the notes of #202 and #208 ----

fn stock_notes(b: &netsmith::Built) -> Vec<&String> {
    b.notes.iter().filter(|n| n.contains("stock")).collect()
}

#[test]
fn a_consigned_part_gets_no_stock_note_though_its_row_states_a_stock_of_zero() {
    let boards = |mode: &str| {
        outcome_with(
            "invented.nsm",
            &board(mode),
            &Loader::std(),
            &catalogs(ROWS_WITH_NUMBER),
            None,
        )
        .unwrap_or_else(|e| panic!("{}", netsmith::render_problems(&e)))
        .boards
    };
    // The control: both resistors are assembled from stock, so each gets the note.
    let from_stock = boards("");
    assert_eq!(
        stock_notes(&from_stock[0]).len(),
        2,
        "{:#?}",
        from_stock[0].notes
    );
    // `.a` (R1) is consigned: only `.b` (R2) keeps its note.
    let consigned = boards(CONSIGNED);
    let notes = stock_notes(&consigned[0]);
    assert_eq!(notes.len(), 1, "{:#?}", consigned[0].notes);
    assert!(notes[0].starts_with("R2 (`pair.b`)"), "{}", notes[0]);
}

const GENERIC: &str = r#"
import std::parts::passive::Capacitor
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
    .c: Capacitor {
        .capacitance &= 130nF +/- 10%
        .max_voltage >= .bias * 1.5
    }
    .supply.pos ~ .c.a
    .supply.neg ~ .c.b
}
"#;

fn unresolved(src: &str) -> Vec<String> {
    // A project's catalogs, which is what offers generic parts (ADR 0047).
    let root = TempDir::new("consigned-generic");
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"invented\"\n",
    )
    .unwrap();
    let catalogs = netsmith::project::Project::load(root.path())
        .unwrap()
        .catalogs()
        .unwrap();
    outcome_with("invented.nsm", src, &Loader::std(), &catalogs, None)
        .unwrap_or_else(|e| panic!("{}", netsmith::render_problems(&e)))
        .boards
        .iter()
        .flat_map(|b| b.guideline_warnings())
        .filter(|w| w.contains("unresolved generic"))
        .collect()
}

#[test]
fn a_consigned_generic_is_not_an_unresolved_generic_since_the_owner_supplies_it() {
    // The control: no row can be ordered for 130 nF, so the part to assemble is a finding.
    assert_eq!(unresolved(GENERIC).len(), 1, "{:#?}", unresolved(GENERIC));
    let consigned = GENERIC.replace(
        "        .capacitance &= 130nF +/- 10%\n",
        "        .capacitance &= 130nF +/- 10%\n        .assembly &= AssemblyMode::Consigned\n",
    );
    assert_ne!(consigned, GENERIC);
    assert_eq!(unresolved(&consigned), Vec::<String>::new());
}

// ---- fab ----

fn run(source: &str, dir: &std::path::Path) -> Result<Vec<netsmith::fab::Fabricated>, String> {
    fab(
        "invented.nsm",
        source,
        dir,
        &catalogs(ROWS),
        &Libraries::standard(),
        None,
    )
}

#[test]
fn fab_places_a_consigned_part_lists_it_in_bom_and_cpl_and_asks_for_no_number() {
    if netsmith_kicad::testlib::skip_without_kicad_libraries() {
        return;
    }
    let dir = TempDir::unused("fab-consigned");
    // `.a` is consigned and has no `lcsc`; `.b` is by hand. Without `consigned` the same
    // board is refused for the number of `.a`.
    let hand_b = |mode: &str| {
        board(mode).replace(
            ".a.assembly &= .assembly_mode\n",
            ".a.assembly &= .assembly_mode\n    .b.assembly &= AssemblyMode::Hand\n",
        )
    };
    let err = run(&hand_b(""), &dir).expect_err("a part to assemble needs a number");
    assert!(
        err.contains("no LCSC part number") && err.contains("R1"),
        "{err}"
    );

    let dir = TempDir::unused("fab-consigned-ok");
    let made = run(
        &hand_b("        .assembly_mode &= AssemblyMode::Consigned"),
        &dir,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let out = dir.join("Invented");
    let bom = fs::read_to_string(out.join("bom.csv")).unwrap();
    assert!(
        bom.lines()
            .any(|l| l.starts_with("ZZR-1234,R1,") && l.ends_with(",")),
        "{bom}"
    );
    assert!(!bom.contains("R2"), "{bom}");
    let cpl = fs::read_to_string(out.join("cpl.csv")).unwrap();
    assert!(cpl.lines().any(|l| l.starts_with("R1,")), "{cpl}");
    assert!(!cpl.lines().any(|l| l.starts_with("R2,")), "{cpl}");
    let notes = fs::read_to_string(out.join("fab-notes.txt")).unwrap();
    assert!(
        notes.contains("consigned: R1 (ZZR-1234, 1 part)")
            && notes.contains("you ship these parts to JLCPCB"),
        "{notes}"
    );
    assert!(notes.contains("hand-solder: J1, R2"), "{notes}");
    assert!(!notes.contains("unassembled"), "{notes}");
    assert!(made[0].notes.contains("consigned: R1"), "{}", made[0].notes);
}
