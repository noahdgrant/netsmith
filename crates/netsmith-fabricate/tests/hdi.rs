//! `fab` of a board on an HDI process (issue #1015), with no KiCad: the package is made, the
//! design rules check passes it with the process's via rules, and the notes say it is ordered
//! from the HDI service with the board's own build-up.

use netsmith_fabricate::fab::fab;
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const HDI: &str = include_str!("../../netsmith-tools/tests/fixtures/hdi_dense.nsm");

#[test]
fn an_hdi_board_becomes_a_package_whose_notes_say_how_to_order_it() {
    let dir = TempDir::unused("fab-test-hdi");
    let made = fab(
        "hdi.nsm",
        HDI,
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made[0].board, "Dense");
    let notes = std::fs::read_to_string(dir.join("Dense").join("fab-notes.txt")).unwrap();
    assert!(notes.contains("design rules check: passed"), "{notes}");
    assert!(
        notes.contains("HDI: order it from JLCPCB's HDI service as a 6-layer board of order 1 with the build-up `HDI-6L-1.2`"),
        "{notes}"
    );
    assert!(
        notes.contains("of the spans F.Cu-In1.Cu, In4.Cu-B.Cu, In1.Cu-In4.Cu"),
        "{notes}"
    );
    let pcb = std::fs::read_to_string(dir.join("Dense").join("Dense.kicad_pcb")).unwrap();
    assert!(pcb.contains("\"In4.Cu\""), "the board file has six copper layers");
    let pro = std::fs::read_to_string(dir.join("Dense").join("Dense.kicad_pro")).unwrap();
    assert!(pro.contains("\"min_microvia_drill\": 0.075"), "{pro}");
}
