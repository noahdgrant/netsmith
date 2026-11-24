//! `--frame` and the `frame` key (issue #1588): one pad's place is the same in every command
//! that prints it as JSON, in the frame asked for, and every JSON document with places says the
//! frame they are in.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_testkit::TempDir;
use serde_json::Value;

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

/// The JSON document `netsmith <args>` prints, whatever its exit code (`board drc` ends with 1
/// for the connections the board leaves open).
fn json(dir: &Path, args: &[&str]) -> Value {
    let run = netsmith(dir, args);
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "netsmith {}: {e}\n{}{}",
            args.join(" "),
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        )
    })
}

/// A folder with `examples/led.nsm`, its pick lock, the placement captured in its layout file,
/// and a track of VCC from the centre of pad 1 of J1 out 3 mm to the right, ending on nothing:
/// the pad's place in the outline's frame and the track's free end.
fn led(tag: &str) -> (TempDir, [f64; 2], [f64; 2]) {
    let dir = TempDir::new(tag);
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    let run = netsmith(&dir, &["layout", "capture", "--file", "led.nsm"]);
    assert!(run.status.success(), "{run:?}");
    let parts = json(
        &dir,
        &[
            "layout", "parts", "J1", "--file", "led.nsm", "--format", "json",
        ],
    );
    let pad = &parts["parts"][0]["pads"][0];
    assert_eq!(pad["number"], "1", "{parts:#}");
    let at = [pad["at"][0].as_f64().unwrap(), pad["at"][1].as_f64().unwrap()];
    let end = [at[0] + 3.0, at[1]];
    let net = pad["net"].as_str().unwrap();
    let (from, to) = (
        format!("{},{}", at[0], at[1]),
        format!("{},{}", end[0], end[1]),
    );
    let run = netsmith(
        &dir,
        &[
            "routes",
            "add-track",
            "--file",
            "led.nsm",
            "--net",
            net,
            "--layer",
            "top",
            "--width",
            "0.3",
            "--from",
            &from,
            "--to",
            &to,
        ],
    );
    assert!(run.status.success(), "{run:?}");
    (dir, at, end)
}

fn point(v: &Value) -> [f64; 2] {
    [
        v[0].as_f64().or(v["x"].as_f64()).unwrap(),
        v[1].as_f64().or(v["y"].as_f64()).unwrap(),
    ]
}

fn near(a: [f64; 2], b: [f64; 2], what: &str) {
    assert!(
        (a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6,
        "{what}: {a:?} is not {b:?}"
    );
}

#[test]
fn one_pad_is_at_one_place_in_every_command_in_the_frame_asked_for() {
    let (dir, pad, end) = led("frame-one-pad");
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let moved = |p: [f64; 2]| [p[0] + by, p[1] + by];
        let (pad, end) = (moved(pad), moved(end));
        let flag = ["--format", "json", "--frame", frame];
        let with = |args: &[&'static str]| [args, &flag[..]].concat();

        let parts = json(
            &dir,
            &with(&["layout", "parts", "J1", "--file", "led.nsm"]),
        );
        assert_eq!(parts["frame"], frame);
        near(point(&parts["parts"][0]["pads"][0]["at"]), pad, "layout parts");

        let routes = json(&dir, &with(&["routes", "show", "--file", "led.nsm"]));
        assert_eq!(routes["frame"], frame);
        let track = &routes["nets"][0]["tracks"][0];
        near(point(&track["from"]), pad, "routes show");
        near(point(&track["to"]), end, "routes show");

        let nets = json(&dir, &with(&["board", "nets", "led.nsm"]));
        assert_eq!(nets["frame"], frame);
        let piece = &nets["nets"][0]["not_counted"][0]["pieces"][0];
        near(point(&piece["from"]), pad, "board nets");
        near(point(&piece["to"]), end, "board nets");

        // The design rules check finds the track's free end.
        let drc = json(&dir, &with(&["board", "drc", "led.nsm"]));
        assert_eq!(drc["frame"], frame);
        near(
            point(&drc["boards"][0]["outline_at"]),
            [by, by],
            "board drc's outline_at",
        );
        let dangling = drc["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["rule"] == "track_dangling")
            .unwrap_or_else(|| panic!("{drc:#}"));
        near(point(&dangling["place"]), end, "board drc");

        // The hole of the through-hole pad, out of the scene's frame (y up).
        let scene = json(&dir, &with(&["dev", "board3d", "led.nsm"]));
        assert_eq!(scene["frame"], frame);
        let hole = scene["board"]["holes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["refdes"] == "J1" && h["pad"] == "1")
            .unwrap_or_else(|| panic!("{scene:#}"));
        near(point(&hole["from"]), pad, "dev board3d");
        let j1 = scene["parts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["refdes"] == "J1")
            .unwrap();
        near(point(&j1["layout"]), pad, "dev board3d's layout place");
        near(
            point(&j1["body"]["min"]),
            point(&parts["parts"][0]["body"]["min"]),
            "dev board3d's body",
        );
    }
}

#[test]
fn every_json_document_with_places_says_their_frame() {
    let (dir, _, _) = led("frame-every-document");
    let out = dir.join("out");
    let out = out.to_str().unwrap();
    let kicad = dir.join("led.kicad_pcb");
    let kicad = kicad.to_str().unwrap();
    let f = "led.nsm";
    let own: &[(&[&str], &str)] = &[
        (&["board", "nets", f], "board-file"),
        (&["board", "drc", f], "board-file"),
        (&["board", "fill", f], "board-file"),
        (&["board", "rules", f], "board-file"),
        (&["board", "check", f], "outline"),
        (&["board", "build", f, "--out", out], "outline"),
        (&["layout", "show", "--file", f], "outline"),
        (&["layout", "set", "power_in", "--turn", "0", "--file", f], "outline"),
        (&["layout", "check", "--file", f], "outline"),
        (&["layout", "summary", "--file", f], "outline"),
        (&["layout", "compare", "led.pcb.toml", "--file", f], "outline"),
        (&["layout", "capture", "--dry-run", "--file", f], "outline"),
        (&["layout", "regenerate", "led", "--file", f], "outline"),
        (&["layout", "fit", "D1", "--file", f], "outline"),
        (&["layout", "parts", "--file", f], "outline"),
        (&["layout", "export", "--kicad-pcb", kicad, "--file", f], "outline"),
        (
            &["layout", "import", kicad, "--dry-run", "--file", f],
            "outline",
        ),
        (&["routes", "show", "--file", f], "outline"),
        (&["routes", "tidy", "--dry-run", "--file", f], "outline"),
    ];
    for (args, frame) in own {
        let doc = json(&dir, &[args, &["--format", "json"][..]].concat());
        assert_eq!(doc["frame"], *frame, "netsmith {}", args.join(" "));
    }
    // The 3D scene says its own frame in prose, as it always has.
    let scene = json(&dir, &["dev", "board3d", f, "--format", "json"]);
    assert!(
        scene["frame"].as_str().unwrap().contains("z up"),
        "{}",
        scene["frame"]
    );
    // `--frame` is for the JSON; the text says its frame.
    let run = netsmith(&dir, &["board", "nets", f, "--frame", "outline"]);
    assert_eq!(run.status.code(), Some(2), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stderr).contains("--format json"));
}
