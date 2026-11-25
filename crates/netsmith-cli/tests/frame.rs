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
        (&["routes", "sites", "D1", "--file", f], "outline"),
        (&["routes", "check-via", "--at", "2,2", "--file", f], "outline"),
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
}

/// A number as the text of `layout parts` and `routes show` say it: to a micrometre.
fn um(v: f64) -> String {
    let r = (v * 1e6).round() / 1e6;
    format!("{}", if r == 0.0 { 0.0 } else { r })
}

#[test]
fn the_text_says_its_places_in_the_frame_asked_for() {
    let (dir, pad, end) = led("frame-text");
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let flag = ["--frame", frame];
        let with = |args: &[&'static str]| [args, &flag[..]].concat();
        let text = |args: &[&'static str]| {
            let run = netsmith(&dir, &with(args));
            assert!(run.status.code().is_some_and(|c| c < 2), "{run:?}");
            String::from_utf8_lossy(&run.stdout).into_owned()
        };
        let (pad, end) = ([pad[0] + by, pad[1] + by], [end[0] + by, end[1] + by]);

        // `layout parts`: the pad's place, and the heading names the frame.
        let parts = text(&["layout", "parts", "J1", "--file", "led.nsm"]);
        let at = format!("pad 1 thru_hole rect at {},{} ", um(pad[0]), um(pad[1]));
        assert!(parts.contains(&at), "{frame}: {at}\n{parts}");
        let heading = if by == 0.0 { "from the outline's top left" } else { "the board file's frame" };
        assert!(parts.lines().next().unwrap().contains(heading), "{parts}");

        // `routes show`: the track's two ends.
        let routes = text(&["routes", "show", "--file", "led.nsm"]);
        let track = format!(
            "({}, {}) to ({}, {})",
            um(pad[0]),
            um(pad[1]),
            um(end[0]),
            um(end[1])
        );
        assert!(routes.contains(&track), "{frame}: {track}\n{routes}");

        // `layout show`: the entry of J1 (the pad is its origin).
        let show = text(&["layout", "show", "power_in", "--file", "led.nsm"]);
        let entry = format!("at {}, {} mm", um(pad[0]), um(pad[1]));
        assert!(show.contains(&entry), "{frame}: {entry}\n{show}");

        // `board drc`: the track's free end, which the text says `at (x, y)`.
        let drc = text(&["board", "drc", "led.nsm"]);
        let dangling = format!("at ({:.3}, {:.3}) on F.Cu", end[0], end[1]);
        assert!(drc.contains(&dangling), "{frame}: {dangling}\n{drc}");

        // `board nets`: the dead stub is the track, which starts at the pad.
        let nets = text(&["board", "nets", "led.nsm"]);
        let stub = format!("at ({:.2}, {:.2})", pad[0], pad[1]);
        assert!(nets.contains(&stub), "{frame}: {stub}\n{nets}");

        // `board fill`: the outline's corner, and the frame it says.
        let fill = text(&["board", "fill", "led.nsm"]);
        let corner = format!("corner is at ({by:.3}, {by:.3})");
        assert!(fill.contains(&corner), "{frame}: {corner}\n{fill}");
        let frame_words = if by == 0.0 { "outline's frame" } else { "board file's frame" };
        assert!(fill.contains(frame_words), "{fill}");

        // `board rules` takes it too.
        text(&["board", "rules", "led.nsm"]);
    }
}

/// A project of the LED example with a physical file that pins its LED and its resistor to the
/// same place: a conflict proved before the placer runs, with the boxes of their courtyards.
fn led_overlapped(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    fs::write(
        dir.join("led.physical.nsm"),
        "import std::pcb::Outline\n\nphysical for LedExample {\n    case {\n        .outline: Outline {\n            .width &= 40mm\n            .height &= 24mm\n        }\n    }\n    .r1.place.x &= 10mm +/- 0.1mm\n    .r1.place.y &= 10mm +/- 0.1mm\n    .led.place.x &= 10mm +/- 0.1mm\n    .led.place.y &= 10mm +/- 0.1mm\n    .led.place.rotation &= 90\n}\n",
    )
    .unwrap();
    dir
}

#[test]
fn board_check_build_and_fab_say_one_courtyard_at_one_place_in_the_frame_asked_for() {
    let dir = led_overlapped("frame-check-build-fab");
    let out = dir.join("out");
    let out = out.to_str().unwrap();
    // The courtyard of R1, pinned at (10, 10) mm in the outline's frame.
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        for args in [
            &["board", "check", "led.nsm"][..],
            &["board", "build", "led.nsm", "--out", out],
            &["board", "fab", "led.nsm", "--out", out],
        ] {
            let doc = json(&dir, &[args, &["--format", "json", "--frame", frame][..]].concat());
            let what = format!("netsmith {} --frame {frame}", args.join(" "));
            assert_eq!(doc["frame"], frame, "{what}");
            let boxed = doc["findings"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|f| f["conflicts"].as_array().into_iter().flatten())
                .flat_map(|c| c["parts"].as_array().into_iter().flatten())
                .find(|p| p["refdes"] == "R1")
                .unwrap_or_else(|| panic!("{what}: {doc:#}"));
            let yard = &boxed["courtyard"];
            for (key, outline) in [("x_min", 8.97), ("x_max", 11.03), ("y_min", 8.97), ("y_max", 11.03)] {
                let got = yard[key].as_f64().unwrap();
                assert!((got - (outline + by)).abs() < 1e-6, "{what}: {key} {got}");
            }
        }
    }
}

#[test]
fn a_finding_fab_waived_for_review_is_in_the_documents_frame() {
    let (dir, _, _) = led("frame-waived");
    let out = dir.join("out");
    let out = out.to_str().unwrap();
    // A track of VCC from (5, 5) to (8, 5) mm touches nothing: a `track_dangling` error of the
    // design rules check at its start, which `--for-review` waives. The document is in the
    // outline's frame, and so is the finding's place.
    let run = netsmith(
        &dir,
        &[
            "routes", "add-track", "--file", "led.nsm", "--net", "VCC", "--layer", "top", "--width",
            "0.3", "--from", "5,5", "--to", "8,5",
        ],
    );
    assert!(run.status.success(), "{run:?}");
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let doc = json(
            &dir,
            &[
                "board", "fab", "led.nsm", "--for-review", "--no-pictures", "--no-step",
                "--no-pdf", "--out", out, "--format", "json", "--frame", frame,
            ],
        );
        assert_eq!(doc["frame"], frame);
        let finding = doc["findings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["rule"] == "track_dangling" && f["waived"] == "for-review")
            .unwrap_or_else(|| panic!("{doc:#}"));
        near(
            point(&finding["place"]),
            [5.0 + by, 5.0 + by],
            "the waived finding's place",
        );
    }
}

/// The first `(x, y)` or `x,y` after `marker` in `text`.
fn place_after(text: &str, marker: &str) -> [f64; 2] {
    let rest = text
        .split_once(marker)
        .unwrap_or_else(|| panic!("no `{marker}` in\n{text}"))
        .1;
    let start = rest
        .find(|c: char| c.is_ascii_digit() || c == '-')
        .unwrap_or_else(|| panic!("no place after `{marker}` in\n{text}"));
    let numbers: Vec<f64> = rest[start..]
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .filter(|w| !w.is_empty())
        .take(2)
        .map(|w| w.parse().unwrap())
        .collect();
    [numbers[0], numbers[1]]
}

fn text_of(dir: &Path, args: &[&str]) -> String {
    let run = netsmith(dir, args);
    assert!(run.status.code().is_some_and(|c| c < 2), "{run:?}");
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

#[test]
fn the_text_of_the_layout_commands_says_one_place_as_its_json_does() {
    let (dir, pad, _) = led("frame-text-layout");
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let flag = ["--frame", frame];
        let with = |args: &[&'static str], format: &'static str| {
            [args, &["--format", format][..], &flag[..]].concat()
        };
        let place = [pad[0] + by, pad[1] + by];

        // `layout set`: the entry of J1, which is at the pad.
        let set = &["layout", "set", "power_in", "--turn", "0", "--file", "led.nsm"];
        let said = text_of(&dir, &with(set, "text"));
        near(place_after(&said, ": at "), place, "layout set's text");
        let doc = json(&dir, &with(set, "json"));
        near(point(&doc["entry"]["at"]), place, "layout set's json");

        // `layout summary`: the corner of the largest free rectangle of the top face.
        let summary = &["layout", "summary", "--file", "led.nsm"];
        let said = text_of(&dir, &with(summary, "text"));
        let doc = json(&dir, &with(summary, "json"));
        let corner = point(&doc["faces"][0]["largest_free"][0]["min"]);
        // The corner is after the rectangle's size, `W x H mm at (x, y)`.
        near(place_after(&said, "mm at "), corner, "layout summary");

        // `layout fit`: where the part is now.
        let fit = &["layout", "fit", "D1", "--file", "led.nsm"];
        let said = text_of(&dir, &with(fit, "text"));
        let doc = json(&dir, &with(fit, "json"));
        near(
            place_after(&said, "  now "),
            point(&doc["now"]["at"]),
            "layout fit",
        );
        // The command it gives to write a place is in the layout file's frame, whatever the flag.
        if let Some(line) = said.lines().find(|l| l.contains("netsmith layout set")) {
            let at = place_after(line, "--at ");
            let first = &doc["poses"][0]["at"];
            assert!(
                (at[0] - first[0].as_f64().unwrap() + by).abs() < 1e-6,
                "{line}\n{doc:#}"
            );
        }
    }
}

#[test]
fn the_text_of_compare_and_regenerate_says_the_places_of_the_parts_as_json_does() {
    let (dir, pad, _) = led("frame-text-compare");
    fs::copy(dir.join("led.pcb.toml"), dir.join("before.pcb.toml")).unwrap();
    let run = netsmith(
        &dir,
        &[
            "layout",
            "set",
            "power_in",
            "--at",
            &format!("{},{}", pad[0] + 1.0, pad[1]),
            "--file",
            "led.nsm",
        ],
    );
    assert!(run.status.success(), "{run:?}");
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let flag = ["--frame", frame];
        let with = |args: &[&'static str], format: &'static str| {
            [args, &["--format", format][..], &flag[..]].concat()
        };
        let compare = &["layout", "compare", "before.pcb.toml", "--file", "led.nsm"];
        let said = text_of(&dir, &with(compare, "text"));
        let doc = json(&dir, &with(compare, "json"));
        let moved = doc["moved"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["refdes"] == "J1")
            .unwrap_or_else(|| panic!("{doc:#}"));
        near(point(&moved["from"]), [pad[0] + by, pad[1] + by], "compare's json from");
        near(point(&moved["to"]), [pad[0] + 1.0 + by, pad[1] + by], "compare's json to");
        let line = said.lines().find(|l| l.contains("moved J1")).unwrap();
        near(place_after(line, ": "), point(&moved["from"]), "compare's text from");
        near(place_after(line, "-> "), point(&moved["to"]), "compare's text to");

        let regenerate = &["layout", "regenerate", "led", "--file", "led.nsm"];
        let said = text_of(&dir, &with(regenerate, "text"));
        let doc = json(&dir, &with(regenerate, "json"));
        let region = point(&doc["region"]);
        near(place_after(&said, " in "), region, "regenerate's region");
        let first = &doc["parts"][0];
        let line = said.lines().find(|l| l.contains("->")).unwrap();
        near(
            place_after(line, "-> "),
            point(&first["after"]["at"]),
            "regenerate's text and json",
        );
    }
}

/// The LED example with a corner and a collinear pair of GND copper, written by hand.
fn led_with_corner(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    let run = netsmith(
        &dir,
        &[
            "routes", "add-track", "--net", "GND", "--layer", "bottom", "--width", "0.15",
            "--from", "1.0,0.8", "--to", "2.0,0.8", "--file", "led.nsm",
        ],
    );
    assert!(run.status.success(), "{run:?}");
    let file = dir.join("led.routes.toml");
    let text = fs::read_to_string(&file).unwrap();
    let first = text
        .lines()
        .find(|l| l.contains("from = [1.0, 0.8]"))
        .unwrap()
        .to_string();
    let more = [
        first.clone(),
        first.replace("from = [1.0, 0.8], to = [2.0, 0.8]", "from = [2.0, 0.8], to = [3.5, 0.8]"),
        first.replace("from = [1.0, 0.8], to = [2.0, 0.8]", "from = [3.5, 0.8], to = [3.5, 2.0]"),
    ]
    .join("\n");
    fs::write(&file, text.replace(&first, &more)).unwrap();
    dir
}

#[test]
fn the_text_of_routes_tidy_says_the_place_of_a_change_as_its_json_does() {
    let dir = led_with_corner("frame-text-tidy");
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let args = |format: &'static str| {
            [
                "routes", "tidy", "--dry-run", "--file", "led.nsm", "--format", format, "--frame",
                frame,
            ]
        };
        let said = text_of(&dir, &args("text"));
        let doc = json(&dir, &args("json"));
        let corner = doc["changes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| (point(&c["at"])[0] - (3.5 + by)).abs() < 1e-6)
            .unwrap_or_else(|| panic!("{doc:#}"));
        near(point(&corner["at"]), [3.5 + by, 0.8 + by], "tidy's json");
        let line = said.lines().find(|l| l.contains("corner:")).unwrap();
        near(place_after(line, " at "), [3.5 + by, 0.8 + by], "tidy's text");
    }
}

#[test]
fn the_lines_of_the_design_rules_check_in_fab_are_moved_with_the_flag() {
    let (dir, _, _) = led("frame-text-fab-notes");
    // A dangling track of VCC from (5, 5) mm: `--for-review` waives it, and `fab` says it in
    // the outline's frame, as the rest of its text (issue #1614), or in the frame asked for.
    let run = netsmith(
        &dir,
        &[
            "routes", "add-track", "--file", "led.nsm", "--net", "VCC", "--layer", "top", "--width",
            "0.3", "--from", "5,5", "--to", "8,5",
        ],
    );
    assert!(run.status.success(), "{run:?}");
    let out = dir.join("out");
    let out = out.to_str().unwrap();
    for (flag, at) in [
        (None, "at (5.000, 5.000)"),
        (Some("board-file"), "at (25.000, 25.000)"),
        (Some("outline"), "at (5.000, 5.000)"),
    ] {
        let mut args = vec![
            "board", "fab", "led.nsm", "--for-review", "--no-pictures", "--no-step", "--no-pdf",
            "--out", out,
        ];
        if let Some(frame) = flag {
            args.extend(["--frame", frame]);
        }
        let run = netsmith(&dir, &args);
        let said = String::from_utf8_lossy(&run.stderr).into_owned();
        let line = said
            .lines()
            .find(|l| l.contains("[track_dangling]"))
            .unwrap_or_else(|| panic!("{flag:?}: {said}"));
        assert!(line.contains(at), "{flag:?}: {line}");
    }
}

#[test]
fn fab_lists_the_origins_of_the_parts_in_the_frame_asked_for() {
    let dir = TempDir::new("frame-text-origins");
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    let out = dir.join("out");
    let out = out.to_str().unwrap();
    let mut found = Vec::new();
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let args = [
            "board", "fab", "led.nsm", "--no-glb", "--no-step", "--no-pdf", "--out", out,
            "--frame", frame,
        ];
        let said = text_of(&dir, &args);
        let line = said.lines().find(|l| l.trim_start().starts_with("J1 (`")).unwrap_or_else(|| panic!("{said}"));
        // The offset is a distance: it does not move.
        let dx: f64 = line.split("dx ").nth(1).unwrap().split(' ').next().unwrap().parse().unwrap();
        found.push((place_after(line, "origin "), dx));
        let head = said.lines().find(|l| l.contains("where each footprint's origin is")).unwrap();
        let words = if by == 0.0 { "from the outline's top left" } else { "the board file's frame" };
        assert!(head.contains(words), "{head}");
    }
    let [(a, dx_a), (b, dx_b)] = found[..] else { unreachable!() };
    near([b[0] - a[0], b[1] - a[1]], [20.0, 20.0], "the origin moves by the corner");
    assert_eq!(dx_a, dx_b, "the offset stays");
}

/// The `at (x, y)` of the line of `text` that names `rule` (`error [track_dangling] ...`).
fn place_of_line(text: &str, rule: &str) -> [f64; 2] {
    let line = text
        .lines()
        .find(|l| l.contains(&format!("[{rule}]")))
        .unwrap_or_else(|| panic!("no [{rule}] line in:\n{text}"));
    let (_, rest) = line.split_once(" at (").unwrap();
    let (pair, _) = rest.split_once(')').unwrap();
    let (x, y) = pair.split_once(", ").unwrap();
    [x.parse().unwrap(), y.parse().unwrap()]
}

/// Whether a line of `text` that names `rule` says the place `b`, to its three decimals.
fn says_at(text: &str, rule: &str, b: [f64; 2]) -> bool {
    text.lines()
        .filter(|l| l.contains(&format!("[{rule}]")))
        .any(|l| {
            let a = place_of_line(l, rule);
            (a[0] - b[0]).abs() < 6e-4 && (a[1] - b[1]).abs() < 6e-4
        })
}

/// A place a text says, to its three decimals, is `b`.
fn near_said(a: [f64; 2], b: [f64; 2], what: &str) {
    assert!(
        (a[0] - b[0]).abs() < 6e-4 && (a[1] - b[1]).abs() < 6e-4,
        "{what}: {a:?} is not {b:?}"
    );
}

#[test]
fn routes_add_says_the_place_of_a_refusal_in_the_frame_asked_for_in_text_and_json() {
    let (dir, pad, _) = led("frame-routes-add");
    // A track of GND that crosses the VCC track (pad 1 of J1 out 3 mm to the right) at x + 1.
    let (x, y) = (pad[0] + 1.0, pad[1]);
    fs::write(
        dir.join("shapes.toml"),
        format!(
            "[[track]]\nnet = \"GND\"\nfrom = [{x}, {}]\nto = [{x}, {}]\n",
            y - 1.0,
            y + 1.0
        ),
    )
    .unwrap();
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let args = |format: &'static str| {
            [
                "routes", "add", "shapes.toml", "--file", "led.nsm", "--dry-run", "--format", format,
                "--frame", frame,
            ]
        };
        let place = [x + by, y + by];
        let said = text_of(&dir, &args("text"));
        let line = said.lines().find(|l| l.contains("[tracks_crossing]")).unwrap_or_else(|| panic!("{said}"));
        near_said(place_after(line, " cross; they are on nets GND and VCC at "), place, "routes add's text");
        let doc = json(&dir, &args("json"));
        assert_eq!(doc["frame"], frame);
        let shape = &doc["shapes"][0];
        near(point(&shape["findings"][0]["at"]), place, "routes add's finding");
        // The sentence of the shape says the same place.
        let sentence = shape["said"].as_str().unwrap();
        near_said(
            place_after(sentence, " cross; they are on nets GND and VCC at "),
            place,
            "routes add's said",
        );
    }
}

/// The LED example with a wall of locked GND vias between the pads of VCC, which has no way
/// across it: the net is walled in by the vias.
fn led_walled(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    for args in [
        &["layout", "capture", "--file", "led.nsm"][..],
        &[
            "routes", "add-track", "--file", "led.nsm", "--net", "GND", "--layer", "bottom",
            "--width", "0.15", "--from", "12,1", "--to", "13,1",
        ],
    ] {
        let run = netsmith(&dir, args);
        assert!(run.status.success(), "{run:?}");
    }
    let file = dir.join("led.routes.toml");
    let vias: Vec<String> = (0..19)
        .map(|i| {
            format!(
                "{{ at = [5.0, {:.2}], diameter = 0.6, drill = 0.3, lock = true }}",
                0.4 + 0.4 * f64::from(i)
            )
        })
        .collect();
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("tracks = ["), "{text}");
    fs::write(&file, text.replacen("tracks = [", &format!("vias = [{}]\ntracks = [", vias.join(", ")), 1)).unwrap();
    dir
}

#[test]
fn routes_route_says_the_vias_that_wall_a_net_in_in_the_frame_asked_for() {
    let dir = led_walled("frame-routes-route");
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let args = |format: &'static str| {
            [
                "routes", "route", "VCC", "--file", "led.nsm", "--dry-run", "--format", format,
                "--frame", frame,
            ]
        };
        // The first and the last via of the wall, as the sentence says them.
        let first = format!("net GND at ({}, {})", um(5.0 + by), um(0.4 + by));
        let last = format!("net GND at ({}, {})", um(5.0 + by), um(7.6 + by));
        let said = text_of(&dir, &args("text"));
        assert!(said.contains(&first) && said.contains(&last), "{frame}: {said}");
        let doc = json(&dir, &args("json"));
        assert_eq!(doc["frame"], frame);
        let vias = doc["walls"][0]["vias"].as_array().unwrap();
        assert_eq!(vias.len(), 19, "{doc:#}");
        assert_eq!(vias[0], first.as_str(), "{frame}");
        assert_eq!(vias[18], last.as_str(), "{frame}");
    }
}

#[test]
fn routes_import_says_the_place_of_a_fault_in_the_copper_as_its_json_does() {
    let dir = TempDir::new("frame-routes-import");
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    let out = dir.join("first");
    let run = netsmith(
        &dir,
        &["board", "fab", "led.nsm", "--for-review", "--locked", "--no-pictures", "--no-step", "--no-pdf", "--out", out.to_str().unwrap()],
    );
    assert!(run.status.success(), "{run:?}");
    // The board `fab` wrote, with a track of net 3 across the tracks of the other nets.
    let board = fs::read_to_string(out.join("LedExample/LedExample.kicad_pcb")).unwrap();
    assert!(board.contains("(net 3 \"net-led.anode\")"), "the example's nets changed");
    let board = board.trim_end().strip_suffix(')').unwrap().to_string()
        + "\t(segment\n\t\t(start 26.5 21.0)\n\t\t(end 26.5 28.0)\n\t\t(width 0.15)\n\t\t(layer \"F.Cu\")\n\t\t(net 3)\n\t\t(uuid \"aaaaaaaa-84cb-50bb-a4ff-ffc825c25fee\")\n\t)\n)\n";
    fs::write(dir.join("hand.kicad_pcb"), board).unwrap();
    for (frame, by) in [("outline", 0.0), ("board-file", 20.0)] {
        let args = |format: &'static str| {
            [
                "routes", "import", "hand.kicad_pcb", "--file", "led.nsm", "--format", format,
                "--frame", frame,
            ]
        };
        let said = text_of(&dir, &args("text"));
        let line = said.lines().find(|l| l.starts_with("net GND:")).unwrap_or_else(|| panic!("{said}"));
        let doc = json(&dir, &args("json"));
        assert_eq!(doc["frame"], frame);
        let gnd = doc["nets"].as_array().unwrap().iter().find(|n| n["net"] == "GND").unwrap();
        let at = point(&gnd["refusals"][0]["at"]);
        // The crossing is on the new track, at x = 6.5 in the outline's frame.
        near(at, [6.5 + by, at[1]], "routes import's json");
        near_said(place_after(line, "[tracks_crossing] at "), at, "routes import's text");
    }
}

#[test]
fn board3d_and_layout_check_say_the_place_of_a_part_in_the_notch_in_the_frame_asked_for() {
    // The L-shaped board with its resistor in the cut-away corner: the placement check says
    // where its courtyard is past the outline, `at (x, y) mm from the board's top left corner`.
    let dir = TempDir::new("frame-board3d-notch");
    let fixtures = repo().join("scripts/editor-verify-js/fixtures");
    for name in ["polygon_outline.nsm", "polygon_outline.physical.nsm"] {
        fs::copy(fixtures.join(name), dir.join(name)).unwrap();
    }
    for args in [
        &["layout", "capture", "--file", "polygon_outline.nsm"][..],
        &["layout", "set", "r", "--at", "32,7", "--file", "polygon_outline.nsm"],
    ] {
        let run = netsmith(&dir, args);
        assert!(run.status.success(), "{run:?}");
    }
    for (frame, by, words) in [
        ("outline", 0.0, "mm from the board's top left corner"),
        ("board-file", 20.0, "mm in the board file's frame"),
    ] {
        let place = [33.5 + by, 6.3 + by];
        let sentence = "is not wholly inside the board outline";
        // `dev board3d`: the placement on stdout and the refusal on stderr.
        let run = netsmith(&dir, &["dev", "board3d", "polygon_outline.nsm", "--frame", frame]);
        assert_eq!(run.status.code(), Some(1), "{run:?}");
        for (what, text) in [
            ("stdout", String::from_utf8_lossy(&run.stdout).into_owned()),
            ("stderr", String::from_utf8_lossy(&run.stderr).into_owned()),
        ] {
            let line = text
                .lines()
                .find(|l| l.contains(sentence))
                .unwrap_or_else(|| panic!("{frame} {what}: {text}"));
            near(place_after(line, "box, at "), place, "board3d's text");
            assert!(line.contains(words), "{frame}: {line}");
            assert_eq!(by != 0.0, !line.contains("board's top left corner"), "{frame}: {line}");
        }
        // The JSON of the same board, in the frame asked for.
        let scene = json(
            &dir,
            &["dev", "board3d", "polygon_outline.nsm", "--frame", frame, "--format", "json"],
        );
        assert_eq!(scene["frame"], frame);
        // `layout check`: the sentence in the `message` of a finding.
        let checked = json(
            &dir,
            &["layout", "check", "--file", "polygon_outline.nsm", "--format", "json", "--frame", frame],
        );
        assert_eq!(checked["frame"], frame);
        let message = checked["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["message"].as_str())
            .find(|m| m.contains(sentence))
            .unwrap_or_else(|| panic!("{frame}: {checked:#}"));
        near(place_after(message, "box, at "), place, "layout check's json");
        assert!(message.contains(words), "{frame}: {message}");
    }
}

#[test]
fn the_places_of_fab_notes_are_in_the_outlines_frame_as_its_header_says() {
    // A track of VCC from (5, 5) to (8, 5) mm in the outline's frame (the routes file's) touches
    // nothing: a `track_dangling` error at its start, which `--for-review` waives and
    // `fab-notes.txt` lists (issue #1614).
    let (dir, _, _) = led("frame-fab-notes");
    let run = netsmith(
        &dir,
        &[
            "routes", "add-track", "--file", "led.nsm", "--net", "VCC", "--layer", "top", "--width",
            "0.3", "--from", "5,5", "--to", "8,5",
        ],
    );
    assert!(run.status.success(), "{run:?}");
    let end = [5.0, 5.0];
    let run = netsmith(
        &dir,
        &[
            "board", "fab", "led.nsm", "--for-review", "--no-pictures", "--no-step", "--no-pdf",
            "--out", "out",
        ],
    );
    assert_eq!(run.status.code(), Some(1), "{run:?}");
    let notes = fs::read_to_string(dir.join("out/LedExample/fab-notes.txt")).unwrap();
    let header = notes.lines().next().unwrap();
    assert!(
        header.ends_with("; places in mm from the outline's top left corner, y down"),
        "{notes}"
    );
    near_said(place_of_line(&notes, "track_dangling"), end, "fab-notes.txt");
    // `fab`'s own message says the same place.
    let said = String::from_utf8_lossy(&run.stderr);
    near_said(place_of_line(&said, "track_dangling"), end, "fab's message");

    // `board drc` keeps its own frame, the board file's, and with `--frame outline` says the
    // place the notes say.
    let drc = netsmith(&dir, &["board", "drc", "led.nsm"]);
    let drc = String::from_utf8_lossy(&drc.stdout);
    let file = [end[0] + 20.0, end[1] + 20.0];
    assert!(says_at(&drc, "track_dangling", file), "board drc:\n{drc}");
    assert!(!says_at(&drc, "track_dangling", end), "board drc:\n{drc}");
    let drc = netsmith(&dir, &["board", "drc", "led.nsm", "--frame", "outline"]);
    let drc = String::from_utf8_lossy(&drc.stdout);
    assert!(
        says_at(&drc, "track_dangling", end),
        "board drc --frame outline:\n{drc}"
    );
}

/// `routes move-via` (issue #1644): `--dry-run` writes nothing, its JSON has its places in the
/// frame asked for, and a move writes the via and its track's new legs.
#[test]
fn routes_move_via_moves_the_via_with_its_track_in_the_frame_asked_for() {
    let (dir, pad, end) = led("frame-move-via");
    let at = format!("{},{}", end[0], end[1]);
    let shown = json(
        &dir,
        &["routes", "show", "--file", "led.nsm", "--format", "json"],
    );
    let net = shown["nets"][0]["net"].as_str().unwrap().to_string();
    let base = ["--file", "led.nsm", "--net", &net];
    let run = netsmith(
        &dir,
        &[&["routes", "add-via", "--at", &at][..], &base[..]].concat(),
    );
    assert!(run.status.success(), "{run:?}");
    let file = dir.join("led.routes.toml");
    let was = fs::read_to_string(&file).unwrap();
    let new = [end[0] - 0.5, end[1] - 1.0];
    let to = format!("{},{}", new[0], new[1]);
    let mv = [
        &["routes", "move-via", "--via", "1", "--to", &to][..],
        &base[..],
    ]
    .concat();
    let doc = json(
        &dir,
        &[
            &mv[..],
            &["--dry-run", "--format", "json", "--frame", "board-file"][..],
        ]
        .concat(),
    );
    assert_eq!(doc["frame"], "board-file", "{doc:#}");
    assert_eq!(doc["written"], false, "{doc:#}");
    assert_eq!(doc["refused"], Value::Null, "{doc:#}");
    near(point(&doc["to"]), [new[0] + 20.0, new[1] + 20.0], "to");
    let way = doc["tracks"][0]["way_after"].as_array().unwrap();
    near(point(&way[0]), [pad[0] + 20.0, pad[1] + 20.0], "the end that stays");
    near(
        point(way.last().unwrap()),
        [new[0] + 20.0, new[1] + 20.0],
        "the end in the via",
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), was, "a dry run wrote");

    let run = netsmith(&dir, &mv);
    let text = String::from_utf8_lossy(&run.stdout);
    assert!(run.status.success(), "{run:?}");
    assert!(text.contains("wrote "), "{text}");
    let shown = json(
        &dir,
        &["routes", "show", "--file", "led.nsm", "--format", "json"],
    );
    let vcc = shown["nets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["net"] == net.as_str())
        .unwrap();
    near(point(&vcc["vias"][0]["at"]), new, "the via");
    assert_eq!(vcc["tracks"].as_array().unwrap().len(), 2, "{vcc:#}");

    // A locked via is refused, and nothing is written.
    let run = netsmith(
        &dir,
        &[&["routes", "lock", "--via", "1"][..], &base[..]].concat(),
    );
    assert!(run.status.success(), "{run:?}");
    let was = fs::read_to_string(&file).unwrap();
    let run = netsmith(&dir, &mv);
    assert_eq!(run.status.code(), Some(1), "{run:?}");
    assert!(
        String::from_utf8_lossy(&run.stderr).contains("is locked"),
        "{run:?}"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), was);
}
