//! A load that draws in bursts (issue #1279): `.peak_current` for `.burst`, with `.current` its
//! average. The source gives the peak and the other loads' steady current at once, and what it
//! cannot give comes from the capacitance across the rail, down to the least voltage a part on
//! the net needs.

use netsmith_check::{Checked, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    c.add_file(
        "capacitor",
        "data/jlcpcb/catalog/capacitor.csv",
        include_str!("../../../data/jlcpcb/catalog/capacitor.csv"),
    )
    .unwrap();
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

/// A 3.3 V source that gives at most 600 mA at an instant, an MCU that draws up to `mcu`, a radio
/// that bursts 500 mA for `burst`, and `extra`.
fn board(mcu: &str, burst: &str, extra: &str) -> String {
    format!(
        "import std::parts::passive::CapacitorBank\n\
         import std::sources::Rail\n\n\
         board Radio {{\n    \
         .supply: Rail {{ .voltage &= 3.3V +/- 3%; .pos.net &= \"3V3\"; .max_peak_current &= 600mA }}\n    \
         .mcu: Rail {{ .current &= 5mA..{mcu} }}\n    \
         .radio: Rail {{ .current &= 10mA..40mA; .peak_current &= 500mA; .burst &= {burst} }}\n    \
         .supply ~ .mcu\n    \
         .supply ~ .radio\n\
         {extra}\n}}\n"
    )
}

/// The bulk capacitance, at least `c`, across the supply.
fn bulk(c: &str) -> String {
    format!(
        "    .bulk: CapacitorBank\n    \
         .bulk.total_effective_capacitance >= {c}\n    \
         .supply.pos ~> .bulk ~> .supply.neg\n"
    )
}

#[test]
fn a_burst_and_the_other_loads_within_the_source_s_limit_pass() {
    clean(&board("100mA", "3ms", ""));
}

#[test]
fn the_other_loads_are_added_to_the_peak_during_a_burst() {
    one(
        &board("150mA", "3ms", ""),
        &[
            "during a burst of `radio`",
            "500mA for 3ms",
            "the other loads on its net draw up to 150mA",
            "the source gives up to 650mA",
            "`radio.max_peak_current` (600mA)",
            "no part on the net states the least voltage it works at",
            "`.radio.voltage >= ...`",
        ],
    );
}

/// Without `.burst` the peak is an inrush, checked alone as before (issue #750).
#[test]
fn a_peak_with_no_burst_is_checked_alone() {
    let src = board("150mA", "3ms", "").replace("; .burst &= 3ms", "");
    clean(&src);
}

#[test]
fn the_capacitance_across_the_rail_holds_what_the_source_cannot_give() {
    // 50 mA for 200 us is 10 uC; from 3.201 V down to 3 V that takes 49.75 uF.
    clean(&board(
        "150mA",
        "200us",
        &format!("    .radio.voltage >= 3V\n{}", bulk("60uF")),
    ));
}

#[test]
fn too_little_capacitance_names_the_fall_and_what_holding_the_burst_takes() {
    // 50 mA for 3 ms is 150 uC, which takes 746.3 uF from 3.201 V down to 3 V.
    one(
        &board(
            "150mA",
            "3ms",
            &format!("    .radio.voltage >= 3V\n{}", bulk("60uF")),
        ),
        &[
            "the source gives up to 650mA",
            "across the rail give the other 50mA for 3ms",
            "from 3.201V",
            "below the 3V that `radio` needs",
            "takes 746.3uF of effective capacitance",
        ],
    );
}

#[test]
fn a_net_with_no_capacitor_says_none_gives_the_rest() {
    one(
        &board("150mA", "3ms", "    .radio.voltage >= 3V\n"),
        &[
            "no capacitor across the rail gives the other 50mA",
            "below the 3V that `radio` needs",
        ],
    );
}

/// The least voltage is the highest that any rail on the net states, the MCU's here.
#[test]
fn the_least_voltage_is_the_highest_a_part_on_the_net_needs() {
    one(
        &board(
            "150mA",
            "200us",
            &format!(
                "    .radio.voltage >= 3V\n    .mcu.voltage >= 3.1V\n{}",
                bulk("60uF")
            ),
        ),
        &["below the 3.1V that `mcu` needs"],
    );
}

/// The duty says how much of the average the bursts are, so the average cannot be less.
#[test]
fn an_average_below_the_bursts_share_is_refused() {
    clean(&board("100mA", "3ms", "    .radio.duty &= 0.05\n"));
    one(
        &board("100mA", "3ms", "    .radio.duty &= 0.1\n"),
        &[
            "max(radio.current) must be at least (max(radio.peak_current) * max(radio.duty))",
            "40mA is below 50mA",
        ],
    );
}

/// `~` joins the supply and the loads, and shares none of the burst's figures: the MCU, joined to
/// the same source, states no burst and has none.
#[test]
fn a_burst_is_the_load_s_own_and_not_shared_by_a_join() {
    let s = solved(&board("100mA", "3ms", "    .radio.duty &= 0.05\n"));
    let model = &s.model;
    for name in ["burst", "duty", "peak_current"] {
        let f = model.field_at(&format!("mcu.{name}")).unwrap();
        assert!(
            !matches!(model.field(f).domain, netsmith_check::Domain::Quantity(_)),
            "mcu.{name} has a value: {:?}",
            model.field(f).domain
        );
    }
}

/// A load inside a module: the loads beyond the module's port draw during its burst too.
#[test]
fn the_loads_beyond_a_module_s_port_are_counted() {
    let src = "import std::sources::Rail\n\n\
        module Radio {\n    \
        .vdd: Rail\n    \
        .tx: Rail { .current &= 10mA..40mA; .peak_current &= 500mA; .burst &= 3ms }\n    \
        .vdd ~ .tx\n\
        }\n\n\
        board Two {\n    \
        .supply: Rail { .voltage &= 3.3V +/- 3%; .pos.net &= \"3V3\"; .max_peak_current &= 600mA }\n    \
        .mcu: Rail { .current &= 5mA..150mA }\n    \
        .radio: Radio\n    \
        .supply ~ .mcu\n    \
        .supply ~ .radio.vdd\n\
        }\n";
    one(
        src,
        &[
            "during a burst of `radio.tx`",
            "the other loads on its net draw up to 150mA",
        ],
    );
}
