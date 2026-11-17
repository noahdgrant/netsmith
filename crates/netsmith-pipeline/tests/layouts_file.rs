//! `std/layouts.nsm` (issue #1032): every block is for a row of a catalog of a standard generic,
//! so a mistyped manufacturer's number or a generic that has no such catalog is found here and
//! not by a board that never picks the row.

use netsmith_check::Loader;
use netsmith_pipeline::outcome_with;
use netsmith_solve::Catalogs;

const LAYOUTS: &str = include_str!("../../../std/layouts.nsm");

/// The generics that have blocks, with the CSV of their catalog.
const CATALOGS: [(&str, &str); 1] = [(
    "LinearRegulator",
    include_str!("../../../data/jlcpcb/catalog/ldo.csv"),
)];

/// The manufacturer's part numbers of a catalog file: the second column, which has no comma.
fn mpns(csv: &str) -> Vec<&str> {
    csv.lines()
        .skip(1)
        .filter_map(|l| l.split(',').nth(1))
        .filter(|m| !m.is_empty())
        .collect()
}

#[test]
fn every_block_is_for_a_row_of_a_catalog() {
    let file = netsmith_lang::parse(LAYOUTS).unwrap_or_else(|e| panic!("{}", e.message));
    for block in &file.layouts {
        let keyed = block.keyed.as_ref().expect("a top-level block is keyed");
        let Some((_, csv)) = CATALOGS.iter().find(|(g, _)| *g == keyed.generic.name) else {
            panic!(
                "`layout \"{}\" for {}`: no catalog is listed for the generic in this test",
                keyed.mpn.0, keyed.generic.name
            );
        };
        assert!(
            mpns(csv).contains(&keyed.mpn.0.as_str()),
            "`layout \"{}\" for {}` is not for a row of its catalog",
            keyed.mpn.0,
            keyed.generic.name
        );
    }
}

#[test]
fn the_library_file_checks_with_a_board_that_uses_each_generic() {
    // A mistake in a block (an unknown pin, a source left out) is an error of every board that
    // uses its generic.
    let board = "import std::parts::regulator::LinearRegulator
import std::sources::Rail

board Supply {
    .input: Rail { .voltage &= 5V +/- 5% }
    .reg: LinearRegulator { .vout.voltage &= 3.3V +/- 5% }
    .input ~ .reg.vin
    .load: Rail { .current &= 0mA..50mA }
    .reg.vout ~ .load
}
";
    outcome_with("ldo.nsm", board, &Loader::std(), &Catalogs::jlcpcb(), None)
        .unwrap_or_else(|e| panic!("{e:?}"));
}
