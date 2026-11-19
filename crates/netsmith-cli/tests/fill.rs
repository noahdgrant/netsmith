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
    // The pads no spoke joins are `board drc`'s `unjoinable_thermal` (issue #1222).
    let parts = |all: Vec<String>| {
        let mut all = all;
        all.sort();
        all
    };
    let unjoinable = parts(
        doc["unjoinable"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u["part"].as_str().unwrap().to_string())
            .collect(),
    );
    let found = parts(
        checked["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["rule"] == "unjoinable_thermal")
            .map(|f| f["parts"][0].as_str().unwrap().to_string())
            .collect(),
    );
    assert_eq!(unjoinable, found);
    assert_eq!(doc["counts"]["unjoinable"], unjoinable.len());

    // The counts of the board are the counts of its fills, and each fill's of its pads.
    let fills = doc["fills"].as_array().unwrap();
    let kinds = ["no_spokes", "one_spoke", "two_or_more_spokes", "solid", "none"];
    for kind in kinds.iter().chain(&["starved", "unjoinable"]) {
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
            "starved thermal reliefs (one or more but fewer than 2 spokes and tracks join the pad to the fill): {}\n",
            ours.len()
        )),
        "{said}"
    );
}

/// A hand zone of 3V3 on the bottom of the four-layer example, over the pad J2.1 of its net,
/// in the bottom's GND pour.
const ZONE_3V3: &str = r#"version = 1

[OutlineAndPours.nets.3V3]
pads = "x"

[[OutlineAndPours.nets.3V3.zones]]
layers = ["bottom"]
priority = 0
clearance = 0.3
min_width = 0.25
pads = "thermal"
thermal_gap = 0.5
spoke_width = 0.5
islands = "remove"
outline = [[25.0, 29.0], [30.0, 29.0], [30.0, 32.5], [25.0, 32.5]]
lock = true
"#;

/// The examples' project of four layers, as it is.
fn pours_project(name: &str) -> TempDir {
    let dir = starving_project(name);
    fs::write(
        dir.join("main.nsm"),
        include_str!("../../../examples/outline_and_pours.nsm"),
    )
    .unwrap();
    dir
}

/// The GND findings of `board drc --format json`, by rule.
fn gnd_findings(dir: &Path) -> Vec<String> {
    let doc = json(dir, &["board", "drc", "main.nsm"]);
    let mut out: Vec<String> = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["nets"].as_array().is_some_and(|n| n.iter().any(|n| n == "GND")))
        .map(|f| f["rule"].as_str().unwrap().to_string())
        .collect();
    out.sort();
    out
}

#[test]
fn a_hand_zone_of_another_net_cuts_the_pour_and_leaves_it_joining_its_pads() {
    // Issue #1314: a hand zone of 3V3 on the bottom took the bottom's GND pour away, which
    // opened GND's connections. The pour stays round the zone.
    let dir = pours_project("fill-zone-in-pour");
    let fills = |doc: &serde_json::Value| -> Vec<(String, String, String, f64)> {
        doc["fills"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["layer"] == "bottom")
            .map(|f| {
                (
                    f["net"].as_str().unwrap().to_string(),
                    f["source"].as_str().unwrap().to_string(),
                    f["pads"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|p| format!("{}.{}", p["part"].as_str().unwrap(), p["pad"].as_str().unwrap()))
                        .collect::<Vec<_>>()
                        .join(" "),
                    f["area"].as_f64().unwrap(),
                )
            })
            .collect()
    };
    let before = fills(&json(&dir, &["board", "fill", "main.nsm"]));
    let drc_before = gnd_findings(&dir);
    fs::write(dir.join("main.routes.toml"), ZONE_3V3).unwrap();
    let after = fills(&json(&dir, &["board", "fill", "main.nsm"]));
    assert_eq!(before.len(), 1, "{before:?}");
    assert_eq!(after.len(), 2, "{after:?}");
    let (pour, zone) = (&after[0], &after[1]);
    // The pour is cut where the zone has the area, and joins the pads it joined before.
    assert_eq!((pour.0.as_str(), pour.1.as_str()), ("GND", "pour"), "{after:?}");
    assert_eq!(pour.2, before[0].2);
    // The zone is 17.5 mm² and has J2.1, round which the pour kept its clearance already.
    let lost = before[0].3 - pour.3;
    assert!(lost > 12.0 && lost < 30.0, "{before:?} {after:?}");
    assert_eq!((zone.0.as_str(), zone.1.as_str(), zone.2.as_str()), ("3V3", "zone", "J2.1"));
    // The design rules check finds nothing more of GND than it did with no zone.
    assert_eq!(gnd_findings(&dir), drc_before);
    let said = String::from_utf8_lossy(&netsmith(&dir, &["board", "copper", "main.nsm"]).stdout).into_owned();
    assert!(said.contains("(cut by 1 hand zone(s) of other nets, drawn round them)"), "{said}");
}

/// The area of a ring by the shoelace, in mm².
fn ring_area(ring: &serde_json::Value) -> f64 {
    let p: Vec<(f64, f64)> = ring
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["x"].as_f64().unwrap(), v["y"].as_f64().unwrap()))
        .collect();
    (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

#[test]
fn each_fill_has_its_outline_with_holes_and_its_area_is_the_outlines() {
    // Issue #1293: the polygons, with holes, of each layer's fill.
    let dir = starving_project("fill-outlines");
    let doc = json(&dir, &["board", "fill", "main.nsm"]);
    for f in doc["fills"].as_array().unwrap() {
        let pieces = f["outline"].as_array().unwrap();
        assert!(!pieces.is_empty(), "{}", f["layer"]);
        let area: f64 = pieces
            .iter()
            .map(|p| {
                let holes: f64 = p["holes"]
                    .as_array()
                    .map_or(0.0, |h| h.iter().map(ring_area).sum());
                ring_area(&p["outer"]) - holes
            })
            .sum();
        let want = f["area"].as_f64().unwrap();
        assert!((area - want).abs() < want * 0.001, "{}: {area} against {want}", f["layer"]);
    }
    // The top pour has holes round the pads of other nets it keeps clear of.
    let top = doc["fills"].as_array().unwrap().iter().find(|f| f["layer"] == "top").unwrap();
    assert!(top["outline"].as_array().unwrap().iter().any(|p| p["holes"].as_array().is_some_and(|h| !h.is_empty())));
}

/// The starved and unjoinable pads of `board fill`, by name.
fn unjoined(dir: &Path) -> Vec<String> {
    let doc = json(dir, &["board", "fill", "main.nsm"]);
    let mut all: Vec<String> = ["starved", "unjoinable"]
        .iter()
        .flat_map(|k| doc[k].as_array().unwrap().clone())
        .map(|p| format!("{}.{}", p["part"].as_str().unwrap(), p["pad"].as_str().unwrap()))
        .collect();
    all.sort();
    all
}

#[test]
fn join_pad_joins_each_starved_and_unjoinable_pad_with_a_short_track_and_says_a_joined_one() {
    let dir = starving_project("join-pad");
    let pads = unjoined(&dir);
    assert!(pads.len() >= 2, "the 1.5 mm clearance leaves pads unjoined: {pads:?}");
    let drc = |dir: &Path| {
        let doc = json(dir, &["board", "drc", "main.nsm"]);
        let mut errors: Vec<String> = doc["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["severity"] == "error")
            .map(|f| f["rule"].as_str().unwrap().to_string())
            .collect();
        errors.sort();
        errors
    };
    let errors = drc(&dir);

    // A dry run says the track and writes nothing.
    let dry = json(&dir, &["routes", "join-pad", &pads[0], "--file", "main.nsm", "--dry-run"]);
    assert_eq!((&dry["written"], &dry["already"]), (&false.into(), &false.into()), "{dry}");
    assert!(dry["join"]["length"].as_f64().unwrap() <= 5.0, "{dry}");
    assert!(!dir.join("main.routes.toml").exists());

    for pad in &pads {
        let run = netsmith(&dir, &["routes", "join-pad", pad, "--file", "main.nsm"]);
        let said = String::from_utf8_lossy(&run.stdout);
        assert!(run.status.success(), "{pad}: {said}{}", String::from_utf8_lossy(&run.stderr));
        assert!(said.contains(" into the fill on top\n"), "{said}");
        assert!(said.contains("wrote main.routes.toml"), "{said}");
    }
    assert_eq!(unjoined(&dir), Vec::<String>::new());
    assert_eq!(drc(&dir), errors, "a join adds no error of the design rules check");
    let file = fs::read_to_string(dir.join("main.routes.toml")).unwrap();
    assert_eq!(file.matches("layer = \"top\"").count(), pads.len(), "{file}");

    // Joined now: said, and nothing written.
    let again = json(&dir, &["routes", "join-pad", &pads[0], "--file", "main.nsm"]);
    assert_eq!((&again["already"], &again["written"]), (&true.into(), &false.into()), "{again}");
    assert_eq!(fs::read_to_string(dir.join("main.routes.toml")).unwrap(), file);
}

#[test]
fn join_pad_takes_a_via_into_a_plane_when_the_pads_layer_has_no_fill_and_refuses_a_net_with_none() {
    let dir = starving_project("join-pad-via");
    // No pour on top: the ground pads there are joined only through a via to the plane.
    let source = fs::read_to_string(dir.join("main.nsm"))
        .unwrap()
        .replace("        .top.net &= \"GND\"\n", "");
    fs::write(dir.join("main.nsm"), source).unwrap();
    let doc = json(&dir, &["routes", "join-pad", "C1.2", "--file", "main.nsm"]);
    assert_eq!(doc["written"], true, "{doc}");
    assert_eq!((&doc["join"]["layer"], &doc["join"]["fill_layer"]), (&"top".into(), &"inner1".into()), "{doc}");
    assert_eq!(doc["join"]["via"], doc["join"]["to"]);
    let checked = json(&dir, &["board", "drc", "main.nsm"]);
    let c1_2_open = checked["findings"].as_array().unwrap().iter().any(|f| {
        f["rule"] == "unconnected_items" && f["message"].as_str().unwrap().contains("pad 2 [GND] of C1")
    });
    assert!(!c1_2_open, "{checked}");

    // A net with no fill, and a part the board has not.
    for (pad, says) in [("C1.1", "net VIN has no pour, plane or zone"), ("ZZ9.1", "has no part ZZ9")] {
        let run = netsmith(&dir, &["routes", "join-pad", pad, "--file", "main.nsm"]);
        assert_eq!(run.status.code(), Some(1), "{pad}");
        assert!(String::from_utf8_lossy(&run.stderr).contains(says), "{pad}: {}", String::from_utf8_lossy(&run.stderr));
    }

}
