//! A soft margin round a part (issue 1310): `soft margin 3mm round .r1` in the physical file asks
//! the placer to keep other parts' courtyards 3 mm from the courtyard of `r1` where it can; a
//! margin it could not keep is a warning of `fab` and `layout check`, an error when strict.
//! Every board here is invented: three resistors on a rail, on a 30 by 20 mm board, on the sample
//! libraries (no KiCad is needed).

use std::cell::RefCell;

use netsmith_check::Loader;
use netsmith_fabricate::fab::{FabOptions, fab_with_options};
use netsmith_fabricate::layout_edit::{Severity, check};
use netsmith_pcb::Board;
use netsmith_pipeline::{Built, Libraries, build_with};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const SOURCE: &str = "import std::parts::passive::Resistor
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
    .r2: Resistor { .resistance &= 15kohm +/- 1% }
    .r3: Resistor { .resistance &= 15kohm +/- 1% }
    .rail.pos ~> .r1 ~> .rail.neg
    .rail.pos ~> .r2 ~> .rail.neg
    .rail.pos ~> .r3 ~> .rail.neg
}
";

fn physical(extra: &str) -> String {
    format!(
        "import std::pcb::Outline

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= 30mm
            .height &= 20mm
        }}
    }}
{extra}}}
"
    )
}

fn loader(extra: &str) -> Loader {
    Loader::std().with_physical("widget.nsm", "widget.physical.nsm", &physical(extra))
}

fn build(extra: &str) -> Result<Built, String> {
    build_with(
        "widget.nsm",
        SOURCE,
        &loader(extra),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .map(|mut b| b.remove(0))
}

fn built(extra: &str) -> Built {
    build(extra).unwrap_or_else(|e| panic!("{e}"))
}

fn placed(b: &Built) -> Board {
    let libraries = Libraries::sample();
    netsmith_placer::board_from_design_placed(
        &b.design,
        &libraries.mapping,
        &libraries.footprints,
        &netsmith_pcb::place_options(&b.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_board_with_no_soft_margin_lists_none() {
    let b = built("");
    assert_eq!(b.design.features.soft_margins, []);
    let board = placed(&b);
    assert!(board.parts.iter().all(|p| p.soft_margin == 0.0));
    assert_eq!(board.soft_margins_broken(), []);
}

#[test]
fn the_statement_gives_the_part_its_distance_and_its_line() {
    let b = built("    soft margin 3mm round .r1\n");
    let margins = &b.design.features.soft_margins;
    assert_eq!(margins.len(), 1, "{margins:?}");
    assert_eq!((margins[0].path.as_str(), margins[0].margin), ("r1", 3.0));
    assert_eq!(margins[0].source, "widget.physical.nsm:10");
    let board = placed(&b);
    let margin = |r: &str| board.parts.iter().find(|p| p.path == r).unwrap().soft_margin;
    assert_eq!((margin("r1"), margin("r2"), margin("r3")), (3.0, 0.0, 0.0));
}

#[test]
fn the_placer_keeps_other_parts_out_of_the_margin_where_it_can() {
    // Without the statement the placer puts a neighbour of r1 nearer than 3 mm, so that the
    // statement is what keeps it away.
    let mut plain = placed(&built(""));
    for p in &mut plain.parts {
        p.soft_margin = if p.path == "r1" { 3.0 } else { 0.0 };
    }
    assert!(!plain.soft_margins_broken().is_empty());
    let board = placed(&built("    soft margin 3mm round .r1\n"));
    assert_eq!(board.soft_margins_broken(), []);
    assert_eq!(board.check_placement(), vec![]);
    let r1 = board.parts.iter().find(|p| p.path == "r1").unwrap();
    for other in board.parts.iter().filter(|p| p.path != "r1") {
        assert_eq!(r1.soft_short(other), None, "{}", other.refdes);
    }
}

#[test]
fn a_margin_the_board_cannot_keep_is_listed_with_both_parts_the_numbers_and_the_line() {
    // 40 mm round a part on a 30 by 20 mm board: no other part can be that far.
    let board = placed(&built("    soft margin 40mm round .r1\n"));
    let broken = board.soft_margins_broken();
    assert!(!broken.is_empty(), "{:?}", board.parts);
    let first = &broken[0];
    assert_eq!((first.path.as_str(), first.margin), ("r1", 40.0));
    assert!(first.gap < 40.0 && first.gap >= 0.0, "{first:?}");
    let text = first.to_string();
    for want in [
        "mm from `r1` (R1), courtyard to courtyard, and the soft margin round `r1` (R1) asks for 40 mm",
        "(widget.physical.nsm:10)",
    ] {
        assert!(text.contains(want), "{want:?} is not in {text}");
    }
    // The board is still a valid placement: the margin is soft.
    assert_eq!(board.check_placement(), vec![]);
}

#[test]
fn the_statement_is_refused_with_the_line_and_what_to_write() {
    for (extra, want) in [
        (
            "    soft margin -1mm round .r1\n",
            "a margin is zero or more, and `-1mm` is -1 mm",
        ),
        (
            "    soft margin 3 round .r1\n",
            "the margin is a length such as `3mm`, and `3` is not",
        ),
        (
            "    soft margin .r1.height round .r1\n",
            "the margin is one fixed length such as `3mm`",
        ),
        (
            "    soft margin 3mm round .rail\n",
            "names `.rail`, which is a",
        ),
        (
            "    soft margin 3mm round .r1\n    soft margin 4mm round .r1\n",
            "`.r1` already has a `soft margin`",
        ),
    ] {
        let Err(e) = build(extra) else {
            panic!("`{extra}` builds")
        };
        assert!(e.contains(want), "{want:?} is not in {e}");
    }
}

#[test]
fn the_circuit_source_may_not_state_one() {
    let source = SOURCE.replace(
        "    .rail.pos ~> .r1 ~> .rail.neg\n",
        "    soft margin 3mm round .r1\n    .rail.pos ~> .r1 ~> .rail.neg\n",
    );
    let Err(e) = build_with(
        "widget.nsm",
        &source,
        &loader(""),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    ) else {
        panic!("a soft margin in the circuit builds")
    };
    assert!(e.contains("a `soft margin` belongs in a physical file"), "{e}");
}

#[test]
fn layout_check_says_a_broken_margin_among_locked_parts_as_a_warning_and_an_error_when_strict() {
    let b = built("    soft margin 3mm round .r1\n");
    let libraries = Libraries::sample();
    let dir = TempDir::new("soft-margin-layout");
    let file = dir.join("widget.pcb.toml");
    // Two resistors 1 mm apart: both locked there.
    std::fs::write(
        &file,
        "version = 1\n\n[Widget.parts.r1]\nat = [10.0, 10.0]\nrotation = 0\nlock = true\n\n[Widget.parts.r2]\nat = [12.5, 10.0]\nrotation = 0\nlock = true\n",
    )
    .unwrap();
    let checked = check(&b.design, &file, &libraries, false).unwrap_or_else(|e| panic!("{e}"));
    let finding = checked
        .findings
        .iter()
        .find(|f| f.kind == "soft_margin")
        .unwrap_or_else(|| panic!("no `soft_margin`: {checked:?}"));
    assert_eq!(finding.severity, Severity::Warning);
    assert_eq!(finding.path.as_deref(), Some("r2"));
    assert!(finding.message.contains("the soft margin round `r1` (R1) asks for 3 mm"), "{finding:?}");
    assert!(checked.ok, "a warning leaves the file ok: {checked:?}");
    let strict = check(&b.design, &file, &libraries, true).unwrap_or_else(|e| panic!("{e}"));
    let finding = strict.findings.iter().find(|f| f.kind == "soft_margin").unwrap();
    assert_eq!(finding.severity, Severity::Error);
    assert!(!strict.ok);
}

thread_local! {
    /// What `fab` said on this thread.
    static SAID: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn watch(message: &str) {
    SAID.with(|s| s.borrow_mut().push(message.to_string()));
}

fn fab(extra: &str, strict: bool) -> Result<(), String> {
    let dir = TempDir::new("soft-margin-fab");
    SAID.with(|s| s.borrow_mut().clear());
    fab_with_options(
        "widget.nsm",
        SOURCE,
        &loader(extra),
        &dir.join("fab"),
        &Catalogs::jlcpcb().with_strict_guidelines(strict),
        &Libraries::sample(),
        None,
        &FabOptions {
            progress: Some(watch),
            ..FabOptions::default()
        },
    )
    .map(|_| ())
}

#[test]
fn fab_warns_of_a_margin_it_could_not_keep_and_is_stopped_by_it_when_strict() {
    let said = |want: &str| SAID.with(|s| s.borrow().iter().any(|m| m.contains(want)));
    let _ = fab("    soft margin 40mm round .r1\n", false);
    assert!(
        said("warning: ") && said("and the soft margin round `r1` (R1) asks for 40 mm"),
        "{:?}",
        SAID.with(|s| s.borrow().clone())
    );
    let error = fab("    soft margin 40mm round .r1\n", true).expect_err("strict");
    assert!(
        error.contains("the soft margin round `r1` (R1) asks for 40 mm")
            && error.contains("because the run is strict"),
        "{error}"
    );
    // A margin the placer keeps says nothing.
    let _ = fab("    soft margin 3mm round .r1\n", true);
    assert!(!said("soft margin"), "{:?}", SAID.with(|s| s.borrow().clone()));
}
