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
    KicadCliExe::locate().is_some()
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
            && e.contains("the centre of the body of J4 (`radio.row_b`, F.Fab) minus the centre of the body of J3 (`radio.row_a`, F.Fab) is 16 mm along x, and it allows 17.45 to 17.55 mm (1.45 mm over)"),
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
            && e.contains("J3 (`radio.row_a`) is fixed (the layout file locks it at (10, 20))")
            && e.contains("J4 (`radio.row_b`) is fixed (the layout file locks it at (26, 20))")
            && e.contains("the centre of the body of J4 (`radio.row_b`) minus the centre of the body of J3 (`radio.row_a`) is 16 mm along x, and it allows 17.45 to 17.55 mm (1.45 mm over)"),
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
