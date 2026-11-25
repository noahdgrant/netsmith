//! `netsmith routes fanout` (issue #1640) on the LED example: a pad fanned out to a site of
//! `--sites`, a place there the check refuses said with what blocks it, `--dry-run` writing
//! nothing, a plan file followed, and a pad with no site in reach listed with exit 1. The group
//! fanout of a BGA is in `netsmith-fabricate`'s `tests/fanout.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fanout(dir: &Path, args: &[&str]) -> Output {
    let mut all = vec!["routes", "fanout", "R1"];
    all.extend_from_slice(args);
    all.extend_from_slice(&["--file", "led.nsm"]);
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(&all)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn said(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

#[test]
fn fanout_writes_a_pad_to_its_via_a_dry_run_writes_nothing_and_a_plan_is_followed() {
    let dir = TempDir::new("routes-fanout");
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    let file = dir.join("led.routes.toml");

    // R1's two pads are 1.02 mm apart: no site between them, so its pad 1 has none in reach.
    let run = fanout(&dir, &["--pads", "1"]);
    let text = said(&run);
    assert_eq!(run.status.code(), Some(1), "{text}");
    assert!(text.contains("R1.1 [VCC]: no legal via site within"), "{text}");
    assert!(!file.exists());

    // A site beside it, and one between the pads the check refuses.
    let sites = ["--pads", "1", "--sites", "6.6,2.84;7.6,3.35"];
    let run = fanout(&dir, &[&sites[..], &["--dry-run"]].concat());
    let text = said(&run);
    assert!(run.status.success(), "{text}");
    assert!(text.contains("R1.1 [VCC] to the via at 6.600,2.840"), "{text}");
    assert!(
        text.contains("the site at 7.600,3.350 of `--sites` is refused: clearance"),
        "{text}"
    );
    assert!(text.contains("dry run: led.routes.toml is not written"), "{text}");
    assert!(!file.exists(), "a dry run writes no file");

    let run = fanout(&dir, &[&sites[..], &["--format", "json"]].concat());
    assert!(run.status.success(), "{}", said(&run));
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(doc["written"], true);
    assert_eq!(doc["dry_run"], false);
    assert_eq!(doc["escaped"][0]["pad"], "R1.1");
    assert_eq!(doc["sites_refused"].as_array().unwrap().len(), 1);
    let written = fs::read_to_string(&file).unwrap();
    assert!(written.contains("[LedExample.nets.VCC]"), "{written}");
    assert!(written.contains("vias = [\n  { at = [6.6, 2.84]"), "{written}");

    // Asked again, the pad is joined already.
    let run = fanout(&dir, &["--pads", "1"]);
    let text = said(&run);
    assert!(run.status.success(), "{text}");
    assert!(text.contains("joined already by the routes file's copper: R1.1"), "{text}");

    // A plan: the other pad to a site below it; a planned site the check refuses is refused.
    fs::write(dir.join("plan.txt"), "# R1\n2 = 7.6,4.86\n").unwrap();
    let run = fanout(&dir, &["--plan", "plan.txt", "--dry-run"]);
    let text = said(&run);
    assert!(run.status.success(), "{text}");
    assert!(text.contains("R1.2 [") && text.contains("to the via at 7.600,4.860"), "{text}");
    fs::write(dir.join("plan.txt"), "2 = 7.6,3.35\n").unwrap();
    let run = fanout(&dir, &["--plan", "plan.txt"]);
    let text = said(&run);
    assert_eq!(run.status.code(), Some(1), "{text}");
    assert!(
        text.contains("the plan's site of R1.2 at (7.6, 3.35) is refused"),
        "{text}"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), written);
}
