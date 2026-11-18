//! Groups of nets matched in length as the source states them (issue #1252): a
//! `std::sources::LengthGroup` has as many lines as the indices the board joins, and the design
//! carries its nets and the top of its tolerance to `Features::length_groups`.

use netsmith_check::{Loader, check};
use netsmith_core::{Design, LengthGroup};
use netsmith_design::design_board;
use netsmith_solve::{Catalogs, solve};

fn designed(src: &str) -> Result<Design, Vec<String>> {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s).map(|l| l.design)
}

/// A board with three resistors from `D0`, `D1` and `D2` to `GND`, and `extra` statements.
fn board(extra: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\nimport std::sources::LengthGroup\nboard B {{\n    .rs: 3 x Resistor {{\n        .resistance &= 1kohm +/- 5%\n        .package &= \"0402\"\n        .b.net &= \"GND\"\n    }}\n    .rs[0].a.net &= \"D0\"\n    .rs[1].a.net &= \"D1\"\n    .rs[2].a.net &= \"D2\"\n{extra}}}\n"
    )
}

fn errors_of(src: &str) -> String {
    designed(src).unwrap_err().join("\n")
}

#[test]
fn a_length_group_has_a_line_for_each_index_the_board_joins() {
    let d = designed(&board(
        "    .lane: LengthGroup {\n        .tolerance &= 0mm..0.5mm\n    }\n    .lane.lines[0] ~ .rs[0].a\n    .lane.lines[2] ~ .rs[2].a\n    .lane.lines[1] ~ .rs[1].a\n",
    ))
    .unwrap();
    assert_eq!(
        d.features.length_groups,
        [LengthGroup {
            path: "lane".to_string(),
            nets: vec!["D0".to_string(), "D1".to_string(), "D2".to_string()],
            tolerance: 0.5,
        }]
    );
}

#[test]
fn a_line_is_named_inside_the_groups_block_too() {
    let d = designed(&board(
        "    .lane: LengthGroup {\n        .tolerance &= 0mm..0.25mm\n        .lines[0].net &= \"D1\"\n        .lines[1].net &= \"D0\"\n    }\n",
    ))
    .unwrap();
    let group = &d.features.length_groups[0];
    assert_eq!(group.nets, ["D1", "D0"]);
    assert_eq!(group.tolerance, 0.25);
}

#[test]
fn a_skipped_index_is_a_line_on_no_net() {
    let errors = errors_of(&board(
        "    .lane: LengthGroup {\n        .tolerance &= 0mm..0.5mm\n    }\n    .lane.lines[0] ~ .rs[0].a\n    .lane.lines[2] ~ .rs[2].a\n",
    ));
    assert!(
        errors.contains("`lane.lines[1]` is on no net, and the group has a line for each index up to `[2]`"),
        "{errors}"
    );
}

#[test]
fn two_lines_on_one_net_are_an_error() {
    let errors = errors_of(&board(
        "    .lane: LengthGroup {\n        .tolerance &= 0mm..0.5mm\n    }\n    .lane.lines[0] ~ .rs[0].a\n    .lane.lines[1] ~ .rs[0].a\n",
    ));
    assert!(
        errors.contains("`lane.lines[0]` and `lane.lines[1]` are both on the net `D0`"),
        "{errors}"
    );
}

#[test]
fn a_group_of_one_line_or_none_is_an_error() {
    let one = errors_of(&board(
        "    .lane: LengthGroup {\n        .tolerance &= 0mm..0.5mm\n    }\n    .lane.lines[0] ~ .rs[0].a\n",
    ));
    assert!(one.contains("`lane` has one line, on `D0`"), "{one}");
    let none = errors_of(&board(
        "    .lane: LengthGroup {\n        .tolerance &= 0mm..0.5mm\n    }\n",
    ));
    assert!(none.contains("`lane` has no lines"), "{none}");
}

#[test]
fn a_tolerance_unstated_with_no_top_or_of_nothing_is_an_error_that_says_what_to_write() {
    let with = |tolerance: &str| {
        errors_of(&board(&format!(
            "    .lane: LengthGroup {{\n{tolerance}    }}\n    .lane.lines[0] ~ .rs[0].a\n    .lane.lines[1] ~ .rs[1].a\n"
        )))
    };
    let unstated = with("");
    assert!(
        unstated.contains("`lane.tolerance` is not stated") && unstated.contains("`lane.tolerance &= 0mm..0.5mm`"),
        "{unstated}"
    );
    let open = with("        .tolerance &= 0.1mm..\n");
    assert!(open.contains("with no top"), "{open}");
    let zero = with("        .tolerance &= 0mm\n");
    assert!(
        zero.contains("`lane.tolerance` is 0") && zero.contains("give it a top above 0"),
        "{zero}"
    );
}
