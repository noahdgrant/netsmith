//! A board's `.environment` and a part's temperature range as the model holds them (ADR 0099).

use netsmith_check::*;

const HEAD: &str = "import std::env::Environment\nimport std::parts::passive::Resistor\n";

fn model(src: &str, board: &str) -> Model {
    let c = check("t.nsm", &format!("{HEAD}{src}"), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    c.board(board).unwrap().model.clone()
}

fn part(m: &Model, name: &str) -> InstId {
    match m.member(m.root, name) {
        Some(Member::Inst(id)) => id,
        other => panic!("`{name}` is not a member: {other:?}"),
    }
}

/// The ends of a field's value in kelvin (the display of a quantity rounds to four digits).
fn ends(m: &Model, field: FieldId) -> (f64, f64) {
    match &m.field(field).domain {
        Domain::Quantity(q) => (q.lo(), q.hi()),
        other => panic!("`{}` is {other}", m.path_of_field(field)),
    }
}

#[test]
fn a_board_with_an_environment_has_its_temperature_for_every_part() {
    let m = model(
        "board B {\n    .environment: Environment { .temperature &= 233.15K..358.15K }\n    .r: Resistor\n}\n",
        "B",
    );
    let field = m
        .environment_temperature_of(part(&m, "r"))
        .expect("the board's environment");
    assert_eq!(m.path_of_field(field), "environment.temperature");
    assert_eq!(ends(&m, field), (233.15, 358.15));
}

#[test]
fn a_board_with_no_environment_has_none() {
    let m = model("board B {\n    .r: Resistor\n}\n", "B");
    assert!(m.environment_of(part(&m, "r")).is_none());
    assert!(m.environment_temperature_of(part(&m, "r")).is_none());
}

#[test]
fn every_part_has_a_range_that_it_or_the_board_may_state() {
    let m = model(
        "board B {\n    .r: Resistor\n    .r.temp_min &= 233.15K\n    .r.temp_max &= 358.15K\n}\n",
        "B",
    );
    let (min, max) = m
        .temperature_range_of(part(&m, "r"))
        .expect("a part has a range");
    assert_eq!(m.path_of_field(min), "r.temp_min");
    assert_eq!(ends(&m, min), (233.15, 233.15));
    assert_eq!(ends(&m, max), (358.15, 358.15));
    // A board has no range of its own: it is not a part.
    assert!(m.temperature_range_of(m.root).is_none());
}

#[test]
fn a_temperature_in_degc_is_kelvin_underneath_and_a_sign_on_it_is_the_numbers() {
    let m = model(
        "board B {\n    .environment: Environment { .temperature &= -40degC..125degC }\n    .r: Resistor\n    .r.temp_min &= -55degC\n    .r.temp_max &= 0degC + 150K\n}\n",
        "B",
    );
    let field = m.environment_temperature_of(part(&m, "r")).unwrap();
    assert_eq!(ends(&m, field), (233.15, 398.15));
    let (min, max) = m.temperature_range_of(part(&m, "r")).unwrap();
    assert_eq!(ends(&m, min), (218.15, 218.15));
    // A temperature plus a rise is a temperature; a difference of two is kelvin.
    assert_eq!(ends(&m, max), (423.15, 423.15));
}

#[test]
fn a_tolerance_is_a_difference_so_it_is_in_kelvin() {
    let ok = check(
        "t.nsm",
        &format!("{HEAD}board B {{\n    .r: Resistor\n    .r.temp_max &= 85degC +/- 5K\n}}\n"),
        &Loader::std(),
    );
    assert!(ok.is_ok(), "{}", ok.render());
    let c = check(
        "t.nsm",
        &format!("{HEAD}board B {{\n    .r: Resistor\n    .r.temp_max &= 85degC +/- 5degC\n}}\n"),
        &Loader::std(),
    );
    assert!(!c.is_ok());
    assert!(
        c.render()
            .contains("a tolerance is a difference of temperatures: write it in kelvin"),
        "{}",
        c.render()
    );
}

#[test]
fn a_range_is_a_temperature_and_nothing_else() {
    let c = check(
        "t.nsm",
        &format!("{HEAD}board B {{\n    .r: Resistor\n    .r.temp_max &= 85C\n}}\n"),
        &Loader::std(),
    );
    assert!(!c.is_ok());
    let said = c.render();
    assert!(said.contains("temp_max"), "{said}");
}

#[test]
fn each_named_environment_has_its_cited_range() {
    // The ranges are the AEC-Q100 grades (Table 1) and the two conventions, each stated in the
    // doc comment of its definition in `std/env.nsm`.
    let ranges = [
        ("Automotive", 233.15, 398.15),
        ("AutomotiveGrade0", 233.15, 423.15),
        ("AutomotiveGrade2", 233.15, 378.15),
        ("AutomotiveGrade3", 233.15, 358.15),
        ("Industrial", 233.15, 358.15),
        ("Commercial", 273.15, 343.15),
    ];
    for (name, lo, hi) in ranges {
        let c = check(
            "t.nsm",
            &format!(
                "import std::env::{name}\nimport std::parts::passive::Resistor\nboard B {{\n    .environment: {name} {{}}\n    .r: Resistor\n}}\n"
            ),
            &Loader::std(),
        );
        assert!(c.is_ok(), "{name}: {}", c.render());
        let m = c.board("B").unwrap().model.clone();
        let field = m.environment_temperature_of(part(&m, "r")).unwrap();
        assert_eq!(ends(&m, field), (lo, hi), "{name}");
    }
}

#[test]
fn a_board_narrows_a_named_environment_and_its_statement_replaces_the_offer() {
    let c = check(
        "t.nsm",
        "import std::env::Automotive\nimport std::parts::passive::Resistor\nboard B {\n    .environment: Automotive {}\n    .environment.temperature &= -20degC..90degC\n    .r: Resistor\n}\n",
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = c.board("B").unwrap().model.clone();
    let field = m.environment_temperature_of(part(&m, "r")).unwrap();
    assert_eq!(ends(&m, field), (253.15, 363.15));
}
