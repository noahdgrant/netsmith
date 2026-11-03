//! The standard library's models of circuits (ADR 0025): the feedback divider and the buck
//! converter building blocks, solved with the JLCPCB catalogs. The guide shows the messages;
//! these tests pin the numbers.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

/// Check and solve the only board of `src` with the JLCPCB catalogs.
fn run(src: &str) -> Solution {
    let checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    solve(board, &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

fn picked(s: &Solution, path: &str) -> String {
    s.picks
        .iter()
        .find(|p| s.model.path_of(p.part) == path)
        .unwrap_or_else(|| panic!("`{path}` was not picked"))
        .label
        .clone()
}

/// A divider that makes `vout` from a reference `vref`, on a board that takes its output.
fn divider(vref: &str, vout: &str) -> String {
    format!(
        "import std::parts::regulator::FeedbackDivider\nimport std::sources::Rail\n\n\
         board Adjustable {{\n    .divider: FeedbackDivider {{\n        .vref &= {vref}\n        \
         .vout &= {vout}\n    }}\n    .out: Rail {{\n        .pos.net &= \"VOUT\"\n        \
         .neg.net &= \"GND\"\n    }}\n    .divider.top ~ .out.pos\n    .divider.bottom ~ .out.neg\n    \
         .out.voltage == .divider.vout\n}}\n"
    )
}

#[test]
fn the_divider_picks_resistors_that_keep_the_output_inside_the_wanted_range() {
    // 1.0 V +/- 1% reference, 3.8 V +/- 3% wanted: the resistors' 1% and the reference's 1%
    // together may move the output by 3% at most, so a ratio near 2.8 is needed.
    let s = run(&divider("1.0V +/- 1%", "3.8V +/- 3%"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let out = range(&s, "divider.vout");
    assert!(
        out.lo() >= 3.8 * 0.97 - 1e-9 && out.hi() <= 3.8 * 1.03 + 1e-9,
        "{out}"
    );
    // Worst case, not typical: the reference and both resistors move the output by 2.99%.
    let (top, bottom) = (picked(&s, "divider.rtop"), picked(&s, "divider.rbottom"));
    assert_eq!(
        (top.as_str(), bottom.as_str()),
        ("6.2k", "2.2k"),
        "{top} {bottom}"
    );
}

#[test]
fn the_current_of_the_divider_runs_through_both_resistors_and_is_rated() {
    let s = run(&divider("1.0V +/- 1%", "3.8V +/- 3%"));
    // (3.8 V - 1 V) / 8.4 kohm is about 0.33 mA, with the tolerances 0.43 to 0.48 mA.
    let i = range(&s, "divider.rtop.current");
    assert!(i.lo() > 0.4e-3 && i.hi() < 0.5e-3, "{i}");
    let below = range(&s, "divider.rbottom.current");
    assert!(below.lo() > 0.4e-3 && below.hi() < 0.5e-3, "{below}");
    // And a rating is checked: the 0402 resistors are 62.5 mW and carry well under 1 mW.
    let p = range(&s, "divider.rbottom.max_power");
    assert!(p.lo() > 0.0 && p.hi() < 1.0, "{p}");
}

#[test]
fn an_output_the_tolerances_cannot_hold_is_refused_with_the_numbers() {
    // The reference (1%) and two 1% resistors move the output by about 2.5%, so +/- 2% is out
    // of reach.
    let s = run(&divider("1.0V +/- 1%", "3.8V +/- 2%"));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    for needle in [
        "no combination of candidates satisfies `divider.rbottom` and `divider.rtop` together",
        "`divider.vout` must stay within 3.724V..3.876V",
        "divider.vref * (1 + (divider.rtop.resistance / divider.rbottom.resistance))",
        "works out to",
    ] {
        assert!(m[0].contains(needle), "`{needle}` is not in: {}", m[0]);
    }
}

#[test]
fn an_output_below_the_reference_is_refused_with_the_numbers() {
    // The output of a divider is at least the reference, so 0.9 V from 1.0 V cannot be made.
    let s = run(&divider("1.0V +/- 1%", "0.9V +/- 1%"));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("`divider.vout` must stay within 891mV..909mV")
            && m[0].contains("works out to"),
        "{}",
        m[0]
    );
}

// ---- the buck converter building blocks ---------------------------------------------

/// The invented IC of the guide. `fsw` is `FSW`, `IMAX` its largest output current and
/// `CIN`/`COUT` the capacitance it asks for.
fn ic(fsw: &str, imax: &str, cout: &str) -> String {
    format!(
        "import std::parts::regulator::Buck\nimport std::sources::Rail\n\n\
         part Zbk2 {{\n    .vin: Rail\n    .buck: Buck {{\n        .vref &= 0.8V +/- 1.5%\n        \
         {fsw}\n        .min_input_voltage &= 4.5V\n        .max_input_voltage &= 6V\n        \
         .max_output_current &= {imax}\n        .efficiency &= 0.85..0.95\n        \
         .min_inductance &= 1.5uH\n        .min_input_capacitance &= 2uF\n        \
         .min_output_capacitance &= {cout}\n    }}\n    .buck.sw.neg ~ .vin.neg\n    \
         .vin.current == -.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency)\n    \
         pins {{\n        .vin.pos power \"VIN\" \"1\"\n        .vin.neg power \"GND\" \"2\"\n        \
         .buck.sw.pos power \"SW\" \"3\"\n        .buck.fb input \"FB\" \"4\"\n        \
         .en input \"EN\" \"5\"\n    }}\n}}\n"
    )
}

const STAGE_IMPORTS: &str = "import std::parts::regulator::BuckStage\n";

/// The board of the guide: 5 V in, 1.8 V +/- 3% out, a load of `load`, and `extra`.
fn converter(ic: &str, load: &str, extra: &str) -> String {
    format!(
        "{ic}\n{STAGE_IMPORTS}\nboard Converter {{\n    .input: Rail {{\n        .voltage &= 5V +/- 5%\n        \
         .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n    .output: Rail {{\n        \
         .pos.net &= \"1V8\"\n        .neg.net &= \"GND\"\n        .current &= {load}\n    }}\n    \
         .u: Zbk2\n    .stage: BuckStage {{ .vout.voltage &= 1.8V +/- 3% }}\n    .input ~ .stage.vin\n    \
         .input ~ .u.vin\n    .stage.vout ~ .output\n    .stage.buck ~ .u.buck\n    \
         .u.en ~ .input.pos\n{extra}}}\n"
    )
}

fn guide_ic() -> String {
    ic(".fsw &= 1.2MHz +/- 15%", "2A", "10uF")
}

/// The solver's notes on the part picked at `path`, as its catalog label.
fn label(s: &Solution, path: &str) -> String {
    picked(s, path)
}

#[test]
fn the_buck_stage_picks_an_inductor_that_carries_the_load_and_half_the_ripple() {
    let s = run(&converter(&guide_ic(), "0mA..1A", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    // The 2.2 uH Chilisin: 1.76 uH at its tolerance, still over the IC's 1.5 uH, saturating
    // at 3.6 A.
    assert_eq!(label(&s, "stage.inductor"), "MHCD322512A-2R2M-A8S");
    let l = range(&s, "stage.inductor.inductance");
    assert!(
        (l.lo() - 1.76e-6).abs() < 1e-9 && (l.hi() - 2.64e-6).abs() < 1e-9,
        "{l}"
    );
    // (vin - vout) * vout / (vin * L * fsw): the ripple is largest at the highest input with
    // the least inductance and the lowest frequency.
    let ripple = range(&s, "stage.ripple");
    assert!(ripple.lo() > 0.29 && ripple.hi() < 0.70, "{ripple}");
    // The inductor carries 1 A and half the ripple: under 1.4 A of 3.6 A.
    let i = range(&s, "stage.inductor.current");
    assert!(i.hi() <= 1.0 + 1e-9, "the chain current is the load: {i}");
    assert!(i.hi() + ripple.hi() / 2.0 < 1.4);
}

#[test]
fn the_input_current_follows_from_the_efficiency_and_reaches_the_supply() {
    let s = run(&converter(&guide_ic(), "0mA..1A", ""));
    // 1 A at 1.746 V.. 1.854 V from 4.75 V..5.25 V at 85%..95% efficiency.
    let drawn = range(&s, "u.vin.current");
    assert!(
        drawn.lo() <= 1e-9 && (drawn.hi() - 1.854 / (4.75 * 0.85)).abs() < 1e-3,
        "{drawn}"
    );
    // The supply gives what the IC draws: the stage's own input draws nothing.
    let given = range(&s, "input.current");
    assert!(
        (given.lo() + drawn.hi()).abs() < 1e-9 && given.hi() <= 1e-9,
        "{given}"
    );
    assert!(
        range(&s, "stage.vin.current").hi().abs() < 1e-9,
        "the stage draws no DC current"
    );
}

#[test]
fn the_capacitors_have_the_effective_capacitance_the_ic_asks_for() {
    let s = run(&converter(&guide_ic(), "0mA..1A", ""));
    let cin = range(&s, "stage.input_cap.effective_capacitance");
    let cout = range(&s, "stage.output_cap.effective_capacitance");
    assert!(cin.lo() >= 2e-6, "{cin}");
    assert!(cout.lo() >= 10e-6, "{cout}");
    // Asked for 100 uF, no capacitor of the catalog has that much left at 1.8 V.
    let s = run(&converter(
        &ic(".fsw &= 1.2MHz +/- 15%", "2A", "100uF"),
        "0mA..1A",
        "",
    ));
    let m = messages(&s);
    assert!(
        m.iter().any(|m| m.contains("no `capacitor` candidate satisfies `stage.output_cap`")
            && m.contains("stage.output_cap.effective_capacitance must always be at least stage.buck.min_output_capacitance")
            && m.contains("is not always above 100uF")),
        "{m:#?}"
    );
}

#[test]
fn a_load_over_the_ics_current_or_an_input_over_its_range_is_refused_with_the_numbers() {
    let s = run(&converter(&guide_ic(), "0mA..3A", ""));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("-stage.vout.current must always be at most stage.buck.max_output_current")
            && m[0].contains("0A..3A is not always below 2A"),
        "{}",
        m[0]
    );
    let hot = converter(&guide_ic(), "0mA..1A", "").replace("5V +/- 5%", "12V +/- 5%");
    let m = messages(&run(&hot));
    assert!(
        m.iter().any(|m| m
            .contains("stage.vin.voltage must always be at most stage.buck.max_input_voltage")
            && m.contains("11.4V..12.6V is not always below 6V")),
        "{m:#?}"
    );
}

#[test]
fn an_inductor_that_would_saturate_under_load_plus_half_the_ripple_is_not_picked() {
    // 3.4 A through the 2.2 uH inductor is under its 3.6 A, but 3.4 A and half of 0.67 A
    // ripple is not. A 6 A IC lets the load through; the catalog has no inductor for it.
    let src = converter(&ic(".fsw &= 1.2MHz +/- 15%", "6A", "10uF"), "0A..3.4A", "");
    let m = messages(&run(&src));
    assert!(
        m.iter().any(
            |m| m.contains("no `inductor` candidate satisfies `stage.inductor`")
                && m.contains("stage.inductor.current + (stage.ripple / 2)")
                && m.contains("must always be at most stage.inductor.saturation_current")
        ),
        "{m:#?}"
    );
}

#[test]
fn a_frequency_that_a_resistor_sets_is_one_relation_the_ic_writes() {
    // The IC has no fixed frequency: `rt_value` sets it, and the board states the resistor.
    let rt_ic = ic("", "2A", "10uF")
        .replace(
            "    .buck.sw.neg ~ .vin.neg\n",
            "    .rt_value: Resistance\n    .buck.fsw == 24GHz * 1ohm / .rt_value\n    .buck.sw.neg ~ .vin.neg\n",
        )
        .replace(
            "        .en input \"EN\" \"5\"\n",
            "        .en input \"EN\" \"5\"\n        .rt passive \"RT\" \"6\"\n",
        );
    let extra = "    .rt: Resistor { .resistance &= 20kohm +/- 1% }\n    .u.rt ~ .rt.a\n    \
                 .rt.b ~ .input.neg\n    .u.rt_value == .rt.resistance\n";
    let src = converter(&rt_ic, "0mA..1A", extra).replace(
        "import std::parts::regulator::BuckStage\n",
        "import std::parts::regulator::BuckStage\nimport std::parts::passive::Resistor\n",
    );
    let s = run(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    // 24 GHz ohm / 20 kohm = 1.2 MHz, and the stage works its ripple out with that.
    let f = range(&s, "u.buck.fsw");
    assert!(
        (f.lo() - 24e9 / 20.2e3).abs() < 1e3 && (f.hi() - 24e9 / 19.8e3).abs() < 1e3,
        "{f}"
    );
    assert!(range(&s, "stage.ripple").hi() < 0.7);
}
