//! What the checker says when the source is wrong, and what it does not say.

use netsmith_check::*;

const IMPORTS: &str = "\
import std::parts::passive::{Resistor, Capacitor, Led}
import std::sources::Rail
import std::logic::Logic
import std::parts::regulator::LinearRegulator
";

fn run(body: &str) -> Checked {
    check("test.nsm", &format!("{IMPORTS}\n{body}"), &Loader::std())
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

/// Exactly one diagnostic, and it contains `needle`.
fn one_error(body: &str, needle: &str) -> Checked {
    let c = run(body);
    let msgs = messages(&c);
    assert_eq!(
        msgs.len(),
        1,
        "expected one error mentioning `{needle}`, got {msgs:#?}"
    );
    assert!(
        msgs[0].contains(needle),
        "`{}` does not mention `{needle}`",
        msgs[0]
    );
    c
}

#[test]
fn a_value_in_the_wrong_unit_is_refused() {
    one_error(
        "board B {\n    .s: Rail { .voltage &= 5mA }\n}\n",
        "`s.voltage` holds V, but this value is A",
    );
}

#[test]
fn text_where_a_number_belongs_is_refused() {
    one_error(
        "board B {\n    .s: Rail { .voltage &= \"five\" }\n}\n",
        "holds V, but this value is text",
    );
}

#[test]
fn narrowing_to_a_disjoint_range_is_a_contradiction() {
    one_error(
        "board B {\n    .s: Rail {\n        .voltage &= 5V\n        .voltage &= 3V\n    }\n}\n",
        "contradiction on `s.voltage`",
    );
}

#[test]
fn overlapping_narrowing_keeps_the_intersection() {
    let c = run(
        "board B {\n    .s: Rail {\n        .voltage &= 1V..5V\n        .voltage &= 3V..9V\n    }\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let Some(Domain::Quantity(q)) = m.domain_at("s.voltage") else {
        panic!()
    };
    assert_eq!((q.lo(), q.hi()), (3.0, 5.0));
}

#[test]
fn the_order_of_statements_does_not_change_the_result() {
    let forward = run(
        "board B {\n    .s: Rail {\n        .voltage &= 1V..5V\n        .voltage &= 3V..9V\n    }\n    .r: Resistor { .resistance &= 1kohm +/- 1% }\n}\n",
    );
    let backward = run(
        "board B {\n    .r: Resistor { .resistance &= 1kohm +/- 1% }\n    .s: Rail {\n        .voltage &= 3V..9V\n        .voltage &= 1V..5V\n    }\n}\n",
    );
    let (a, b) = (
        &forward.board("B").unwrap().model,
        &backward.board("B").unwrap().model,
    );
    for path in ["s.voltage", "r.resistance"] {
        assert_eq!(a.domain_at(path), b.domain_at(path), "{path}");
    }
}

#[test]
fn a_constraint_may_use_a_member_declared_later() {
    let c = run(
        "board B {\n    .led.current == (.s.voltage - .led.forward_voltage) / .r.resistance\n    .led: Led\n    .s: Rail\n    .r: Resistor\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(c.board("B").unwrap().model.relations().len(), 5);
}

#[test]
fn a_constraint_that_mentions_another_field_is_not_evaluated_early() {
    let c = run(
        "board B {\n    .s: Rail { .voltage &= 5V }\n    .r: Resistor { .resistance &= s.voltage / 1mA }\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    assert_eq!(
        m.domain_at("r.resistance"),
        Some(&Domain::Unset),
        "left for the solver"
    );
    // The bare `s` reached the board's member from inside `.r`'s block.
    let rel = m
        .relations()
        .iter()
        .find(|r| matches!(&r.lhs, RExpr::Field(f) if m.path_of_field(*f) == "r.resistance"))
        .expect("the queued narrowing");
    assert_eq!(rel.op, netsmith_lang::ast::CmpOp::Narrow);
}

#[test]
fn plain_equals_is_refused_with_the_two_ways_to_say_it() {
    one_error(
        "board B {\n    .s: Rail { .voltage = 5V }\n}\n",
        "there is no `=`: use `&=` to narrow a value, or `==` for an equation that must hold",
    );
}

#[test]
fn an_equation_with_mismatched_sides_is_refused() {
    one_error(
        "board B {\n    .s: Rail\n    .led: Led\n    .led.current == .s.voltage\n}\n",
        "the two sides do not match: left is A, right is V",
    );
}

#[test]
fn adding_unlike_quantities_is_refused() {
    one_error(
        "board B {\n    .s: Rail\n    .led: Led\n    .s.voltage == .led.current + .led.forward_voltage\n}\n",
        "cannot add A and V",
    );
}

#[test]
fn dividing_by_a_range_that_includes_zero_is_refused() {
    one_error(
        "board B {\n    .r: Resistor { .resistance &= 1V / 0A }\n}\n",
        "divisor",
    );
}

#[test]
fn unknown_member_names_the_block() {
    one_error(
        "board B {\n    .s: Rail { .nope &= 1V }\n}\n",
        "`nope` is not a member of this Rail",
    );
}

#[test]
fn unknown_member_suggests_the_nearest_member() {
    // A British spelling of `.color` on an `Led`.
    one_error(
        "board B {\n    .led: Led { .colour &= \"RED\" }\n}\n",
        "`colour` is not a member of this Led; did you mean `.color`?",
    );
    // Another case of the same letters.
    one_error(
        "board B {\n    .r: Resistor { .Resistance &= 1kohm }\n}\n",
        "did you mean `.resistance`?",
    );
}

#[test]
fn unknown_member_far_from_every_member_has_no_suggestion() {
    let c = one_error(
        "board B {\n    .s: Rail { .nope &= 1V }\n}\n",
        "`nope` is not a member of this Rail",
    );
    assert!(
        !messages(&c)[0].contains("did you mean"),
        "{:#?}",
        messages(&c)
    );
}

#[test]
fn unknown_name_suggests_the_dot() {
    one_error(
        "board B {\n    .r: Resistor { resistance &= 1kohm }\n}\n",
        "write `.resistance` for a member of this block",
    );
}

#[test]
fn bare_names_do_not_see_the_user_of_a_module() {
    // `v` belongs to the board, but a module's body is checked on its own terms.
    one_error(
        "module M {\n    .r: Resistor { .resistance &= v.voltage / 1mA }\n}\nboard B {\n    .v: Rail\n    .m: M\n}\n",
        "`v` is not defined in any enclosing block",
    );
}

#[test]
fn a_member_missing_from_a_path_names_the_type() {
    one_error(
        "board B {\n    .s: Rail\n    .s.nope.x &= 1V\n}\n",
        "`Rail` has no member `nope`",
    );
}

#[test]
fn values_have_no_members() {
    one_error(
        "board B {\n    .s: Rail\n    .s.voltage.x &= 1V\n}\n",
        "a value has no member `x`",
    );
}

#[test]
fn unknown_types_and_duplicate_members() {
    one_error("board B {\n    .x: Nope\n}\n", "unknown type `Nope`");
    one_error(
        "board B {\n    .x: Resistor\n    .x: Capacitor\n}\n",
        "`x` is already declared in this block",
    );
}

#[test]
fn a_failed_declaration_does_not_cascade() {
    // One error for the unknown type; the uses of `.x` below add nothing.
    one_error(
        "board B {\n    .x: Nope\n    .x.anything &= 1V\n    .x ~> .x\n}\n",
        "unknown type `Nope`",
    );
}

#[test]
fn imports_name_what_is_missing() {
    let c = check(
        "t.nsm",
        "import std::nope::Thing\nboard B {\n    .t: Thing\n}\n",
        &Loader::std(),
    );
    assert_eq!(
        messages(&c),
        ["unknown module `std::nope`"],
        "no cascade from `Thing`"
    );
    let c = check(
        "t.nsm",
        "import std::sources::Nope\nboard B {}\n",
        &Loader::std(),
    );
    assert_eq!(
        messages(&c),
        ["module `std::sources` has no definition `Nope`"]
    );
}

#[test]
fn connecting_unlike_interfaces_is_refused() {
    one_error(
        "board B {\n    .s: Rail\n    .l: Logic\n    .s ~ .l\n}\n",
        "cannot connect a `Rail` to a `Logic`",
    );
}

#[test]
fn connecting_a_pin_to_an_interface_is_refused() {
    one_error(
        "board B {\n    .s: Rail\n    .s.pos ~ .s\n}\n",
        "`~` joins two pins or two interfaces",
    );
}

#[test]
fn a_chain_passes_only_through_parts_with_a_bridge() {
    one_error(
        "board B {\n    .s: Rail\n    .t: LinearRegulator\n    .s.pos ~> .t ~> .s.neg\n}\n",
        "`LinearRegulator` has no `bridge`",
    );
}

#[test]
fn a_chain_cannot_start_from_an_interface() {
    one_error(
        "board B {\n    .s: Rail\n    .r: Resistor\n    .s ~> .r ~> .s.neg\n}\n",
        "not from an interface",
    );
}

#[test]
fn a_chain_may_start_or_end_at_a_part() {
    let c = run(
        "board B {\n    .a: Resistor\n    .b: Resistor\n    .c: Resistor\n    .a ~> .b ~> .c\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    assert_eq!(m.chains().len(), 1);
    assert_eq!(m.chains()[0].parts.len(), 3);
    // a.b joins b.a, and b.b joins c.a.
    assert_eq!(m.nets().len(), 2);
}

#[test]
fn bridge_and_catalog_belong_in_parts() {
    one_error(
        "module M {\n    .a: Pin\n    .b: Pin\n    bridge .a ~> .b\n}\n",
        "`bridge` can only appear inside a part",
    );
    one_error(
        "module M {\n    .a: Pin\n    catalog c { .a }\n}\n",
        "`catalog` can only appear inside a part",
    );
}

#[test]
fn a_catalog_can_list_only_plain_values() {
    one_error(
        "part P {\n    .a: Pin\n    .b: Pin\n    bridge .a ~> .b\n    catalog c { .a }\n}\n",
        "not a plain value",
    );
}

#[test]
fn ampersand_equals_needs_a_field() {
    one_error(
        "board B {\n    .r: Resistor\n    .r &= 1kohm\n}\n",
        "`&=` sets a value, but `.r` is a part",
    );
    one_error(
        "board B {\n    1V &= 2V\n}\n",
        "`&=` needs a field on its left",
    );
}

#[test]
fn a_part_where_a_value_is_expected_is_refused() {
    one_error(
        "board B {\n    .r: Resistor\n    .s: Rail\n    .s.voltage == .r\n}\n",
        "expected a value, but `.r` is a part",
    );
}

#[test]
fn a_tolerance_needs_a_fixed_value_and_a_range_end_may_be_a_value() {
    // A tolerance still needs fixed numbers (ADR 0066 changes ranges only).
    one_error(
        "board B {\n    .s: Rail\n    .r: Resistor\n    .r.resistance &= .s.voltage +/- 5%\n}\n",
        "a tolerance needs a fixed value and a fixed tolerance",
    );
    // A value at a range's end is allowed; its dimension must still match the other end.
    one_error(
        "board B {\n    .s: Rail\n    .t: Rail\n    .t.current &= .s.voltage..5mA\n}\n",
        "cannot form a range from V and A",
    );
    // And the range is of the dimension of the field it narrows.
    one_error(
        "board B {\n    .s: Rail\n    .r: Resistor\n    .r.resistance &= .s.voltage..5V\n}\n",
        "`r.resistance` holds Ω, but this value is V",
    );
}

#[test]
fn a_tolerance_must_be_a_percentage_or_the_same_unit() {
    one_error(
        "board B {\n    .s: Rail { .voltage &= 5V +/- 2mA }\n}\n",
        "a tolerance on V must be a percentage or in the same unit",
    );
}

#[test]
fn connected_pins_must_agree_on_the_net_name() {
    one_error(
        "board B {\n    .a: Resistor {\n        .a.net &= \"X\"\n    }\n    .b: Resistor {\n        .a.net &= \"Y\"\n    }\n    .a.a ~ .b.a\n}\n",
        "name their net differently",
    );
}

#[test]
fn pins_that_agree_on_a_name_share_it() {
    let c = run(
        "board B {\n    .a: Resistor { .a.net &= \"X\" }\n    .b: Resistor { .a.net &= \"X\" }\n    .a.a ~ .b.a\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    assert_eq!(m.nets().len(), 1);
    assert_eq!(m.nets()[0].name, "X");
}

#[test]
fn definitions_that_are_never_used_are_still_checked() {
    let c = run("module Unused {\n    .r: Resistor { .resistance &= 5V }\n}\n");
    assert_eq!(c.boards.len(), 0);
    assert!(
        messages(&c)[0].contains("holds Ω, but this value is V"),
        "{:?}",
        messages(&c)
    );
}

#[test]
fn a_scalar_cannot_have_a_body() {
    one_error(
        "board B {\n    .s: Rail\n    .x: Voltage { .y &= 1V }\n}\n",
        "a plain value has no members",
    );
}

#[test]
fn declarations_need_a_single_dotted_name() {
    one_error(
        "board B {\n    x: Resistor\n}\n",
        "a declaration needs a single member name",
    );
    one_error(
        "board B {\n    .a.b: Resistor\n}\n",
        "a declaration needs a single member name",
    );
}

#[test]
fn rendered_diagnostics_point_into_the_right_file() {
    let c = run("board B {\n    .s: Rail { .voltage &= 5mA }\n}\n");
    let text = c.render();
    // The imports take four lines, then a blank line, so the board starts at line 6.
    assert!(text.contains("test.nsm:7:"), "{text}");
    assert!(text.contains(".voltage &= 5mA"), "{text}");
}

#[test]
fn a_parse_error_in_the_file_is_reported_with_its_position() {
    let c = check("broken.nsm", "board B {\n    .x &=\n}\n", &Loader::std());
    assert_eq!(c.diagnostics.len(), 1);
    assert_eq!(c.diagnostics[0].line_col(&c.sources), (3, 1));
}

#[test]
fn a_parse_error_in_a_library_file_points_at_that_file() {
    let mut loader = Loader::empty();
    loader.insert("lib::bad", "lib/bad.nsm", "part P {\n    .x &=\n}\n");
    let c = check(
        "t.nsm",
        "import lib::bad::P\nboard B {\n    .p: P\n}\n",
        &loader,
    );
    let text = c.render();
    assert!(text.contains("lib/bad.nsm:3:1"), "{text}");
}

// ---- a module is configured only through its interface -------------------------

const STAGE: &str = "\
module Inner {
    .c: Capacitor
}

module Stage {
    .vout: Rail
    .gain: Number
    .r: Resistor { .resistance &= 1kohm }
    .inner: Inner
}

";

fn with_stage(body: &str) -> String {
    format!("{STAGE}{body}")
}

const ONLY_NARROWING: &str = "only narrowing (`&=`) reaches inside a module";

/// The narrowed range of `path` in board `B`.
fn narrowed(src: &str, path: &str) -> netsmith_quantity::Quantity {
    let c = run(src);
    assert!(c.is_ok(), "{}", c.render());
    match c.board("B").unwrap().model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

#[test]
fn a_board_narrows_a_value_inside_a_module_by_path() {
    let q = narrowed(
        &with_stage("board B {\n    .s: Stage\n    .s.r.resistance &= 0.5kohm..2kohm\n}\n"),
        "s.r.resistance",
    );
    // The module's `1kohm` and the board's range, intersected.
    assert_eq!((q.lo(), q.hi()), (1000.0, 1000.0));
}

#[test]
fn a_board_narrows_a_value_inside_a_module_from_its_block() {
    let q = narrowed(
        &with_stage("board B {\n    .s: Stage { .inner.c.capacitance &= 1uF }\n}\n"),
        "s.inner.c.capacitance",
    );
    assert!((q.lo() - 1e-6).abs() < 1e-12, "{q}");
}

#[test]
fn a_narrowing_inside_a_module_that_contradicts_it_is_a_contradiction() {
    one_error(
        &with_stage("board B {\n    .s: Stage\n    .s.r.resistance &= 2kohm\n}\n"),
        "s.r.resistance",
    );
}

#[test]
fn a_module_narrows_a_value_inside_a_module_it_contains() {
    let c = run(
        "module Inner {\n    .c: Capacitor\n}\nmodule Outer {\n    .inner: Inner\n    .inner.c.capacitance &= 1uF\n}\nboard B {\n    .o: Outer\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_relation_cannot_reach_inside_a_module() {
    let c = one_error(
        &with_stage("board B {\n    .s: Stage\n    .s.r.resistance == 1kohm\n}\n"),
        ONLY_NARROWING,
    );
    assert!(messages(&c)[0].starts_with("`r` is inside `Stage`"));
}

#[test]
fn a_bound_cannot_reach_inside_a_module() {
    one_error(
        &with_stage("board B {\n    .s: Stage\n    .s.r.resistance <= 2kohm\n}\n"),
        ONLY_NARROWING,
    );
}

#[test]
fn a_default_cannot_reach_inside_a_module() {
    one_error(
        &with_stage("board B {\n    .s: Stage\n    .s.r.resistance ?= 1kohm\n}\n"),
        ONLY_NARROWING,
    );
}

#[test]
fn a_board_cannot_connect_to_a_pin_inside_a_module() {
    one_error(
        &with_stage("board B {\n    .s: Stage\n    .s.r.a ~ .s.vout.pos\n}\n"),
        ONLY_NARROWING,
    );
}

#[test]
fn reading_a_value_inside_a_module_is_reaching_in_too() {
    // The right side of a `&=` reads; only its left side narrows.
    one_error(
        &with_stage(
            "board B {\n    .s: Stage\n    .x: Resistor\n    .x.resistance &= .s.r.resistance\n}\n",
        ),
        ONLY_NARROWING,
    );
}

#[test]
fn a_module_is_configured_through_its_rails_pins_and_values() {
    let c = run(&with_stage(
        "board B {\n    .s: Stage\n    .p: Rail\n    .s.vout.voltage &= 3V\n    .s.gain &= 2\n    .s.vout ~ .p\n    .s.vout.pos ~> .p.neg\n}\n",
    ));
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    assert!(matches!(
        m.domain_at("s.vout.voltage"),
        Some(Domain::Quantity(_))
    ));
    assert!(matches!(m.domain_at("s.gain"), Some(Domain::Quantity(_))));
}

#[test]
fn a_module_may_name_its_own_parts() {
    // `Stage` sets `.r.resistance` in its own body, and the stage checks clean.
    let c = run(&with_stage("board B {\n    .s: Stage\n}\n"));
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_part_exposes_all_of_its_members() {
    // A part has no inside: a resistor's pins and values are all its interface.
    let c = run("board B {\n    .r: Resistor { .resistance &= 1kohm }\n    .r.a.net &= \"X\"\n}\n");
    assert!(c.is_ok(), "{}", c.render());
}

// ---- net names -----------------------------------------------------------------

#[test]
fn pins_that_ask_for_the_same_name_are_one_net() {
    // Nothing connects them, but both are called GND, so they are the same net.
    let c = run(
        "board B {\n    .a: Resistor { .a.net &= \"GND\" }\n    .b: Resistor { .a.net &= \"GND\" }\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    assert_eq!(m.nets().len(), 1);
    assert_eq!(m.nets()[0].name, "GND");
    assert_eq!(m.nets()[0].terminals.len(), 2);
}

#[test]
fn a_net_nobody_named_is_called_after_its_smallest_terminal() {
    let c = run("board B {\n    .z: Resistor\n    .a: Resistor\n    .z.b ~ .a.a\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    let net = &c.board("B").unwrap().model.nets()[0];
    assert_eq!(net.name, "net-a.a");
    assert!(!net.explicit);
}

#[test]
fn generated_names_do_not_depend_on_declaration_order() {
    let a = run("board B {\n    .z: Resistor\n    .a: Resistor\n    .z.b ~ .a.a\n}\n");
    let b = run("board B {\n    .a: Resistor\n    .z: Resistor\n    .a.a ~ .z.b\n}\n");
    let names = |c: &Checked| -> Vec<String> {
        c.board("B")
            .unwrap()
            .model
            .nets()
            .iter()
            .map(|n| n.name.clone())
            .collect()
    };
    assert_eq!(names(&a), names(&b));
}

#[test]
fn a_generated_name_never_collides_with_one_somebody_chose() {
    let c = run(
        "board B {\n    .a: Resistor { .b.net &= \"net-a.a\" }\n    .z: Resistor\n    .z.b ~ .a.a\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let names: Vec<_> = c
        .board("B")
        .unwrap()
        .model
        .nets()
        .iter()
        .map(|n| n.name.clone())
        .collect();
    assert_eq!(names.len(), 2);
    assert_ne!(names[0], names[1]);
}

#[test]
fn a_net_with_only_ports_is_named_after_its_smallest_port() {
    let c = run("board B {\n    .x: Logic\n    .y: Logic\n    .x.line ~ .y.line\n}\n");
    assert!(c.is_ok(), "{}", c.render());
    let net = &c.board("B").unwrap().model.nets()[0];
    assert!(net.terminals.is_empty());
    assert_eq!(net.name, "net-x.line");
}

// ---- only a board names nets ---------------------------------------------------

#[test]
fn a_module_cannot_name_a_net() {
    one_error(
        "module M {\n    .r: Resistor\n    .r.a.net &= \"X\"\n}\n",
        "`r.a.net` names a net, and only a board names nets",
    );
}

#[test]
fn a_part_cannot_name_a_net() {
    one_error(
        "part P {\n    .a: Pin\n    .a.net &= \"X\"\n}\n",
        "`a.net` names a net, and only a board names nets",
    );
}

#[test]
fn a_module_cannot_name_the_net_of_a_plane() {
    // A plane's net is a net name like a pin's, so only a board may set it.
    one_error(
        "import std::mfg::stackup::InnerLayer\nmodule M {\n    .p: InnerLayer\n    .p.net &= \"GND\"\n}\n",
        "`p.net` names a net, and only a board names nets",
    );
}

#[test]
fn a_board_names_the_net_of_a_plane() {
    let c = run(
        "import std::mfg::stackup::InnerLayer\nboard B {\n    .p: InnerLayer\n    .p.net &= \"GND\"\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn the_board_names_a_modules_nets_through_its_interface() {
    let c = run(&with_stage(
        "board B {\n    .s: Stage { .vout.pos.net &= \"3V3\" }\n}\n",
    ));
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    assert!(m.nets().iter().any(|n| n.name == "3V3" && n.explicit));
}

#[test]
fn two_instances_of_a_module_keep_separate_generated_nets() {
    // With no name to share, each instance's internal node is named after its own path.
    let c = run(
        "module Pair {\n    .a: Resistor\n    .b: Resistor\n    .a.b ~ .b.a\n}\nboard B {\n    .p1: Pair\n    .p2: Pair\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let names: Vec<_> = c
        .board("B")
        .unwrap()
        .model
        .nets()
        .iter()
        .map(|n| n.name.clone())
        .collect();
    assert_eq!(names, ["net-p1.a.b", "net-p2.a.b"]);
}
