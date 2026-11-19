//! What the checker refuses in a `direction` statement (issue #1306): a control pin that is not
//! an input, a side whose pin is not bidirectional, a pin on both sides, a pin whose direction
//! two statements fix, and the statement outside a part.
//!
//! `Shift` is invented for these tests; it is not a real part.

use netsmith_check::*;

const SHIFT: &str = r#"
import std::logic::Logic
import std::sources::Rail

part Shift {
    .vcc: Rail
    .a: Logic
    .a.reference ~ .vcc
    .b: Logic
    .b.reference ~ .vcc
    DIRECTIONS
    pins {
        .vcc.pos power         "VCC" "1"
        .vcc.neg power         "GND" "2"
        .a.line  bidirectional "A"   "3"
        .b.line  bidirectional "B"   "4"
        .dir     input         "DIR" "5"
        .oe      input         "OE"  "6"
        .y       output        "Y"   "7"
    }
}

board B {
    .s: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "3V3"; .neg.net &= "GND" }
    .x: Shift
    .s ~ .x.vcc
    .x.dir ~ .s.pos
    .x.oe ~ .s.neg
    .x.a.line ~ .x.b.line
}
"#;

fn messages(directions: &str) -> Vec<String> {
    let src = SHIFT.replace("DIRECTIONS", directions);
    let c = check("test.nsm", &src, &Loader::std());
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_direction_for_each_level_of_one_control_pin_is_clean() {
    let m = messages("direction .a to .b when .dir high\n    direction .b to .a when .dir low");
    assert_eq!(m, Vec::<String>::new());
}

#[test]
fn the_direction_is_in_the_model_with_the_pins_of_each_side() {
    let src = SHIFT.replace("DIRECTIONS", "direction .a to .b when .dir high");
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.boards[0].model;
    let pin = |path: &str| {
        model
            .instances()
            .map(|(i, _)| i)
            .find(|&i| model.path_of(i) == path)
            .unwrap_or_else(|| panic!("{path}"))
    };
    let dir = pin("x.dir");
    assert_eq!(
        model.direction_of(pin("x.a.line")),
        Some((dir, Some(PinKind::Input), None))
    );
    assert_eq!(
        model.direction_of(pin("x.b.line")),
        Some((dir, Some(PinKind::Output), None))
    );
    // The reference rail of a `Logic` carries no signal.
    assert_eq!(model.direction_of(pin("x.vcc.pos")), None);
}

#[test]
fn a_control_pin_that_is_not_an_input_is_refused() {
    let m = messages("direction .a to .b when .y high");
    assert_eq!(
        m,
        ["`x.y` fixes the direction of a `direction`, so it is an input, but it is an output"]
    );
}

#[test]
fn a_side_whose_pin_is_not_bidirectional_is_refused() {
    let m = messages("direction .a to .oe when .dir high");
    assert_eq!(
        m,
        ["`x.oe` is an input, which has one direction already; a `direction` names bidirectional pins"]
    );
}

#[test]
fn a_pin_on_both_sides_is_refused() {
    let m = messages("direction .a to .a.line when .dir high");
    assert_eq!(
        m,
        ["`.a.line` is on both sides of the `direction`: a signal goes from one pin to another"]
    );
}

#[test]
fn the_same_level_stated_twice_is_refused() {
    let m = messages("direction .a to .b when .dir high\n    direction .b to .a when .dir high");
    assert_eq!(
        m,
        ["the direction of `.b.line` when `.dir` is high is already stated"]
    );
}

#[test]
fn a_pin_whose_direction_two_control_pins_fix_is_refused() {
    let m = messages("direction .a to .b when .dir high\n    direction .b to .a when .oe low");
    assert_eq!(
        m,
        ["`.b.line` already has its direction fixed by `.dir`; one control pin fixes the direction of a pin"]
    );
}

#[test]
fn a_control_pin_on_a_side_is_refused() {
    let m = messages("direction .a to .dir when .dir high");
    assert_eq!(
        m,
        ["`.dir` fixes the direction, so it is not a pin the signal passes through"]
    );
}

#[test]
fn a_direction_outside_a_part_is_refused() {
    let src = SHIFT
        .replace("DIRECTIONS", "")
        .replace("    .x.a.line ~ .x.b.line\n", "    .x.a.line ~ .x.b.line\n    direction .x.a to .x.b when .x.dir high\n");
    let c = check("test.nsm", &src, &Loader::std());
    let m: Vec<String> = c.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert_eq!(m, ["`direction` can only appear directly inside a part"]);
}
