//! A hand pick (ADR 0122): a design that narrows a part's `.lcsc` or `.mpn` holds it to the
//! catalog row with that number, before the pick lock, and the row is judged by everything the
//! design says. The rows are the standard resistor catalog's.

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
        .and_then(|p| p.lcsc.clone())
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
fn an_lcsc_number_holds_the_part_to_its_row() {
    let s = run(&format!(".lcsc &= \"{OTHER_ROW}\""), &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(lcsc_of_r1(&s).as_deref(), Some(OTHER_ROW));
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
            lcsc: Some(SOLVER_ROW.into()),
            mpn: Some("0402WGF3300TCE".into()),
        }],
    );
    let held = Catalogs::jlcpcb().with_pick_lock(lock);
    // The lock alone holds the part to its row.
    assert_eq!(lcsc_of_r1(&run("", &held)).as_deref(), Some(SOLVER_ROW));
    // A hand pick is a requirement, and the lock only records what was picked.
    let s = run(&format!(".lcsc &= \"{OTHER_ROW}\""), &held);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(lcsc_of_r1(&s).as_deref(), Some(OTHER_ROW));
}

#[test]
fn a_number_no_row_has_is_an_error_at_the_narrowing() {
    let s = run(".lcsc &= \"C0000000\"", &Catalogs::jlcpcb());
    assert_eq!(
        messages(&s),
        vec![
            "`r1` is held by `.lcsc &= \"C0000000\"` to a row of the `resistor` catalog, and the catalog has no row with the LCSC number C0000000: no other row was tried for it; change the narrowing to a number the catalog has, or remove it to let the solver pick"
        ]
    );
    let d = &s.diagnostics[0];
    let text = board(".lcsc &= \"C0000000\"");
    assert_eq!(&text[d.span.start..d.span.end], ".lcsc &= \"C0000000\"");
}

#[test]
fn two_numbers_of_different_rows_name_no_row() {
    let s = run(
        &format!(".lcsc &= \"{SOLVER_ROW}\"; .mpn &= \"{OTHER_MPN}\""),
        &Catalogs::jlcpcb(),
    );
    assert!(
        messages(&s).iter().any(|m| m.contains(&format!(
            "the catalog has no row with the LCSC number {SOLVER_ROW} and the manufacturer part number {OTHER_MPN}"
        ))),
        "{:?}",
        messages(&s)
    );
}

#[test]
fn a_hand_picked_row_that_fails_says_no_other_row_was_tried() {
    // 2.2 kohm in 0402 overheats at the LED's current.
    let s = run(".lcsc &= \"C25879\"", &Catalogs::jlcpcb());
    let said = messages(&s).join("\n");
    assert!(said.contains("max_power"), "{said}");
    assert!(
        said.contains("`r1` is held by `.lcsc &= \"C25879\"` to one row of the `resistor` catalog, and no other row was tried for it"),
        "{said}"
    );
}

#[test]
fn an_lcsc_number_on_a_part_with_no_catalog_is_an_error() {
    let src = board("").replace(
        "    .supply.pos ~> .r1",
        "    .sw: Switch { .lcsc &= \"C1\" }\n    .sw.a ~ .supply.pos\n    .sw.b ~ .supply.pos\n    .supply.pos ~> .r1",
    );
    let checked = check("main.nsm", &src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let s = solve(checked.standalone_boards().next().unwrap(), &Catalogs::jlcpcb());
    assert!(
        messages(&s).contains(
            &"`sw.lcsc` holds the part to a catalog row, but `sw` (`Switch`) picks from no catalog: remove the narrowing".to_string()
        ),
        "{:?}",
        messages(&s)
    );
}
