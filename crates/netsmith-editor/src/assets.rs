//! The web app, built from `web/` by `build.rs` and embedded in the binary.

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The embedded file at `path` (`index.html`, `assets/index-ab12.js`) and its content type.
pub fn find(path: &str) -> Option<(&'static str, &'static [u8])> {
    FILES
        .iter()
        .find(|(name, _, _)| *name == path)
        .map(|(_, mime, bytes)| (*mime, *bytes))
}

/// Whether the web app was built into this binary. It is not when Node was missing or the
/// build failed at compile time; [`missing_bundle_message`] says which.
pub fn bundle_is_present() -> bool {
    find("index.html").is_some()
}

/// What to tell someone who starts the editor of a binary with no web app: the reason the
/// build recorded, and the command that builds it by hand.
pub fn missing_bundle_message() -> String {
    let reason = if WEB_STATUS.is_empty() {
        "the build recorded no reason"
    } else {
        WEB_STATUS
    };
    format!(
        "this netsmith was built without the editor's web app: {reason}\nto build it, install Node 20 or newer, then run `cd crates/netsmith-editor/web && npm ci && npm run build` and build netsmith again (`cargo build` alone does the same)"
    )
}
