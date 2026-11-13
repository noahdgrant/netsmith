//! The crate's integration tests, one executable. Each file of this folder is a module of it
//! (a file per subject); a new file is picked up by `automod::dir!` below, with no list to edit. Run one
//! file's tests with `cargo test -p netsmith-3d --test main <file>::`, or with
//! `cargo nextest run -p netsmith-3d <file>::`.

// The test processes are shared with netsmith-place-route's tests and live there, so that a
// change to them selects this crate's tests too (it depends on that one, scripts/ci-changes).
#[path = "../../netsmith-place-route/tests/common/mod.rs"]
mod common;
automod::dir!("tests");
