//! The web app's own tests (the `.nsm` tokenizer, the graph layout), run with Node 20 or
//! newer if there is one and the packages are installed. `cargo test` passes without.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// A directory with a `node` of at least 20 in it: on `PATH`, else the newest of `~/.nvm`.
fn node_dir() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME") {
        let mut found: Vec<PathBuf> =
            std::fs::read_dir(Path::new(&home).join(".nvm/versions/node"))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path().join("bin"))
                .collect();
        found.sort();
        found.reverse();
        dirs.extend(found);
    }
    dirs.into_iter().find(|dir| {
        Command::new(dir.join("node"))
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|v| {
                v.trim()
                    .trim_start_matches('v')
                    .split('.')
                    .next()?
                    .parse::<u32>()
                    .ok()
            })
            .is_some_and(|major| major >= 20)
    })
}

#[test]
fn the_web_apps_unit_tests_pass() {
    let web = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
    let Some(node) = node_dir() else {
        eprintln!("skipped: no Node 20 or newer");
        return;
    };
    if !web.join("node_modules/vitest").is_dir() {
        eprintln!(
            "skipped: the web app's packages are not installed (`cd crates/netsmith-editor/web && npm ci` installs them)"
        );
        return;
    }
    let path = std::env::join_paths(std::iter::once(node).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let out = Command::new("npm")
        .args(["test", "--silent"])
        .env("PATH", path)
        .current_dir(&web)
        .output()
        .expect("npm runs");
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
