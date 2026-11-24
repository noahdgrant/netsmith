//! `netsmith layout parts --format json` gives a `round_rect` pad's corner radius and the ratio
//! it came from, and says the shape of the others (issue #1626). The footprints with an explicit
//! `roundrect_rratio` and with none are read in `netsmith-pcb`'s `part_geometry` tests; here the
//! command prints the document.

use std::fs;
use std::path::Path;
use std::process::Command;

use netsmith_testkit::TempDir;
use serde_json::Value;

#[test]
fn a_round_rect_pad_has_its_radius_and_ratio_and_another_shape_has_neither() {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let dir = TempDir::new("layout-parts-pads");
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(examples.join(name), dir.join(name)).unwrap();
    }
    let run = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["layout", "parts", "--file", "led.nsm", "--format", "json"])
        .current_dir(&dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap();
    assert!(run.status.success(), "{run:?}");
    let doc: Value = serde_json::from_slice(&run.stdout).unwrap();
    let mut round = 0;
    for part in doc["parts"].as_array().unwrap() {
        for pad in part["pads"].as_array().unwrap() {
            if pad["shape"] == "round_rect" {
                round += 1;
                // The generated land of a passive states no ratio: KiCad's 0.25 of the shorter
                // side.
                let (w, h) = (pad["width"].as_f64().unwrap(), pad["height"].as_f64().unwrap());
                assert_eq!(pad["corner_ratio"], 0.25, "{pad}");
                let radius = pad["corner_radius"].as_f64().unwrap();
                assert!((radius - 0.25 * w.min(h)).abs() <= 1e-4, "{pad}");
            } else {
                for key in ["corner_radius", "corner_ratio", "anchor_shape"] {
                    assert!(pad.get(key).is_none(), "{key}: {pad}");
                }
            }
        }
    }
    assert!(round > 0, "the board has no round_rect pad: {doc}");
}
