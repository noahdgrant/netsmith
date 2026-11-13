//! The crate's integration tests, one executable. Each file of this folder is a module of it
//! (a file per subject); a new file is picked up by `automod::dir!` below, with no list to edit. Run one
//! file's tests with `cargo test -p netsmith-placer --test main <file>::`, or with
//! `cargo nextest run -p netsmith-placer <file>::`.

// `common` holds the synthetic boards and processes, which the router's, netsmith-3d's and the
// netsmith crate's tests share (`#[path]` modules there).
automod::dir!("tests");
mod common;
