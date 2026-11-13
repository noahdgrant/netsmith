//! Helpers the tests share with the umbrella crate's and the command line's.

// The fake LCSC endpoint, which `netsmith` and `netsmith-cli` tests use as well.
#[path = "../../../netsmith/tests/common/endpoints.rs"]
pub mod endpoints;
