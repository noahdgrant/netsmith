//! The web app, built from `web/` by `build.rs` and embedded in the binary.

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The `SOURCE-HASH` of the embedded page, or `None` when the binary has no page.
pub fn bundle_hash() -> Option<&'static str> {
    (!BUNDLE_HASH.is_empty()).then_some(BUNDLE_HASH)
}

/// `git describe --tags --always` at build time, if netsmith was built in a git checkout.
pub fn git_describe() -> Option<&'static str> {
    option_env!("NETSMITH_GIT_DESCRIBE").filter(|s| !s.is_empty())
}

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
