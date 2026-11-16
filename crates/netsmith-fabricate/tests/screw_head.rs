//! A mounting hole that states `.head_diameter` keeps room for its screw head (ADR 0045, C3):
//! the placer keeps the other parts out of it, the placement check names a part that is in it,
//! and the board file has it as a rule area, so KiCad's own check and the design rules check
//! keep tracks, vias and pours out too. Every board here is invented: a 40 by 30 mm board
//! with a mounting hole in the middle and resistors.

use netsmith_check::Loader;
use netsmith_fabricate::fab::{FabOptions, fab_with_options};
use netsmith_kicad::{Mapping, Search, SymbolLib};
use netsmith_pcb::{Board, FootprintLibrary, PlacementProblem, Rect, kicad_pcb, place_options};
use netsmith_pipeline::{Libraries, build_with};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

/// The board: a rail and `parts` resistors between its nets.
fn source(parts: usize) -> String {
    let mut body = String::new();
    for n in 1..=parts {
        body += &format!(
            "    .r{n}: Resistor {{ .resistance &= 15kohm +/- 1% }}\n    .rail.pos ~> .r{n} ~> .rail.neg\n"
        );
    }
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
{body}}}
"
    )
}

/// The physical file: a 40 by 30 mm outline, a hole in its middle with a 12 mm head, and
/// `extra`.
fn physical(head: &str, extra: &str) -> String {
    format!(
        "import std::parts::mechanical::MountingHole
import std::pcb::Outline

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= 40mm
            .height &= 30mm
        }}
        .hole: MountingHole {{
{head}        }}
    }}
    .hole.place.x &= 20mm
    .hole.place.y &= 15mm
{extra}}}
"
    )
}

const HEAD: &str = "            .head_diameter &= 12mm\n";

fn loader(head: &str, extra: &str) -> Loader {
    Loader::std().with_physical(
        "widget.nsm",
        "widget.physical.nsm",
        &physical(head, extra),
    )
}

/// The libraries: the standard mapping, and footprints that netsmith generates, so that no
/// KiCad install is read (a hole's footprint is generated).
fn libraries() -> Libraries {
    let nowhere = TempDir::new("screw-head-no-kicad");
    let footprints = FootprintLibrary::installed_by(&Search::under(nowhere.path()));
    Libraries::with_mapping(
        Mapping::standard(),
        SymbolLib::at(netsmith_testkit::sample_symbols()),
        footprints,
    )
}

/// The board of `parts` resistors, placed by the placer on the sample libraries.
fn place(parts: usize, head: &str, extra: &str) -> Board {
    let libraries = libraries();
    let built = build_with(
        "widget.nsm",
        &source(parts),
        &loader(head, extra),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut options = place_options(&built[0].design, None);
    options.lines_warn = true;
    netsmith_placer::board_from_design_placed(
        &built[0].design,
        &libraries.mapping,
        &libraries.footprints,
        &options,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_head_is_a_keepout_the_hole_carries_round_its_centre() {
    let board = place(1, HEAD, "");
    let hole = board.part("H1").expect("H1");
    let areas = board.keepout_areas();
    assert_eq!(areas.len(), 1, "{areas:?}");
    let head = &areas[0];
    assert_eq!(head.name, "screw head of hole");
    assert_eq!(head.carrier.as_deref(), Some("hole"));
    assert!(head.tracks && head.vias && head.copper && head.parts);
    let c = hole.at;
    assert_eq!(
        (head.rect.min.x, head.rect.min.y, head.rect.max.x, head.rect.max.y),
        (c.x - 6.0, c.y - 6.0, c.x + 6.0, c.y + 6.0)
    );
}

#[test]
fn a_hole_with_no_head_keeps_nothing_out() {
    let board = place(1, "", "");
    assert!(board.keepout_areas().is_empty());
}

#[test]
fn the_placer_keeps_every_other_part_out_of_the_head() {
    let board = place(5, HEAD, "");
    let problems = board.check_placement();
    assert_eq!(problems, vec![], "a clean placement");
    let hole = board.part("H1").expect("H1").at;
    let head = Rect::new(
        netsmith_pcb::Pt::new(hole.x - 6.0, hole.y - 6.0),
        netsmith_pcb::Pt::new(hole.x + 6.0, hole.y + 6.0),
    );
    for part in board.parts.iter().filter(|p| p.refdes != "H1") {
        for room in part.courtyards() {
            let apart = room.max.x <= head.min.x
                || room.min.x >= head.max.x
                || room.max.y <= head.min.y
                || room.min.y >= head.max.y;
            assert!(apart, "{} is in the head of H1: {room:?}", part.refdes);
        }
    }
}

#[test]
fn a_part_pinned_in_the_head_is_named_with_the_screw_head() {
    let board = place(
        1,
        HEAD,
        "    .r1.place.x &= 25mm\n    .r1.place.y &= 15mm\n",
    );
    let problems = board.check_placement();
    let text: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert!(
        problems.iter().any(|p| matches!(
            p,
            PlacementProblem::InKeepout { refdes, keepout, .. }
                if refdes == "R1" && keepout == "screw head of hole"
        )),
        "{text:?}"
    );
    assert!(
        text.iter().any(|t| t.ends_with("overlaps the screw head of `hole`, which forbids parts")),
        "{text:?}"
    );
}

#[test]
fn the_board_file_has_the_head_as_a_rule_area() {
    let text = kicad_pcb(&place(1, HEAD, ""));
    assert!(text.contains("(name \"screw head of hole\")"), "{text}");
    assert!(text.contains("(tracks not_allowed)"), "{text}");
    assert!(text.contains("(copperpour not_allowed)"), "{text}");
}

#[test]
fn fab_refuses_a_part_pinned_in_the_head() {
    let dir = TempDir::unused("screw-head-fab");
    let made = fab_with_options(
        "widget.nsm",
        &source(1),
        &loader(HEAD, "    .r1.place.x &= 25mm\n    .r1.place.y &= 15mm\n"),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &libraries(),
        None,
        &FabOptions::default(),
    );
    let error = made.expect_err("R1 is in the head of H1");
    assert!(error.contains("screw head"), "{error}");
}

#[test]
fn the_router_lays_no_track_through_the_head() {
    // The two resistors share a net and stand on either side of the head, so the straight
    // way between them is through it.
    let mut board = place(
        2,
        HEAD,
        "    .r1.place.x &= 6mm\n    .r1.place.y &= 15mm\n    .r2.place.x &= 34mm\n    .r2.place.y &= 15mm\n",
    );
    assert_eq!(board.check_placement(), vec![], "a clean placement");
    netsmith_router::route(&mut board);
    assert!(!board.tracks.is_empty(), "the router laid tracks");
    let hole = board.part("H1").expect("H1").at;
    for track in &board.tracks {
        for step in 0..=50 {
            let t = f64::from(step) / 50.0;
            let (x, y) = (
                track.from.x + (track.to.x - track.from.x) * t,
                track.from.y + (track.to.y - track.from.y) * t,
            );
            assert!(
                (x - hole.x).abs() >= 6.0 - 1e-6 || (y - hole.y).abs() >= 6.0 - 1e-6,
                "a track of {} is in the head at ({x:.2}, {y:.2})",
                track.net
            );
        }
    }
}
