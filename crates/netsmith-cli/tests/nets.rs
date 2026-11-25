//! `netsmith board nets` (issue #1223): the table of a board's nets and diff pairs, from the
//! copper `view pcb` draws, routed first with `--route`.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

/// Two headers joined pin to pin, the two lines stated a diff pair with a `DiffPair`.
const LINK: &str = r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::DiffPair

/// Two lines of a link that the board routes as a pair, with no impedance stated.
board Link {
    .process: TwoLayer
    .process.stackup.thickness &= 1.6mm
    .near: Header1x2
    .far: Header1x2
    .lines: DiffPair
    .near.p1 ~ .far.p1
    .near.p2 ~ .far.p2
    .lines.p ~ .near.p1
    .lines.n ~ .near.p2
}
"#;

fn netsmith(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("NETSMITH_NO_KICAD", "1")
        .output()
        .unwrap()
}

fn json(cwd: &Path, args: &[&str]) -> serde_json::Value {
    let run = netsmith(cwd, &[args, &["--format", "json"]].concat());
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    // One JSON document on stdout, and nothing else there.
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        )
    })
}

#[test]
fn the_routed_pair_has_both_lengths_and_its_skew_within_the_target() {
    let dir = TempDir::new("board-nets");
    fs::write(dir.join("link.nsm"), LINK).unwrap();
    // Nothing routed: no net has copper, so the table is empty.
    let bare = json(&dir, &["board", "nets", "link.nsm"]);
    assert_eq!(bare["board"], "Link");
    assert_eq!(bare["nets"].as_array().unwrap().len(), 0, "{bare}");
    assert_eq!(bare["pairs"].as_array().unwrap().len(), 0, "{bare}");
    let routed = json(&dir, &["board", "nets", "link.nsm", "--route"]);
    let nets = routed["nets"].as_array().unwrap();
    assert_eq!(nets.len(), 2, "{routed}");
    for n in nets {
        assert!(n["length"].as_f64().unwrap() > 0.0, "{n}");
        assert!(n["ratio"].as_f64().unwrap() >= 1.0 - 1e-9, "{n}");
        assert_eq!(n["layer_changes"].as_u64(), Some(0), "{n}");
    }
    let pairs = routed["pairs"].as_array().unwrap();
    assert_eq!(pairs.len(), 1, "{routed}");
    let pair = &pairs[0];
    println!("{pair}");
    let (lp, ln) = (
        pair["length_p"].as_f64().unwrap(),
        pair["length_n"].as_f64().unwrap(),
    );
    // The pair's two lengths are the two nets' rows, and its skew their difference.
    let length = |net: &serde_json::Value| -> f64 {
        nets.iter().find(|n| &n["net"] == net).unwrap()["length"]
            .as_f64()
            .unwrap()
    };
    assert!((length(&pair["p"]) - lp).abs() < 1e-9 && (length(&pair["n"]) - ln).abs() < 1e-9);
    assert!((pair["skew"].as_f64().unwrap() - (lp - ln).abs()).abs() < 1e-9);
    assert_eq!(pair["over"], false, "{pair}");
    assert_eq!(pair["limit"].as_f64(), Some(0.1));
    // The coupled length is a share of the first net's (the router's tests measure how much).
    let (coupled, share) = (
        pair["coupled"].as_f64().unwrap(),
        pair["share"].as_f64().unwrap(),
    );
    assert!((coupled - share * lp).abs() < 1e-6, "{pair}");
    // The text has a row for each net, then the pair.
    let text = netsmith(&dir, &["board", "nets", "link.nsm", "--route", "--lengths"]);
    let text = String::from_utf8_lossy(&text.stdout).into_owned();
    assert!(text.starts_with("Link:\nnet "), "{text}");
    assert!(text.contains("pair "), "{text}");
    assert!(text.contains("within the 0.1 mm limit"), "{text}");
}

/// Four lines from one header to another, the outer two crossed, stated a length group
/// (issue #1252): the line on the back is longer than the three on the front.
const LANE: &str = r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x4
import std::sources::LengthGroup

/// Four lines of a parallel bus, matched in length.
board Lane {
    .process: TwoLayer
    .process.stackup.thickness &= 1.6mm
    .near: Header1x4
    .far: Header1x4
    .lane: LengthGroup { .tolerance &= 0mm..0.2mm }
    .near.p1 ~ .far.p4
    .near.p2 ~ .far.p2
    .near.p3 ~ .far.p3
    .near.p4 ~ .far.p1
    .lane.lines[0] ~ .near.p1
    .lane.lines[1] ~ .near.p2
    .lane.lines[2] ~ .near.p3
    .lane.lines[3] ~ .near.p4
}
"#;

/// The spread of the routed lengths of `nets` of a `board nets` table.
fn spread(table: &serde_json::Value) -> f64 {
    let lengths: Vec<f64> = table["nets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["length"].as_f64().unwrap())
        .collect();
    let max = lengths.iter().copied().fold(f64::MIN, f64::max);
    let min = lengths.iter().copied().fold(f64::MAX, f64::min);
    max - min
}

#[test]
fn a_length_group_the_source_states_is_lengthened_by_the_router_and_listed() {
    let dir = TempDir::new("board-nets-group");
    // The same board with no group: its lines are not matched.
    let bare: String = LANE
        .lines()
        .filter(|l| !l.contains("lane"))
        .map(|l| format!("{l}\n"))
        .collect();
    fs::write(dir.join("bare.nsm"), bare).unwrap();
    let unmatched = json(&dir, &["board", "nets", "bare.nsm", "--route"]);
    assert_eq!(unmatched["groups"].as_array().unwrap().len(), 0, "{unmatched}");
    let before = spread(&unmatched);
    println!("spread with no group: {before} mm");
    assert!(before > 0.2, "{unmatched}");

    fs::write(dir.join("lane.nsm"), LANE).unwrap();
    let routed = json(&dir, &["board", "nets", "lane.nsm", "--route"]);
    let groups = routed["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "{routed}");
    let group = &groups[0];
    println!("{group}");
    assert_eq!(group["path"], "lane");
    assert_eq!(group["tolerance"].as_f64(), Some(0.2));
    assert_eq!(group["over"], false, "{group}");
    // Its nets in the order of its lines, the nets each line is joined to.
    let nets: Vec<&str> = group["nets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["net"].as_str().unwrap())
        .collect();
    assert_eq!(nets.len(), 4, "{group}");
    assert!(group["spread"].as_f64().unwrap() <= 0.2, "{group}");
    assert!((spread(&routed) - group["spread"].as_f64().unwrap()).abs() < 1e-9);
    let text = netsmith(&dir, &["board", "nets", "lane.nsm", "--route"]);
    let text = String::from_utf8_lossy(&text.stdout).into_owned();
    assert!(text.contains("group lane: "), "{text}");
    assert!(text.contains("within the 0.2 mm tolerance"), "{text}");
}

/// `board nets --power-stage` (issue #1585) of the buck example: its IC is recognised as a
/// switching regulator (a provider of `BuckIc`) with its nets by role, its input capacitors, the
/// capacitors on its other pins and its feedback net; nothing is routed, so no way has a track
/// and no copper of another net is near the switch node, and `board drc` notes none.
#[test]
fn the_power_stage_of_a_buck_names_its_roles_and_capacitors() {
    let dir = TempDir::new("board-nets-power-stage");
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for f in ["buck_converter.nsm", "buck_converter.lock.toml"] {
        fs::copy(examples.join(f), dir.join(f)).unwrap();
    }
    let out = json(&dir, &["board", "nets", "buck_converter.nsm", "--power-stage"]);
    println!("{out}");
    let stages = out["power_stages"].as_array().unwrap();
    assert_eq!(stages.len(), 1, "{out}");
    let s = &stages[0];
    assert!(s["path"].as_str().unwrap().ends_with("ic"), "{s}");
    for role in ["vin", "gnd", "sw"] {
        assert!(!s[role].as_str().unwrap().is_empty(), "{s}");
    }
    let caps = s["input_caps"].as_array().unwrap();
    assert!(!caps.is_empty(), "{s}");
    assert!(caps[0]["vin"]["distance"].as_f64().unwrap() > 0.0, "{s}");
    assert!(caps[0]["vin"]["routed"].is_null(), "{s}");
    assert!(s["feedback"]["net"].is_string(), "{s}");
    assert_eq!(s["foreign"].as_array().unwrap().len(), 0, "{s}");
    let text = netsmith(&dir, &["board", "nets", "buck_converter.nsm", "--power-stage"]);
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("power stage:") && text.contains("input "), "{text}");
    let drc = netsmith(&dir, &["board", "drc", "buck_converter.nsm"]);
    let drc = String::from_utf8_lossy(&drc.stdout);
    assert!(!drc.contains("switch_node_copper"), "{drc}");
}

/// `board nets --decoupling` (issue #1730) of the regulator example: both capacitors are found
/// with the regulator they decouple, routed on tracks; moved farther from its pins, the output
/// capacitor has the longer loop, and `--compare` with the JSON kept before says so, in its own
/// frame or the outline's.
#[test]
fn a_decoupling_capacitor_moved_farther_from_its_pin_has_the_longer_loop_in_the_compare() {
    let dir = TempDir::new("board-nets-decoupling");
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    for f in ["linear_regulator.nsm", "linear_regulator.lock.toml"] {
        fs::copy(examples.join(f), dir.join(f)).unwrap();
    }
    let run = |args: &[&str]| {
        let mut all = vec!["board", "nets", "linear_regulator.nsm", "--decoupling", "--route"];
        all.extend_from_slice(args);
        netsmith(&dir, &all)
    };
    let ok = |out: &std::process::Output| {
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    // The placement held, so the move is the only change.
    ok(&netsmith(&dir, &["layout", "capture", "--file", "linear_regulator.nsm"]));
    let before = ok(&run(&["--format", "json"]));
    fs::write(dir.join("before.json"), &before).unwrap();
    let before: serde_json::Value = serde_json::from_str(&before).unwrap();
    assert_eq!(before["frame"], "board-file", "{before}");
    let caps = before["decoupling"].as_array().unwrap();
    assert_eq!(caps.len(), 2, "{before}");
    let loop_of = |doc: &serde_json::Value, path: &str| -> serde_json::Value {
        doc["decoupling"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["path"] == path)
            .unwrap_or_else(|| panic!("no {path}: {doc}"))
            .clone()
    };
    let near = loop_of(&before, "stage.output_cap");
    assert!(near["part"].as_str().unwrap().starts_with('U'), "{near}");
    assert_eq!(near["loop"]["complete"], true, "{near}");
    let cap = near["cap"].as_str().unwrap().to_string();

    ok(&netsmith(
        &dir,
        &[
            "layout", "set", "stage.output_cap", "--at", "16.75,3.5", "--lock", "--file",
            "linear_regulator.nsm",
        ],
    ));
    let after: serde_json::Value =
        serde_json::from_str(&ok(&run(&["--compare", "before.json", "--format", "json"]))).unwrap();
    let far = loop_of(&after, "stage.output_cap");
    let length = |l: &serde_json::Value| l["loop"]["length"].as_f64().unwrap();
    assert!(length(&far) > length(&near) + 2.0, "{near}\n{far}");
    assert!(
        far["supply"]["distance"].as_f64().unwrap() > near["supply"]["distance"].as_f64().unwrap(),
        "{far}"
    );
    let rows = after["compare"]["loops"].as_array().unwrap();
    let row = rows.iter().find(|r| r["cap"] == cap.as_str()).unwrap();
    assert!(row["length_change"].as_f64().unwrap() > 2.0, "{row}");
    assert_eq!(after["compare"]["against"], "before.json");
    // The text says it too, and the outline's frame moves the capacitor's place.
    let text = ok(&run(&["--compare", "before.json"]));
    assert!(text.contains("decoupling loops compared with before.json"), "{text}");
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("{cap}: loop ")))
        .unwrap_or_else(|| panic!("{text}"));
    assert!(line.contains(" mm (+"), "{line}");
    let framed: serde_json::Value =
        serde_json::from_str(&ok(&run(&["--format", "json", "--frame", "outline"]))).unwrap();
    assert_eq!(framed["frame"], "outline");
    let at = |l: &serde_json::Value| l["at"][0].as_f64().unwrap();
    let moved = loop_of(&framed, "stage.output_cap");
    assert!((at(&far) - at(&moved) - 20.0).abs() < 1e-6, "{far}\n{moved}");
    // Not with the power stage's report, nor a gate.
    for other in ["--power-stage", "--gate", "--lengths"] {
        let out = run(&[other]);
        assert_eq!(out.status.code(), Some(2), "{other}");
    }
}
