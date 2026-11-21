//! A hand pick (ADR 0122, ADR 0143): a design that narrows a part's `.mpn`, and its
//! `.manufacturer` when two parts share the number, holds it to the catalog row of that part,
//! before the pick lock, and the row is judged by everything the design says. A `.lcsc` is
//! refused with the `.mpn` to write. The rows are the standard resistor catalog's.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, LockedRow, PickLock, Solution, solve};

/// The LED of `examples/led.nsm`, with `R1` in the resistor's block.
fn board(r1: &str) -> String {
    format!(
        r#"
import std::parts::passive::{{Resistor, Led, Switch}}
import std::sources::Rail

board LedExample {{
    .supply: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
        .neg.net &= "GND"
    }}
    .led: Led {{ .current &= 5mA..15mA }}
    .r1: Resistor {{ {r1} }}
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}}
"#
    )
}

fn run(r1: &str, catalogs: &Catalogs) -> Solution {
    let checked = check("main.nsm", &board(r1), &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    solve(checked.standalone_boards().next().unwrap(), catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn lcsc_of_r1(s: &Solution) -> Option<String> {
    s.picks
        .iter()
        .find(|p| s.model.path_of(p.part) == "r1")
        .and_then(|p| p.sku.clone())
}

/// The row the solver picks for `r1` with no hand pick, and one it does not that fits as well.
const SOLVER_ROW: &str = "C25104";
const OTHER_ROW: &str = "C23151";
const OTHER_MPN: &str = "0603WAF3900T5E";

#[test]
fn with_no_hand_pick_the_solver_takes_the_first_row() {
    let s = run("", &Catalogs::jlcpcb());
    assert_eq!(lcsc_of_r1(&s).as_deref(), Some(SOLVER_ROW));
}

#[test]
fn an_lcsc_number_is_refused_with_the_mpn_to_write() {
    let s = run(&format!(".lcsc &= \"{OTHER_ROW}\""), &Catalogs::jlcpcb());
    let said = messages(&s);
    let line = Catalogs::jlcpcb()
        .offers()
        .by_sku(OTHER_ROW)
        .unwrap()
        .at
        .clone();
    assert_eq!(
        said,
        vec![format!(
            "`.lcsc &= \"{OTHER_ROW}\"` on `r1`: a hand pick is by the manufacturer's part number now, and a supplier's number is an offer (ADR 0143): write `.mpn &= \"{OTHER_MPN}\"` in its place, the part the lcsc offer {OTHER_ROW} is of ({line}); `netsmith project migrate-offers` rewrites these narrowings in a project's sources"
        )]
    );
    let d = &s.diagnostics[0];
    let text = board(&format!(".lcsc &= \"{OTHER_ROW}\""));
    assert_eq!(
        &text[d.span.start..d.span.end],
        format!(".lcsc &= \"{OTHER_ROW}\"")
    );
    // A number no offer has says so.
    let s = run(".lcsc &= \"C0000000\"", &Catalogs::jlcpcb());
    assert!(
        messages(&s)[0].contains("no offer has the sku C0000000"),
        "{:?}",
        messages(&s)
    );
}

#[test]
fn a_manufacturer_part_number_holds_the_part_to_its_row() {
    let s = run(&format!(".mpn &= \"{OTHER_MPN}\""), &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(lcsc_of_r1(&s).as_deref(), Some(OTHER_ROW));
}

#[test]
fn a_hand_pick_comes_before_the_lock() {
    let mut lock = PickLock::new("main.lock.toml");
    lock.set(
        "LedExample",
        vec![LockedRow {
            path: "r1".into(),
            catalog: "resistor".into(),
            row: "330Ω 1% 0402 C25104".into(),
            skus: vec![("jlcpcb-assembly".into(), SOLVER_ROW.into())],
            manufacturer: None,
            mpn: Some("0402WGF3300TCE".into()),
            held: None,
        }],
    );
    let held = Catalogs::jlcpcb().with_pick_lock(lock);
    // The lock alone holds the part to its row.
    assert_eq!(lcsc_of_r1(&run("", &held)).as_deref(), Some(SOLVER_ROW));
    // A hand pick is a requirement, and the lock only records what was picked.
    let s = run(&format!(".mpn &= \"{OTHER_MPN}\""), &held);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(lcsc_of_r1(&s).as_deref(), Some(OTHER_ROW));
}

#[test]
fn a_lock_holds_the_row_of_its_manufacturer_when_two_rows_share_a_name() {
    // Two makers of one number: the rows' default name is the mpn, so the two share it, and the
    // lock's manufacturer says which row it holds (ADR 0143).
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file(
            "resistor",
            "catalogs/resistor.csv",
            "manufacturer,mpn,resistance,max_power,package,source\n\
Acme Passives,ZZR-330,330ohm +/- 1%,62.5mW,0402,https://example.invalid/acme\n\
Zenith Passives,ZZR-330,330ohm +/- 1%,62.5mW,0402,https://example.invalid/zenith\n",
        )
        .unwrap();
    let held = |maker: Option<&str>| {
        let mut lock = PickLock::new("main.lock.toml");
        lock.set(
            "LedExample",
            vec![LockedRow {
                path: "r1".into(),
                catalog: "resistor".into(),
                row: "ZZR-330".into(),
                skus: Vec::new(),
                manufacturer: maker.map(str::to_string),
                mpn: Some("ZZR-330".into()),
                held: None,
            }],
        );
        catalogs.clone().with_pick_lock(lock)
    };
    let maker_of_r1 = |s: &Solution| {
        s.picks
            .iter()
            .find(|p| s.model.path_of(p.part) == "r1")
            .and_then(|p| p.manufacturer.clone())
    };
    for maker in ["Acme Passives", "Zenith Passives"] {
        let s = run("", &held(Some(maker)));
        assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
        assert_eq!(maker_of_r1(&s).as_deref(), Some(maker));
    }
    // A lock of version 1 has no manufacturer: either row is the locked one, and the first that
    // fits is taken, as the solver takes it with no lock.
    let s = run("", &held(None));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(maker_of_r1(&s).as_deref(), Some("Acme Passives"));
    // A manufacturer no row of the name has is a changed row, and the message names both.
    let s = run("", &held(Some("Other Maker")));
    let said = messages(&s).join("\n");
    assert!(
        said.contains("the row `ZZR-330` of the `resistor` catalog now has the manufacturer part number ZZR-330, and the lock has ZZR-330 (the manufacturer Acme Passives, and the lock has Other Maker)"),
        "{said}"
    );
    // A hand pick of the number alone offers both parts, and the first that fits is taken.
    let s = run(".mpn &= \"ZZR-330\"", &catalogs);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(maker_of_r1(&s).as_deref(), Some("Acme Passives"));
    // With the manufacturer, it holds that maker's row.
    for maker in ["Acme Passives", "Zenith Passives"] {
        let s = run(
            &format!(".mpn &= \"ZZR-330\"; .manufacturer &= \"{maker}\""),
            &catalogs,
        );
        assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
        assert_eq!(maker_of_r1(&s).as_deref(), Some(maker));
    }
    // A manufacturer with no number picks nothing, and says why.
    let s = run(".manufacturer &= \"Acme Passives\"", &catalogs);
    assert!(
        messages(&s).contains(&"`r1.manufacturer &= \"Acme Passives\"` says which of two parts of one number a hand pick is, and `r1` has no `.mpn` narrowed: a hand pick is by `.mpn` (ADR 0143); narrow `.mpn` too, or remove the narrowing".to_string()),
        "{:?}",
        messages(&s)
    );
}

#[test]
fn a_number_no_row_has_is_an_error_at_the_narrowing() {
    let s = run(".mpn &= \"ZZ-0000\"", &Catalogs::jlcpcb());
    assert_eq!(
        messages(&s),
        vec![
            "`r1` is held by `.mpn &= \"ZZ-0000\"` to a row of the `resistor` catalog, and the catalog has no row with the manufacturer part number ZZ-0000: no other row was tried for it; change the narrowing to a number the catalog has, or remove it to let the solver pick"
        ]
    );
    let d = &s.diagnostics[0];
    let text = board(".mpn &= \"ZZ-0000\"");
    assert_eq!(&text[d.span.start..d.span.end], ".mpn &= \"ZZ-0000\"");
}

#[test]
fn a_number_and_a_manufacturer_of_different_rows_name_no_row() {
    let s = run(
        &format!(".mpn &= \"{OTHER_MPN}\"; .manufacturer &= \"Other Maker\""),
        &Catalogs::jlcpcb(),
    );
    assert!(
        messages(&s).iter().any(|m| m.contains(&format!(
            "the catalog has no row with the manufacturer part number {OTHER_MPN} of the manufacturer Other Maker"
        ))),
        "{:?}",
        messages(&s)
    );
}

#[test]
fn a_hand_picked_row_that_fails_says_no_other_row_was_tried() {
    // 2.2 kohm in 0402 overheats at the LED's current.
    let s = run(".mpn &= \"0402WGF2201TCE\"", &Catalogs::jlcpcb());
    let said = messages(&s).join("\n");
    assert!(said.contains("max_power"), "{said}");
    assert!(
        said.contains("`r1` is held by `.mpn &= \"0402WGF2201TCE\"` to one row of the `resistor` catalog, and no other row was tried for it"),
        "{said}"
    );
}

#[test]
fn a_manufacturer_on_a_part_with_no_catalog_is_an_error_and_an_lcsc_is_refused_there_too() {
    let src = board("").replace(
        "    .supply.pos ~> .r1",
        "    .sw: Switch { .manufacturer &= \"Acme\"; .lcsc &= \"C1\" }\n    .sw.a ~ .supply.pos\n    .sw.b ~ .supply.pos\n    .supply.pos ~> .r1",
    );
    let checked = check("main.nsm", &src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let s = solve(checked.standalone_boards().next().unwrap(), &Catalogs::jlcpcb());
    let said = messages(&s);
    assert!(
        said.contains(
            &"`sw.manufacturer` holds the part to a catalog row, but `sw` (`Switch`) picks from no catalog: remove the narrowing".to_string()
        ),
        "{said:?}"
    );
    assert!(
        said.iter().any(|m| m.starts_with("`.lcsc &= \"C1\"` on `sw`: a hand pick is by the manufacturer's part number now")),
        "{said:?}"
    );
}
