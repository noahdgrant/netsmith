//! The crate's integration tests, one executable. Each file of this folder is a module of it; a
//! new file is picked up by `automod::dir!` below. Run one file's tests with
//! `cargo test -p netsmith-lint --test main <file>::`.

automod::dir!("tests");
