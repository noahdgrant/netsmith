//! A part's height growth (issue #1263): a part states `.height_growth`, how much its body may
//! grow in height over its life (a cell or a supercapacitor that swells), and the height check
//! adds it to the part's height, stated or from its catalog row, before it compares the height
//! with a limit; the finding says so. The placer keeps the grown part out from under a limit its
//! grown height is over. A part with no height stays unknown, whatever it grows by. Every board,
//! row and number here is invented: a 40 by 30 mm board with a lid of 3 mm over its top 18 mm.

use netsmith_check::Loader;
use netsmith_pcb::{Board, PlacementProblem, Pt};
use netsmith_pipeline::{Built, Libraries, build_with};
use netsmith_solve::Catalogs;

/// The board: a rail and three resistors between its nets, with `extra` in the board's block.
fn source(extra: &str) -> String {
    format!(
        "import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Widget {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .rail: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }}
    .r1: Resistor {{ .resistance &= 31.6kohm +/- 1% }}
    .r2: Resistor {{ .resistance &= 15kohm +/- 1% }}
    .r3: Resistor {{ .resistance &= 15kohm +/- 1% }}
    .rail.pos ~> .r1 ~> .rail.neg
    .rail.pos ~> .r2 ~> .rail.neg
    .rail.pos ~> .r3 ~> .rail.neg
{extra}}}
"
    )
}

/// The outline, the lid, and `extra`.
fn physical(extra: &str) -> String {
    format!(
        "import std::pcb::{{Outline, Keepout}}

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= 40mm
            .height &= 30mm
        }}
    }}
    .lid: Keepout {{
        .x &= 0mm
        .y &= 0mm
        .width &= 40mm
        .height &= 18mm
        .layers &= \"top\"
        .max_height &= 3mm
    }}
    .r2.height &= 1mm
    .r3.height &= 1mm
{extra}}}
"
    )
}

/// The project's row for `r1`'s value: 2.5 mm tall, by a document of the row's.
const ROWS: &str = "\
mpn,resistance,max_power,package,height,source
ACME-R316,31.6kohm +/- 1%,62.5mW,0402,2.5mm,https://example.invalid/acme.pdf height: page 2
";

fn catalogs() -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    netsmith_solve::migrate::add_to(&mut c, "resistor", "catalogs/resistor.csv", ROWS)
        .unwrap();
    c
}

fn build(source_extra: &str, physical_extra: &str) -> Result<Built, String> {
    let loader =
        Loader::std().with_physical("widget.nsm", "widget.physical.nsm", &physical(physical_extra));
    build_with(
        "widget.nsm",
        &source(source_extra),
        &loader,
        &catalogs(),
        &Libraries::sample(),
    )
    .map(|mut b| b.remove(0))
}

fn built(physical_extra: &str) -> Built {
    build("", physical_extra).unwrap_or_else(|e| panic!("{e}"))
}

fn placed(b: &Built) -> Board {
    let libraries = Libraries::sample();
    netsmith_placer::board_from_design_placed(
        &b.design,
        &libraries.mapping,
        &libraries.footprints,
        &netsmith_pcb::place_options(&b.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// The board of `b` with R1 put under the lid, as a person who moved it by hand would.
fn r1_under_the_lid(b: &Built) -> Board {
    let mut board = placed(b);
    let top = board.outline.min;
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R1")
        .unwrap()
        .at = Pt::new(top.x + 20.0, top.y + 6.0);
    board
}

fn too_tall(board: &Board) -> Vec<PlacementProblem> {
    board
        .check_placement()
        .into_iter()
        .filter(|p| matches!(p, PlacementProblem::TooTall { .. }))
        .collect()
}

#[test]
fn a_part_states_its_growth_and_the_board_has_it() {
    let b = built("    .r1.height_growth &= 0.8mm\n");
    let growths: Vec<(&str, f64)> = b
        .design
        .features
        .height_growths
        .iter()
        .map(|g| (g.path.as_str(), g.growth))
        .collect();
    assert_eq!(growths, [("r1", 0.8)]);
    let board = placed(&b);
    let r1 = board.part("R1").expect("R1");
    assert_eq!(r1.height, Some(2.5), "the catalog row's height, not grown");
    assert_eq!(r1.height_growth, 0.8);
    assert!((r1.grown_height().unwrap() - 3.3).abs() < 1e-9);
    assert_eq!(board.part("R2").expect("R2").height_growth, 0.0);
}

#[test]
fn a_part_that_fits_the_limit_but_not_when_grown_is_too_tall_and_the_finding_says_so() {
    // Without the growth the row's 2.5 mm fits the lid's 3 mm.
    assert_eq!(too_tall(&r1_under_the_lid(&built(""))), vec![]);
    let found = too_tall(&r1_under_the_lid(&built("    .r1.height_growth &= 0.8mm\n")));
    assert_eq!(found.len(), 1, "{found:?}");
    let PlacementProblem::TooTall {
        height,
        limit,
        growth,
        ..
    } = &found[0]
    else {
        unreachable!()
    };
    assert!((height - 3.3).abs() < 1e-9 && *limit == 3.0 && *growth == 0.8);
    let text = found[0].to_string();
    assert!(
        text.contains("`r1` (R1) stands 3.30 mm tall on the top face when grown (its 2.50 mm height with its 0.80 mm `.height_growth` added), and the keepout `lid` allows 3.00 mm there (0.30 mm too tall)"),
        "{text}"
    );
}

#[test]
fn the_growth_is_added_to_a_height_the_physical_file_states_over_the_row() {
    // The physical file's 1.5 mm wins over the row's 2.5 mm, and the growth is added to it.
    let b = built("    .r1.height &= 1.5mm\n    .r1.height_growth &= 1.2mm\n");
    assert_eq!(b.catalog_heights, vec![]);
    let board = r1_under_the_lid(&b);
    assert_eq!(too_tall(&board), vec![], "1.5 + 1.2 mm fits 3 mm");
    let b = built("    .r1.height &= 1.5mm\n    .r1.height_growth &= 1.6mm\n");
    let found = too_tall(&r1_under_the_lid(&b));
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0]
            .to_string()
            .contains("its 1.50 mm height with its 1.60 mm `.height_growth` added"),
        "{}",
        found[0]
    );
}

#[test]
fn the_placer_keeps_a_part_out_from_under_a_limit_its_grown_height_is_over() {
    let b = built("    .r1.height_growth &= 0.8mm\n");
    let board = placed(&b);
    assert_eq!(board.check_placement(), vec![], "a clean placement");
    let r1 = board.part("R1").expect("R1");
    let lid_bottom = board.outline.min.y + 18.0;
    assert!(
        r1.body().min.y >= lid_bottom - 1e-6,
        "R1's body is at y = {:.2} mm from the top, under the lid",
        r1.body().min.y - board.outline.min.y
    );
}

#[test]
fn a_growth_on_a_part_of_no_height_leaves_it_unchecked() {
    // r2 states a growth; take its height away, as for a part nobody stated one for: the board
    // still does not know how tall it is, and it is unchecked under the lid, not a pass.
    let mut board = placed(&built("    .r2.height_growth &= 0.5mm
"));
    let top = board.outline.min;
    let r2 = board.parts.iter_mut().find(|p| p.refdes == "R2").unwrap();
    r2.height = None;
    r2.at = Pt::new(top.x + 20.0, top.y + 6.0);
    assert_eq!(board.part("R2").unwrap().grown_height(), None);
    assert_eq!(too_tall(&board), vec![]);
    assert!(
        board.heights_unchecked().iter().any(|u| u.refdes == "R2"),
        "{:?}",
        board.heights_unchecked()
    );
}

#[test]
fn the_circuit_source_may_state_a_growth_too() {
    let b = build("    .r1.height_growth &= 0.8mm\n", "").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.design.features.height_growths.len(), 1);
}

#[test]
fn a_growth_that_is_not_one_value_of_nothing_or_more_is_refused_with_its_name() {
    let e = build("", "    .r1.height_growth &= -0.5mm\n").map(|_| ()).expect_err("a negative growth");
    assert!(
        e.contains("`r1.height_growth` is -0.5 mm, and a growth does not lower a body"),
        "{e}"
    );
    let e = build("", "    .r1.height_growth &= 0.2mm..0.5mm\n").map(|_| ()).expect_err("a range");
    assert!(e.contains("`r1.height_growth` is 200um..500um, a range"), "{e}");
}
