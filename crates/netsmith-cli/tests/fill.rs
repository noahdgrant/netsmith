//! `netsmith board fill` (issue #1187): how the fill of a board's pours joins each pad, from the
//! fill the Gerbers hold, with the starved thermal reliefs `board drc` finds.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

/// A project folder with the four-layer example (ground pours on both outer layers, ground and
/// 3.3 V planes), its top pour's clearance raised to 1.5 mm so that it cuts the spokes of the
/// ground pads beside other nets' pads.
fn starving_project(name: &str) -> TempDir {
    let dir = TempDir::new(name);
    let source = include_str!("../../../examples/outline_and_pours.nsm").replace(
        "        .top.stitch_pitch &= 0mm..8mm\n",
        "        .top.stitch_pitch &= 0mm..8mm\n        .top.clearance &= 1.5mm\n",
    );
    assert!(source.contains(".top.clearance"), "the example's pour moved");
    for (file, text) in [
        ("main.nsm", source.as_str()),
        (
            "main.physical.nsm",
            include_str!("../../../examples/outline_and_pours.physical.nsm"),
        ),
        (
            "main.pcb.toml",
            include_str!("../../../examples/outline_and_pours.pcb.toml"),
        ),
        (
            "main.lock.toml",
            include_str!("../../../examples/outline_and_pours.lock.toml"),
        ),
    ] {
        fs::write(dir.join(file), text).unwrap();
    }
    dir
}

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
    // One JSON document on stdout, and nothing else there.
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        )
    })
}

/// A place to the micrometre, to compare places of two documents.
fn place(v: &serde_json::Value) -> (i64, i64) {
    let mm = |v: &serde_json::Value| (v.as_f64().unwrap() * 1e3).round() as i64;
    (mm(&v["x"]), mm(&v["y"]))
}

#[test]
fn the_starved_pads_are_the_ones_board_drc_finds_and_the_counts_add_up() {
    let dir = starving_project("fill-starved");
    let doc = json(&dir, &["board", "fill", "main.nsm"]);
    assert_eq!(doc["board"], "OutlineAndPours");
    let mut ours: Vec<(String, (i64, i64))> = doc["starved"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["part"].as_str().unwrap().to_string(), place(&s["at"])))
        .collect();
    ours.sort();
    assert!(!ours.is_empty(), "the 1.5 mm clearance starves a pad: {doc}");
    assert_eq!(doc["counts"]["starved"], ours.len());

    let checked = json(&dir, &["board", "drc", "main.nsm"]);
    let mut theirs: Vec<(String, (i64, i64))> = checked["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["rule"] == "starved_thermal")
        .map(|f| (f["parts"][0].as_str().unwrap().to_string(), place(&f["place"])))
        .collect();
    theirs.sort();
    assert_eq!(ours, theirs);

    // The counts of the board are the counts of its fills, and each fill's of its pads.
    let fills = doc["fills"].as_array().unwrap();
    let kinds = ["no_spokes", "one_spoke", "two_or_more_spokes", "solid", "none"];
    for kind in kinds.iter().chain(&["starved"]) {
        let sum: u64 = fills.iter().map(|f| f["counts"][kind].as_u64().unwrap()).sum();
        assert_eq!(doc["counts"][kind].as_u64(), Some(sum), "{kind}");
    }
    for f in fills {
        let pads: u64 = kinds.iter().map(|k| f["counts"][k].as_u64().unwrap()).sum();
        assert_eq!(pads as usize, f["pads"].as_array().unwrap().len(), "{f}");
    }
    let top = fills.iter().find(|f| f["layer"] == "top").unwrap();
    assert_eq!((&top["source"], &top["net"]), (&"pour".into(), &"GND".into()));
    assert!(!top["cleared"].as_array().unwrap().is_empty());
    assert!(fills.iter().any(|f| f["layer"] == "inner1" && f["source"] == "plane"));

    let run = netsmith(&dir, &["board", "fill", "main.nsm"]);
    assert!(run.status.success(), "{}", String::from_utf8_lossy(&run.stderr));
    let said = String::from_utf8_lossy(&run.stdout);
    assert!(said.contains("top pour on GND: "), "{said}");
    assert!(said.contains(" (starved)\n"), "{said}");
    assert!(
        said.contains(&format!(
            "starved thermal reliefs (fewer than 2 spokes and tracks join the pad to the fill): {}\n",
            ours.len()
        )),
        "{said}"
    );
}
