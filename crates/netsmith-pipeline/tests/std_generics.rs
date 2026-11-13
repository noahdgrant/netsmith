//! The standard passives, diodes, MOSFET and linear regulator are generics that are their own
//! first provider (ADR 0068 stage 2). Two things are shown here. A design that adds no provider
//! solves exactly as it did when they were parts: the same picks, the same effort. And a project
//! that adds a provider of one has it picked when the standard rows and series cannot serve.
//! Every provider, part number and name is invented.

use netsmith_pipeline::providers;
use netsmith_pipeline::{Outcome, Problem, outcome_with};
use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, solve, solve_system};

const LED: &str = include_str!("../../../examples/led.nsm");
const STATUS: &str = include_str!("../../../examples/status_indicator.nsm");
const LDO: &str = include_str!("../../../examples/linear_regulator.nsm");
const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

const PROTECT: &str = "\
import std::parts::diode::Tvs
import std::parts::passive::Fuse
import std::sources::Rail

board Protected {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .load: Rail {
        .current &= 100mA..400mA
        .pos.net &= \"VFUSED\"
        .neg.net &= \"GND\"
    }
    .fuse: Fuse { .rated_current &= 1A }
    .tvs: Tvs { .protected_limit &= 36V }
    .input.pos ~> .fuse ~> .load.pos
    .input.pos ~> .tvs ~> .input.neg
    .input.neg ~ .load.neg
}
";

const BEHIND: &str = "\
import std::mfg::jlcpcb::TwoLayer
import std::parts::diode::Schottky
import std::parts::passive::Capacitor
import std::sources::Rail

board Behind {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .load: Rail {
        .pos.net &= \"VLOAD\"
        .neg.net &= \"GND\"
        .current &= 0mA..100mA
    }
    .supply.neg ~ .load.neg
    .z_diode: Schottky {}
    .supply.pos ~> .z_diode ~> .load.pos
    .load.voltage == .supply.voltage - .z_diode.drop
    .a_cap: Capacitor { .effective_capacitance >= 1uF }
    .load.pos ~> .a_cap ~> .load.neg
}
";

const CLAMP: &str = "\
import std::parts::diode::{Diode, Zener}
import std::sources::Rail

board Clamp {
    .input: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .z: Zener { .zener_voltage &= 11V..13V }
    .z.bias <= .z.zener_voltage
    .d: Diode { .max_reverse_voltage &= 10V }
    .input.pos ~> .z ~> .input.neg
    .input.neg ~> .d ~> .input.pos
}
";

const SWITCH: &str = "\
import std::parts::passive::Resistor
import std::parts::transistor::NMosfet
import std::sources::Rail

board Switch {
    .supply: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .logic: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .q: NMosfet {
        .drain_source_voltage &= supply.voltage
        .gate_source_voltage &= logic.voltage
    }
    .load: Resistor { .resistance &= 10kohm +/- 1% }
    .q.gate ~ .logic.pos
    .q.source ~ .supply.neg
    .q.drain ~ .load.a
    .load.b ~ .supply.pos
}
";

const SWITCH_WEAK: &str = "\
import std::parts::transistor::NMosfet
import std::sources::Rail

board Switch {
    .supply: Rail {
        .voltage &= 12V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .logic: Rail {
        .voltage &= 1.2V +/- 5%
        .pos.net &= \"1V2\"
        .neg.net &= \"GND\"
    }
    .q: NMosfet {
        .drain_source_voltage &= supply.voltage
        .gate_source_voltage &= logic.voltage
    }
    .q.gate ~ .logic.pos
    .q.source ~ .supply.neg
    .q.drain ~ .supply.pos
}
";

const INDUCTOR: &str = "\
import std::parts::passive::{Crystal, FerriteBead, Inductor}
import std::sources::Rail

board Filter {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .load: Rail {
        .pos.net &= \"OUT\"
        .neg.net &= \"GND\"
        .current &= 1A
    }
    .l: Inductor {
        .inductance &= 2.2uH +/- 30%
        .current &= 1A
    }
    .fb: FerriteBead { .impedance >= 100ohm }
    .xtal: Crystal { .frequency &= 16MHz +/- 30ppm }
    .supply.pos ~> .l ~> .load.pos
    .supply.neg ~ .load.neg
}
";

const SATURATES: &str = "\
import std::parts::passive::Inductor
import std::sources::Rail

board Saturates {
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .load: Rail {
        .pos.net &= \"OUT\"
        .neg.net &= \"GND\"
        .current &= 5A
    }
    .l: Inductor {
        .inductance &= 2.2uH +/- 30%
        .current &= 5A
    }
    .supply.pos ~> .l ~> .load.pos
    .supply.neg ~ .load.neg
}
";

const BANK: &str = "\
import std::parts::passive::Capacitor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Bank {
    .process: TwoLayer { .stackup.thickness &= 1.6mm }
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .bulk: Capacitor { .count &= 1..8 }
    .supply.pos ~> .bulk ~> .supply.neg
    .bulk.count * .bulk.effective_capacitance >= 60uF
    .bulk.max_voltage >= 6V
}
";

const TIGHT: &str = "\
import std::parts::passive::Resistor

board Tight {
    .r1: Resistor { .resistance &= 10kohm +/- 0.1% }
}
";

/// What solving each board of a source costs and gives, as one line each: the board, the
/// number of candidates the search tried (`Solution::trials`), the parts picked (path, row
/// and how many) and the number of findings.
fn effort(name: &str, source: &str) -> Vec<String> {
    let checked = check("main.nsm", source, &Loader::std());
    assert!(checked.is_ok(), "{name}: {}", checked.render());
    let catalogs = Catalogs::jlcpcb();
    let line = |label: &str, s: &netsmith_solve::Solution| {
        let mut picks: Vec<String> = s
            .picks
            .iter()
            .map(|p| format!("{}={}x{}", s.model.path_of(p.part), p.candidate, p.count))
            .collect();
        picks.sort();
        format!(
            "{name}/{label}: trials {} picks [{}] findings {}",
            s.trials,
            picks.join(", "),
            s.diagnostics.len()
        )
    };
    let mut lines: Vec<String> = checked
        .standalone_boards()
        .map(|b| line(&b.name, &solve(b, &catalogs)))
        .collect();
    lines.extend(
        checked
            .systems
            .iter()
            .map(|s| line(&s.name, &solve_system(s, &catalogs))),
    );
    lines
}

fn all_efforts() -> Vec<String> {
    [
        ("led", LED),
        ("status", STATUS),
        ("ldo", LDO),
        ("two_boards", TWO_BOARDS),
        ("protect", PROTECT),
        ("behind", BEHIND),
        ("clamp", CLAMP),
        ("switch", SWITCH),
        ("switch_weak", SWITCH_WEAK),
        ("inductor", INDUCTOR),
        ("saturates", SATURATES),
        ("bank", BANK),
        ("tight", TIGHT),
    ]
    .iter()
    .flat_map(|(name, source)| effort(name, source))
    .collect()
}

/// The lines this printed when the twelve were `part`s (the keyword of `std/parts/*.nsm` was
/// the only difference), recorded before the conversion. `two_boards` was 72 trials until the
/// search left out candidates that a trial would refuse (issue #184): its picks are the same.
const BEFORE: &str = "\
led/LedExample: trials 3 picks [led=KT-0603R RED LED 0603 C2286x1, power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1, r1=330Ω 1% 0402 C25104x1] findings 0\n\
status/IndicatorExample: trials 5 picks [enable_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1, power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1, status_indicator.bypass=100nF 10% 16V X7R 0402 C1525x1, status_indicator.current_limit=1kΩ 1% 0402 C11702x1, status_indicator.led=LTST-C190KGKT GREEN LED 0603 C125094x1] findings 0\n\
ldo/RegulatorExample: trials 62 picks [power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1, power_out=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1, stage.input_cap=10uF 20% 10V X5R 0402 C315248x1, stage.output_cap=22uF 20% 6.3V X5R 0603 C59461x1, stage.regulator=XC6206P332MR-G 3.3V 100mA LDO SOT-23 C5446x1] findings 0\n\
two_boards/TwoBoards: trials 67 picks [led.led=KT-0603R RED LED 0603 C2286x1, led.power_in=PM2.54-1X2PM-H85 2-pin 2.54mm socket C42431841x1, led.r1=100Ω 1% 0402 C25076x1, power.power_in=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1, power.power_out=PH2.54-1X2P-H25 2-pin 2.54mm header C42431798x1, power.stage.input_cap=10uF 20% 10V X5R 0402 C315248x1, power.stage.output_cap=22uF 20% 6.3V X5R 0603 C59461x1, power.stage.regulator=XC6206P332MR-G 3.3V 100mA LDO SOT-23 C5446x1] findings 0\n\
protect/Protected: trials 5 picks [fuse=SMD1206P100TF 1A 6V resettable fuse 1206 C20800x1, tvs=SMAJ15A 15V TVS SMA C113958x1] findings 0\n\
behind/Behind: trials 28 picks [a_cap=10uF 20% 6.3V X5R 0402 C15525x1, z_diode=1N5819WS 40V 1A Schottky SOD-323 C191023x1] findings 0\n\
clamp/Clamp: trials 1 picks [] findings 3\n\
switch/Switch: trials 3 picks [load=10kΩ 1% 0603 C25804x1, q=AO3400A 30V 5.7A N-MOSFET SOT-23 C20917x1] findings 0\n\
switch_weak/Switch: trials 2 picks [] findings 1\n\
inductor/Filter: trials 4 picks [fb=BLM18PG221SN1D 220R 1.4A ferrite bead 0603 C80165x1, l=MHCD322512A-2R2M-A8S 2.2uH 3.6A inductor 1210 C285758x1, xtal=X503216MSB2GI 16MHz 20pF crystal 5032 C112572x1] findings 0\n\
saturates/Saturates: trials 1 picks [] findings 1\n\
bank/Bank: trials 199 picks [bulk=47uF 20% 6.3V X5R 0805 C16780x6] findings 0\n\
tight/Tight: trials 0 picks [] findings 1\n\
";

#[test]
fn the_picks_and_the_effort_of_the_standard_kinds_are_what_they_were_as_parts() {
    let now = all_efforts().join("\n");
    if std::env::var("STD_GENERICS_PRINT").is_ok() {
        println!("{now}");
    }
    assert_eq!(now.trim(), BEFORE.trim());
}

// ---- a project's provider of a standard generic ------------------------------------------

const THIN_FILM: &str = "\
import std::parts::passive::Resistor

/// An invented 0.1 % thin-film resistor.
part PrecisionThinFilm provides Resistor {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    catalog precision_thin_film { .resistance, .max_power, .package }
}
";

const THIN_FILM_CSV: &str = "\
mpn,name,resistance,max_power,package,library,source
PTF-10K,PTF 10k 0.1% 0402,10kohm +/- 0.1%,125mW,0402,Extended,https://example.invalid/ptf-10k
";

const POLY_CAP: &str = "\
import std::parts::passive::Capacitor

/// An invented polymer capacitor.
part PolyCap provides Capacitor {
    pins {
        .a passive \"1\" \"1\"
        .b passive \"2\" \"2\"
    }
    catalog poly_cap { .capacitance, .max_voltage, .package }
}
";

const POLY_CAP_CSV: &str = "\
mpn,name,capacitance,max_voltage,package,library,source
PC-100U,PC 100uF 10V,100uF +/- 20%,10V,CASE_B,Extended,https://example.invalid/pc-100u
";

const BIG_LDO: &str = "\
import std::parts::regulator::LinearRegulator

/// An invented 3.3 V, 1 A regulator.
part BigLdo provides LinearRegulator {
    pins {
        .vin.pos  power \"IN\"  \"3\"
        .vin.neg  power \"GND\" \"1\"
        .vout.pos power \"OUT\" \"2\"
    }
    catalog big_ldo { .vout.voltage, .dropout, .max_current, .max_power, .package }
}
";

const BIG_LDO_CSV: &str = "\
mpn,name,vout.voltage,dropout,max_current,max_power,package,library,source
BL-33,BL 3.3V 1A LDO,3.3V +/- 3%,0.5V,1A,2W,SOT-223,Extended,https://example.invalid/bl-33
";

struct World {
    loader: Loader,
    catalogs: Catalogs,
}

/// The standard library and the invented providers (module, source, catalog name and rows).
fn world(files: &[(&str, &str, &str, &str)]) -> World {
    let mut loader = Loader::std();
    let mut catalogs = Catalogs::jlcpcb();
    for (module, source, catalog, rows) in files {
        loader.insert(
            module,
            &format!("{}.nsm", module.replace("::", "/")),
            source,
        );
        catalogs
            .add_file(catalog, &format!("catalogs/{catalog}.csv"), rows)
            .unwrap();
    }
    World { loader, catalogs }
}

fn thin_film() -> World {
    world(&[("src::ptf", THIN_FILM, "precision_thin_film", THIN_FILM_CSV)])
}

fn build(w: &World, source: &str) -> Result<Outcome, Vec<Problem>> {
    outcome_with("main.nsm", source, &w.loader, &w.catalogs, None)
}

fn fails(result: Result<Outcome, Vec<Problem>>) -> Vec<Problem> {
    match result {
        Ok(_) => panic!("this should not build"),
        Err(problems) => problems,
    }
}

fn board(imports: &str, body: &str) -> String {
    format!("import std::parts::passive::Resistor\n{imports}\nboard Divider {{\n{body}\n}}\n")
}

#[test]
fn a_provider_of_resistor_is_picked_when_no_row_or_series_value_serves() {
    let w = thin_film();
    let source = board(
        "import src::ptf::PrecisionThinFilm",
        "    .r1: Resistor { .resistance &= 10kohm +/- 0.1% }",
    );
    let before = providers::trials_run();
    let o = build(&w, &source).unwrap_or_else(|e| panic!("{e:?}"));
    let b = &o.boards[0];
    assert_eq!(providers::trials_run() - before, 1, "one more trial");
    assert_eq!(b.providers.len(), 1);
    let note = &b.providers[0];
    assert_eq!(
        (
            note.path.as_str(),
            note.generic.as_str(),
            note.chosen.as_str()
        ),
        ("r1", "Resistor", "PrecisionThinFilm")
    );
    assert_eq!(note.passed_over.len(), 1);
    assert_eq!(note.passed_over[0].0, "Resistor");
    assert!(
        b.notes
            .iter()
            .any(|n| n
                .starts_with("r1 is a Resistor: picked PrecisionThinFilm; passed over Resistor (")),
        "{:?}",
        b.notes
    );
    // What the design stage makes of it: the provider's name as the kind, the generic it provides, the
    // letters the generic states, the row of the provider's own catalog.
    let part = &b.design.parts[0];
    assert_eq!(part.kind, "PrecisionThinFilm");
    assert_eq!(part.provides, ["Resistor"]);
    assert_eq!(part.refdes, "R1");
    assert_eq!(part.path, "r1");
    assert_eq!(part.package.as_deref(), Some("0402"));
    assert_eq!(b.sourcing[0].mpn.as_deref(), Some("PTF-10K"));
}

#[test]
fn a_provider_in_scope_changes_nothing_while_the_standard_rows_serve() {
    let w = thin_film();
    let with = board(
        "import src::ptf::PrecisionThinFilm",
        "    .r1: Resistor { .resistance &= 10kohm +/- 1% }",
    );
    let without = board("", "    .r1: Resistor { .resistance &= 10kohm +/- 1% }");
    let before = providers::trials_run();
    let a = build(&w, &with).unwrap();
    assert_eq!(
        providers::trials_run() - before,
        0,
        "the first solve is enough"
    );
    let b = build(&w, &without).unwrap();
    assert!(a.boards[0].providers.is_empty());
    assert!(a.boards[0].notes.iter().all(|n| !n.contains("picked")));
    assert_eq!(a.boards[0].design.parts, b.boards[0].design.parts);
    assert_eq!(a.boards[0].notes, b.boards[0].notes);
    assert_eq!(a.boards[0].design.parts[0].kind, "Resistor");
    assert!(a.boards[0].design.parts[0].provides.is_empty());
}

#[test]
fn resistors_whose_current_nothing_states_cost_no_trial_for_a_provider_in_scope() {
    // A resistor with no stated current leaves its rating check not evaluated, whichever
    // provider it is. Trying each other provider for each such part would cost a whole solve
    // apiece (37 seconds against half a second for 30 of them, measured), and the answer
    // would be the same, so a part that is its own generic's first provider is not tried.
    let w = thin_film();
    let body: String = (0..30)
        .map(|i| format!("    .pull{i}: Resistor {{ .resistance &= 10kohm +/- 1% }}\n"))
        .collect();
    let source = board("import src::ptf::PrecisionThinFilm", &body);
    let before = providers::trials_run();
    let o = build(&w, &source).unwrap();
    assert_eq!(providers::trials_run() - before, 0);
    assert!(o.boards[0].providers.is_empty());
    assert!(
        o.boards[0]
            .notes
            .iter()
            .any(|n| n.starts_with("not evaluated:")),
        "{:?}",
        o.boards[0].notes
    );
    assert!(
        o.boards[0]
            .design
            .parts
            .iter()
            .all(|p| p.kind == "Resistor")
    );
}

#[test]
fn a_provider_that_is_not_imported_is_not_tried_and_the_failure_is_the_same() {
    let w = thin_film();
    let none = board("", "    .r1: Resistor { .resistance &= 10kohm +/- 0.1% }");
    let plain = world(&[]);
    let a = fails(build(&w, &none));
    let b = fails(build(&plain, &none));
    assert_eq!(
        a.iter().map(|p| &p.message).collect::<Vec<_>>(),
        b.iter().map(|p| &p.message).collect::<Vec<_>>()
    );
    assert!(
        a[0].message
            .contains("the `resistor` catalog has nothing for `r1`"),
        "{}",
        a[0].message
    );
}

#[test]
fn naming_the_provider_as_the_type_picks_nothing() {
    let w = thin_film();
    let source = board(
        "import src::ptf::PrecisionThinFilm",
        "    .r1: PrecisionThinFilm { .resistance &= 10kohm +/- 0.1% }",
    );
    let before = providers::trials_run();
    let o = build(&w, &source).unwrap();
    assert_eq!(providers::trials_run() - before, 0);
    let b = &o.boards[0];
    assert!(b.providers.is_empty());
    assert_eq!(b.design.parts[0].kind, "PrecisionThinFilm");
    assert_eq!(b.design.parts[0].provides, ["Resistor"]);
}

#[test]
fn a_provider_that_cannot_serve_either_ends_in_the_message_of_every_provider() {
    let w = thin_film();
    // 0.05 % is more than the invented part has.
    let source = board(
        "import src::ptf::PrecisionThinFilm",
        "    .r1: Resistor { .resistance &= 10kohm +/- 0.05% }",
    );
    let e = fails(build(&w, &source));
    let m = &e[0].message;
    assert!(
        m.starts_with("no Resistor provider satisfies `r1` at "),
        "{m}"
    );
    assert!(m.contains("Resistor"), "{m}");
    assert!(m.contains("PrecisionThinFilm"), "{m}");
}

#[test]
fn a_provider_of_the_linear_regulator_is_picked_for_the_load_the_standard_row_cannot_carry() {
    let w = world(&[("src::bigldo", BIG_LDO, "big_ldo", BIG_LDO_CSV)]);
    let heavy = LDO.replace("0mA..50mA", "0mA..500mA").replace(
        "import std::sources::Rail\n",
        "import std::sources::Rail\nimport src::bigldo::BigLdo\n",
    );
    assert!(heavy.contains("BigLdo"));
    let o = build(&w, &heavy).unwrap_or_else(|e| panic!("{e:?}"));
    let b = &o.boards[0];
    let note = b
        .providers
        .iter()
        .find(|n| n.generic == "LinearRegulator")
        .expect("the regulator was picked");
    assert_eq!(note.path, "stage.regulator");
    assert_eq!(note.chosen, "BigLdo");
    assert_eq!(note.passed_over[0].0, "LinearRegulator");
    let part = b
        .design
        .parts
        .iter()
        .find(|p| p.path == "stage.regulator")
        .unwrap();
    assert_eq!((part.kind.as_str(), part.refdes.as_str()), ("BigLdo", "U1"));
    assert_eq!(part.provides, ["LinearRegulator"]);

    // The same board at 50 mA is served by the standard regulator and never reaches the provider.
    let light = LDO.replace(
        "import std::sources::Rail\n",
        "import std::sources::Rail\nimport src::bigldo::BigLdo\n",
    );
    let o = build(&w, &light).unwrap();
    assert!(o.boards[0].providers.is_empty());
    let part = o.boards[0]
        .design
        .parts
        .iter()
        .find(|p| p.path == "stage.regulator")
        .unwrap();
    assert_eq!(part.kind, "LinearRegulator");
}

#[test]
fn a_provider_of_the_capacitor_is_a_bank_when_the_design_says_so() {
    let w = world(&[("src::poly", POLY_CAP, "poly_cap", POLY_CAP_CSV)]);
    let source = "\
import std::parts::passive::Capacitor
import src::poly::PolyCap
import std::sources::Rail

board Bulk {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .bulk: PolyCap { .count &= 3..8 }
    .supply.pos ~> .bulk ~> .supply.neg
}
";
    let o = build(&w, source).unwrap_or_else(|e| panic!("{e:?}"));
    let parts = &o.boards[0].design.parts;
    let caps: Vec<(&str, &str)> = parts
        .iter()
        .map(|p| (p.refdes.as_str(), p.path.as_str()))
        .collect();
    assert_eq!(caps, [("C1", "bulk[0]"), ("C2", "bulk[1]"), ("C3", "bulk[2]")]);
    assert!(parts.iter().all(|p| p.kind == "PolyCap"));
    assert!(parts.iter().all(|p| p.provides == ["Capacitor"]));
}
