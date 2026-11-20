//! A test helper: a catalog file written with the `lcsc` and `library` columns a row had before
//! ADR 0143, added as the migration turns it into a catalog file and the offers its rows make
//! (`netsmith_solve::migrate::add_to`). A test whose rows need a supplier's number reads easier
//! with the number on the row; this keeps it there and exercises the migration too.

use netsmith_solve::Catalogs;

/// Add the catalog file `file` of `name`, whose text may have the `lcsc` and `library` columns,
/// and the offers they make.
pub fn add(c: &mut Catalogs, name: &str, file: &str, text: &str) -> Result<(), String> {
    netsmith_solve::migrate::add_to(c, name, file, text)
}
