//! `unused .processor.usb` (ADR 0083), end to end: the notes a build prints and the power
//! findings the editor serves for a port that no part of the board uses. The processor is
//! invented: its datasheet limits VBUS to 4.35 V to 5.5 V whether or not a board uses the port.

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith_check::Loader;
use netsmith_editor::Backend;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;
use std::fs;

const SRC: &str = r#"
import std::bus::Usb2
import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

/// Zproc, an invented processor.
part Zproc {
    .vdd: Rail
    .vdd.current &= 0mA..20mA
    .usb: Usb2
    .usb.vbus.neg ~ .vdd.neg
    .usb.vbus.voltage >= 4.35V
    .usb.vbus.voltage <= 5.5V

    pins {
        .vdd.pos      power         "VDD"  "1"
        .vdd.neg      power         "VSS"  "2"
        .usb.vbus.pos power         "VBUS" "3" optional
        .usb.dp       bidirectional "DP"   "4"
        .usb.dm       bidirectional "DM"   "5"
    }
}

board Bench {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .current &= -50mA..0mA
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .proc: Zproc
    .supply ~ .proc.vdd
UNUSED}
"#;

fn board(unused: &str) -> netsmith::Outcome {
    outcome(unused).unwrap_or_else(|p| panic!("{}", netsmith::render_problems(&p)))
}

fn outcome(unused: &str) -> Result<netsmith::Outcome, Vec<netsmith::Problem>> {
    netsmith::outcome_with(
        "bench.nsm",
        &SRC.replace("UNUSED", unused),
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
}

#[test]
fn a_port_nothing_uses_is_noted_twice_until_the_board_says_so() {
    let b = &board("").boards[0];
    assert_eq!(
        b.power_notes(),
        ["Bench: proc.usb.vbus.pos has no source: nothing supplies it"]
    );
    assert_eq!(
        b.notes
            .iter()
            .filter(|n| n.contains("proc.usb"))
            .collect::<Vec<_>>(),
        [
            "not evaluated: proc.usb.vbus.voltage >= 4.35V",
            "not evaluated: proc.usb.vbus.voltage <= 5.5V"
        ]
    );
}

#[test]
fn the_statement_removes_both_and_nothing_else() {
    let with = &board("    unused .proc.usb\n").boards[0];
    assert_eq!(with.power_notes(), Vec::<String>::new());
    assert!(
        with.notes.iter().all(|n| !n.contains("usb")),
        "{:?}",
        with.notes
    );
    // What else the board had to say is unchanged.
    let without = &board("").boards[0];
    let rest = |notes: &[String]| -> Vec<String> {
        notes
            .iter()
            .filter(|n| !n.contains("proc.usb"))
            .cloned()
            .collect()
    };
    assert_eq!(rest(&with.notes), rest(&without.notes));
}

#[test]
fn a_board_that_connects_the_port_and_says_it_is_unused_does_not_build() {
    let src = SRC
        .replace("UNUSED", "    .vbus: Rail {\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"VBUS\"\n        .neg.net &= \"GND\"\n    }\n    .vbus ~ .proc.usb.vbus\n    unused .proc.usb\n");
    let Err(problems) =
        netsmith::outcome_with("bench.nsm", &src, &Loader::std(), &Catalogs::jlcpcb(), None)
    else {
        panic!("the statement and the connection contradict each other");
    };
    let said = netsmith::render_problems(&problems);
    assert!(
        said.contains("is stated `unused` at bench.nsm:")
            && said.contains("remove the connection, or remove `unused`"),
        "{said}"
    );
}

/// The problems the editor lists for the board, as text.
fn editor_problems(name: &str, unused: &str) -> Vec<String> {
    let root = TempDir::new(&format!("unused-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"bench\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), SRC.replace("UNUSED", unused)).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    analysis
        .problems
        .iter()
        .map(|p| p.message.clone())
        .collect()
}

#[test]
fn the_editor_lists_the_port_until_the_board_says_it_is_unused() {
    let without = editor_problems("without", "");
    assert!(
        without
            .iter()
            .any(|m| m.contains("proc.usb.vbus.pos has no source")),
        "{without:?}"
    );
    assert!(
        without
            .iter()
            .any(|m| m.contains("not evaluated: proc.usb.vbus.voltage >= 4.35V")),
        "{without:?}"
    );
    let with = editor_problems("with", "    unused .proc.usb\n");
    assert!(with.iter().all(|m| !m.contains("usb")), "{with:?}");
}
