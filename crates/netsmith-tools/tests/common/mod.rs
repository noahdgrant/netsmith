//! Helpers the board tests of this crate share.

#![allow(dead_code)]

pub mod port;
// KiCad's electrical rules check of a schematic, shared with netsmith-pipeline's tests.
#[path = "../../../netsmith-pipeline/tests/common/erc.rs"]
pub mod erc;
