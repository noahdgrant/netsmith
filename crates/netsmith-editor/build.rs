//! Puts the editor's page into the binary. The page is the bundle built from `web/` (Preact and
//! Vite), and no built page is committed (ADR 0114). This script embeds, in this order:
//!
//! 1. the page built from the sources (`npm ci` and `npm run build`, see `build_web.rs`), when
//!    Node 20 or newer is installed (releases and CI always have it);
//! 2. otherwise a small placeholder page that says the editor's page was not built and what to
//!    do (use a release binary, or install Node and build again), with a build warning that says
//!    why nothing was built. `cargo build` with no Node still succeeds.
//!
//! The finished page is copied into `OUT_DIR` and this script writes `assets.rs`: a table of its
//! files embedded with `include_bytes!`, `WEB_STATUS`, the reason there is no page when there is
//! none, and `WEB_PLACEHOLDER`, whether the only page is the placeholder. The editor prints that
//! reason when it is started. `NETSMITH_SKIP_WEB=1` skips the build and embeds the placeholder.
//! `scripts/web-bundle` builds the page by hand.
//!
//! **Why Node is part of the build (ADR 0019).** The page's source editor is CodeMirror 6, which
//! has done well the work of undo that survives a file switch, selections, search, a gutter
//! that scrolls with the text, squiggles and bracket matching. A small editor of our own (a
//! `<textarea>` painted over by a `<pre>`) would have to do that work again, and the editor is
//! the product. The grammar for `.nsm` is ours either way (`web/src/nsm.ts`). The cost is
//! Node 20 or newer, found on `PATH` or in `~/.nvm/versions/node/*/bin` (`web/.nvmrc` says 22),
//! for a build with the page; a build without it gets the placeholder. `web/dist` and
//! `node_modules` are not committed. Packages are installed again when `node_modules` was
//! not installed by this build, from this lockfile and under this Node major, or lacks `tsc` or
//! `vite`. When a step fails, the reason (the command, the directory, the Node and npm versions,
//! the start of `PATH`, the exit code and the last lines of its output) is printed as cargo
//! warnings and compiled into the binary: `netsmith editor` prints it with the manual command,
//! and the build runs again next time even if no file changed. Building needs the npm registry
//! once; running never does.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[allow(dead_code)]
mod build_web;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let web = manifest.join("web");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    for source in build_web::SOURCES {
        println!("cargo:rerun-if-changed={}", web.join(source).display());
    }
    println!("cargo:rerun-if-changed=build_web.rs");
    println!("cargo:rerun-if-env-changed=NETSMITH_SKIP_WEB");

    // The bundle that is embedded is a copy made under the build lock: another cargo build in
    // another target directory may empty `web/dist` while this one compiles.
    let bundle = out_dir.join("dist");
    let mut status = String::new();
    let mut warning = String::new();
    // The hash of the sources the embedded page was built from (`Project::bundle_hash`); empty
    // for the placeholder.
    let mut bundle_hash = String::new();
    git_describe();
    if env::var_os("NETSMITH_SKIP_WEB").is_some() {
        let _ = fs::remove_dir_all(&bundle);
        status = "NETSMITH_SKIP_WEB was set when it was built".into();
    } else {
        let dirs = build_web::search_dirs(env::var_os("PATH"), env::var_os("HOME"));
        match build_web::provide(&web, &dirs, build_web::LOCK_WAIT, &bundle) {
            build_web::Provided::Built => {
                bundle_hash = build_web::source_hash(&web);
            }
            build_web::Provided::Missing(reason) => {
                status = reason.clone();
                warning = format!(
                    "the editor's page was not built, so this binary has a placeholder page instead: {reason}"
                );
            }
        }
    }
    let placeholder = !status.is_empty();
    if placeholder {
        let _ = fs::remove_dir_all(&bundle);
        fs::create_dir_all(&bundle).unwrap();
        fs::write(bundle.join("index.html"), placeholder_page(&status)).unwrap();
    }
    if !warning.is_empty() {
        println!("cargo:warning={}", first_line(&warning));
        for line in warning.lines().skip(1) {
            println!("cargo:warning={line}");
        }
        println!(
            "cargo:warning=use a release binary, or install Node 20 or newer and build again (scripts/web-bundle builds the page by hand)"
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
        "pub static WEB_PLACEHOLDER: bool = {placeholder};\n"
    ));
    table.push_str(&format!(
        "pub static BUNDLE_HASH: &str = {bundle_hash:?};\n"
    ));
    fs::write(out_dir.join("assets.rs"), table).unwrap();
}

/// The page of a binary with no editor page: it says so and what to do about it, with the reason
/// the build recorded. Nothing in it is loaded from elsewhere.
fn placeholder_page(reason: &str) -> String {
    let reason = reason
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><title>netsmith editor: page not built</title>\n<style>body{{font:16px/1.5 system-ui,sans-serif;max-width:42rem;margin:4rem auto;padding:0 1rem}}pre{{white-space:pre-wrap;background:#8881;padding:.75rem}}</style></head>\n<body><h1>The editor's page was not built</h1>\n<p>This netsmith was built without Node, so it has no editor page to show. Use a release binary, or install Node 20 or newer and build netsmith again (<code>cargo build</code>).</p>\n<p>Why the page was not built:</p>\n<pre>{reason}</pre></body></html>\n"
    )
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
