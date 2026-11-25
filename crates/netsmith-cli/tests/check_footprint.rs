//! `netsmith board check-footprint` (issue #1684): one footprint's pads against a board's
//! process, with nothing placed or routed. A footprint file with two pads 0.05 mm nearer than
//! the process's space between surface-mount pads is refused with the pair, the rule and the
//! figures, in text and JSON; the same pads far enough apart, and a part of a board, pass.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `netsmith <args>` in `dir`, with KiCad switched off: the check needs none.
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

/// A footprint of two surface-mount pads 0.3 mm wide, `gap` apart.
fn two_pads(gap: f64) -> String {
    let x = (0.3 + gap) / 2.0;
    format!(
        "(footprint \"Two\" (layer \"F.Cu\") (attr smd)
  (property \"Reference\" \"REF**\" (at 0 -1.5) (layer \"F.SilkS\") (effects (font (size 1 1) (thickness 0.15))))
  (fp_rect (start -1.2 -1) (end 1.2 1) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\"))
  (pad \"1\" smd rect (at -{x} 0) (size 0.3 0.6) (layers \"F.Cu\" \"F.Paste\" \"F.Mask\"))
  (pad \"2\" smd rect (at {x} 0) (size 0.3 0.6) (layers \"F.Cu\" \"F.Paste\" \"F.Mask\")))\n"
    )
}

/// A folder with the LED example (JLCPCB's two-layer process: 0.15 mm between surface-mount
/// pads) and the two footprints.
fn project(name: &str) -> TempDir {
    let dir = TempDir::new(name);
    for f in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(f), dir.join(f)).unwrap();
    }
    fs::write(dir.join("Tight.kicad_mod"), two_pads(0.10)).unwrap();
    fs::write(dir.join("Wide.kicad_mod"), two_pads(0.50)).unwrap();
    dir
}

#[test]
fn pads_0_05_mm_too_close_are_reported_with_the_rule_and_the_figures_in_text_and_json() {
    let dir = project("check-footprint-tight");
    let run = netsmith(
        &dir,
        &["board", "check-footprint", "Tight.kicad_mod", "led.nsm"],
    );
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    let out = text(&run);
    assert!(
        out.contains(
            "error [pad_to_pad] pads 1 and 2: needs 0.1500 mm, has 0.1000 mm, short by 0.0500 mm"
        ),
        "{out}"
    );
    assert!(out.contains("1 rule(s) broken"), "{out}");
    // The mask web between the two openings is judged too (`.min_mask_bridge`).
    assert!(out.contains("[solder_mask_bridge] pads 1 and 2"), "{out}");

    let run = netsmith(
        &dir,
        &[
            "board",
            "check-footprint",
            "Tight.kicad_mod",
            "led.nsm",
            "--format",
            "json",
        ],
    );
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    let doc: serde_json::Value =
        serde_json::from_slice(&run.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(&run)));
    assert_eq!(doc["legal"], false, "{doc:#}");
    assert_eq!(doc["nets"], "pad", "{doc:#}");
    assert_eq!(doc["pads"], 2, "{doc:#}");
    let errors = doc["errors"].as_array().unwrap();
    assert_eq!(errors.len(), 1, "{doc:#}");
    let e = &errors[0];
    assert_eq!(e["rule"], "pad_to_pad", "{doc:#}");
    assert_eq!(e["pads"], serde_json::json!(["1", "2"]), "{doc:#}");
    assert_eq!(e["required"], 0.15, "{doc:#}");
    assert_eq!(e["measured"], 0.1, "{doc:#}");
    assert_eq!(e["shortfall"], 0.05, "{doc:#}");
}

#[test]
fn a_legal_footprint_and_a_part_of_the_board_report_none() {
    let dir = project("check-footprint-legal");
    let run = netsmith(
        &dir,
        &[
            "board",
            "check-footprint",
            "Wide.kicad_mod",
            "led.nsm",
            "--format",
            "json",
        ],
    );
    assert!(run.status.success(), "{}", text(&run));
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(doc["legal"], true, "{doc:#}");
    assert_eq!(doc["errors"], serde_json::json!([]), "{doc:#}");
    assert_eq!(doc["warnings"], serde_json::json!([]), "{doc:#}");

    let run = netsmith(&dir, &["board", "check-footprint", "R1", "led.nsm"]);
    assert!(run.status.success(), "{}", text(&run));
    let out = text(&run);
    assert!(out.contains("R1 (Resistor_SMD:R_0402_1005Metric)"), "{out}");
    assert!(out.contains("each pad on its part's net"), "{out}");
    assert!(out.contains("no rule broken"), "{out}");

    let run = netsmith(&dir, &["board", "check-footprint", "U9", "led.nsm"]);
    assert_eq!(run.status.code(), Some(1), "{}", text(&run));
    assert!(text(&run).contains("no part `U9`"), "{}", text(&run));
}

/// A footprint of eight surface-mount pads 0.28 mm wide at a pitch of 0.4 mm: 0.12 mm of copper
/// between neighbours, under the process's 0.13 mm mask web.
fn fine_pitch() -> String {
    let pads: String = (0..8)
        .map(|k| {
            format!(
                "  (pad \"{}\" smd rect (at {:.1} 0) (size 0.28 0.8) (layers \"F.Cu\" \"F.Paste\" \"F.Mask\"))\n",
                k + 1,
                k as f64 * 0.4 - 1.4
            )
        })
        .collect();
    format!(
        "(footprint \"Fine\" (layer \"F.Cu\") (attr smd)
  (property \"Reference\" \"REF**\" (at 0 -1.5) (layer \"F.SilkS\") (effects (font (size 1 1) (thickness 0.15))))
  (fp_rect (start -2 -1) (end 2 1) (stroke (width 0.05) (type solid)) (layer \"F.CrtYd\"))
{pads})\n"
    )
}

#[test]
fn a_0_4_mm_pitch_footprint_has_one_mask_bridge_line_with_the_count_and_spacing() {
    // Issue #1685: seven pairs no mask web fits between are one line, with every pad.
    let dir = project("check-footprint-fine");
    fs::write(dir.join("Fine.kicad_mod"), fine_pitch()).unwrap();
    let run = netsmith(
        &dir,
        &["board", "check-footprint", "Fine.kicad_mod", "led.nsm"],
    );
    let out = text(&run);
    let bridges: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("[solder_mask_bridge]"))
        .collect();
    assert_eq!(bridges.len(), 1, "{out}");
    assert!(
        bridges[0].contains("pads 1, 2, 3, 4, 5, 6, 7 and 8: 7 pairs, needs 0.1300 mm, has 0.1200 mm"),
        "{out}"
    );

    let run = netsmith(
        &dir,
        &[
            "board",
            "check-footprint",
            "Fine.kicad_mod",
            "led.nsm",
            "--format",
            "json",
        ],
    );
    let doc: serde_json::Value =
        serde_json::from_slice(&run.stdout).unwrap_or_else(|e| panic!("{e}: {}", text(&run)));
    let bridges: Vec<&serde_json::Value> = doc["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["rule"] == "solder_mask_bridge")
        .collect();
    assert_eq!(bridges.len(), 1, "{doc:#}");
    assert_eq!(
        bridges[0]["pads"],
        serde_json::json!(["1", "2", "3", "4", "5", "6", "7", "8"]),
        "{doc:#}"
    );
    assert!(
        bridges[0]["message"]
            .as_str()
            .unwrap()
            .contains("7 pairs of"),
        "{doc:#}"
    );
}
