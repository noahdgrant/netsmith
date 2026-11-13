//! Generic capacitors, inductors and LEDs (ADR 0047). A generic ceramic
//! capacitor resolves only to a row with a recorded DC-bias curve (owner decision,
//! 2026-11-03); the ratings of an inductor and an LED are read from the design. All parts
//! and numbers in the rows here are invented.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Need, Series, Solution, Spec, resolve, solve};

fn solved(source: &str, catalogs: &Catalogs) -> Solution {
    let c = check("t.nsm", source, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn never(_: &str) -> Option<u64> {
    None
}

fn catalogs(name: &str, rows: &str) -> Catalogs {
    let mut c = Catalogs::empty().with_series(Some(Series::E24));
    c.add_file(name, "catalogs/rows.csv", rows).unwrap();
    c
}

// ---- capacitors --------------------------------------------------------------------

const CAPS: &str = "lcsc,mpn,name,label,capacitance,effective_capacitance,max_voltage,package,library,source\n\
C900101,ZZC-100N-16,100nF 16V curve,100n,100nF +/- 10%,derate capacitance by bias: 0:1 16:0.5,16V,0402,Basic,https://example.invalid/a\n\
C900102,ZZC-120N-10,120nF 10V curve,120n,120nF +/- 10%,derate capacitance by bias: 0:1 10:0.5,10V,0402,Basic,https://example.invalid/b\n\
C900103,ZZC-120N-50,120nF 50V no curve,120n,120nF +/- 10%,derate capacitance by bias:,50V,0402,Basic,https://example.invalid/c\n\
C900104,ZZC-120N-25,120nF 25V curve,120n,120nF +/- 10%,derate capacitance by bias: 0:1 25:0.4,25V,0402,Extended,https://example.invalid/d\n";

fn cap_board(extra: &str) -> String {
    format!(
        "import std::parts::passive::Capacitor\nboard B {{\n    .c: Capacitor {{\n        .capacitance &= 130nF +/- 10%\n{extra}    }}\n}}\n"
    )
}

#[test]
fn a_value_no_row_has_gives_a_generic_capacitor_with_no_curve_and_the_design_s_voltage() {
    // E24 has 1.3, the rows do not.
    let s = solved(
        &cap_board("        .max_voltage >= 12V\n"),
        &catalogs("capacitor", CAPS),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let pick = &s.picks[0];
    assert!(pick.lcsc.is_none());
    assert_eq!(pick.label, "130n");
    let g = pick.generic.as_ref().expect("a generic part");
    let Spec::Capacitor {
        nominal,
        tolerance_percent,
        max_voltage,
        ..
    } = &g.spec
    else {
        panic!("{g:?}")
    };
    assert!((nominal - 130e-9).abs() < 1e-15);
    assert_eq!(*tolerance_percent, 10.0);
    assert_eq!(*max_voltage, Need::AtLeast(12.0));
    assert_eq!(g.package, "0402");
    assert_eq!(
        g.describe(),
        "130 nF 10% 0402, rated voltage at least 12 V (E24, needs a recorded DC-bias curve)"
    );
}

#[test]
fn a_generic_capacitor_is_not_used_where_the_effective_capacitance_is_asked() {
    // No bias curve, no effective capacitance: the design cannot be satisfied by a generic
    // part, and the message says why.
    let src = "import std::parts::passive::Capacitor\nimport std::sources::Rail\nboard B {\n    .supply: Rail {\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"VCC\"\n        .neg.net &= \"GND\"\n    }\n    .c: Capacitor {\n        .effective_capacitance >= 130nF\n    }\n    .supply.pos ~ .c.a\n    .supply.neg ~ .c.b\n}\n";
    let s = solved(src, &catalogs("capacitor", CAPS));
    assert!(s.picks.is_empty(), "{:?}", s.picks);
    let text = format!("{:?}", s.diagnostics);
    assert!(
        text.contains("generic parts have no recorded bias curve"),
        "{text}"
    );
}

fn generic_cap(voltage: Need) -> netsmith_solve::Generic {
    netsmith_solve::Generic {
        catalog: "capacitor".into(),
        package: "0402".into(),
        spec: Spec::Capacitor {
            series: Series::E24,
            nominal: 120e-9,
            tolerance_percent: 10.0,
            max_voltage: voltage,
        },
    }
}

#[test]
fn a_generic_capacitor_resolves_only_to_a_row_with_a_curve_and_enough_voltage() {
    let c = catalogs("capacitor", CAPS);
    // 120 nF, 10%: three rows have the value. The 50 V one has no curve and is skipped;
    // with no voltage asked the Basic one with a curve wins.
    let got = resolve(&generic_cap(Need::Nothing), &c, &never).unwrap();
    assert_eq!(got.lcsc.as_deref(), Some("C900102"));
    // 12 V asked: the 10 V row is out, the 50 V row has no curve, the 25 V row is left.
    let got = resolve(&generic_cap(Need::AtLeast(12.0)), &c, &never).unwrap();
    assert_eq!(got.lcsc.as_deref(), Some("C900104"));
}

#[test]
fn a_generic_capacitor_whose_only_matching_row_has_no_curve_is_unresolved_by_name() {
    let c = catalogs("capacitor", CAPS);
    let e = resolve(&generic_cap(Need::AtLeast(40.0)), &c, &never).unwrap_err();
    assert!(
        e.starts_with("unresolved: no row of the `capacitor` catalog can be ordered for 120 nF 10% 0402, rated voltage at least 40 V"),
        "{e}"
    );
    assert!(
        e.contains("1 row(s) match but have no recorded DC-bias curve, which a generic ceramic capacitor requires: 120nF 50V no curve"),
        "{e}"
    );
    assert!(e.contains("Nearest in the catalog:"), "{e}");
}

#[test]
fn a_capacitor_row_of_a_wider_tolerance_does_not_stand_for_a_tighter_generic_part() {
    let rows = "lcsc,mpn,name,label,capacitance,effective_capacitance,max_voltage,package,library,source\n\
C900105,ZZC-1,1uF 20%,1u,1uF +/- 20%,derate capacitance by bias: 0:1 10:0.5,10V,0402,Basic,https://example.invalid/e\n";
    let mut g = generic_cap(Need::Nothing);
    if let Spec::Capacitor { nominal, .. } = &mut g.spec {
        *nominal = 1e-6;
    }
    let e = resolve(&g, &catalogs("capacitor", rows), &never).unwrap_err();
    assert!(e.contains("unresolved"), "{e}");
}

// ---- inductors ---------------------------------------------------------------------

const INDUCTORS: &str = "lcsc,mpn,name,label,inductance,saturation_current,dc_resistance,package,library,source\n\
C900201,ZZL-27-A,2.7uH 3A 80m,2.7u,2.7uH +/- 20%,3A,0.08ohm,1210,Extended,https://example.invalid/a\n\
C900202,ZZL-27-B,2.7uH 1A 50m,2.7u,2.7uH +/- 20%,1A,0.05ohm,1210,Extended,https://example.invalid/b\n";

fn inductor_board(extra: &str) -> String {
    format!(
        "import std::parts::passive::Inductor\nboard B {{\n    .l: Inductor {{\n        .inductance &= 3.3uH +/- 20%\n{extra}    }}\n}}\n"
    )
}

#[test]
fn a_generic_inductor_takes_the_current_the_design_gives_as_its_saturation_need() {
    let s = solved(
        &inductor_board("        .current &= 2A\n        .dc_resistance <= 0.1ohm\n"),
        &catalogs("inductor", INDUCTORS),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let g = s.picks[0].generic.as_ref().expect("a generic part");
    let Spec::Inductor {
        saturation_current,
        dc_resistance,
        ..
    } = &g.spec
    else {
        panic!("{g:?}")
    };
    assert_eq!(*saturation_current, Need::AtLeast(2.0));
    assert_eq!(*dc_resistance, Need::AtMost(0.1));
    assert_eq!(s.picks[0].label, "3.3u");
}

#[test]
fn a_rating_nothing_bounds_stays_unknown_and_is_not_guessed() {
    // No current is stated, so there is nothing to hold the saturation current to; the
    // part is picked and the check that mentions it is not evaluated.
    let s = solved(&inductor_board(""), &catalogs("inductor", INDUCTORS));
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let g = s.picks[0].generic.as_ref().unwrap();
    let Spec::Inductor {
        saturation_current,
        dc_resistance,
        ..
    } = &g.spec
    else {
        panic!()
    };
    assert_eq!(
        (saturation_current, dc_resistance),
        (&Need::Nothing, &Need::Nothing)
    );
}

#[test]
fn a_generic_inductor_resolves_to_a_row_with_the_saturation_current_and_resistance() {
    let c = catalogs("inductor", INDUCTORS);
    let g = |isat: Need, dcr: Need| netsmith_solve::Generic {
        catalog: "inductor".into(),
        package: "1210".into(),
        spec: Spec::Inductor {
            series: Series::E24,
            nominal: 2.7e-6,
            tolerance_percent: 20.0,
            saturation_current: isat,
            dc_resistance: dcr,
        },
    };
    // 2 A asked: only the 3 A row.
    let got = resolve(&g(Need::AtLeast(2.0), Need::Nothing), &c, &never).unwrap();
    assert_eq!(got.lcsc.as_deref(), Some("C900201"));
    // 0.5 A and at most 60 mohm: only the second.
    let got = resolve(&g(Need::AtLeast(0.5), Need::AtMost(0.06)), &c, &never).unwrap();
    assert_eq!(got.lcsc.as_deref(), Some("C900202"));
    // 2 A and at most 60 mohm: no row.
    let e = resolve(&g(Need::AtLeast(2.0), Need::AtMost(0.06)), &c, &never).unwrap_err();
    assert!(
        e.starts_with("unresolved: no row of the `inductor` catalog can be ordered for 2.7 uH 20% 1210, saturation current at least 2 A, DC resistance at most 60 mohm"),
        "{e}"
    );
    assert!(e.contains("Nearest in the catalog:"), "{e}");
}

const RANKED: &str = "lcsc,mpn,name,label,inductance,saturation_current,dc_resistance,package,library,source\n\
C900401,ZZ-2R2,ZZ-2R2 2.2uH 3.6A,ZZ-2R2,2.2uH +/- 20%,3.6A,0.085ohm,1210,Extended,https://example.invalid/a\n\
C900402,ZZ-3R3,ZZ-3R3 3.3uH 1.25A,ZZ-3R3,3.3uH +/- 20%,1.25A,0.144ohm,1210,Extended,https://example.invalid/b\n\
C900403,ZZ-1R0,ZZ-1R0 1uH 1A 0402,ZZ-1R0,1uH +/- 20%,1A,0.2ohm,0402,Extended,https://example.invalid/c\n\
C900404,ZZ-4R7,ZZ-4R7 4.7uH 5.3A,ZZ-4R7,4.7uH +/- 20%,5.3A,0.03ohm,5030,Extended,https://example.invalid/d\n";

#[test]
fn the_nearest_rows_are_those_nearest_the_need_that_fails_and_not_the_nearest_in_value() {
    let c = catalogs("inductor", RANKED);
    let generic = netsmith_solve::Generic {
        catalog: "inductor".into(),
        package: "0402".into(),
        spec: Spec::Inductor {
            series: Series::E24,
            nominal: 1e-6,
            tolerance_percent: 30.0,
            saturation_current: Need::AtLeast(6.05),
            dc_resistance: Need::Nothing,
        },
    };
    // No row saturates at 6.05 A. The 1 uH row of the same package is nearest in value and
    // far from the need; the 4.7 uH row at 5.3 A is the one that nearly meets it.
    let e = resolve(&generic, &c, &never).unwrap_err();
    assert!(e.contains("saturation current at least 6.05 A"), "{e}");
    let names: Vec<String> = netsmith_solve::alternatives(&generic, &c)
        .into_iter()
        .map(|a| a.name)
        .collect();
    assert_eq!(
        names,
        [
            "ZZ-4R7 4.7uH 5.3A",
            "ZZ-2R2 2.2uH 3.6A",
            "ZZ-3R3 3.3uH 1.25A"
        ],
        "{e}"
    );
    let nearest = e.split("Nearest in the catalog: ").nth(1).unwrap();
    assert!(nearest.starts_with("ZZ-4R7 4.7uH 5.3A"), "{e}");
}

// ---- LEDs --------------------------------------------------------------------------

const LEDS: &str = "lcsc,mpn,name,label,color,forward_voltage,package,library,source\n\
C900301,ZZ-RED,Red 0603,ZZ-RED,Red,1.8V..2.2V,0603,Basic,https://example.invalid/a\n\
C900302,ZZ-GRN,Green 0603,ZZ-GRN,Green,2.0V..2.4V,0603,Extended,https://example.invalid/b\n";

fn led_board(extra: &str) -> String {
    format!("import std::parts::passive::{{Color, Led}}\nboard B {{\n    .led: Led {{\n{extra}    }}\n}}\n")
}

#[test]
fn a_generic_led_takes_the_colour_and_forward_voltage_the_design_states() {
    // No row has a forward voltage as low as this.
    let s = solved(
        &led_board("        .color &= Color::Red\n        .forward_voltage &= 1.5V..1.7V\n"),
        &catalogs("led", LEDS),
    );
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let g = s.picks[0].generic.as_ref().expect("a generic part");
    let Spec::Led {
        color,
        forward_voltage,
    } = &g.spec
    else {
        panic!("{g:?}")
    };
    assert_eq!(*color, Need::Text("Red".into()));
    assert!(
        matches!(forward_voltage, Need::Within { lo, hi } if (*lo - 1.5).abs() < 1e-9 && (*hi - 1.7).abs() < 1e-9),
        "{forward_voltage:?}"
    );
    assert_eq!(g.describe(), "Red LED 0402, forward voltage 1.5 V..1.7 V");
}

#[test]
fn a_generic_led_is_not_used_when_the_forward_voltage_is_used_but_not_stated() {
    // A colour no row has, with no forward voltage stated: the forward voltage feeds
    // `.drop`, so a generic part that cannot say it is not used, and nothing is guessed.
    let s = solved(
        &led_board("        .color &= Color::Yellow\n"),
        &catalogs("led", LEDS),
    );
    assert!(s.picks.is_empty(), "{:?}", s.picks);
    assert_eq!(s.diagnostics.len(), 1);
}

#[test]
fn a_generic_led_resolves_to_a_row_of_the_colour_inside_the_stated_range() {
    let c = catalogs("led", LEDS);
    let g = |color: Need, vf: Need| netsmith_solve::Generic {
        catalog: "led".into(),
        package: "0603".into(),
        spec: Spec::Led {
            color,
            forward_voltage: vf,
        },
    };
    let got = resolve(
        &g(
            Need::Text("Green".into()),
            Need::Within { lo: 1.9, hi: 2.5 },
        ),
        &c,
        &never,
    )
    .unwrap();
    assert_eq!(got.lcsc.as_deref(), Some("C900302"));
    // A range the green row does not fit inside.
    let e = resolve(
        &g(
            Need::Text("Green".into()),
            Need::Within { lo: 2.2, hi: 2.3 },
        ),
        &c,
        &never,
    )
    .unwrap_err();
    assert!(
        e.starts_with("unresolved: no row of the `led` catalog can be ordered for Green LED 0603, forward voltage 2.2 V..2.3 V."),
        "{e}"
    );
    // The same colour comes first among the rows offered instead.
    assert!(
        e.contains("Nearest in the catalog: Green 0603 (Extended), Red 0603 (Basic)."),
        "{e}"
    );
}

#[test]
fn the_unchanged_catalogs_still_give_rows_first() {
    // A value with a row gets the row, not a generic part.
    let s = solved(
        &cap_board("").replace("130nF", "120nF"),
        &catalogs("capacitor", CAPS),
    );
    assert!(s.picks[0].generic.is_none());
    assert!(s.picks[0].lcsc.is_some());
}

// ---- holding a part to one row ------------------------------------------------------

#[test]
fn a_part_held_to_a_row_is_solved_with_that_row_and_its_stated_value_left_out() {
    // The design says 130 nF +/- 10%; the 120 nF row is not that, so held to it the design
    // still solves (the stated value is what the alternative replaces) and the pick is the
    // row. Without the pin the same catalog gives a generic part.
    let c = catalogs("capacitor", CAPS);
    let board = cap_board("");
    let free = solved(&board, &c);
    assert!(free.picks[0].generic.is_some());
    let pinned = solved(&board, &c.with_pin("c", "120nF 10V curve"));
    assert!(pinned.diagnostics.is_empty(), "{:?}", pinned.diagnostics);
    assert_eq!(pinned.picks[0].lcsc.as_deref(), Some("C900102"));
    assert!(pinned.picks[0].generic.is_none());
}

#[test]
fn a_part_held_to_a_row_that_breaks_a_relation_is_refused_with_the_relation() {
    let c = catalogs("capacitor", CAPS);
    let board = cap_board("        .max_voltage >= 12V\n");
    // The 10 V row cannot carry the 12 V the design demands.
    let pinned = solved(&board, &c.with_pin("c", "120nF 10V curve"));
    assert_eq!(pinned.diagnostics.len(), 1, "{:?}", pinned.diagnostics);
    assert!(
        pinned.diagnostics[0]
            .message
            .contains("c.max_voltage must always be at least"),
        "{}",
        pinned.diagnostics[0].message
    );
}
