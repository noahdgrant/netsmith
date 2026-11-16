//! `shared NAME [limit N] { .load ... }` in a board or a module (issue #806): what the checker
//! reads and what it refuses. What the group does to the currents is in the solver's tests
//! (`crates/netsmith-solve/tests/load_groups.rs`).

use netsmith_check::*;

const IMPORTS: &str = "import std::sources::Rail\n";

fn checked(body: &str) -> Checked {
    let src = format!(
        "{IMPORTS}\nmodule Zone {{\n    .vdd: Rail\n}}\n\nboard B {{\n    .a: Rail\n    .b: Rail\n    .c: Rail\n    .z: Zone\n{body}}}\n"
    );
    check("main.nsm", &src, &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn only(body: &str, message: &str) {
    let c = checked(body);
    assert_eq!(messages(&c), [message], "{}", c.render());
}

#[test]
fn a_board_states_a_group_of_its_loads() {
    let c = checked("    shared HEATERS limit 2 { .a .b .c .z.vdd }\n");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").expect("the board").model;
    let groups = m.load_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "HEATERS");
    assert_eq!(groups[0].limit, 2);
    let members: Vec<String> = groups[0]
        .members
        .iter()
        .map(|&(_, f)| m.path_of_field(f))
        .collect();
    assert_eq!(members, ["a.current", "b.current", "c.current", "z.vdd.current"]);
}

#[test]
fn a_module_states_a_group_for_each_of_its_instances() {
    let src = format!(
        "{IMPORTS}\nmodule Pair {{\n    .x: Rail\n    .y: Rail\n    shared ONE {{ .x .y }}\n}}\n\nboard B {{\n    .p: Pair\n    .q: Pair\n}}\n"
    );
    let c = check("main.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").expect("the board").model;
    let scopes: Vec<String> = m.load_groups().iter().map(|g| m.path_of(g.scope)).collect();
    assert_eq!(scopes, ["p", "q"]);
}

#[test]
fn a_member_is_a_rail_by_its_path() {
    only(
        "    shared S { .a heater }\n",
        "a `shared` group of a board or a module lists loads by their path (`.heater`), each a rail that draws; `heater` is a name, which only a part's `shared` lists (a peripheral or a function)",
    );
    only(
        "    shared S { .a .z }\n",
        "`.z` is a `Zone`, and a `shared` group of a board or a module lists loads: rails that draw, such as `.heater: Rail`",
    );
    only(
        "    shared S { .a .b.voltage }\n",
        "`.b.voltage` is a value, and a `shared` group of a board or a module lists loads: rails that draw, such as `.heater: Rail`",
    );
    only(
        "    shared S { .a .a.pos }\n",
        "`.a.pos` is a pin, and a `shared` group of a board or a module lists loads: rails that draw, such as `.heater: Rail`",
    );
}

#[test]
fn a_load_is_listed_once_and_is_in_one_group() {
    only(
        "    shared S { .a .b .a }\n",
        "`.a` is listed twice in `shared S`",
    );
    only(
        "    shared S { .a .b }\n    shared T { .b .c }\n",
        "`.b` is already a load of `shared S` at main.nsm:12; a load is in one group",
    );
}

#[test]
fn a_group_is_stated_once() {
    only(
        "    shared S { .a .b }\n    shared S { .c .z.vdd }\n",
        "`shared S` is already stated at main.nsm:12; one statement lists all of its loads",
    );
}

#[test]
fn a_limit_is_fewer_than_the_loads() {
    only(
        "    shared S limit 2 { .a .b }\n",
        "`shared S` allows 2 of its 2 loads on at once, which is every one of them, so it says nothing; list more loads or give a smaller limit",
    );
    only(
        "    shared S { .a }\n",
        "`shared S` lists one load, which is on whenever it is; a group lists two or more loads that take turns",
    );
}
