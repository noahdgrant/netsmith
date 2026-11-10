//! Relations between two parts (ADR 0045, section 3), from the physical file to the package:
//! the placer holds them, the check measures them with every number, and an unmet one stops
//! `fab`. The board is invented: two rows of sockets that a datasheet puts 17.5 mm apart (in
//! a module), and an aligned pair across another module.

use netsmith_kicad::testlib::TempDir;
use std::fs;

use netsmith::Libraries;
use netsmith::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith_check::Loader;
use netsmith_fab::KicadCliExe;
use netsmith_solve::Catalogs;

const BOARD: &str = "\
import std::parts::connector::{Header1x4, Socket1x4, Socket1x10}
import std::parts::passive::Resistor
import std::mfg::jlcpcb::TwoLayer

module Radio {
    .row_a: Socket1x10
    .row_b: Socket1x10
    .row_a.p1 ~ .row_b.p1
    .row_a.p2 ~ .row_b.p2
}

module Gnss {
    .antenna: Header1x4
    .coax: Socket1x4
    .antenna.p1 ~ .coax.p1
    .antenna.p2 ~ .coax.p2
}

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .radio: Radio
    .gnss: Gnss
}
";

/// What the invented datasheet says: the rows are 17.5 mm apart, and the antenna and the coax
/// socket sit on one horizontal line.
const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Widget {
    case {
        .outline: Outline {
            .width &= 60mm
            .height &= 50mm
        }
    }

    // Datasheet figure 4: the two rows are 17.5 mm +/- 0.05 mm apart, centre to centre.
    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm
    // Cable drawing: the antenna header and the coax socket are on one line.
    .gnss.antenna.place.y == .gnss.coax.place.y
}
";

fn have_libraries() -> bool {
    KicadCliExe::locate_runnable().is_some()
}

fn fab(
    name: &str,
    physical: &str,
    layout: &str,
    catalogs: &Catalogs,
) -> (TempDir, Result<Vec<Fabricated>, String>) {
    let dir = TempDir::unused(&format!("relations-test-{name}"));
    let file = dir.join("source").join("widget.pcb.toml");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    if !layout.is_empty() {
        fs::write(&file, layout).unwrap();
    }
    let made = fab_with_options(
        "widget.nsm",
        BOARD,
        &Loader::std().with_physical("widget.nsm", "widget.physical.nsm", physical),
        &dir.join("fab"),
        catalogs,
        &Libraries::standard(),
        None,
        &FabOptions {
            layout: Some(file),
            ..FabOptions::default()
        },
    );
    (dir, made)
}

fn pcb_of(dir: &TempDir) -> String {
    fs::read_to_string(dir.join("fab/Widget/Widget.kicad_pcb")).unwrap()
}

#[test]
fn the_placer_holds_the_row_spacing_and_the_alignment_and_the_package_says_so() {
    if !have_libraries() {
        return;
    }
    let (dir, made) = fab("met", PHYSICAL, "", &Catalogs::jlcpcb());
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    let notes = &made[0].notes;
    assert!(
        notes.contains("design rules check: passed")
            && notes.contains("constraints: 2 of 2 satisfied"),
        "{notes}"
    );
    // Not evaluated no more: the relations are the placer's.
    assert!(!notes.contains("not evaluated"), "{notes}");
    // The check measured the board file itself, the two relations among its requirements.
    let pcb = pcb_of(&dir);
    assert!(pcb.contains("(footprint"), "a board was written");
}

#[test]
fn the_same_source_gives_the_same_board_with_relations() {
    if !have_libraries() {
        return;
    }
    let read = |name: &str| {
        let (dir, made) = fab(name, PHYSICAL, "", &Catalogs::jlcpcb());
        made.unwrap_or_else(|e| panic!("{e}"));
        pcb_of(&dir)
    };
    assert_eq!(read("same-a"), read("same-b"));
}

/// A layout file that puts the second row 16 mm from the first, by hand, and locks them.
const LOCKED: &str = "version = 1\n[Widget.parts.\"radio.row_a\"]\nat = [10.0, 20.0]\nlock = true\n[Widget.parts.\"radio.row_b\"]\nat = [26.0, 20.0]\nlock = true\n";

/// The same, not locked: the layout file's places win over the placer, and the check
/// measures them.
const MOVED: &str = "version = 1\n[Widget.parts.\"radio.row_a\"]\nat = [10.0, 20.0]\n[Widget.parts.\"radio.row_b\"]\nat = [26.0, 20.0]\n";

#[test]
fn rows_put_the_wrong_distance_apart_by_hand_stop_fab_with_every_number() {
    if !have_libraries() {
        return;
    }
    let (dir, made) = fab("moved", PHYSICAL, MOVED, &Catalogs::jlcpcb());
    let e = made.expect_err("the rows are 16 mm apart");
    assert!(
        e.contains("error [constraint] constraint radio.row_b.place.x - radio.row_a.place.x (widget.physical.nsm:")
            && e.contains("the centre of the body of J2 (`radio.row_b`, F.Fab) minus the centre of the body of J1 (`radio.row_a`, F.Fab) is 16 mm along x, and it allows 17.45 to 17.55 mm (1.45 mm over)"),
        "{e}"
    );
    assert!(
        !dir.join("fab/Widget/Widget-gerbers.zip").exists(),
        "nothing is packaged"
    );
}

#[test]
fn a_relation_that_cannot_be_met_between_two_locked_parts_is_reported_before_placing() {
    if !have_libraries() {
        return;
    }
    // The lock breaks the window, and `lock` says the placer may not move either part.
    let (_dir, made) = fab("locked", PHYSICAL, LOCKED, &Catalogs::jlcpcb());
    let e = made.expect_err("both rows are locked 16 mm apart");
    assert!(
        e.contains("the requirements on where parts go cannot all be met")
            && e.contains("J1 (`radio.row_a`) is fixed (the layout file locks it at (10, 20))")
            && e.contains("J2 (`radio.row_b`) is fixed (the layout file locks it at (26, 20))")
            && e.contains("the centre of the body of J2 (`radio.row_b`) minus the centre of the body of J1 (`radio.row_a`) is 16 mm along x, and it allows 17.45 to 17.55 mm (1.45 mm over)"),
        "{e}"
    );
}

#[test]
fn with_constraints_warn_an_unmet_relation_is_a_warning_in_the_notes() {
    if !have_libraries() {
        return;
    }
    let catalogs = Catalogs::jlcpcb().with_constraints_warn(true);
    // Unlocked: the layout file's position wins over the placer, and the check measures it.
    let moved = "version = 1\n[Widget.parts.\"radio.row_b\"]\nat = [40.0, 20.0]\n";
    let (_dir, made) = fab("warned", PHYSICAL, moved, &catalogs);
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    assert!(
        made[0]
            .notes
            .contains("warning [constraint] constraint radio.row_b.place.x - radio.row_a.place.x"),
        "{}",
        made[0].notes
    );
}

/// Place `physical` (no routing) and give the board.
fn placed(physical: &str) -> Result<netsmith_pcb::Board, String> {
    let loader = Loader::std().with_physical("widget.nsm", "widget.physical.nsm", physical);
    let libs = Libraries::standard();
    let built = netsmith::build_with("widget.nsm", BOARD, &loader, &Catalogs::jlcpcb(), &libs)
        .unwrap_or_else(|e| panic!("{e}"));
    let design = &built[0].design;
    netsmith_pcb::Board::from_design_placed(
        design,
        &libs.mapping,
        &libs.footprints,
        &netsmith_pcb::place_options(design, None),
    )
    .map_err(|e| e.to_string())
}

/// The centre of the body of the part at `path`, on the sheet.
fn centre(board: &netsmith_pcb::Board, path: &str) -> (f64, f64) {
    let part = board
        .parts
        .iter()
        .find(|p| p.path == path)
        .unwrap_or_else(|| panic!("no part {path}"));
    let c = part.body().centre();
    (c.x - board.outline.min.x, c.y - board.outline.min.y)
}

/// The parts of `BOARD` with a case outline of the given size.
fn physical_with(body: &str) -> String {
    format!(
        "import std::pcb::Outline

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= 60mm
            .height &= 50mm
        }}
    }}
{body}}}
"
    )
}

#[test]
fn a_relation_across_modules_with_bodies_off_the_grid_holds_exactly() {
    if !have_libraries() {
        return;
    }
    // The socket row and the antenna header have bodies whose centres are not a multiple of
    // the placer's 0.05 mm grid from their origins: the pair is put off the grid to line up.
    let board = placed(&physical_with(
        "    .gnss.antenna.place.y == .radio.row_a.place.y\n    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm\n",
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    let (a, b, h) = (
        centre(&board, "radio.row_a"),
        centre(&board, "radio.row_b"),
        centre(&board, "gnss.antenna"),
    );
    assert!(
        (h.1 - a.1).abs() <= 1e-4,
        "the antenna's centre is {} mm from the row's along y",
        h.1 - a.1
    );
    assert!(
        ((b.0 - a.0) - 17.5).abs() <= 0.05 + 1e-4,
        "the rows are {} mm apart",
        b.0 - a.0
    );
    assert_eq!(board.check_placement(), vec![]);
}

#[test]
fn a_chain_of_relations_holds_in_every_link() {
    if !have_libraries() {
        return;
    }
    let board = placed(&physical_with(
        "    .radio.row_b.place.x - .radio.row_a.place.x == 20mm +/- 0.01mm\n    .gnss.antenna.place.x - .radio.row_b.place.x == 12mm +/- 0.01mm\n    .gnss.coax.place.y == .gnss.antenna.place.y\n",
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    let c = |p: &str| centre(&board, p);
    assert!(((c("radio.row_b").0 - c("radio.row_a").0) - 20.0).abs() <= 0.01 + 1e-4);
    assert!(((c("gnss.antenna").0 - c("radio.row_b").0) - 12.0).abs() <= 0.01 + 1e-4);
    assert!((c("gnss.coax").1 - c("gnss.antenna").1).abs() <= 1e-4);
    assert_eq!(board.check_placement(), vec![]);
}

#[test]
fn a_part_pinned_by_its_requirements_is_where_the_other_follows_it() {
    if !have_libraries() {
        return;
    }
    let board = placed(&physical_with(
        "    .radio.row_a.place.x &= 10mm\n    .radio.row_a.place.y &= 25mm\n    .radio.row_a.place.rotation &= 0\n    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm\n",
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    let (a, b) = (centre(&board, "radio.row_a"), centre(&board, "radio.row_b"));
    assert!(
        (a.0 - 10.0).abs() <= 1e-4 && (a.1 - 25.0).abs() <= 1e-4,
        "{a:?}"
    );
    assert!(((b.0 - a.0) - 17.5).abs() <= 0.05 + 1e-4, "{b:?}");
}

#[test]
fn the_same_inputs_give_the_same_board_for_relations_too() {
    if !have_libraries() {
        return;
    }
    let body = "    .gnss.antenna.place.y == .radio.row_a.place.y\n    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm\n";
    let at = |board: &netsmith_pcb::Board| -> Vec<(String, f64, f64, f64)> {
        board
            .parts
            .iter()
            .map(|p| (p.refdes.clone(), p.at.x, p.at.y, p.rotation))
            .collect()
    };
    let first = at(&placed(&physical_with(body)).unwrap());
    let second = at(&placed(&physical_with(body)).unwrap());
    assert_eq!(first, second);
}

#[test]
fn two_values_that_break_a_relation_are_an_error_with_the_numbers_before_any_placing() {
    if !have_libraries() {
        return;
    }
    let loader = Loader::std().with_physical(
        "widget.nsm",
        "widget.physical.nsm",
        &physical_with(
            "    .radio.row_a.place.x &= 10mm\n    .radio.row_b.place.x &= 26mm\n    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm\n",
        ),
    );
    let e = netsmith::build_with(
        "widget.nsm",
        BOARD,
        &loader,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
    )
    .err()
    .expect("the rows are 16 mm apart");
    assert!(
        e.contains("(radio.row_b.place.x - radio.row_a.place.x) is 16mm")
            && e.contains("17.45mm..17.55mm")
            && e.contains("widget.physical.nsm:"),
        "{e}"
    );
}

#[test]
fn a_part_whose_own_window_cannot_reach_the_relation_is_reported_with_the_numbers() {
    if !have_libraries() {
        return;
    }
    // Row A is pinned at x = 10; row B must be 17.5 mm from it, and may not pass x = 20.
    let e = placed(&physical_with(
        "    .radio.row_a.place.x &= 10mm\n    .radio.row_a.place.y &= 25mm\n    .radio.row_a.place.rotation &= 0\n    .radio.row_b.place.x <= 20mm\n    .radio.row_b.place.rotation &= 0\n    .radio.row_b.place.x - .radio.row_a.place.x == 17.5mm +/- 0.05mm\n",
    ))
    .expect_err("row B cannot be at 27.5 mm");
    assert!(
        e.contains("cannot be met")
            && e.contains("J1 (`radio.row_a`) is fixed")
            && e.contains("J2 (`radio.row_b`) cannot be where it holds")
            && e.contains("the centre of its body must be 27.45 to 27.55 mm along x, and its requirements allow at most 20 mm"),
        "{e}"
    );
}

#[test]
fn two_relations_that_cannot_both_hold_are_reported_by_the_check_and_not_looped_on() {
    if !have_libraries() {
        return;
    }
    let physical = physical_with(
        "    .radio.row_b.place.x - .radio.row_a.place.x >= 20mm\n    .radio.row_b.place.x - .radio.row_a.place.x <= 18mm\n",
    );
    let started = std::time::Instant::now();
    let (_dir, made) = fab("contradiction", &physical, "", &Catalogs::jlcpcb());
    let e = made.expect_err("no placement holds both");
    // `fab` stops before placing and names the two lines that cannot both hold (issue #80,
    // ADR 0045 section 14).
    assert!(
        e.contains("no placement meets every requirement of the physical file")
            && e.contains("these lines cannot both hold: widget.physical.nsm:")
            && e.contains("`.radio.row_b.place.x - .radio.row_a.place.x >= 20mm`")
            && e.contains("`.radio.row_b.place.x - .radio.row_a.place.x <= 18mm`")
            && e.contains(
                "to be at least 20 mm and at most 18 mm, and no value is in both (2 mm apart)"
            ),
        "{e}"
    );
    // The lines are proved to conflict before the placer starts, so there is no arrangement
    // to measure: nothing is said of where the placer ended.
    assert!(!e.contains("where the placer ended"), "{e}");
    assert!(
        started.elapsed().as_secs() < 60,
        "it took {:?}",
        started.elapsed()
    );
}
