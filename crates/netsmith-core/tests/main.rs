//! The crate's integration tests, one executable. Each file of this folder is a module of it
//! (a file per subject); a new file is picked up by `automod::dir!` below, with no list to edit. Run one
//! file's tests with `cargo test -p netsmith-core --test main <file>::`, or with
//! `cargo nextest run -p netsmith-core <file>::`.

automod::dir!("tests");
