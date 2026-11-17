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

/// The HDI board placed, as `netsmith routes` edits it.
fn placed() -> netsmith_pcb::Board {
    let built = netsmith_pipeline::build_with(
        "hdi.nsm",
        HDI,
        &netsmith_check::Loader::std(),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let libraries = Libraries::sample();
    netsmith_placer::board_from_design(&built[0].design, &libraries.mapping, &libraries.footprints)
        .unwrap()
}

#[test]
fn a_via_added_with_two_layers_is_the_processs_via_of_that_span() {
    use netsmith_fabricate::routes::{Edit, Refusal, edit};
    use netsmith_pcb::CopperLayer::{Back, Front, Inner};
    // The board, 3 by 2 mm round its one resistor, made 10 mm square for room for the vias.
    let mut board = placed();
    board.outline.max = board.outline.min + netsmith_pcb::Pt::new(10.0, 10.0);
    let via = |at: [f64; 2], layers| Edit::AddVia {
        net: "GND".into(),
        at,
        diameter: None,
        drill: None,
        layers,
        lock: false,
    };
    let mut routes = netsmith_pcb::Routes::default();
    // A blind via from the bottom up, named the lower first: the laser's via, the upper first.
    let said = edit(&board, &mut routes, &via([5.0, 5.0], Some([Back, Inner(4)]))).unwrap();
    assert_eq!(said, "added a blind via from inner4 to bottom to net GND");
    // The top and the bottom are the through via, written with no layers.
    edit(&board, &mut routes, &via([7.0, 5.0], Some([Front, Back]))).unwrap();
    let vias = &routes.boards[&board.name].nets["GND"].vias;
    assert_eq!(
        (vias[0].layers.as_slice(), vias[0].diameter, vias[0].drill),
        (["inner4".to_string(), "bottom".to_string()].as_slice(), 0.25, 0.1)
    );
    assert!(vias[1].layers.is_empty());
    assert_eq!(vias[1].diameter, board.rules.via_diameter);
    // A skip via is not made (ADR 0141).
    match edit(&board, &mut routes, &via([5.0, 7.0], Some([Front, Inner(2)]))) {
        Err(Refusal::Refused(why)) => assert!(
            why.contains("makes no via from `top` to `inner2`")
                && why.contains("top,inner1, inner4,bottom, inner1,inner4, top,bottom"),
            "{why}"
        ),
        other => panic!("{other:?}"),
    }
}
