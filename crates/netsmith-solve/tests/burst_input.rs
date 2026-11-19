//! A load's burst as the input of the converter that feeds it sees it (issue #1340). The burst
//! is reflected through the converter by its own `dc` statement: a linear regulator draws the
//! same current at its input, a switching regulator the output's power over the least input
//! voltage and efficiency. On the input it is added to the other loads there and held against
//! the input's `.max_peak_current`, and the capacitance across the input holds the rest down to
//! the least voltage a part on the input needs, the converter's own least input among them.
//! The LDO row and the converter are invented (no real part has a recorded source for them,
//! `data/jlcpcb/README.md`); the numbers are worked out by hand in the comments.

use netsmith_check::{Checked, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

const LDO: &str = "mpn,vout.voltage,dropout,max_current,max_power,package,source
ZZL-33,3.3V +/- 2%,0.6V,1A,3W,SOT-223,https://example.invalid/zzl
";

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    c.add_file(
        "capacitor",
        "data/jlcpcb/catalog/capacitor.csv",
        include_str!("../../../data/jlcpcb/catalog/capacitor.csv"),
    )
    .unwrap();
    c.add_file("ldo", "catalogs/ldo.csv", LDO).unwrap();
    c
}

fn solved(src: &str) -> Solution {
    let checked: Checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{:#?}", checked.diagnostics);
    let board = checked.standalone_boards().next().expect("a board");
    solve(board, &catalogs())
}

fn messages(src: &str) -> Vec<String> {
    solved(src)
        .diagnostics
        .into_iter()
        .map(|d| d.message)
        .collect()
}

fn clean(src: &str) {
    let m = messages(src);
    assert!(m.is_empty(), "expected no messages, got {m:#?}");
}

fn one(src: &str, parts: &[&str]) {
    let m = messages(src);
    assert_eq!(m.len(), 1, "expected one message, got {m:#?}");
    for p in parts {
        assert!(m[0].contains(p), "`{p}` is not in `{}`", m[0]);
    }
}

/// A 5 V input that gives at most `limit` at an instant, a 3.3 V LDO, an MCU that draws up to
/// 50 mA and a radio that bursts 500 mA for `burst` behind it, and `extra`.
fn ldo_board(limit: &str, burst: &str, extra: &str) -> String {
    format!(
        "import std::parts::passive::CapacitorBank\n\
         import std::parts::regulator::LinearRegulator\n\
         import std::sources::Rail\n\n\
         board Radio {{\n    \
         .input: Rail {{ .voltage &= 5V +/- 2%; .pos.net &= \"VIN\"; .max_peak_current &= {limit} }}\n    \
         .u: LinearRegulator\n    \
         .input ~ .u.vin\n    \
         .mcu: Rail {{ .current &= 5mA..50mA }}\n    \
         .radio: Rail {{ .current &= 10mA..40mA; .peak_current &= 500mA; .burst &= {burst} }}\n    \
         .u.vout ~ .mcu\n    \
         .u.vout ~ .radio\n\
         {extra}\n}}\n"
    )
}

/// The bulk capacitance, at least `c`, across the input.
fn input_bulk(c: &str) -> String {
    format!(
        "    .bulk: CapacitorBank\n    \
         .bulk.total_effective_capacitance >= {c}\n    \
         .input.pos ~> .bulk ~> .input.neg\n"
    )
}

#[test]
fn a_burst_the_source_in_front_can_give_passes() {
    clean(&ldo_board("1A", "2ms", ""));
}

/// The LDO passes the 550 mA on; a 400 mA input leaves 150 mA for 2 ms, 300 uC, to the input's
/// capacitance. The LDO needs 3.234 V + 0.6 V = 3.834 V at its input, 1.066 V below the input's
/// least 4.9 V: 281.4 uF.
#[test]
fn a_linear_regulator_passes_the_burst_to_its_input() {
    one(
        &ldo_board("400mA", "2ms", ""),
        &[
            "during a burst of `radio` (for 2ms) `u`, which feeds it, draws up to 550mA from its input",
            "which a linear regulator passes on",
            "so the source in front gives up to 550mA",
            "above `u.vin.max_peak_current` (400mA)",
            "no capacitor across the input gives the other 150mA",
            "below the 3.834V that `u.vin` needs",
            "takes 281.4uF of effective capacitance across the input",
        ],
    );
}

/// 150 mA for 500 us is 75 uC: 70.36 uF from 4.9 V down to 3.834 V.
#[test]
fn the_capacitance_across_the_input_holds_the_burst() {
    clean(&ldo_board("400mA", "500us", &input_bulk("80uF")));
    one(
        &ldo_board("400mA", "2ms", &input_bulk("80uF")),
        &[
            "across the input give the other 150mA for 2ms, and the input falls by",
            "from 4.9V",
            "takes 281.4uF",
        ],
    );
}

/// The other loads on the input draw during the burst too.
#[test]
fn the_other_loads_on_the_input_are_added() {
    one(
        &ldo_board(
            "600mA",
            "2ms",
            "    .heater: Rail { .current &= 0mA..100mA }\n    .input ~ .heater\n",
        ),
        &[
            "draws up to 550mA from its input",
            "the other loads on the input draw up to 100mA",
            "so the source in front gives up to 650mA",
            "no capacitor across the input gives the other 50mA",
        ],
    );
}

/// An input that states no limit is not judged, and nothing is said (ADR 0074).
#[test]
fn an_input_with_no_limit_is_not_judged() {
    clean(&ldo_board("400mA", "2ms", "").replace("; .max_peak_current &= 400mA", ""));
}

/// A project's own switching converter, reflected by its `dc switching_regulator`: 1 A at
/// 3.3 V * 1.03 = 3.399 V over 4.9 V and 0.85 is 816.1 mA.
fn switcher_board(limit: &str, burst: &str, extra: &str) -> String {
    format!(
        "import std::parts::passive::CapacitorBank\n\
         import std::sources::Rail\n\n\
         part Step {{\n    \
         .designator &= \"U\"\n    \
         .vin: Rail\n    \
         .vout: Rail\n    \
         .vin.neg ~ .vout.neg\n    \
         .efficiency: Number\n    \
         .efficiency &= 0.85..0.9\n    \
         .vin.voltage >= 4.5V\n    \
         .vout.voltage &= 3.3V +/- 3%\n    \
         .vout.max_peak_current &= 2A\n    \
         .vin.current == -.vout.current * .vout.voltage / (.vin.voltage * .efficiency)\n    \
         dc switching_regulator .vin .vout .efficiency\n    \
         pins \"SOT-23\" {{\n        \
         .vin.pos  power \"IN\"  \"1\"\n        \
         .vin.neg  power \"GND\" \"2\"\n        \
         .vout.pos power \"OUT\" \"3\"\n    \
         }}\n    \
         .package &= \"SOT-23\"\n    \
         .package: Package\n\
         }}\n\n\
         board Radio {{\n    \
         .input: Rail {{ .voltage &= 5V +/- 2%; .pos.net &= \"VIN\"; .max_peak_current &= {limit} }}\n    \
         .u: Step\n    \
         .input ~ .u.vin\n    \
         .radio: Rail {{ .current &= 10mA..40mA; .peak_current &= 1A; .burst &= {burst} }}\n    \
         .u.vout ~ .radio\n\
         {extra}\n}}\n"
    )
}

/// 816.1 mA against 500 mA leaves 316.1 mA for 1 ms, 316.1 uC, held from 4.9 V down to the
/// converter's own least input, 4.5 V: 790.2 uF.
#[test]
fn a_project_s_own_switching_converter_is_reflected_by_its_dc_statement() {
    clean(&switcher_board("900mA", "1ms", ""));
    one(
        &switcher_board("500mA", "1ms", ""),
        &[
            "`u`, which feeds it, draws up to 816.1mA from its input",
            "the 1A its output gives at 3.399V, over the least input 4.9V and the least efficiency 0.85",
            "below the 4.5V that `u.vin` needs",
            "takes 790.2uF of effective capacitance across the input",
        ],
    );
}
