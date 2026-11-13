//! `PMosfet` and `LevelTranslator` are standard generics with a catalog that has no rows
//! (ADR 0063): a project's own rows, or a provider of its own, serve a design. Every part, row
//! and number here is invented.

use netsmith_pipeline::{Outcome, Problem, outcome_with};
use netsmith_check::Loader;
use netsmith_solve::Catalogs;

const P_MOSFET_CSV: &str = include_str!("../../../data/jlcpcb/catalog/p_mosfet.csv");
const TRANSLATOR_CSV: &str = include_str!("../../../data/jlcpcb/catalog/level_translator.csv");

/// An invented 30 V, 4 A P-channel part, as a project's own row of the standard catalog.
const ZP_ROWS: &str = "\
mpn,name,max_drain_source_voltage,max_drain_current,max_gate_source_voltage,gate_threshold,on_resistance,package,library,source
ZP-30,ZP-30 30V 4A P-MOSFET,30V,4A,12V,0.4V..1.3V,0.07ohm,SOT-23,Extended,https://example.invalid/zp-30
";

/// An invented translator, as a project's own row of the standard catalog.
const ZT_ROWS: &str = "\
mpn,name,vcc_min,vcc_max,vcc_abs_max,a_abs_max,vt_plus_max,vt_minus_min,max_output_current,max_supply_current,package,library,source
ZT-17,ZT-17 translator,2.5V,3.6V,4.2V,4.2V,1.1V,0.45V,3mA,750nA,SOT-23-5,Extended,https://example.invalid/zt-17
";

/// An invented provider of `PMosfet`, with a catalog of its own.
const ZP_PROVIDER: &str = "\
import std::parts::transistor::PMosfet

part ZpPower provides PMosfet {
    pins {
        .gate   input   \"G\" \"1\"
        .source passive \"S\" \"2\"
        .drain  passive \"D\" \"3\"
    }
    catalog zp_power { .max_drain_source_voltage, .max_drain_current, .max_gate_source_voltage, .gate_threshold, .on_resistance, .package }
}
";

const ZP_POWER_CSV: &str = "\
mpn,name,max_drain_source_voltage,max_drain_current,max_gate_source_voltage,gate_threshold,on_resistance,package,library,source
ZP-60,ZP-60 60V 8A P-MOSFET,60V,8A,20V,1V..3V,0.03ohm,SOT-23,Extended,https://example.invalid/zp-60
";

/// An invented provider of `LevelTranslator`.
const ZT_PROVIDER: &str = "\
import std::parts::logic::LevelTranslator

part ZtWide provides LevelTranslator {
    pins {
        .nc      no_connect \"NC\"  \"1\"
        .a.line  input      \"A\"   \"2\"
        .vcc.neg power      \"GND\" \"3\"
        .y.line  output     \"Y\"   \"4\"
        .vcc.pos power      \"VCC\" \"5\"
    }
    catalog zt_wide { .vcc_min, .vcc_max, .vcc_abs_max, .a_abs_max, .vt_plus_max, .vt_minus_min, .max_output_current, .max_supply_current, .package }
}
";

const ZT_WIDE_CSV: &str = "\
mpn,name,vcc_min,vcc_max,vcc_abs_max,a_abs_max,vt_plus_max,vt_minus_min,max_output_current,max_supply_current,package,library,source
ZT-55,ZT-55 wide translator,3V,5.5V,6.5V,6.5V,2.1V,0.7V,6mA,8uA,SOT-23-5,Extended,https://example.invalid/zt-55
";

struct World {
    loader: Loader,
    catalogs: Catalogs,
}

/// The standard library and catalogs, with a project's files: (module, source) of the parts
/// and (catalog name, rows) of the catalog files.
fn world(modules: &[(&str, &str)], files: &[(&str, &str)]) -> World {
    let mut loader = Loader::std();
    for (module, source) in modules {
        loader.insert(
            module,
            &format!("{}.nsm", module.replace("::", "/")),
            source,
        );
    }
    let mut catalogs = Catalogs::jlcpcb();
    for (name, rows) in files {
        catalogs
            .add_file(name, &format!("catalogs/{name}.csv"), rows)
            .unwrap();
    }
    World { loader, catalogs }
}

fn build(w: &World, source: &str) -> Result<Outcome, Vec<Problem>> {
    outcome_with("main.nsm", source, &w.loader, &w.catalogs, None)
}

fn fails(w: &World, source: &str) -> String {
    match build(w, source) {
        Ok(_) => panic!("this should not build"),
        Err(problems) => problems
            .iter()
            .map(|p| p.message.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// A high-side switch: the P-channel part's source on the supply, its drain on the load (a
/// drain on the ground would be a short across the supply, which the DC analysis finds).
fn high_side(imports: &str, volts: &str, gate_drive: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\nimport std::parts::transistor::PMosfet\n{imports}import std::sources::Rail\n\n\
board HighSide {{\n    .supply: Rail {{\n        .voltage &= {volts}\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n\
    .q: PMosfet {{\n        .drain_source_voltage &= supply.voltage\n        .gate_source_voltage &= {gate_drive}\n        .current &= 0A..1A\n    }}\n\
    .load: Resistor {{ .resistance &= 100kohm +/- 1% }}\n\
    .q.gate ~ .supply.neg\n    .q.source ~ .supply.pos\n    .q.drain ~ .load.a\n    .load.b ~ .supply.neg\n}}\n"
    )
}

/// A translator between a 1.8 V part and a 3.3 V rail: the stresses, and what the supply pin
/// draws, are the design's.
fn translated(imports: &str, supply: &str, extra: &str) -> String {
    format!(
        "import std::parts::connector::Header1x2\nimport std::parts::logic::LevelTranslator\n{imports}import std::sources::Rail\n\n\
board Shifter {{\n    .supply: Rail {{\n        .voltage &= {supply}\n        .pos.net &= \"VCC\"\n        .neg.net &= \"GND\"\n    }}\n\
    .u: LevelTranslator {{\n        .vcc_peak &= 3.6V\n        .a_peak &= 1.98V\n        .output_current &= 1mA\n        .vcc.current &= 0A..750nA\n{extra}    }}\n\
    .j: Header1x2\n    .supply ~ .u.vcc\n    .j.p1 ~ .u.a.line\n    .j.p2 ~ .u.y.line\n}}\n"
    )
}

/// The manufacturer's part number the bill of materials has for a part.
fn mpn_of<'a>(b: &'a netsmith_pipeline::Built, refdes: &str) -> Option<&'a str> {
    b.sourcing
        .iter()
        .find(|s| s.refdes == refdes)
        .and_then(|s| s.mpn.as_deref())
}

/// The part number picked for the switch `Q1` (the load's resistor is the first row).
fn q_mpn(b: &netsmith_pipeline::Built) -> Option<&str> {
    b.sourcing
        .iter()
        .find(|s| s.refdes == "Q1")
        .and_then(|s| s.mpn.as_deref())
}

fn only_header(csv: &str) -> bool {
    csv.lines().count() == 1 && csv.starts_with("lcsc,mpn,name,label,")
}

#[test]
fn the_standard_catalogs_of_the_two_have_a_header_and_no_row_and_say_where_rows_go() {
    assert!(only_header(P_MOSFET_CSV), "{P_MOSFET_CSV}");
    assert!(only_header(TRANSLATOR_CSV), "{TRANSLATOR_CSV}");
    // The header is the catalog's columns and the fixed ones, in the order of the n_mosfet file.
    let header = P_MOSFET_CSV.lines().next().unwrap();
    let n_mosfet = include_str!("../../../data/jlcpcb/catalog/n_mosfet.csv");
    assert_eq!(header, n_mosfet.lines().next().unwrap());
    // With no rows and no file of the project, a board is refused where the row is missing.
    let m = fails(&world(&[], &[]), &high_side("", "5V +/- 5%", "5V +/- 5%"));
    assert!(
        m.contains("the `p_mosfet` catalog has nothing for `q`"),
        "{m}"
    );
    let m = fails(&world(&[], &[]), &translated("", "3.3V +/- 5%", ""));
    assert!(
        m.contains("the `level_translator` catalog has nothing for `u`"),
        "{m}"
    );
}

#[test]
fn a_project_row_of_an_invented_p_channel_part_is_picked_for_the_requirement() {
    let w = world(&[], &[("p_mosfet", ZP_ROWS)]);
    let o = build(&w, &high_side("", "5V +/- 5%", "5V +/- 5%")).unwrap_or_else(|e| panic!("{e:?}"));
    let b = &o.boards[0];
    let q = b.design.parts.iter().find(|p| p.path == "q").unwrap();
    assert_eq!((q.kind.as_str(), q.refdes.as_str()), ("PMosfet", "Q1"));
    assert!(q.provides.is_empty(), "the generic itself, no provider");
    assert_eq!(q_mpn(b), Some("ZP-30"));
    assert!(b.providers.is_empty());
}

#[test]
fn a_p_channel_stress_beyond_the_maxima_is_refused_with_the_numbers() {
    let w = world(&[], &[("p_mosfet", ZP_ROWS)]);
    // A 24 V supply puts 22.8 V to 25.2 V between the gate and the source of a part good for 12 V.
    let m = fails(&w, &high_side("", "24V +/- 5%", "24V +/- 5%"));
    assert!(
        m.contains("no `p_mosfet` candidate satisfies `q`")
            && m.contains("ZP-30 30V 4A P-MOSFET")
            && m.contains(
                "q.gate_source_voltage must always be at most q.max_gate_source_voltage, but 22.8V..25.2V is not always below 12V"
            ),
        "{m}"
    );
    // A 40 V supply is beyond the drain-source maximum of 30 V first.
    let m = fails(&w, &high_side("", "40V +/- 5%", "5V"));
    assert!(
        m.contains(
            "q.drain_source_voltage must always be at most q.max_drain_source_voltage, but 38V..42V is not always below 30V"
        ),
        "{m}"
    );
}

#[test]
fn an_invented_provider_of_pmosfet_conforms_and_is_picked_when_the_rows_cannot_serve() {
    let w = world(
        &[("src::zp", ZP_PROVIDER)],
        &[("p_mosfet", ZP_ROWS), ("zp_power", ZP_POWER_CSV)],
    );
    // The standard row (30 V) serves a 12 V design: no provider is tried.
    let imports = "import src::zp::ZpPower\n";
    let o = build(&w, &high_side(imports, "5V +/- 5%", "5V +/- 5%")).unwrap();
    assert!(o.boards[0].providers.is_empty());
    assert_eq!(q_mpn(&o.boards[0]), Some("ZP-30"));
    // A 48 V supply is beyond the row, and the provider's 60 V part serves.
    let o = build(&w, &high_side(imports, "48V +/- 5%", "10V")).unwrap_or_else(|e| panic!("{e:?}"));
    let b = &o.boards[0];
    let note = &b.providers[0];
    assert_eq!(
        (
            note.path.as_str(),
            note.generic.as_str(),
            note.chosen.as_str()
        ),
        ("q", "PMosfet", "ZpPower")
    );
    let q = b.design.parts.iter().find(|p| p.path == "q").unwrap();
    assert_eq!(
        (q.kind.as_str(), q.provides.clone()),
        ("ZpPower", vec!["PMosfet".to_string()])
    );
    assert_eq!(q_mpn(b), Some("ZP-60"));
}

#[test]
fn a_provider_of_pmosfet_alone_serves_when_the_standard_catalog_has_no_row() {
    // No project row of the standard catalog: the generic has nothing, the provider is tried.
    let w = world(&[("src::zp", ZP_PROVIDER)], &[("zp_power", ZP_POWER_CSV)]);
    let imports = "import src::zp::ZpPower\n";
    let o = build(&w, &high_side(imports, "12V +/- 5%", "10V")).unwrap_or_else(|e| panic!("{e:?}"));
    let b = &o.boards[0];
    assert_eq!(b.providers[0].chosen, "ZpPower");
    assert_eq!(b.providers[0].passed_over[0].0, "PMosfet");
    assert_eq!(q_mpn(b), Some("ZP-60"));
}

#[test]
fn a_project_row_of_an_invented_translator_is_picked_for_the_requirement() {
    let w = world(&[], &[("level_translator", ZT_ROWS)]);
    let o = build(&w, &translated("", "3.3V +/- 5%", "")).unwrap_or_else(|e| panic!("{e:?}"));
    let b = &o.boards[0];
    let u = b.design.parts.iter().find(|p| p.path == "u").unwrap();
    assert_eq!(
        (u.kind.as_str(), u.refdes.as_str()),
        ("LevelTranslator", "U1")
    );
    assert_eq!(u.package.as_deref(), Some("SOT-23-5"));
    assert_eq!(mpn_of(b, "U1"), Some("ZT-17"));
}

#[test]
fn a_translator_supply_outside_its_range_or_a_stress_beyond_its_maxima_is_refused_with_the_numbers()
{
    let w = world(&[], &[("level_translator", ZT_ROWS)]);
    // A 5 V supply is above the 3.6 V the part is good for.
    let m = fails(&w, &translated("", "5V +/- 5%", ""));
    assert!(
        m.contains("no `level_translator` candidate satisfies `u`")
            && m.contains("u.vcc.voltage must always be at most u.vcc_max, but 4.75V..5.25V is not always below 3.6V"),
        "{m}"
    );
    // The input driven to 5 V at its peak is above the 4.6 V of the absolute maximum.
    let source = translated("", "3.3V +/- 5%", "").replace("1.98V", "5V");
    let m = fails(&w, &source);
    assert!(
        m.contains("u.a_peak must always be at most u.a_abs_max, but 5V is not always below 4.2V"),
        "{m}"
    );
    // The supply peak.
    let source = translated("", "3.3V +/- 5%", "").replace("vcc_peak &= 3.6V", "vcc_peak &= 4.3V");
    let m = fails(&w, &source);
    assert!(
        m.contains(
            "u.vcc_peak must always be at most u.vcc_abs_max, but 4.3V is not always below 4.2V"
        ),
        "{m}"
    );
    // The load on the output.
    let source =
        translated("", "3.3V +/- 5%", "").replace("output_current &= 1mA", "output_current &= 6mA");
    let m = fails(&w, &source);
    assert!(
        m.contains("u.output_current must always be at most u.max_output_current, but 6mA is not always below 3mA"),
        "{m}"
    );
    // The draw of the supply pin is the row's own (ADR 0066): a design that states a wider
    // range than the 750 nA is narrowed to it, and one wholly above it is a contradiction.
    let source = translated("", "3.3V +/- 5%", "").replace("0A..750nA", "0A..2uA");
    let o = build(&w, &source).unwrap_or_else(|e| panic!("{e:?}"));
    let draw = o.boards[0].trace.model.domain_at("u.vcc.current").cloned();
    let Some(netsmith_check::Domain::Quantity(q)) = draw else {
        panic!("no draw: {draw:?}")
    };
    assert!((q.lo(), q.hi()) == (0.0, 750e-9), "{q}");
    let source = translated("", "3.3V +/- 5%", "").replace("0A..750nA", "2uA..3uA");
    let m = fails(&w, &source);
    assert!(
        m.contains("contradiction on `u.vcc.current`: it is already 2uA..3uA, which does not overlap 0A..750nA"),
        "{m}"
    );
}

#[test]
fn an_invented_provider_of_the_translator_conforms_and_serves_a_5v_supply() {
    let w = world(
        &[("src::zt", ZT_PROVIDER)],
        &[("level_translator", ZT_ROWS), ("zt_wide", ZT_WIDE_CSV)],
    );
    let imports = "import src::zt::ZtWide\n";
    // A 3.3 V supply is served by the row: no provider is tried.
    let o = build(&w, &translated(imports, "3.3V +/- 5%", "")).unwrap();
    assert!(o.boards[0].providers.is_empty());
    // A 5 V supply is not, and the provider serves it. Its absolute maximum of 6.5 V takes the
    // peaks of the design; the draw of the pin is 750 nA against its 8 uA.
    let source = translated(imports, "5V +/- 5%", "")
        .replace("vcc_peak &= 3.6V", "vcc_peak &= 5.5V")
        .replace("a_peak &= 1.98V", "a_peak &= 5.5V");
    let o = build(&w, &source).unwrap_or_else(|e| panic!("{e:?}"));
    let b = &o.boards[0];
    let note = &b.providers[0];
    assert_eq!(
        (
            note.path.as_str(),
            note.generic.as_str(),
            note.chosen.as_str()
        ),
        ("u", "LevelTranslator", "ZtWide")
    );
    let u = b.design.parts.iter().find(|p| p.path == "u").unwrap();
    assert_eq!(u.provides, ["LevelTranslator"]);
    assert_eq!(mpn_of(b, "U1"), Some("ZT-55"));
}

#[test]
fn a_picked_translator_row_puts_its_own_supply_current_on_the_rail() {
    let w = world(&[], &[("level_translator", ZT_ROWS)]);
    // The design states no draw: the row's 750 nA is the translator's, and the rail counts it: a supply that is
    // allowed less is refused below.
    let source =
        translated("", "3.3V +/- 5%", "").replace("        .vcc.current &= 0A..750nA\n", "");
    assert!(!source.contains("vcc.current"));
    let o = build(&w, &source).unwrap_or_else(|e| panic!("{e:?}"));
    let model = &o.boards[0].trace.model;
    let Some(netsmith_check::Domain::Quantity(q)) = model.domain_at("u.vcc.current") else {
        panic!("the draw has no value")
    };
    assert!((q.lo(), q.hi()) == (0.0, 750e-9), "{q}");
    // A supply that may give 500 nA is refused with the numbers.
    let source = source.replace(
        "        .pos.net &= \"VCC\"\n",
        "        .pos.net &= \"VCC\"\n        .current &= -500nA..0A\n",
    );
    let m = fails(&w, &source);
    assert!(
        m.contains("`supply.current` must stay within -500nA..0A, but the others on it work out to -750nA..0A"),
        "{m}"
    );
}
