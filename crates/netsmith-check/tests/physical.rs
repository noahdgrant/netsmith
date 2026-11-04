//! A board's physical file (ADR 0048): pairing, the reach into modules, what it may state,
//! and the old form leaving the circuit source.

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::connector::Header1x2
import std::parts::passive::Resistor
import std::parts::mechanical::{Fiducial, Label, MountingHole}
import std::pcb::{Cutout, Keepout, Outline}

module Block {
    .j: Header1x2
    .r: Resistor {}
    .j.p1 ~> .r ~> .j.p2
}

";

const PHYS_HEAD: &str = "\
import std::parts::connector::Header1x2
import std::parts::mechanical::{Fiducial, Label, MountingHole}
import std::pcb::{Cutout, Keepout, Outline}
";

const BOARD: &str = "board B {\n    .b: Block\n    .j: Header1x2\n}\n";

fn run(circuit: &str, physical: &str) -> Checked {
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", physical);
    check("t.nsm", &format!("{HEAD}{circuit}"), &loader)
}

fn physical(body: &str) -> Checked {
    run(BOARD, &format!("{PHYS_HEAD}physical for B {{\n{body}\n}}\n"))
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_physical_file_reaches_a_part_inside_a_module_by_its_path() {
    let c = physical("    .b.j.place.left &= 8mm +/- 0.2mm\n    .b.r.place.side &= \"bottom\"");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    assert!(m.field_at("b.j.place.left").is_some());
    assert_eq!(
        m.domain_at("b.r.place.side"),
        Some(&Domain::Text("bottom".into()))
    );
}

#[test]
fn the_circuit_source_still_cannot_reach_into_a_module() {
    let c = run(
        "board B {\n    .b: Block\n    .b.j.pin_count &= 2\n}\n",
        "",
    );
    assert_eq!(
        messages(&c),
        [
            "`j` is inside `Block` and is not part of its interface; set it through a rail, a pin, or a value that `Block` declares"
        ]
    );
}

#[test]
fn relations_between_parts_use_the_equation_form() {
    let c = physical(
        "    .b.j.place.x - .j.place.x == 17.5mm +/- 0.05mm\n    .b.r.place.y == .j.place.y",
    );
    assert!(c.is_ok(), "{}", c.render());
    // The finding of a relation points at the physical file.
    let in_file = |c: &Checked| {
        c.boards[0]
            .model
            .relations()
            .iter()
            .filter(|r| c.sources.get(r.source).name == "t.physical.nsm")
            .count()
    };
    assert_eq!(in_file(&c), 2);
}

#[test]
fn a_physical_file_may_not_change_what_a_part_is() {
    let c = physical("    .b.r.resistance &= 10kohm\n    .b.j.pin_count &= 3");
    let m = messages(&c);
    assert_eq!(m.len(), 2, "{m:?}");
    assert!(
        m[0].starts_with("`b.r.resistance` is not physical intent: a physical file states where a part is"),
        "{}",
        m[0]
    );
    assert!(m[0].ends_with("put this in the circuit source"), "{}", m[0]);
}

#[test]
fn a_relation_that_mentions_the_circuit_is_refused_too() {
    let c = physical("    .b.r.place.x >= .b.r.resistance");
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].starts_with("`b.r.resistance` is not physical intent"), "{}", m[0]);
}

#[test]
fn a_physical_file_may_narrow_the_height_of_a_part() {
    let c = physical("    .b.r.height &= 1.2mm");
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_physical_file_may_not_connect_or_define() {
    let c = physical("    .j.p1 ~ .b.j.p1\n    .b.j.p1 ~> .b.j.p2\n    tie .j.p1 ~ .j.p2");
    let m = messages(&c);
    assert_eq!(m.len(), 3, "{m:?}");
    assert!(
        m[0].contains("a connection with `~` changes the circuit; put it in the circuit source"),
        "{}",
        m[0]
    );
    assert!(m[2].contains("a `tie` changes the circuit"), "{}", m[2]);
}

#[test]
fn the_case_section_holds_the_outline_cutouts_holes_fiducials_and_labels() {
    let c = physical(
        "    case {\n        .outline: Outline { .width &= 44mm; .height &= 72mm }\n        .window: Cutout { .x &= 3mm; .y &= 4mm; .width &= 5mm; .height &= 3mm }\n        .hole1: MountingHole\n        .fid: Fiducial\n        .rev: Label { .text &= \"REV A\" }\n        .j.place.bottom >= .outline.height\n    }\n    .zone: Keepout { .x &= 8mm; .y &= 20mm; .width &= 28mm; .height &= 24mm }",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    assert!(m.field_at("outline.width").is_some());
    assert!(m.field_at("zone.width").is_some());
    assert!(m.inst_at("hole1").is_some());
    assert!(m.relations().iter().any(|r| c.sources.get(r.source).name == "t.physical.nsm"));
}

#[test]
fn an_outline_outside_the_case_section_says_where_it_goes() {
    let c = physical("    .outline: Outline { .width &= 44mm }");
    assert_eq!(
        messages(&c),
        ["an outline belongs in the `case { }` section of `physical for B`: wrap `.outline: Outline` in `case { ... }`"]
    );
}

#[test]
fn a_physical_file_declares_no_part_or_module() {
    let c = physical("    .extra: Header1x2");
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].starts_with("a physical file declares the outline, cutouts, keepouts, mounting holes, fiducials and labels of a board; `.extra: Header1x2` is none of them"),
        "{}",
        m[0]
    );
}

#[test]
fn the_old_form_in_a_boards_circuit_source_is_an_error_that_says_where_it_goes() {
    let c = run(
        "board B {\n    .b: Block\n    .j: Header1x2\n    .outline: Outline { .width &= 44mm }\n    .window: Cutout { .x &= 3mm }\n    .zone: Keepout { .x &= 8mm }\n    .hole: MountingHole\n    .fid: Fiducial\n    .rev: Label\n    .j.place.x &= 3mm\n    .j.place.y >= .b.j.place.y\n}\n",
        "",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 8, "{m:#?}");
    assert_eq!(
        m[0],
        "`.outline: Outline` is an outline, which is physical intent, and a board's circuit source does not hold it (ADR 0048); declare it in the `case { }` section of `physical for B` in `t.physical.nsm`"
    );
    assert!(m[2].contains("a keepout that a board draws") && m[2].contains("(a keepout that a part carries stays in the part)"), "{}", m[2]);
    assert_eq!(
        m[6],
        "`j.place.x` is a placement requirement, which is physical intent, and a board's circuit source does not state it (ADR 0048); put it in `physical for B` in `t.physical.nsm`"
    );
}

#[test]
fn a_keepout_in_a_part_or_a_module_stays() {
    let c = run(
        "module Fenced {\n    .keep: Keepout { .width &= 2mm }\n}\nboard B {\n    .f: Fenced\n}\n",
        "",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_unit_for_a_board_that_is_not_there_lists_the_boards() {
    let c = run(BOARD, "physical for Nope {}\n");
    assert_eq!(
        messages(&c),
        ["`physical for Nope` names no board of the source it is paired with; its boards are `B`"]
    );
}

#[test]
fn a_unit_for_a_module_is_refused_until_the_module_layer_exists() {
    let c = run(BOARD, "physical for Block {}\n");
    let m = messages(&c);
    assert_eq!(m.len(), 1);
    assert!(m[0].contains("names a module, and a physical file for anything but a board is not built yet"), "{}", m[0]);
}

#[test]
fn a_file_holds_the_units_of_several_boards_and_each_only_once() {
    let src = "board A {\n    .j: Header1x2\n}\nboard B {\n    .j: Header1x2\n}\n";
    let c = run(
        src,
        "physical for A { .j.place.x &= 1mm }\nphysical for B { .j.place.x &= 2mm }\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        c.board("A").unwrap().model.domain_at("j.place.x").is_some(),
        true
    );
    let c = run(src, "physical for A {}\nphysical for A {}\n");
    assert_eq!(
        messages(&c),
        ["`physical for A` is already given in this file; keep one unit for each board"]
    );
}

#[test]
fn a_physical_unit_in_a_circuit_source_says_to_use_a_file_of_its_own() {
    let c = check(
        "led.nsm",
        "board B {}\nphysical for B {}\n",
        &Loader::std(),
    );
    assert_eq!(
        messages(&c),
        ["`physical for B` belongs in a file of its own, `led.physical.nsm`, beside this source; a source file holds the circuit"]
    );
}

#[test]
fn a_physical_file_holds_no_definitions() {
    let c = run(BOARD, "board X {}\nphysical for B {}\n");
    assert_eq!(
        messages(&c),
        ["a physical file holds `physical for` units and nothing else; `board X` belongs in the circuit source"]
    );
}

#[test]
fn a_case_section_outside_a_physical_file_is_refused() {
    let c = run("board B {\n    case {\n    }\n}\n", "");
    assert_eq!(messages(&c).len(), 1);
    assert!(messages(&c)[0].starts_with("a `case { }` section belongs in a physical file"));
}

#[test]
fn a_name_in_the_physical_file_that_does_not_exist_is_named() {
    let c = physical("    .b.nope.place.x &= 1mm");
    assert_eq!(messages(&c), ["`Block` has no member `nope`"]);
    assert_eq!(c.sources.get(c.diagnostics[0].source).name, "t.physical.nsm");
}

#[test]
fn a_board_made_inside_a_system_keeps_its_physical_file() {
    let c = run(
        "board B {\n    .j: Header1x2\n}\nsystem S {\n    .main: B\n}\n",
        "physical for B { .j.place.x &= 1mm }\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    let sys = c.system("S").unwrap();
    assert!(sys.model.field_at("main.j.place.x").is_some());
}
