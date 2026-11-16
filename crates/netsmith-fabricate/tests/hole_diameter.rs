//! A mounting hole or a plated hole states its drill as `.diameter` (issue #89): the footprint
//! is generated for that drill, the same as the stock ones at 2.2 mm and 3.2 mm. Every board
//! here is invented: a 40 by 30 mm board with holes and one resistor.

use netsmith_check::Loader;
use netsmith_kicad::{Mapping, Search, SymbolLib};
use netsmith_pcb::{Board, FootprintLibrary, PadKind, place_options};
use netsmith_pipeline::{Libraries, build_with};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

fn source(plated: &str) -> String {
    SOURCE.replace("{plated}", plated)
}

const SOURCE: &str = "import std::parts::passive::Resistor
import std::parts::mechanical::PlatedHole
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .rail: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .r1: Resistor { .resistance &= 15kohm +/- 1% }
    .rail.pos ~> .r1 ~> .rail.neg
{plated}}
";

/// The physical file: an outline, and the holes `holes` states.
fn physical(holes: &str) -> String {
    format!(
        "import std::parts::mechanical::{{MountingHole, PlatedHole}}
import std::pcb::Outline

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= 40mm
            .height &= 30mm
        }}
{holes}    }}
}}
"
    )
}

/// The libraries: the standard mapping and generated footprints, so that no KiCad is read.
fn libraries() -> Libraries {
    let nowhere = TempDir::new("hole-diameter-no-kicad");
    let footprints = FootprintLibrary::installed_by(&Search::under(nowhere.path()));
    Libraries::with_mapping(
        Mapping::standard(),
        SymbolLib::at(netsmith_testkit::sample_symbols()),
        footprints,
    )
}

fn built(holes: &str) -> Result<Board, String> {
    built_with(holes, "")
}

fn built_with(holes: &str, plated: &str) -> Result<Board, String> {
    let libraries = libraries();
    let loader =
        Loader::std().with_physical("widget.nsm", "widget.physical.nsm", &physical(holes));
    let built = build_with(
        "widget.nsm",
        &source(plated),
        &loader,
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .map_err(|e| e.to_string())?;
    let options = place_options(&built[0].design, None);
    netsmith_placer::board_from_design_placed(
        &built[0].design,
        &libraries.mapping,
        &libraries.footprints,
        &options,
    )
    .map_err(|e| e.to_string())
}

/// The drill of the one pad of `refdes`, in mm, and the pad's kind.
fn drill(board: &Board, refdes: &str) -> (f64, PadKind) {
    let part = board.part(refdes).unwrap_or_else(|| panic!("{refdes}"));
    let pad = &part.footprint.pads[0];
    (pad.drill.expect("a drill").width, pad.kind)
}

#[test]
fn a_mounting_hole_of_any_diameter_is_a_drill_of_that_size_with_no_copper() {
    let board = built(
        "        .a: MountingHole { .diameter &= 2.375mm }
        .b: MountingHole { .diameter &= 4.5mm }
        .c: MountingHole { .package &= \"M3\" }
        .d: MountingHole { .diameter &= 3.2mm }
",
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(drill(&board, "H1"), (2.375, PadKind::NpThruHole));
    assert_eq!(drill(&board, "H2"), (4.5, PadKind::NpThruHole));
    // The stock M3 is the same drawing as a stated 3.2 mm.
    assert_eq!(drill(&board, "H3"), (3.2, PadKind::NpThruHole));
    let (stock, stated) = (
        &board.part("H3").unwrap().footprint,
        &board.part("H4").unwrap().footprint,
    );
    assert_eq!(stock.pads.len(), stated.pads.len());
    assert_eq!(stock.courtyard, stated.courtyard);
}

#[test]
fn a_plated_hole_of_any_diameter_has_a_pad_twice_its_drill() {
    let board = built_with("", "    .a: PlatedHole { .diameter &= 4.5mm }\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let (size, kind) = drill(&board, "H1");
    assert_eq!(size, 4.5);
    assert_eq!(kind, PadKind::ThruHole);
    let pad = &board.part("H1").unwrap().footprint.pads[0];
    assert_eq!((pad.width, pad.height), (9.0, 9.0));
}

#[test]
fn a_hole_that_states_a_package_and_a_diameter_is_an_error() {
    let e = built("        .a: MountingHole {\n .package &= \"M3\"\n .diameter &= 3mm\n }\n").unwrap_err();
    assert!(
        e.contains("states `.package` \"M3\" and `.diameter` 3 mm"),
        "{e}"
    );
}

#[test]
fn a_diameter_that_is_a_range_or_zero_is_an_error() {
    let e = built("        .a: MountingHole { .diameter &= 2mm..3mm }\n").unwrap_err();
    assert!(e.contains("`a.diameter`") && e.contains("range"), "{e}");
    let e = built("        .a: MountingHole { .diameter &= 0mm }\n").unwrap_err();
    assert!(e.contains("`a.diameter` is 0 mm"), "{e}");
}

#[test]
fn a_diameter_the_generator_cannot_draw_names_the_range() {
    let e = built("        .a: MountingHole { .diameter &= 30mm }\n").unwrap_err();
    assert!(e.contains("drill=30mm") && e.contains("out of range"), "{e}");
}
