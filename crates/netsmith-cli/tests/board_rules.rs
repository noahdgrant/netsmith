//! `netsmith board rules` (issue #1513): the rules `board drc` holds a board to, as data. The
//! document of a design's board and of the KiCad board `fab` writes for it give the same numbers
//! of the project and rules files, and those are the numbers in the project file `fab` writes.

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
        .output()
        .unwrap()
}

fn text(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

fn json(run: &Output) -> serde_json::Value {
    assert!(run.status.success(), "{}", text(run));
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(run)))
}

#[test]
fn the_rules_are_the_checks_for_the_design_and_for_the_board_fab_writes() {
    let dir = TempDir::new("board-rules");
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    let ours = json(&netsmith(
        &dir,
        &["board", "rules", "led.nsm", "--format", "json"],
    ));
    assert_eq!(ours["board"], "LedExample");
    assert!(ours["outline_at"]["x"].is_number(), "{ours:#}");
    let nets: Vec<&str> = ours["nets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["net"].as_str().unwrap())
        .collect();
    assert!(nets.contains(&"GND"), "{nets:?}");
    for n in ours["nets"].as_array().unwrap() {
        assert!(
            n["clearance"].as_f64().unwrap() >= ours["clearance"]["least"].as_f64().unwrap(),
            "{n}"
        );
        assert!(
            n["least_width"].as_f64().unwrap() >= ours["widths"]["least"].as_f64().unwrap(),
            "{n}"
        );
    }
    assert_eq!(ours["classes"][0]["name"], "Default");
    assert!(ours["mask"]["to_track"].is_number(), "{ours:#}");
    assert!(ours["pad_to_pad"].is_number(), "{ours:#}");

    // The board `fab` writes, with its project and rules files: the same numbers, and the
    // project file's are the document's.
    let fab = netsmith(&dir, &["board", "fab", "led.nsm", "--locked", "--out", "fab"]);
    assert!(fab.status.success(), "{}", text(&fab));
    let pcb = dir.join("fab/LedExample/LedExample.kicad_pcb");
    let theirs = json(&netsmith(
        &dir,
        &[
            "board",
            "rules",
            "--kicad-pcb",
            pcb.to_str().unwrap(),
            "--format",
            "json",
        ],
    ));
    for key in ["clearance", "copper_to_edge", "pad_to_pad"] {
        assert_eq!(ours[key], theirs[key], "{key}");
    }
    // A class's clearance and nets are checked; the width it draws at is what a net wants,
    // which `fab` may choose wider once it routes, and no rule holds a track to it.
    let checked = |doc: &serde_json::Value| -> Vec<serde_json::Value> {
        doc["classes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| serde_json::json!([c["name"], c["clearance"], c["patterns"], c["priority"]]))
            .collect()
    };
    assert_eq!(checked(&ours), checked(&theirs));
    for key in ["to_copper", "to_track", "plated_pad_to_track", "hole_to_hole"] {
        assert_eq!(ours["holes"][key], theirs["holes"][key], "holes.{key}");
    }
    let pro: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(pcb.with_extension("kicad_pro")).unwrap(),
    )
    .unwrap();
    let rules = &pro["board"]["design_settings"]["rules"];
    assert_eq!(ours["clearance"]["least"], rules["min_clearance"]);
    assert_eq!(ours["widths"]["least"], rules["min_track_width"]);
    assert_eq!(ours["copper_to_edge"], rules["min_copper_edge_clearance"]);

    // The text names each kind of rule.
    let run = netsmith(&dir, &["board", "rules", "led.nsm"]);
    let said = text(&run);
    assert!(run.status.success(), "{said}");
    for word in ["LedExample:", "clearance:", "track width:", "mask:", "classes:", "nets:"] {
        assert!(said.contains(word), "{word}: {said}");
    }
}
