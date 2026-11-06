//! Pin protrusion (ADR 0045 section 15; owner decisions of 2026-11-04): a flat part on the
//! back over the cut pins of a 1x10 socket row on the front, from the physical file to the
//! package. The board is invented: 60 x 40 mm, a lid zone that allows 3.6 mm on the back, a
//! flat part 3.4 mm tall that is fitted by hand, and pins that protrude 0.2 mm.
//!
//! The board is a project (the flat part is invented and maps to a footprint of the project's
//! own), so it is fabricated with KiCad's libraries, and the fab tests skip themselves when
//! they are not installed.

use std::fs;

use netsmith::Libraries;
use netsmith::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith_check::Loader;
use netsmith_kicad::testlib::{TempDir, skip_without_kicad_libraries};
use netsmith_solve::Catalogs;

const CIRCUIT: &str = "\
import std::parts::connector::Socket1x10
import std::mfg::jlcpcb::TwoLayer

/// An invented flat part, 30 x 26 mm, with a pad at each end.
part FlatCell {
    .designator &= \"U\"

    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }

    .package: Package
}

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .row: Socket1x10
    .cell: FlatCell { .assembly &= AssemblyMode::Hand }
    .row.p1 ~ .cell.a
}
";

const KICAD_TOML: &str = "\
[[library]]
name = \"flat\"
footprints = \"libs/flat.pretty\"

[[part]]
kind = \"FlatCell\"
package = \"Flat\"
default = true
symbol = \"Connector_Generic:Conn_01x02\"
footprint = \"flat:Flat\"

[part.pin_names]
A = \"Pin_1\"
B = \"Pin_2\"
";

const FOOTPRINT: &str = "(footprint \"Flat\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
     (property \"Reference\" \"REF**\" (at 0 -14 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (property \"Value\" \"Flat\" (at 0 14 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
     (attr smd) \
     (pad \"1\" smd rect (at -12 0) (size 2 2) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
     (pad \"2\" smd rect (at 12 0) (size 2 2) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
     (fp_rect (start -14.5 -12.5) (end 14.5 12.5) (stroke (width 0.1) (type solid)) (fill none) (layer \"F.Fab\")) \
     (fp_rect (start -15 -13) (end 15 13) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n";

/// The physical file: the outline, the lid zone and the two parts pinned over each other;
/// `protrusion` and `height` are the lines under test.
fn physical(protrusion: &str, height: &str) -> String {
    format!(
        "import std::pcb::{{Keepout, Outline}}

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= 60mm
            .height &= 40mm
        }}
    }}

    // Case drawing, slot D (invented): the lid leaves 3.6 mm under the board.
    .back_room: Keepout {{
        .x &= 0mm
        .y &= 0mm
        .width &= 60mm
        .height &= 40mm
        .layers &= \"bottom\"
        .max_height &= 3.6mm
    }}

    .row.place.side &= \"top\"
    .row.place.rotation &= 0
    .row.place.x &= 30mm +/- 0.5mm
    .row.place.y &= 20mm +/- 0.5mm
    .cell.place.side &= \"bottom\"
    .cell.place.x &= 30mm +/- 1mm
    .cell.place.y &= 20mm +/- 1mm
{height}{protrusion}}}
"
    )
}

const HEIGHT: &str = "    // Datasheet: the flat part is 3.4 mm tall.\n    .cell.height &= 3.4mm\n";
const PROTRUSION: &str = "    // After soldering the row's pins are cut to protrude 0.2 mm.\n    .row.pin_protrusion &= 0.2mm\n";

/// The project in a folder, and the result of `fab` on it with `physical`.
fn fab(
    name: &str,
    physical: &str,
) -> (TempDir, Result<Vec<Fabricated>, String>, Option<Libraries>) {
    let dir = TempDir::unused(&format!("protrusion-test-{name}"));
    let project = dir.join("project");
    fs::create_dir_all(project.join("libs/flat.pretty")).unwrap();
    fs::write(project.join("kicad.toml"), KICAD_TOML).unwrap();
    fs::write(project.join("libs/flat.pretty/Flat.kicad_mod"), FOOTPRINT).unwrap();
    let libraries = Libraries::for_project(Some(&project)).unwrap_or_else(|e| panic!("{e}"));
    let made = fab_with_options(
        "widget.nsm",
        CIRCUIT,
        &Loader::std().with_physical("widget.nsm", "widget.physical.nsm", physical),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &libraries,
        None,
        &FabOptions::default(),
    );
    (dir, made, Some(libraries))
}

/// The line number of the first line of `text` that has `needle`.
fn line_of(text: &str, needle: &str) -> usize {
    text.lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no {needle:?}"))
        + 1
}

#[test]
fn a_flat_part_over_pins_that_protrude_0_2_mm_places_and_the_package_says_how_to_cut_them() {
    if skip_without_kicad_libraries() {
        return;
    }
    let text = physical(PROTRUSION, HEIGHT);
    let (dir, made, _) = fab("works", &text);
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    let notes = &made[0].notes;
    assert!(notes.contains("design rules check: passed"), "{notes}");
    // The step for a person, next to the hand-solder line, with the line that states it.
    let line = line_of(&text, ".row.pin_protrusion &= 0.2mm");
    let expected = format!(
        "trim: J1 (row): after soldering, cut the pins to protrude 0.20 mm from the bottom face, before U1 (cell) is fitted (widget.physical.nsm:{line}); measure one protruding pin"
    );
    assert!(notes.contains(&expected), "{expected}\n{notes}");
    let lines: Vec<&str> = notes.lines().collect();
    let hand = lines
        .iter()
        .position(|l| l.starts_with("hand-solder:"))
        .unwrap_or_else(|| panic!("{notes}"));
    assert!(
        lines[hand].contains("J1") && lines[hand].contains("U1"),
        "{notes}"
    );
    assert_eq!(
        lines[hand + 1],
        expected,
        "the trim line follows it: {notes}"
    );
    // Every requirement is met on the file written.
    let on_disk = fs::read_to_string(dir.join("fab/Widget/fab-notes.txt")).unwrap();
    assert_eq!(&on_disk, notes);
}

#[test]
fn without_a_protrusion_the_same_board_is_refused_and_the_message_names_the_member() {
    if skip_without_kicad_libraries() {
        return;
    }
    let (_, made, _) = fab("none", &physical("", HEIGHT));
    let e = made.expect_err("the cell stands over the pads of the row");
    assert!(
        e.contains("J1") && e.contains("U1") && e.contains("`.row.pin_protrusion &= 0.2mm`"),
        "{e}"
    );
}

#[test]
fn a_protrusion_that_leaves_too_little_room_names_the_numbers_and_the_lines() {
    if skip_without_kicad_libraries() {
        return;
    }
    let text = physical("    .row.pin_protrusion &= 0.4mm\n", HEIGHT);
    let (_, made, _) = fab("f1", &text);
    let e = made.expect_err("3.4 + 0.4 mm does not fit 3.6 mm");
    let line = line_of(&text, ".row.pin_protrusion &= 0.4mm");
    for expected in [
        // Both parts are pinned, so the lines are proved to conflict before the placer starts.
        "these lines cannot both hold",
        "over a pad of J1 (`row`) (pins protruding 0.4 mm)",
        "protrude 0.40 mm",
        &format!("widget.physical.nsm:{line}"),
        "3.80 mm",
        "`back_room` allows 3.60 mm",
        "0.20 mm too much",
        "cut the pins to protrude 0.20 mm or less",
    ] {
        assert!(e.contains(expected), "missing {expected:?} in\n{e}");
    }
}

#[test]
fn a_covering_part_with_no_height_is_an_error() {
    if skip_without_kicad_libraries() {
        return;
    }
    let (_, made, _) = fab("f2", &physical(PROTRUSION, ""));
    let e = made.expect_err("the stack cannot be checked");
    assert!(
        e.contains("U1 states no height") && e.contains("`.cell.height`"),
        "{e}"
    );
}

#[test]
fn a_protrusion_longer_than_the_part_allows_is_a_refused_claim() {
    // A part of this kind has pins that protrude at most 3 mm; the board states 5 mm.
    let source = "\
import std::mfg::jlcpcb::TwoLayer

part ShortPins {
    .designator &= \"J\"
    pins {
        .p1 passive \"P1\" \"1\"
    }
    .pin_protrusion <= 3mm
    .package: Package
}

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .j: ShortPins
}
";
    let checked = |extra: &str| {
        netsmith_check::check(
            "widget.nsm",
            source,
            &Loader::std().with_physical(
                "widget.nsm",
                "widget.physical.nsm",
                &format!("physical for Widget {{\n{extra}}}\n"),
            ),
        )
    };
    // The diagnostics of checking and solving, as the tools print them.
    let problems = |extra: &str| -> String {
        let checked = checked(extra);
        assert!(checked.is_ok(), "{}", checked.render());
        checked
            .standalone_boards()
            .flat_map(|board| {
                netsmith_solve::solve(board, &Catalogs::jlcpcb())
                    .diagnostics
                    .iter()
                    .map(|d| d.render(&checked.sources))
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    // Within what the part allows, nothing is said.
    assert_eq!(problems("    .j.pin_protrusion &= 2mm\n"), "");
    // Past it, nothing is left: the message gives both numbers and names the field.
    let e = problems("    .j.pin_protrusion &= 5mm\n");
    assert!(
        e.contains("pin_protrusion") && e.contains("5mm") && e.contains("3mm"),
        "{e}"
    );
}
