//! Helpers the tests share with the other crates' (`netsmith-workspace`, `netsmith-cli`).

#![allow(dead_code)]

// The fake LCSC endpoint, which the tests of the editor's backend and the command line use as well.
pub mod endpoints;
// KiCad's electrical rules check of a schematic, shared with netsmith-pipeline's tests.
#[path = "../../../netsmith-pipeline/tests/common/erc.rs"]
pub mod erc;
