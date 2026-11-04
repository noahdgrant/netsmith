//! The `row` statement of a physical file (ADR 0048, "Syntax decisions" 11), from the file to
//! the package: the placer holds the pitch and the common line, and a pad moved by hand is a
//! finding that names the statement's line and the two neighbours. The board is invented: a
//! module of twelve one-pin-pair headers that stand for a row of test pads.

use netsmith_kicad::testlib::TempDir;
use std::fs;

use netsmith::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_fab::KicadCliExe;
use netsmith_solve::Catalogs;

const BOARD: &str = "\
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

module Pads {
    .tp1: Header1x2
    .tp2: Header1x2
    .tp3: Header1x2
    .tp4: Header1x2
    .tp5: Header1x2
    .tp6: Header1x2
    .tp7: Header1x2
    .tp8: Header1x2
    .tp9: Header1x2
    .tp10: Header1x2
    .tp11: Header1x2
    .tp12: Header1x2
}

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .pads: Pads
}
";

/// The row is on line 11 of this file; a finding must say so.
const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Widget {
    case {
        .outline: Outline {
            .width &= 80mm
            .height &= 40mm
        }
    }

    row .pads.tp1 .. .pads.tp12 along x pitch 5mm +/- 0.05mm
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
    let dir = TempDir::unused(&format!("row-test-{name}"));
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

#[test]
fn the_placer_holds_a_row_and_the_package_counts_each_pair() {
    if !have_libraries() {
        return;
    }
    let (_dir, made) = fab("met", PHYSICAL, "", &Catalogs::jlcpcb());
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    let notes = &made[0].notes;
    // Eleven pairs of neighbours, a pitch and a common line each.
    assert!(
        notes.contains("design rules check: passed")
            && notes.contains("constraints: 22 of 22 satisfied"),
        "{notes}"
    );
}

#[test]
fn a_pad_moved_by_hand_is_a_finding_with_the_row_s_line_and_the_two_neighbours() {
    if !have_libraries() {
        return;
    }
    // The layout file's place wins over the placer, and the check measures it: `tp7` is 2 mm
    // too far from `tp6`, and each is off the common line of its neighbours.
    let moved = "version = 1\n[Widget.parts.\"pads.tp6\"]\nat = [30.0, 20.0]\n[Widget.parts.\"pads.tp7\"]\nat = [37.0, 20.0]\n";
    let (_dir, made) = fab("moved", PHYSICAL, moved, &Catalogs::jlcpcb());
    let e = made.expect_err("tp7 is 7 mm from tp6");
    assert!(
        e.contains("error [constraint] constraint pads.tp7.place.x - pads.tp6.place.x (widget.physical.nsm:11) is violated")
            && e.contains("is 7 mm along x, and it allows 4.95 to 5.05 mm (1.95 mm over)"),
        "{e}"
    );
    // The two neighbours by designator and path: `tp6` is J6 and `tp7` is J7 in natural order.
    assert!(
        e.contains("J7 (`pads.tp7`") && e.contains("J6 (`pads.tp6`"),
        "{e}"
    );
}

#[test]
fn parts_are_numbered_in_natural_order_so_the_names_and_the_labels_agree() {
    let loader = Loader::std().with_physical("widget.nsm", "widget.physical.nsm", PHYSICAL);
    let built = build_with(
        "widget.nsm",
        BOARD,
        &loader,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let design = &built[0].design;
    let refdes = |path: &str| {
        design
            .parts
            .iter()
            .find(|p| p.path == path)
            .unwrap_or_else(|| panic!("no part {path}"))
            .refdes
            .clone()
    };
    for n in 1..=12 {
        assert_eq!(refdes(&format!("pads.tp{n}")), format!("J{n}"));
    }
}
