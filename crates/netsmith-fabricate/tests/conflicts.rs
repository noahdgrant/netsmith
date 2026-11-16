//! When no placement meets the requirements of a physical file, the refusal names the lines
//! (ADR 0045): every requirement not met where the placer ended, with its
//! `file:line`, the text of the line, the part, the measured value, the window and how far
//! out; and the lines that cannot all hold, where that is proved from the windows the lines
//! leave, the keepouts and cutouts and the outline. A conflict that cannot be proved
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
    // Issue #998: each line is data with the window it allows.
    let lines = &report.proofs[0].lines;
    let windows: Vec<_> = lines
        .iter()
        .map(|l| (l.measure, l.unit, l.min, l.max))
        .collect();
    assert_eq!(
        windows,
        [
            (Some("relation_x"), Some("mm"), Some(20.0), None),
            (Some("relation_x"), Some("mm"), None, Some(18.0)),
        ],
        "{lines:?}"
    );
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
    // Issue #998: a bound on a field is data with its window.
    let x = report.proofs[0]
        .lines
        .iter()
        .find(|l| l.id.as_deref() == Some("r2.place.x"))
        .expect("the line on r2's x");
    assert_eq!(
        (x.measure, x.unit, x.min, x.max),
        (Some("x"), Some("mm"), Some(10.2), Some(10.4))
    );
}

#[test]
fn a_face_and_a_height_limit_on_that_face_are_a_finding_and_no_conflict() {
    // R1 stands 9 mm tall on the top face, which a lid over the whole board limits to 3 mm:
    // no placement can meet that, so it is not a requirement the lines cannot all hold but a
    // part too tall, which the placement check names with the margin (issue #777).
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
    let (design, board) = place(2, "", (40, 30), extra);
    let board = board.unwrap_or_else(|e| panic!("{e}"));
    let tall: Vec<String> = board
        .check_placement()
        .iter()
        .filter(|p| matches!(p, PlacementProblem::TooTall { .. }))
        .map(ToString::to_string)
        .collect();
    assert_eq!(tall.len(), 1, "{tall:?}");
    for want in [
        "`r1` (R1) stands 9.00 mm tall on the top face",
        "the keepout `lid` allows 3.00 mm there (6.00 mm too tall)",
    ] {
        assert!(tall[0].contains(want), "{want:?} is not in {:?}", tall[0]);
    }
    let report = board.explain_requirements(&design, &place_options(&design, None));
    assert!(
        report.as_ref().is_none_or(|r| r.conflicts.is_empty()),
        "{report:?}"
    );
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
    // The same as data: the relation, with both parts and the axis (issue #816).
    let relation = made[0]
        .requirements
        .iter()
        .find(|r| r.id == "r2.place.x - r1.place.x")
        .unwrap_or_else(|| panic!("{:?}", made[0].requirements));
    assert_eq!(relation.measure, "relation_x");
    assert_eq!(relation.parts.len(), 2);
}

// The same requirements as data, for `--format json` (issue #816): each not met is a
// `RequirementMiss` beside its sentence, with the requirement, the value measured, the window it
// allows and how far out it is, one test for each kind.

fn json(miss: &netsmith_pcb::RequirementMiss) -> serde_json::Value {
    serde_json::to_value(miss).unwrap()
}

/// The board of two resistors with `extra` in its physical file, placed, and then `moved`.
fn moved(extra: &str, moved: impl FnOnce(&mut Board)) -> RequirementReport {
    let (design, board) = place(2, "", (40, 30), extra);
    let mut board = board.unwrap_or_else(|e| panic!("{e}"));
    let options = place_options(&design, None);
    assert_eq!(board.explain_requirements(&design, &options), None);
    moved(&mut board);
    board
        .explain_requirements(&design, &options)
        .expect("a report")
}

#[test]
fn a_field_of_a_part_not_met_carries_the_measure_the_window_and_the_miss() {
    let report = moved("    .r1.place.x >= 20mm\n    .r1.place.x <= 30mm\n", |board| {
        let origin = board.outline.min;
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == "R1")
            .unwrap()
            .at = Pt::new(origin.x + 5.0, origin.y + 5.0);
    });
    assert_eq!(report.misses.len(), report.unmet.len());
    let miss = &report.misses[0];
    assert_eq!(miss.text, report.unmet[0]);
    let json = json(miss);
    // Two lines state one requirement, so the source names both.
    assert_eq!(
        json,
        serde_json::json!({
            "id": "r1.place.x",
            "source": "widget.physical.nsm:10, widget.physical.nsm:11",
            "parts": ["R1"],
            "measure": "x",
            "unit": "mm",
            "measured": 5.0,
            "min": 20.0,
            "max": 30.0,
            "over": 15.0,
        })
    );
}

#[test]
fn an_open_end_of_a_window_is_left_out() {
    let report = moved("    .r1.place.right <= 30mm\n", |board| {
        let origin = board.outline.min;
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == "R1")
            .unwrap()
            .at = Pt::new(origin.x + 38.0, origin.y + 5.0);
    });
    let json = json(&report.misses[0]);
    assert_eq!(json["measure"], "right");
    assert_eq!(json["max"], 30.0);
    assert!(json.get("min").is_none(), "{json}");
    assert!(json["over"].as_f64().unwrap() > 7.0, "{json}");
}

#[test]
fn a_turn_not_met_is_in_degrees() {
    let report = moved("    .r1.place.rotation &= 90\n", |board| {
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == "R1")
            .unwrap()
            .rotation = 0.0;
    });
    assert_eq!(
        json(&report.misses[0]),
        serde_json::json!({
            "id": "r1.place.rotation",
            "source": "widget.physical.nsm:10",
            "parts": ["R1"],
            "measure": "rotation",
            "unit": "degrees",
            "measured": 0.0,
            "min": 90.0,
            "max": 90.0,
            "over": 90.0,
        })
    );
}

#[test]
fn a_face_not_met_says_both_faces() {
    let report = moved("    .r1.place.side &= \"bottom\"\n", |board| {
        board
            .parts
            .iter_mut()
            .find(|p| p.refdes == "R1")
            .unwrap()
            .side = netsmith_pcb::Side::Front;
    });
    assert_eq!(
        json(&report.misses[0]),
        serde_json::json!({
            "id": "r1.place.side",
            "source": "widget.physical.nsm:10",
            "parts": ["R1"],
            "measure": "side",
            "side": "top",
            "allowed_side": "bottom",
        })
    );
}

#[test]
fn a_relation_not_met_names_both_parts_and_the_axis() {
    let report = report(2, (40, 30), PITCH_AND_WINDOWS);
    let miss = &report.misses[0];
    assert_eq!(miss.text, report.unmet[0]);
    let json = json(miss);
    assert_eq!(json["id"], "r2.place.x - r1.place.x");
    assert_eq!(json["source"], "widget.physical.nsm:12");
    assert_eq!(json["parts"], serde_json::json!(["R2", "R1"]));
    assert_eq!(json["measure"], "relation_x");
    assert_eq!(json["min"], 29.9);
    assert_eq!(json["max"], 30.1);
    // Measured and out are what the sentence says.
    let measured = json["measured"].as_f64().unwrap();
    let over = json["over"].as_f64().unwrap();
    assert!((29.9 - measured).max(measured - 30.1) - over < 1e-3, "{json}");
}

#[test]
fn parts_that_do_not_fit_are_data_with_every_pair_and_every_part() {
    let (_, board) = place(1, HEADERS, (14, 9), HEADER_LINES);
    let Err(BoardError::DoesNotFit(error)) = board else {
        panic!("the headers cannot both be there");
    };
    let report = error.report();
    assert_eq!(report.outline.width, 14.0);
    assert_eq!(report.outline.height, 9.0);
    // Every part, as `placement-failure.txt` lists them, with its member path.
    assert_eq!(report.parts.len(), error.parts.len());
    assert!(report.parts.iter().any(|p| p.part.refdes == "J1" && p.part.path == "h1"));
    // The pair that overlaps is named with its area, and both parts with their paths.
    let ended = error.ended.as_ref().expect("the placer tried");
    assert_eq!(report.overlapping.len(), ended.overlapping.len());
    let pair = report
        .overlapping
        .iter()
        .find(|o| [o.a.refdes.as_str(), o.b.refdes.as_str()] == ["J1", "J2"] || [o.a.refdes.as_str(), o.b.refdes.as_str()] == ["J2", "J1"])
        .unwrap_or_else(|| panic!("{:?}", report.overlapping));
    assert!(pair.area > 0.0);
    assert!(["h1", "h2"].contains(&pair.a.path.as_str()));
    assert_eq!(report.outside.len(), ended.outside_parts.len());
    assert_eq!(report.tries, Some(ended.tries));
    // The lines that cannot both hold, as in the message.
    assert_eq!(report.conflicts.len(), 1);
    // As JSON: the keys a program reads.
    let json = serde_json::to_value(&report).unwrap();
    for key in [
        "outline",
        "needs",
        "margin",
        "courtyard_area",
        "available_area",
        "certain",
        "oversize",
        "overlapping",
        "outside",
        "relations",
        "conflicts",
        "parts",
    ] {
        assert!(json.get(key).is_some(), "{key} is not in {json}");
    }
    assert!(json["parts"][0].get("refdes").is_some());
}

// ---- the conflicts as data (issue #779) ----------------------------------------------------

#[test]
fn a_conflict_is_data_with_its_kind_lines_parts_and_how_far() {
    let report = report(2, (40, 30), PITCH_AND_WINDOWS);
    assert_eq!(report.proofs.len(), report.conflicts.len());
    let proof = &report.proofs[0];
    assert_eq!(proof.text, report.conflicts[0]);
    let json = serde_json::to_value(proof).unwrap();
    assert_eq!(json["kind"], "relation");
    // The relation first, then the windows of its parts, each with its line and text.
    let sources: Vec<&str> = json["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["source"].as_str().unwrap())
        .collect();
    assert_eq!(
        sources,
        [
            "widget.physical.nsm:12",
            "widget.physical.nsm:10",
            "widget.physical.nsm:11"
        ]
    );
    assert_eq!(json["lines"][0]["kind"], "requirement");
    assert_eq!(
        json["lines"][0]["text"],
        ".r2.place.x - .r1.place.x == 30mm +/- 0.1mm"
    );
    // The parts as the id reads them, each with the box its courtyard lies within.
    assert_eq!(json["parts"][0]["refdes"], "R2");
    assert_eq!(json["parts"][1]["path"], "r1");
    assert!(json["parts"][1]["courtyard"]["x_min"].as_f64().is_some(), "{json}");
    assert_eq!(json["by"], 24.9);
    // The sentence is the finding's message, not a field.
    assert!(json.get("text").is_none());
}

/// The headers of [`HEADERS`] locked by a layout file over each other, with a line of the
/// physical file on the resistor (so that the requirements are explained).
const LOCKED_OVER: &str = "version = 1

[Widget.parts.h1]
at = [7.0, 4.0]
lock = true

[Widget.parts.h2]
at = [7.0, 4.3]
lock = true
";

#[test]
fn locks_of_the_layout_file_that_overlap_are_a_conflict_with_the_locks_and_the_boxes() {
    let libraries = Libraries::sample();
    let built = build_with(
        "widget.nsm",
        &source(1, HEADERS),
        &loader(14, 9, "    .r1.place.x >= 1mm\n"),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let design = &built[0].design;
    let layout = (
        netsmith_pcb::Layout::parse(LOCKED_OVER, "widget.pcb.toml").unwrap(),
        "widget.pcb.toml".to_string(),
    );
    let placed = netsmith_fabricate::layout::place(
        design,
        Some(&layout),
        &libraries,
        false,
        netsmith_fabricate::layout::DEFAULT_PLACE_SEEDS,
    );
    // Placed or refused, the locks are proved not to hold, with the boxes and the overlap.
    let (board, options) = match placed {
        Ok(p) => (p.board, p.options),
        Err(failure) => {
            let ended = netsmith_fabricate::layout::where_ended(
                design,
                Some(&layout),
                &libraries,
                false,
                &failure,
            )
            .expect("the board where the placer ended");
            (ended.board, ended.options)
        }
    };
    let conflicts = board
        .explain_requirements(design, &options)
        .map(|r| r.proofs)
        .unwrap_or_default();
    let overlap = conflicts
        .iter()
        .find(|c| c.kind == "overlap")
        .unwrap_or_else(|| panic!("{conflicts:?}"));
    let locks: Vec<(&str, Option<f64>, Option<f64>)> = overlap
        .lines
        .iter()
        .filter(|l| l.kind == "lock")
        .map(|l| (l.part.as_ref().unwrap().refdes.as_str(), l.x, l.y))
        .collect();
    assert_eq!(
        locks,
        [("J1", Some(7.0), Some(4.0)), ("J2", Some(7.0), Some(4.3))]
    );
    assert_eq!(overlap.parts.len(), 2);
    assert!(overlap.parts.iter().all(|p| p.courtyard.is_some()));
    let size = overlap.overlap.as_ref().expect("the overlap");
    assert!(size.width > 0.0 && size.height > 0.0, "{size:?}");
    // The board has the layout file's places and the overlap the placement check finds.
    let j1 = board.part("J1").unwrap();
    assert_eq!(j1.at - board.outline.min, Pt::new(7.0, 4.0));
    let faults = board.placement_faults();
    let pair = faults
        .iter()
        .find(|f| {
            f.kind == "overlap"
                && f.parts.iter().any(|p| p.part.refdes == "J1")
                && f.parts.iter().any(|p| p.part.refdes == "J2")
        })
        .unwrap_or_else(|| panic!("{faults:?}"));
    assert!(pair.area.unwrap() > 0.0 && pair.shared.is_some(), "{pair:?}");
    let json = serde_json::to_value(pair).unwrap();
    assert!(json["parts"][0]["courtyard"]["x_min"].is_number(), "{json}");
}

#[test]
fn locks_that_break_a_requirement_give_the_board_with_the_locks_and_the_misses() {
    // The headers' lines of the physical file, and the layout file locks both lower than they
    // allow (as a drag in the editor does): the placer refuses the locks before placing, and the
    // board is shown with the parts where the file locks them and the requirements not met.
    let libraries = Libraries::sample();
    let built = build_with(
        "widget.nsm",
        &source(1, HEADERS),
        &loader(14, 9, HEADER_LINES),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let design = &built[0].design;
    let layout = (
        netsmith_pcb::Layout::parse(LOCKED_OVER, "widget.pcb.toml").unwrap(),
        "widget.pcb.toml".to_string(),
    );
    let failure = match netsmith_fabricate::layout::place(design, Some(&layout), &libraries, true, 1)
    {
        Ok(_) => panic!("the locks break the lines"),
        Err(failure) => failure,
    };
    assert!(
        failure.message().contains("is locked in the layout file at (7, 4) mm, and that breaks a requirement"),
        "{}",
        failure.message()
    );
    let ended =
        netsmith_fabricate::layout::where_ended(design, Some(&layout), &libraries, true, &failure)
            .expect("the board with the layout file's places");
    let at = |r: &str| {
        let at = ended.board.part(r).unwrap().at - ended.board.outline.min;
        ((at.x * 1e6).round() / 1e6, (at.y * 1e6).round() / 1e6)
    };
    assert_eq!((at("J1"), at("J2")), ((7.0, 4.0), (7.0, 4.3)));
    assert!(
        ended.board.placement_faults().iter().any(|f| f.kind == "overlap"),
        "{:?}",
        ended.board.check_placement()
    );
    // The lines the locks break, measured where the parts stand.
    let explained = ended.explain(design);
    let missed: Vec<&str> = explained.misses.iter().map(|m| m.id.as_str()).collect();
    assert!(missed.contains(&"h1.place.y") && missed.contains(&"h2.place.y"), "{missed:?}");
    // With no layout file the refusal is the design's: there is no board to show.
    assert!(
        netsmith_fabricate::layout::where_ended(design, None, &libraries, true, &failure)
            .is_none()
    );
}
