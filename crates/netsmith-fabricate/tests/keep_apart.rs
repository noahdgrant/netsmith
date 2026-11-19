//! A distance between kinds of parts (issue 1277): `keep 6mm between Resistor and Capacitor` in a
//! board's physical file asks every resistor to stand 6 mm from every capacitor, courtyard to
//! courtyard. It is a requirement on where parts go: the placer keeps it as hard as an overlap,
//! and `fab` and `layout check` stop where it is not met. Every board here is invented: two
//! resistors and a capacitor on a rail, on a 30 by 20 mm board, on the sample libraries (no KiCad
//! is needed).

use std::cell::RefCell;

use netsmith_check::Loader;
use netsmith_fabricate::fab::{FabOptions, fab_detailed};
use netsmith_fabricate::layout_edit::{Severity, check};
use netsmith_pcb::Board;
use netsmith_pipeline::{Built, Libraries, build_with};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const SOURCE: &str = "import std::parts::passive::Resistor
import std::parts::passive::Capacitor
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
    .c1: Capacitor {
        .capacitance &= 100nF +/- 10%
        .max_voltage >= .bias * 1.5
    }
    .rail.pos ~> .r1 ~> .rail.neg
    .rail.pos ~> .r2 ~> .rail.neg
    .rail.pos ~ .c1.a
    .rail.neg ~ .c1.b
}
";

const RULE: &str = "    keep 6mm between Resistor and Capacitor\n";

fn physical(extra: &str) -> String {
    format!(
        "import std::pcb::Outline
import std::parts::passive::Resistor
import std::parts::passive::Capacitor
import std::parts::passive::Crystal
import std::parts::regulator::FeedbackDivider

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

fn part<'a>(board: &'a Board, path: &str) -> &'a netsmith_pcb::Placed {
    board.parts.iter().find(|p| p.path == path).unwrap()
}

#[test]
fn a_board_with_no_rule_keeps_no_distance() {
    let b = built("");
    assert_eq!(b.design.features.keep_apart, []);
    let board = placed(&b);
    assert!(board.parts.iter().all(|p| p.apart.is_empty()));
    assert_eq!(board.kept_apart_broken(), []);
}

#[test]
fn the_rule_reaches_every_part_of_each_kind_with_its_line() {
    let b = built(RULE);
    let rules = &b.design.features.keep_apart;
    assert_eq!(rules.len(), 1, "{rules:?}");
    assert_eq!(rules[0].rule, "keep 6mm between Resistor and Capacitor");
    assert_eq!(rules[0].distance, 6.0);
    assert_eq!(rules[0].parts, [vec!["r1", "r2"], vec!["c1"]]);
    assert_eq!(rules[0].source, "widget.physical.nsm:14");
    let board = placed(&b);
    let others = |path: &str| -> Vec<String> {
        part(&board, path)
            .apart
            .iter()
            .map(|a| a.path.clone())
            .collect()
    };
    assert_eq!(others("r1"), ["c1"]);
    assert_eq!(others("r2"), ["c1"]);
    assert_eq!(others("c1"), ["r1", "r2"]);
}

#[test]
fn the_placer_keeps_the_distance() {
    // Without the rule the placer puts the capacitor nearer a resistor than 6 mm, so that the
    // rule is what keeps them apart.
    let plain = placed(&built(""));
    let near = ["r1", "r2"]
        .iter()
        .filter(|r| part(&plain, r).courtyard_distance(part(&plain, "c1")) < 6.0)
        .count();
    assert!(near > 0, "{:?}", plain.parts);
    let board = placed(&built(RULE));
    assert_eq!(board.kept_apart_broken(), []);
    assert_eq!(board.check_placement(), vec![]);
    for r in ["r1", "r2"] {
        let gap = part(&board, r).courtyard_distance(part(&board, "c1"));
        assert!(gap >= 6.0 - 1e-6, "{r} is {gap} mm from c1");
    }
}

#[test]
fn a_kind_kept_from_itself_keeps_each_pair_of_its_parts_apart() {
    let b = built("    keep 5mm between Resistor and Resistor\n");
    let board = placed(&b);
    assert_eq!(board.kept_apart_broken(), []);
    let gap = part(&board, "r1").courtyard_distance(part(&board, "r2"));
    assert!(gap >= 5.0 - 1e-6, "{gap}");
    // A part is not kept from itself.
    assert!(part(&board, "r1").apart.iter().all(|a| a.path != "r1"));
}

#[test]
fn a_distance_the_board_cannot_keep_is_listed_with_both_parts_the_numbers_and_the_line() {
    // 40 mm on a 30 by 20 mm board: no two parts can be that far apart.
    let board = placed(&built("    keep 40mm between Resistor and Capacitor\n"));
    let broken = board.kept_apart_broken();
    assert_eq!(broken.len(), 2, "{broken:?}");
    let first = &broken[0];
    assert_eq!(first.distance, 40.0);
    assert!(first.gap < 40.0 && first.gap >= 0.0, "{first:?}");
    let text = first.to_string();
    for want in [
        "requirement `keep 40mm between Resistor and Capacitor` (widget.physical.nsm:14) is violated: ",
        "courtyard to courtyard, and it asks for 40 mm",
    ] {
        assert!(text.contains(want), "{want:?} is not in {text}");
    }
    let miss = serde_json::to_value(first.miss()).unwrap();
    assert_eq!(miss["measure"], "distance");
    assert_eq!(miss["min"], 40.0);
    assert_eq!(miss["source"], "widget.physical.nsm:14");
    assert!(miss.get("max").is_none(), "{miss}");
}

#[test]
fn a_rule_that_reaches_no_pair_is_a_note() {
    let b = built("    keep 3mm between Resistor and Crystal\n");
    assert_eq!(b.design.features.keep_apart.len(), 1);
    assert!(
        b.notes.iter().any(|n| n.contains(
            "`keep 3mm between Resistor and Crystal` keeps no two parts apart: the board has no `Crystal`"
        )),
        "{:?}",
        b.notes
    );
    let b = built("    keep 3mm between Capacitor and Capacitor\n");
    assert!(
        b.notes
            .iter()
            .any(|n| n.contains("the board has one `Capacitor` only")),
        "{:?}",
        b.notes
    );
}

#[test]
fn the_rule_is_refused_with_the_line_and_what_to_write() {
    for (extra, want) in [
        (
            "    keep -1mm between Resistor and Capacitor\n",
            "a distance is zero or more, and `-1mm` is -1 mm",
        ),
        (
            "    keep 3 between Resistor and Capacitor\n",
            "the distance is a length such as `10mm`, and `3` is not",
        ),
        (
            "    keep 3mm between Resistor and Inductor\n",
            "no part, module or generic `Inductor` is in scope",
        ),
        (
            "    keep 3mm between Resistor and Outline\n",
            "`keep` names a part or a generic",
        ),
        (
            "    keep 3mm between Resistor and FeedbackDivider\n",
            "`FeedbackDivider` is a module; `keep 3mm between Resistor and FeedbackDivider` keeps parts apart",
        ),
        (
            "    keep 3mm between Resistor and Capacitor\n    keep 4mm between Capacitor and Resistor\n",
            "`Capacitor` and `Resistor` already keep a distance, at ",
        ),
        (
            "    case {\n        keep 3mm between Resistor and Capacitor\n    }\n",
            "is not part of the case",
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
        "    keep 3mm between Resistor and Capacitor\n    .rail.pos ~> .r1 ~> .rail.neg\n",
    );
    let Err(e) = build_with(
        "widget.nsm",
        &source,
        &loader(""),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    ) else {
        panic!("a `keep` in the circuit builds")
    };
    assert!(
        e.contains("`keep 3mm between Resistor and Capacitor` belongs in a board's physical file"),
        "{e}"
    );
}

#[test]
fn layout_check_says_two_locked_parts_too_near_as_a_requirement() {
    let b = built(RULE);
    let libraries = Libraries::sample();
    let dir = TempDir::new("keep-apart-layout");
    let file = dir.join("widget.pcb.toml");
    // A resistor and the capacitor 3 mm apart: both locked there.
    std::fs::write(
        &file,
        "version = 1\n\n[Widget.parts.r1]\nat = [10.0, 10.0]\nrotation = 0\nlock = true\n\n[Widget.parts.c1]\nat = [13.0, 10.0]\nrotation = 0\nlock = true\n",
    )
    .unwrap();
    let checked = check(&b.design, &file, &libraries, false).unwrap_or_else(|e| panic!("{e}"));
    let finding = checked
        .findings
        .iter()
        .find(|f| f.kind == "requirement" && f.message.contains("keep 6mm"))
        .unwrap_or_else(|| panic!("no `requirement`: {checked:?}"));
    assert_eq!(finding.severity, Severity::Error);
    assert!(matches!(finding.path.as_deref(), Some("c1" | "r1")), "{finding:?}");
    assert!(
        finding
            .message
            .contains("courtyard to courtyard, and it asks for 6 mm"),
        "{finding:?}"
    );
    assert!(!checked.ok);
}

thread_local! {
    /// What `fab` said on this thread.
    static SAID: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn watch(message: &str) {
    SAID.with(|s| s.borrow_mut().push(message.to_string()));
}

fn fab(extra: &str, warn: bool) -> Result<(), netsmith_fabricate::fab::FabFailure> {
    let dir = TempDir::new("keep-apart-fab");
    SAID.with(|s| s.borrow_mut().clear());
    fab_detailed(
        "widget.nsm",
        SOURCE,
        &loader(extra),
        &dir.join("fab"),
        &Catalogs::jlcpcb().with_constraints_warn(warn),
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
fn fab_stops_on_a_distance_it_could_not_keep_and_warns_when_constraints_warn() {
    let rule = "    keep 40mm between Resistor and Capacitor\n";
    let failure = fab(rule, false).expect_err("a requirement not met stops fab");
    assert!(
        failure
            .text
            .contains("requirement `keep 40mm between Resistor and Capacitor` (widget.physical.nsm:14) is violated"),
        "{}",
        failure.text
    );
    assert_eq!(failure.requirements.len(), 2, "{:?}", failure.requirements);
    assert!(failure.requirements.iter().all(|m| m.measure == "distance"));
    let said = |want: &str| SAID.with(|s| s.borrow().iter().any(|m| m.contains(want)));
    let _ = fab(rule, true);
    assert!(
        said("warning: requirement `keep 40mm between Resistor and Capacitor`"),
        "{:?}",
        SAID.with(|s| s.borrow().clone())
    );
    // A distance the placer keeps says nothing.
    let _ = fab(RULE, false);
    assert!(!said("keep 6mm"), "{:?}", SAID.with(|s| s.borrow().clone()));
}
