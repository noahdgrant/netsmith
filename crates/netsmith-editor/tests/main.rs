//! The crate's integration tests, one executable. Each file of this folder is a module of it
//! (a file per subject, as before); a new file is picked up by `automod::dir!` below, with no list to edit.
//! Run one file's tests with `cargo test -p netsmith-editor --test main <file>::`, or with
//! `cargo nextest run -p netsmith-editor <file>::`.

automod::dir!("tests");

/// The build script's own source (`build.rs` includes it too), which `build_web` and `bundle`
/// test: loaded once, here, and not by each of them.
#[allow(dead_code)]
#[path = "../build_web.rs"]
mod build_script;
