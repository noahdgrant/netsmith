//! The tests of the `netsmith` binary, one executable. Each file of this folder is a module of
//! it; a new file is picked up by `automod::dir!` below, with no list to edit. A test that does not start
//! the binary belongs to the crate that holds the code (a stage crate, or `netsmith-tools`). Run one file's tests with
//! `cargo test -p netsmith-cli --test main <file>::`.

mod common;

automod::dir!("tests");
