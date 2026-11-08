//! The committed copy of the page (`web-dist`) cannot go stale. Always: it is a complete page,
//! and it is stamped with the hash of the sources in `web/` as they are now (a change to a
//! source without `scripts/web-bundle update` fails here, with no Node). With Node 20 or newer
//! and the web app's packages installed (or `NETSMITH_REQUIRE_BUNDLE_CHECK=1`, which installs
//! them): the page the sources build to is, byte for byte, the committed one.

use crate::build_script as build_web;

use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn dirs() -> (PathBuf, PathBuf) {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    (manifest.join("web"), manifest.join("web-dist"))
}

const FIX: &str = "run `scripts/web-bundle update` and commit crates/netsmith-editor/web-dist";

#[test]
fn the_committed_page_is_complete() {
    let (_, committed) = dirs();
    assert!(
        build_web::page_is_complete(&committed),
        "{} has no complete page: {FIX}",
        committed.display()
    );
    let page = fs::read_to_string(committed.join("index.html")).unwrap();
    assert!(page.contains("<script"), "{page}");
    assert!(
        committed.join(build_web::BUNDLE_STAMP).is_file(),
        "{} is missing: {FIX}",
        build_web::BUNDLE_STAMP
    );
}

#[test]
fn the_committed_page_is_stamped_with_the_hash_of_the_current_sources() {
    let (web, committed) = dirs();
    let stamp = fs::read_to_string(committed.join(build_web::BUNDLE_STAMP)).unwrap_or_default();
    let hash = build_web::source_hash(&web);
    assert_eq!(
        stamp.trim(),
        hash,
        "web-dist was built from other sources than crates/netsmith-editor/web has now; {FIX}"
    );
}

#[test]
fn the_committed_page_is_what_the_sources_build_to() {
    let (web, committed) = dirs();
    let required = env::var_os("NETSMITH_REQUIRE_BUNDLE_CHECK").is_some();
    let node_dirs = build_web::search_dirs(env::var_os("PATH"), env::var_os("HOME"));
    let Ok(tools) = build_web::find_node(&node_dirs) else {
        assert!(
            !required,
            "NETSMITH_REQUIRE_BUNDLE_CHECK is set but there is no Node 20"
        );
        eprintln!("skipped: no Node 20 or newer (scripts/web-bundle check does this)");
        return;
    };
    if !required && build_web::install_problem(&web, &tools).is_some() {
        eprintln!(
            "skipped: the web app's packages are not installed (scripts/web-bundle check installs them)"
        );
        return;
    }
    // A build from nothing: not the bundle that happens to be in web/dist.
    let _ = fs::remove_file(web.join("dist").join(build_web::DONE_MARKER));
    build_web::ensure(&web, &node_dirs, build_web::LOCK_WAIT).unwrap();
    let differences = build_web::bundle_differences(&web.join("dist"), &committed);
    assert!(
        differences.is_empty(),
        "web-dist is not what the sources build to:\n  {}\n{FIX}",
        differences.join("\n  ")
    );
}
