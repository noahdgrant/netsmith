//! A module's own physical file (ADR 0048, the module layer, issue #813): the module names it,
//! every board that uses the module gets it, it states only how the module's parts stand to
//! one another, a board's file may only narrow it, and an anchor names a part for a board.

use netsmith_check::*;

const DOCK: &str = "\
import std::parts::connector::Header1x2

module Dock {
    physical \"dock.physical.nsm\"
    .row_a: Header1x2
    .row_b: Header1x2
    .row_a.p1 ~ .row_b.p1
}
";

const PAIR: &str = ".row_b.place.x - .row_a.place.x == 17.5mm +/- 0.05mm";

/// A board in `main.nsm` with two docks, the dock's physical unit `dock_body` and the board's
/// `board_body`.
fn run_with(dock: &str, dock_phys: Option<&str>, board_body: &str) -> Checked {
    let mut loader = Loader::std();
    loader.insert("src::dock", "src/dock.nsm", dock);
    if let Some(body) = dock_phys {
        loader.attach_physical(
            "src/dock.nsm",
            "src/dock.physical.nsm",
            &format!("physical for Dock {{\n{body}\n}}\n"),
        );
    }
    loader.attach_physical(
        "main.nsm",
        "main.physical.nsm",
        &format!("physical for B {{\n{board_body}\n}}\n"),
    );
    check(
        "main.nsm",
        "import src::dock::Dock\n\nboard B {\n    .d: Dock\n    .e: Dock\n}\n",
        &loader,
    )
}

fn run(dock_body: &str, board_body: &str) -> Checked {
    run_with(DOCK, Some(dock_body), board_body)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn one_message(c: &Checked) -> String {
    let m = messages(c);
    assert_eq!(m.len(), 1, "{}", c.render());
    m[0].clone()
}

/// The relations of the board that a physical file states, as the fields they relate.
fn related(c: &Checked, file: &str) -> Vec<String> {
    let m = &c.boards[0].model;
    m.relations()
        .iter()
        .filter(|r| c.sources.get(r.source).name == file)
        .map(|r| format!("{} == {}", m.show_expr(&r.lhs), m.show_expr(&r.rhs)))
        .collect()
}

#[test]
fn every_use_of_a_module_gets_its_physical_file() {
    let c = run(PAIR, "");
    assert!(c.is_ok(), "{}", c.render());
    let found = related(&c, "src/dock.physical.nsm");
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].contains("d.row_b.place.x"), "{found:?}");
    assert!(found[1].contains("e.row_b.place.x"), "{found:?}");
}

#[test]
fn a_module_that_names_a_missing_file_is_an_error() {
    let c = run_with(DOCK, None, "");
    assert_eq!(
        one_message(&c),
        "`module Dock` names its physical file `dock.physical.nsm`, and there is no such file beside `src/dock.nsm`; add the file with `physical for Dock { ... }`, or remove this line"
    );
}

#[test]
fn a_module_names_the_file_beside_its_source_by_stem() {
    let dock = DOCK.replace("\"dock.physical.nsm\"", "\"other.physical.nsm\"");
    let c = run_with(&dock, Some(PAIR), "");
    let m = messages(&c);
    assert!(
        m.contains(&"`physical \"other.physical.nsm\"`: a module's physical file is the one beside its source, named by its stem, `dock.physical.nsm`; name that file".to_string()),
        "{m:?}"
    );
}

#[test]
fn a_module_unit_needs_the_statement_and_the_statement_needs_a_unit() {
    let dock = DOCK.replace("    physical \"dock.physical.nsm\"\n", "");
    let c = run_with(&dock, Some(PAIR), "");
    assert_eq!(
        one_message(&c),
        "`physical for Dock` names a module, and `module Dock` does not name this file: add `physical \"dock.physical.nsm\"` to it, so that every board that uses `Dock` gets the file and a missing file is an error (ADR 0048)"
    );
    // A file with no unit for the module that names it.
    let mut loader = Loader::std();
    loader.insert("src::dock", "src/dock.nsm", DOCK);
    loader.attach_physical("src/dock.nsm", "src/dock.physical.nsm", "// nothing yet\n");
    let c = check(
        "main.nsm",
        "import src::dock::Dock\n\nboard B {\n    .d: Dock\n}\n",
        &loader,
    );
    assert_eq!(
        one_message(&c),
        "`module Dock` names its physical file `dock.physical.nsm`, which has no `physical for Dock` unit; add one"
    );
}

#[test]
fn only_a_module_names_its_physical_file() {
    let c = check(
        "main.nsm",
        "board B {\n    physical \"main.physical.nsm\"\n}\n",
        &Loader::std(),
    );
    assert_eq!(
        one_message(&c),
        "`physical \"main.physical.nsm\"` is in board `B`: a board's physical file is found beside its source by stem, `main.physical.nsm`, and the board names none; remove it"
    );
}

#[test]
fn a_module_file_states_no_place_in_the_board_frame() {
    let c = run(".row_a.place.x &= 3mm", "");
    assert_eq!(
        one_message(&c),
        "`.row_a.place.x` is where a part is in the board's frame, and the physical file of module `Dock` states only how its parts stand to one another, the same wherever the board puts the module (`.b.place.x - .a.place.x == 2.54mm`, a `row`); state the place in the physical file of the board that uses `Dock`"
    );
    // A sum of two places moves with the module: it is the board's.
    let c = run(".row_a.place.x + .row_b.place.x == 10mm", "");
    assert!(
        one_message(&c).contains("is where a part is in the board's frame"),
        "{}",
        c.render()
    );
    // A centre between two places, a line across them and a row are free of the frame.
    let c = run(
        "(.row_a.place.x + .row_b.place.x) / 2 - .row_a.place.x == 1mm\n.row_b.place.y == .row_a.place.y",
        "",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_module_file_declares_nothing() {
    let c = run(
        "case {\n    .edge: Outline { .width &= 10mm\n .height &= 10mm }\n}",
        "",
    );
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.starts_with("a `case { }` section belongs in a board's physical file")),
        "{m:?}"
    );
}

#[test]
fn a_board_file_narrows_the_module_window() {
    let c = run(
        PAIR,
        ".d.row_b.place.x - .d.row_a.place.x == 17.5mm +/- 0.01mm\n.e.row_a.place.x - .e.row_b.place.x == -17.52mm +/- 0.01mm",
    );
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_board_file_that_loosens_or_disagrees_names_both_lines() {
    let c = run(
        PAIR,
        ".d.row_b.place.x - .d.row_a.place.x == 17.5mm +/- 0.1mm",
    );
    assert_eq!(
        one_message(&c),
        "`.d.row_b.place.x - .d.row_a.place.x == 17.5mm +/- 0.1mm` would loosen the physical file of module `Dock`, which states `.row_b.place.x - .row_a.place.x == 17.5mm +/- 0.05mm` (src/dock.physical.nsm:2): it leaves `d.row_b.place.x - d.row_a.place.x` between 17.4mm and 17.6mm, and the module's file between 17.45mm and 17.55mm; a physical file may only narrow what a module's states (ADR 0048), so tighten the window to lie inside the module's, or change the module"
    );
    let c = run(PAIR, ".e.row_b.place.x - .e.row_a.place.x >= 20mm");
    let m = one_message(&c);
    assert!(
        m.starts_with("`.e.row_b.place.x - .e.row_a.place.x >= 20mm` disagrees with the physical file of module `Dock`"),
        "{m}"
    );
    assert!(m.contains("at least 20mm"), "{m}");
}

#[test]
fn an_anchor_names_a_part_for_the_board_file() {
    let c = run(
        "anchor .dock = .row_a\n.row_b.place.x - .dock.place.x == 17.5mm",
        ".d.dock.place.left &= 8mm +/- 0.2mm",
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    // Only the dock the board's line names is narrowed.
    assert_ne!(
        m.domain_at("d.row_a.place.left"),
        m.domain_at("e.row_a.place.left")
    );
}

#[test]
fn an_anchor_is_a_name_of_its_own_in_a_module_file() {
    let c = run("anchor .row_b = .row_a", "");
    assert_eq!(
        one_message(&c),
        "`anchor .row_b` would hide the member `.row_b` of `Dock`; an anchor is a name of its own"
    );
    let c = run("", "anchor .dock = .d");
    assert!(
        one_message(&c).starts_with("an `anchor` belongs in a module's physical file, where it names a part inside the module"),
        "{}",
        c.render()
    );
    let c = run("anchor .dock = .row_a.p1", "");
    assert!(
        one_message(&c).contains("names something that is not a part or a module inside `Dock`"),
        "{}",
        c.render()
    );
}

#[test]
fn a_rename_inside_a_module_names_the_broken_path() {
    let c = run("", ".d.row_c.place.x &= 1mm");
    assert_eq!(
        one_message(&c),
        "`.d.row_c.place.x` names nothing: module `Dock` has no member `row_c`. A physical file reaches inside a module by the path of the part, so a rename inside `Dock` breaks this path; an `anchor .name = .part` line in the physical file of `Dock` gives a part a name that a rename inside the module changes in one place; did you mean `.row_a`?"
    );
    let c = run("anchor .dock = .row_a", ".d.dok.place.x &= 1mm");
    let m = one_message(&c);
    assert!(m.contains("did you mean `.dock`?"), "{m}");
    assert!(m.ends_with("`Dock` has the anchors `.dock`; did you mean `.dock`?"), "{m}");
}
