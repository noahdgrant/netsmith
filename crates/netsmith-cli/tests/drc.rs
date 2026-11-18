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
    assert!(
        !said
            .lines()
            .any(|l| l.starts_with("timing:") && l.contains("route")),
        "{said}"
    );
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

#[test]
fn fab_writes_the_processs_rules_beside_the_project_and_a_kicad_boards_check_reads_them() {
    // Issue #1185: the project file's least clearance is 0.1 mm, the process's space between
    // surface-mount pads 0.15 mm, which the `.kicad_dru` holds for KiCad and for this check.
    let dir = led("drc-rules-file");
    ok(&netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "first"]));
    let file = dir.join("first/LedExample/LedExample.kicad_dru");
    let rules = fs::read_to_string(&file).unwrap();
    assert!(
        rules.contains("(rule \"netsmith pad_to_pad\"\n\t(constraint clearance (min 0.15mm))"),
        "{rules}"
    );
    let pcb = "first/LedExample/LedExample.kicad_pcb";
    ok(&netsmith(&dir, &["board", "drc", "--kicad-pcb", pcb]));
    // A process that asked 1 mm between the pads would refuse the LED's and the resistor's.
    fs::write(&file, rules.replace("(min 0.15mm)", "(min 1mm)")).unwrap();
    let run = netsmith(&dir, &["board", "drc", "--kicad-pcb", pcb, "--format", "json"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    let report = json(&run);
    let pads: Vec<&serde_json::Value> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["rule"] == "pad_to_pad")
        .collect();
    assert_eq!(pads.len(), 2, "{report:#}");
    assert!(
        pads.iter().all(|f| f["message"].as_str().unwrap().contains("1.0000 mm")),
        "{pads:#?}"
    );
}

/// A board drawn by hand: a crystal's line XOUT between two pads 4 mm apart, routed 10 mm out
/// and back through two vias and the back, 14 mm in all.
const DETOUR: &str = "(kicad_pcb
	(version 20241229)
	(generator \"netsmith-test\")
	(general (thickness 1.6) (legacy_teardrops no))
	(paper \"A4\")
	(layers
		(0 \"F.Cu\" signal)
		(2 \"B.Cu\" signal)
		(1 \"F.Mask\" user)
		(3 \"B.Mask\" user)
		(25 \"Edge.Cuts\" user)
	)
	(setup (pad_to_mask_clearance 0))
	(net 0 \"\")
	(net 1 \"XOUT\")
	(footprint \"Test:Y1\" (layer \"F.Cu\") (at 10 20 0)
		(property \"Reference\" \"Y1\" (at 0 -3 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))
		(attr smd)
		(pad \"1\" smd rect (at 0 0) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\") (net 1 \"XOUT\"))
	)
	(footprint \"Test:U1\" (layer \"F.Cu\") (at 14 20 0)
		(property \"Reference\" \"U1\" (at 0 -3 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))
		(attr smd)
		(pad \"1\" smd rect (at 0 0) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\") (net 1 \"XOUT\"))
	)
	(segment (start 10 20) (end 10 25) (width 0.2) (layer \"F.Cu\") (net 1))
	(via (at 10 25) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"B.Cu\") (net 1))
	(segment (start 10 25) (end 14 25) (width 0.2) (layer \"B.Cu\") (net 1))
	(via (at 14 25) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"B.Cu\") (net 1))
	(segment (start 14 25) (end 14 20) (width 0.2) (layer \"F.Cu\") (net 1))
	(gr_line (start 0 0) (end 30 0) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))
	(gr_line (start 30 0) (end 30 30) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))
	(gr_line (start 30 30) (end 0 30) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))
	(gr_line (start 0 30) (end 0 0) (stroke (width 0.05) (type default)) (layer \"Edge.Cuts\"))
)
";

#[test]
fn its_notes_list_the_nets_routed_long_and_the_nets_with_the_most_vias() {
    // Issue #1194.
    let dir = TempDir::new("drc-net-routes");
    fs::write(dir.join("detour.kicad_pcb"), DETOUR).unwrap();
    let run = netsmith(&dir, &["board", "drc", "--kicad-pcb", "detour.kicad_pcb"]);
    let said = text(&run);
    let printed = out(&run);
    let notes: Vec<&str> = printed
        .lines()
        .filter(|l| l.starts_with("detour: note: ") || l.starts_with("  XOUT"))
        .collect();
    assert_eq!(
        notes,
        [
            "detour: note: routed long: 1 net routed more than 2x the straight length between its pads and 2 mm over it; look at each that must be short (a crystal's, a clock's, a switching node's)",
            "  XOUT: 14 mm routed, straight 4 mm (3.5x), 2 vias",
            "detour: note: most vias (nets no zone pours, 2 or more): XOUT 2 vias (14 mm routed, straight 4 mm)",
        ],
        "{said}"
    );
    let run = netsmith(
        &dir,
        &["board", "drc", "--kicad-pcb", "detour.kicad_pcb", "--format", "json"],
    );
    let report = json(&run);
    let routes = &report["boards"][0]["routes"];
    assert_eq!(routes["long"][0]["net"], "XOUT", "{report:#}");
    assert_eq!(routes["long"][0]["length"], 14.0);
    assert_eq!(routes["long"][0]["straight"], 4.0);
    assert_eq!(routes["long"][0]["ratio"], 3.5);
    assert_eq!(routes["long"][0]["vias"], 2);
    assert_eq!(routes["most_vias"][0]["net"], "XOUT");
    // The notes are no findings: the check of a board with a long net passes or fails as before.
    assert!(
        !report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["message"].as_str().unwrap().contains("routed long")),
        "{report:#}"
    );
}

