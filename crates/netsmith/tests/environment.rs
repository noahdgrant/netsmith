//! A board's environment against the temperatures of its parts (ADR 0099, PROVISIONAL): the
//! board states a temperature range, a part states the range it works at (its own `.temp_min` and
//! `.temp_max`, or its catalog row's cells), the solver leaves out a row whose range does not
//! cover the environment, the design stage refuses a stated range that does not, and a part
//! that states none is kept and named in a guideline finding. A board with no environment is
//! not judged and says nothing. Every board and row here is invented, except the crystal rows of
//! the standard catalog (the JLCPCB listing's -40 C to 85 C).

use netsmith::{outcome_with, render_problems};
use netsmith_check::Loader;
use netsmith_solve::Catalogs;

/// -40 C, 85 C and 125 C, as `degC` literals (ADR 0103). The tests that state a figure in kelvin
/// (the project's `VEHICLE` below, the cells of a catalog) show that the two spellings are one.
const COLD: &str = "-40degC";
const WARM: &str = "85degC";
const HOT: &str = "125degC";

/// A board with a supply, one resistor and a crystal, and `extra` inside it.
fn board(extra: &str) -> String {
    format!(
        "\
import std::env::Environment
import std::parts::passive::{{Crystal, Resistor}}
import std::sources::Rail

board Widget {{
    .supply: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }}
    .r1: Resistor {{ .resistance &= 71.5kohm +/- 1% }}
    .xtal: Crystal {{ .frequency &= 16MHz +/- 30ppm }}
    .supply.pos ~> .r1 ~> .supply.neg
    .xtal.a ~ .supply.pos
    .xtal.b ~ .supply.neg
{extra}}}
"
    )
}

/// The row these boards' resistor is picked as: a project's own, of a value no standard row
/// has, and it states no range. The standard resistor rows now state theirs (the datasheets'),
/// and every test here needs a part whose row states none, so that it is offered and named.
const UNJUDGED_ROW: &str = "\
mpn,resistance,max_power,package,library,source
ACME-N,71.5kohm +/- 1%,62.5mW,0402,Basic,https://example.invalid/acme.pdf no range
";

/// The standard catalogs and the project row of [`UNJUDGED_ROW`].
fn jlcpcb() -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    c.add_file("resistor", "catalogs/resistor.csv", UNJUDGED_ROW)
        .unwrap();
    c
}

/// A board that states the environment `low..high`.
fn with_environment(low: &str, high: &str) -> String {
    board(&format!(
        "    .environment: Environment {{ .temperature &= {low}..{high} }}\n"
    ))
}

fn build(src: &str, catalogs: &Catalogs) -> Result<netsmith::Built, String> {
    // No libraries: the board is checked, solved and designed, and no schematic is drawn (the
    // sample libraries have no footprint for a crystal).
    outcome_with("widget.nsm", src, &Loader::std(), catalogs, None)
        .map(|o| o.boards.into_iter().next().expect("one board"))
        .map_err(|problems| render_problems(&problems))
}

/// What a build that must fail prints.
fn refused(src: &str, catalogs: &Catalogs) -> String {
    match build(src, catalogs) {
        Ok(_) => panic!("the build was expected to fail"),
        Err(e) => e,
    }
}

fn environment_findings(b: &netsmith::Built) -> Vec<String> {
    b.guideline_warnings()
        .into_iter()
        .filter(|w| w.contains("environment guideline"))
        .collect()
}

#[test]
fn a_part_whose_range_covers_the_environment_is_picked_and_not_named() {
    // The standard crystal rows state -40 C to 85 C (233.15 K to 358.15 K).
    let b = build(&with_environment(COLD, WARM), &jlcpcb()).unwrap();
    assert!(b.sourcing.iter().any(|s| s.kind == "Crystal"));
    let findings = environment_findings(&b);
    // The crystal is judged and met it; the resistor's row states nothing and is named.
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(!findings[0].contains("xtal"), "{}", findings[0]);
}

#[test]
fn a_part_whose_range_does_not_cover_the_environment_is_refused_with_the_numbers() {
    let e = refused(&with_environment(COLD, HOT), &jlcpcb());
    assert!(
        e.contains(
            "the `crystal` catalog has nothing for `xtal`, which needs frequency within 15.9995MHz..16.0005MHz"
        ),
        "{e}"
    );
    assert!(
        e.contains(
            "4 rows were left out because the temperatures they state do not cover the board's environment, `.environment.temperature` 233.15 K to 398.15 K (-40degC to 125degC)"
        ),
        "{e}"
    );
}

#[test]
fn a_row_that_states_no_range_is_offered_and_named_with_what_to_add() {
    let b = build(&with_environment(COLD, WARM), &jlcpcb()).unwrap();
    let findings = environment_findings(&b);
    let message = &findings[0];
    assert!(
        message.starts_with("Widget: environment guideline: `r1` (Resistor) was picked as "),
        "{message}"
    );
    assert!(
        message.contains("and its catalog row states no `temp_min` and no `temp_max`"),
        "{message}"
    );
    assert!(
        message.contains("not judged against the board's environment (`environment.temperature` 233.15 K to 358.15 K (-40degC to 85degC))"),
        "{message}"
    );
    assert!(
        message.contains(
            "or state it on the part, as `.r1.temp_max &= 85degC` for a datasheet that says 85degC"
        ),
        "{message}"
    );
    // The part is still picked: the row is offered, not dropped.
    assert!(
        b.sourcing
            .iter()
            .any(|s| s.kind == "Resistor" && s.mpn.as_deref() == Some("ACME-N"))
    );
}

#[test]
fn a_guideline_finding_stops_the_build_when_the_project_asks_for_it() {
    let strict = jlcpcb().with_strict_guidelines(true);
    let e = refused(&with_environment(COLD, WARM), &strict);
    assert!(e.contains("environment guideline"), "{e}");
}

#[test]
fn a_board_with_no_environment_is_not_judged_and_says_nothing() {
    // Nothing is checked, as before: the crystal is picked at no temperature at all.
    let b = build(&board(""), &jlcpcb()).unwrap();
    assert!(environment_findings(&b).is_empty());
    assert!(b.guidelines.iter().all(|g| g.rule != "environment"));
}

#[test]
fn a_range_the_part_states_speaks_for_it_whatever_its_row_says() {
    // The resistor states both ends: it is judged and not named.
    let extra = format!(
        "    .environment: Environment {{ .temperature &= {COLD}..{WARM} }}\n    .r1.temp_min &= {COLD}\n    .r1.temp_max &= 423.15K\n"
    );
    let b = build(&board(&extra), &jlcpcb()).unwrap();
    assert!(
        environment_findings(&b).is_empty(),
        "{:?}",
        environment_findings(&b)
    );
}

#[test]
fn a_stated_range_that_does_not_cover_the_environment_is_an_error_naming_the_part() {
    let extra = format!(
        "    .environment: Environment {{ .temperature &= {COLD}..{WARM} }}\n    .r1.temp_max &= 348.15K\n"
    );
    let e = refused(&board(&extra), &jlcpcb());
    assert!(
        e.contains("`r1` works up to 348.15 K (75degC) only (the design states `.r1.temp_max`), under the board's environment, which reaches 358.15 K (85degC) (`environment.temperature` 233.15 K to 358.15 K (-40degC to 85degC))"),
        "{e}"
    );
}

/// A project's resistor rows, of a value only they have: `ACME-R1`'s row states -20 C to 70 C,
/// `ACME-R2`'s only 125 C.
fn project_rows() -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    c.add_file(
        "resistor",
        "catalogs/resistor.csv",
        "\
mpn,resistance,max_power,package,temp_min,temp_max,library,source
ACME-R1,31.6kohm +/- 1%,62.5mW,0402,253.15K,343.15K,Basic,https://example.invalid/acme.pdf temp: page 1
ACME-R2,31.6kohm +/- 1%,62.5mW,0402,,398.15K,Basic,https://example.invalid/acme.pdf temp: page 1
",
    )
    .unwrap();
    c
}

#[test]
fn a_row_that_does_not_cover_the_environment_is_passed_over_for_the_next() {
    // -40 C to 85 C: ACME-R1 stops at 70 C and is left out; ACME-R2 reaches 125 C but states
    // no lowest temperature, so it is kept and the cold end is named.
    let src = with_environment(COLD, WARM).replace("71.5kohm", "31.6kohm");
    let b = build(&src, &project_rows()).unwrap();
    let r1 = b.sourcing.iter().find(|s| s.kind == "Resistor").unwrap();
    assert_eq!(r1.mpn.as_deref(), Some("ACME-R2"));
    let findings = environment_findings(&b);
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(
        findings[0].contains("`r1` (Resistor) was picked as "),
        "{}",
        findings[0]
    );
    assert!(
        findings[0].contains("states no `temp_min`, so it is not judged"),
        "{}",
        findings[0]
    );
    assert!(
        findings[0].contains("`.r1.temp_min &= -40degC` for a datasheet that says -40degC"),
        "{}",
        findings[0]
    );
}

#[test]
fn an_environment_that_states_no_temperature_is_named() {
    let src = board("    .environment: Environment { }\n");
    let b = build(&src, &jlcpcb()).unwrap();
    let findings = environment_findings(&b);
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(
        findings[0].contains("the board has an environment and `environment.temperature` states no temperature, so no part is judged against it"),
        "{}",
        findings[0]
    );
}

#[test]
fn a_cell_that_is_not_a_temperature_is_refused_with_its_line() {
    let rows = |min: &str, max: &str| {
        let mut c = Catalogs::jlcpcb();
        c.add_file(
            "resistor",
            "catalogs/resistor.csv",
            &format!(
                "mpn,resistance,max_power,package,temp_min,temp_max,library,source\nACME-R1,31.6kohm +/- 1%,62.5mW,0402,{min},{max},Basic,https://example.invalid/acme.pdf\n"
            ),
        )
        .map(|_| c)
    };
    let e = rows("", "85").unwrap_err();
    assert!(
        e.contains("catalogs/resistor.csv:2")
            && e.contains("`temp_max`")
            && e.contains("a temperature such as `85degC` or `358.15K`"),
        "{e}"
    );
    let e = rows("", "350K..360K").unwrap_err();
    assert!(
        e.contains("is a range, and a part's highest temperature is one temperature"),
        "{e}"
    );
    let e = rows("358.15K", "233.15K").unwrap_err();
    assert!(
        e.contains("the lowest temperature of a part is above its highest"),
        "{e}"
    );
    let e = rows("-300K", "").unwrap_err();
    assert!(e.contains("below absolute zero"), "{e}");
    let e = rows("-300degC", "").unwrap_err();
    assert!(e.contains("below absolute zero"), "{e}");
}

#[test]
fn a_cell_may_be_written_in_degc_or_in_kelvin_and_is_the_same_temperature() {
    // `-40degC` and `233.15K` (and `85degC` and `358.15K`) are one temperature: a row in either
    // spelling is judged the same, and the sign of `-40degC` is the number's.
    let row = |min: &str, max: &str| {
        let mut c = Catalogs::jlcpcb();
        c.add_file(
            "resistor",
            "catalogs/resistor.csv",
            &format!(
                "mpn,resistance,max_power,package,temp_min,temp_max,library,source\nACME-D,71.5kohm +/- 1%,62.5mW,0402,{min},{max},Basic,https://example.invalid/acme.pdf\n"
            ),
        )
        .unwrap();
        c
    };
    for (min, max) in [("-40degC", "85degC"), ("233.15K", "358.15K")] {
        let b = build(&with_environment(COLD, WARM), &row(min, max)).unwrap();
        // The resistor is judged and met the environment: only nothing is named for it.
        let findings = environment_findings(&b);
        assert!(findings.is_empty(), "{min}..{max}: {findings:#?}");
    }
    // 80degC is under the board's 85degC: the row is left out, so the resistor has nothing.
    let e = refused(&with_environment(COLD, WARM), &row("-40degC", "80degC"));
    assert!(
        e.contains("have nothing") || e.contains("has nothing"),
        "{e}"
    );
}

/// A Schottky diode across a 12 V rail, in an environment `low..high`. The standard SS34 row is
/// the one rated 3 A: its datasheet gives a junction range, -55 to +125 degC (`TJ`).
fn diode_board(low: &str, high: &str) -> String {
    format!(
        "\
import std::env::Environment
import std::parts::diode::Schottky
import std::sources::Rail

board Clamp {{
    .environment: Environment {{ .temperature &= {low}..{high} }}
    .supply: Rail {{
        .voltage &= 12V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }}
    .d: Schottky {{
        .max_reverse_voltage &= 40V
        .max_forward_current &= 3A
    }}
    .supply.neg ~> .d ~> .supply.pos
}}
"
    )
}

#[test]
fn a_junction_range_is_the_parts_range_and_a_range_that_covers_the_environment_passes() {
    // -55 to +125 degC covers -40 to 125 degC: the diode is judged by it and is not named, and
    // the self-heating of the diode is not computed.
    let b = build(&diode_board("-40degC", "125degC"), &jlcpcb()).unwrap();
    assert!(
        b.sourcing.iter().any(|s| s.mpn.as_deref() == Some("SS34")),
        "{:?}",
        b.sourcing
    );
    let findings = environment_findings(&b);
    assert!(
        findings.iter().all(|f| !f.contains("`d`")),
        "the diode states its junction range and must not be named: {findings:#?}"
    );
    // The same figure refuses a hotter environment, as any stated range does.
    let e = refused(&diode_board("-40degC", "130degC"), &jlcpcb());
    assert!(
        e.contains("the `schottky` catalog has nothing for `d`")
            && e.contains("left out because the temperatures they state do not cover"),
        "{e}"
    );
}

// ---- the manifest's default (ADR 0099) -------------------------------------------------

use netsmith::project::{Manifest, Target};
use netsmith_kicad::testlib::TempDir;
use std::fs;

/// A project's own environment: -40 C to 85 C, offered, so that a board can replace it.
const VEHICLE: &str = "\
interface Automotive {
    /// The cabin, as this project means it.
    .temperature: Temperature
    .temperature ?= 233.15K..358.15K
}
";

/// A project whose manifest has `line` and whose `src/main.nsm` is `main`.
fn project(name: &str, line: &str, main: &str) -> TempDir {
    let root = TempDir::new(&format!("environment-test-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"widget\"\n{line}\n"),
    )
    .unwrap();
    fs::write(root.join("src/vehicle.nsm"), VEHICLE).unwrap();
    fs::write(root.join("src/main.nsm"), main).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(root.join("catalogs/resistor.csv"), UNJUDGED_ROW).unwrap();
    root
}

fn built(root: &TempDir, main: &str) -> Result<netsmith::Built, String> {
    let target = Target::resolve_from(root, None)?;
    let loader = target.loader()?;
    let catalogs = target.catalogs()?;
    outcome_with("src/main.nsm", main, &loader, &catalogs, None)
        .map(|o| o.boards.into_iter().next().expect("one board"))
        .map_err(|problems| render_problems(&problems))
}

#[test]
fn the_manifest_gives_every_board_without_an_environment_the_one_it_names() {
    let main = board("");
    let root = project(
        "default",
        "environment = \"src/vehicle.nsm::Automotive\"",
        &main,
    );
    let b = built(&root, &main).unwrap();
    // The board declares none and is judged against -40 C to 85 C: the crystal rows cover it,
    // and the resistor's row states nothing, so it is named, with the project's range.
    let findings = environment_findings(&b);
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(
        findings[0].contains("`r1` (Resistor)")
            && findings[0]
                .contains("`environment.temperature` 233.15 K to 358.15 K (-40degC to 85degC)"),
        "{}",
        findings[0]
    );
}

#[test]
fn a_project_without_the_key_has_no_environment() {
    let main = board("");
    let root = project("none", "", &main);
    let b = built(&root, &main).unwrap();
    assert!(b.guidelines.iter().all(|g| g.rule != "environment"));
}

#[test]
fn a_board_that_declares_its_own_environment_has_that_one_alone() {
    // The project's 85 C would pass the crystals; the board's 125 C does not, and it is not
    // the intersection of the two.
    let main = with_environment(COLD, HOT);
    let root = project(
        "own",
        "environment = \"src/vehicle.nsm::Automotive\"",
        &main,
    );
    let e = built(&root, &main).map(|_| ()).unwrap_err();
    assert!(
        e.contains("the `crystal` catalog has nothing for `xtal`")
            && e.contains("`.environment.temperature` 233.15 K to 398.15 K (-40degC to 125degC)"),
        "{e}"
    );
}

#[test]
fn a_board_that_only_states_the_temperature_has_the_manifests_environment_as_declared() {
    // No declaration, one statement: the project's `Automotive` is the member, and the board's
    // `&=` replaces its offer whole (it is not intersected with it).
    let main = board(&format!("    .environment.temperature &= {COLD}..{HOT}\n"));
    let root = project(
        "states",
        "environment = \"src/vehicle.nsm::Automotive\"",
        &main,
    );
    let e = built(&root, &main).map(|_| ()).unwrap_err();
    assert!(
        e.contains("233.15 K to 398.15 K (-40degC to 125degC)"),
        "{e}"
    );
    // The same board with a range inside the offer builds.
    let main = board(&format!("    .environment.temperature &= {COLD}..{WARM}\n"));
    built(
        &project(
            "states-ok",
            "environment = \"src/vehicle.nsm::Automotive\"",
            &main,
        ),
        &main,
    )
    .unwrap();
}

#[test]
fn a_standard_named_environment_is_the_default_and_the_crystals_judge_it() {
    // `Industrial` is -40 to 85 degC: the standard crystal rows (-40 to 85 degC) cover it.
    let main = board("");
    let root = project("named-ok", "environment = \"std::env::Industrial\"", &main);
    let b = built(&root, &main).unwrap();
    assert!(b.sourcing.iter().any(|s| s.kind == "Crystal"));
    // `Automotive` is AEC-Q100 Grade 1, -40 to 125 degC: none of them does.
    let root = project("named-hot", "environment = \"std::env::Automotive\"", &main);
    let e = built(&root, &main).map(|_| ()).unwrap_err();
    assert!(
        e.contains("the `crystal` catalog has nothing for `xtal`")
            && e.contains("`.environment.temperature` 233.15 K to 398.15 K (-40degC to 125degC)"),
        "{e}"
    );
    // A board that declares one and narrows its offer is judged against the narrowed range.
    let main = board(&format!(
        "    .environment: Automotive {{}}\n    .environment.temperature &= {COLD}..{WARM}\n"
    ))
    .replace(
        "import std::env::Environment",
        "import std::env::Automotive",
    );
    let root = project("named-narrowed", "", &main);
    built(&root, &main).unwrap();
}

#[test]
fn the_standard_environment_can_be_the_default_and_a_board_then_states_its_temperature() {
    let main = board("");
    let root = project("std", "environment = \"std::env::Environment\"", &main);
    let b = built(&root, &main).unwrap();
    let findings = environment_findings(&b);
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(
        findings[0].contains("states no temperature, so no part is judged against it"),
        "{}",
        findings[0]
    );
}

#[test]
fn the_manifest_key_is_a_standard_module_or_a_file_of_the_project_and_a_definition() {
    let with = |spelled: &str| {
        Manifest::parse(&format!(
            "[project]\nname = \"a\"\nenvironment = \"{spelled}\"\n"
        ))
    };
    assert_eq!(
        with("std::env::Environment")
            .unwrap()
            .environment
            .as_deref(),
        Some("std::env::Environment")
    );
    assert!(with("src/vehicle.nsm::Automotive").is_ok());
    for (spelled, why) in [
        ("Environment", "is not a module and a definition"),
        (
            "std::env::environment-1",
            "does not end in the name of a definition",
        ),
        (
            "src::vehicle::Automotive",
            "names neither a standard module nor a `.nsm` file",
        ),
        (
            "vehicle.nsm::Automotive",
            "is not a module of the project (a file of `src/`)",
        ),
        (
            "../vehicle.nsm::Automotive",
            "must be a path inside the project",
        ),
    ] {
        let e = with(spelled).unwrap_err();
        assert!(e.contains(why), "{spelled}: {e}");
    }
    let m = Manifest {
        environment: Some("std::env::Environment".into()),
        ..Manifest::new("a")
    };
    assert_eq!(Manifest::parse(&m.render()), Ok(m));
    assert!(!Manifest::new("a").render().contains("environment"));
}

#[test]
fn a_manifest_key_that_names_nothing_is_refused_naming_the_key() {
    let main = board("");
    let root = project(
        "missing",
        "environment = \"src/nowhere.nsm::Automotive\"",
        &main,
    );
    let e = built(&root, &main).map(|_| ()).unwrap_err();
    assert!(
        e.contains("environment = \"src/nowhere.nsm::Automotive\" names the module `src::nowhere`"),
        "{e}"
    );
    let root = project(
        "undefined",
        "environment = \"src/vehicle.nsm::Nothing\"",
        &main,
    );
    let e = built(&root, &main).map(|_| ()).unwrap_err();
    assert!(e.contains("the manifest's `environment` names `Nothing`, which `src::vehicle` does not define as an interface"), "{e}");
}

#[test]
fn the_editor_lists_the_finding_as_a_warning_at_the_part_and_an_error_if_the_project_says() {
    use netsmith::editor::Workspace;
    use netsmith_editor::Backend;
    use netsmith_editor::api::{ProblemStage, Severity};

    let main = "\
import std::env::Environment
import std::parts::passive::Resistor
import std::sources::Rail

board Widget {
    .environment: Environment { .temperature &= 233.15K..358.15K }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .r1: Resistor { .resistance &= 71.5kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
";
    let root = project("editor", "", main);
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let found: Vec<_> = analysis
        .problems
        .iter()
        .filter(|p| p.message.contains("environment guideline"))
        .collect();
    assert_eq!(found.len(), 1, "{:?}", analysis.problems);
    assert_eq!(found[0].severity, Severity::Warning);
    assert_eq!(found[0].stage, Some(ProblemStage::Guideline));
    assert_eq!(found[0].path.as_deref(), Some("r1"));
    assert!(
        found[0]
            .message
            .contains("its catalog row states no `temp_min` and no `temp_max`"),
        "{}",
        found[0].message
    );

    // With `guidelines = "error"` the build fails and it is an error instead.
    let root = project("editor-error", "guidelines = \"error\"", main);
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    assert!(
        analysis
            .problems
            .iter()
            .any(|p| p.severity == Severity::Error && p.message.contains("environment guideline")),
        "{:?}",
        analysis.problems
    );
}

#[test]
fn a_part_that_is_not_fitted_is_not_judged() {
    // `r2` is left off the board (`AssemblyMode::None`): it is not fitted, so there is no range
    // to name. `r1`, the same row, is the one finding.
    let src = board(&format!(
        "    .environment: Environment {{ .temperature &= {COLD}..{WARM} }}\n    .r2: Resistor {{ .resistance &= 71.5kohm +/- 1% }}\n    .r2.assembly &= AssemblyMode::None\n    .supply.pos ~> .r2 ~> .supply.neg\n"
    ));
    let b = build(&src, &jlcpcb()).unwrap();
    let findings = environment_findings(&b);
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(
        findings[0].contains("`r1` (Resistor) was picked"),
        "{}",
        findings[0]
    );
    assert!(!findings[0].contains("r2"), "{}", findings[0]);
}

// ---- a row with only a dielectric class (ADR 0099, ENV2-C) ----------------------------------

const CAPACITOR_HEADER: &str = "mpn,capacitance,effective_capacitance,max_voltage,package,dielectric,temp_min,temp_max,library,source\n";

/// A board with a supply and one bypass capacitor of the invented 100 nF rows of
/// [`class_rows`], and `extra` inside it.
fn bypass_board(extra: &str) -> String {
    format!(
        "\
import std::env::Environment
import std::parts::passive::Capacitor
import std::sources::Rail

board Widget {{
    .supply: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }}
    .c: Capacitor {{ .capacitance &= 100nF +/- 10% }}
    .supply.pos ~> .c ~> .supply.neg
{extra}}}
"
    )
}

/// Project capacitor rows, each of the one value only they have. `rows` are the cells from
/// `dielectric` to `temp_max`.
fn class_rows(rows: &[(&str, &str)]) -> Catalogs {
    let mut text = CAPACITOR_HEADER.to_string();
    for (mpn, cells) in rows {
        text.push_str(&format!(
            "{mpn},100nF +/- 10%,derate capacitance by bias:,16V,0402,{cells},Basic,https://example.invalid/{mpn}.pdf\n"
        ));
    }
    let mut c = Catalogs::empty();
    c.add_file("capacitor", "catalogs/capacitor.csv", &text)
        .unwrap();
    c
}

fn environment(low: &str, high: &str) -> String {
    format!("    .environment: Environment {{ .temperature &= {low}..{high} }}\n")
}

#[test]
fn a_row_with_only_a_class_is_judged_by_the_classes_range_and_met() {
    // X5R is -55 to 85 degC: the board's -40 to 85 degC is inside it, so the part is judged,
    // met, and not named as unjudged.
    let rows = class_rows(&[("ZZC-X5R", "X5R,,")]);
    let b = build(&bypass_board(&environment(COLD, WARM)), &rows).unwrap();
    assert_eq!(b.sourcing[0].mpn.as_deref(), Some("ZZC-X5R"));
    assert!(
        environment_findings(&b).is_empty(),
        "{:?}",
        environment_findings(&b)
    );
}

#[test]
fn a_row_with_only_a_class_that_does_not_cover_the_environment_is_not_met_and_says_the_class() {
    // The same row at 125 degC is over X5R's 85 degC.
    let rows = class_rows(&[("ZZC-X5R", "X5R,,")]);
    let e = refused(&bypass_board(&environment(COLD, HOT)), &rows);
    assert!(
        e.contains("the `capacitor` catalog has nothing for `c`"),
        "{e}"
    );
    assert!(
        e.contains("1 row was left out because the temperatures it state do not cover the board's environment, `.environment.temperature` 233.15 K to 398.15 K (-40degC to 125degC)"),
        "{e}"
    );
    assert!(
        e.contains("a row that states no temperature of its own is judged by the range of its dielectric class (X5R), which is the class's and not its datasheet's"),
        "{e}"
    );
}

#[test]
fn a_row_of_a_class_that_covers_the_environment_is_taken_when_an_earlier_one_does_not() {
    // X5R first stops at 85 degC; X7R's class range reaches 125 degC and is picked.
    let rows = class_rows(&[("ZZC-X5R", "X5R,,"), ("ZZC-X7R", "X7R,,")]);
    let b = build(&bypass_board(&environment(COLD, HOT)), &rows).unwrap();
    assert_eq!(b.sourcing[0].mpn.as_deref(), Some("ZZC-X7R"));
    assert!(environment_findings(&b).is_empty());
}

#[test]
fn a_rows_own_range_beats_its_class_at_the_end_it_states() {
    // The X5R row states 125 degC itself: the class's 85 degC is ignored at that end, and the
    // class still gives the cold end (-55 degC) the row leaves empty.
    let rows = class_rows(&[("ZZC-X5R", "X5R,,398.15K")]);
    let b = build(&bypass_board(&environment(COLD, HOT)), &rows).unwrap();
    assert_eq!(b.sourcing[0].mpn.as_deref(), Some("ZZC-X5R"));
    assert!(environment_findings(&b).is_empty());
    // The other way: an X7R row that states 85 degC itself is out at 125 degC, though the class
    // says 125 degC.
    let rows = class_rows(&[("ZZC-X7R", "X7R,218.15K,358.15K")]);
    let e = refused(&bypass_board(&environment(COLD, HOT)), &rows);
    assert!(e.contains("1 row was left out because"), "{e}");
    assert!(
        !e.contains("dielectric class"),
        "a row that states both ends has none of its range from the class: {e}"
    );
    // The row's own cold end beats the class's too: -20 degC is over a stated -10 degC.
    let rows = class_rows(&[("ZZC-X7R", "X7R,263.15K,398.15K")]);
    let e = refused(&bypass_board(&environment(COLD, HOT)), &rows);
    assert!(e.contains("1 row was left out because"), "{e}");
}

#[test]
fn a_row_with_no_class_and_no_range_is_still_unjudged_and_named() {
    let rows = class_rows(&[("ZZC-NONE", ",,")]);
    let b = build(&bypass_board(&environment(COLD, WARM)), &rows).unwrap();
    let findings = environment_findings(&b);
    assert_eq!(findings.len(), 1, "{findings:#?}");
    assert!(
        findings[0].contains("`c` (Capacitor) was picked as ZZC-NONE and its catalog row states no `temp_min` and no `temp_max`"),
        "{}",
        findings[0]
    );
}

#[test]
fn the_design_may_state_the_range_of_a_part_whose_row_has_only_a_class() {
    // The design states both ends, and speaks for the part: the class is not read.
    let extra = format!(
        "{}    .c.temp_min &= {COLD}\n    .c.temp_max &= 423.15K\n",
        environment(COLD, HOT)
    );
    let rows = class_rows(&[("ZZC-X5R", "X5R,,")]);
    let b = build(&bypass_board(&extra), &rows);
    // The solver reads the design's statement before the class, so the 85 degC row is offered.
    let b = b.unwrap();
    assert_eq!(b.sourcing[0].mpn.as_deref(), Some("ZZC-X5R"));
    assert!(environment_findings(&b).is_empty());
}
