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
    let src = format!("import {import}::{kind}\nboard B {{\n    .x: {kind}\n}}\n");
    let c = run(&src);
    let m = &c.board("B").expect("the board checks").model;
    let x = m.inst_at("x").unwrap();
    let mut rows: Vec<_> = m
        .instance(x)
        .tables
        .iter()
        .flat_map(|t| t.rows.iter())
        .map(|r| (m.path_of(r.pin), r.kind, r.name.clone(), r.pads.clone()))
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
        let src = format!("import {import}::{kind}\nboard B {{\n    .x: {kind}\n}}\n");
        let c = run(&src);
        let m = &c.board("B").expect("the board checks").model;
        assert_eq!(
            m.domain_at("x.assembly"),
            Some(&netsmith_check::Domain::Text("none".into())),
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
         board B {\n    .tp: TestPoint\n    .h: PlatedHole\n    .m: MountingHole\n    .f: Fiducial\n}\n",
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
                     .supply.pos ~> .d ~> .supply.neg"
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
        one(
            &on_12v(
                &imports,
                &format!(
                    "    .d: {kind} {{ .max_forward_current &= 1A\n .current &= 1.5A }}\n    \
                     .supply.pos ~ .d.anode\n    .supply.neg ~ .d.cathode"
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
fn a_uart_joined_straight_has_two_outputs_on_a_net_and_two_inputs_with_no_driver() {
    let m = messages(&bus_board(&format!(" .m.uart ~ .s.uart\n{SPI}{I2C}")));
    assert_eq!(m.len(), 2, "{m:#?}");
    assert!(
        m[0].starts_with(
            "net `net-m.uart.rx` has only inputs (`m.uart.rx`, `s.uart.rx`): nothing drives it"
        ),
        "{}",
        m[0]
    );
    assert_eq!(
        m[1],
        "net `net-m.uart.tx` has two outputs: `m.uart.tx`, `s.uart.tx`"
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
