//! The page this build embeds (ADR 0114: no built page is committed; `build.rs` builds it from
//! `web/` with Node 20 or newer, and embeds a placeholder when there is none). When the build had
//! Node, the embedded page is complete and is the one of the sources in `web/` as they are now.
//! When it had none these tests skip with a message, unless `NETSMITH_REQUIRE_BUNDLE_CHECK` is set
//! (CI and releases always have Node), when a placeholder fails them.

use crate::build_script as build_web;

use std::{
    env,
    path::{Path, PathBuf},
};

fn web() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("web")
}

/// Whether the real page is embedded. Without it: fails when `NETSMITH_REQUIRE_BUNDLE_CHECK` is
/// set, else says so and lets the caller skip.
fn have_page() -> bool {
    if netsmith_editor::bundle_is_present() {
        return true;
    }
    assert!(
        env::var_os("NETSMITH_REQUIRE_BUNDLE_CHECK").is_none(),
        "NETSMITH_REQUIRE_BUNDLE_CHECK is set but this build embeds the placeholder page: {}",
        netsmith_editor::missing_bundle_message()
    );
    eprintln!(
        "skipped: this build has no editor page (no Node 20 or newer when it was built; scripts/web-bundle builds it)"
    );
    false
}

fn text(path: &str) -> String {
    let (_, bytes) = netsmith_editor::find_asset(path).unwrap_or_else(|| panic!("no {path}"));
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn the_embedded_page_is_the_real_page_or_the_placeholder_that_says_so() {
    let (_, page) =
        netsmith_editor::find_asset("index.html").expect("every build has an index.html");
    let page = String::from_utf8_lossy(page);
    if netsmith_editor::bundle_is_present() {
        assert!(page.contains("<div id=\"app\">"), "{page}");
        assert!(!page.contains("was not built"), "{page}");
        assert!(netsmith_editor::bundle_hash().is_some());
    } else {
        // The placeholder: one page that says what to do, and nothing loaded from elsewhere.
        assert!(page.contains("The editor's page was not built"), "{page}");
        assert!(
            page.contains("release binary") && page.contains("Node 20"),
            "{page}"
        );
        assert!(!page.contains("<script"), "{page}");
        assert_eq!(netsmith_editor::asset_paths().count(), 1);
        assert_eq!(netsmith_editor::bundle_hash(), None);
    }
}

#[test]
fn the_embedded_page_is_complete_and_of_the_current_sources() {
    if !have_page() {
        return;
    }
    let page = text("index.html");
    assert!(page.contains("<script"), "{page}");
    // Every file the page names is embedded.
    let mut rest = page.as_str();
    while let Some(at) = rest.find("assets/") {
        rest = &rest[at..];
        let end = rest
            .find(|c: char| c == '"' || c == '\'' || c == ')' || c.is_whitespace())
            .unwrap_or(rest.len());
        assert!(
            netsmith_editor::find_asset(&rest[..end]).is_some(),
            "index.html names {} and it is not embedded",
            &rest[..end]
        );
        rest = &rest[end..];
    }
    assert_eq!(
        netsmith_editor::bundle_hash(),
        Some(build_web::source_hash(&web()).as_str()),
        "the embedded page was built from other sources than crates/netsmith-editor/web has now"
    );
}

/// The most the 3D view's chunk (three.js and the view, ADR 0105) may weigh, gzipped.
const CHUNK_3D_BUDGET: usize = 250_000;

/// Text that is only in three.js or in the 3D view, never in the rest of the page.
const ONLY_3D: [&str; 3] = ["WebGLRenderer", "PMREMGenerator", "Drawing the board in 3D"];

fn gzipped(bytes: &[u8]) -> usize {
    use std::io::Write;
    let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    z.write_all(bytes).unwrap();
    z.finish().unwrap().len()
}

/// The 3D view is a chunk of its own, loaded when it first opens (ADR 0105): it is at most
/// 250 kB gzipped, and the page's main chunk (the script `index.html` loads) has none of
/// three.js or of the view in it, only the dynamic import that fetches it.
#[test]
fn the_3d_view_is_a_chunk_of_its_own_within_its_budget() {
    if !have_page() {
        return;
    }
    let page = text("index.html");
    let main = page
        .split("src=\"/")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .find(|src| src.ends_with(".js"))
        .unwrap_or_else(|| panic!("index.html loads no script: {page}"));
    let main_js = text(main);
    for marker in ONLY_3D {
        assert!(
            !main_js.contains(marker),
            "the main chunk {main} has `{marker}` in it: three.js or the 3D view is no longer loaded on demand; import it only through `load3d` (src/pcb3d/mode.ts)"
        );
    }
    let mut chunks: Vec<(String, usize)> = Vec::new();
    for path in netsmith_editor::asset_paths().filter(|p| p.starts_with("assets/")) {
        if path.ends_with(".js") {
            let (_, bytes) = netsmith_editor::find_asset(path).unwrap();
            if String::from_utf8_lossy(bytes).contains("WebGLRenderer") {
                let name = path.trim_start_matches("assets/");
                chunks.push((name.to_string(), gzipped(bytes)));
            }
        }
    }
    assert_eq!(
        chunks.len(),
        1,
        "three.js should be in exactly one chunk: {chunks:?}"
    );
    let (name, size) = &chunks[0];
    assert!(
        main_js.contains(name.as_str()),
        "the main chunk does not name {name}, the chunk it loads for the 3D view"
    );
    assert!(
        *size <= CHUNK_3D_BUDGET,
        "the 3D view's chunk {name} is {size} bytes gzipped, over its budget of {CHUNK_3D_BUDGET} (ADR 0105)"
    );
}
