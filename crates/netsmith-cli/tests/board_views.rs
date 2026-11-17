//! `netsmith board bom`, `power`, `system` and `panel` against the editor's real server over the
//! same project (issue #1096): each view's `--format json` is the document the editor's route
//! answers for the same board, and the text and the choice of a name say what they should.

use netsmith_editor::Backend;
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;
use std::fs;
use std::net::{Ipv4Addr, TcpListener};
use std::process::{Command, Output};
use std::sync::Arc;
use std::thread;

const LED: &str = include_str!("../../../examples/led.nsm");
const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");
const PANEL: &str = include_str!("../../../examples/led_panel.nsm");
const PANEL_PHYSICAL: &str = include_str!("../../../examples/led_panel.physical.nsm");

/// A project of one source file, `src/main.nsm`, with `extra` files beside it.
fn project(tag: &str, source: &str, extra: &[(&str, &str)]) -> TempDir {
    let root = TempDir::new(tag);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), source).unwrap();
    for (name, text) in extra {
        fs::write(root.join("src").join(name), text).unwrap();
    }
    root
}

/// The editor's server over the project (asking no supplier, as `--offline` does), on a free port.
fn serve(root: &TempDir) -> u16 {
    let target = Target::resolve_from(root, None).unwrap().with_offline(true);
    let workspace = Workspace::open(target).unwrap();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || netsmith_editor::serve(listener, Arc::new(workspace)));
    port
}

/// What the editor's route answers.
fn editor(port: u16, route: &str) -> serde_json::Value {
    let url = format!("http://127.0.0.1:{port}{route}");
    let text = ureq::get(&url)
        .call()
        .unwrap_or_else(|e| panic!("{url}: {e}"))
        .body_mut()
        .read_to_string()
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

fn netsmith(root: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// The JSON a command prints, which must be the whole of stdout.
fn json(root: &TempDir, args: &[&str]) -> serde_json::Value {
    let mut all = args.to_vec();
    all.extend(["--format", "json"]);
    let output = netsmith(root, &all);
    assert!(output.status.success(), "{all:?}: {}", stderr(&output));
    serde_json::from_str(&stdout(&output))
        .unwrap_or_else(|e| panic!("{all:?} printed no single JSON document ({e}):\n{}", stdout(&output)))
}

#[test]
fn the_bill_of_materials_is_the_editors() {
    let root = project("views-bom", LED, &[]);
    let port = serve(&root);
    let expected = editor(port, "/api/bom?board=LedExample");
    assert_eq!(json(&root, &["board", "bom", "--offline"]), expected);
    // Named, and for the file instead of the project, it is the same.
    assert_eq!(
        json(&root, &["board", "bom", "src/main.nsm", "--board", "LedExample", "--offline"]),
        expected
    );
    assert!(
        !expected["lines"].as_array().unwrap().is_empty(),
        "the bill has lines: {expected}"
    );
}

#[test]
fn the_bill_of_materials_prints_the_lines_and_the_total() {
    let root = project("views-bom-text", LED, &[]);
    let output = netsmith(&root, &["board", "bom", "--offline"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    for want in ["LedExample: bill of materials", "QTY", "LCSC", "R1", "Total: no line has a price"] {
        assert!(text.contains(want), "missing {want:?} in\n{text}");
    }
}

#[test]
fn the_power_tree_is_the_editors() {
    let root = project("views-power", LED, &[]);
    let port = serve(&root);
    let expected = editor(port, "/api/power?board=LedExample");
    assert_eq!(json(&root, &["board", "power"]), expected);
    assert!(!expected["nodes"].as_array().unwrap().is_empty());
    let output = netsmith(&root, &["board", "power"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    for want in ["LedExample: power tree", "demand", "No power issues"] {
        assert!(text.contains(want), "missing {want:?} in\n{text}");
    }
}

#[test]
fn a_system_and_the_boards_of_it_are_the_editors() {
    let root = project("views-system", TWO_BOARDS, &[]);
    let port = serve(&root);
    // The one system is the default, and a board of it shows its members and nets.
    let system = editor(port, "/api/system?name=TwoBoards");
    assert_eq!(json(&root, &["board", "system"]), system);
    assert_eq!(system["kind"], "system");
    assert_eq!(json(&root, &["board", "system", "--name", "TwoBoards"]), system);
    let board = editor(port, "/api/system?name=TwoBoards%2Fpower");
    assert_eq!(
        json(&root, &["board", "system", "--name", "TwoBoards/power"]),
        board
    );
    assert_eq!(
        json(&root, &["board", "system", "--name", "power"]),
        board,
        "a board is named as `--board` of the other commands names it"
    );
    let output = netsmith(&root, &["board", "system"]);
    let text = stdout(&output);
    for want in ["TwoBoards: system", "Members:", "Joins:"] {
        assert!(text.contains(want), "missing {want:?} in\n{text}");
    }
}

#[test]
fn a_board_of_a_system_has_its_bill_and_power_tree() {
    let root = project("views-system-boards", TWO_BOARDS, &[]);
    let port = serve(&root);
    for (board, route) in [("power", "TwoBoards%2Fpower"), ("led", "TwoBoards%2Fled")] {
        assert_eq!(
            json(&root, &["board", "bom", "--board", board, "--offline"]),
            editor(port, &format!("/api/bom?board={route}"))
        );
        assert_eq!(
            json(&root, &["board", "power", "--board", board]),
            editor(port, &format!("/api/power?board={route}"))
        );
    }
}

#[test]
fn a_panel_is_the_editors() {
    let root = project("views-panel", PANEL, &[("main.physical.nsm", PANEL_PHYSICAL)]);
    let port = serve(&root);
    let expected = editor(port, "/api/panel?name=LedPanel");
    assert_eq!(json(&root, &["board", "panel"]), expected);
    assert_eq!(json(&root, &["board", "panel", "--name", "LedPanel"]), expected);
    assert_eq!(expected["columns"], 3);
    let output = netsmith(&root, &["board", "panel"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("LedPanel: a panel of 3 x 2 LedTile"), "{text}");
    assert!(!text.contains("<svg"), "the drawing is for the JSON: {text}");
}

#[test]
fn a_name_that_cannot_be_chosen_is_said_with_the_choices() {
    let root = project("views-names", TWO_BOARDS, &[]);
    // Two boards: one must be named.
    let output = netsmith(&root, &["board", "bom", "--offline"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    let said = stderr(&output);
    assert!(
        said.contains("--board") && said.contains("TwoBoards/power") && said.contains("TwoBoards/led"),
        "{said}"
    );
    let output = netsmith(&root, &["board", "power", "--board", "Nope"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("no board named `Nope`"), "{}", stderr(&output));
    // No panel in the source.
    let output = netsmith(&root, &["board", "panel"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("no panel"), "{}", stderr(&output));
    // Nothing was printed on stdout by a failure.
    assert!(stdout(&output).is_empty());
}

#[test]
fn a_source_that_does_not_build_says_why_and_fails() {
    let root = project("views-broken", "board B {\n  nonsense\n}\n", &[]);
    let output = netsmith(&root, &["board", "bom", "--offline"]);
    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert!(stdout(&output).is_empty());
    assert!(!stderr(&output).is_empty());
}

#[test]
fn the_views_agree_with_the_backend_they_share() {
    // The command is the Backend's call and nothing else: the workspace's own answer, serialised.
    let root = project("views-backend", LED, &[]);
    let target = Target::resolve_from(&root, None).unwrap().with_offline(true);
    let workspace = Workspace::open(target).unwrap();
    let want = serde_json::to_value(workspace.power("LedExample").unwrap()).unwrap();
    assert_eq!(json(&root, &["board", "power"]), want);
}

/// A load nothing supplies, on a connector so that it builds.
const ORPHAN: &str = "import std::mfg::jlcpcb::TwoLayer
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
        .min_track &= 1mm..
    }
    .out: Header1x2
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}
";

#[test]
fn the_power_issues_are_in_the_text_and_do_not_fail_the_command() {
    let root = project("views-issues", ORPHAN, &[]);
    let output = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board", "power"])
        .current_dir(&root)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        )
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("Power issues, worst first:") && text.contains("note: V5 has no source"),
        "{text}"
    );
}
