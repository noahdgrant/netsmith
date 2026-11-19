//! A scenario of a board's supplies, `scenario backup { remove .usb; source .cell; sense .det }`
//! (issue #1258): the syntax, the printer and the formatter.

use netsmith_lang::ast::*;
use netsmith_lang::{format, parse};

const BOARD: &str = "board B {\n    scenario backup {\n        remove .usb\n        source .cell\n        sense .mcu.det\n    }\n}\n";

#[test]
fn a_scenario_is_a_name_and_statements_that_name_paths_and_prints_back_to_its_source() {
    let file = parse(BOARD).unwrap();
    let Item::Scenario(q) = &file.definitions[0].items[0] else {
        panic!("{:?}", file.definitions[0].items[0])
    };
    assert_eq!(q.name.name, "backup");
    let words: Vec<String> = q
        .steps
        .iter()
        .map(|s| format!("{} {}", s.word.word(), s.path.to_source()))
        .collect();
    assert_eq!(words, ["remove .usb", "source .cell", "sense .mcu.det"]);
    assert_eq!(file.to_source(), BOARD);
    assert_eq!(format(BOARD).unwrap(), BOARD);
}

#[test]
fn the_statements_of_a_scenario_may_be_parted_by_semicolons_and_are_laid_out_one_to_a_line() {
    let one_line = "board B {\n    scenario backup { remove .usb; source .cell; sense .mcu.det }\n}\n";
    assert_eq!(parse(one_line).unwrap().to_source(), BOARD);
    assert_eq!(format(one_line).unwrap(), BOARD);
}

#[test]
fn a_member_called_scenario_is_still_a_member() {
    let src = "board B {\n    .scenario: Rail\n}\n";
    assert_eq!(parse(src).unwrap().to_source(), src);
}

fn refused(body: &str) -> String {
    parse(&format!("board B {{\n    scenario s {{ {body} }}\n}}\n"))
        .unwrap_err()
        .message
}

#[test]
fn a_scenario_out_of_shape_is_refused_with_its_shape() {
    for (body, says) in [
        ("source .cell", "a supply it takes away (`remove`)"),
        ("remove .usb", "a source that stays (`source`)"),
        ("remove; source .cell", "`remove` is followed by the path"),
    ] {
        let message = refused(body);
        assert!(
            message.contains(says) && message.contains("scenario backup {"),
            "`{body}`: {message}"
        );
    }
    let message = refused("remove .usb; keep .cell");
    assert!(message.contains("`remove`, `source` or `sense`"), "{message}");
}
