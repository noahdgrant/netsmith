//! `netsmith project show` against the editor's real server over the same project (issue #1138):
//! its `--format json` holds the documents the editor's `GET /api/project`, `GET /api/analysis`
//! and `GET /api/orders` answer, and its text names the boards, systems, panels, files and
//! orders an agent gives to `board bom --board`, `board system --name` and `board panel --name`.

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

/// A project of a board, a system and a panel, with two orders.
fn project(tag: &str) -> TempDir {
    let root = TempDir::new(tag);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/led.nsm\", \"src/two.nsm\", \"src/panel.nsm\"]\n\n\
         [order.small.boards]\nLedExample = 10\n\n[order.system.boards]\nTwoBoards = 2\n",
    )
    .unwrap();
    fs::write(root.join("src/led.nsm"), LED).unwrap();
    fs::write(root.join("src/two.nsm"), TWO_BOARDS).unwrap();
    fs::write(root.join("src/panel.nsm"), PANEL).unwrap();
    fs::write(root.join("src/panel.physical.nsm"), PANEL_PHYSICAL).unwrap();
    root
}

/// The editor's server over the project, on a free port.
fn serve(root: &TempDir) -> u16 {
    let target = Target::resolve_from(root, None).unwrap();
    let workspace = Workspace::open(target).unwrap();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || netsmith_editor::serve(listener, Arc::new(workspace)));
    port
}

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

/// The command in the folder of the project, with the environment of this test, so that it finds
/// KiCad (or not) as the server in this process does.
fn netsmith(root: &TempDir, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// How long an analysis took is the only field that is not the project's: two readings of the
/// same project take two times.
fn without_time(mut analysis: serde_json::Value) -> serde_json::Value {
    analysis.as_object_mut().unwrap().remove("elapsedMs");
    analysis
}

#[test]
fn the_project_is_the_editors() {
    let root = project("show-json");
    let port = serve(&root);
    let output = netsmith(&root, &["project", "show", "--format", "json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let shown: serde_json::Value = serde_json::from_str(&stdout(&output))
        .unwrap_or_else(|e| panic!("not a single JSON document ({e}):\n{}", stdout(&output)));
    let project = editor(port, "/api/project");
    let analysis = editor(port, "/api/analysis");
    assert_eq!(shown["project"], project);
    assert_eq!(without_time(shown["analysis"].clone()), without_time(analysis.clone()));
    assert_eq!(shown["orders"], editor(port, "/api/orders"));
    // What the issue asks the document to hold.
    assert!(project["files"].as_array().unwrap().len() > 3, "{project}");
    assert!(project["version"].is_string());
    assert_eq!(analysis["boards"].as_array().unwrap().len(), 4, "{analysis}");
    assert_eq!(analysis["systems"][0]["name"], "TwoBoards");
    assert_eq!(analysis["panels"][0]["name"], "LedPanel");
    assert_eq!(shown["orders"].as_array().unwrap().len(), 2);
}

#[test]
fn the_project_is_printed_for_a_person() {
    let root = project("show-text");
    let output = netsmith(&root, &["project", "show"]);
    assert!(output.status.success(), "{}", stderr(&output));
    // The columns are aligned: compare the words.
    let text = stdout(&output)
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n");
    for want in [
        "demo: project, netsmith ",
        "KiCad: ",
        "Boards (",
        "LedExample",
        "TwoBoards/power",
        "Systems (1):",
        "TwoBoards src/two.nsm TwoBoards/power, TwoBoards/led",
        "Panels (1):",
        "LedPanel",
        "LedTile 3 x 2",
        "Files (",
        "src/led.nsm top level",
        "src/panel.physical.nsm physical",
        "netsmith.toml manifest",
        "of the standard library",
        "Orders (2):",
        "small: LedExample x10",
        "system: TwoBoards x2",
    ] {
        assert!(text.contains(want), "missing {want:?} in\n{text}");
    }
    assert!(!text.contains("std/"), "the standard library is counted: {text}");
}

#[test]
fn one_file_shows_its_boards_and_a_mistake_is_counted() {
    let root = project("show-file");
    let output = netsmith(&root, &["project", "show", "src/led.nsm"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("LedExample") && !text.contains("TwoBoards/power"), "{text}");
    fs::write(root.join("src/led.nsm"), "board B {\n  nonsense\n}\n").unwrap();
    let output = netsmith(&root, &["project", "show"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        !stdout(&output).contains("Analysis: 0 errors"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn a_project_with_no_boards_shows_its_files() {
    let root = TempDir::new("show-library");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"lib\"\ntop_level = []\n",
    )
    .unwrap();
    fs::write(root.join("src/parts.nsm"), "").unwrap();
    let output = netsmith(&root, &["project", "show"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("lib: project") && text.contains("netsmith.toml") && text.contains("manifest") && text.contains("Orders: none"),
        "{text}"
    );
}
