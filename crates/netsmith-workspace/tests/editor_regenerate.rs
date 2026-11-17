//! Regenerating one module in the editor (issue #825): the proposal writes nothing, the page
//! sends its changes as one edit, a route after it keeps the copper of the nets the module does
//! not touch and routes only the rest, and an undo back to the placement routed before shows its
//! copper again with no router. Work is counted in nets kept and routed again, never in time.

use std::{
    fs,
    sync::{Arc, Mutex},
};

use netsmith_editor::{
    Backend,
    api::{PcbChange, PcbEdit, PcbRegenerate},
};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const BOARD: &str = "Lights";

/// Three modules of a resistor and an LED across one rail: regenerating `a` touches the rail
/// and its own middle net, not the middle nets of `b` and `c`.
const SOURCE: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

/// A resistor and an LED across a rail.
module Indicator {
    .vin: Rail
    .led: Led {
        .current &= 5mA..15mA
    }
    .r: Resistor {}
    .vin.pos ~> .r ~> .led ~> .vin.neg
}

board Lights {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
    }
    .a: Indicator
    .b: Indicator
    .c: Indicator
    .supply ~ .a.vin
    .supply ~ .b.vin
    .supply ~ .c.vin
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
}
";

fn project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-regenerate-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"lights\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), SOURCE).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

fn ask(stamp: &str, module: &str, seed: u32) -> PcbRegenerate {
    PcbRegenerate {
        board: BOARD.into(),
        stamp: stamp.into(),
        module: module.into(),
        seed,
        margin: None,
    }
}

/// The edit turned round: what the page sends to undo it.
fn undo_of(changes: &[PcbChange]) -> Vec<PcbChange> {
    changes
        .iter()
        .map(|c| match c {
            PcbChange::Part {
                path,
                before,
                after,
            } => PcbChange::Part {
                path: path.clone(),
                before: after.clone(),
                after: before.clone(),
            },
            other => other.clone(),
        })
        .collect()
}

#[test]
fn a_regenerated_module_is_one_edit_and_a_route_after_it_keeps_the_rest_of_the_copper() {
    let (root, editor) = project("route");
    let say: netsmith_editor::Say = Arc::new(|_: &str| {});
    let first = editor.route(BOARD, &say).unwrap();
    assert!(first.unrouted.is_empty(), "{first:?}");
    assert!(first.carried.is_empty() && first.rerouted.is_empty(), "{first:?}");
    let routed = editor.pcb_scene(BOARD).unwrap();
    assert!(routed.routed);

    // A proposal that moves a part; it writes nothing.
    let layout = root.join("src/main.pcb.toml");
    let proposal = (0..12)
        .map(|seed| editor.pcb_regenerate(&ask(&routed.layout_stamp, "a", seed), &say).unwrap())
        .find(|p| !p.changes.is_empty() && p.parts.iter().any(|q| {
            routed.parts.iter().find(|r| r.path == q.path).is_some_and(|r| r.courtyard != q.courtyard)
        }))
        .expect("a seed moves a part of `a`");
    assert!(!layout.exists(), "a proposal wrote the layout file");
    let paths: Vec<&str> = proposal.parts.iter().map(|p| p.path.as_str()).collect();
    assert_eq!(paths, ["a.led", "a.r"], "{proposal:?}");
    assert!(proposal.nets.iter().any(|n| n == "VCC"), "{:?}", proposal.nets);
    assert!(!proposal.nets.iter().any(|n| n.contains("b.") || n.contains("c.")), "{:?}", proposal.nets);
    // Still routed: a proposal changes nothing.
    assert!(editor.pcb_scene(BOARD).unwrap().routed);

    // Accepted: one edit, which writes an entry for each part placed again.
    let edited = editor
        .pcb_edit(&PcbEdit {
            board: BOARD.into(),
            stamp: routed.layout_stamp.clone(),
            changes: proposal.changes.clone(),
        })
        .unwrap();
    let file = fs::read_to_string(&layout).unwrap();
    assert_eq!(file.matches("source = \"regen\"").count(), 2, "{file}");
    assert!(!edited.scene.routed, "the edit's board is placed again, unrouted");
    for part in &proposal.parts {
        let now = edited.scene.parts.iter().find(|p| p.path == part.path).unwrap();
        assert_eq!(now.courtyard, part.courtyard, "{}", part.path);
    }

    // Routed again: the middle nets of `b` and `c` keep their copper, `a`'s nets are routed again.
    let second = editor.route(BOARD, &say).unwrap();
    println!("kept {:?}, routed again {:?}", second.carried, second.rerouted);
    assert!(second.unrouted.is_empty(), "{second:?}");
    for kept in ["net-b.led.anode", "net-c.led.anode"] {
        assert!(second.carried.iter().any(|n| n == kept), "{kept} lost its copper: {second:?}");
    }
    for net in &proposal.nets {
        assert!(!second.carried.contains(net), "{net} was kept: {second:?}");
    }

    // Undo: the placement routed first, and its copper with it, with no router.
    let scene = editor.pcb_scene(BOARD).unwrap();
    let undone = editor
        .pcb_edit(&PcbEdit {
            board: BOARD.into(),
            stamp: scene.layout_stamp.clone(),
            changes: undo_of(&proposal.changes),
        })
        .unwrap();
    assert!(undone.scene.routed, "the undo did not bring the copper back");
    assert_eq!(undone.scene.svg, routed.svg);
}

#[test]
fn a_regeneration_is_refused_with_the_reason_or_a_stale_stamp() {
    let (_root, editor) = project("refused");
    let say: netsmith_editor::Say = Arc::new(|_: &str| {});
    let scene = editor.pcb_scene(BOARD).unwrap();
    let missing = editor.pcb_regenerate(&ask(&scene.layout_stamp, "d", 0), &say).unwrap_err();
    assert_eq!(missing.status, 422, "{missing:?}");
    assert!(missing.message.contains("no part in `d`"), "{}", missing.message);
    let stale = editor.pcb_regenerate(&ask("not the stamp", "a", 0), &say).unwrap_err();
    assert_eq!(stale.status, 409, "{stale:?}");
}

#[test]
fn a_proposal_says_each_step_as_it_begins() {
    let (_root, editor) = project("progress");
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    let say: netsmith_editor::Say = {
        let heard = Arc::clone(&heard);
        Arc::new(move |line: &str| heard.lock().unwrap().push(line.to_string()))
    };
    // A board the editor has not placed yet is placed first ("placing Lights"); after that a
    // proposal starts from the board placed. The steps are counted, never timed.
    let stamp = editor.pcb_scene(BOARD).unwrap().layout_stamp;
    editor.pcb_regenerate(&ask(&stamp, "b", 0), &say).unwrap();
    let lines = heard.lock().unwrap().clone();
    assert_eq!(
        lines.iter().rev().take(2).rev().collect::<Vec<_>>(),
        ["placing the parts of b again", "measuring the placement before and after"],
        "{lines:?}"
    );
    heard.lock().unwrap().clear();
    editor.pcb_regenerate(&ask(&stamp, "c", 1), &say).unwrap();
    assert_eq!(
        *heard.lock().unwrap(),
        ["placing the parts of c again", "measuring the placement before and after"]
    );
}

#[test]
fn a_later_session_keeps_the_copper_of_the_nets_a_regeneration_left_alone() {
    let (root, editor) = project("later");
    let say: netsmith_editor::Say = Arc::new(|_: &str| {});
    let first = editor.route(BOARD, &say).unwrap();
    assert!(first.unrouted.is_empty(), "{first:?}");
    // The router's copper is kept on disk, in a folder git ignores.
    let kept = root.join(".netsmith/routes/main.Lights.toml");
    assert!(kept.is_file(), "no kept copper at {}", kept.display());
    assert_eq!(fs::read_to_string(root.join(".netsmith/.gitignore")).unwrap().lines().last(), Some("*"));
    drop(editor);

    // A new session, which has routed nothing: a regeneration and a route after it.
    let later = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap_or_else(|e| panic!("{e}"));
    let scene = later.pcb_scene(BOARD).unwrap();
    assert!(!scene.routed);
    let proposal = (0..12)
        .map(|seed| later.pcb_regenerate(&ask(&scene.layout_stamp, "a", seed), &say).unwrap())
        .find(|p| !p.changes.is_empty() && p.parts.iter().any(|q| {
            scene.parts.iter().find(|r| r.path == q.path).is_some_and(|r| r.courtyard != q.courtyard)
        }))
        .expect("a seed moves a part of `a`");
    later
        .pcb_edit(&PcbEdit {
            board: BOARD.into(),
            stamp: scene.layout_stamp.clone(),
            changes: proposal.changes.clone(),
        })
        .unwrap();
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    let listen: netsmith_editor::Say = {
        let heard = Arc::clone(&heard);
        Arc::new(move |line: &str| heard.lock().unwrap().push(line.to_string()))
    };
    let second = later.route(BOARD, &listen).unwrap();
    println!("kept {:?}, routed again {:?}", second.carried, second.rerouted);
    let lines = heard.lock().unwrap().join("\n");
    assert!(lines.contains("starting from the router's copper kept in"), "{lines}");
    assert!(second.unrouted.is_empty(), "{second:?}");
    for kept in ["net-b.led.anode", "net-c.led.anode"] {
        assert!(second.carried.iter().any(|n| n == kept), "{kept} lost its copper: {second:?}");
    }
    for net in &proposal.nets {
        assert!(!second.carried.contains(net), "{net} was kept: {second:?}");
        assert!(second.rerouted.contains(net), "{net} was not routed again: {second:?}");
    }
}
