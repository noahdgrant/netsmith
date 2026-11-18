//! `netsmith routes route` (issue #1195) on the LED example: `--dry-run` writes nothing, a net
//! routed is written to its record, a connection joined already is said, and the JSON says what
//! was done. What stood in the way and `--rip` are in `netsmith-fabricate`'s `tests/route_net.rs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_testkit::TempDir;

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

fn said(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

fn route(dir: &Path, args: &[&str]) -> Output {
    let mut all = vec!["routes", "route"];
    all.extend_from_slice(args);
    all.extend_from_slice(&["--file", "led.nsm"]);
    netsmith(dir, &all)
}

#[test]
fn routes_route_writes_a_net_and_a_dry_run_writes_nothing() {
    let dir = TempDir::new("routes-route");
    for name in ["led.nsm", "led.lock.toml"] {
        fs::copy(repo().join("examples").join(name), dir.join(name)).unwrap();
    }
    let file = dir.join("led.routes.toml");

    let run = route(&dir, &["VCC", "--dry-run"]);
    let text = said(&run);
    assert!(run.status.success(), "{text}");
    assert!(text.contains("net VCC: routed with"), "{text}");
    assert!(text.contains("dry run: led.routes.toml is not written"), "{text}");
    assert!(!file.exists(), "a dry run writes no file");

    let run = route(&dir, &["VCC"]);
    let text = said(&run);
    assert!(run.status.success(), "{text}");
    assert!(text.contains("wrote led.routes.toml"), "{text}");
    assert!(text.contains("the narrowest place of its way"), "{text}");
    let written = fs::read_to_string(&file).unwrap();
    assert!(written.contains("[LedExample.nets.VCC]"), "{written}");

    // A dry run leaves the file as it was, and says so in its JSON.
    let run = route(&dir, &["GND", "--dry-run", "--format", "json"]);
    assert!(run.status.success(), "{}", said(&run));
    let doc: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(doc["dry_run"], true);
    assert_eq!(doc["written"], false);
    assert_eq!(doc["refused"], serde_json::Value::Null);
    assert_eq!(doc["net"], "GND");
    assert!(!doc["nets"][0]["tracks"].as_array().unwrap().is_empty());
    assert_eq!(fs::read_to_string(&file).unwrap(), written);

    // One connection, then the net is joined.
    let run = route(&dir, &["GND:D1.1-J1.2"]);
    let text = said(&run);
    assert!(run.status.success(), "{text}");
    assert!(text.contains("net GND (D1.1 to J1.2): routed with"), "{text}");
    let run = route(&dir, &["GND"]);
    let text = said(&run);
    assert!(run.status.success(), "{text}");
    assert!(text.contains("net GND is joined already"), "{text}");

    // A net the board does not have is refused.
    let run = route(&dir, &["NOPE"]);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    assert!(said(&run).contains("no net `NOPE`"));
}
