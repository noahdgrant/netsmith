//! A part's maximum voltage in steps over temperature against the board's environment and the
//! voltage across it (issue #1257): the outcomes the guide's examples do not show.

use netsmith_check::{Loader, check};
use netsmith_design::{Designed, design_board};
use netsmith_solve::{Catalogs, solve};

/// A supercapacitor with the steps given, and no catalog.
fn part(steps: &str, bias: bool) -> String {
    let (role, bridge) = if bias {
        ("    #[role(bias)]\n    .bias: Voltage\n", "    bridge .a ~> .b\n")
    } else {
        ("", "")
    };
    format!(
        "part Supercap {{\n    .designator &= \"C\"\n    pins {{\n        .a passive \"+\" \"1\"\n        .b passive \"-\" \"2\"\n    }}\n{role}    .max_voltage: Voltage\n    .max_voltage &= {steps}\n{bridge}}}\n"
    )
}

fn build(part: &str, environment: Option<&str>, rail: Option<&str>) -> Result<Designed, Vec<String>> {
    let environment = environment.map_or(String::new(), |t| {
        format!("    .environment: Environment {{ .temperature &= {t} }}\n")
    });
    let rail = rail.map_or(String::new(), |v| {
        format!(
            "    .supply: Rail {{ .voltage &= {v}; .pos.net &= \"VBAK\" }}\n    .supply.pos ~> .hold ~> .supply.neg\n"
        )
    });
    let src = format!(
        "import std::env::Environment\nimport std::sources::Rail\n{part}board B {{\n{environment}    .hold: Supercap {{}}\n{rail}}}\n"
    );
    let c = check("t.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s)
}

fn steps_findings(d: &Designed) -> Vec<String> {
    d.guidelines
        .iter()
        .filter(|g| g.message.contains("steps over temperature"))
        .map(|g| g.message.clone())
        .collect()
}

const STEPS: &str = "5.5V to 70degC, 4.2V to 85degC";

#[test]
fn steps_that_hold_the_rail_at_every_temperature_the_board_reaches_say_nothing() {
    let d = build(&part(STEPS, true), Some("-20degC..85degC"), Some("3.6V +/- 5%")).unwrap();
    assert!(steps_findings(&d).is_empty(), "{:?}", steps_findings(&d));
    // A board that stays at or under 70 degC reaches the first step alone, which holds 5 V.
    let d = build(&part(STEPS, true), Some("-20degC..70degC"), Some("5V +/- 5%")).unwrap();
    assert!(steps_findings(&d).is_empty());
}

#[test]
fn a_board_always_above_a_step_breaks_it_from_its_coldest() {
    let e = build(&part(STEPS, true), Some("75degC..85degC"), Some("5V +/- 5%")).unwrap_err();
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(
        e[0].starts_with("`hold` is rated for 4.2 V only from 348.15 K (75degC), the coldest the board's environment reaches, up to 358.15 K (85degC)"),
        "{}",
        e[0]
    );
}

#[test]
fn an_environment_past_the_last_step_is_refused_with_the_last_temperature() {
    let e = build(&part(STEPS, true), Some("-20degC..105degC"), Some("3.3V +/- 5%")).unwrap_err();
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(
        e[0].starts_with("`hold` is rated for no voltage above 358.15 K (85degC), its last step (the design states `.hold.max_voltage &= 5.5V to 70degC, 4.2V to 85degC`), and the board's environment reaches 378.15 K (105degC)"),
        "{}",
        e[0]
    );
}

#[test]
fn steps_that_cannot_be_judged_are_named() {
    // No environment: no temperature to judge them at.
    let d = build(&part(STEPS, true), None, Some("5V +/- 5%")).unwrap();
    let found = steps_findings(&d);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("and the board states no `.environment.temperature`, so they are judged at no temperature"),
        "{}",
        found[0]
    );
    // Across no rail: the voltage across it is not known.
    let d = build(&part(STEPS, true), Some("-20degC..85degC"), None).unwrap();
    let found = steps_findings(&d);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].contains("and the voltage across it, `hold.bias`, is not known, so they are not judged: place `hold` across a rail, or state `.hold.bias`"),
        "{}",
        found[0]
    );
}

#[test]
fn a_part_with_no_voltage_across_it_cannot_state_steps() {
    let e = build(&part(STEPS, false), Some("-20degC..85degC"), None).unwrap_err();
    assert_eq!(e.len(), 1, "{e:#?}");
    assert!(
        e[0].contains("but nothing gives it the voltage across it to compare them with: a part rated so has a `#[role(bias)]` member and a `bridge`"),
        "{}",
        e[0]
    );
}
