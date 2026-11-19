//! The package of a board with an `alternate` part (issue 1415): `bom.csv` and `cpl.csv` list the
//! part that is fitted, once for each designator, and `bom-alternates.csv` lists each alternate
//! with the designator it replaces, its part number and its LCSC number. `fab-notes.txt` points to
//! it. A panel has the same file with the cell in the designator.

use netsmith_check::Loader;
use netsmith_fabricate::fab::{Fabricated, fab_with};
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;
use std::fs;
use std::path::Path;

const BOARD: &str = r#"import std::mfg::jlcpcb::{TwoLayer, VCutPanel}
import std::parts::connector::Header1x2
import std::parts::passive::{Capacitor, Resistor}

board Filter {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .min_edge_clearance &= 0.4mm..
    }
    .j: Header1x2
    .r: Resistor { .resistance &= 10kohm +/- 5%; .package &= "0402" }
    ALTERNATE
    .j.p1 ~ .r.a
    .j.p2 ~ .r.b
}
"#;

const PANEL: &str = r#"
panel FilterPanel {
    .cell: Filter
    .fab: VCutPanel { .columns &= 3; .rows &= 2; .rail &= 5mm }
}
"#;

const ALTERNATE: &str = r#"alternate .r: Capacitor { .package &= "0402"; .capacitance &= 100nF +/- 10% }"#;

const PHYSICAL: &str = r#"import std::pcb::Outline

physical for Filter {
    case {
        .outline: Outline { .width &= 40mm; .height &= 30mm; .corner_radius &= 0mm }
    }
}
"#;

fn make(source: &str, dir: &Path) -> Vec<Fabricated> {
    fab_with(
        "main.nsm",
        source,
        &Loader::std().with_physical("main.nsm", "main.physical.nsm", PHYSICAL),
        dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn rows(dir: &Path, folder: &str, file: &str) -> Vec<String> {
    fs::read_to_string(dir.join(folder).join(file))
        .unwrap_or_else(|e| panic!("{folder}/{file}: {e}"))
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn bom_and_cpl_keep_the_fitted_part_and_the_alternates_have_their_own_file() {
    if netsmith_kicad::skip_without_kicad_libraries() {
        return;
    }
    let dir = TempDir::unused("alternates-fab");
    let made = make(&BOARD.replace("ALTERNATE", ALTERNATE), &dir);
    assert_eq!(made.len(), 1);

    let alternates = rows(&dir, "Filter", "bom-alternates.csv");
    assert_eq!(alternates[0], "Designator,Part Number,LCSC Part #");
    let [row] = &alternates[1..] else {
        panic!("one alternate: {alternates:?}")
    };
    let cells: Vec<&str> = row.split(',').collect();
    assert_eq!(cells[0], "R1", "the designator it replaces: {row}");
    assert!(cells[1].starts_with("CL05"), "its part number: {row}");
    assert!(
        cells[2].starts_with('C') && cells[2][1..].bytes().all(|b| b.is_ascii_digit()),
        "its LCSC number: {row}"
    );

    // The house's upload lists R1 once, as the resistor, and no capacitor.
    let bom = rows(&dir, "Filter", "bom.csv").join("\n");
    assert_eq!(bom.matches("R1").count(), 1, "{bom}");
    assert!(!bom.contains(cells[2]), "{bom}");
    let cpl = rows(&dir, "Filter", "cpl.csv");
    assert_eq!(cpl.iter().filter(|l| l.starts_with("R1,")).count(), 1, "{cpl:?}");

    let notes = fs::read_to_string(dir.join("Filter/fab-notes.txt")).unwrap();
    assert!(
        notes.contains("alternate (not fitted, DNP; not in bom.csv, listed in bom-alternates.csv): R1 may take `Capacitor`"),
        "{notes}"
    );
}

#[test]
fn a_board_with_no_alternate_has_no_file_and_a_later_fab_removes_an_old_one() {
    if netsmith_kicad::skip_without_kicad_libraries() {
        return;
    }
    let dir = TempDir::unused("alternates-none");
    make(&BOARD.replace("ALTERNATE", ALTERNATE), &dir);
    assert!(dir.join("Filter/bom-alternates.csv").exists());
    make(&BOARD.replace("ALTERNATE", ""), &dir);
    assert!(
        !dir.join("Filter/bom-alternates.csv").exists(),
        "the list of an earlier package does not stay"
    );
    let notes = fs::read_to_string(dir.join("Filter/fab-notes.txt")).unwrap();
    assert!(!notes.contains("alternate"), "{notes}");
}

#[test]
fn a_panel_lists_each_alternate_for_each_cell_with_the_cell_in_the_designator() {
    if netsmith_kicad::skip_without_kicad_libraries() {
        return;
    }
    let dir = TempDir::unused("alternates-panel");
    let source = format!("{}{PANEL}", BOARD.replace("ALTERNATE", ALTERNATE));
    let made = make(&source, &dir);
    assert_eq!(made.len(), 2, "the board and the panel");

    let panel = rows(&dir, "FilterPanel", "bom-alternates.csv");
    let designators: Vec<&str> = panel[1..]
        .iter()
        .map(|l| l.split(',').next().unwrap())
        .collect();
    assert_eq!(designators, ["R1_1", "R1_2", "R1_3", "R1_4", "R1_5", "R1_6"], "{panel:?}");
    // The board's own file is in `cell/`, with the board's designator.
    let own = rows(&dir, "FilterPanel/cell/Filter", "bom-alternates.csv");
    assert_eq!(own.len(), 2, "{own:?}");
    assert!(own[1].starts_with("R1,"), "{own:?}");
    let notes = fs::read_to_string(dir.join("FilterPanel/fab-notes.txt")).unwrap();
    assert!(notes.contains("bom-alternates.csv lists each alternate once for each cell"), "{notes}");
}
