//! Each note of a build has a cause (issue #192): the tool that makes the note says it, the
//! editor sends it as `Problem.cause`, and the Problems dock groups the notes by it. One
//! invented board for each kind of note; the causes that only a solver or a design-stage test can
//! reach (a level with no value, a pin the model cannot judge, a track width, a bank) are tested
//! there (`netsmith-solve` `levels::`, `bus_clock::`; `netsmith-design` `guidelines::`, `bank::`).

use std::fs;

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith::{Built, outcome_with};
use netsmith_check::Loader;
use netsmith_core::Cause;
use netsmith_editor::{
    Backend,
    api::{Cause as ApiCause, ProblemStage, Severity},
};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

fn built(src: &str, catalogs: &Catalogs) -> Built {
    let outcome = outcome_with("invented.nsm", src, &Loader::std(), catalogs, None)
        .unwrap_or_else(|e| panic!("{}", netsmith::render_problems(&e)));
    outcome.boards.into_iter().next().expect("one board")
}

/// The notes of the board, each with its cause.
fn noted(b: &Built) -> Vec<(&str, Cause)> {
    assert_eq!(b.notes.len(), b.note_causes.len());
    b.notes
        .iter()
        .map(String::as_str)
        .zip(b.note_causes.iter().copied())
        .collect()
}

fn cause_of(b: &Built, has: &str) -> Cause {
    let found: Vec<_> = noted(b)
        .into_iter()
        .filter(|(n, _)| n.contains(has))
        .collect();
    assert_eq!(found.len(), 1, "{has}: {:#?}", noted(b));
    found[0].1
}

/// A constant-power load behind a series resistor, with the resistor's own rating check
/// (`drop * current <= max_power`), and `extra` in the board.
fn rated(extra: &str) -> String {
    format!(
        r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::passive::Resistor
import std::sources::Rail

board Circle {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .supply: Rail {{
        .voltage &= 9V..16V
        .pos.net &= "VBATT"
        .neg.net &= "GND"
    }}
    .r1: Resistor {{ .resistance &= 10ohm +/- 1% }}
    .load: Rail {{
        .pos.net &= "VLOAD"
        .neg.net &= "GND"
    }}
    .supply.neg ~ .load.neg
    .supply.pos ~> .r1 ~> .load.pos
    .load.voltage == .supply.voltage - .r1.drop
    .load.current == 0.5W / .load.voltage{extra}
}}
"#
    )
}

const OUTLINE_STAGE: &str = r#"import std::parts::passive::Resistor
import std::sources::Rail

/// A step-down stage in outline: a resistor from the input to the output, and a
/// divider from the output to ground. The resistor is joined with `~`, not a chain, so
/// what it carries is not modelled.
module Stage {
    .vin: Rail
    .vout: Rail
    .loose: Resistor { .resistance &= 1ohm +/- 1% }
    .r_top: Resistor { .resistance &= 100kohm +/- 1% }
    .r_bottom: Resistor { .resistance &= 100kohm +/- 1% }

    .vin.pos ~ .loose.a
    .loose.b ~ .vout.pos
    .vin.neg ~ .vout.neg
    .vout.pos ~> .r_top ~> .r_bottom ~> .vout.neg
}

board Supply {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .stage: Stage { .vout.voltage &= 3.3V +/- 3% }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0A..2A
    }
    .input ~ .stage.vin
    .stage.vout ~ .output
}
"#;

const BOARD: &str = r#"
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

module Limit {
    .supply: Rail
    .limit: Resistor { .resistance &= 45.8kohm..46kohm }
    .supply.pos ~> .limit ~> .supply.neg
}

board Invented {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }
    .charge: Limit
    .supply ~ .charge.supply
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}
"#;

/// A load that nothing supplies, on a connector, so that it builds into a schematic.
fn orphan() -> String {
    "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Bench {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .load: Rail {
        .pos.net &= \"V5\"
        .neg.net &= \"GND\"
        .current &= 0mA..10mA
        .min_track &= 1mm..1m
    }
    .out: Header1x2
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
"
    .to_string()
}

#[test]
fn a_check_that_waits_on_a_value_nothing_states_is_not_stated() {
    // What the output draws is not stated: the resistor's power cannot be judged.
    let src = OUTLINE_STAGE.replace("        .current &= 0A..2A\n", "");
    let b = built(&src, &Catalogs::jlcpcb());
    assert_eq!(cause_of(&b, "stage.loose.max_power"), Cause::NotStated);
}

#[test]
fn a_pin_the_balance_does_not_model_is_a_gap_of_the_parts_model() {
    let src = OUTLINE_STAGE.replace("        .current &= 0A..2A\n", "");
    let b = built(&src, &Catalogs::jlcpcb());
    assert_eq!(cause_of(&b, "is incomplete"), Cause::PartModel);
}

#[test]
fn values_that_depend_on_each_other_are_a_circle() {
    let b = built(&rated(""), &Catalogs::jlcpcb());
    assert_eq!(cause_of(&b, "depend on each other"), Cause::Circular);
}

#[test]
fn a_part_the_catalog_says_is_out_of_stock_is_a_pick() {
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file(
            "resistor",
            "catalogs/resistor.csv",
            "lcsc,mpn,resistance,max_power,package,library,stock,source\n\
             C99,ZZR-459,45.9kohm +/- 0.1%,150mW,0603,Extended,0,https://example.invalid/zzr-459\n",
        )
        .unwrap();
    let b = built(BOARD, &catalogs);
    assert_eq!(cause_of(&b, "states a stock of 0"), Cause::Pick);
}

fn project(name: &str, src: &str) -> TempDir {
    let root = TempDir::new(&format!("note-causes-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), src).unwrap();
    root
}

#[test]
fn a_rail_nothing_supplies_is_a_supply_note_and_the_editor_sends_the_cause_of_every_note() {
    let root = project("supply", &orphan());
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    let power: Vec<_> = analysis
        .problems
        .iter()
        .filter(|p| p.stage == Some(ProblemStage::Power))
        .collect();
    assert_eq!(power.len(), 1, "{:?}", analysis.problems);
    assert_eq!(power[0].cause, Some(ApiCause::Supply));
    // A note has a cause; a problem that is not a note has none.
    for p in &analysis.problems {
        assert_eq!(
            p.cause.is_some(),
            p.severity == Severity::Note,
            "{}: {:?}",
            p.message,
            p.cause
        );
    }
    // The page reads it by this name.
    let json = serde_json::to_value(power[0]).unwrap();
    assert_eq!(json["cause"], "supply");
}

#[test]
fn the_notes_of_the_solver_reach_the_page_with_their_causes() {
    let src = OUTLINE_STAGE.replace("        .current &= 0A..2A\n", "");
    let root = project("solver", &src);
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    let cause = |has: &str| {
        analysis
            .problems
            .iter()
            .find(|p| p.message.contains(has))
            .unwrap_or_else(|| panic!("{has}: {:#?}", analysis.problems))
            .cause
    };
    assert_eq!(cause("is incomplete"), Some(ApiCause::PartModel));
    assert_eq!(cause("stage.loose.max_power"), Some(ApiCause::NotStated));
}
