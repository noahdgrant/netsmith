//! Enum fields in the solver (ADR 0054): a check with `==` judges a variant, and a catalog
//! column of an enum type (the capacitor's `dielectric`, ADR 0055) matches by the variant's
//! name. The optional `dielectric` and `rms_current` columns, and a TVS diode's
//! `breakdown_voltage` (ADR 0073), may be left out of a file.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn solved_with(src: &str, catalogs: &Catalogs) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn text(s: &Solution, path: &str) -> Option<String> {
    match s.model.domain_at(path) {
        Some(Domain::Text(t)) => Some(t.clone()),
        _ => None,
    }
}

// ---- `==` on a variant ----------------------------------------------------------

fn resistor(body: &str) -> String {
    format!(
        "import std::parts::passive::Resistor\nboard B {{\n    .r1: Resistor {{ .resistance &= 1kohm +/- 1% }}\n{body}}}\n"
    )
}

#[test]
fn a_check_that_a_part_is_assembled_the_way_the_design_says_passes() {
    let s = solved_with(
        &resistor(
            "    .r1.assembly &= AssemblyMode::Hand\n    .r1.assembly == AssemblyMode::Hand\n",
        ),
        &Catalogs::jlcpcb(),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
}

#[test]
fn a_check_against_another_variant_fails_naming_both_as_they_are_written() {
    let s = solved_with(
        &resistor(
            "    .r1.assembly &= AssemblyMode::Hand\n    .r1.assembly == AssemblyMode::Fab\n",
        ),
        &Catalogs::jlcpcb(),
    );
    // The part has a catalog, so the pick judges the check first, and the check reports on
    // the part that was left; both say the variants as they are written.
    let m = messages(&s);
    assert!(
        m.iter().any(|m| m.contains(
            "`r1.assembly` is AssemblyMode::Hand, but this check requires AssemblyMode::Fab"
        )),
        "{m:#?}"
    );
}

#[test]
fn a_check_on_a_variant_nobody_stated_is_not_evaluated() {
    let s = solved_with(
        &resistor("    .r1.assembly == AssemblyMode::Fab\n"),
        &Catalogs::jlcpcb(),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
}

#[test]
fn a_module_value_that_nobody_set_ties_nothing() {
    // ADR 0042's tie, now between two `AssemblyMode` values.
    let src = "\
import std::parts::connector::Header1x2
module Block {
    .assembly_mode: AssemblyMode
    .j: Header1x2
    .j.assembly &= .assembly_mode
}
board B { .block: Block }
";
    let s = solved_with(src, &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(text(&s, "block.j.assembly"), None);
}

#[test]
fn a_module_value_that_the_board_set_reaches_the_part() {
    let src = "\
import std::parts::connector::Header1x2
module Block {
    .assembly_mode: AssemblyMode
    .j: Header1x2
    .j.assembly &= .assembly_mode
}
board B {
    .block: Block
    .block.assembly_mode &= AssemblyMode::None
}
";
    let s = solved_with(src, &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(text(&s, "block.j.assembly").as_deref(), Some("None"));
}

// ---- the dielectric column -------------------------------------------------------

const CAPACITOR_HEADER: &str =
    "mpn,capacitance,effective_capacitance,max_voltage,package,dielectric,library,source\n";

/// Two invented 100 nF rows, X7R first.
fn capacitors(second_class: &str) -> Catalogs {
    let text = format!(
        "{CAPACITOR_HEADER}\
ZZC-100N-A,100nF +/- 10%,derate capacitance by bias:,16V,0402,X7R,Basic,https://example.invalid/zzc-a.pdf\n\
ZZC-100N-B,100nF +/- 10%,derate capacitance by bias:,16V,0402,{second_class},Basic,https://example.invalid/zzc-b.pdf\n"
    );
    let mut c = Catalogs::empty();
    c.add_file("capacitor", "catalogs/capacitor.csv", &text)
        .unwrap();
    c
}

fn bypass(state: &str) -> String {
    format!(
        "\
import std::parts::passive::{{Capacitor, Dielectric}}
import std::sources::Rail
board B {{
    .supply: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }}
    .c: Capacitor {{
        .capacitance &= 100nF +/- 10%
{state}    }}
    .supply.pos ~> .c ~> .supply.neg
}}
"
    )
}

#[test]
fn a_design_that_states_no_class_takes_the_first_row() {
    let s = solved_with(&bypass(""), &capacitors("C0G"));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZC-100N-A");
}

#[test]
fn a_design_that_states_a_class_gets_a_row_of_that_class() {
    let s = solved_with(
        &bypass("        .dielectric &= Dielectric::C0G\n"),
        &capacitors("C0G"),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZC-100N-B");
    assert_eq!(text(&s, "c.dielectric").as_deref(), Some("C0G"));
}

#[test]
fn a_class_no_row_has_is_a_requirement_nothing_meets() {
    let s = solved_with(
        &bypass("        .dielectric &= Dielectric::X5R\n"),
        &capacitors("C0G"),
    );
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("the `capacitor` catalog has nothing for `c`") && m[0].contains("dielectric"),
        "{}",
        m[0]
    );
}

#[test]
fn a_dielectric_can_be_compared_with_equals_equals() {
    let s = solved_with(
        &bypass("        .dielectric == Dielectric::X7R\n"),
        &capacitors("C0G"),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZC-100N-A");
}

#[test]
fn a_cell_that_is_not_a_variant_is_refused_naming_the_row_and_the_variants() {
    let s = solved_with(
        &bypass("        .dielectric &= Dielectric::X7R\n"),
        &capacitors("X7r"),
    );
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("`c` cannot be chosen from the `capacitor` catalog")
            && m[0].contains("the `dielectric` of `ZZC-100N-B`")
            && m[0]
                .contains("\"X7r\", which is not a variant of `Dielectric` (`C0G`, `X5R`, `X7R`)"),
        "{}",
        m[0]
    );
}

#[test]
fn a_file_that_leaves_the_column_out_is_read_as_empty_there() {
    let text = "mpn,capacitance,effective_capacitance,max_voltage,package,library,source\n\
ZZC-100N-A,100nF +/- 10%,derate capacitance by bias:,16V,0402,Basic,https://example.invalid/zzc-a.pdf\n";
    let mut c = Catalogs::empty();
    c.add_file("capacitor", "catalogs/capacitor.csv", text)
        .unwrap();
    let s = solved_with(&bypass(""), &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZC-100N-A");
    // And a design that states a class is offered nothing from it.
    let s = solved_with(&bypass("        .dielectric &= Dielectric::X7R\n"), &c);
    assert_eq!(s.diagnostics.len(), 1, "{:?}", messages(&s));
}

#[test]
fn every_standard_capacitor_row_has_a_class_that_the_enum_knows() {
    // A row with a class that `Dielectric` lacks would be refused when a part reads the file.
    let s = solved_with(
        &bypass("        .dielectric &= Dielectric::X7R\n"),
        &Catalogs::jlcpcb(),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(text(&s, "c.dielectric").as_deref(), Some("X7R"));
}

// ---- the heating current of an inductor -------------------------------------------

fn inductor(body: &str) -> String {
    format!(
        "\
import std::parts::passive::Inductor
import std::sources::Rail
board B {{
    .supply: Rail {{
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }}
    .l: Inductor {{
        .inductance &= 2.2uH +/- 20%
{body}    }}
    .supply.pos ~> .l ~> .supply.neg
}}
"
    )
}

#[test]
fn the_standard_inductors_have_no_heating_current_so_a_design_that_needs_one_gets_none() {
    let s = solved_with(&inductor(""), &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    let s = solved_with(
        &inductor("        .rms_current >= 1A\n"),
        &Catalogs::jlcpcb(),
    );
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("the `inductor` catalog has nothing for `l`"),
        "{}",
        m[0]
    );
}

#[test]
fn a_project_row_that_records_a_heating_current_meets_the_requirement() {
    let text = "mpn,inductance,saturation_current,dc_resistance,rms_current,package,library,source\n\
ZZL-2R2-A,2.2uH +/- 20%,3A,0.1ohm,1.5A,1210,Basic,https://example.invalid/zzl-a.pdf\n\
ZZL-2R2-B,2.2uH +/- 20%,3A,0.1ohm,2.5A,1210,Basic,https://example.invalid/zzl-b.pdf\n";
    let mut c = Catalogs::empty();
    c.add_file("inductor", "catalogs/inductor.csv", text)
        .unwrap();
    let s = solved_with(&inductor("        .rms_current >= 2A\n"), &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZL-2R2-B");
}

// ---- the breakdown voltage of a TVS diode (ADR 0073) ---------------------------------

/// A 12 V rail that can reach 16 V, a TVS diode across it, and `body` in the board.
fn tvs(body: &str) -> String {
    format!(
        "import std::parts::diode::Tvs
import std::sources::Rail

board B {{
    .input: Rail {{
        .voltage &= 12V +/- 5%
        .peak &= 16V
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }}
    .tvs: Tvs {{
        .protected_limit &= 36V
    }}
{body}    .input.pos ~> .tvs ~> .input.neg
}}
"
    )
}

const MARGIN: &str = "    .tvs.breakdown_voltage >= .input.peak * 1.1\n";

/// Two project rows of the `tvs` catalog that record a minimum breakdown voltage: 16.0 V
/// peak with 10% margin needs 17.6 V, which the first (17.2 V) lacks and the second has.
const TVS_ROWS: &str = "mpn,standoff_voltage,clamping_voltage,peak_pulse_current,package,breakdown_voltage,library,source
ZZT-17-A,17V,27V,10A,SMA,17.2V,Extended,https://example.invalid/zzt-a.pdf
ZZT-17-B,17V,27V,10A,SMA,18.9V,Extended,https://example.invalid/zzt-b.pdf
";

/// Only the first row: its breakdown voltage is under the margin.
const TVS_ROWS_ONLY_A: &str = "mpn,standoff_voltage,clamping_voltage,peak_pulse_current,package,breakdown_voltage,library,source
ZZT-17-A,17V,27V,10A,SMA,17.2V,Extended,https://example.invalid/zzt-a.pdf
";

/// The same rows from a file written before the column existed.
const TVS_ROWS_WITHOUT_THE_COLUMN: &str =
    "mpn,standoff_voltage,clamping_voltage,peak_pulse_current,package,library,source
ZZT-17-A,17V,27V,10A,SMA,Extended,https://example.invalid/zzt-a.pdf
";

#[test]
fn a_tvs_diode_with_no_breakdown_voltage_stated_is_picked_from_the_standard_rows_as_before() {
    let s = solved_with(&tvs(""), &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks.len(), 1);
    assert!(
        s.picks[0].candidate.starts_with("SMAJ"),
        "{}",
        s.picks[0].candidate
    );
}

#[test]
fn a_breakdown_voltage_margin_is_refused_with_the_standard_rows_that_record_one_or_none() {
    // Nine standard rows record no breakdown voltage and are never offered; the SMDJ36A records one
    // (40 V at least, above the margin) and is refused for its clamp over the protected limit.
    let s = solved_with(&tvs(MARGIN), &Catalogs::jlcpcb());
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("SMDJ36A"), "{}", m[0]);
    assert!(m[0].contains("tvs.clamping_voltage"), "{}", m[0]);
    assert!(m[0].contains("9 for `breakdown_voltage`"), "{}", m[0]);
    assert!(s.picks.is_empty());
}

#[test]
fn a_narrowed_breakdown_voltage_is_refused_with_the_standard_rows_too() {
    let s = solved_with(
        &tvs("    .tvs.breakdown_voltage &= 17.6V..30V\n"),
        &Catalogs::jlcpcb(),
    );
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("the `tvs` catalog has nothing for `tvs`"),
        "{}",
        m[0]
    );
}

#[test]
fn a_project_row_that_records_a_breakdown_voltage_meets_the_margin() {
    let mut c = Catalogs::empty();
    c.add_file("tvs", "catalogs/tvs.csv", TVS_ROWS).unwrap();
    let s = solved_with(&tvs(MARGIN), &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZT-17-B");
}

#[test]
fn a_project_row_with_too_little_breakdown_voltage_fails_the_margin_naming_the_numbers() {
    let mut c = Catalogs::empty();
    c.add_file("tvs", "catalogs/tvs.csv", TVS_ROWS_ONLY_A)
        .unwrap();
    let s = solved_with(&tvs(MARGIN), &c);
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("tvs.breakdown_voltage must always be at least")
            && m[0].contains("17.2V")
            && m[0].contains("17.6V"),
        "{}",
        m[0]
    );
}

#[test]
fn a_tvs_file_written_before_the_column_stays_valid_and_fits_a_design_that_does_not_state_it() {
    let mut c = Catalogs::empty();
    c.add_file("tvs", "catalogs/tvs.csv", TVS_ROWS_WITHOUT_THE_COLUMN)
        .unwrap();
    let s = solved_with(&tvs(""), &c);
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(s.picks[0].candidate, "ZZT-17-A");
    // A design that states the margin gets nothing from it.
    let s = solved_with(&tvs(MARGIN), &c);
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("the `tvs` catalog has nothing for `tvs`"),
        "{}",
        m[0]
    );
}
