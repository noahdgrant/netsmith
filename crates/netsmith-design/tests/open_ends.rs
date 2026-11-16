//! A range with an open end where the design stage reads a number: what it takes from the
//! end that is there, and what it refuses for the end that is not. Every number is invented.

use netsmith_check::{Loader, check};
use netsmith_core::Design;
use netsmith_design::design_board;
use netsmith_solve::{Catalogs, solve};

fn designed(src: &str) -> Result<Design, Vec<String>> {
    // Physical intent in the board is split into its own file, as the language takes it.
    let (circuit, physical) = netsmith_testkit::split_physical(src);
    let mut loader = Loader::std();
    if let Some(physical) = physical {
        loader.attach_physical("t.nsm", "t.physical.nsm", &physical);
    }
    let c = check("t.nsm", &circuit, &loader);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s).map(|l| l.design)
}

fn board_with(process: &str, members: &str) -> String {
    format!(
        "import std::env::Environment\nimport std::parts::passive::Resistor\nimport std::mfg::jlcpcb::TwoLayer\nimport std::sources::Rail\nboard B {{\n    {process}\n    .r: Resistor\n    {members}\n}}\n"
    )
}

#[test]
fn a_rule_stated_with_no_upper_limit_is_read_at_its_bottom() {
    let d = designed(&board_with(
        ".process: TwoLayer {\n        .min_track &= 0.15mm..\n        .min_edge_clearance &= 0.5mm..\n    }",
        "",
    ))
    .unwrap();
    let p = d.process.unwrap();
    assert_eq!((p.min_track, p.min_edge_clearance), (0.15, 0.5));
    assert_eq!(p.min_clearance, 0.1, "what was not narrowed is the library's");
}

#[test]
fn a_temperature_rise_with_no_upper_limit_is_refused_and_one_with_no_lower_limit_is_read() {
    // `max_temperature_rise` is an upper bound the library leaves unstated; a board that gives
    // it no upper limit gives the design no number to design to.
    let errors = designed(&board_with(
        ".process: TwoLayer {\n        .max_temperature_rise &= 20K..\n    }",
        "",
    ))
    .unwrap_err();
    let text = errors.join("\n");
    assert!(
        text.contains("`process.max_temperature_rise` is 20K.., which has no upper limit"),
        "{text}"
    );
    let d = designed(&board_with(
        ".process: TwoLayer {\n        .max_temperature_rise &= ..20K\n    }",
        "",
    ))
    .unwrap();
    assert_eq!(d.process.unwrap().max_temperature_rise, Some(20.0));
}

#[test]
fn an_environment_with_an_open_end_judges_nothing_and_says_so() {
    let errors = designed(&board_with(
        ".process: TwoLayer",
        ".environment: Environment { .temperature &= -40degC.. }",
    ))
    .unwrap_err();
    let text = errors.join("\n");
    assert!(
        text.contains("`environment.temperature` has an open end")
            && text.contains("state both the coldest and the hottest"),
        "{text}"
    );
}

#[test]
fn an_outline_size_with_an_open_end_has_no_limit_there_and_no_negative_size() {
    let src = "import std::mfg::jlcpcb::TwoLayer\nimport std::parts::passive::Resistor\nimport std::pcb::Outline\nboard B {\n    .process: TwoLayer\n    .r: Resistor\n    .outline: Outline {\n        .width &= 50mm..\n        .height &= ..40mm\n    }\n}\n";
    let o = designed(src).unwrap().features.outline.unwrap();
    assert_eq!(o.width.0, 50.0);
    assert!(o.width.1.is_infinite());
    assert_eq!(o.height, (0.0, 40.0));
}
