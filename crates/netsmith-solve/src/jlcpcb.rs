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

/// The parts that were looked at and not offered, `data/jlcpcb/considered.csv`: the LCSC number
/// of each part whose reason says it was out of stock when it was read. A project's catalog may
/// still offer such a part, and the build says so (a note, issue #176); the solver does not
/// read stock and the pick is the same.
const CONSIDERED: &str = include_str!("../../../data/jlcpcb/considered.csv");

/// Whether `considered.csv` records a part as out of stock when it was read, by its LCSC number.
pub(crate) fn out_of_stock_when_read(lcsc: &str) -> bool {
    static LISTED: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    LISTED
        .get_or_init(|| listed(CONSIDERED))
        .iter()
        .any(|number| number == lcsc)
}

/// The numbers of the rows of a `considered.csv` whose `not_offered` says `out of stock`. The
/// file is checked by this crate's tests, so one that cannot be read is a bug in the data and
/// stops here.
fn listed(text: &str) -> Vec<String> {
    let records =
        crate::table::parse_csv(text).unwrap_or_else(|e| panic!("data/jlcpcb/considered.csv: {e}"));
    let Some((header, rows)) = records.split_first() else {
        return Vec::new();
    };
    let column = |name: &str| {
        header
            .1
            .iter()
            .position(|c| c == name)
            .unwrap_or_else(|| panic!("data/jlcpcb/considered.csv has no `{name}` column"))
    };
    let (lcsc, reason) = (column("lcsc"), column("not_offered"));
    rows.iter()
        .filter(|(_, row)| {
            row.get(reason)
                .is_some_and(|r| r.to_ascii_lowercase().contains("out of stock"))
        })
        .filter_map(|(_, row)| row.get(lcsc))
        .map(|number| number.trim().to_string())
        .filter(|number| !number.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_considered_file_lists_its_out_of_stock_parts() {
        for number in ["C1723147", "C4346185", "C2505121"] {
            assert!(out_of_stock_when_read(number), "{number}");
        }
        // Considered for another reason (the AMS1117): not out of stock.
        assert!(!out_of_stock_when_read("C6186"));
        assert!(!out_of_stock_when_read("C25104"));
    }

    #[test]
    fn only_a_reason_that_says_out_of_stock_counts() {
        let text = "role,lcsc,not_offered\n\
                    a,C1,Out of stock when read\n\
                    b,C2,wrong footprint\n\
                    c,,out of stock\n";
        assert_eq!(listed(text), vec!["C1".to_string()]);
    }
}
