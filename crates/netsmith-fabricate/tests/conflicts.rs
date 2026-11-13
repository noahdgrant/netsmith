//! When no placement meets the requirements of a physical file, the refusal names the lines
//! (ADR 0045): every requirement not met where the placer ended, with its
//! `file:line`, the text of the line, the part, the measured value, the window and how far
//! out; and the lines that cannot all hold, where that is proved from the windows the lines
//! leave, the keepouts, the height limits and the outline. A conflict that cannot be proved
//! is not claimed. Every board here is invented: a 40 by 30 mm board with resistors, and
//! headers.
//!
//! The lines are proved to conflict before the placer starts (`tests/precheck.rs`). These
//! tests place with `lines_warn` (constraints are warnings for the run), so that the placer
//! runs anyway and the report after placing, which says where it ended, is still tested.

use netsmith_fabricate::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith_pipeline::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_pcb::{Board, BoardError, PlacementProblem, Pt, RequirementReport, place_options};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

/// A board of `parts` resistors and the physical file of `extra` on a 40 x 30 mm outline.
fn source(parts: usize, headers: &str) -> String {
    let mut body = String::new();
    for n in 1..=parts {
        body += &format!(
            "    .r{n}: Resistor {{ .resistance &= 15kohm +/- 1% }}\n    .rail.pos ~> .r{n} ~> .rail.neg\n"
        );
    }
    format!(
        "import std::parts::connector::{{Header1x2, Header1x4}}
import std::parts::passive::Resistor
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
{body}{headers}}}
"
    )
}

fn physical(width: u32, height: u32, extra: &str) -> String {
    format!(
        "import std::pcb::{{Cutout, Keepout, Outline}}

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= {width}mm
            .height &= {height}mm
        }}
    }}
{extra}}}
"
    )
}

fn loader(width: u32, height: u32, extra: &str) -> Loader {
    Loader::std().with_physical(
        "widget.nsm",
        "widget.physical.nsm",
        &physical(width, height, extra),
    )
}

/// Place the board of `parts` resistors with `extra` in its physical file.
fn place(
    parts: usize,
    headers: &str,
    size: (u32, u32),
    extra: &str,
) -> (netsmith_core::Design, Result<Board, BoardError>) {
    let libraries = Libraries::sample();
    let built = build_with(
        "widget.nsm",
        &source(parts, headers),
        &loader(size.0, size.1, extra),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let design = built[0].design.clone();
    let mut options = place_options(&design, None);
    options.lines_warn = true;
    let board = netsmith_placer::board_from_design_placed(
        &design,
        &libraries.mapping,
        &libraries.footprints,
        &options,
    );
    (design, board)
}

fn report(parts: usize, size: (u32, u32), extra: &str) -> RequirementReport {
    let (design, board) = place(parts, "", size, extra);
    let board = board.unwrap_or_else(|e| panic!("{e}"));
    board
        .explain_requirements(&design, &place_options(&design, None))
        .expect("a report")
}

/// A line of the physical file is its name, its number and its text.
const R1_LOW: &str = "widget.physical.nsm:10 `.r1.place.x >= 20mm`";
const R2_HIGH: &str = "widget.physical.nsm:11 `.r2.place.x <= 25mm`";
const PITCH: &str = "widget.physical.nsm:12 `.r2.place.x - .r1.place.x == 30mm +/- 0.1mm`";

/// A relation that the windows of its two parts leave no way to meet: `r1` is at least 20 mm
/// from the left edge, `r2` at most 25, and `r2` is to be 30 mm to the right of `r1`.
const PITCH_AND_WINDOWS: &str = "    .r1.place.x >= 20mm
    .r2.place.x <= 25mm
    .r2.place.x - .r1.place.x == 30mm +/- 0.1mm
";

#[test]
fn a_relation_the_windows_of_its_parts_cannot_meet_is_named_with_the_three_lines() {
    let report = report(2, (40, 30), PITCH_AND_WINDOWS);
    assert_eq!(report.conflicts.len(), 1, "{report}");
    let conflict = &report.conflicts[0];
    for want in [
        PITCH,
        R1_LOW,
        R2_HIGH,
        "the centre of the body of `r2` (R2) minus the centre of the body of `r1` (R1) must be 29.9 to 30.1 mm along x",
        "make it at most 5 mm (24.9 mm out)",
    ] {
        assert!(conflict.contains(want), "{want:?} is not in {conflict:?}");
    }
    // Where the placer ended, the relation is not held: its line, part, value, window, miss.
    assert_eq!(report.unmet.len(), 1, "{report}");
    let unmet = &report.unmet[0];
    for want in [
        PITCH,
        "of the body of `r2` (R2) minus the centre of the body of `r1` (R1) is ",
        "mm along x, and it allows 29.9 to 30.1 mm (",
        "mm out)",
    ] {
        assert!(unmet.contains(want), "{want:?} is not in {unmet:?}");
    }
    let text = report.to_string();
    assert!(
        text.starts_with("no placement meets every requirement of the physical file:\n  these lines cannot both hold: "),
        "{text}"
    );
    assert!(text.contains("\n  where the placer ended, 1 requirement(s) are not met:\n    "));
}

#[test]
fn two_relations_between_the_same_parts_that_no_value_meets_are_named() {
    let extra = "    .r2.place.x - .r1.place.x >= 20mm
    .r2.place.x - .r1.place.x <= 18mm
";
    let report = report(2, (40, 30), extra);
    assert_eq!(report.conflicts.len(), 1, "{report}");
    let conflict = &report.conflicts[0];
    for want in [
        "widget.physical.nsm:10 `.r2.place.x - .r1.place.x >= 20mm`; widget.physical.nsm:11 `.r2.place.x - .r1.place.x <= 18mm`",
        "the centre of the body of `r2` (R2) minus the centre of the body of `r1` (R1) to be at least 20 mm and at most 18 mm, and no value is in both (2 mm apart)",
    ] {
        assert!(conflict.contains(want), "{want:?} is not in {conflict:?}");
    }
    // The same two, one of them written the other way round.
    let extra = "    .r2.place.x - .r1.place.x >= 20mm
    .r1.place.x - .r2.place.x >= -18mm
";
    let report = self::report(2, (40, 30), extra);
    assert_eq!(report.conflicts.len(), 1, "{report}");
    assert!(
        report.conflicts[0].contains("and at most 18 mm, and no value is in both (2 mm apart)"),
        "{report}"
    );
}

#[test]
fn a_part_the_lines_pin_inside_a_keepout_is_named_with_the_keepout() {
    let extra = "    .r1.place.x &= 20mm
    .r1.place.y &= 10mm
    .blocked: Keepout {
        .x &= 15mm
        .y &= 5mm
        .width &= 10mm
        .height &= 10mm
    }
";
    let (design, board) = place(2, "", (40, 30), extra);
    let board = board.unwrap_or_else(|e| panic!("{e}"));
    let report = board
        .explain_requirements(&design, &place_options(&design, None))
        .expect("a report");
    assert_eq!(report.conflicts.len(), 1, "{report}");
    let conflict = &report.conflicts[0];
    for want in [
        "widget.physical.nsm:10 `.r1.place.x &= 20mm`",
        "widget.physical.nsm:11 `.r1.place.y &= 10mm`",
        "the courtyard of `r1` (R1) within x 19.07 to 20.93, y 9.07 to 10.93 mm",
        "the keepout `blocked` (x 15 to 25, y 5 to 15 mm, which forbids parts)",
        "by at least 0.94 x 0.94 mm wherever it is put",
    ] {
        assert!(conflict.contains(want), "{want:?} is not in {conflict:?}");
    }
    // The part is where its lines put it, so no line is unmet: the keepout is what it meets.
    assert_eq!(report.unmet, Vec::<String>::new());
    assert!(
        board
            .check_placement()
            .iter()
            .any(|p| matches!(p, PlacementProblem::InKeepout { refdes, .. } if refdes == "R1"))
    );
}

#[test]
fn two_parts_whose_windows_leave_their_courtyards_over_each_other_are_named() {
    let extra = "    .r1.place.x &= 10mm +/- 0.1mm
    .r1.place.y &= 10mm +/- 0.1mm
    .r2.place.x &= 10.3mm +/- 0.1mm
    .r2.place.y &= 10mm +/- 0.1mm
";
    let report = report(2, (40, 30), extra);
    assert_eq!(report.conflicts.len(), 1, "{report}");
    let conflict = &report.conflicts[0];
    for want in [
        "widget.physical.nsm:10 `.r1.place.x &= 10mm +/- 0.1mm`",
        "widget.physical.nsm:13 `.r2.place.y &= 10mm +/- 0.1mm`",
        "the courtyard of `r1` (R1) within x 8.97 to 11.03, y 8.97 to 11.03 mm",
        "that of `r2` (R2) within x 9.27 to 11.33, y 8.97 to 11.03 mm",
        "the two overlap by at least 0.44 x 0.74 mm wherever they are put",
    ] {
        assert!(conflict.contains(want), "{want:?} is not in {conflict:?}");
    }
}

#[test]
fn a_face_and_a_height_limit_on_that_face_are_named() {
    let extra = "    .r1.height &= 9mm
    .r1.place.side &= \"top\"
    .lid: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 40mm
        .height &= 30mm
        .layers &= \"top\"
        .max_height &= 3mm
    }
";
    let report = report(2, (40, 30), extra);
    assert_eq!(report.conflicts.len(), 1, "{report}");
    let conflict = &report.conflicts[0];
    for want in [
        "widget.physical.nsm:11 `.r1.place.side &= \"top\"`",
        "they keep `r1` (R1) on the top face",
        "under the keepout `lid` (x 0 to 40, y 0 to 30 mm) wherever it is put",
        "that keepout allows 3 mm on the top face while R1 stands 9 mm tall",
        // The lines leave R1 anywhere, and it is the outline that puts it under the lid.
        "this holds only inside the outline of 40 x 30 mm, where the placer keeps every courtyard 0.4 mm from the edge",
    ] {
        assert!(conflict.contains(want), "{want:?} is not in {conflict:?}");
    }
}

#[test]
fn a_height_limit_on_the_other_face_is_not_a_conflict() {
    let extra = "    .r1.height &= 9mm
    .r1.place.side &= \"bottom\"
    .lid: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 40mm
        .height &= 30mm
        .layers &= \"top\"
        .max_height &= 3mm
    }
";
    let (design, board) = place(2, "", (40, 30), extra);
    let board = board.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(board.check_placement(), vec![]);
    assert_eq!(
        board.explain_requirements(&design, &place_options(&design, None)),
        None
    );
}

#[test]
fn an_unmet_line_is_listed_without_a_cause_when_no_conflict_is_proved() {
    // Met by the placer, then moved by hand: the lines are consistent, so nothing is claimed.
    let extra = "    .r1.place.x >= 20mm
    .r1.place.x <= 30mm
";
    let (design, board) = place(2, "", (40, 30), extra);
    let mut board = board.unwrap_or_else(|e| panic!("{e}"));
    let options = place_options(&design, None);
    assert_eq!(board.explain_requirements(&design, &options), None);
    let origin = board.outline.min;
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R1")
        .unwrap()
        .at = Pt::new(origin.x + 5.0, origin.y + 5.0);
    let report = board
        .explain_requirements(&design, &options)
        .expect("a report");
    assert_eq!(report.conflicts, Vec::<String>::new());
    assert_eq!(report.unmet.len(), 1, "{report}");
    let unmet = &report.unmet[0];
    let want = "widget.physical.nsm:10 `.r1.place.x >= 20mm` and widget.physical.nsm:11 `.r1.place.x <= 30mm`: the centre of the body of `r1` (R1) (F.Fab) is 5 mm, and it allows 20 to 30 mm (15 mm out)";
    assert!(unmet.contains(want), "{want:?} is not in {unmet:?}");
    let text = report.to_string();
    assert!(
        text.starts_with("1 requirement(s) of the physical file are not met where the placer ended; no two lines could be proved to conflict, so no cause is named:\n    "),
        "{text}"
    );
}

#[test]
fn a_design_without_requirements_has_nothing_to_explain() {
    let (design, board) = place(2, "", (40, 30), "");
    let board = board.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        board.explain_requirements(&design, &place_options(&design, None)),
        None
    );
}

/// Two headers whose windows put them over each other on a 14 x 9 mm board.
const HEADERS: &str = "    .h1: Header1x4
    .h2: Header1x4
    .h1.p1 ~ .rail.neg
    .h2.p1 ~ .rail.neg
";
const HEADER_LINES: &str = "    .h1.place.x &= 7mm +/- 0.1mm
    .h1.place.y &= 4mm +/- 0.1mm
    .h2.place.x &= 7mm +/- 0.1mm
    .h2.place.y &= 4.3mm +/- 0.1mm
";

#[test]
fn a_refusal_that_the_parts_do_not_fit_carries_the_lines_that_conflict() {
    let (_, board) = place(1, HEADERS, (14, 9), HEADER_LINES);
    let Err(BoardError::DoesNotFit(error)) = board else {
        panic!("the headers cannot both be there");
    };
    let text = error.to_string();
    for want in [
        "2 part(s) do not fit the outline of 14.0 x 9.0 mm",
        "\n  no placement meets every requirement of the physical file:\n    these lines cannot both hold: ",
        "widget.physical.nsm:10 `.h1.place.x &= 7mm +/- 0.1mm`",
        "widget.physical.nsm:13 `.h2.place.y &= 4.3mm +/- 0.1mm`",
        "the courtyard of `h1` (J1) within x 1.285 to 12.715, y -1.715 to 9.715 mm and that of `h2` (J2) within",
        "the two overlap by at least 3.4 x 3.1 mm wherever they are put",
    ] {
        assert!(text.contains(want), "{want:?} is not in {text}");
    }
    // `placement-failure.txt` is written from the same text.
    assert!(
        error
            .full_report()
            .contains("these lines cannot both hold: ")
    );
}

#[test]
fn a_part_too_big_for_the_outline_is_the_outlines_fault_and_not_a_line_s() {
    // The header is bigger than the board in every turn: the refusal says so, before placing,
    // and the line that only asks for a place is not blamed.
    let (_, board) = place(1, HEADERS, (12, 12), "    .h1.place.y >= 2mm\n");
    let Err(BoardError::Unplaceable(found)) = board else {
        panic!("a header does not fit 12 x 12 mm");
    };
    assert_eq!(found.conflicts, Vec::<String>::new());
    // Both headers of the board are bigger than the outline; neither is a line's fault.
    assert_eq!(found.faults.len(), 2, "{found:?}");
    let text = &found.faults[0].text;
    assert!(
        text.contains("a part is bigger in every turn it may take: `h1` (J1) (Header1x4)")
            && text.contains("`h1` (J1) (Header1x4) has a courtyard of"),
        "{text}"
    );
}

#[test]
fn a_line_that_puts_a_part_off_the_board_is_named_with_the_outline() {
    // The header fits the 16 x 6 mm board when it is turned a quarter, but not at the height
    // the lines ask for: the lines and the outline cannot both hold.
    let (design, board) = place(
        1,
        HEADERS,
        (16, 6),
        "    .h1.place.x &= 7mm +/- 0.1mm\n    .h1.place.y &= 4mm +/- 0.1mm\n",
    );
    let report = match board {
        Ok(board) => board.explain_requirements(&design, &place_options(&design, None)),
        Err(BoardError::DoesNotFit(error)) => error.requirements.map(|r| *r),
        Err(e) => panic!("{e}"),
    }
    .expect("a report");
    assert_eq!(report.conflicts.len(), 1, "{report}");
    let conflict = &report.conflicts[0];
    for want in [
        "widget.physical.nsm:10 `.h1.place.x &= 7mm +/- 0.1mm`; widget.physical.nsm:11 `.h1.place.y &= 4mm +/- 0.1mm`",
        "no place meets them and keeps the courtyard of `h1` (J1) inside the outline of 16 x 6 mm and 0.4 mm from its edge",
        "turned 90 degrees the lines leave the origin of its footprint in x 3.085 to 3.285, y 3.9 to 4.1 mm and the outline leaves it in x 2.2 to 6.17, y 2.2 to 3.8 mm",
    ] {
        assert!(conflict.contains(want), "{want:?} is not in {conflict:?}");
    }
}

#[test]
fn each_pair_of_holes_and_each_pair_of_courtyards_is_reported_once() {
    // Two headers a millimetre apart: their holes overlap, in both directions.
    let (design, board) = place(0, HEADERS, (40, 30), "");
    let mut board = board.unwrap_or_else(|e| panic!("{e}"));
    let origin = board.outline.min;
    for (refdes, at) in [("J1", (10.0, 10.0)), ("J2", (11.0, 10.5))] {
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == refdes)
            .unwrap()
            .at = Pt::new(origin.x + at.0, origin.y + at.1);
    }
    let _ = design;
    let lines: Vec<String> = board
        .check_placement()
        .iter()
        .map(ToString::to_string)
        .collect();
    let mut once = lines.clone();
    once.sort();
    once.dedup();
    assert_eq!(once.len(), lines.len(), "a line twice in {lines:#?}");
    let overlaps = lines.iter().filter(|l| l.contains("courtyards of")).count();
    assert_eq!(overlaps, 1, "{lines:#?}");
    // A hole to hole distance is the same from either part: it is listed for J1 only.
    let to_hole: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("from the hole of"))
        .collect();
    assert!(!to_hole.is_empty(), "{lines:#?}");
    assert!(
        to_hole
            .iter()
            .all(|l| l.starts_with("a hole of `h1` (J1) (pad ")),
        "{to_hole:#?}"
    );
    // Each hole is named by its pad, so two pairs with the same numbers are told apart.
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("a hole of `h1` (J1) (pad 1) is ")
                && l.contains("hole of `h2` (J2) (pad 1),")),
        "{lines:#?}"
    );
    // The distance from a hole to the other part's copper is not the same both ways: it is
    // listed for each.
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("a hole of `h1` (J1) ")
                && l.contains("copper of `h2` (J2) (pad "))
    );
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("a hole of `h2` (J2) ")
                && l.contains("copper of `h1` (J1) (pad "))
    );
}

/// `fab` on the board with `extra`, into a temporary folder.
fn fab(
    name: &str,
    parts: usize,
    headers: &str,
    size: (u32, u32),
    extra: &str,
    warn: bool,
) -> (TempDir, Result<Vec<Fabricated>, String>) {
    let dir = TempDir::unused(&format!("conflicts-{name}"));
    let mut catalogs = Catalogs::jlcpcb();
    if warn {
        catalogs = catalogs.with_constraints_warn(true);
    }
    let made = fab_with_options(
        "widget.nsm",
        &source(parts, headers),
        &loader(size.0, size.1, extra),
        &dir.join("fab"),
        &catalogs,
        &Libraries::sample(),
        None,
        &FabOptions::default(),
    );
    (dir, made)
}

#[test]
fn fab_names_the_lines_that_conflict_before_placing_anything() {
    let (dir, made) = fab("relation", 2, "", (40, 30), PITCH_AND_WINDOWS, false);
    let error = made.expect_err("no placement meets the lines");
    // One error, the lines first, and no placer ran: there is no arrangement to describe.
    assert!(
        error.starts_with("error: Widget: no placement meets every requirement of the physical file:\n  these lines cannot both hold: "),
        "{error}"
    );
    for want in [PITCH, R1_LOW, R2_HIGH] {
        assert!(error.contains(want), "{want:?} is not in {error}");
    }
    assert!(!error.contains("where the placer ended"), "{error}");
    assert!(
        !dir.join("fab").join("placement-failure.txt").exists(),
        "the placer did not run, so there is no list of where it ended"
    );
}

#[test]
fn fab_refuses_a_part_pinned_in_a_keepout_before_placing() {
    let extra = "    .r1.place.x &= 20mm
    .r1.place.y &= 10mm
    .blocked: Keepout {
        .x &= 15mm
        .y &= 5mm
        .width &= 10mm
        .height &= 10mm
    }
";
    let (_dir, made) = fab("keepout", 2, "", (40, 30), extra, false);
    let error = made.expect_err("R1 is in the keepout");
    assert!(
        error.starts_with("error: Widget: no placement meets every requirement"),
        "{error}"
    );
    assert!(
        error
            .contains("these lines cannot both hold: widget.physical.nsm:10 `.r1.place.x &= 20mm`"),
        "{error}"
    );
    // The fault of an arrangement is not there: nothing was placed.
    assert!(
        !error.contains("R1 overlaps the keepout `blocked`"),
        "{error}"
    );
}

#[test]
fn fab_names_headers_whose_windows_overlap_before_placing() {
    let (dir, made) = fab("headers", 1, HEADERS, (14, 9), HEADER_LINES, false);
    let error = made.expect_err("the headers cannot both be there");
    assert!(error.contains("these lines cannot both hold: "), "{error}");
    assert!(
        error.contains("widget.physical.nsm:10 `.h1.place.x &= 7mm +/- 0.1mm`"),
        "{error}"
    );
    assert!(!dir.join("fab").join("placement-failure.txt").exists());
}

#[test]
fn constraints_as_warnings_do_not_stop_fab_at_the_lines_before_routing() {
    // `--constraints warn`: the lines that are not met are warnings, listed by the check
    // after the routing as ever; the early refusal is for errors only.
    let (_dir, made) = fab("warn", 2, "", (40, 30), PITCH_AND_WINDOWS, true);
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    assert!(
        made[0].notes.contains(
            "warning [constraint] constraint r2.place.x - r1.place.x (widget.physical.nsm:12) is violated"
        ),
        "{}",
        made[0].notes
    );
}
