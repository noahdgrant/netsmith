//! A part with no room among the parts that stay where they are is said before any placing
//! (issue #1283): a warning of `board check`, `board build`, `layout check` and `fab`, an error
//! with `--strict`, naming the part and the largest free rectangle. Every board here is
//! invented: two 4-pin headers, the first pinned in the middle of an 8 by 14 mm outline by the
//! physical file, the second kept on the top and placed by the placer, on the sample
//! libraries (no KiCad is needed).

use std::cell::Cell;

use netsmith_check::Loader;
use netsmith_fabricate::fab::{FabOptions, fab_with_options, precheck_board};
use netsmith_fabricate::layout_edit::{Severity, check};
use netsmith_pcb::{parts_without_room, place_options};
use netsmith_pipeline::{Built, Libraries, build_with};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const SOURCE: &str = "import std::parts::connector::Header1x4
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
    .h1: Header1x4
    .h1.p1 ~ .rail.neg
    .h2: Header1x4
    .h2.p1 ~ .rail.neg
}
";

/// The outline of `width` by 14 mm, `h1` pinned with its body's centre in the middle, and `h2`
/// kept on the top.
fn physical(width: u32) -> String {
    format!(
        "import std::pcb::Outline

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= {width}mm
            .height &= 14mm
        }}
    }}
    .h1.place.x &= {}mm
    .h1.place.y &= 7mm
    .h1.place.rotation &= 0
    .h1.place.side &= \"top\"
    .h2.place.side &= \"top\"
}}
",
        f64::from(width) / 2.0
    )
}

fn loader(width: u32) -> Loader {
    Loader::std().with_physical("widget.nsm", "widget.physical.nsm", &physical(width))
}

fn built(width: u32) -> Built {
    build_with(
        "widget.nsm",
        SOURCE,
        &loader(width),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
}

#[test]
fn a_header_beside_a_pinned_one_on_a_narrow_board_has_no_room_and_is_named() {
    let b = built(8);
    let libraries = Libraries::sample();
    let room = parts_without_room(
        &b.design,
        &libraries.mapping,
        &libraries.footprints,
        &place_options(&b.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    // The pinned header stays where it is: the other is the one part measured.
    assert_eq!(room.measured, 1, "{room:?}");
    assert_eq!(room.no_room.len(), 1, "{room:?}");
    let h2 = &room.no_room[0];
    assert_eq!((h2.path.as_str(), h2.refdes.as_str()), ("h2", "J2"));
    let faces: Vec<&str> = h2.faces.iter().map(|f| f.face).collect();
    assert_eq!(faces, ["top"], "a requirement keeps it on the top");
    let largest = h2.faces[0].largest.expect("a strip beside the header is free");
    assert!(
        largest.max[0] - largest.min[0] < h2.courtyard[0].min(h2.courtyard[1]),
        "{largest:?} holds {h2:?}"
    );
    let text = h2.to_string();
    for want in [
        "`h2` (J2) has no room among the parts that stay where they are",
        "turned 0, 90 degrees",
        "the largest free rectangle is ",
        " on the top",
    ] {
        assert!(text.contains(want), "{want:?} is not in {text}");
    }
}

#[test]
fn board_check_warns_of_it_and_is_refused_by_it_when_strict() {
    let b = built(8);
    let libraries = Libraries::sample();
    let found = precheck_board(&b, None, &libraries, false, false);
    assert!(found.errors.is_empty(), "{found:?}");
    assert!(
        found.warnings.iter().any(|w| w.contains("`h2` (J2) has no room")),
        "{found:?}"
    );
    assert_eq!(found.place_warnings().len(), found.warnings.len());
    let strict = precheck_board(&b, None, &libraries, false, true);
    assert_eq!(strict.errors.len(), 1, "{strict:?}");
    assert!(
        strict.errors[0].contains("`h2` (J2) has no room")
            && strict.errors[0].contains("because the run is strict"),
        "{strict:?}"
    );
    assert_eq!(strict.error_proofs.len(), strict.errors.len());
}

#[test]
fn a_board_with_room_beside_the_pinned_header_says_nothing() {
    let b = built(14);
    let libraries = Libraries::sample();
    let found = precheck_board(&b, None, &libraries, false, true);
    assert!(found.errors.is_empty(), "{found:?}");
    assert!(
        !found.warnings.iter().any(|w| w.contains("has no room")),
        "{found:?}"
    );
}

#[test]
fn layout_check_says_it_as_a_warning_and_an_error_when_strict() {
    let b = built(8);
    let libraries = Libraries::sample();
    let dir = TempDir::new("no-room-layout");
    let file = dir.join("widget.pcb.toml");
    let checked = check(&b.design, &file, &libraries, false).unwrap_or_else(|e| panic!("{e}"));
    let finding = checked
        .findings
        .iter()
        .find(|f| f.kind == "no_room")
        .unwrap_or_else(|| panic!("no `no_room`: {checked:?}"));
    assert_eq!(finding.severity, Severity::Warning);
    assert_eq!(finding.path.as_deref(), Some("h2"));
    assert!(finding.message.contains("the largest free rectangle is"), "{finding:?}");
    assert!(checked.ok, "a warning leaves the file ok: {checked:?}");
    let strict = check(&b.design, &file, &libraries, true).unwrap_or_else(|e| panic!("{e}"));
    let finding = strict.findings.iter().find(|f| f.kind == "no_room").unwrap();
    assert_eq!(finding.severity, Severity::Error);
    assert!(!strict.ok);
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

#[test]
fn strict_fab_stops_at_it_before_the_placer_starts() {
    let dir = TempDir::new("no-room-fab");
    PLACING.with(|p| p.set(false));
    let made = fab_with_options(
        "widget.nsm",
        SOURCE,
        &loader(8),
        &dir.join("fab"),
        &Catalogs::jlcpcb().with_strict_guidelines(true),
        &Libraries::sample(),
        None,
        &FabOptions {
            progress: Some(watch),
            ..FabOptions::default()
        },
    );
    let error = made.expect_err("h2 has no room");
    assert!(error.contains("`h2` (J2) has no room"), "{error}");
    assert!(!PLACING.with(Cell::get), "the placer was started: {error}");
}
