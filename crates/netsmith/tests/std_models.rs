//! The standard library's models of circuits (ADR 0025): the feedback divider and the buck
//! converter building blocks, solved with the JLCPCB catalogs. The guide shows the messages;
//! these tests pin the numbers.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Need, Series, Solution, Spec, resolve, solve};

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
    // Asked for 1 mF, not even a bank of eight of the best capacitor keeps that much at 1.8 V.
    let s = run(&converter(
        &ic(".fsw &= 1.2MHz +/- 15%", "2A", "1000uF"),
        "0mA..1A",
        "",
    ));
    let m = messages(&s);
    assert!(
        m.iter().any(|m| m.contains("no `capacitor` candidate satisfies `stage.output_cap`")
            && m.contains("(stage.output_cap.count * stage.output_cap.effective_capacitance) must always be at least stage.buck.min_output_capacitance")
            && m.contains("is not always above 1mF")),
        "{m:#?}"
    );
}

#[test]
fn a_datasheet_that_asks_for_more_than_one_capacitor_keeps_is_met_by_a_bank() {
    // 47 uF effective at 1.8 V, and 40 uF at the 5 V input, where the same row keeps about a
    // third: no single capacitor of the catalog is enough on either side.
    let mut hungry = ic(".fsw &= 1.2MHz +/- 15%", "2A", "47uF");
    hungry = hungry.replace(
        ".min_input_capacitance &= 2uF",
        ".min_input_capacitance &= 40uF",
    );
    let s = run(&converter(&hungry, "0mA..1A", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let n_out = range(&s, "stage.output_count");
    let n_in = range(&s, "stage.input_count");
    assert_eq!(n_out.lo(), n_out.hi(), "{n_out}");
    assert_eq!(n_in.lo(), n_in.hi(), "{n_in}");
    assert!(n_out.lo() >= 2.0 && n_in.lo() >= 2.0, "{n_out} {n_in}");
    // The count of the member and of the capacitor are one number, and it is the fewest.
    assert_eq!(range(&s, "stage.output_cap.count"), n_out);
    assert_eq!(range(&s, "stage.input_cap.count"), n_in);
    let cout = range(&s, "stage.output_cap.effective_capacitance");
    let cin = range(&s, "stage.input_cap.effective_capacitance");
    assert!(n_out.lo() * cout.lo() >= 47e-6, "{n_out} x {cout}");
    assert!((n_out.lo() - 1.0) * cout.lo() < 47e-6, "{n_out} x {cout}");
    assert!(n_in.lo() * cin.lo() >= 40e-6, "{n_in} x {cin}");
    assert!((n_in.lo() - 1.0) * cin.lo() < 40e-6, "{n_in} x {cin}");
    // Every other check is of one capacitor of the bank.
    assert!(range(&s, "stage.output_cap.max_voltage").lo() >= 1.854);
    assert!(range(&s, "stage.input_cap.max_voltage").lo() >= 5.25);
    for side in ["stage.input_cap", "stage.output_cap"] {
        let p = s
            .picks
            .iter()
            .find(|p| s.model.path_of(p.part) == side)
            .unwrap();
        assert_eq!(p.count as f64, range(&s, &format!("{side}.count")).lo());
    }
}

#[test]
fn a_bank_of_one_is_the_single_capacitor_the_stage_picked_before() {
    let s = run(&converter(&guide_ic(), "0mA..1A", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    for side in ["input", "output"] {
        let n = range(&s, &format!("stage.{side}_count"));
        assert_eq!((n.lo(), n.hi()), (1.0, 1.0), "{side}: {n}");
    }
}

#[test]
fn the_board_may_state_the_count_and_the_checks_hold_it() {
    let hungry = ic(".fsw &= 1.2MHz +/- 15%", "2A", "47uF");
    // Three of 47 uF, where two would do: the count is the board's.
    let s = run(&converter(
        &hungry,
        "0mA..1A",
        "    .stage.output_count &= 3\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(range(&s, "stage.output_count").lo(), 3.0);
    assert_eq!(range(&s, "stage.output_cap.count").lo(), 3.0);
    // A range is what the solver may choose from.
    let s = run(&converter(
        &hungry,
        "0mA..1A",
        "    .stage.output_count &= 3..5\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(range(&s, "stage.output_count").lo(), 3.0);
    // One is too few, and the message says what the board stated and what was needed.
    let s = run(&converter(
        &hungry,
        "0mA..1A",
        "    .stage.output_count &= 1\n",
    ));
    let m = messages(&s);
    // One error, the pick's: the `==` that ties the stated count to the capacitor's does not
    // report again for a capacitor that the pick has already refused.
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("no `capacitor` candidate satisfies `stage.output_cap`")
            && m[0].contains("1 x ")
            && m[0].contains("is not always above 47uF"),
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

#[test]
fn the_buck_stage_rates_its_capacitors_for_the_peak_of_their_rail() {
    // A 6 V peak on the 5 V input is within the 6.3 V capacitors the stage picks.
    let s = run(&converter(
        &guide_ic(),
        "0mA..1A",
        "    .input.peak &= 6V\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let v = range(&s, "stage.input_cap.max_voltage");
    assert!(v.lo() >= 6.3 - 1e-9, "{v}");
    // 8 V is over them, and one 10 V capacitor loses too much at 5 V to give the IC its 2 uF,
    // so the stage takes several of them.
    let s = run(&converter(
        &guide_ic(),
        "0mA..1A",
        "    .input.peak &= 8V\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(range(&s, "stage.input_cap.max_voltage").lo() >= 8.0);
    assert!(range(&s, "stage.input_count").lo() >= 2.0);
    // No capacitor of the catalog is rated for 500 V, however many are put together.
    let s = run(&converter(
        &guide_ic(),
        "0mA..1A",
        "    .input.peak &= 500V\n",
    ));
    let m = messages(&s);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("candidate satisfies `stage.input_cap`"),
        "{}",
        m[0]
    );
}

#[test]
fn a_peak_equal_to_a_rails_voltage_range_is_accepted() {
    // A rail that states no overshoot says so with `.peak == .voltage`. The two are one value,
    // so a range of it is not "always at least" itself, and the stage must not ask that.
    let s = run(&converter(
        &guide_ic(),
        "0mA..1A",
        "    .input.peak == .input.voltage\n    .output.peak == .output.voltage\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let v = range(&s, "stage.input_cap.max_voltage");
    assert!(v.lo() >= 5.25 - 1e-9, "{v}");
}

#[test]
fn a_peak_stated_below_the_steady_voltage_cannot_weaken_a_capacitors_rating() {
    // The input runs at 4.75 V to 5.25 V, so a peak of 3 V is a typing mistake. It does not
    // get to let a capacitor through that the steady maximum would refuse: each capacitor is
    // rated for the larger of the two.
    let s = run(&converter(
        &guide_ic(),
        "0mA..1A",
        "    .input.peak &= 3V\n",
    ));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let v = range(&s, "stage.input_cap.max_voltage");
    assert!(v.lo() >= 5.25 - 1e-9, "{v}");
    // With no peak stated the steady voltage is still checked.
    let s = run(&converter(&guide_ic(), "0mA..1A", ""));
    assert!(range(&s, "stage.input_cap.max_voltage").lo() >= 5.25 - 1e-9);
}

#[test]
fn the_buck_stage_states_the_current_its_divider_draws() {
    let s = run(&converter(&guide_ic(), "0mA..1A", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    // (vout - vref) / rtop: microamps, far under the load.
    let i = range(&s, "stage.divider_current");
    assert!(i.lo() > 0.0 && i.hi() < 1e-3, "{i}");
}

/// The invented IC with `limits` added to its `buck` member.
fn ic_with(limits: &str) -> String {
    guide_ic().replace(
        "        .min_input_capacitance",
        &format!("        {limits}\n        .min_input_capacitance"),
    )
}

#[test]
fn an_ic_that_states_no_inductor_limits_checks_none() {
    // Both unset: the relations are not evaluated and the stage builds as before, with the
    // 2.2 uH inductor, which is the only one with the saturation current for this load.
    let s = run(&converter(&guide_ic(), "0mA..1A", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(label(&s, "stage.inductor"), "MHCD322512A-2R2M-A8S");
}

#[test]
fn limits_the_inductor_meets_are_accepted() {
    // The picked inductor saturates at 3.6 A and has 85 mohm.
    let ic = ic_with(".min_inductor_saturation &= 3A\n        .max_inductor_dcr &= 0.1ohm");
    let s = run(&converter(&ic, "0mA..1A", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert_eq!(label(&s, "stage.inductor"), "MHCD322512A-2R2M-A8S");
}

#[test]
fn an_inductor_too_weak_for_the_ic_is_refused_with_the_numbers() {
    // No inductor of the catalog saturates at 4 A, though all of them carry this small load.
    let ic = ic_with(".min_inductor_saturation &= 4A");
    let m = messages(&run(&converter(&ic, "0mA..100mA", "")));
    assert!(
        m.iter().any(
            |m| m.contains("no `inductor` candidate satisfies `stage.inductor`")
                && m.contains("stage.inductor.saturation_current must always be at least stage.buck.min_inductor_saturation")
                && m.contains("is not always above 4A")
        ),
        "{m:#?}"
    );
}

#[test]
fn an_inductor_too_resistive_for_the_ic_is_refused_with_the_numbers() {
    // The least DC resistance in the catalog is 85 mohm, and all of them carry this small load.
    let ic = ic_with(".max_inductor_dcr &= 0.05ohm");
    let m = messages(&run(&converter(&ic, "0mA..100mA", "")));
    assert!(
        m.iter().any(
            |m| m.contains("no `inductor` candidate satisfies `stage.inductor`")
                && m.contains("stage.inductor.dc_resistance must always be at most stage.buck.max_inductor_dcr")
        ),
        "{m:#?}"
    );
}

// ---- a generic inductor in a stage (ADR 0057) ----------------------------------------

/// `run`, with generic parts of the E24 series offered after the catalog's rows.
fn run_generic(src: &str) -> Solution {
    let checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    solve(board, &Catalogs::jlcpcb().with_series(Some(Series::E24)))
}

/// What the generic inductor picked at `path` asks of the row that will stand for it.
fn saturation_need(s: &Solution, path: &str) -> Need {
    let pick = s
        .picks
        .iter()
        .find(|p| s.model.path_of(p.part) == path)
        .unwrap_or_else(|| panic!("`{path}` was not picked"));
    match &pick.generic.as_ref().expect("a generic part").spec {
        Spec::Inductor {
            saturation_current, ..
        } => saturation_current.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_generic_inductor_takes_the_stage_s_load_and_half_the_ripple_as_its_saturation_need() {
    // No row of the catalog saturates at 4 A (see above), so a generic part is offered, and
    // the stage's check `current + ripple / 2 <= saturation_current` asks more of it than the
    // part's own `current <= saturation_current` does: the load is 0.1 A and the ripple adds.
    let ic = ic_with(".min_inductor_saturation &= 4A");
    let s = run_generic(&converter(&ic, "0mA..100mA", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let need = saturation_need(&s, "stage.inductor");
    // The IC's 4 A is the larger demand here: the generic part takes exactly that.
    assert_eq!(need, Need::AtLeast(4.0));
    // The check of the stage is made with it, and holds with nothing to spare.
    let isat = range(&s, "stage.inductor.saturation_current");
    assert!(
        (isat.lo() - 4.0).abs() < 1e-9 && (isat.hi() - 4.0).abs() < 1e-9,
        "{isat}"
    );
}

#[test]
fn the_half_ripple_reaches_the_row_a_generic_inductor_has_to_become() {
    // A load of 3.9 A with 5 V in and 1.8 V out: the load alone is under the IC's 4 A, and the
    // ripple takes the demand over it.
    let ic = ic("", "6A", "10uF").replace(
        "        .max_output_current",
        "        .fsw &= 1.2MHz +/- 15%\n        .max_output_current",
    );
    let s = run_generic(&converter(&ic, "0A..3.9A", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let Need::AtLeast(demand) = saturation_need(&s, "stage.inductor") else {
        panic!("{:?}", saturation_need(&s, "stage.inductor"))
    };
    let ripple = range(&s, "stage.ripple");
    assert!(
        (demand - (3.9 + ripple.hi() / 2.0)).abs() < 1e-6 && demand > 3.9,
        "{demand} for a ripple of {ripple}"
    );
    // The need is a requirement on the row: no row of the catalog meets it, and the step that
    // resolves the part says so by name instead of ordering a part that does not.
    let pick = s
        .picks
        .iter()
        .find(|p| s.model.path_of(p.part) == "stage.inductor")
        .unwrap();
    let generic = pick.generic.as_ref().unwrap();
    let e = resolve(generic, &Catalogs::jlcpcb(), &|_| None).unwrap_err();
    assert!(
        e.starts_with("unresolved: no row of the `inductor` catalog"),
        "{e}"
    );
    assert!(e.contains("saturation current at least 4"), "{e}");
}

#[test]
fn a_generic_inductor_still_has_to_meet_what_it_can_not_be_given() {
    // The IC asks for more inductance than a generic inductor of any value has (1 mH at most):
    // the check refuses, as it does for the rows.
    let ic = ic("", "6A", "10uF")
        .replace(
            "        .max_output_current",
            "        .fsw &= 1.2MHz +/- 15%\n        .max_output_current",
        )
        .replace(".min_inductance &= 1.5uH", ".min_inductance &= 2mH");
    let m = messages(&run_generic(&converter(&ic, "0A..1A", "")));
    assert!(
        m.iter()
            .any(|m| m.contains("no `inductor` candidate satisfies `stage.inductor`")),
        "{m:#?}"
    );
}

/// An IC whose frequency a resistor on RT sets inside a window it offers as a default, and
/// whose least inductance follows the frequency (invented numbers, shaped like a datasheet's).
fn rt_ic() -> String {
    ic("", "2A", "10uF")
        .replace(
            "    .buck.sw.neg ~ .vin.neg\n",
            "    .rt_value: Resistance\n    .rt_value &= 10.7kohm..232kohm\n    \
             .fsw_nominal: Frequency\n    .fsw_nominal == 46.5MHz * 1kohm / (.rt_value + 5.2kohm)\n    \
             .fsw_tolerance: Number\n    .fsw_tolerance &= 0.925..1.075\n    .fsw: Frequency\n    \
             .fsw ?= 0.85MHz..1.25MHz\n    .fsw == .fsw_nominal * .fsw_tolerance\n    \
             .buck.fsw == .fsw\n    \
             .buck.min_inductance == (2 * .buck.sw.voltage - .vin.voltage + 4V) / (2.5 * .fsw * 1A)\n    \
             .buck.sw.neg ~ .vin.neg\n",
        )
        .replace("        .min_inductance &= 1.5uH\n", "")
        .replace(
            "        .en input \"EN\" \"5\"\n",
            "        .en input \"EN\" \"5\"\n        .rt passive \"RT\" \"6\"\n",
        )
}

fn rt_board(rt_window: &str) -> String {
    let src = converter(
        &rt_ic().replace(".fsw ?= 0.85MHz..1.25MHz", rt_window),
        "0mA..1A",
        "    .rt: Resistor\n    .u.rt ~ .rt.a\n    .rt.b ~ .input.neg\n    .u.rt_value == .rt.resistance\n",
    );
    src.replace(
        "import std::parts::regulator::BuckStage\n",
        "import std::parts::regulator::BuckStage\nimport std::parts::passive::Resistor\n",
    )
}

#[test]
fn the_frequency_resistor_the_solver_picks_reaches_the_inductor_s_check() {
    // The window is the IC's default for the frequency. The pick of the resistor is made
    // inside it (ADR 0057), and the least inductance is worked out from the frequency of
    // that resistor, not from the whole window of RT or the default window.
    let s = run(&rt_board(".fsw ?= 0.85MHz..1.25MHz"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let rt = range(&s, "u.rt_value");
    assert!(rt.hi() - rt.lo() < 1e3, "the resistor's own range: {rt}");
    let f = range(&s, "u.fsw");
    assert!(
        f.lo() > 0.85e6 && f.hi() < 1.25e6 && f.hi() / f.lo() < 1.2,
        "{f}"
    );
    let least = range(&s, "u.buck.min_inductance");
    let (vin, vout) = (range(&s, "input.voltage"), range(&s, "stage.vout.voltage"));
    let expect = |f: f64, vin: f64, vout: f64| (2.0 * vout - vin + 4.0) / (2.5 * f);
    let (lo, hi) = (
        expect(f.hi(), vin.hi(), vout.lo()),
        expect(f.lo(), vin.lo(), vout.hi()),
    );
    assert!(
        (least.lo() - lo).abs() < 1e-9 && (least.hi() - hi).abs() < 1e-9,
        "{least} against {lo:e}..{hi:e}"
    );
}

#[test]
fn a_window_no_resistor_can_meet_is_refused_and_names_the_resistor() {
    // A window of 100 kHz around 1 MHz is narrower than the 7.5 % of the frequency and the
    // resistor's 1 % together: no row meets it. The refusal names the resistor and the
    // window, and the build has an error.
    let s = run(&rt_board(".fsw ?= 0.95MHz..1.05MHz"));
    let m = messages(&s);
    assert!(
        m.iter()
            .any(|m| m.contains("`rt`") && m.contains("resistor")),
        "{m:#?}"
    );
    assert!(s.unresolved.iter().any(|u| s.model.path_of(u.part) == "rt"));
}

// ---- I2cPullups states what it draws (ADR 0026, "A part states its own requirements") ----

/// An invented I2C sensor that draws 100 uA to 1 mA, a linear regulator that makes the 3.3 V
/// net from 5 V, a load on that net, the sensor and the pull-ups on it. `rail_current` is a
/// current a board states on the pull-ups' own supply (`""` for none).
fn i2c_board(pullup: &str, rail_current: &str) -> String {
    format!(
        "import std::bus::{{I2c, I2cPullups}}\n\
         import std::parts::regulator::LinearRegulator\n\
         import std::sources::Rail\n\n\
         part Sensor {{\n    .designator &= \"U\"\n    .vcc: Rail\n    .i2c: I2c\n    \
         .vcc.current &= 100uA..1mA\n    pins {{\n        .vcc.pos power \"VCC\" \"1\"\n        \
         .vcc.neg power \"GND\" \"2\"\n        .i2c.sda bidirectional \"SDA\" \"3\"\n        \
         .i2c.scl input \"SCL\" \"4\"\n    }}\n}}\n\n\
         board Bus {{\n    .input: Rail {{\n        .voltage &= 5V +/- 5%\n        \
         .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n    \
         .ldo: LinearRegulator {{ .vout.voltage &= 3.3V +/- 5% }}\n    \
         .load: Rail {{\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n        \
         .current &= 0mA..20mA\n    }}\n    .input ~ .ldo.vin\n    .ldo.vout ~ .load\n    \
         .sensor: Sensor\n    .sensor.vcc ~ .load\n    \
         .pullups: I2cPullups {{ .resistance &= {pullup}\n {rail_current} }}\n    \
         .pullups.supply ~ .load\n    .sensor.i2c ~ .pullups.bus\n}}\n"
    )
}

/// The relations the solver could not evaluate, as text.
fn not_evaluated(s: &Solution) -> Vec<String> {
    s.deferred
        .iter()
        .map(|&i| {
            let r = &s.model.relations()[i];
            format!(
                "{} {}",
                s.model.show_expr(&r.lhs),
                s.model.show_expr(&r.rhs)
            )
        })
        .collect()
}

#[test]
fn i2c_pullups_state_their_supply_current_so_the_net_is_solved() {
    let s = run(&i2c_board("4.7kohm +/- 5%", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    // Before: the pull-ups' current was unknown, the 3.3 V net had two unknowns, and the
    // regulator's checks on its output were not evaluated.
    let open = not_evaluated(&s);
    assert!(open.is_empty(), "not evaluated: {open:#?}");
    // Each pull-up carries 0 up to the supply over its resistance (3.3 V + 5 % over the
    // resistor picked, at the bottom of its tolerance), and the supply draws the two together.
    let hi = 3.3 * 1.05 / range(&s, "pullups.sda_pullup.resistance").lo();
    let (sda, scl) = (
        range(&s, "pullups.sda_pullup.current"),
        range(&s, "pullups.scl_pullup.current"),
    );
    for i in [sda, scl] {
        assert!(
            i.lo().abs() < 1e-12 && (i.hi() - hi).abs() < 1e-6 * hi,
            "{i}"
        );
    }
    let supply = range(&s, "pullups.supply.current");
    assert!(
        supply.lo().abs() < 1e-12 && (supply.hi() - (sda.hi() + scl.hi())).abs() < 1e-12,
        "{supply} against {sda} and {scl}"
    );
    // The regulator supplies the load, the sensor and the pull-ups, so its output is the
    // negative of the sum of the draws: at least the sensor's 100 uA, at most all of them.
    let ldo = range(&s, "ldo.vout.current");
    let all = 20e-3 + 1e-3 + supply.hi();
    assert!(
        ldo.lo() <= -all + 1e-12 && ldo.lo() > -all - 1e-9 && (ldo.hi() + 100e-6).abs() < 1e-12,
        "{ldo} against the draws {all}"
    );
}

#[test]
fn the_pick_of_the_pullup_resistors_does_not_change() {
    let s = run(&i2c_board("4.7kohm +/- 5%", ""));
    assert_eq!(picked(&s, "pullups.sda_pullup"), "4.7k");
    assert_eq!(picked(&s, "pullups.scl_pullup"), "4.7k");
    // The two stay one value, as the module says.
    assert_eq!(
        range(&s, "pullups.sda_pullup.resistance"),
        range(&s, "pullups.scl_pullup.resistance")
    );
}

#[test]
fn a_current_the_board_states_on_the_pullup_supply_is_a_budget() {
    // The two lines draw up to 1.49 mA together: 2 mA holds it, the module's sum is kept.
    let s = run(&i2c_board("4.7kohm +/- 5%", ".supply.current &= 0mA..2mA"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:?}", not_evaluated(&s));
    let supply = range(&s, "pullups.supply.current");
    assert!(supply.hi() < 1.6e-3 && supply.hi() > 1.4e-3, "{supply}");
}

#[test]
fn i2c_pullups_take_the_same_two_offers_for_both_lines() {
    // Nothing stated: 0 up to the supply over the resistance, as always.
    let plain = run(&i2c_board("4.7kohm +/- 5%", ""));
    assert_eq!(range(&plain, "pullups.overdrive").hi(), 0.0);
    assert_eq!(
        range(&plain, "pullups.supply_top"),
        range(&plain, "pullups.supply.voltage")
    );
    // A driver that holds a line 1.7 V above the supply: both lines can push current back.
    let s = run(&i2c_board("4.7kohm +/- 5%", ".overdrive &= 0V..1.7V"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let r_lo = range(&s, "pullups.sda_pullup.resistance").lo();
    for line in ["pullups.sda_pullup.current", "pullups.scl_pullup.current"] {
        let i = range(&s, line);
        assert!((i.lo() + 1.7 / r_lo).abs() < 1e-9, "{line} {i}");
        assert!(i.hi() > 0.0);
    }
    let supply = range(&s, "pullups.supply.current");
    assert!(supply.lo() < 0.0, "{supply}");
}

#[test]
fn a_current_the_board_states_that_the_pullups_cannot_keep_is_a_contradiction() {
    // 3 mA to 4 mA cannot be drawn by two pull-ups of 4.7 kohm.
    let s = run(&i2c_board("4.7kohm +/- 5%", ".supply.current &= 3mA..4mA"));
    let m = messages(&s);
    assert!(!m.is_empty(), "no message for a contradiction");
    assert!(
        m.iter().any(|m| m.contains("pullups.supply.current")),
        "{m:#?}"
    );
}

#[test]
fn a_pullup_of_a_few_ohms_is_picked_for_its_power_now_that_its_current_is_known() {
    // 3.3 V + 5 % over 100 ohm - 1 % is 121 mW: a 0402 or 0603 chip (62.5 mW, 100 mW) cannot
    // carry it, so the power relation of the resistor, which was not evaluated while the
    // current was unknown, now decides the pick.
    let s = run(&i2c_board("100ohm +/- 1%", ""));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(not_evaluated(&s).is_empty(), "{:?}", not_evaluated(&s));
    let worst = 3.3 * 1.05 * 3.3 * 1.05 / (100.0 * 0.99);
    for line in ["sda", "scl"] {
        let power = range(&s, &format!("pullups.{line}_pullup.max_power"));
        assert!(
            power.lo() >= worst,
            "{line}: rated {power}, drawn up to {worst} W"
        );
    }
}
