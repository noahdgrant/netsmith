//! The bill of materials names the order code of a part that states `.variants` (issue 1311):
//! the code that has every feature the board needs, as the part's number and, for a part that
//! picks from no catalog, its value.

use netsmith_check::{Loader, check};
use netsmith_design::design_board;
use netsmith_solve::{Catalogs, solve};

const MODEM: &str = r#"
part Modem {
    .variants {
        "MDM-100-G" GNSS LTE
        "MDM-100" LTE
    }
    pins { .a passive "A" "1" }
    CATALOG
}

board B {
    .u: Modem
    .u.needs &= NEED
}
"#;

fn sourced(catalog: &str, need: &str, catalogs: &Catalogs) -> netsmith_design::Sourced {
    let src = MODEM.replace("CATALOG", catalog).replace("NEED", need);
    let c = check("t.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], catalogs);
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let d = design_board(&c.boards[0], &s).unwrap_or_else(|e| panic!("{e:?}"));
    d.sourcing.into_iter().next().unwrap()
}

#[test]
fn a_part_with_no_catalog_is_listed_by_its_code() {
    let s = sourced("", "GNSS", &Catalogs::jlcpcb());
    assert_eq!(s.mpn.as_deref(), Some("MDM-100-G"));
    assert_eq!(s.value, "MDM-100-G");
    assert_eq!(s.lcsc, None);
}

#[test]
fn a_part_with_a_keyed_catalog_takes_the_row_of_its_code() {
    let mut catalogs = Catalogs::empty();
    netsmith_solve::migrate::add_to(&mut catalogs, 
            "modem",
            "catalogs/modem.csv",
            "mpn,lcsc,source\nMDM-100,C900001,test row\nMDM-100-G,C900002,test row\n",
        )
        .unwrap();
    let s = sourced("catalog modem by .mpn { .mpn }", "GNSS", &catalogs);
    assert_eq!(s.mpn.as_deref(), Some("MDM-100-G"));
    assert_eq!(s.lcsc.as_deref(), Some("C900002"));
    let s = sourced("catalog modem by .mpn { .mpn }", "LTE\n    .u.mpn &= \"MDM-100\"", &catalogs);
    assert_eq!(s.lcsc.as_deref(), Some("C900001"));
}
