//! Parts with pin tables in the solver: which package is chosen, keyed catalogs, and the
//! faults that wait for the package (ADR 0016).
//!
//! `Ldo9` and `Mcu32` are invented for these tests, and so are their catalog rows: none
//! of them is a real part, and none is in the standard library or in `data/jlcpcb`.

use netsmith_check::{Domain, Loader, Model, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Candidate, Catalog, Catalogs, Solution, Value, solve};

const LDO9: &str = r#"
import std::sources::Rail

part Ldo9 {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg

    .dropout:     Voltage
    .max_current: Current
    .vin.voltage >= .vout.voltage + .dropout
    .vin.current == -.vout.current
    -.vout.current <= .max_current

    pins "DFN-8-EP" {
        .vout.pos      power       "OUT"    "1" "2"
        .sense         input       "SENSE"  "3"
        .vin.neg       power       "GND"    "4" "9"
        .nc            no_connect  "NC"     "5"
        .en            input       "EN"     "6"
        .vin.pos       power       "IN"     "7" "8"
    }
    pins "SOT-23-5" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .en            input       "EN"     "3"
        .nc            no_connect  "NC"     "4"
        .vout.pos      power       "OUT"    "5"
    }
    tie .sense ~ .vout.pos

    .package: Package
    catalog ldo9 { .vout.voltage, .dropout, .max_current, .package }
}

board B {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .u: Ldo9 { .vout.voltage &= 3.3V +/- 5% }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.en ~ .u.vin.pos
CONNECT}
"#;

const SENSE: &str = "    .u.sense ~ .u.vout.pos\n";

fn ldo(src_connect: &str) -> String {
    LDO9.replace("CONNECT", src_connect)
}

fn tolerance(volts: f64, percent: f64) -> Quantity {
    let center = Quantity::literal(volts, Some("V")).unwrap().quantity;
    let tol = Quantity::literal(percent, Some("%")).unwrap().quantity;
    Quantity::tolerance(&center, &tol, true).unwrap()
}

fn amps(a: f64) -> Value {
    Value::Quantity(Quantity::literal(a, Some("A")).unwrap().quantity)
}

/// An invented regulator row.
fn ldo_row(name: &str, package: &str) -> Candidate {
    Candidate {
        name: name.into(),
        label: name.into(),
        columns: vec![
            ("vout.voltage".into(), Value::Quantity(tolerance(3.3, 2.0))),
            (
                "dropout".into(),
                Value::Quantity(Quantity::literal(0.3, Some("V")).unwrap().quantity),
            ),
            ("max_current".into(), amps(0.5)),
            ("package".into(), Value::Text(package.into())),
        ],
        lcsc: None,
        mpn: None,
        library: None,
        assembly: None,
        height: None,
        generic: None,
    }
}

/// The smaller package is listed first, so it is the one the solver would take.
fn ldo_catalogs(rows: Vec<Candidate>) -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "ldo9".into(),
        candidates: rows,
    });
    c
}

fn both_packages() -> Catalogs {
    ldo_catalogs(vec![
        ldo_row("ILLUSTRATIVE-LDO9-SMALL", "SOT-23-5"),
        ldo_row("ILLUSTRATIVE-LDO9-BIG", "DFN-8-EP"),
    ])
}

fn solved(src: &str, catalogs: &Catalogs) -> (Model, Solution) {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    let s = solve(&c.boards[0], catalogs);
    (c.boards[0].model.clone(), s)
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn package(s: &Solution, part: &str) -> Option<String> {
    match s.model.domain_at(&format!("{part}.package")) {
        Some(Domain::Text(t)) => Some(t.clone()),
        _ => None,
    }
}

#[test]
fn a_row_whose_package_lacks_a_pin_the_board_connects_is_not_a_candidate() {
    let (_, s) = solved(&ldo(SENSE), &both_packages());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    // The first row is the SOT-23-5, which has no sense pin.
    assert_eq!(package(&s, "u").as_deref(), Some("DFN-8-EP"));
    assert_eq!(s.picks[0].candidate, "ILLUSTRATIVE-LDO9-BIG");
}

#[test]
fn a_board_that_never_uses_the_pin_takes_the_first_row() {
    // Without the sense pin the DFN could not be used either: its tie is not made. Only
    // the SOT-23-5 does.
    let (_, s) = solved(&ldo(""), &both_packages());
    assert_eq!(package(&s, "u").as_deref(), Some("SOT-23-5"));
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
}

#[test]
fn when_every_package_lacks_a_pin_the_message_says_so() {
    let only_small = ldo_catalogs(vec![ldo_row("ILLUSTRATIVE-LDO9-SMALL", "SOT-23-5")]);
    let (_, s) = solved(&ldo(SENSE), &only_small);
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].starts_with("the `ldo9` catalog has nothing for `u`, which needs"),
        "{}",
        msgs[0]
    );
    assert!(
        msgs[0].contains(
            "rows were left out because package `SOT-23-5` has no `sense`, which the board connects"
        ),
        "{}",
        msgs[0]
    );
    assert_eq!(s.unresolved.len(), 1);
}

#[test]
fn a_package_the_board_names_is_checked_against_the_pins_it_connects() {
    // The board asks for the SOT-23-5 and connects the sense pin: no row is left, and the
    // check says which pin is missing from which package.
    let src = ldo(SENSE).replace(".u: Ldo9 {", ".u: Ldo9 { .package &= \"SOT-23-5\"\n");
    let c = check("test.nsm", &src, &Loader::std());
    let msgs: Vec<_> = c.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert_eq!(
        msgs,
        [
            "`u.sense` (`SENSE`) does not exist in package `SOT-23-5` of `Ldo9`, but the board connects it; the packages that have it: `DFN-8-EP`"
        ]
    );
}

#[test]
fn pin_faults_that_wait_for_the_package_are_reported_after_the_solve() {
    // Only a DFN row. The board connects sense and leaves `en` open, which is a fault
    // only once the package is known to have `en`.
    let only_big = ldo_catalogs(vec![ldo_row("ILLUSTRATIVE-LDO9-BIG", "DFN-8-EP")]);
    let src = ldo(SENSE).replace("    .u.en ~ .u.vin.pos\n", "");
    let c = check("test.nsm", &src, &Loader::std());
    assert!(
        c.is_ok(),
        "nothing is known before the solve:\n{}",
        c.render()
    );
    let (_, s) = solved(&src, &only_big);
    assert_eq!(
        messages(&s),
        ["`u.en` is an input that must be connected (pad 6, `EN`), but nothing else is on its net"]
    );
}

#[test]
fn the_pins_of_the_chosen_package_are_in_the_solved_model() {
    let (_, s) = solved(&ldo(SENSE), &both_packages());
    let u = s.model.inst_at("u").unwrap();
    let out = s.model.inst_at("u.vout.pos").unwrap();
    assert_eq!(s.model.package_of(u).as_deref(), Some("DFN-8-EP"));
    assert_eq!(s.model.pin_row(out).unwrap().pads, ["1", "2"]);
}

// ---- keyed catalogs ----------------------------------------------------------------

const MCU: &str = r#"
import std::sources::Rail

part Mcu32 {
    .vdd: Rail
    .vdd.voltage >= 1.7V
    .vdd.voltage <= 3.6V
    .vdd.current &= 1mA..25mA

    pins "QFN-32" {
        .vdd.pos power         "VDD" "1" "17" "32"
        .vdd.neg power         "VSS" "8" "24" "33"
        .pa2     bidirectional "PA2" "12" fn GPIO UART1_TX SPI1_SCK
    }
    pins "TSSOP-20" {
        .vdd.pos power         "VDD" "4"
        .vdd.neg power         "VSS" "2"
        .pa2     bidirectional "PA2" "8" fn GPIO UART1_TX
    }

    .package: Package
    .mpn: String
    catalog mcu by .mpn { .mpn, .package }
}

board B {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Mcu32 { MPN }
    .supply ~ .u.vdd
}
"#;

fn mcu_row(mpn: &str, package: &str) -> Candidate {
    Candidate {
        name: mpn.into(),
        label: mpn.into(),
        columns: vec![
            ("mpn".into(), Value::Text(mpn.into())),
            ("package".into(), Value::Text(package.into())),
        ],
        lcsc: None,
        mpn: None,
        library: None,
        assembly: None,
        height: None,
        generic: None,
    }
}

fn mcu_catalogs() -> Catalogs {
    let mut c = Catalogs::empty();
    c.insert(Catalog {
        name: "mcu".into(),
        candidates: vec![
            mcu_row("ILLUSTRATIVE-MCU32-A", "QFN-32"),
            mcu_row("ILLUSTRATIVE-MCU32-B", "TSSOP-20"),
        ],
    });
    c
}

#[test]
fn a_keyed_catalog_will_not_choose_a_row_for_the_design() {
    let (_, s) = solved(&MCU.replace("MPN", ""), &mcu_catalogs());
    assert_eq!(
        messages(&s),
        ["`u` is an `Mcu32`; name the part with .mpn: ILLUSTRATIVE-MCU32-A, ILLUSTRATIVE-MCU32-B"]
    );
    assert!(s.picks.is_empty());
    assert_eq!(s.unresolved.len(), 1);
}

#[test]
fn naming_the_key_selects_that_row_and_its_package_selects_the_pinout() {
    let src = MCU.replace("MPN", ".mpn &= \"ILLUSTRATIVE-MCU32-B\"");
    let (_, s) = solved(&src, &mcu_catalogs());
    assert!(s.diagnostics.is_empty(), "{:?}", messages(&s));
    assert_eq!(package(&s, "u").as_deref(), Some("TSSOP-20"));
    // The pads are the TSSOP-20's, not the QFN-32's.
    let vdd = s.model.inst_at("u.vdd.pos").unwrap();
    assert_eq!(s.model.pin_row(vdd).unwrap().pads, ["4"]);
    // The other rows' packages do not matter.
    let src = MCU.replace("MPN", ".mpn &= \"ILLUSTRATIVE-MCU32-A\"");
    let (_, s) = solved(&src, &mcu_catalogs());
    let vdd = s.model.inst_at("u.vdd.pos").unwrap();
    assert_eq!(s.model.pin_row(vdd).unwrap().pads, ["1", "17", "32"]);
}

#[test]
fn a_key_no_row_has_is_a_catalog_miss_not_a_listing() {
    let src = MCU.replace("MPN", ".mpn &= \"NO-SUCH-PART\"");
    let (_, s) = solved(&src, &mcu_catalogs());
    let msgs = messages(&s);
    assert_eq!(msgs.len(), 1, "{msgs:#?}");
    assert!(
        msgs[0].starts_with("the `mcu` catalog has nothing for `u`, which needs"),
        "{}",
        msgs[0]
    );
    assert!(
        msgs[0].contains("mpn within \"NO-SUCH-PART\""),
        "{}",
        msgs[0]
    );
}

// ---- pads --------------------------------------------------------------------------

fn pads_board(part_body: &str, board_body: &str) -> Vec<String> {
    let src =
        format!("part P {{\n{part_body}\n}}\nboard B {{\n    .p: P\n    .q: P\n{board_body}\n}}\n");
    let (_, s) = solved(&src, &Catalogs::empty());
    messages(&s)
}

#[test]
fn a_pad_given_to_two_pins_that_the_part_does_not_join_is_refused() {
    let msgs = pads_board(
        "    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"1\"\n    }",
        "    .p.a ~ .q.a\n",
    );
    assert_eq!(
        msgs,
        [
            "pad 1 of `P` is given to both `a` and `b`; two pins share a pad only if the part joins them"
        ]
    );
}

#[test]
fn a_pad_shared_by_pins_the_part_joins_is_one_pin() {
    let msgs = pads_board(
        "    pins {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"1\"\n    }\n    .a ~ .b",
        "    .p.a ~ .q.a\n",
    );
    assert!(msgs.is_empty(), "{msgs:#?}");
}

#[test]
fn a_pad_shared_in_a_package_names_the_package() {
    let src = "part P {\n    .package: Package\n    pins \"X\" {\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"1\"\n    }\n}\nboard B {\n    .p: P { .package &= \"X\" }\n    .q: P { .package &= \"X\" }\n    .p.a ~ .q.a\n}\n";
    let (_, s) = solved(src, &Catalogs::empty());
    assert_eq!(
        messages(&s),
        [
            "pad 1 of `P` (package `X`) is given to both `a` and `b`; two pins share a pad only if the part joins them"
        ]
    );
}

#[test]
fn a_pin_joined_inside_the_part_to_a_pin_with_pads_needs_none() {
    let msgs = pads_board(
        "    pins {\n        .a passive \"A\" \"1\"\n    }\n    .c: Pin\n    .a ~ .c",
        "    .p.c ~ .q.c\n",
    );
    assert!(msgs.is_empty(), "{msgs:#?}");
}

#[test]
fn a_pin_with_no_pads_and_no_alias_is_refused_once_for_the_part() {
    let msgs = pads_board(
        "    pins {\n        .a passive \"A\" \"1\"\n    }\n    .c: Pin\n    .d: Pin",
        "    .p.c ~ .q.c\n    .p.d ~ .q.d\n    .p.a ~ .q.a\n",
    );
    let wanted = [
        "`p` is a `P` and its pins (`c`, `d`) have no pads, so it cannot be placed on a symbol; add them to a `pins` table of `P`, which says which pad each pin is",
        "`q` is a `P` and its pins (`c`, `d`) have no pads, so it cannot be placed on a symbol; add them to a `pins` table of `P`, which says which pad each pin is",
    ];
    assert_eq!(msgs, wanted);
}
