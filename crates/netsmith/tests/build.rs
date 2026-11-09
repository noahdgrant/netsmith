//! Source text in, KiCad schematic out.

use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_kicad::sexp::Sexp;
use netsmith_solve::{Catalogs, Library};

const LED: &str = include_str!("../../../examples/led.nsm");
const REGULATOR: &str = include_str!("../../../examples/linear_regulator.nsm");
const STATUS: &str = include_str!("../../../examples/status_indicator.nsm");
const FOUR_LAYER: &str = include_str!("../../../examples/four_layer.nsm");
const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

/// Build with our own small KiCad libraries, so that no KiCad is needed.
fn build(name: &str, src: &str, catalogs: &Catalogs) -> Result<Vec<netsmith::Built>, String> {
    build_with(name, src, &Loader::std(), catalogs, &Libraries::sample())
}

fn built(src: &str) -> Vec<netsmith::Built> {
    build("t.nsm", src, &Catalogs::jlcpcb()).unwrap_or_else(|e| panic!("{e}"))
}

fn error(src: &str) -> String {
    build("t.nsm", src, &Catalogs::jlcpcb())
        .err()
        .expect("the build should fail")
}

#[test]
fn the_led_board_builds_into_a_schematic() {
    let boards = built(LED);
    assert_eq!(boards.len(), 1);
    let b = &boards[0];
    assert_eq!(b.board, "LedExample");

    let root = Sexp::parse(&b.schematic).unwrap();
    assert_eq!(root.head(), Some("kicad_sch"));
    assert_eq!(
        root.find_all("symbol").count(),
        3,
        "a header, a resistor and an LED"
    );
    assert!(
        b.schematic.contains("(property \"Value\" \"330\""),
        "the solver's resistor is the value"
    );
    assert!(b.schematic.contains("Resistor_SMD:R_0402_1005Metric"));
    assert!(b.schematic.contains("LED_SMD:LED_0603_1608Metric"));

    let mut labels: Vec<_> = root
        .find_all("label")
        .map(|l| l.args()[0].text().unwrap().to_string())
        .collect();
    labels.sort();
    assert_eq!(
        labels,
        ["GND", "GND", "VCC", "VCC", "net-led.anode", "net-led.anode"]
    );
    assert!(b.project.contains("LedExample.kicad_pro"));
}

#[test]
fn the_report_lists_parts_and_nets() {
    let b = &built(LED)[0];
    assert!(b.report.contains("R1"), "{}", b.report);
    assert!(
        b.report.contains("Resistor_SMD:R_0402_1005Metric"),
        "{}",
        b.report
    );
    assert!(
        b.report.contains("net net-led.anode: D1.2, R1.2"),
        "{}",
        b.report
    );
}

#[test]
fn what_was_not_decided_is_noted_not_hidden() {
    // The sample catalogs have no LED, so it is left unchosen and the build says so.
    let b = &build("t.nsm", LED, &Catalogs::sample()).unwrap()[0];
    assert!(
        b.notes.iter().any(|n| n.contains("led: not chosen")),
        "{:?}",
        b.notes
    );
    // The LED's current is unknown without its forward voltage; the resistor's drop is not,
    // since the chain passes one current and the resistor was chosen.
    assert!(
        b.notes.iter().any(|n| n.contains("led.current")),
        "{:?}",
        b.notes
    );
    assert!(
        !b.notes.iter().any(|n| n.contains("r1.drop")),
        "{:?}",
        b.notes
    );
}

/// A stage whose output net has a pin that nothing models (ADR 0027): the resistor is joined
/// with `~` and not in a chain.
const OUTLINE_STAGE: &str = r#"import std::parts::passive::Resistor
import std::sources::Rail

/// A step-down stage in outline: a resistor from the input to the output, and a
/// divider from the output to ground. The resistor is joined with `~`, not a chain, so
/// what it carries is not modelled.
module Stage {
    .vin: Rail
    .vout: Rail
    .loose: Resistor { .resistance &= 1ohm +/- 1% }
    .r_top: Resistor { .resistance &= 100kohm +/- 1% }
    .r_bottom: Resistor { .resistance &= 100kohm +/- 1% }

    .vin.pos ~ .loose.a
    .loose.b ~ .vout.pos
    .vin.neg ~ .vout.neg
    .vout.pos ~> .r_top ~> .r_bottom ~> .vout.neg
}

board Supply {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .stage: Stage { .vout.voltage &= 3.3V +/- 3% }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0A..2A
    }
    .input ~ .stage.vin
    .stage.vout ~ .output
}
"#;

#[test]
fn an_incomplete_balance_is_noted_with_the_pins_it_does_not_model() {
    // What the output draws is not stated, so the DC analysis has nothing to solve with and the
    // equations are all there is (a stated load is the next test).
    let src = OUTLINE_STAGE.replace("        .current &= 0A..2A\n", "");
    let b = &built(&src)[0];
    assert!(
        b.notes.iter().any(|n| n
            == "the balance on net `3V3` in `stage` is incomplete: `stage.loose.b` is not modelled, \
                so `stage.r_top.current` is not set from it"),
        "{:#?}",
        b.notes
    );
    // The input resistor's power is not evaluated, which is no failure: its ends are two rails
    // that are 1.5 V to 2.2 V apart over 1 ohm, which Ohm's law does not prove within a rating.
    // The divider's two resistors are proven by the bound and say nothing.
    assert!(
        b.notes
            .iter()
            .any(|n| n.starts_with("not evaluated:") && n.contains("stage.loose.max_power")),
        "{:#?}",
        b.notes
    );
}

#[test]
fn a_stated_load_lets_the_dc_analysis_judge_the_stage_and_the_notes_go() {
    // With what the output draws stated, the DC analysis solves the stage (the resistor, the
    // divider and the load) and judges the resistor's power with the numbers: nothing waits, and
    // the balance's note goes with the current it said was not set (ADR 0076).
    //
    // The stage's output window encloses what the circuit gives (a 5 V input less a 1 ohm drop,
    // 4.73 V to 5.25 V): a stated range the circuit goes outside of is an error of its own
    // (ADR 0076, amendment 17), and the window the other tests state (3.3 V +/- 3%) is the
    // outline's, which no circuit here gives.
    let src = OUTLINE_STAGE
        .replace("0A..2A", "0A..20mA")
        .replace("3.3V +/- 3%", "3V..5.5V");
    let b = &built(&src)[0];
    assert!(
        !b.notes.iter().any(|n| n.contains("stage.loose.max_power")),
        "{:#?}",
        b.notes
    );
    assert!(
        !b.notes.iter().any(|n| n.contains("is incomplete")),
        "{:#?}",
        b.notes
    );
    // A load that the 1 ohm resistor cannot carry within its rating is an error that says so.
    let err = build("t.nsm", OUTLINE_STAGE, &Catalogs::jlcpcb())
        .err()
        .expect("2 A through 1 ohm and 100 mW");
    assert!(
        err.contains(
            "(stage.loose.drop * stage.loose.current) must always be at most stage.loose.max_power"
        ) && err.contains("the DC analysis puts `stage.loose.current` at")
            && err.contains("output at 2A"),
        "{err}"
    );
}

#[test]
fn a_complete_board_has_no_note_about_its_balances() {
    for src in [LED, REGULATOR, TWO_BOARDS] {
        for b in built(src) {
            assert!(
                !b.notes.iter().any(|n| n.contains("is incomplete")),
                "{:#?}",
                b.notes
            );
        }
    }
}

#[test]
fn building_twice_gives_identical_files() {
    let (a, b) = (&built(LED)[0], &built(LED)[0]);
    assert_eq!(a.schematic, b.schematic);
    assert_eq!(a.project, b.project);
}

#[test]
fn a_mistake_in_the_source_is_reported_with_its_line() {
    let e = error(&LED.replace("5V +/- 5%", "5mA +/- 5%"));
    assert!(e.contains("holds V, but this value is A"), "{e}");
    assert!(e.contains("t.nsm:"), "{e}");
}

#[test]
fn a_requirement_nothing_can_meet_stops_the_build_with_the_numbers() {
    let e = error(&LED.replace(".current &= 5mA..15mA", ".current &= 9.9mA..10.1mA"));
    assert!(
        e.contains("no combination of candidates satisfies `led` and `r1` together"),
        "{e}"
    );
    // The closest candidate is no longer one the window rejects: 620 ohm keeps the current
    // inside it, but dissipates up to 63.9 mW in a part rated for 62.5 mW.
    assert!(
        e.contains("r1.max_power") && e.contains("60.15mW..63.88mW is not always below 62.5mW"),
        "{e}"
    );
}

#[test]
fn the_regulator_stage_builds_with_its_catalog_pinout() {
    let b = &built(REGULATOR)[0];
    assert_eq!(b.board, "RegulatorExample");
    assert!(b.report.contains("U1"), "{}", b.report);
    assert!(
        b.report.contains("Package_TO_SOT_SMD:SOT-23 "),
        "{}",
        b.report
    );
    assert!(
        b.report.contains("net GND: C1.2, C2.2, J1.2, J2.2, U1.1"),
        "{}",
        b.report
    );
    assert!(b.schematic.contains("Regulator_Linear:AP1117-15"));
}

#[test]
fn the_build_carries_the_sourcing_of_every_part() {
    let rows = |src| -> Vec<(String, Option<String>, Option<Library>, bool)> {
        built(src)[0]
            .sourcing
            .iter()
            .map(|s| (s.refdes.clone(), s.lcsc.clone(), s.library, s.assembled()))
            .collect()
    };
    let part = |r: &str, l: &str, lib| (r.to_string(), Some(l.to_string()), Some(lib), true);
    // Hand-soldered, and sourced: the `header` catalog rates its contacts (ADR 0040).
    let header = |r: &str| {
        (
            r.to_string(),
            Some("C42431798".to_string()),
            Some(Library::Extended),
            false,
        )
    };
    assert_eq!(
        rows(LED),
        [
            part("D1", "C2286", Library::Basic),
            header("J1"),
            part("R1", "C25104", Library::Basic)
        ]
    );
    assert_eq!(
        rows(REGULATOR),
        [
            header("J1"),
            header("J2"),
            part("C1", "C315248", Library::Extended),
            part("C2", "C59461", Library::Basic),
            part("U1", "C5446", Library::Basic)
        ]
    );
    assert_eq!(
        rows(STATUS),
        [
            header("J1"),
            header("J2"),
            part("C1", "C1525", Library::Basic),
            part("R1", "C11702", Library::Basic),
            part("D1", "C125094", Library::Extended)
        ]
    );
}

#[test]
fn the_report_says_where_each_part_comes_from() {
    let r = &built(LED)[0].report;
    assert!(r.contains("C25104 Basic"), "{r}");
    assert!(r.contains("C2286 Basic"), "{r}");
    assert!(r.contains("hand-soldered"), "{r}");
}

#[test]
fn what_is_hand_soldered_is_what_kicad_says_is_through_hole() {
    for src in [LED, REGULATOR, STATUS] {
        for s in &built(src)[0].sourcing {
            let libraries = Libraries::sample();
            let footprint = libraries
                .mapping
                .map(&s.kind, s.package.as_deref())
                .unwrap()
                .footprint;
            let through_hole = libraries.footprints.load(&footprint).unwrap().mount
                == netsmith_pcb::Mount::ThroughHole;
            assert_eq!(s.assembled(), !through_hole, "{} {}", s.refdes, s.kind);
        }
    }
}

#[test]
fn the_status_indicator_builds() {
    let b = &built(STATUS)[0];
    assert_eq!(b.board, "IndicatorExample");
    for refdes in ["J1", "J2", "C1", "R1", "D1"] {
        assert!(
            b.report.contains(refdes),
            "{refdes} is missing:\n{}",
            b.report
        );
    }
}

#[test]
fn a_part_kicad_has_no_symbol_for_stops_the_build_and_says_why() {
    let src = "\
part Odd {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
}

board B {
    .p: Odd
    .p.a ~ .p.b
}
";
    let e = error(src);
    assert!(
        e.contains("there is no KiCad symbol for a `Odd` yet"),
        "{e}"
    );
}

#[test]
fn a_part_with_no_pads_stops_the_build_and_says_why() {
    let src = "\
part Bare {
    .a: Pin
    .b: Pin
}

board B {
    .p: Bare
    .p.a ~ .p.b
}
";
    let e = error(src);
    assert!(
        e.contains("`p` is a `Bare`") && e.contains("no pads"),
        "{e}"
    );
}

#[test]
fn a_file_with_no_board_is_an_error() {
    let e = error("part P {\n    .a: Pin\n}\n");
    assert!(e.contains("has no `board` to build"), "{e}");
}

/// The LED example without its manufacturing process.
fn led_without_a_process() -> String {
    let source = LED.replace(
        "    /// Made by JLCPCB's two-layer process, on a board 1.6 mm thick.\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n\n",
        "",
    );
    assert_ne!(source, LED, "the example names a process");
    source
}

#[test]
fn a_board_with_no_process_still_builds_a_schematic() {
    let b = &built(&led_without_a_process())[0];
    assert_eq!(b.design.process, None);
    assert_eq!(b.design.parts.len(), 3);
}

#[test]
fn the_process_is_carried_into_the_design() {
    let p = built(LED)[0].design.process.clone().expect("a process");
    assert_eq!(
        (p.name.as_str(), p.fabricator.as_str()),
        ("TwoLayer", "JLCPCB")
    );
    assert_eq!(p.stackup.chosen_thickness(), Some(1.6));
}

#[test]
fn narrowing_a_rule_outside_what_the_process_can_make_names_the_field() {
    let finer = LED.replace(
        ".stackup.thickness &= 1.6mm",
        ".stackup.thickness &= 1.6mm\n        .min_track &= 0.05mm..0.08mm",
    );
    let e = error(&finer);
    assert!(
        e.contains("contradiction on `process.min_track`")
            && e.contains("100um..1m")
            && e.contains("50um..80um"),
        "{e}"
    );
    let thicker = LED.replace(".stackup.thickness &= 1.6mm", ".stackup.thickness &= 3mm");
    let e = error(&thicker);
    assert!(
        e.contains("contradiction on `process.stackup.thickness`"),
        "{e}"
    );
}

#[test]
fn the_four_layer_board_builds_into_a_schematic_like_any_other() {
    let boards = built(FOUR_LAYER);
    assert_eq!(boards[0].board, "FourLayerExample");
    let p = boards[0].design.process.as_ref().unwrap();
    assert_eq!((p.name.as_str(), p.stackup.layers), ("FourLayer", 4));
}

#[test]
fn a_four_layer_board_that_asks_for_another_thickness_is_told_the_field() {
    // Only the 1.6 mm build is sourced, so only it is offered.
    let thinner = FOUR_LAYER.replace(
        ".inner1.net &= \"GND\"",
        ".inner1.net &= \"GND\"\n        .stackup.thickness &= 1.2mm",
    );
    let e = error(&thinner);
    assert!(
        e.contains("contradiction on `process.stackup.thickness`"),
        "{e}"
    );
}

#[test]
fn a_module_cannot_name_a_planes_net_even_through_a_process() {
    // The process is the board's member, so the board names the plane's net.
    let in_module = "import std::mfg::jlcpcb::FourLayer\nmodule M {\n    .p: FourLayer\n    .p.inner1.net &= \"GND\"\n}\nboard B {\n    .m: M\n}\n";
    let e = error(in_module);
    assert!(
        e.contains("`p.inner1.net` names a net, and only a board names nets"),
        "{e}"
    );
}

// ---- a system -----------------------------------------------------------------------

#[test]
fn a_system_builds_a_schematic_per_board_in_a_directory_per_board() {
    let boards = built(TWO_BOARDS);
    let labels: Vec<_> = boards.iter().map(|b| b.label()).collect();
    assert_eq!(labels, ["TwoBoards/power", "TwoBoards/led"]);
    assert_eq!(
        boards[0].dir(std::path::Path::new("out")),
        std::path::Path::new("out/TwoBoards/power")
    );
    for b in &boards {
        let root = Sexp::parse(&b.schematic).unwrap();
        assert_eq!(root.head(), Some("kicad_sch"));
        assert!(b.project.contains(&format!("{}.kicad_pro", b.design.name)));
    }
    assert_eq!(boards[0].design.name, "PowerBoard");
    assert_eq!(boards[1].design.name, "LedBoard");
    assert!(
        boards[0]
            .report
            .starts_with("board TwoBoards/power (PowerBoard)")
    );
    // Each board has its own parts, numbered from one.
    assert!(boards[1].report.contains("D1"), "{}", boards[1].report);
    assert!(!boards[1].report.contains("U1"), "{}", boards[1].report);
    // Paths in a note are from the board, as they would be if it were built alone.
    assert!(boards[1].notes.is_empty(), "{:?}", boards[1].notes);
}

#[test]
fn a_board_a_system_places_is_built_once_and_a_board_it_does_not_is_still_built() {
    let src = format!("{TWO_BOARDS}\nboard Spare {{\n}}\n");
    let labels: Vec<_> = built(&src).iter().map(|b| b.label()).collect();
    assert_eq!(labels, ["Spare", "TwoBoards/power", "TwoBoards/led"]);
}

#[test]
fn a_file_with_only_a_board_builds_as_before() {
    let b = &built(LED)[0];
    assert_eq!(
        (b.system.as_deref(), b.label().as_str()),
        (None, "LedExample")
    );
    assert_eq!(
        b.dir(std::path::Path::new("out")),
        std::path::Path::new("out/LedExample")
    );
}

#[test]
fn a_system_that_asks_too_much_of_a_board_is_refused_with_both_boards_and_the_numbers() {
    // The LED board's LED is run at 150 mA, whatever its resistor says.
    let src = TWO_BOARDS
        .replace(
            ".led.current == (.input.voltage - .led.forward_voltage) / .r1.resistance\n",
            "",
        )
        .replace(".current &= 5mA..20mA", ".current &= 150mA");
    let e = error(&src);
    for needle in [
        "led.input ~ power.output",
        "`power.output.current` (board `PowerBoard`)",
        "`led.input.current` (board `LedBoard`)",
        "0A..50mA",
        "-150mA",
    ] {
        assert!(e.contains(needle), "`{needle}` is not in:\n{e}");
    }
    assert!(e.contains("two_boards") || e.contains("t.nsm"), "{e}");
}

#[test]
fn a_system_with_no_boards_is_refused() {
    let e = error("system Empty {\n}\n");
    assert!(e.contains("system Empty has no boards"), "{e}");
}

// ---- KiCad libraries -------------------------------------------------------------------

use crate::common;

use netsmith_kicad::Mapping;
use netsmith_kicad::testlib::TempDir;

#[test]
fn a_build_without_the_symbol_library_stops_once_and_names_the_path_and_the_variable() {
    let libraries = Libraries::with_mapping(
        Mapping::standard(),
        netsmith_kicad::SymbolLib::at("/nonexistent/symbols"),
        netsmith_pcb::FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints()),
    );
    let e = build_with(
        "t.nsm",
        LED,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .err()
    .expect("the build should fail");
    assert!(e.contains("symbol library `Device` not found"), "{e}");
    assert!(e.contains("/nonexistent/symbols/Device.kicad_sym"), "{e}");
    assert!(e.contains("NETSMITH_KICAD_SYMBOLS"), "{e}");
    assert_eq!(e.matches("not found").count(), 1, "{e}");
}

#[test]
fn a_build_without_the_footprint_library_names_the_path_and_the_variable() {
    let libraries = Libraries::with_mapping(
        Mapping::standard(),
        netsmith_kicad::SymbolLib::at(netsmith_kicad::testlib::sample_symbols()),
        netsmith_pcb::FootprintLibrary::at("/nonexistent/footprints"),
    );
    let e = build_with(
        "t.nsm",
        LED,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .err()
    .expect("the build should fail");
    assert!(e.contains("footprint `"), "{e}");
    assert!(e.contains("/nonexistent/footprints/"), "{e}");
    assert!(e.contains("NETSMITH_KICAD_FOOTPRINTS"), "{e}");
}

/// The instances of `reference` in a schematic, as `(unit, pin numbers)`.
fn instances(schematic: &str, reference: &str) -> Vec<(String, Vec<String>)> {
    let root = Sexp::parse(schematic).unwrap();
    root.find_all("symbol")
        .filter(|s| {
            s.find_all("property").any(|p| {
                p.args()[0].text() == Some("Reference") && p.args()[1].text() == Some(reference)
            })
        })
        .map(|s| {
            (
                s.find("unit").unwrap().args()[0]
                    .text()
                    .unwrap()
                    .to_string(),
                s.find_all("pin")
                    .map(|p| p.args()[0].text().unwrap().to_string())
                    .collect(),
            )
        })
        .collect()
}

#[test]
fn a_part_from_a_project_library_with_several_units_is_placed_whole() {
    let dir = TempDir::new("project-build");
    let libraries = common::project_libraries(dir.path());
    let b = &build_with(
        "t.nsm",
        common::DUAL_BOARD,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"))[0];

    // Every unit is an instance of the one part, with its own pins.
    let refdes = &b
        .design
        .parts
        .iter()
        .find(|p| p.kind == "Dual")
        .unwrap()
        .refdes;
    assert_eq!(
        instances(&b.schematic, refdes),
        [
            (
                "1".to_string(),
                vec!["3".to_string(), "2".into(), "1".into()]
            ),
            (
                "2".to_string(),
                vec!["5".to_string(), "6".into(), "7".into()]
            ),
            ("3".to_string(), vec!["8".to_string(), "4".into()]),
        ]
    );
    // Pins 2, 3 and 5 are on no net, so each gets a no-connect flag.
    let root = Sexp::parse(&b.schematic).unwrap();
    assert_eq!(root.find_all("no_connect").count(), 3);
    // The library's symbol is embedded once, as `acme:Dual`.
    let embedded: Vec<_> = root
        .find("lib_symbols")
        .unwrap()
        .find_all("symbol")
        .map(|s| s.args()[0].text().unwrap().to_string())
        .collect();
    assert_eq!(embedded.iter().filter(|n| *n == "acme:Dual").count(), 1);
    assert!(
        b.schematic
            .contains("(property \"Footprint\" \"acme:DIP-8\"")
    );
    // The DIP's footprint says through-hole, so the board house does not place it.
    let u = b.sourcing.iter().find(|s| s.kind == "Dual").unwrap();
    assert!(!u.assembled());
    // And the report names the project's footprint.
    assert!(b.report.contains("acme:DIP-8"), "{}", b.report);
}

#[test]
fn a_project_library_that_is_missing_names_the_file_kicad_toml_lists() {
    let dir = TempDir::new("project-missing");
    let libraries = common::project_libraries(dir.path());
    std::fs::remove_file(dir.path().join("libs/acme.kicad_sym")).unwrap();
    let e = build_with(
        "t.nsm",
        common::DUAL_BOARD,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .err()
    .expect("the build should fail");
    assert!(e.contains("symbol library `acme` not found"), "{e}");
    assert!(e.contains("kicad.toml lists"), "{e}");
    assert!(e.contains("libs/acme.kicad_sym"), "{e}");
}

#[test]
fn a_build_with_no_kicad_install_stops_once_and_lists_where_it_looked() {
    use netsmith_kicad::install::Search;
    let dir = TempDir::new("project-no-install");
    common::project_libraries(dir.path());
    let nowhere = TempDir::new("no-install");
    let machine = Search::under(nowhere.path());
    let libraries = Libraries::with_mapping(
        netsmith_kicad::Mapping::for_project(Some(dir.path())).unwrap(),
        netsmith_kicad::SymbolLib::installed_by(&machine),
        netsmith_pcb::FootprintLibrary::installed_by(&machine),
    );
    let e = build_with(
        "t.nsm",
        common::DUAL_BOARD,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .err()
    .expect("the build should fail: the header is a stock part");
    assert!(e.contains("no KiCad "), "{e}");
    assert!(e.contains("usr/share/kicad/"), "{e}");
    assert!(e.contains("Applications/KiCad/KiCad.app"), "{e}");
    assert!(e.contains("mnt/c/Program Files/KiCad/9.0"), "{e}");
    assert!(e.contains("NETSMITH_KICAD_SYMBOLS"), "{e}");
    assert!(e.contains("NETSMITH_KICAD_FOOTPRINTS"), "{e}");
    // The library of the project itself is never the one reported missing.
    assert!(!e.contains("`acme`"), "{e}");
}

#[test]
fn a_pin_shared_by_every_unit_is_labelled_in_each() {
    let dir = TempDir::new("project-shared");
    let libraries = common::project_libraries(dir.path());
    let b = &build_with(
        "t.nsm",
        common::SHARED_BOARD,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"))[0];
    let refdes = &b
        .design
        .parts
        .iter()
        .find(|p| p.kind == "Shared")
        .unwrap()
        .refdes;
    let placed = instances(&b.schematic, refdes);
    assert_eq!(placed.len(), 2);
    assert!(
        placed
            .iter()
            .all(|(_, pins)| pins.contains(&"9".to_string()))
    );
    // The ground net is labelled on the header, on the shared pin of both units, and on
    // the power flag that supplies it.
    let root = Sexp::parse(&b.schematic).unwrap();
    let net = &b
        .design
        .nets
        .iter()
        .find(|n| n.pins.iter().any(|p| p.pin == "9"))
        .unwrap()
        .name;
    let labels = root
        .find_all("label")
        .filter(|l| l.args()[0].text() == Some(net.as_str()))
        .count();
    assert_eq!(labels, 4);
    // Pin 1 is on no net.
    assert_eq!(root.find_all("no_connect").count(), 1);
}

/// Two stages that each wrap a regulator behind their own rails, on one input rail: the guide's
/// `Stage` twice. Each stage's rail is joined to its regulator's, and the supply's current is
/// the sum of what the two stages' loads draw.
const TWO_STAGES: &str = r#"import std::parts::connector::Header1x2
import std::parts::regulator::LinearRegulator
import std::sources::Rail

module Stage {
    .vin: Rail
    .vout: Rail
    .regulator: LinearRegulator
    .vin ~ .regulator.vin
    .regulator.vout ~ .vout
}

board TwoStages {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .a: Stage { .vout.voltage &= 3.3V +/- 5% }
    .b: Stage { .vout.voltage &= 3.3V +/- 5% }
    .out_a: Rail {
        .pos.net &= "A3V3"
        .neg.net &= "GND"
        .current &= 0mA..20mA
    }
    .out_b: Rail {
        .pos.net &= "B3V3"
        .neg.net &= "GND"
        .current &= 0mA..30mA
    }
    .input ~ .a.vin
    .input ~ .b.vin
    .a.vout ~ .out_a
    .b.vout ~ .out_b
    .in_header: Header1x2
    .input.pos ~ .in_header.p1
    .input.neg ~ .in_header.p2
}
"#;

#[test]
fn two_stages_that_wrap_a_regulator_each_add_up_on_the_input_rail() {
    // A module's rail is the current of the part it is joined to (ADR 0077): nothing waits on a circle.
    let b = &built(TWO_STAGES)[0];
    assert!(
        !b.notes.iter().any(|n| n.contains("depend on each other")),
        "{:#?}",
        b.notes
    );
}
