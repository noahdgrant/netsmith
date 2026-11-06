//! Puts the editor's page into the binary. The page is the bundle built from `web/`
//! (Preact and Vite). This script embeds, in this order of preference:
//!
//! 1. the committed bundle in `web-dist/`, when its `SOURCE-HASH` is the hash of the bytes of the
//!    sources in `web/`: no Node is needed, which is the case for a checkout and for a package;
//! 2. a bundle built from the sources (`npm ci` and `npm run build`, see `build_web.rs`), when
//!    Node 20 or newer is installed: the case after a change to `web/`;
//! 3. the committed bundle of older sources, with a build warning that says why nothing was built.
//!
//! The finished bundle is copied into `OUT_DIR` and this script writes `assets.rs`: a table of
//! its files embedded with `include_bytes!`, and `WEB_STATUS`, the reason there is no bundle when
//! there is none. The editor prints that reason when it is started. `NETSMITH_SKIP_WEB=1` skips
//! all of it. `scripts/web-bundle` makes the committed bundle and checks that it is current.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[allow(dead_code)]
mod build_web;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let web = manifest.join("web");
    let committed = manifest.join("web-dist");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    for source in build_web::SOURCES {
        println!("cargo:rerun-if-changed={}", web.join(source).display());
    }
    println!("cargo:rerun-if-changed={}", committed.display());
    println!("cargo:rerun-if-changed=build_web.rs");
    println!("cargo:rerun-if-env-changed=NETSMITH_SKIP_WEB");

    // The bundle that is embedded is a copy made under the build lock: another cargo build in
    // another target directory may empty `web/dist` while this one compiles.
    let bundle = out_dir.join("dist");
    let mut status = String::new();
    let mut warning = String::new();
    // The hash of the sources the embedded page was built from (`Project::bundle_hash`).
    let mut bundle_hash = String::new();
    git_describe();
    if env::var_os("NETSMITH_SKIP_WEB").is_some() {
        let _ = fs::remove_dir_all(&bundle);
        status = "NETSMITH_SKIP_WEB was set when it was built".into();
    } else {
        let dirs = build_web::search_dirs(env::var_os("PATH"), env::var_os("HOME"));
        match build_web::provide(&web, &committed, &dirs, build_web::LOCK_WAIT, &bundle) {
            build_web::Provided::Committed => {
                bundle_hash = stamp_of(&committed);
            }
            build_web::Provided::Built => {
                bundle_hash = build_web::source_hash(&web);
            }
            build_web::Provided::OutOfDate(reason) => {
                bundle_hash = stamp_of(&committed);
                warning = format!(
                    "the editor's page is the committed one in crates/netsmith-editor/web-dist, which was built from older sources than crates/netsmith-editor/web: {reason}"
                );
            }
            build_web::Provided::Missing(reason) => {
                status = reason.clone();
                warning = format!("the editor's web app is not built: {reason}");
            }
        }
    }
    if !warning.is_empty() {
        println!("cargo:warning={}", first_line(&warning));
        for line in warning.lines().skip(1) {
            println!("cargo:warning={line}");
        }
        println!(
            "cargo:warning=build it with: scripts/web-bundle update (needs Node 20 or newer), or by hand: cd crates/netsmith-editor/web && npm ci && npm run build"
        );
        // A path that is never there makes cargo run this script again on the next build, so
        // installing Node (or fixing the source) is enough; no file in `web/` changes then.
        println!("cargo:rerun-if-changed=web/.web-build-failed-so-try-again");
    }

    let mut table = String::from("pub static FILES: &[(&str, &str, &[u8])] = &[\n");
    let mut files = Vec::new();
    collect(&bundle, &bundle, &mut files);
    files.sort();
    for name in files {
        let path = bundle.join(&name);
        table.push_str(&format!(
            "    ({name:?}, {:?}, include_bytes!({:?})),\n",
            mime(&name),
            path.display().to_string()
        ));
    }
    table.push_str("];\n");
    table.push_str(&format!("pub static WEB_STATUS: &str = {status:?};\n"));
    table.push_str(&format!(
        "pub static BUNDLE_HASH: &str = {bundle_hash:?};\n"
    ));
    fs::write(out_dir.join("assets.rs"), table).unwrap();
}

/// The `SOURCE-HASH` a committed bundle carries, or nothing.
fn stamp_of(committed: &Path) -> String {
    fs::read_to_string(committed.join(build_web::BUNDLE_STAMP))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Passes `git describe --tags --always` of the checkout to the crate as
/// `NETSMITH_GIT_DESCRIBE`, when there is a git and a checkout; otherwise nothing is set. The
/// script runs again when the checked-out commit changes (`HEAD` and its log), not when files
/// are edited, so a build with uncommitted changes is not marked.
fn git_describe() {
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let Some(describe) = run(&["describe", "--tags", "--always"]) else {
        return;
    };
    if let Some(dir) = run(&["rev-parse", "--absolute-git-dir"]) {
        for file in ["HEAD", "logs/HEAD"] {
            let path = Path::new(&dir).join(file);
            if path.exists() {
                println!("cargo:rerun-if-changed={}", path.display());
            }
        }
    }
    println!("cargo:rustc-env=NETSMITH_GIT_DESCRIBE={describe}");
}

fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, out);
        } else if let Ok(relative) = path.strip_prefix(root) {
            out.push(
                relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
}

fn mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "json" => "application/json",
        "woff2" => "font/woff2",
        "png" => "image/png",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}
