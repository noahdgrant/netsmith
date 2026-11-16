//! The text of `netsmith explain` (issue 1008), read from the values of a solved file. Every
//! value here is invented.

use netsmith_pipeline::solved::explain;
use serde_json::{Value, json};

fn boards() -> Vec<Value> {
    vec![json!({
        "board": "Bench",
        "members": [
            {"path": "reg.inductor.current", "unit": "A", "range": [0.005, 0.05], "shown": "5mA..50mA"},
            {"path": "reg.inductor.package", "value": "1210", "shown": "\"1210\""},
            {"path": "register.x", "unit": "", "range": [1.0, 1.0], "shown": "1"},
        ],
        "picks": [
            {"path": "reg.inductor", "catalog": "inductor", "row": "L1", "passed": [
                {"row": "L0", "why": "refused", "message": "too small", "at": "main.nsm:7"},
                {"row": "L00", "why": "unfit", "message": "the package is not 1210"},
            ]},
            {"path": "reg.cap", "catalog": "capacitor", "row": "C1", "count": 2,
             "held": "the pick lock main.lock.toml", "passed": []},
            {"path": "register.r", "catalog": "resistor", "row": "R1", "passed": []},
        ],
    })]
}

#[test]
fn a_part_prints_its_row_the_rows_it_passed_over_and_its_members() {
    let said = explain(&boards(), "reg.inductor", None).unwrap();
    assert_eq!(
        said,
        "board Bench\n  reg.inductor: `L1` from the `inductor` catalog\n    rows passed over (2):\n      `L0`: refused (at main.nsm:7): too small\n      `L00`: unfit: the package is not 1210\n  members:\n    reg.inductor.current  5mA..50mA\n    reg.inductor.package  \"1210\"\n"
    );
}

#[test]
fn a_module_holds_what_is_inside_it_and_not_a_name_that_begins_like_it() {
    let said = explain(&boards(), "reg", None).unwrap();
    assert!(said.contains("  reg.inductor: ") && said.contains("  reg.cap: 2 x `C1`"));
    assert!(!said.contains("register"), "{said}");
}

#[test]
fn a_held_part_says_no_other_row_was_tried_and_the_rows_listed_can_be_capped() {
    let said = explain(&boards(), "reg.cap", None).unwrap();
    assert!(
        said.contains("held by the pick lock main.lock.toml: no other row was tried"),
        "{said}"
    );
    assert!(!said.contains("rows passed over"), "{said}");
    let said = explain(&boards(), "reg.inductor", Some(1)).unwrap();
    assert!(
        said.contains("`L0`: refused") && !said.contains("`L00`"),
        "{said}"
    );
    assert!(said.contains("... and 1 more (`--all-rows` lists every row)"), "{said}");
}

#[test]
fn a_part_of_a_failed_group_prints_the_choices_its_turn_was_made_with() {
    let failed = vec![json!({
        "board": "Bench", "failed": true, "members": [], "picks": [],
        "unpicked": [{"path": "b", "catalog": "resistor", "with": ["a = 100", "c = 5"],
            "passed": [{"row": "R9", "why": "refused", "message": "no", "at": "main.nsm:3"}]}],
    })];
    let said = explain(&failed, "b", None).unwrap();
    assert!(said.starts_with("board Bench (failed)\n"), "{said}");
    assert!(said.contains("  b: no row of the `resistor` catalog could be chosen (its group failed)"));
    assert!(said.contains("    with the parts before it at a = 100, c = 5\n"), "{said}");
}

#[test]
fn nothing_at_the_path_says_which_parts_have_a_record() {
    let e = explain(&boards(), "nowhere", None).unwrap_err();
    assert!(e.contains("at or inside `nowhere`") && e.contains("`reg.inductor`, `reg.cap`"), "{e}");
    assert!(explain(&[], "x", None).unwrap_err().contains("have no picks"));
}
