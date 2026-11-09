//! The standard parts and buses (ADR 0023): their pin tables, their ratings in the worst
//! case, and the roles of the buses. The examples in `docs/LANGUAGE.md` show the messages;
//! these tests pin the numbers and the pins that the export mapping depends on.

use netsmith_check::{Checked, Loader, PinKind, check};
use netsmith_solve::{Catalogs, solve};

/// The catalogs of the parts these tests solve: resistors, capacitors, LEDs and regulators.
/// The kinds that gained catalogs in ADR 0025 (Schottky, Zener, TVS, ferrite bead, inductor,
/// fuse, N-MOSFET, crystal) are left out on purpose, so that a test states a rating by hand
/// and checks the relation that uses it; the rows are tested in `netsmith-solve`, and the
/// guide solves those kinds with their rows.
fn catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    for (name, text) in [
        (
            "resistor",
            include_str!("../../../data/jlcpcb/catalog/resistor.csv"),
        ),
        (
            "capacitor",
            include_str!("../../../data/jlcpcb/catalog/capacitor.csv"),
        ),
        ("led", include_str!("../../../data/jlcpcb/catalog/led.csv")),
        ("ldo", include_str!("../../../data/jlcpcb/catalog/ldo.csv")),
    ] {
        c.add_file(name, &format!("data/jlcpcb/catalog/{name}.csv"), text)
            .unwrap();
    }
    c
}

fn run(src: &str) -> Checked {
    check("test.nsm", src, &Loader::std())
}

/// The messages of the checker, or, if it is clean, of the solver.
fn messages(src: &str) -> Vec<String> {
    let checked = run(src);
    if !checked.is_ok() {
        return checked
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect();
    }
    let catalogs = catalogs();
    checked
        .standalone_boards()
        .flat_map(|b| solve(b, &catalogs).diagnostics)
        .map(|d| d.message)
        .collect()
}

fn clean(src: &str) {
    let m = messages(src);
    assert!(m.is_empty(), "expected no messages, got {m:#?}");
}

/// Exactly one message, and it contains every piece of `parts`.
fn one(src: &str, parts: &[&str]) {
    let m = messages(src);
    assert_eq!(m.len(), 1, "expected one message, got {m:#?}");
    for p in parts {
        assert!(m[0].contains(p), "`{p}` is not in `{}`", m[0]);
    }
}

/// A 12 V rail `supply` on nets `VIN` and `GND`, and `body`.
fn on_12v(imports: &str, body: &str) -> String {
    format!(
        "{imports}\nimport std::sources::Rail\nboard B {{\n    .supply: Rail {{\n        \
         .voltage &= 12V +/- 5%\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n\
         {body}\n}}\n"
    )
}

/// `src` with `extra` added at the end of its last block (the board).
fn with(src: &str, extra: &str) -> String {
    let end = src.rfind("\n}\n").expect("the source ends with a block");
    format!("{}\n{extra}{}", &src[..end], &src[end..])
}

// ---- pin tables ---------------------------------------------------------------------

/// The pins of `kind`, from its pin table: (path, pin kind, datasheet name, pads).
fn pins_of(import: &str, kind: &str) -> Vec<(String, PinKind, String, Vec<String>)> {
    // In a module: a board declares its own mounting holes and fiducials in its physical file.
    let src = format!(
        "import {import}::{kind}\nmodule M {{\n    .x: {kind}\n}}\nboard B {{\n    .m: M\n}}\n"
    );
    let c = run(&src);
    let m = &c.board("B").expect("the board checks").model;
    let x = m.inst_at("m.x").unwrap();
    let mut rows: Vec<_> = m
        .instance(x)
        .tables
        .iter()
        .flat_map(|t| t.rows.iter())
        .map(|r| {
            let path = m.path_of(r.pin);
            (
                path.strip_prefix("m.").unwrap_or(&path).to_string(),
                r.kind,
                r.name.clone(),
                r.pads.clone(),
            )
        })
        .collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
}

fn row(
    path: &str,
    kind: PinKind,
    name: &str,
    pads: &[&str],
) -> (String, PinKind, String, Vec<String>) {
    (
        format!("x.{path}"),
        kind,
        name.to_string(),
        pads.iter().map(|p| p.to_string()).collect(),
    )
}

#[test]
fn a_header_or_socket_has_pin_n_on_pad_n() {
    let cases = [
        ("Header1x2", 2),
        ("Header1x4", 4),
        ("Header1x8", 8),
        ("Header1x10", 10),
        ("Socket1x2", 2),
        ("Socket1x4", 4),
        ("Socket1x8", 8),
        ("Socket1x10", 10),
        ("Header2x3", 6),
        ("Header2x5", 10),
        ("Header2x10", 20),
    ];
    for (kind, n) in cases {
        let rows = pins_of("std::parts::connector", kind);
        assert_eq!(rows.len(), n, "{kind}");
        for i in 1..=n {
            let pin = rows
                .iter()
                .find(|r| r.0 == format!("x.p{i}"))
                .unwrap_or_else(|| panic!("{kind} has no p{i}"));
            assert_eq!(
                (pin.1, pin.3.clone()),
                (PinKind::Passive, vec![i.to_string()])
            );
        }
    }
}

#[test]
fn two_terminal_parts_have_the_pads_the_mapping_relies_on() {
    use PinKind::Passive;
    // Diodes follow the convention of `Led`: cathode 1, anode 2.
    let diode = vec![
        row("anode", Passive, "A", &["2"]),
        row("cathode", Passive, "K", &["1"]),
    ];
    for kind in ["Diode", "Schottky", "Zener", "Tvs"] {
        assert_eq!(pins_of("std::parts::diode", kind), diode, "{kind}");
    }
    let two = vec![
        row("a", Passive, "A", &["1"]),
        row("b", Passive, "B", &["2"]),
    ];
    for kind in ["Inductor", "FerriteBead", "Crystal", "Fuse", "Switch"] {
        assert_eq!(pins_of("std::parts::passive", kind), two, "{kind}");
    }
}

#[test]
fn a_mosfet_has_a_gate_that_must_be_connected() {
    use PinKind::{Input, Passive};
    let fet = vec![
        row("drain", Passive, "D", &["3"]),
        row("gate", Input, "G", &["1"]),
        row("source", Passive, "S", &["2"]),
    ];
    assert_eq!(pins_of("std::parts::transistor", "NMosfet"), fet);
    assert_eq!(pins_of("std::parts::transistor", "PMosfet"), fet);
    let m = messages("import std::parts::transistor::NMosfet\nboard B {\n    .q: NMosfet\n}\n");
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("`q.gate` is an input that must be connected"),
        "{}",
        m[0]
    );
}

#[test]
fn connectors_and_mechanical_parts_have_their_pins() {
    use PinKind::{Bidirectional, Passive, Power};
    assert_eq!(
        pins_of("std::parts::connector", "Coax"),
        vec![
            row("shield", Passive, "SHIELD", &["2"]),
            row("signal", Passive, "SIGNAL", &["1"]),
        ]
    );
    assert_eq!(
        pins_of("std::parts::connector", "ProgrammingPads6").len(),
        6
    );
    let power = pins_of("std::parts::connector", "UsbCPower");
    // The six-contact power-only receptacle (ADR 0021): no A4, B4, A1 or B1.
    assert!(power.contains(&row("vbus.pos", Power, "VBUS", &["A9", "B9"])));
    assert!(power.contains(&row("vbus.neg", Power, "GND", &["A12", "B12"])));
    assert!(power.contains(&row("cc1", Passive, "CC1", &["A5"])));
    assert!(power.contains(&row("cc2", Passive, "CC2", &["B5"])));
    let usb2 = pins_of("std::parts::connector", "UsbC2");
    assert!(usb2.contains(&row(
        "usb.vbus.pos",
        Power,
        "VBUS",
        &["A4", "B4", "A9", "B9"]
    )));
    assert!(usb2.contains(&row(
        "usb.vbus.neg",
        Power,
        "GND",
        &["A1", "B1", "A12", "B12"]
    )));
    assert!(usb2.contains(&row("sbu1", Passive, "SBU1", &["A8"])));
    assert!(usb2.contains(&row("sbu2", Passive, "SBU2", &["B8"])));
    assert!(usb2.contains(&row("usb.dp", Bidirectional, "D+", &["A6", "B6"])));
    assert!(usb2.contains(&row("usb.dm", Bidirectional, "D-", &["A7", "B7"])));
    assert_eq!(usb2.len(), power.len() + 4);

    // The OBD-II plug (SAE J1962): sixteen pads, the supply on 16 and 4, signal ground on 5.
    let obd2 = pins_of("std::parts::connector", "Obd2M");
    assert_eq!(obd2.len(), 16);
    assert!(obd2.contains(&row("vbatt.pos", Power, "BATTPOWER", &["16"])));
    assert!(obd2.contains(&row("vbatt.neg", Power, "CHASGND", &["4"])));
    assert!(obd2.contains(&row("signal_gnd", Power, "SIGGND", &["5"])));
    assert!(obd2.contains(&row("can_high", Bidirectional, "CANHIGH", &["6"])));
    assert!(obd2.contains(&row("can_low", Bidirectional, "CANLOW", &["14"])));
    assert!(obd2.contains(&row("iso_k", Bidirectional, "ISOK-LINE", &["7"])));
    assert!(obd2.contains(&row("iso_l", Bidirectional, "ISOL-LINE", &["15"])));

    assert_eq!(pins_of("std::parts::mechanical", "MountingHole"), vec![]);
    assert_eq!(pins_of("std::parts::mechanical", "Fiducial"), vec![]);
    assert_eq!(
        pins_of("std::parts::mechanical", "PlatedHole"),
        vec![row("p1", Passive, "1", &["1"])]
    );
    assert_eq!(
        pins_of("std::parts::mechanical", "TestPoint"),
        vec![row("p1", Passive, "TP", &["1"])]
    );
}

#[test]
fn the_mechanical_parts_and_the_programming_pads_are_fitted_by_nobody() {
    for (import, kind) in [
        ("std::parts::mechanical", "MountingHole"),
        ("std::parts::mechanical", "PlatedHole"),
        ("std::parts::mechanical", "Fiducial"),
        ("std::parts::mechanical", "TestPoint"),
        ("std::parts::connector", "ProgrammingPads6"),
    ] {
        let src = format!(
            "import {import}::{kind}\nmodule M {{\n    .x: {kind}\n}}\nboard B {{\n    .m: M\n}}\n"
        );
        let c = run(&src);
        let m = &c.board("B").expect("the board checks").model;
        assert_eq!(
            m.domain_at("m.x.assembly"),
            Some(&netsmith_check::Domain::Text("None".into())),
            "{kind}"
        );
    }
    // A kind that is fitted leaves the field to the board and the catalog.
    let src = "import std::parts::connector::Header1x2\nboard B {\n    .x: Header1x2\n}\n";
    let c = run(src);
    let m = &c.board("B").unwrap().model;
    assert_eq!(
        m.domain_at("x.assembly"),
        Some(&netsmith_check::Domain::Unset)
    );
}

#[test]
fn a_test_point_on_no_net_is_refused_and_a_plated_hole_may_float() {
    let m = messages(
        "import std::parts::mechanical::{TestPoint, PlatedHole, MountingHole, Fiducial}\n\
         module Mech {\n    .m: MountingHole\n    .f: Fiducial\n}\n\
         board B {\n    .tp: TestPoint\n    .h: PlatedHole\n    .mech: Mech\n}\n",
    );
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("`tp.p1`") && m[0].contains("must be connected"),
        "{}",
        m[0]
    );
}

// ---- ratings ------------------------------------------------------------------------

#[test]
fn a_diode_across_a_rail_blocks_the_rail_and_a_current_stays_in_its_rating() {
    for kind in ["Diode", "Schottky"] {
        let imports = format!("import std::parts::diode::{kind}");
        let board = |reverse: &str| {
            on_12v(
                &imports,
                &format!(
                    "    .d: {kind} {{ .max_reverse_voltage &= {reverse} }}\n    \
                     .supply.neg ~> .d ~> .supply.pos"
                ),
            )
        };
        clean(&board("30V"));
        one(
            &board("10V"),
            &[
                "d.bias must always be at most d.max_reverse_voltage",
                "11.4V..12.6V",
                "10V",
            ],
        );
        // The forward current is a diode in series with a load: across the rail, wired to
        // conduct, it would be a short, which the next test finds.
        one(
            &on_12v(
                &imports,
                &format!(
                    "    .load: Rail {{ .pos.net &= \"VF\"\n .neg.net &= \"GND\" }}\n    \
                     .d: {kind} {{ .max_forward_current &= 1A\n .current &= 1.5A }}\n    \
                     .supply.pos ~> .d ~> .load.pos\n    .supply.neg ~ .load.neg"
                ),
            ),
            &[
                "d.current must always be at most d.max_forward_current",
                "1.5A",
                "1A",
            ],
        );
    }
}

#[test]
fn a_zener_clamp_needs_the_rail_under_the_breakdown_and_its_power_is_limited() {
    let imports = "import std::parts::diode::Zener";
    // Used as a regulator the part states nothing about the rail across it.
    clean(&on_12v(
        imports,
        "    .z: Zener { .zener_voltage &= 5.1V +/- 5% }\n    .supply.pos ~> .z ~> .supply.neg",
    ));
    // The design's own line: the clamp must not conduct.
    one(
        &on_12v(
            imports,
            "    .z: Zener { .zener_voltage &= 5.1V +/- 5% }\n    \
             .z.bias <= .z.zener_voltage\n    .supply.pos ~> .z ~> .supply.neg",
        ),
        &[
            "z.bias must always be at most z.zener_voltage",
            "11.4V..12.6V",
            "4.845V..5.355V",
        ],
    );
    // Not across a rail, with the stress stated: inside its power.
    clean(&on_12v(
        imports,
        "    .z: Zener { .bias &= 5V\n .current &= 50mA\n .max_power &= 500mW }",
    ));
}

/// A board with `.d` of `kind` across the 12 V rail; `chain` is the chain that wires it.
fn across_12v(kind: &str, fields: &str, chain: &str) -> String {
    on_12v(
        &format!("import std::parts::diode::{kind}"),
        &format!("    .d: {kind} {{ {fields} }}\n    {chain}"),
    )
}

#[test]
fn a_diode_wired_the_wrong_way_round_across_a_rail_is_found_and_named() {
    // `Diode` and `Schottky` conduct from anode to cathode, so the anode on `pos` is a short.
    for kind in ["Diode", "Schottky"] {
        let fields = ".max_reverse_voltage &= 40V\n .forward_voltage &= 0.7V";
        clean(&across_12v(
            kind,
            fields,
            ".supply.neg ~> .d ~> .supply.pos",
        ));
        one(
            &across_12v(kind, fields, ".supply.pos ~> .d ~> .supply.neg"),
            &[
                "d.bias must always be at least -d.forward_voltage",
                "-12.6V..-11.4V",
                "-700mV",
            ],
        );
        // By pins, in either statement order, and by net name.
        one(
            &across_12v(
                kind,
                fields,
                ".supply.neg ~ .d.cathode\n    .supply.pos ~ .d.anode",
            ),
            &["d.bias must always be at least -d.forward_voltage"],
        );
    }
    // `Zener` and `Tvs` conduct from cathode to anode when they clamp: cathode on `pos`.
    for (kind, fields) in [
        ("Zener", ".zener_voltage &= 5.1V +/- 5%"),
        ("Tvs", ".standoff_voltage &= 15V\n .clamping_voltage &= 24V"),
    ] {
        clean(&across_12v(
            kind,
            fields,
            ".supply.pos ~> .d ~> .supply.neg",
        ));
        one(
            &across_12v(kind, fields, ".supply.neg ~> .d ~> .supply.pos"),
            &["d.bias must always be at least 0V", "-12.6V..-11.4V"],
        );
    }
}

#[test]
fn a_reversed_diode_does_not_pass_the_rating_of_the_rail_it_would_block() {
    // The rail is 12 V and the diode blocks 10 V: wired to block, the rating fails with the
    // rail's voltage; wired to conduct, it blocks nothing (its bias is negative) and the
    // finding is the wiring's.
    let fields = ".max_reverse_voltage &= 10V\n .forward_voltage &= 0.7V";
    one(
        &across_12v("Diode", fields, ".supply.neg ~> .d ~> .supply.pos"),
        &["d.bias must always be at most d.max_reverse_voltage"],
    );
    one(
        &across_12v("Diode", fields, ".supply.pos ~> .d ~> .supply.neg"),
        &["d.bias must always be at least -d.forward_voltage"],
    );
}

#[test]
fn a_diode_across_no_rail_is_not_given_a_voltage_and_a_stated_bias_is_the_blocking_one() {
    // In series with a load, the diode is across no rail: nothing is derived, and the bias
    // the board states is the voltage it blocks, as before.
    let board = |bias: &str| {
        on_12v(
            "import std::parts::diode::Schottky",
            &format!(
                "    .load: Rail {{ .pos.net &= \"VF\"\n .neg.net &= \"GND\" }}\n    \
                 .d: Schottky {{ .max_reverse_voltage &= 40V\n .max_forward_current &= 1A\n \
                 .forward_voltage &= 0.5V\n .bias &= {bias} }}\n    \
                 .supply.pos ~> .d ~> .load.pos\n    .supply.neg ~ .load.neg"
            ),
        )
    };
    clean(&board("24V"));
    one(
        &board("60V"),
        &["d.bias must always be at most d.max_reverse_voltage"],
    );
}

#[test]
fn a_zener_over_its_power_is_refused() {
    one(
        &on_12v(
            "import std::parts::diode::Zener",
            "    .z: Zener {\n .bias &= 5.1V\n .current &= 100mA\n .max_power &= 500mW\n }",
        ),
        &["must always be at most z.max_power", "510mW", "500mW"],
    );
}

#[test]
fn a_tvs_diode_stands_off_the_rail_and_clamps_under_the_limit() {
    let imports = "import std::parts::diode::Tvs";
    let board = |standoff: &str, clamp: &str, limit: &str| {
        on_12v(
            imports,
            &format!(
                "    .t: Tvs {{ .standoff_voltage &= {standoff}\n .clamping_voltage &= {clamp}\n \
                 .protected_limit &= {limit} }}\n    .supply.pos ~> .t ~> .supply.neg"
            ),
        )
    };
    clean(&board("15V", "24V", "36V"));
    one(
        &board("12V", "24V", "36V"),
        &[
            "t.bias must always be at most t.standoff_voltage",
            "11.4V..12.6V",
            "12V",
        ],
    );
    one(
        &board("15V", "24V", "20V"),
        &[
            "t.clamping_voltage must always be at most t.protected_limit",
            "24V",
            "20V",
        ],
    );
    // Nothing stated about what it protects: not evaluated, and no complaint.
    clean(&on_12v(
        imports,
        "    .t: Tvs { .standoff_voltage &= 15V\n .clamping_voltage &= 24V }\n    \
         .supply.pos ~> .t ~> .supply.neg",
    ));
}

#[test]
fn a_fuse_is_checked_against_the_draw_of_its_load() {
    let board = |rated: &str| {
        on_12v(
            "import std::parts::passive::Fuse",
            &format!(
                "    .load: Rail {{ .current &= 100mA..600mA\n .pos.net &= \"VF\"\n .neg.net &= \"GND\" }}\n    \
                 .fuse: Fuse {{ .rated_current &= {rated} }}\n    \
                 .supply.pos ~> .fuse ~> .load.pos\n    .supply.neg ~ .load.neg"
            ),
        )
    };
    clean(&board("1A"));
    one(
        &board("500mA"),
        &[
            "fuse.current must always be at most fuse.rated_current",
            "100mA..600mA",
            "500mA",
        ],
    );
}

/// A part in a `~>` chain from a rail has the rail's `.peak` as its `.supply_peak` (ADR 0086),
/// so a fuse is compared with it with no line of the board's. What does not reach a part is
/// still the board's: a diode's rating against the fuse in front of it (issue 175).
#[test]
fn a_fuse_rated_voltage_is_compared_with_the_peak_of_its_rail() {
    let board = |rated: &str, line: &str| {
        on_12v(
            "import std::parts::passive::Fuse",
            &format!(
                "    .supply.peak &= 40V\n    \
                 .load: Rail {{ .current &= 100mA..600mA\n .pos.net &= \"VF\"\n .neg.net &= \"GND\" }}\n    \
                 .fuse: Fuse {{ .rated_current &= 1A\n .rated_voltage &= {rated} }}\n    \
                 .supply.pos ~> .fuse ~> .load.pos\n    .supply.neg ~ .load.neg\n{line}"
            ),
        )
    };
    clean(&board("60V", ""));
    one(
        &board("32V", ""),
        &[
            "fuse.supply_peak must always be at most fuse.rated_voltage",
            "40V",
            "32V",
        ],
    );
    // The line a board wrote before still holds, and agrees with it.
    clean(&board("60V", "    .fuse.rated_voltage >= .supply.peak"));
}

/// A rail that states no `.peak` leaves the fuse's check open: it is listed as not evaluated
/// and the board builds, as for any rating that waits on a value.
#[test]
fn a_fuse_on_a_rail_with_no_peak_is_not_refused() {
    clean(&on_12v(
        "import std::parts::passive::Fuse",
        "    .load: Rail { .current &= 100mA..600mA\n .pos.net &= \"VF\"\n .neg.net &= \"GND\" }\n    \
         .fuse: Fuse { .rated_current &= 1A\n .rated_voltage &= 32V }\n    \
         .supply.pos ~> .fuse ~> .load.pos\n    .supply.neg ~ .load.neg",
    ));
}

#[test]
fn a_diode_behind_a_fuse_is_rated_only_for_its_own_current_unless_the_board_says_more() {
    let board = |diode_rating: &str, line: &str| {
        on_12v(
            "import std::parts::diode::Schottky\nimport std::parts::passive::Fuse",
            &format!(
                "    .load: Rail {{ .current &= 100mA..600mA\n .pos.net &= \"VF\"\n .neg.net &= \"GND\" }}\n    \
                 .fuse: Fuse {{ .rated_current &= 3A }}\n    \
                 .d: Schottky {{ .max_forward_current &= {diode_rating} }}\n    \
                 .supply.pos ~> .fuse ~> .d ~> .load.pos\n    .supply.neg ~ .load.neg\n{line}"
            ),
        )
    };
    let line = "    .d.max_forward_current >= .fuse.rated_current";
    // The diode carries the load's draw (the chain passes one current), so 1 A is enough for
    // that check and the diode is not found short of the fuse's 3 A.
    clean(&board("1A", ""));
    clean(&board("5A", line));
    one(
        &board("1A", line),
        &[
            "d.max_forward_current must always be at least fuse.rated_current",
            "1A",
            "3A",
        ],
    );
}

#[test]
fn a_reverse_polarity_diode_blocks_the_voltage_the_board_states_and_not_one_it_leaves_out() {
    let board = |blocks: &str, stress: &str| {
        on_12v(
            "import std::parts::diode::Schottky",
            &format!(
                "    .load: Rail {{ .current &= 100mA..600mA\n .pos.net &= \"VF\"\n .neg.net &= \"GND\" }}\n    \
                 .d: Schottky {{ .max_reverse_voltage &= {blocks}\n .max_forward_current &= 1A\n {stress} }}\n    \
                 .supply.pos ~> .d ~> .load.pos\n    .supply.neg ~ .load.neg"
            ),
        )
    };
    clean(&board("40V", ".bias &= 24V"));
    one(
        &board("40V", ".bias &= 60V"),
        &[
            "d.bias must always be at most d.max_reverse_voltage",
            "60V",
            "40V",
        ],
    );
    // A diode in series is across no rail, so its bias is unset until the board states it.
    clean(&board("40V", ""));
}

#[test]
fn a_fuse_whose_current_is_only_a_remainder_is_not_evaluated() {
    // The fuse starts a chain that ends on a signal pin, and the rail it is on has a supply
    // of up to 2 A and another load of up to 600 mA. By subtraction the fuse would carry up to
    // 2 A, which says nothing about it, so a 500 mA rating is not evaluated (ADR 0026).
    let board = on_12v(
        "import std::parts::passive::Fuse\nimport std::parts::connector::Header1x2",
        "    .supply.current &= -2A..0A\n    \
         .load: Rail { .current &= 0mA..600mA\n .pos.net &= \"VIN\"\n .neg.net &= \"GND\" }\n    \
         .supply.pos ~ .load.pos\n    .supply.neg ~ .load.neg\n    \
         .fuse: Fuse { .rated_current &= 500mA }\n    .sense: Header1x2\n    \
         .supply.pos ~> .fuse ~> .sense.p1\n    .supply.neg ~ .sense.p2",
    );
    clean(&board);
    let checked = run(&board);
    let s = solve(checked.standalone_boards().next().unwrap(), &catalogs());
    assert!(
        s.deferred.iter().any(|&i| s
            .model
            .show_expr(&s.model.relations()[i].lhs)
            .contains("fuse.current")),
        "the rating is listed as not evaluated"
    );
}

#[test]
fn an_inductor_a_bead_and_a_switch_stay_under_their_current_ratings() {
    let one_part = |kind: &str, fields: &str| {
        on_12v(
            &format!("import std::parts::passive::{kind}"),
            &format!(
                "    .p: {kind} {{ {fields} }}\n    .supply.pos ~ .p.a\n    .supply.neg ~ .p.b"
            ),
        )
    };
    one(
        &one_part("Inductor", ".saturation_current &= 1A\n .current &= 1.5A"),
        &[
            "p.current must always be at most p.saturation_current",
            "1.5A",
            "1A",
        ],
    );
    clean(&one_part(
        "Inductor",
        ".saturation_current &= 2A\n .current &= 1.5A",
    ));
    one(
        &one_part("FerriteBead", ".rated_current &= 500mA\n .current &= 1A"),
        &[
            "p.current must always be at most p.rated_current",
            "1A",
            "500mA",
        ],
    );
    one(
        &one_part("Switch", ".rated_current &= 100mA\n .current &= 1A"),
        &["p.current must always be at most p.rated_current"],
    );
}

#[test]
fn a_mosfet_is_checked_against_its_maxima_and_its_gate_drive_against_the_threshold() {
    for (kind, bridge) in [("NMosfet", "drain"), ("PMosfet", "source")] {
        let imports = format!("import std::parts::transistor::{kind}");
        let board = |maxima: &str, stress: &str| {
            on_12v(
                &imports,
                &format!(
                    "    .q: {kind} {{\n .max_drain_source_voltage &= 20V\n .max_drain_current &= 2A\n \
                     .max_gate_source_voltage &= 8V\n .gate_threshold &= 1V..2V\n {maxima}\n \
                     .drain_source_voltage &= 12V\n .gate_source_voltage &= 4.5V\n {stress} }}\n    \
                     .q.gate ~ .supply.pos\n    .q.{bridge} ~ .supply.neg"
                ),
            )
        };
        clean(&board("", ""));
        one(
            &on_12v(
                &imports,
                &format!(
                    "    .q: {kind} {{ .max_drain_source_voltage &= 10V\n .drain_source_voltage &= 12V }}\n    .q.gate ~ .supply.pos"
                ),
            ),
            &[
                "q.drain_source_voltage must always be at most q.max_drain_source_voltage",
                "12V",
                "10V",
            ],
        );
        one(
            &on_12v(
                &imports,
                &format!(
                    "    .q: {kind} {{ .max_gate_source_voltage &= 4V\n .gate_source_voltage &= 4.5V }}\n    .q.gate ~ .supply.pos"
                ),
            ),
            &[
                "q.gate_source_voltage must always be at most q.max_gate_source_voltage",
                "4.5V",
                "4V",
            ],
        );
        one(
            &on_12v(
                &imports,
                &format!(
                    "    .q: {kind} {{ .gate_threshold &= 4V..5V\n .gate_source_voltage &= 4.5V }}\n    .q.gate ~ .supply.pos"
                ),
            ),
            &[
                "q.gate_source_voltage must always be at least q.gate_threshold",
                "4.5V",
                "4V..5V",
            ],
        );
        one(
            &on_12v(
                &imports,
                &format!(
                    "    .q: {kind} {{ .max_drain_current &= 2A\n .current &= 3A }}\n    .q.gate ~ .supply.pos"
                ),
            ),
            &[
                "q.current must always be at most q.max_drain_current",
                "3A",
                "2A",
            ],
        );
    }
}

#[test]
fn a_capacitor_rating_rule_is_one_line_that_does_not_name_the_rail() {
    let board = |rail: &str, cap: &str| {
        format!(
            "import std::parts::passive::Capacitor\nimport std::sources::Rail\nboard B {{\n    \
             .supply: Rail {{\n .voltage &= {rail}\n .pos.net &= \"VIN\"\n .neg.net &= \"GND\" }}\n    \
             .c: Capacitor {{\n .capacitance &= 100nF +/- 20%\n {cap}\n .max_voltage >= .bias * 1.5 }}\n    \
             .supply.pos ~> .c ~> .supply.neg\n}}\n"
        )
    };
    // A 50 V part (the highest rating in the catalog) holds 1.5 times 3.3 V and not 1.5 times
    // 40 V.
    clean(&board("3.3V +/- 5%", ""));
    let m = messages(&board("40V +/- 5%", ""));
    assert!(!m.is_empty());
    assert!(
        m.iter().any(
            |m| m.contains("c.max_voltage must always be at least (c.bias * 1.5)")
                && m.contains("57V..63V")
        ),
        "{m:#?}"
    );
    // Capacitor itself is unchanged: with no rating rule it solves on 12 V as before.
    clean(&on_12v(
        "import std::parts::passive::Capacitor",
        "    .c: Capacitor { .capacitance &= 100nF +/- 20% }\n    .supply.pos ~> .c ~> .supply.neg",
    ));
}

// ---- buses --------------------------------------------------------------------------

/// A part with a UART, an SPI port and an I2C port, whose roles are set by `spi_master`.
fn bus_part(name: &str, spi_master: bool) -> String {
    let (out, inp) = if spi_master {
        ("output", "input")
    } else {
        ("input", "output")
    };
    format!(
        "part {name} {{\n .vdd: Rail\n .uart: Uart\n .spi: Spi\n .i2c: I2c\n pins {{\n \
         .vdd.pos power \"VDD\" \"1\"\n .vdd.neg power \"VSS\" \"2\"\n \
         .uart.tx output \"TX\" \"3\"\n .uart.rx input \"RX\" \"4\"\n \
         .spi.sclk {out} \"SCK\" \"5\"\n .spi.mosi {out} \"MOSI\" \"6\"\n \
         .spi.miso {inp} \"MISO\" \"7\"\n .spi.cs {out} \"CS\" \"8\"\n \
         .i2c.sda bidirectional \"SDA\" \"9\"\n .i2c.scl bidirectional \"SCL\" \"10\"\n }}\n}}\n"
    )
}

fn bus_board(joins: &str) -> String {
    format!(
        "import std::bus::{{I2c, I2cPullups, Spi, Uart}}\nimport std::sources::Rail\n{}{}\
         board B {{\n .supply: Rail {{\n .voltage &= 3.3V +/- 5%\n .pos.net &= \"3V3\"\n .neg.net &= \"GND\" }}\n \
         .m: Master\n .s: Slave\n .supply ~ .m.vdd\n .supply ~ .s.vdd\n{joins}\n}}\n",
        bus_part("Master", true),
        bus_part("Slave", false),
    )
}

const CROSSED: &str = " .m.uart.tx ~ .s.uart.rx\n .m.uart.rx ~ .s.uart.tx\n";
const SPI: &str = " .m.spi ~ .s.spi\n";
/// The bus with its pull-ups; `I2C_BARE` is the bus alone.
const I2C: &str = " .m.i2c ~ .s.i2c\n .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }\n \
     .supply ~ .pullups.supply\n .m.i2c ~ .pullups.bus\n";
const I2C_BARE: &str = " .m.i2c ~ .s.i2c\n";

#[test]
fn buses_joined_as_the_roles_say_check_clean() {
    clean(&bus_board(&format!("{CROSSED}{SPI}{I2C}")));
}

#[test]
fn a_uart_joined_with_a_tilde_crosses_and_one_joined_pin_by_pin_straight_is_refused() {
    // Since ADR 0064 `~` between two `Uart` interfaces joins `tx` to `rx`, so the board that
    // had two outputs on a net is a board that builds.
    clean(&bus_board(&format!(" .m.uart ~ .s.uart\n{SPI}{I2C}")));
    // Joined pin by pin with the same names, the lines meet themselves: one message for each
    // net, and the rules of a net (two outputs, only inputs) say nothing more.
    let m = messages(&bus_board(&format!(
        " .m.uart.tx ~ .s.uart.tx\n .m.uart.rx ~ .s.uart.rx\n{SPI}{I2C}"
    )));
    assert_eq!(m.len(), 2, "{m:#?}");
    assert!(
        m[0].starts_with("net `net-m.uart.rx` joins `rx` to `rx` (`m.uart.rx`, `s.uart.rx`)"),
        "{}",
        m[0]
    );
    assert!(
        m[1].starts_with("net `net-m.uart.tx` joins `tx` to `tx` (`m.uart.tx`, `s.uart.tx`)"),
        "{}",
        m[1]
    );
}

#[test]
fn a_uart_with_one_direction_missing_leaves_a_receiver_open() {
    one(
        &bus_board(&format!(" .m.uart.tx ~ .s.uart.rx\n{SPI}{I2C}")),
        &[
            "`m.uart.rx` is an input that must be connected",
            "pad 4, `RX`",
        ],
    );
}

#[test]
fn two_spi_masters_have_their_outputs_on_the_same_nets() {
    let src = bus_board(&format!("{CROSSED}{I2C}")).replace(".s: Slave", ".s: Master");
    let m = messages(&with(&src, " .m.spi ~ .s.spi"));
    for net in ["sclk", "mosi", "cs"] {
        let want = format!("net `net-m.spi.{net}` has two outputs: `m.spi.{net}`, `s.spi.{net}`");
        assert!(m.contains(&want), "{want} not in {m:#?}");
    }
}

// ---- the clock of a bus (ADR 0064, stage 0) --------------------------------------------

/// A part with an SPI port whose clock is bounded by its datasheet: `limit` is the bound
/// as it is written (`<= 10MHz`). The pins are `bidirectional`, so that the parts say nothing
/// about who drives (the roles are another stage).
fn clocked_part(name: &str, limit: &str) -> String {
    format!(
        "part {name} {{\n .vdd: Rail\n .spi: Spi\n .spi.frequency {limit}\n pins {{\n \
         .vdd.pos power \"VDD\" \"1\"\n .vdd.neg power \"VSS\" \"2\"\n \
         .spi.sclk bidirectional \"SCK\" \"3\"\n .spi.mosi bidirectional \"MOSI\" \"4\"\n \
         .spi.miso bidirectional \"MISO\" \"5\"\n .spi.cs bidirectional \"CS\" \"6\"\n }}\n}}\n"
    )
}

/// A processor, a flash and an IMU on one bus, the processor's clock stated by `clock`.
fn clock_board(clock: &str) -> String {
    format!(
        "import std::bus::Spi\nimport std::sources::Rail\n{}{}{}\
         board B {{\n .supply: Rail {{\n .voltage &= 3.3V +/- 5%\n .pos.net &= \"3V3\"\n .neg.net &= \"GND\" }}\n \
         .cpu: Zcpu\n .flash: Zflash\n .imu: Zimu\n .supply ~ .cpu.vdd\n .supply ~ .flash.vdd\n .supply ~ .imu.vdd\n \
         .cpu.spi ~ .flash.spi\n .cpu.spi ~ .imu.spi\n{clock}\n}}\n",
        clocked_part("Zcpu", "<= 32MHz"),
        clocked_part("Zflash", "<= 33MHz"),
        clocked_part("Zimu", "<= 10MHz"),
    )
}

#[test]
fn a_bus_clock_inside_every_bound_builds() {
    clean(&clock_board(" .cpu.spi.frequency &= 8MHz"));
}

#[test]
fn a_bus_clock_above_a_bound_is_refused_naming_the_field() {
    let m = messages(&clock_board(" .cpu.spi.frequency &= 20MHz"));
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(
        m[0].contains("imu.spi.frequency") && m[0].contains("10MHz") && m[0].contains("20MHz"),
        "{}",
        m[0]
    );
}

#[test]
fn a_bus_clock_joined_by_a_tilde_reaches_the_third_part() {
    let checked = run(&clock_board(" .cpu.spi.frequency &= 8MHz"));
    assert!(checked.is_ok(), "{}", checked.render());
    let solution = solve(checked.board("B").unwrap(), &catalogs());
    for part in ["cpu", "flash", "imu"] {
        let path = format!("{part}.spi.frequency");
        let shown = solution
            .model
            .domain_at(&path)
            .map(|d| format!("{d:?}"))
            .unwrap_or_default();
        assert!(shown.contains("8000000"), "{path} is {shown}");
    }
}

#[test]
fn the_i2c_pullups_pick_resistors_inside_the_stated_resistance() {
    let src = bus_board(&format!("{CROSSED}{SPI}{I2C}"));
    let checked = run(&src);
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.board("B").unwrap();
    let solution = solve(board, &catalogs());
    assert!(solution.diagnostics.is_empty());
    let mut picked: Vec<_> = solution
        .picks
        .iter()
        .map(|p| solution.model.path_of(p.part))
        .collect();
    picked.sort();
    assert_eq!(picked, ["pullups.scl_pullup", "pullups.sda_pullup"]);
    // Both lines of the bus are on the pull-ups' nets.
    let nets = &checked.board("B").unwrap().model.nets();
    let on_net = |pin: &str| {
        nets.iter()
            .find(|n| n.terminals.iter().any(|t| board.model.path_of(*t) == pin))
            .map(|n| {
                n.terminals
                    .iter()
                    .map(|t| board.model.path_of(*t))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };
    assert!(on_net("m.i2c.sda").contains(&"pullups.sda_pullup.b".to_string()));
    assert!(on_net("m.i2c.scl").contains(&"pullups.scl_pullup.b".to_string()));
}

#[test]
fn a_usb_c_receptacle_supplies_what_its_loads_draw() {
    // The receptacle's `vbus.current` is not stated: it follows from the loads (ADR 0007).
    let src = "import std::parts::connector::UsbCPower\nimport std::sources::Rail\n\
         board B {\n .usb: UsbCPower {\n .vbus.voltage &= 5V +/- 5%\n .vbus.pos.net &= \"VBUS\"\n \
         .vbus.neg.net &= \"GND\" }\n .load: Rail { .current &= 100mA..300mA }\n .usb.vbus ~ .load\n}\n";
    let checked = run(src);
    assert!(checked.is_ok(), "{}", checked.render());
    let solution = solve(checked.board("B").unwrap(), &catalogs());
    assert!(solution.diagnostics.is_empty());
    let board = &checked.board("B").unwrap().model;
    let usb = board.inst_at("usb.vbus").unwrap();
    let current = board.member(usb, "current");
    assert!(current.is_some());
    // A budget on the receptacle is a bound the design writes.
    let over = src.replace(
        " .usb.vbus ~ .load\n",
        " .usb.vbus ~ .load\n .usb.vbus.current >= -200mA\n",
    );
    one(
        &over,
        &[
            "usb.vbus.current must always be at least -200mA",
            "-300mA..-100mA",
        ],
    );
}

#[test]
fn an_i2c_bus_without_pullups_is_refused_and_i2c_pullups_satisfy_it() {
    let m = messages(&bus_board(&format!("{CROSSED}{SPI}{I2C_BARE}")));
    assert_eq!(m.len(), 2, "{m:#?}");
    assert!(m[0].starts_with("net `net-m.i2c.scl` carries the scl line of an I2C bus (`m.i2c.scl`, `s.i2c.scl`) and has no pull-up"), "{}", m[0]);
    assert!(
        m[1].starts_with("net `net-m.i2c.sda` carries the sda line"),
        "{}",
        m[1]
    );
}

#[test]
fn the_obd2_plug_supplies_from_pin_16_and_leaves_every_other_pin_open() {
    // The supply is the connector's; nothing else on it needs a connection, signal ground
    // included.
    clean(
        "import std::parts::connector::Obd2M\nimport std::sources::Rail\n\
         board B {\n\
             .obd: Obd2M {\n\
                 .vbatt.voltage &= 12V +/- 15%\n\
                 .vbatt.pos.net &= \"VBATT\"\n\
                 .vbatt.neg.net &= \"GND\"\n\
             }\n\
             .load: Rail { .current &= 100mA..1A }\n\
             .obd.vbatt ~ .load\n\
         }\n",
    );
}

#[test]
fn joined_rails_share_their_peak_voltage() {
    let src = "import std::sources::Rail\nboard B {\n    .a: Rail {\n        .voltage &= 12V\n        \
               .peak &= 40V\n    }\n    .b: Rail\n    .a ~ .b\n    .b.peak <= 30V\n}\n";
    let m = messages(src);
    assert_eq!(m.len(), 1, "{m:#?}");
    assert!(m[0].contains("b.peak") && m[0].contains("40V"), "{}", m[0]);
}

#[test]
fn a_capacitor_with_a_count_is_still_a_two_pin_part_and_the_count_is_a_plain_number() {
    // The count is a value of the part and not a pin: a bank has the pads of one capacitor.
    assert_eq!(pins_of("std::parts::passive", "Capacitor").len(), 2);
    let board = |count: &str| {
        on_12v(
            "import std::parts::passive::Capacitor",
            &format!(
                "    .c: Capacitor {{ .capacitance &= 100nF +/- 20%; .count &= {count} }}\n    \
                 .supply.pos ~> .c ~> .supply.neg"
            ),
        )
    };
    clean(&board("3"));
    clean(&board("1..4"));
    // A count is a number, not a capacitance.
    let m = messages(&board("3uF"));
    assert!(!m.is_empty());
    assert!(m.iter().any(|m| m.contains("c.count")), "{m:#?}");
}
