//! A pick between the providers of a generic (ADR 0068), from the source to the notes and the
//! design: the first provider for which the whole design holds is taken, and what was passed
//! over is said. Every part, number and name is invented.

use netsmith_pipeline::providers::{self, MAX_PROVIDER_TRIALS};
use netsmith_pipeline::{Outcome, Problem, outcome_with};
use netsmith_check::Loader;
use netsmith_solve::Catalogs;

const GENERIC: &str = "\
generic CoaxReceptacle {
    .designator &= \"J\"
    pins {
        .signal passive
        .shield passive
    }
    .impedance: Resistance
    .impedance &= 50ohm
    .max_frequency: Frequency
    .mpn: String
}
";

/// A provider in its own file: `(module, name, top frequency, part number)`.
type Provider = (&'static str, &'static str, &'static str, &'static str);

fn provider_source(name: &str, fmax: &str, catalog: &str) -> String {
    format!(
        "import src::rf::CoaxReceptacle\n\npart {name} provides CoaxReceptacle {{\n    pins {{\n        .signal passive \"SIGNAL\" \"1\"\n        .shield passive \"GND\" \"2\" \"3\"\n    }}\n    .max_frequency &= {fmax}\n\n    .package: Package\n    catalog {catalog} {{ .mpn, .package }}\n}}\n"
    )
}

struct World {
    loader: Loader,
    catalogs: Catalogs,
}

fn world(providers: &[Provider]) -> World {
    let mut loader = Loader::std();
    loader.insert("src::rf", "src/rf.nsm", GENERIC);
    let mut catalogs = Catalogs::jlcpcb();
    for (module, name, fmax, mpn) in providers {
        let catalog = name.to_lowercase();
        loader.insert(
            module,
            &format!("{}.nsm", module.replace("::", "/")),
            &provider_source(name, fmax, &catalog),
        );
        catalogs
            .add_file(
                &catalog,
                &format!("catalogs/{catalog}.csv"),
                &format!("mpn,package,library,source\n{mpn},PKG_{name},Extended,https://example.invalid/{mpn}\n"),
            )
            .unwrap();
    }
    World { loader, catalogs }
}

fn imports(providers: &[Provider]) -> String {
    let mut text = String::from("import src::rf::CoaxReceptacle\n");
    for (module, name, _, _) in providers {
        text.push_str(&format!("import {module}::{name}\n"));
    }
    text
}

const ALPHA: Provider = ("src::alpha", "AlphaCoax", "1GHz", "ALPHA-1");
const BETA: Provider = ("src::beta", "BetaCoax", "3GHz", "BETA-2");

fn feed(providers: &[Provider], body: &str) -> String {
    format!("{}\nboard Feed {{\n{body}\n}}\n", imports(providers))
}

fn build(w: &World, source: &str) -> Result<Outcome, Vec<Problem>> {
    outcome_with("main.nsm", source, &w.loader, &w.catalogs, None)
}

fn messages(problems: &[Problem]) -> Vec<String> {
    problems.iter().map(|p| p.message.clone()).collect()
}

#[test]
fn a_requirement_picks_the_provider_that_meets_it() {
    let w = world(&[ALPHA, BETA]);
    let o = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .rf: CoaxReceptacle { .max_frequency >= 500MHz }",
        ),
    )
    .unwrap();
    let b = &o.boards[0];
    assert_eq!(b.providers.len(), 1);
    assert_eq!(b.providers[0].chosen, "AlphaCoax");
    assert!(b.providers[0].passed_over.is_empty());
    assert_eq!(b.notes[0], "rf is a CoaxReceptacle: picked AlphaCoax");
    let part = &b.design.parts[0];
    assert_eq!(part.kind, "AlphaCoax");
    assert_eq!(part.path, "rf");
    assert_eq!(part.refdes, "J1");
}

#[test]
fn a_provider_below_the_requirement_is_passed_over_for_the_next() {
    let w = world(&[ALPHA, BETA]);
    let o = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .rf: CoaxReceptacle { .max_frequency >= 1.61GHz }",
        ),
    )
    .unwrap();
    let b = &o.boards[0];
    assert_eq!(b.providers[0].chosen, "BetaCoax");
    assert_eq!(b.providers[0].passed_over.len(), 1);
    let (name, why) = &b.providers[0].passed_over[0];
    assert_eq!(name, "AlphaCoax");
    assert!(
        why.contains("rf.max_frequency must always be at least 1.61GHz"),
        "{why}"
    );
    assert!(why.contains("1GHz"), "{why}");
    assert!(
        b.notes[0].starts_with("rf is a CoaxReceptacle: picked BetaCoax; passed over AlphaCoax ("),
        "{}",
        b.notes[0]
    );
    assert_eq!(b.design.parts[0].kind, "BetaCoax");
}

#[test]
fn the_picked_provider_is_a_part_at_the_uses_own_path_with_the_catalog_row() {
    let w = world(&[ALPHA, BETA]);
    let o = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .rf: CoaxReceptacle { .max_frequency >= 1.61GHz }",
        ),
    )
    .unwrap();
    let b = &o.boards[0];
    assert_eq!(b.design.parts.len(), 1);
    assert_eq!(b.design.parts[0].path, "rf");
    assert_eq!(b.design.parts[0].package.as_deref(), Some("PKG_BetaCoax"));
    assert_eq!(b.sourcing[0].mpn.as_deref(), Some("BETA-2"));
}

#[test]
fn when_no_provider_works_the_closest_is_named_with_why_the_others_fail() {
    let w = world(&[ALPHA, BETA]);
    let e = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .rf: CoaxReceptacle { .max_frequency >= 10GHz }",
        ),
    )
    .err()
    .expect("nothing reaches 10 GHz");
    assert_eq!(e.len(), 1, "{:?}", messages(&e));
    let m = &e[0].message;
    let lines: Vec<&str> = m.lines().collect();
    assert_eq!(
        lines[0],
        "no CoaxReceptacle provider satisfies `rf` at main.nsm:6: none of the 2 in scope works",
        "{m}"
    );
    // One entry for each provider in the order tried, the closest marked, each with the
    // reason one level in and a failed pick from its own catalog one level further.
    assert_eq!(lines[1], "  AlphaCoax (the closest):", "{m}");
    assert!(lines[2].starts_with("    no `alphacoax` candidate satisfies `rf`:"), "{m}");
    assert!(lines[3].starts_with("      closest of the 1 that fit: "), "{m}");
    assert!(
        lines[4].starts_with("      fails because: rf.max_frequency must always be at least 10GHz, but 1GHz is not always above 10GHz"),
        "{m}"
    );
    assert_eq!(lines[5], "  BetaCoax:", "{m}");
    assert!(
        lines[8].contains("but 3GHz is not always above 10GHz"),
        "{m}"
    );
    assert_eq!(lines.len(), 9, "{m}");
    // At the line of the use.
    assert_eq!(e[0].at.as_ref().unwrap().line, 6, "{:?}", e[0].at);
}

#[test]
fn one_provider_in_scope_keeps_the_message_of_a_part_written_by_its_type() {
    let w = world(&[ALPHA]);
    let by_use = build(
        &w,
        &feed(
            &[ALPHA],
            "    .rf: CoaxReceptacle { .max_frequency >= 10GHz }",
        ),
    )
    .err()
    .unwrap();
    let by_type = build(
        &w,
        &feed(&[ALPHA], "    .rf: AlphaCoax { .max_frequency >= 10GHz }"),
    )
    .err()
    .unwrap();
    assert_eq!(messages(&by_use), messages(&by_type));
    assert!(messages(&by_use)[0].contains("rf.max_frequency must always be at least 10GHz"));
}

#[test]
fn the_part_number_finds_the_provider_whose_catalog_has_it() {
    let w = world(&[ALPHA, BETA]);
    let o = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .rf: CoaxReceptacle { .mpn &= \"BETA-2\" }",
        ),
    )
    .unwrap();
    let b = &o.boards[0];
    assert_eq!(b.providers[0].chosen, "BetaCoax");
    assert_eq!(b.providers[0].passed_over.len(), 1);
    assert_eq!(b.providers[0].passed_over[0].0, "AlphaCoax");
    assert!(
        b.providers[0].passed_over[0]
            .1
            .contains("the catalog has no row with the manufacturer part number BETA-2"),
        "{}",
        b.providers[0].passed_over[0].1
    );
    assert_eq!(b.sourcing[0].mpn.as_deref(), Some("BETA-2"));
}

#[test]
fn a_part_number_that_no_provider_has_is_one_error_naming_each() {
    let w = world(&[ALPHA, BETA]);
    let e = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .rf: CoaxReceptacle { .mpn &= \"NOPE-0\" }",
        ),
    )
    .err()
    .unwrap();
    assert_eq!(e.len(), 1, "{:?}", messages(&e));
    let m = &e[0].message;
    assert!(
        m.starts_with("no CoaxReceptacle provider satisfies `rf` at "),
        "{m}"
    );
    assert!(m.contains("AlphaCoax") && m.contains("BetaCoax"), "{m}");
}

#[test]
fn naming_the_provider_as_the_type_forces_it_and_nothing_is_picked() {
    let w = world(&[ALPHA, BETA]);
    let o = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .rf: BetaCoax { .max_frequency >= 500MHz }",
        ),
    )
    .unwrap();
    let b = &o.boards[0];
    assert!(b.providers.is_empty());
    assert!(
        b.notes.iter().all(|n| !n.contains("picked")),
        "{:?}",
        b.notes
    );
    assert_eq!(b.design.parts[0].kind, "BetaCoax");
    // And a forced provider that fails fails, with no other tried in its place.
    let e = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .rf: AlphaCoax { .max_frequency >= 2GHz }",
        ),
    )
    .err()
    .unwrap();
    assert!(
        messages(&e)[0].contains("rf.max_frequency must always be at least 2GHz"),
        "{:?}",
        messages(&e)
    );
}

#[test]
fn no_provider_in_scope_is_an_error_at_the_use() {
    let w = world(&[ALPHA, BETA]);
    let e = build(&w, &feed(&[], "    .rf: CoaxReceptacle"))
        .err()
        .unwrap();
    assert_eq!(e.len(), 1);
    assert!(e[0].message.starts_with("`rf` is a `CoaxReceptacle`, and no part or module in the files this file imports provides it"), "{}", e[0].message);
    assert_eq!(e[0].at.as_ref().unwrap().line, 4);
}

#[test]
fn the_second_provider_is_tried_only_because_the_first_fails() {
    // Both would do: the first in order is taken.
    let w = world(&[ALPHA, BETA]);
    let o = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .rf: CoaxReceptacle { .max_frequency >= 100MHz }",
        ),
    )
    .unwrap();
    assert_eq!(o.boards[0].providers[0].chosen, "AlphaCoax");
    assert_eq!(providers::trials_run(), 0, "the first solve was enough");
}

#[test]
fn the_pick_is_the_same_whatever_order_the_definitions_are_written_in() {
    let run = |swap: bool| {
        let mut w = world(&[ALPHA, BETA]);
        let a = provider_source("AlphaCoax", "1GHz", "alphacoax");
        let b = provider_source("BetaCoax", "3GHz", "betacoax");
        // Both in one file, in either order, and a second file with a third provider.
        let both = if swap {
            format!("{b}\n{}", a.replace("import src::rf::CoaxReceptacle\n", ""))
        } else {
            format!("{a}\n{}", b.replace("import src::rf::CoaxReceptacle\n", ""))
        };
        w.loader.insert("src::alpha", "src/alpha.nsm", &both);
        let source = "import src::rf::CoaxReceptacle\nimport src::alpha::AlphaCoax\nboard Feed {\n    .rf: CoaxReceptacle { .max_frequency >= 1.61GHz }\n}\n";
        let o = build(&w, source).unwrap();
        (
            o.boards[0].notes.clone(),
            o.boards[0].design.parts[0].kind.clone(),
        )
    };
    let one = run(false);
    let two = run(true);
    assert_eq!(one, two);
    assert_eq!(one.1, "BetaCoax");
}

#[test]
fn the_pick_is_the_same_on_three_runs_and_in_another_project_folder() {
    let first: Vec<_> = (0..3)
        .map(|_| {
            let w = world(&[ALPHA, BETA]);
            let o = build(
                &w,
                &feed(
                    &[ALPHA, BETA],
                    "    .rf: CoaxReceptacle { .max_frequency >= 1.61GHz }",
                ),
            )
            .unwrap();
            (
                o.boards[0].notes.clone(),
                o.boards[0].design.parts[0].kind.clone(),
            )
        })
        .collect();
    assert!(first.windows(2).all(|w| w[0] == w[1]));
    // The same sources under other module paths in another folder: the same provider by name.
    let beta2: Provider = ("deep::elsewhere::beta", "BetaCoax", "3GHz", "BETA-2");
    let alpha2: Provider = ("deep::elsewhere::alpha", "AlphaCoax", "1GHz", "ALPHA-1");
    let mut w = world(&[alpha2, beta2]);
    w.loader.insert("src::rf", "src/rf.nsm", GENERIC);
    let o = build(
        &w,
        &feed(
            &[alpha2, beta2],
            "    .rf: CoaxReceptacle { .max_frequency >= 1.61GHz }",
        ),
    )
    .unwrap();
    assert_eq!(o.boards[0].design.parts[0].kind, first[0].1);
}

#[test]
fn a_design_with_an_unrelated_mistake_is_not_solved_once_per_combination() {
    // Two uses of three providers each, and a resistor nothing can satisfy.
    let gamma: Provider = ("src::gamma", "GammaCoax", "9GHz", "GAMMA-3");
    let w = world(&[ALPHA, BETA, gamma]);
    let body = "    .a: CoaxReceptacle { .max_frequency >= 100MHz }\n    .b: CoaxReceptacle { .max_frequency >= 100MHz }\n    .r: Resistor { .resistance &= 1ohm +/- 0.0001% }";
    let source = format!(
        "import std::parts::passive::Resistor\n{}\nboard Feed {{\n{body}\n}}\n",
        imports(&[ALPHA, BETA, gamma])
    );
    let before = providers::trials_run();
    let e = build(&w, &source).err().unwrap();
    let spent = providers::trials_run() - before;
    assert!(messages(&e)[0].contains("`r`"), "{:?}", messages(&e));
    assert!(spent <= 4, "{spent} trials for 3 x 3 combinations");
    assert!(spent >= 1);
}

#[test]
fn the_search_stops_at_the_bound_and_says_which_uses_it_was_about() {
    // Four uses, each with three providers that all fail, each with its own number: 81
    // combinations, more than are tried.
    let mut loader = Loader::std();
    loader.insert(
        "src::g",
        "src/g.nsm",
        "generic Thing {\n    .rating: Voltage\n}\n",
    );
    let mut text = String::from("import src::g::Thing\n");
    for p in ["One", "Two", "Three"] {
        let volts = match p {
            "One" => 1,
            "Two" => 2,
            _ => 3,
        };
        loader.insert(
            &format!("src::{}", p.to_lowercase()),
            &format!("src/{}.nsm", p.to_lowercase()),
            &format!(
                "import src::g::Thing\npart P{p} provides Thing {{\n    .rating &= {volts}V\n}}\n"
            ),
        );
        text.push_str(&format!("import src::{}::P{p}\n", p.to_lowercase()));
    }
    text.push_str("board B {\n");
    for u in ["a", "b", "c", "d"] {
        text.push_str(&format!("    .{u}: Thing {{ .rating >= 10V }}\n"));
    }
    text.push_str("}\n");
    let before = providers::trials_run();
    let e = outcome_with("main.nsm", &text, &loader, &Catalogs::jlcpcb(), None)
        .err()
        .unwrap();
    assert_eq!(providers::trials_run() - before, MAX_PROVIDER_TRIALS - 1);
    // What is wrong with the design as it stands first, one error for each of the four uses
    // at their first provider; then that the search gave up.
    let m = messages(&e);
    assert_eq!(m.len(), 5, "{m:?}");
    for (message, use_) in m.iter().zip(["a", "b", "c", "d"]) {
        assert!(
            message.starts_with(&format!("{use_}.rating must always be at least 10V")),
            "{message}"
        );
    }
    assert_eq!(
        m[4],
        format!(
            "gave up after trying {MAX_PROVIDER_TRIALS} provider combinations for `a` and `b` and `c` and `d`; give them fewer providers in scope, or name a provider as the type of one of them"
        )
    );
}

#[test]
fn a_failing_design_with_many_uses_says_what_is_wrong_before_it_says_it_gave_up() {
    // 70 uses of a generic with two providers, both of which serve, and a resistor that no
    // row serves: the mistake is the resistor's, and the other providers change nothing. The
    // bound is reached before every use has been varied, and the build used to say only that
    // it gave up.
    let w = world(&[ALPHA, BETA]);
    let mut body = String::from("    .r: Resistor { .resistance &= 1ohm +/- 0.0001% }\n");
    for i in 0..70 {
        body.push_str(&format!("    .rf{i:02}: CoaxReceptacle\n"));
    }
    let source = format!(
        "import std::parts::passive::Resistor\n{}\nboard Feed {{\n{body}}}\n",
        imports(&[ALPHA, BETA])
    );
    let before = providers::trials_run();
    let e = build(&w, &source).err().unwrap();
    assert_eq!(providers::trials_run() - before, MAX_PROVIDER_TRIALS - 1);
    let m = messages(&e);
    assert_eq!(m.len(), 2, "{m:?}");
    assert!(
        m[0].starts_with("the `resistor` catalog has nothing for `r`"),
        "{}",
        m[0]
    );
    // The seven uses the bound did not reach, six named and the rest counted.
    assert_eq!(
        m[1],
        format!(
            "gave up after trying {MAX_PROVIDER_TRIALS} provider combinations for `rf63` and `rf64` and `rf65` and `rf66` and `rf67` and `rf68` and 1 more; give them fewer providers in scope, or name a provider as the type of one of them"
        )
    );
}

#[test]
fn a_pass_found_while_a_use_is_varied_alone_is_not_solved_again() {
    // `a` needs 2 GHz, which only the second provider has, and `b` asks nothing. Varying `a`
    // alone finds the pass; the search in order reaches the same assignment and takes it
    // from there, so the two trials are the two assignments tried, not three.
    let w = world(&[ALPHA, BETA]);
    let before = providers::trials_run();
    let o = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .a: CoaxReceptacle { .max_frequency >= 2GHz }\n    .b: CoaxReceptacle",
        ),
    )
    .unwrap();
    assert_eq!(providers::trials_run() - before, 2);
    let picked: Vec<(&str, &str)> = o.boards[0]
        .providers
        .iter()
        .map(|p| (p.path.as_str(), p.chosen.as_str()))
        .collect();
    assert_eq!(picked, [("a", "BetaCoax"), ("b", "AlphaCoax")]);
}

#[test]
fn the_pick_is_made_use_by_use_in_path_order_and_the_first_keeps_its_choice() {
    // `a` takes the first provider that works for it whatever `b` needs.
    let w = world(&[ALPHA, BETA]);
    let o = build(
        &w,
        &feed(
            &[ALPHA, BETA],
            "    .a: CoaxReceptacle { .max_frequency >= 500MHz }\n    .b: CoaxReceptacle { .max_frequency >= 2GHz }",
        ),
    )
    .unwrap();
    let picked: Vec<(&str, &str)> = o.boards[0]
        .providers
        .iter()
        .map(|p| (p.path.as_str(), p.chosen.as_str()))
        .collect();
    assert_eq!(picked, [("a", "AlphaCoax"), ("b", "BetaCoax")]);
}

#[test]
fn a_physical_requirement_keyed_by_the_uses_path_still_applies_after_a_pick() {
    let mut w = world(&[ALPHA, BETA]);
    w.loader.attach_physical(
        "main.nsm",
        "main.physical.nsm",
        "import std::pcb::Outline\nphysical for Feed {\n    case {\n        .outline: Outline {\n            .width &= 30mm\n            .height &= 20mm\n        }\n    }\n    .rf.place.left &= 1mm..5mm\n}\n",
    );
    let body = "    .process: TwoLayer { .stackup.thickness &= 1.6mm }\n    .rf: CoaxReceptacle { .max_frequency >= 1.61GHz }";
    let source = format!(
        "import std::mfg::jlcpcb::TwoLayer\n{}\nboard Feed {{\n{body}\n}}\n",
        imports(&[ALPHA, BETA])
    );
    let o = build(&w, &source).unwrap_or_else(|e| panic!("{:?}", messages(&e)));
    let b = &o.boards[0];
    assert_eq!(b.design.parts[0].path, "rf");
    assert_eq!(b.design.parts[0].kind, "BetaCoax");
    assert!(
        !b.design.features.constraints.is_empty(),
        "the requirement on `rf` reached the design for the picked part"
    );
}

#[test]
fn a_system_picks_for_the_boards_it_holds() {
    let w = world(&[ALPHA, BETA]);
    let source = format!(
        "{}\nboard Feed {{\n    .rf: CoaxReceptacle {{ .max_frequency >= 1.61GHz }}\n}}\n\nsystem Pair {{\n    .main: Feed\n}}\n",
        imports(&[ALPHA, BETA])
    );
    let o = build(&w, &source).unwrap_or_else(|e| panic!("{:?}", messages(&e)));
    let b = o.boards.iter().find(|b| b.system.is_some()).unwrap();
    assert_eq!(b.providers.len(), 1);
    assert_eq!(b.providers[0].path, "rf");
    assert_eq!(b.providers[0].chosen, "BetaCoax");
    assert_eq!(b.design.parts[0].kind, "BetaCoax");
}

#[test]
fn a_use_inside_a_module_is_picked_with_the_path_through_the_module() {
    let w = world(&[ALPHA, BETA]);
    let source = format!(
        "{}\nmodule Antenna {{\n    .rf: CoaxReceptacle {{ .max_frequency >= 1.61GHz }}\n}}\n\nboard Feed {{\n    .ant: Antenna\n}}\n",
        imports(&[ALPHA, BETA])
    );
    let o = build(&w, &source).unwrap();
    let b = &o.boards[0];
    assert_eq!(b.providers[0].path, "ant.rf");
    assert_eq!(b.providers[0].chosen, "BetaCoax");
    assert_eq!(b.design.parts[0].path, "ant.rf");
}

// ---- a module-shaped generic: a buck converter with two invented ICs ------------------------

const BUCK_GENERIC: &str = "\
import std::sources::Rail

/// A buck regulator with everything around it.
generic BuckConverter {
    .vin: Rail
    .vout: Rail
    /// The highest voltage the input pin survives.
    .vin_abs_max: Voltage
    // A transient never goes past what the input survives.
    .vin.peak <= .vin_abs_max
}
";

/// The guide's invented 4.5 V to 6 V, 2 A converter.
const ZBK2: &str = "\
import std::parts::regulator::BuckIc

part Zbk2 provides BuckIc {
    .vref &= 0.8V +/- 1.5%
    .fsw &= 1.2MHz +/- 15%
    .min_input_voltage &= 4.5V
    .max_input_voltage &= 6V
    .max_output_current &= 2A
    .efficiency &= 0.85..0.95
    .min_inductance &= 1.5uH
    .min_inductor_saturation &= 3A
    .max_inductor_dcr &= 0.1ohm
    .min_input_capacitance &= 2uF
    .min_output_capacitance &= 10uF
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)

    pins {
        .vin.pos   power   \"VIN\"  \"1\" \"2\"
        .vin.neg   power   \"GND\"  \"3\" \"9\"
        .sw.pos    power   \"SW\"   \"4\" \"5\"
        .fb        input   \"FB\"   \"6\"
        .en        input   \"EN\"   \"7\"
    }

    support {
        .en ~ .vin.pos
    }
}
";

/// Another invented IC: 4.5 V to 60 V, 3.5 A.
const ZBK9: &str = "\
import std::parts::regulator::BuckIc

part Zbk9 provides BuckIc {
    .vref &= 0.8V +/- 1.5%
    .fsw &= 1.2MHz +/- 15%
    .min_input_voltage &= 4.5V
    .max_input_voltage &= 60V
    .max_output_current &= 3.5A
    .efficiency &= 0.85..0.95
    .min_inductance &= 1.5uH
    .min_inductor_saturation &= 3A
    .max_inductor_dcr &= 0.1ohm
    .min_input_capacitance &= 2uF
    .min_output_capacitance &= 10uF
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)

    pins {
        .vin.pos   power   \"VIN\"  \"1\" \"2\"
        .vin.neg   power   \"GND\"  \"3\" \"9\"
        .sw.pos    power   \"SW\"   \"4\" \"5\"
        .fb        input   \"FB\"   \"6\"
        .en        input   \"EN\"   \"7\"
    }

    support {
        .en ~ .vin.pos
    }
}
";

const BUCK_PROVIDERS: &str = "\
import std::parts::regulator::Buck
import src::buck::BuckConverter
import src::zbk2::Zbk2
import src::zbk9::Zbk9

/// Zbk2 with its circuit.
module Zbk2Buck provides BuckConverter {
    .vin_abs_max &= 7V

    .buck: Buck<Zbk2>
    .buck.vin ~ .vin
    .buck.vout ~ .vout
}

/// Zbk9 with its circuit.
module Zbk9Buck provides BuckConverter {
    .vin_abs_max &= 65V

    .buck: Buck<Zbk9>
    .buck.vin ~ .vin
    .buck.vout ~ .vout
}
";

fn buck_world() -> World {
    let mut loader = Loader::std();
    loader.insert("src::buck", "src/buck.nsm", BUCK_GENERIC);
    loader.insert("src::zbk2", "src/zbk2.nsm", ZBK2);
    loader.insert("src::zbk9", "src/zbk9.nsm", ZBK9);
    loader.insert("src::ics", "src/ics.nsm", BUCK_PROVIDERS);
    World {
        loader,
        catalogs: Catalogs::jlcpcb(),
    }
}

fn converter(volts: &str, abs_max: &str) -> String {
    format!(
        "\
import src::buck::BuckConverter
import src::ics::Zbk9Buck    // brings every provider of the file into scope
import std::sources::Rail

board Converter {{
    .input: Rail {{
        .voltage &= {volts}
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }}
    .output: Rail {{
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
        .current &= 0mA..1A
    }}

    .reg: BuckConverter {{
        .vout.voltage &= 3.3V +/- 4%
        .vin_abs_max >= {abs_max}
    }}
    .input ~ .reg.vin
    .reg.vout ~ .output
}}
"
    )
}

#[test]
fn a_buck_converter_picks_the_ic_that_takes_the_input_voltage() {
    let w = buck_world();
    let o =
        build(&w, &converter("12V +/- 5%", "40V")).unwrap_or_else(|e| panic!("{:?}", messages(&e)));
    let b = &o.boards[0];
    assert_eq!(b.providers[0].path, "reg");
    assert_eq!(b.providers[0].generic, "BuckConverter");
    assert_eq!(b.providers[0].chosen, "Zbk9Buck");
    assert_eq!(b.providers[0].passed_over[0].0, "Zbk2Buck");
    assert!(
        b.providers[0].passed_over[0]
            .1
            .contains("ic.max_input_voltage"),
        "{}",
        b.providers[0].passed_over[0].1
    );
    assert!(
        b.notes[0].starts_with("reg is a BuckConverter: picked Zbk9Buck; passed over Zbk2Buck ("),
        "{}",
        b.notes[0]
    );
    // The module's parts are on the board at paths through the use.
    let paths: Vec<&str> = b.design.parts.iter().map(|p| p.path.as_str()).collect();
    assert!(paths.contains(&"reg.buck.ic"), "{paths:?}");
    assert!(
        paths.iter().any(|p| p.starts_with("reg.buck.inductor")),
        "{paths:?}"
    );
}

#[test]
fn the_same_converter_at_five_volts_takes_the_first_ic() {
    let w = buck_world();
    let o =
        build(&w, &converter("5V +/- 5%", "5V")).unwrap_or_else(|e| panic!("{:?}", messages(&e)));
    let b = &o.boards[0];
    assert_eq!(b.providers[0].chosen, "Zbk2Buck");
    assert!(b.providers[0].passed_over.is_empty());
    // A bound that waits on the rail's peak, which nothing states, is named and not a reason to
    // try the other IC.
    assert!(
        b.notes
            .iter()
            .any(|n| n == "not evaluated: reg.vin.peak <= reg.vin_abs_max"),
        "{:?}",
        b.notes
    );
    assert_eq!(providers::trials_run(), 0);
}

#[test]
fn a_buck_converter_no_ic_can_take_names_the_closest() {
    let w = buck_world();
    let e = build(&w, &converter("12V +/- 5%", "70V")).err().unwrap();
    assert_eq!(e.len(), 1, "{:?}", messages(&e));
    let m = &e[0].message;
    assert!(
        m.starts_with("no BuckConverter provider satisfies `reg` at "),
        "{m}"
    );
    assert!(m.contains("none of the 2 in scope works"), "{m}");
    assert!(m.contains("\n  Zbk9Buck (the closest):\n"), "{m}");
    assert!(
        m.contains("\n    reg.vin_abs_max must always be at least 70V, but 65V is not always above 70V"),
        "{m}"
    );
    assert!(m.contains("\n  Zbk2Buck:\n"), "{m}");
}

// ---- a module that takes a part: one candidate for each IC (ADR 0115) ----------------------

const CONVERTER: &str = "\
import std::parts::regulator::{Buck, BuckIc}
import src::buck::BuckConverter

/// Any buck IC with its circuit.
module Converter<Ic: BuckIc> provides BuckConverter {
    .vin_abs_max &= 65V

    .buck: Buck<Ic>
    .buck.vin ~ .vin
    .buck.vout ~ .vout
}
";

fn converter_world() -> World {
    let mut loader = Loader::std();
    loader.insert("src::buck", "src/buck.nsm", BUCK_GENERIC);
    loader.insert("src::zbk2", "src/zbk2.nsm", ZBK2);
    loader.insert("src::zbk9", "src/zbk9.nsm", ZBK9);
    loader.insert("src::converter", "src/converter.nsm", CONVERTER);
    World {
        loader,
        catalogs: Catalogs::jlcpcb(),
    }
}

fn picked_converter(volts: &str) -> String {
    converter(volts, "40V")
        .replace(
            "import src::ics::Zbk9Buck    // brings every provider of the file into scope",
            "import src::converter::Converter\nimport src::zbk2::Zbk2\nimport src::zbk9::Zbk9",
        )
        .replace("board Converter {", "board PowerBoard {")
}

#[test]
fn a_parametric_converter_tries_each_ic_of_its_bound_in_order() {
    let w = converter_world();
    let o =
        build(&w, &picked_converter("12V +/- 5%")).unwrap_or_else(|e| panic!("{:?}", messages(&e)));
    let b = &o.boards[0];
    assert_eq!(b.providers[0].chosen, "Converter<Zbk9>");
    // The standard library's IC first (its catalog has no row here), then the project's.
    let passed: Vec<&str> = b.providers[0]
        .passed_over
        .iter()
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(passed, ["Converter<Lt8641>", "Converter<Zbk2>"]);
    assert_eq!(b.providers[0].candidates, 3);
    assert!(
        b.providers[0].passed_over[1]
            .1
            .contains("reg.buck.ic.max_input_voltage"),
        "{}",
        b.providers[0].passed_over[1].1
    );
    // The IC and its circuit are parts at paths through the use.
    let paths: Vec<&str> = b.design.parts.iter().map(|p| p.path.as_str()).collect();
    assert!(paths.contains(&"reg.buck.ic"), "{paths:?}");
    let ic = b
        .design
        .parts
        .iter()
        .find(|p| p.path == "reg.buck.ic")
        .unwrap();
    assert_eq!(ic.kind, "Zbk9");
}

#[test]
fn a_board_that_names_the_ic_picks_nothing() {
    let w = converter_world();
    let source =
        picked_converter("5V +/- 5%").replace(".reg: BuckConverter {", ".reg: Converter<Zbk2> {");
    let o = build(&w, &source).unwrap_or_else(|e| panic!("{:?}", messages(&e)));
    let b = &o.boards[0];
    assert!(b.providers.is_empty(), "{:?}", b.providers);
    assert_eq!(providers::trials_run(), 0);
}

// ---- what cannot be judged is not what holds -----------------------------------------------

/// A provider that leaves the top frequency open, which the requirement on the use needs.
fn silent(name: &str, catalog: &str) -> String {
    format!(
        "import src::rf::CoaxReceptacle\n\npart {name} provides CoaxReceptacle {{\n    pins {{\n        .signal passive \"SIGNAL\" \"1\"\n        .shield passive \"GND\" \"2\" \"3\"\n    }}\n    .package: Package\n    catalog {catalog} {{ .mpn, .package }}\n}}\n"
    )
}

fn silent_world() -> World {
    let mut w = world(&[ALPHA, BETA]);
    w.loader
        .insert("src::aaa", "src/aaa.nsm", &silent("AaaCoax", "aaacoax"));
    w.catalogs
        .add_file(
            "aaacoax",
            "catalogs/aaacoax.csv",
            "mpn,package,library,source\nAAA-0,PKG_Aaa,Extended,https://example.invalid/aaa-0\n",
        )
        .unwrap();
    w
}

#[test]
fn a_provider_that_cannot_be_judged_is_passed_over_for_one_that_can() {
    let w = silent_world();
    let source = format!(
        "import src::aaa::AaaCoax\n{}\nboard Feed {{\n    .rf: CoaxReceptacle {{ .max_frequency >= 500MHz }}\n}}\n",
        imports(&[ALPHA, BETA])
    );
    let o = build(&w, &source).unwrap();
    let b = &o.boards[0];
    assert_eq!(b.providers[0].chosen, "AlphaCoax");
    let (name, why) = &b.providers[0].passed_over[0];
    assert_eq!(name, "AaaCoax");
    assert_eq!(why, "not evaluated: rf.max_frequency >= 500MHz");
    // And the pick was judged: nothing is left unevaluated.
    assert!(
        b.notes.iter().all(|n| !n.starts_with("not evaluated")),
        "{:?}",
        b.notes
    );
}

#[test]
fn when_no_provider_can_be_judged_the_first_stands_and_the_bound_is_named() {
    let mut w = silent_world();
    w.loader
        .insert("src::bbb", "src/bbb.nsm", &silent("BbbCoax", "bbbcoax"));
    w.catalogs
        .add_file(
            "bbbcoax",
            "catalogs/bbbcoax.csv",
            "mpn,package,library,source\nBBB-0,PKG_Bbb,Extended,https://example.invalid/bbb-0\n",
        )
        .unwrap();
    let source = "import src::rf::CoaxReceptacle\nimport src::aaa::AaaCoax\nimport src::bbb::BbbCoax\nboard Feed {\n    .rf: CoaxReceptacle { .max_frequency >= 500MHz }\n}\n";
    let before = providers::trials_run();
    let o = build(&w, source).unwrap();
    let b = &o.boards[0];
    assert_eq!(b.providers[0].chosen, "AaaCoax");
    assert!(
        b.notes
            .iter()
            .any(|n| n == "not evaluated: rf.max_frequency >= 500MHz"),
        "{:?}",
        b.notes
    );
    // Varying the use changed nothing, so nothing more than that was tried.
    assert!(providers::trials_run() - before <= 1);
}

#[test]
fn a_relation_of_the_generic_holds_for_each_provider_and_decides_the_pick() {
    let mut w = world(&[ALPHA, BETA]);
    let generic = GENERIC.replace(
        "    .mpn: String\n",
        "    .mpn: String\n    .max_frequency >= 2GHz\n",
    );
    w.loader.insert("src::rf", "src/rf.nsm", &generic);
    // The board states nothing: the generic's own bound fails the first provider.
    let o = build(&w, &feed(&[ALPHA, BETA], "    .rf: CoaxReceptacle")).unwrap();
    let b = &o.boards[0];
    assert_eq!(b.providers[0].chosen, "BetaCoax");
    assert_eq!(b.providers[0].passed_over[0].0, "AlphaCoax");
}

#[test]
fn the_providers_own_failure_and_the_generics_both_end_a_trial() {
    // A provider with a check of its own that a board's value breaks: `.max_frequency` of the
    // use must stay under what the provider's own relation allows.
    let mut w = world(&[BETA]);
    let own = "import src::rf::CoaxReceptacle\n\npart Careful provides CoaxReceptacle {\n    pins {\n        .signal passive \"S\" \"1\"\n        .shield passive \"G\" \"2\"\n    }\n    .max_frequency &= 9GHz\n    .impedance <= 40ohm\n}\n";
    w.loader.insert("src::aaa", "src/aaa.nsm", own);
    let source = format!(
        "import src::aaa::Careful\n{}\nboard Feed {{\n    .rf: CoaxReceptacle {{ .max_frequency >= 1GHz }}\n}}\n",
        imports(&[BETA])
    );
    let o = build(&w, &source).unwrap();
    let b = &o.boards[0];
    // Careful fails its own `.impedance <= 40ohm` against the generic's 50 ohm.
    assert_eq!(b.providers[0].chosen, "BetaCoax");
    assert_eq!(b.providers[0].passed_over[0].0, "Careful");
    assert!(
        b.providers[0].passed_over[0]
            .1
            .contains("rf.impedance must always be at most 40Ω"),
        "{}",
        b.providers[0].passed_over[0].1
    );
}

#[test]
fn a_design_that_uses_no_generic_is_solved_once_and_has_no_provider_notes() {
    let w = world(&[]);
    let source = "import std::parts::passive::Resistor\nboard Plain {\n    .r: Resistor { .resistance &= 10kohm +/- 1% }\n}\n";
    let before = providers::trials_run();
    let o = build(&w, source).unwrap();
    assert_eq!(providers::trials_run() - before, 0);
    assert!(o.boards[0].providers.is_empty());
    assert!(o.boards[0].notes.iter().all(|n| !n.contains("picked")));
}

// ---- what cannot be judged: levels, and values inside a provider's parts --------------------

const LINE_DRIVER: &str = "\
import std::logic::Logic
import std::sources::Rail

/// A part that drives one logic line from its supply. Invented.
generic LineDriver {
    .vdd: Rail
    .out: Logic
    pins {
        .vdd.pos   power
        .vdd.neg   power
        .out.line  output
    }
}
";

fn line_driver(name: &str, levels: &str) -> String {
    format!(
        "import src::drv::LineDriver\n\npart {name} provides LineDriver {{\n    .out.reference ~ .vdd\n{levels}    pins {{\n        .vdd.pos   power  \"VDD\" \"1\"\n        .vdd.neg   power  \"GND\" \"2\"\n        .out.line  output \"OUT\" \"3\"\n    }}\n}}\n"
    )
}

const RECEIVER: &str = "\
import std::logic::Logic
import std::sources::Rail

part Receiver {
    .vdd: Rail
    .in: Logic
    .in.reference ~ .vdd
    .in.vih &= 0.7 * .vdd.voltage
    .in.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos   power \"VDD\" \"1\"
        .vdd.neg   power \"GND\" \"2\"
        .in.line   input \"IN\"  \"3\"
    }
}
";

#[test]
fn a_provider_whose_logic_levels_are_open_is_passed_over_for_one_that_states_them() {
    let mut loader = Loader::std();
    loader.insert("src::drv", "src/drv.nsm", LINE_DRIVER);
    loader.insert("src::rx", "src/rx.nsm", RECEIVER);
    // `AaaDriver` sorts first and states no level: the check against the receiver is not made.
    loader.insert("src::aaa", "src/aaa.nsm", &line_driver("AaaDriver", ""));
    loader.insert(
        "src::zeta",
        "src/zeta.nsm",
        &line_driver(
            "ZetaDriver",
            "    .out.voh &= .vdd.voltage - 0.1V\n    .out.vol &= 0.1V\n",
        ),
    );
    let source = "\
import std::sources::Rail
import src::drv::LineDriver
import src::rx::Receiver
import src::aaa::AaaDriver
import src::zeta::ZetaDriver

board Feed {
    .s3: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .drv: LineDriver
    .rx: Receiver
    .s3 ~ .drv.vdd
    .s3 ~ .rx.vdd
    .drv.out.line ~ .rx.in.line
}
";
    let o = outcome_with("main.nsm", source, &loader, &Catalogs::jlcpcb(), None)
        .unwrap_or_else(|e| panic!("{:?}", messages(&e)));
    let b = &o.boards[0];
    assert_eq!(b.providers[0].chosen, "ZetaDriver");
    let (name, why) = &b.providers[0].passed_over[0];
    assert_eq!(name, "AaaDriver");
    assert!(
        why.starts_with("not evaluated: the high level from `drv.out` to `rx.in` on net"),
        "{why}"
    );
    assert!(
        b.notes.iter().all(|n| !n.starts_with("not evaluated")),
        "{:?}",
        b.notes
    );
}

#[test]
fn a_module_provider_whose_inner_part_leaves_a_datasheet_number_open_is_passed_over() {
    // `AaaStage` wraps a core whose input limit is not stated, so the check that the rail is
    // within it waits; `ZetaStage` states it. The open value is inside a part of the module
    // that is the provider, not the design's: a rail the board joins to the use is.
    let mut loader = Loader::std();
    loader.insert(
        "src::stage",
        "src/stage.nsm",
        "import std::sources::Rail\n\ngeneric Stage {\n    .vin: Rail\n}\n",
    );
    loader.insert(
        "src::core",
        "src/core.nsm",
        "/// The IC of a stage. Invented.\npart Core {\n    .max_input: Voltage\n}\n",
    );
    let stage = |name: &str, state: &str| {
        format!(
            "import src::stage::Stage\nimport src::core::Core\n\nmodule {name} provides Stage {{\n    .core: Core\n{state}    .core.max_input >= .vin.voltage\n}}\n"
        )
    };
    loader.insert("src::aaa", "src/aaa.nsm", &stage("AaaStage", ""));
    loader.insert(
        "src::zeta",
        "src/zeta.nsm",
        &stage("ZetaStage", "    .core.max_input &= 12V\n"),
    );
    let source = "\
import std::sources::Rail
import src::stage::Stage
import src::aaa::AaaStage
import src::zeta::ZetaStage

board Feed {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .st: Stage
    .supply ~ .st.vin
}
";
    let o = outcome_with("main.nsm", source, &loader, &Catalogs::jlcpcb(), None)
        .unwrap_or_else(|e| panic!("{:?}", messages(&e)));
    let b = &o.boards[0];
    assert_eq!(b.providers[0].chosen, "ZetaStage");
    let (name, why) = &b.providers[0].passed_over[0];
    assert_eq!(name, "AaaStage");
    assert!(
        why.starts_with("not evaluated: ") && why.contains("st.core.max_input"),
        "{why}"
    );
}
