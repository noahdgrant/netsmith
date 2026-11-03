//! The catalogs that ship in the binary: `data/jlcpcb/catalog/*.csv`, real parts that JLCPCB
//! assembles, and `data/sample/*.csv`, made-up parts for the solver's tests.
//!
//! This file only loads them. Each CSV is one catalog, named by the file (`resistor.csv` is
//! the `resistor` catalog), and its format is described in [`crate::table`]. Adding a part
//! to a catalog, or a whole catalog, is a change to `data/`, not to this crate:
//! `build.rs` embeds every file in those directories.
//!
//! **The order of the rows is the selection policy** and nothing sorts it here: the files in
//! `data/jlcpcb/catalog/` are ordered Basic before Extended (Extended parts cost a feeder
//! fee), then by the unit price read on a past date, then by ascending value, so the solver
//! takes the cheapest Basic part that works. The files ship no price, stock or date, so only
//! the Basic-before-Extended part of that order can be checked now. `data/jlcpcb/README.md` records where each row came from
//! and which parts were looked at and not offered (`considered.csv`).

use crate::catalog::Catalogs;

include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

/// The catalogs of a set of embedded files. The files are checked by this crate's tests, so
/// one that cannot be read is a bug in the data and stops here with its line.
pub(crate) fn embedded(files: &[(&str, &str, &str)]) -> Catalogs {
    let mut catalogs = Catalogs::empty();
    for (name, file, text) in files {
        if let Err(e) = catalogs.add_file(name, file, text) {
            panic!("{e}");
        }
    }
    catalogs
}
