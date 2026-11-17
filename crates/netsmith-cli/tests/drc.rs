//! `netsmith board drc` (issue #1095): netsmith's own design rules check alone. Its findings
//! are `fab`'s for the same board, it routes nothing and writes nothing, and it checks a KiCad
//! board too.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `netsmith <args>` in `dir`, with KiCad switched off: no command here needs it.
fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .env("NETSMITH_TIMINGS", "1")
        .output()
        .unwrap()
}

fn out(run: &Output) -> String {
    String::from_utf8_lossy(&run.stdout).into_owned()
}

fn text(run: &Output) -> String {
    format!("{}{}", out(run), String::from_utf8_lossy(&run.stderr))
}

fn ok(run: &Output) -> String {
    let said = text(run);
    assert!(run.status.success(), "{said}");
    said
}

/// A folder with `examples/led.nsm` and its pick lock.
fn led(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    dir
}

/// The files and folders of `dir`, sorted.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The findings a `board drc` prints as text, one line each: what `fab` lists in its notes.
fn lines(run: &Output) -> Vec<String> {
    out(run)
        .lines()
        .filter(|l| l.starts_with("error [") || l.starts_with("warning ["))
        .map(str::to_string)
        .collect()
}

/// The findings of the board's check that `fab` lists in `fab-notes.txt`, one line each.
fn fab_notes(notes: &Path) -> Vec<String> {
    fs::read_to_string(notes)
        .unwrap()
        .lines()
        .filter(|l| l.starts_with("  warning [") || l.starts_with("  error ["))
        .map(|l| l.trim().to_string())
        .collect()
}

fn json(run: &Output) -> serde_json::Value {
    serde_json::from_str(&out(run)).unwrap_or_else(|e| panic!("{e}: {}", text(run)))
}

#[test]
fn its_findings_are_fabs_for_the_same_board_and_it_routes_and_writes_nothing() {
    let dir = led("drc-equals-fab");
    // A board with its copper in the routes file, which `fab` routes nothing more on.
    ok(&netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "first"]));
    ok(&netsmith(
        &dir,
        &["routes", "import", "first/LedExample/LedExample.kicad_pcb", "--file", "led.nsm"],
    ));
    let fabbed = ok(&netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "second"]));
    assert!(fabbed.contains("design rules check: passed"), "{fabbed}");
    let expected = fab_notes(&dir.join("second/LedExample/fab-notes.txt"));
    assert!(expected.len() >= 2, "a board with warnings to compare: {expected:#?}");

    let before = listing(&dir);
    let run = netsmith(&dir, &["board", "drc", "led.nsm"]);
    let said = text(&run);
    assert!(run.status.success(), "{said}");
    assert_eq!(lines(&run), expected, "{said}");
    assert!(
        said.contains("design rules check: passed"),
        "the last line says how many: {said}"
    );
    // Nothing is routed (the stages that ran are timed, and there is no routing among them)
    // and nothing is written.
    assert!(said.contains("timing: design LedExample"), "{said}");
    assert!(!said.contains("route"), "{said}");
    assert_eq!(listing(&dir), before);

    // The same findings as data: the rule, the place, the nets and the parts each names.
    let run = netsmith(&dir, &["board", "drc", "led.nsm", "--format", "json"]);
    assert!(run.status.success(), "{}", text(&run));
    let report = json(&run);
    assert_eq!(report["command"], "drc");
    assert_eq!(report["ok"], true);
    let findings = report["findings"].as_array().unwrap();
    assert_eq!(findings.len(), expected.len());
    for (found, line) in findings.iter().zip(&expected) {
        assert_eq!(found["code"], "drc");
        assert_eq!(found["severity"], "warning");
        let rule = found["rule"].as_str().unwrap();
        assert!(line.starts_with(&format!("warning [{rule}]")), "{rule}: {line}");
        assert!(found["place"]["x"].is_f64() && found["place"]["y"].is_f64());
        assert!(line.contains(found["message"].as_str().unwrap().split(": ").next().unwrap()));
    }
    let silk = findings.iter().find(|f| f["rule"] == "silk_over_copper").unwrap();
    assert_eq!(silk["place"]["layer"], "F.SilkS");
    assert_eq!(silk["nets"], serde_json::json!(["VCC"]));
    assert_eq!(silk["parts"], serde_json::json!(["D1", "R1"]));
    assert_eq!(report["boards"][0]["board"], "LedExample");
    assert_eq!(report["boards"][0]["outline_at"]["x"], 20.0);
}

#[test]
fn a_board_with_no_routes_has_a_finding_for_each_connection_and_exits_1() {
    let dir = led("drc-unrouted");
    let before = listing(&dir);
    let run = netsmith(&dir, &["board", "drc", "led.nsm", "--format", "json"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    let report = json(&run);
    assert_eq!(report["ok"], false);
    let errors: Vec<_> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["severity"] == "error")
        .collect();
    // The router would have routed the three connections; nothing routes them here.
    assert_eq!(errors.len(), 3, "{report:#}");
    assert!(errors.iter().all(|f| f["rule"] == "unconnected_items"));
    let nets: Vec<&str> = errors
        .iter()
        .map(|f| f["nets"][0].as_str().unwrap())
        .collect();
    assert_eq!(nets, ["GND", "VCC", "net-led.anode"]);
    assert_eq!(errors[0]["parts"], serde_json::json!(["D1", "J1"]));
    assert!(!text(&run).contains("route"), "{}", text(&run));
    assert_eq!(listing(&dir), before);

    // The text says the same, errors first, and ends with the count.
    let run = netsmith(&dir, &["board", "drc", "led.nsm"]);
    assert_eq!(run.status.code(), Some(1));
    let said = out(&run);
    assert_eq!(lines(&run).iter().filter(|l| l.starts_with("error [")).count(), 3);
    assert!(said.contains("design rules check: 3 error(s)"), "{said}");
}

#[test]
fn a_kicad_board_is_checked_with_the_project_file_beside_it() {
    let dir = led("drc-kicad-board");
    ok(&netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "first"]));
    // The board `fab` wrote passes.
    let run = netsmith(
        &dir,
        &["board", "drc", "--kicad-pcb", "first/LedExample/LedExample.kicad_pcb"],
    );
    let said = text(&run);
    assert!(run.status.success(), "{said}");
    assert!(said.contains("design rules check: passed"), "{said}");

    // The placed board with no copper, as `layout export` writes it, has the connections left.
    ok(&netsmith(
        &dir,
        &["layout", "export", "--file", "led.nsm", "--kicad-pcb", "bare.kicad_pcb"],
    ));
    let run = netsmith(
        &dir,
        &["board", "drc", "--kicad-pcb", "bare.kicad_pcb", "--format", "json"],
    );
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    let report = json(&run);
    assert_eq!(report["boards"][0]["board"], "bare");
    let rules: Vec<&str> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["severity"] == "error")
        .map(|f| f["rule"].as_str().unwrap())
        .collect();
    assert_eq!(rules, ["unconnected_items"; 3]);

    // A file that is not there could not be checked: exit 2, not a finding.
    let run = netsmith(&dir, &["board", "drc", "--kicad-pcb", "nothing.kicad_pcb"]);
    assert_eq!(run.status.code(), Some(2), "{}", text(&run));
    assert!(text(&run).contains("nothing.kicad_pcb"), "{}", text(&run));
}
