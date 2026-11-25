//! `netsmith board resistance` (issue #1703) on the LED example with one track drawn between the
//! two pads of VCC: the resistance worked out by hand, and the track's ends at the pads in the
//! frame asked for.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_testkit::TempDir;
use serde_json::Value;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn json(dir: &Path, args: &[&str]) -> Value {
    let run = netsmith(dir, args);
    assert!(run.status.success(), "netsmith {}: {run:?}", args.join(" "));
    serde_json::from_slice(&run.stdout).unwrap()
}

fn point(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}

#[test]
fn a_track_between_two_pads_has_its_resistance_and_its_places_in_the_frame_asked_for() {
    let dir = TempDir::new("resistance-led");
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    let run = netsmith(&dir, &["layout", "capture", "--file", "led.nsm"]);
    assert!(run.status.success(), "{run:?}");
    let parts = json(&dir, &["layout", "parts", "--file", "led.nsm", "--format", "json"]);
    // The two pads of VCC, by part and number, with their places in the outline's frame.
    let mut vcc: Vec<(String, [f64; 2])> = Vec::new();
    for part in parts["parts"].as_array().unwrap() {
        for pad in part["pads"].as_array().unwrap() {
            if pad["net"] == "VCC" {
                let name = format!(
                    "{}.{}",
                    part["refdes"].as_str().unwrap(),
                    pad["number"].as_str().unwrap()
                );
                vcc.push((name, point(&pad["at"])));
            }
        }
    }
    assert_eq!(vcc.len(), 2, "{parts:#}");
    let (a, b) = (vcc[0].1, vcc[1].1);
    let (from, to) = (format!("{},{}", a[0], a[1]), format!("{},{}", b[0], b[1]));
    let run = netsmith(
        &dir,
        &[
            "routes", "add-track", "--file", "led.nsm", "--net", "VCC", "--layer", "top",
            "--width", "0.3", "--from", &from, "--to", &to,
        ],
    );
    assert!(run.status.success(), "{run:?}");
    let length = (b[0] - a[0]).hypot(b[1] - a[1]);
    // 1 oz of copper, 0.035 mm, at 20 °C.
    let by_hand = 1.72e-5 * length / (0.3 * 0.035);
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let doc = json(
            &dir,
            &[
                "board", "resistance", "VCC", &vcc[0].0, &vcc[1].0, "--file", "led.nsm",
                "--amps", "0.5", "--format", "json", "--frame", frame,
            ],
        );
        assert_eq!(doc["frame"], frame);
        let r = doc["resistance"].as_f64().unwrap();
        assert!((r - by_hand).abs() < 1e-9, "{r} is not {by_hand}: {doc:#}");
        assert!((doc["drop"].as_f64().unwrap() - 0.5 * by_hand).abs() < 1e-9);
        let track = &doc["segments"][0];
        assert_eq!(track["kind"], "track");
        for (got, want) in [(point(&track["from"]), a), (point(&track["to"]), b)] {
            assert!(
                (got[0] - want[0] - by).abs() < 1e-6 && (got[1] - want[1] - by).abs() < 1e-6,
                "{frame}: {got:?} is not {want:?} moved by {by}"
            );
        }
    }
    let run = netsmith(
        &dir,
        &["board", "resistance", "VCC", &vcc[0].0, &vcc[1].0, "--file", "led.nsm"],
    );
    let text = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success() && text.contains("net VCC from "), "{run:?}");
    // An end that is no pad of the net is an error, exit 1.
    let run = netsmith(&dir, &["board", "resistance", "VCC", &vcc[0].0, "Q9", "--file", "led.nsm"]);
    assert_eq!(run.status.code(), Some(1), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stderr).contains("Q9 is no pad of net VCC"), "{run:?}");
}
