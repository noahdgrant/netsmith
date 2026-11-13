//! The crate's integration tests, one executable. Each file of this folder is a module of it
//! (a file per subject, as before); a new file is picked up by `automod::dir!` below, with no list to edit.
//! Run one file's tests with `cargo test -p netsmith --test main <file>::`, or with
//! `cargo nextest run -p netsmith <file>::`.

mod common;
// The synthetic boards and processes of netsmith-placer's tests, for the tests below that
// place and route the example boards (examples, copper, placed, kicad_drc). They live there, so
// that a change to them selects both crates' tests (scripts/ci-changes).
#[path = "../../netsmith-placer/tests/common/mod.rs"]
mod boards;

automod::dir!("tests");
