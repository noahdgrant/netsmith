//! The crate's integration tests, one executable. Each file of this folder is a module of it
//! (a file per subject); a new file is picked up by `automod::dir!` below, with no list to edit. Run one
//! file's tests with `cargo test -p netsmith-router --test main <file>::`, or with
//! `cargo nextest run -p netsmith-router <file>::`.

// `common` holds the synthetic boards and processes, which the placer's, netsmith-3d's and the
// netsmith crate's tests share; it is the placer's (`#[path]` modules here and there).
automod::dir!("tests");
#[path = "../../netsmith-placer/tests/common/mod.rs"]
mod common;
