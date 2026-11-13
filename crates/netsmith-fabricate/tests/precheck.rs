//! The requirements are proved before the placer starts (ADR 0045 section 14):
//! lines that cannot all hold, a courtyard past an edge nobody stated, a part bigger than the
//! outline in every turn. `fab` and `build` say so at once, naming `file:line`, with no
//! placement attempt; a board the placer can place is not refused. Every board here is
//! invented: a 40 by 30 mm board with resistors and headers, on the sample libraries (no
//! KiCad is needed).
//!
//! The case where a part states a face and its module another is refused by the checker when
//! the board is built, also before any placing; its test is in `enclosure_sensor.rs`.

use std::cell::Cell;

use netsmith_fabricate::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith_pipeline::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_pcb::{Board, BoardError, Precheck, place_options};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

/// A board of `parts` resistors and `headers`, in a 40 x 30 mm outline unless `size` says.
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

const HEADER: &str = "    .h1: Header1x4
    .h1.p1 ~ .rail.neg
";

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

fn loader(size: (u32, u32), extra: &str) -> Loader {
    Loader::std().with_physical(
        "widget.nsm",
        "widget.physical.nsm",
        &physical(size.0, size.1, extra),
    )
}

/// What the pre-check says of the board, or the error that stops it earlier.
fn precheck(parts: usize, headers: &str, size: (u32, u32), extra: &str) -> Precheck {
    let libraries = Libraries::sample();
    let built = build_with(
        "widget.nsm",
        &source(parts, headers),
        &loader(size, extra),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let design = &built[0].design;
    netsmith_placer::precheck(
        design,
        &libraries.mapping,
        &libraries.footprints,
        &place_options(design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Place the board with the placer, as `fab` does after the pre-check.
fn place(parts: usize, headers: &str, size: (u32, u32), extra: &str) -> Result<Board, BoardError> {
    let libraries = Libraries::sample();
    let built = build_with(
        "widget.nsm",
        &source(parts, headers),
        &loader(size, extra),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let design = &built[0].design;
    netsmith_placer::board_from_design_placed(
        design,
        &libraries.mapping,
        &libraries.footprints,
        &place_options(design, None),
    )
}

thread_local! {
    /// Set when `fab` says, on this thread, that it has started to place.
    static PLACING: Cell<bool> = const { Cell::new(false) };
}

fn watch(message: &str) {
    if message.contains(": placing ") {
        PLACING.with(|p| p.set(true));
    }
}

/// `fab` on the board, with whether the placer was started.
fn fab(
    name: &str,
    parts: usize,
    headers: &str,
    size: (u32, u32),
    extra: &str,
    warn: bool,
) -> (TempDir, Result<Vec<Fabricated>, String>, bool) {
    let dir = TempDir::unused(&format!("precheck-{name}"));
    let mut catalogs = Catalogs::jlcpcb();
    if warn {
        catalogs = catalogs.with_constraints_warn(true);
    }
    PLACING.with(|p| p.set(false));
    let made = fab_with_options(
        "widget.nsm",
        &source(parts, headers),
        &loader(size, extra),
        &dir.join("fab"),
        &catalogs,
        &Libraries::sample(),
        None,
        &FabOptions {
            progress: Some(watch),
            ..FabOptions::default()
        },
    );
    (dir, made, PLACING.with(Cell::get))
}

// ---- (1) a part pinned inside a keepout that forbids parts ----

const PINNED_IN_KEEPOUT: &str = "    .r1.place.x &= 20mm
    .r1.place.y &= 10mm
    .blocked: Keepout {
        .x &= 15mm
        .y &= 5mm
        .width &= 10mm
        .height &= 10mm
    }
";

#[test]
fn a_part_pinned_inside_a_keepout_is_proved_before_placing() {
    let found = precheck(2, "", (40, 30), PINNED_IN_KEEPOUT);
    assert_eq!(found.conflicts.len(), 1, "{found:?}");
    for want in [
        "widget.physical.nsm:10 `.r1.place.x &= 20mm`",
        "widget.physical.nsm:11 `.r1.place.y &= 10mm`",
        "the keepout `blocked` (x 15 to 25, y 5 to 15 mm, which forbids parts)",
        "by at least 0.94 x 0.94 mm wherever it is put",
    ] {
        assert!(found.conflicts[0].contains(want), "{want:?}: {found:?}");
    }
    assert!(found.faults.is_empty(), "{found:?}");
    // The placer is not asked: `from_design_placed` refuses with the same lines.
    let Err(BoardError::Unplaceable(refused)) = place(2, "", (40, 30), PINNED_IN_KEEPOUT) else {
        panic!("the board must be refused before placing");
    };
    assert_eq!(*refused, found);
}

#[test]
fn fab_refuses_a_part_pinned_inside_a_keepout_without_placing() {
    let (dir, made, placing) = fab("keepout", 2, "", (40, 30), PINNED_IN_KEEPOUT, false);
    let error = made.expect_err("R1 is pinned inside the keepout");
    assert!(
        error.starts_with("error: Widget: no placement meets every requirement of the physical file:\n  these lines cannot both hold: widget.physical.nsm:10 `.r1.place.x &= 20mm`"),
        "{error}"
    );
    assert!(!placing, "the placer was started: {error}");
    assert!(!dir.join("fab").join("placement-failure.txt").exists());
}

#[test]
fn a_part_pinned_just_outside_the_keepout_is_not_refused() {
    // The courtyard of a resistor reaches 0.93 mm from its centre: at x = 26 mm it ends
    // 0.07 mm from the keepout, which is a near miss and a board the placer can place.
    let extra = PINNED_IN_KEEPOUT.replace("20mm", "26mm");
    let found = precheck(2, "", (40, 30), &extra);
    assert_eq!(found.conflicts, Vec::<String>::new());
    assert!(found.faults.is_empty() && !found.is_error(), "{found:?}");
    place(2, "", (40, 30), &extra).unwrap_or_else(|e| panic!("{e}"));
}

// ---- (2) a courtyard past an edge nobody allowed ----

/// A header 11.2 mm long on a board 10 mm wide, turned the long way across it and put at the
/// middle: its courtyard is past the left and the right edge.
const WIDE_PINNED: &str = "    .h1.place.rotation &= 90
    .h1.place.x &= 5mm
    .h1.place.y &= 15mm
";

#[test]
fn a_courtyard_past_two_edges_nobody_stated_is_refused_naming_the_edges() {
    let found = precheck(0, HEADER, (10, 30), WIDE_PINNED);
    assert_eq!(found.conflicts, Vec::<String>::new(), "{found:?}");
    assert_eq!(found.faults.len(), 1, "{found:?}");
    let text = &found.faults[0].text;
    for want in [
        "`h1` (J1) is not wholly inside the board outline: its courtyard is ",
        " mm past the left edge and ",
        " mm past the right edge, and the design does not say it may hang over an edge",
        "widget.physical.nsm:",
    ] {
        assert!(text.contains(want), "{want:?} is not in {text}");
    }
    // Whichever the body does, the message says what would allow it.
    assert!(
        text.contains("to allow it past the left edge, require the body to reach it, for example `.h1.place.left <= 0mm`")
            || text.contains("no statement allows only the courtyard past the left edge")
                && text.contains("issue #99"),
        "{text}"
    );
    assert_eq!(found.faults[0].refdes, "J1");
}

#[test]
fn fab_refuses_the_courtyard_past_an_edge_without_placing() {
    let (_dir, made, placing) = fab("edges", 0, HEADER, (10, 30), WIDE_PINNED, false);
    let error = made.expect_err("the header is past both edges");
    assert!(
        error.starts_with(
            "error: Widget: `h1` (J1) is not wholly inside the board outline: its courtyard is "
        ),
        "{error}"
    );
    assert!(!placing, "the placer was started: {error}");
}

#[test]
fn a_courtyard_past_an_edge_the_lines_state_is_not_refused() {
    // The same header with both overhangs stated by its body's edge fields.
    let extra =
        format!("{WIDE_PINNED}    .h1.place.left <= 0mm\n    .h1.place.right >= .outline.width\n");
    let found = precheck(0, HEADER, (10, 30), &extra);
    assert!(found.faults.is_empty(), "{found:?}");
}

#[test]
fn a_part_whose_window_leaves_a_place_inside_the_edges_is_not_refused() {
    // Only the x window is given, wide enough for the header to sit inside: nothing is
    // proved against it, and the placer can place it.
    let found = precheck(
        0,
        HEADER,
        (40, 30),
        "    .h1.place.x >= 10mm\n    .h1.place.x <= 30mm\n",
    );
    assert!(!found.is_error(), "{found:?}");
    place(
        0,
        HEADER,
        (40, 30),
        "    .h1.place.x >= 10mm\n    .h1.place.x <= 30mm\n",
    )
    .unwrap_or_else(|e| panic!("{e}"));
}

// ---- (4) a window outside the outline ----

#[test]
fn a_window_that_has_no_place_inside_the_outline_is_refused_once() {
    // The centre may be anywhere from 39.8 mm: the courtyard is then past the right edge.
    let found = precheck(2, "", (40, 30), "    .r1.place.x >= 39.8mm\n");
    // Said once, by the message that names the edge and what would allow it.
    assert_eq!(found.conflicts, Vec::<String>::new(), "{found:?}");
    assert_eq!(found.faults.len(), 1, "{found:?}");
    let text = &found.faults[0].text;
    for want in [
        "`r1` (R1) is not wholly inside the board outline: its courtyard is at least ",
        " mm past the right edge wherever its requirements put it",
        "for example `.r1.place.right >= outline.width`",
        "widget.physical.nsm:10 `.r1.place.x >= 39.8mm`",
    ] {
        assert!(text.contains(want), "{want:?} is not in {text}");
    }
}

// ---- (5) two relations of the same parts with no common value ----

#[test]
fn two_relations_of_the_same_parts_with_no_common_value_are_refused() {
    let extra = "    .r2.place.x - .r1.place.x >= 20mm
    .r2.place.x - .r1.place.x <= 18mm
";
    let found = precheck(2, "", (40, 30), extra);
    assert_eq!(found.conflicts.len(), 1, "{found:?}");
    assert!(
        found.conflicts[0].contains("widget.physical.nsm:10 `.r2.place.x - .r1.place.x >= 20mm`; widget.physical.nsm:11 `.r2.place.x - .r1.place.x <= 18mm`")
            && found.conflicts[0].contains("and no value is in both (2 mm apart)"),
        "{found:?}"
    );
    // Two relations that have a value in common are fine.
    let fine = "    .r2.place.x - .r1.place.x >= 10mm
    .r2.place.x - .r1.place.x <= 18mm
";
    assert!(!precheck(2, "", (40, 30), fine).is_error());
    place(2, "", (40, 30), fine).unwrap_or_else(|e| panic!("{e}"));
}

// ---- (6) a stated face against a height limit on that face ----

#[test]
fn a_stated_face_against_a_height_limit_on_that_face_is_refused() {
    let lid = |face: &str| {
        format!(
            "    .r1.height &= 9mm
    .r1.place.side &= \"{face}\"
    .lid: Keepout {{
        .x &= 0mm
        .y &= 0mm
        .width &= 40mm
        .height &= 30mm
        .layers &= \"top\"
        .max_height &= 3mm
    }}
"
        )
    };
    let found = precheck(2, "", (40, 30), &lid("top"));
    assert_eq!(found.conflicts.len(), 1, "{found:?}");
    assert!(
        found.conflicts[0]
            .contains("that keepout allows 3 mm on the top face while R1 stands 9 mm tall"),
        "{found:?}"
    );
    // The same part on the other face, which the lid does not limit, is a board.
    assert!(!precheck(2, "", (40, 30), &lid("bottom")).is_error());
}

// ---- (7) a part bigger than the outline in every turn ----

#[test]
fn a_part_bigger_than_the_outline_in_every_turn_is_refused_before_placing() {
    let found = precheck(0, HEADER, (12, 12), "");
    assert_eq!(found.faults.len(), 1, "{found:?}");
    let text = &found.faults[0].text;
    for want in [
        "the outline is 12.0 x 12.0 mm, which leaves 11.2 x 11.2 mm for a courtyard inside its 0.4 mm margin, and a part is bigger in every turn it may take: `h1` (J1) (Header1x4)",
        "`h1` (J1) (Header1x4) has a courtyard of ",
        "turned 90 degrees",
    ] {
        assert!(text.contains(want), "{want:?} is not in {text}");
    }
    let (_dir, made, placing) = fab("oversize", 0, HEADER, (12, 12), "", false);
    assert!(made.is_err());
    assert!(!placing);
}

#[test]
fn a_part_that_fits_the_outline_turned_a_quarter_is_not_refused() {
    // 16 x 6 mm: the header is too long for the 6 mm side, and fits turned.
    let found = precheck(0, HEADER, (16, 6), "");
    assert!(!found.is_error(), "{found:?}");
    place(0, HEADER, (16, 6), "").unwrap_or_else(|e| panic!("{e}"));
}

// ---- constraints as warnings, and the warnings ----

#[test]
fn lines_that_cannot_hold_are_warnings_when_constraints_are_warnings_for_the_run() {
    let (_dir, made, placing) = fab("warn", 2, "", (40, 30), PINNED_IN_KEEPOUT, true);
    // The placer is asked, as before. Whatever it makes of the board, the pre-check did not
    // stop it: an error is the report after placing, which names the list of where it ended.
    assert!(placing, "constraints are warnings, so the placer runs");
    if let Err(error) = made {
        assert!(error.contains("placement-failure.txt"), "{error}");
    }
}

#[test]
fn a_courtyard_pinned_within_the_margin_of_an_edge_is_a_conflict_and_a_near_miss_is_not() {
    // A resistor whose centre is 0.6 mm from the left edge: the courtyard (0.47 mm either
    // side) is inside the board, closer than the 0.4 mm the placer keeps clear. The 98 proof
    // says so once; a centre 0.9 mm from the edge leaves 0.43 mm and is a board.
    let found = precheck(
        2,
        "",
        (40, 30),
        "    .r1.place.x &= 0.6mm\n    .r1.place.y &= 15mm\n",
    );
    assert_eq!(found.conflicts.len(), 1, "{found:?}");
    assert!(
        found.conflicts[0].contains("and 0.4 mm from its edge"),
        "{found:?}"
    );
    assert!(found.faults.is_empty(), "{found:?}");
    let near = "    .r1.place.x &= 0.9mm\n    .r1.place.y &= 15mm\n";
    assert!(!precheck(2, "", (40, 30), near).is_error());
    place(2, "", (40, 30), near).unwrap_or_else(|e| panic!("{e}"));
}

// ---- cheap ----

/// The CPU time this thread has used: the time it ran, not the time that passed. (For
/// [`measured`] only; the pre-check is not asserted to take any time.)
fn thread_cpu_time() -> std::time::Duration {
    let t = rustix::time::clock_gettime(rustix::time::ClockId::ThreadCPUTime);
    std::time::Duration::new(t.tv_sec as u64, t.tv_nsec as u32)
}

/// The pre-check of a board of `parts` resistors that each have a window (so every part has its
/// pairs and keepouts to try), and the CPU time it took.
fn precheck_of(parts: usize) -> (Precheck, std::time::Duration) {
    let mut lines = String::from(
        "    .keep: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 5mm
        .height &= 5mm
    }
",
    );
    for n in 1..=parts {
        let (col, row) = ((n - 1) % 10, (n - 1) / 10);
        lines += &format!(
            "    .r{n}.place.x &= {}mm +/- 0.1mm\n    .r{n}.place.y &= {}mm +/- 0.1mm\n",
            8 + col * 5,
            8 + row * 4
        );
    }
    let libraries = Libraries::sample();
    let built = build_with(
        "widget.nsm",
        &source(parts, ""),
        &loader((70, 60), &lines),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let design = &built[0].design;
    let options = place_options(design, None);
    let started = thread_cpu_time();
    let found =
        netsmith_placer::precheck(design, &libraries.mapping, &libraries.footprints, &options)
            .unwrap_or_else(|e| panic!("{e}"));
    (found, thread_cpu_time() - started)
}

/// The pre-check of a board of a hundred parts that each have a window finds nothing wrong with
/// windows that do not meet. How long it takes is not asserted, because no test passes or fails by
/// the speed of the machine, and no count of its work can stand for the time: the pairs of parts it
/// compares are every pair, by design, and cheap (600 for 25 parts, 9,900 for 100). `measured` prints
/// the CPU time of both sizes.
#[test]
fn the_precheck_of_a_board_of_a_hundred_parts_finds_nothing_wrong_with_windows_that_do_not_meet() {
    let (found, _) = precheck_of(100);
    assert!(!found.is_error(), "{found:?}");
    assert!(found.warnings.is_empty(), "{found:?}");
}

/// Prints the CPU time of the pre-check of 25 and of 100 parts, and their ratio (about 4.5 when
/// the pre-check grows with the parts, as it did when this was measured on a 10-core machine;
/// 16 would be the square): `cargo test -p netsmith-fabricate --test main precheck::measured -- --ignored
/// --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn measured() {
    let best = |parts| {
        let _ = precheck_of(parts);
        (0..7).map(|_| precheck_of(parts).1).min().unwrap()
    };
    let (small, big) = (best(25), best(100));
    eprintln!(
        "pre-check of 25 parts: {small:?}, of 100 parts: {big:?}, ratio {:.1}",
        big.as_secs_f64() / small.as_secs_f64()
    );
}
