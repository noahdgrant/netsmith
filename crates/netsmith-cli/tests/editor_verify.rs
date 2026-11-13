//! `scripts/editor-verify`, the tool that drives the editor in a headless browser. Its
//! `doctor --no-browser` needs only Node 20 or newer, so it is tested here; the rest needs a
//! browser and is exercised by running the recipes (skill `netsmith-dev-verify-editor`).

use std::{path::Path, process::Command};

fn script() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/editor-verify")
}

/// Whether the wrapper can find a Node of at least 20, as it does: on `PATH`, else in `~/.nvm`.
fn have_node() -> bool {
    Command::new("bash")
        .arg("-c")
        .arg(
            r#"node_ok() { command -v node >/dev/null 2>&1 && [ "$(node -p 'process.versions.node.split(".")[0]')" -ge 20 ]; }
               node_ok || { d="$(ls -d "$HOME"/.nvm/versions/node/v*/bin 2>/dev/null | sort -V | tail -n1)"; [ -n "$d" ] && PATH="$d:$PATH" && node_ok; }"#,
        )
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn doctor_reports_what_is_installed_without_a_browser() {
    if !have_node() {
        eprintln!("skipped: no Node 20 or newer");
        return;
    }
    let out = Command::new(script())
        .args(["doctor", "--no-browser"])
        .env("NETSMITH_BIN", env!("CARGO_BIN_EXE_netsmith"))
        .output()
        .expect("the script runs");
    let said = String::from_utf8_lossy(&out.stdout);
    for line in ["ok   node: v", "ok   netsmith binary: "] {
        assert!(said.contains(line), "{line}: {said}");
    }
    assert!(
        !said.contains("chromium") && !said.contains("playwright"),
        "{said}"
    );
    // The page is embedded in the binary (a placeholder when it was built without Node), so
    // doctor has nothing to look up: `launch` says when the binary has only the placeholder.
    assert!(out.status.success(), "{said}");
}
