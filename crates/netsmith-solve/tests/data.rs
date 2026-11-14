//! The catalog files that ship in the binary (`data/jlcpcb/catalog`, `data/sample`): that
//! every part in them has its source, that they hold the parts the examples rely on in the
//! order the selection policy wants, and that the parts looked at and not offered are not
//! offered (ADR 0022).

use std::fs;
use std::path::{Path, PathBuf};

use netsmith_check::FieldKind;
use netsmith_quantity::{Dimension, Quantity};
use netsmith_solve::{Candidate, Catalog, Catalogs, Derating, Library, Value, parse_csv};

fn data(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(path)
}

/// The records of a file, as header name to cell.
fn records(path: &str) -> Vec<std::collections::BTreeMap<String, String>> {
    let text = fs::read_to_string(data(path)).unwrap_or_else(|e| panic!("{path}: {e}"));
    let rows = parse_csv(&text).unwrap();
    let (_, header) = &rows[0];
    rows[1..]
        .iter()
        .map(|(_, r)| header.iter().cloned().zip(r.iter().cloned()).collect())
        .collect()
}

fn q(name: &str, d: Dimension) -> (String, FieldKind) {
    (name.to_string(), FieldKind::Quantity(d))
}

fn t(name: &str) -> (String, FieldKind) {
    (name.to_string(), FieldKind::Text)
}

/// The columns of `catalog X { ... }` in `std/parts`, in order.
fn columns(name: &str) -> Vec<(String, FieldKind)> {
    match name {
        "resistor" => vec![
            q("resistance", Dimension::RESISTANCE),
            q("max_power", Dimension::POWER),
            t("package"),
            q("temperature_coefficient", Dimension::NONE),
        ],
        "capacitor" => vec![
            q("capacitance", Dimension::CAPACITANCE),
            q("effective_capacitance", Dimension::CAPACITANCE),
            q("max_voltage", Dimension::VOLTAGE),
            t("package"),
            t("dielectric"),
        ],
        "led" => vec![
            t("color"),
            q("forward_voltage", Dimension::VOLTAGE),
            t("package"),
        ],
        "ldo" => vec![
            q("vout.voltage", Dimension::VOLTAGE),
            q("dropout", Dimension::VOLTAGE),
            q("max_current", Dimension::CURRENT),
            q("max_power", Dimension::POWER),
            t("package"),
        ],
        "schottky" => vec![
            q("forward_voltage", Dimension::VOLTAGE),
            q("max_reverse_voltage", Dimension::VOLTAGE),
            q("max_forward_current", Dimension::CURRENT),
            t("package"),
        ],
        "zener" => vec![
            q("zener_voltage", Dimension::VOLTAGE),
            q("max_power", Dimension::POWER),
            t("package"),
            q("test_current", Dimension::CURRENT),
            q("temperature_coefficient", Dimension::VOLTAGE),
            q("leakage_current", Dimension::CURRENT),
            q("leakage_test_voltage", Dimension::VOLTAGE),
        ],
        "tvs" | "tvs_bidirectional" => vec![
            q("standoff_voltage", Dimension::VOLTAGE),
            q("clamping_voltage", Dimension::VOLTAGE),
            q("peak_pulse_current", Dimension::CURRENT),
            t("package"),
            q("breakdown_voltage", Dimension::VOLTAGE),
        ],
        "ferrite_bead" => vec![
            q("impedance", Dimension::RESISTANCE),
            q("impedance_frequency", Dimension::FREQUENCY),
            q("rated_current", Dimension::CURRENT),
            q("dc_resistance", Dimension::RESISTANCE),
            t("package"),
        ],
        "inductor" => vec![
            q("inductance", Dimension::INDUCTANCE),
            q("saturation_current", Dimension::CURRENT),
            q("dc_resistance", Dimension::RESISTANCE),
            t("package"),
            q("rms_current", Dimension::CURRENT),
        ],
        "fuse" => vec![
            q("rated_current", Dimension::CURRENT),
            q("rated_voltage", Dimension::VOLTAGE),
            q("resistance", Dimension::RESISTANCE),
            t("package"),
        ],
        "n_mosfet" => vec![
            q("max_drain_source_voltage", Dimension::VOLTAGE),
            q("max_drain_current", Dimension::CURRENT),
            q("max_gate_source_voltage", Dimension::VOLTAGE),
            q("gate_threshold", Dimension::VOLTAGE),
            q("on_resistance", Dimension::RESISTANCE),
            t("package"),
            q("on_resistance_vgs", Dimension::VOLTAGE),
        ],
        "crystal" => vec![
            q("frequency", Dimension::FREQUENCY),
            q("load_capacitance", Dimension::CAPACITANCE),
            t("package"),
        ],
        "header" => vec![
            t("mating.gender"),
            q("mating.pitch", Dimension::LENGTH),
            q("mating.rows", Dimension::NONE),
            q("mating.positions", Dimension::NONE),
            q("mating.pin_current", Dimension::CURRENT),
            q("mating.seat", Dimension::LENGTH),
        ],
        // The buck converter ICs (issue #620): one row each, the package the only column.
        "lmr16006y" | "rt8059" | "tps54202" => vec![t("package")],
        other => panic!("no columns for {other}"),
    }
}

fn catalog(name: &str) -> Catalog {
    // The columns the `catalog` statement marks `optional` (ADR 0078), for the kind that has
    // any besides the three passive ones.
    let optional: Vec<String> = match name {
        "zener" => [
            "test_current",
            "temperature_coefficient",
            "leakage_current",
            "leakage_test_voltage",
        ]
        .map(String::from)
        .to_vec(),
        "n_mosfet" => vec!["on_resistance_vgs".to_string()],
        "header" => vec!["mating.seat".to_string()],
        _ => Vec::new(),
    };
    Catalogs::jlcpcb()
        .resolve(name, &columns(name), &optional, &[])
        .unwrap_or_else(|| panic!("no {name} catalog"))
        .unwrap_or_else(|e| panic!("{e}"))
}

fn lcscs(name: &str) -> Vec<String> {
    catalog(name)
        .candidates
        .iter()
        .map(|c| c.lcsc.clone().unwrap())
        .collect()
}

fn quantity(value: f64, unit: &str) -> Quantity {
    Quantity::literal(value, Some(unit)).unwrap().quantity
}

fn tolerance(value: f64, unit: &str, percent: f64) -> Quantity {
    Quantity::tolerance(&quantity(value, unit), &quantity(percent, "%"), true).unwrap()
}

const CATALOGS: [&str; 17] = [
    "resistor",
    "capacitor",
    "led",
    "ldo",
    "schottky",
    "zener",
    "tvs",
    "tvs_bidirectional",
    "ferrite_bead",
    "inductor",
    "fuse",
    "n_mosfet",
    "crystal",
    "header",
    "lmr16006y",
    "rt8059",
    "tps54202",
];

#[test]
fn every_standard_file_reads_and_every_part_in_it_has_its_source() {
    for name in CATALOGS {
        let rows = records(&format!("jlcpcb/catalog/{name}.csv"));
        assert_eq!(rows.len(), catalog(name).candidates.len(), "{name}");
        assert!(!rows.is_empty(), "{name} is empty");
        for row in rows {
            let lcsc = &row["lcsc"];
            assert!(!row["mpn"].is_empty(), "{name} {lcsc}");
            // The records the part was read from, and the datasheet where the model needs one.
            assert!(
                row["source"].contains("JLCPCB and LCSC part records"),
                "{name} {lcsc}: {}",
                row["source"]
            );
            // Price, stock and the days they were read go stale, so they are not shipped; an
            // empty `stock` is unknown, never zero (ADR 0039).
            for column in ["price", "stock", "date", "checked"] {
                assert!(row[column].is_empty(), "{name} {lcsc}: {column} is shipped");
            }
            assert!(
                matches!(row["library"].as_str(), "Basic" | "Extended" | "Preferred"),
                "{name} {lcsc}: library `{}`",
                row["library"]
            );
        }
    }
}

/// How many rows of the standard catalogs have a `height` (ADR 0094). Update it with the rows.
const ROWS_WITH_HEIGHT: usize = 106;

#[test]
fn every_height_is_one_length_that_a_document_says_and_its_source_names_where() {
    let mut with = 0;
    for name in CATALOGS {
        for row in records(&format!("jlcpcb/catalog/{name}.csv")) {
            let lcsc = &row["lcsc"];
            let Some(cell) = row.get("height").filter(|c| !c.is_empty()) else {
                continue;
            };
            with += 1;
            let mm: f64 = cell
                .strip_suffix("mm")
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| panic!("{name} {lcsc}: height `{cell}` is not like 2.62mm"));
            // The tallest body in the catalogs is a pin header, 8.9 mm.
            assert!(mm > 0.0 && mm <= 9.0, "{name} {lcsc}: height {mm} mm");
            // The `source` has a `height:` entry that begins with the number of the cell and says
            // which document and where in it, so that a person can look the figure up.
            let source = &row["source"];
            let entry = source
                .split(" ; ")
                .find_map(|e| e.strip_prefix("height: "))
                .unwrap_or_else(|| panic!("{name} {lcsc}: no `height:` in the source: {source}"));
            let stated: f64 = entry
                .split_whitespace()
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| {
                    panic!("{name} {lcsc}: the entry does not begin with a number: {entry}")
                });
            assert!(
                (stated - mm).abs() < 1e-9 && entry[..].contains(" mm"),
                "{name} {lcsc}: the cell says {cell} and the source says {entry}"
            );
            assert!(
                [
                    "page",
                    "Page",
                    "table",
                    "Table",
                    "drawing",
                    "Dimensions",
                    "Specifications"
                ]
                .iter()
                .any(|w| entry.contains(w)),
                "{name} {lcsc}: the entry names no page, table or drawing: {entry}"
            );
        }
    }
    assert_eq!(with, ROWS_WITH_HEIGHT);
}

/// How many rows of the standard catalogs state `temp_min` and `temp_max` (ADR 0099): the four
/// crystals, whose figure is the JLCPCB listing text, and 341 rows read from a datasheet (303
/// ambient or operating ranges, 26 junction ranges of the diodes and the MOSFET, and 12 operating
/// ranges of LEDs; the 1N5819WS, whose range is the one figure of its operating and storage
/// range, is counted with the ranges and not with the junction ones). Update it with the rows.
const ROWS_WITH_TEMPERATURE: usize = 345;

/// The catalogs of semiconductors whose datasheets print a junction range (`TJ`) and no ambient
/// one. A row of them that states a range says in its `notes` which temperature it is (ADR 0099): a
/// junction range is the part's range, and the environment check judges the part by it, which is
/// the limit of that check.
const JUNCTION_CATALOGS: [&str; 5] = ["schottky", "tvs", "tvs_bidirectional", "zener", "n_mosfet"];

/// A temperature cell such as `233.15K`, in kelvin.
fn kelvin(name: &str, lcsc: &str, column: &str, cell: &str) -> f64 {
    cell.strip_suffix('K')
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{name} {lcsc}: {column} `{cell}` is not like 233.15K"))
}

#[test]
fn every_temperature_range_is_two_kelvin_figures_and_its_row_names_the_datasheet() {
    let mut with = 0;
    let mut junction = 0;
    for name in CATALOGS {
        for row in records(&format!("jlcpcb/catalog/{name}.csv")) {
            let lcsc = &row["lcsc"];
            let cell = |column: &str| row.get(column).map_or("", String::as_str);
            let (min, max) = (cell("temp_min"), cell("temp_max"));
            // Both ends or neither: half a range says nothing about the other end.
            assert_eq!(
                min.is_empty(),
                max.is_empty(),
                "{name} {lcsc}: one end of the temperature range is empty"
            );
            if min.is_empty() {
                continue;
            }
            with += 1;
            let (lo, hi) = (
                kelvin(name, lcsc, "temp_min", min),
                kelvin(name, lcsc, "temp_max", max),
            );
            assert!(
                lo < hi,
                "{name} {lcsc}: temp_min {lo} K is not below temp_max {hi} K"
            );
            for k in [lo, hi] {
                assert!(
                    (200.0..=500.0).contains(&k),
                    "{name} {lcsc}: {k} K is not a plausible temperature"
                );
            }
            // The `source` has a `temp_min, temp_max:` entry that begins with the range in
            // degrees Celsius that the cells say, and the `notes` name the datasheet (the crystals,
            // whose range is the listing text of the records, say so instead).
            let source = &row["source"];
            let entry = source
                .split(" ; ")
                .find_map(|e| e.strip_prefix("temp_min, temp_max: "))
                .unwrap_or_else(|| panic!("{name} {lcsc}: no `temp_min, temp_max:` in the source"));
            let notes = &row["notes"];
            if name == "crystal" {
                assert!(entry.contains("JLCPCB listing"), "{name} {lcsc}: {entry}");
                assert!(notes.contains("JLCPCB lists"), "{name} {lcsc}: {notes}");
                continue;
            }
            let (lo_c, hi_c) = (lo - 273.15, hi - 273.15);
            let stated = format!("{lo_c:+.0} C to {hi_c:+.0} C");
            assert!(
                entry.starts_with(&stated),
                "{name} {lcsc}: the cells say {stated} and the source says {entry}"
            );
            assert!(
                ["page", "Page", "sheet 1/1"]
                    .iter()
                    .any(|w| entry.contains(w)),
                "{name} {lcsc}: the entry names no page or sheet: {entry}"
            );
            assert!(
                notes.contains("datasheet") && notes.contains("temp_min and temp_max"),
                "{name} {lcsc}: the notes do not name the datasheet: {notes}"
            );
            if JUNCTION_CATALOGS.contains(&name) {
                // Exactly one statement: the figure is a junction range (counted below), or the
                // datasheet's one figure "does not say junction or ambient" (the 1N5819WS's
                // operating range). A note that merely contains the word `junction` proves neither.
                let is_junction = notes.contains("it is a junction temperature range");
                let says_not = notes.contains("does not say junction");
                assert!(
                    is_junction != says_not,
                    "{name} {lcsc}: the notes of a diode or MOSFET row must say which temperature the figure is, as a junction range or as one the datasheet does not name, and not both: {notes}"
                );
                if is_junction {
                    junction += 1;
                }
            }
        }
    }
    assert_eq!(with, ROWS_WITH_TEMPERATURE);
    assert_eq!(junction, 26, "the rows whose range is a junction range");
}

#[test]
fn the_files_list_basic_parts_before_extended_ones() {
    // Nothing sorts the files. The rows were ordered Basic first, then by the unit price read
    // on a past date, then by value. The prices are no longer shipped, so the order within a
    // library cannot be re-checked here (the README says so); this keeps the part that can.
    for name in CATALOGS {
        let rows = records(&format!("jlcpcb/catalog/{name}.csv"));
        for pair in rows.windows(2) {
            assert!(
                !(pair[0]["library"] != "Basic" && pair[1]["library"] == "Basic"),
                "{name}: {} (Basic) is after {}",
                pair[1]["lcsc"],
                pair[0]["lcsc"]
            );
        }
    }
}

#[test]
fn parts_that_were_looked_at_and_not_offered_are_not_offered() {
    let considered = records("jlcpcb/considered.csv");
    assert!(!considered.is_empty());
    let every: Vec<String> = CATALOGS.iter().flat_map(|n| lcscs(n)).collect();
    for row in &considered {
        assert!(
            !row["not_offered"].is_empty(),
            "{} has no reason",
            row["lcsc"]
        );
        assert!(!row["evidence"].is_empty());
        assert!(
            row["lcsc"].is_empty() || !every.contains(&row["lcsc"]),
            "{} is offered",
            row["lcsc"]
        );
    }
    // The AMS1117, the two 5-pin regulators, the emerald green LED, the 0 ohm jumper, the
    // part that is out of stock and the 1206 capacitor.
    for banned in [
        "C6186", "C82942", "C51118", "C12624", "C17168", "C1723147", "C15008",
    ] {
        assert!(
            considered.iter().any(|r| r["lcsc"] == banned),
            "{banned} is not recorded"
        );
        assert!(!every.iter().any(|l| l == banned), "{banned} was offered");
    }
}

#[test]
fn a_part_is_in_the_standard_catalogs_once() {
    for name in CATALOGS {
        let ids = lcscs(name);
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "{name}");
    }
}

#[test]
fn the_resistors_are_one_percent_in_0402_0603_and_power_0805_basic_first() {
    let cat = catalog("resistor");
    let ids = lcscs("resistor");
    assert!(!ids.contains(&"C17168".to_string()), "the 0 ohm jumper");
    assert!(
        !ids.contains(&"C21189".to_string()),
        "the 0603 0 ohm jumper"
    );
    let libs: Vec<_> = cat.candidates.iter().map(|c| c.library.unwrap()).collect();
    let first_extended = libs.iter().position(|l| *l == Library::Extended).unwrap();
    assert!(libs[..first_extended].iter().all(|l| *l == Library::Basic));
    assert!(
        libs[first_extended..]
            .iter()
            .all(|l| *l == Library::Extended)
    );
    let rows = records("jlcpcb/catalog/resistor.csv");
    for (c, row) in cat.candidates.iter().zip(&rows) {
        let package = row["package"].as_str();
        assert_eq!(c.column("package"), Some(&Value::Text(package.into())));
        assert!(row["resistance"].ends_with("+/- 1%"), "{}", row["lcsc"]);
        match package {
            "0402" | "0603" => {}
            // 0805 is offered for power use only: the name of the part says 250 mW or more.
            "0805" => assert!(
                row["source"].contains("250mW") || row["notes"].contains("250mW"),
                "{} is a 0805 that is not for power use",
                row["lcsc"]
            ),
            other => panic!("{} is {other}", row["lcsc"]),
        }
    }
    assert!(ids.contains(&"C25099".to_string()), "270R is Extended");
    assert!(ids.contains(&"C25102".to_string()), "300R is Extended");
    assert!(
        !ids.contains(&"C247039".to_string()),
        "the 125 mW 0805 is not for power use"
    );
}

/// The complete E24 series from 10 ohm to 1 Mohm is offered in 0402, so that a design that
/// needs an E24 value never hears "the catalog has nothing for it".
#[test]
fn every_e24_value_from_10_ohm_to_1_megohm_is_offered_in_0402() {
    const E24: [u32; 24] = [
        10, 11, 12, 13, 15, 16, 18, 20, 22, 24, 27, 30, 33, 36, 39, 43, 47, 51, 56, 62, 68, 75, 82,
        91,
    ];
    let offered: Vec<(f64, f64)> = catalog("resistor")
        .candidates
        .iter()
        .filter(|c| c.column("package") == Some(&Value::Text("0402".into())))
        .map(|c| match c.column("resistance") {
            Some(Value::Quantity(q)) => (q.lo(), q.hi()),
            other => panic!("{other:?}"),
        })
        .collect();
    let wanted: Vec<f64> = (1..=5u32)
        .flat_map(|decade| {
            E24.iter()
                .map(move |e| f64::from(e * 10u32.pow(decade)) / 10.0)
        })
        .chain([1e6])
        .collect();
    assert_eq!(wanted.len(), 121);
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-9 * b;
    for ohms in wanted {
        assert!(
            offered
                .iter()
                .any(|&(lo, hi)| near(lo, ohms * 0.99) && near(hi, ohms * 1.01)),
            "no 0402 1% resistor of {ohms} ohm"
        );
    }
}

/// Every Basic 0603 1% resistor JLCPCB lists (thick film, E24 and the 1 to 5.1 ohm and
/// 2 and 10 Mohm values) is offered, so a design that asks for 0603 finds one.
#[test]
fn the_basic_0603_resistors_are_offered() {
    let rows = records("jlcpcb/catalog/resistor.csv");
    let basic_0603 = rows
        .iter()
        .filter(|r| r["package"] == "0603" && r["library"] == "Basic")
        .count();
    assert_eq!(basic_0603, 79);
}

#[test]
fn a_resistor_is_the_same_interval_a_source_would_write() {
    let source = tolerance(4.7, "kohm", 1.0);
    let cat = catalog("resistor");
    let c = cat
        .candidates
        .iter()
        .find(|c| c.lcsc.as_deref() == Some("C25900"))
        .unwrap();
    assert_eq!(c.column("resistance"), Some(&Value::Quantity(source)));
    assert_eq!(c.label, "4.7k");
    assert_eq!(c.name, "4.7kΩ 1% 0402 C25900");
    assert_eq!(c.mpn.as_deref(), Some("0402WGF4701TCE"));
}

fn curve_of(lcsc: &str) -> Derating {
    let cat = catalog("capacitor");
    let c = cat
        .candidates
        .iter()
        .find(|c| c.lcsc.as_deref() == Some(lcsc))
        .unwrap();
    match c.column("effective_capacitance") {
        Some(Value::Derated(d)) => d.clone(),
        other => panic!("{other:?}"),
    }
}

fn bias(lo: f64, hi: f64) -> Quantity {
    Quantity::range(&quantity(lo, "V"), &quantity(hi, "V")).unwrap()
}

#[test]
fn capacitors_carry_tolerance_voltage_and_package() {
    let cat = catalog("capacitor");
    let c = cat
        .candidates
        .iter()
        .find(|c| c.lcsc.as_deref() == Some("C23733"))
        .unwrap();
    let source = tolerance(4.7, "uF", 20.0);
    assert_eq!(c.column("capacitance"), Some(&Value::Quantity(source)));
    let d = curve_of("C23733");
    assert_eq!((d.marked, d.against.as_str()), (source, "bias"));
    assert_eq!(
        c.column("max_voltage"),
        Some(&Value::Quantity(quantity(10.0, "V")))
    );
    assert_eq!(c.label, "4.7u");
    let ids = lcscs("capacitor");
    assert!(ids.contains(&"C16780".to_string()), "a verified 0805");
    assert!(
        !ids.contains(&"C15008".to_string()),
        "the 1206 100 uF is recorded in considered.csv, not offered"
    );
}

/// A capacitor has no curve only for a recorded reason, so a missing curve is never an
/// oversight: a maker with no obtainable curve (Murata, Yageo, Fenghua, see
/// `data/jlcpcb/README.md`), or a Samsung class II part of 2.2 uF or more whose Component
/// Library curve is measured at 1 Vrms, which is less deep than the characteristics sheet's
/// 0.5 Vrms for the 4.7 uF part where both were compared.
#[test]
fn a_capacitor_without_a_curve_is_one_of_the_recorded_cases() {
    let cat = catalog("capacitor");
    let rows = records("jlcpcb/catalog/capacitor.csv");
    let mut without = Vec::new();
    for (c, row) in cat.candidates.iter().zip(&rows) {
        let Some(Value::Derated(d)) = c.column("effective_capacitance") else {
            panic!("{:?}", c.lcsc);
        };
        if !d.curve.is_empty() {
            continue;
        }
        let lcsc = row["lcsc"].clone();
        let samsung = row["mpn"].starts_with("CL");
        let value = &row["capacitance"];
        let class_ii = row["name"].contains("X5R") || row["name"].contains("X7R");
        let microfarads = value.contains("uF") && !value.starts_with("0.");
        let big = value
            .split("uF")
            .next()
            .and_then(|n| n.parse::<f64>().ok())
            .is_some_and(|n| n >= 2.2);
        assert!(
            !samsung || (class_ii && microfarads && big),
            "{lcsc} is a Samsung part without a curve"
        );
        without.push(lcsc);
    }
    // No Murata curve was found (see data/jlcpcb/README.md).
    assert!(
        without.contains(&"C3845593".to_string()),
        "the Murata 22 uF"
    );
    assert!(without.len() < cat.candidates.len() / 2);
    // Each Samsung part with a curve names where it came from (the test below).
    assert!(
        rows.iter()
            .filter(|r| r["source"].contains("bias-curve:"))
            .count()
            > 25
    );
}

#[test]
fn a_curve_names_where_it_came_from_and_stops_at_the_rated_voltage() {
    for row in records("jlcpcb/catalog/capacitor.csv") {
        let lcsc = &row["lcsc"];
        let d = curve_of(lcsc);
        if d.curve.is_empty() {
            continue;
        }
        let source = &row["source"];
        assert!(
            source.contains("bias-curve: samsung-componentcenter:https://")
                || source.contains("bias-curve: samsung-characteristics-sheet:https://"),
            "{lcsc}: {source}"
        );
        assert!(source.contains("; measured "), "{lcsc}: {source}");
        // The curve stops at the rated voltage, so a part is never trusted past it.
        let rated: f64 = row["max_voltage"].trim_end_matches('V').parse().unwrap();
        assert_eq!(d.curve.last().unwrap().0, rated, "{lcsc}");
    }
}

#[test]
fn the_4u7_10v_capacitor_keeps_about_a_third_at_3v3_and_a_fifth_at_5v() {
    // Samsung's own sheet for CL05A475MP5NRNC: about -65% at 3.5 V and -78% at 5.25 V.
    let d = curve_of("C23733");
    let out = d.at(&bias(3.135, 3.465)).unwrap();
    assert!(out.lo() > 1.2e-6 && out.lo() < 1.25e-6, "{out}");
    assert!(out.hi() < 2.4e-6, "{out}");
    let input = d.at(&bias(4.75, 5.25)).unwrap();
    assert!(input.lo() < 0.8e-6, "{input}");
}

#[test]
fn the_1u_25v_capacitor_loses_over_half_at_5v() {
    let d = curve_of("C52923");
    let q = d.at(&bias(4.75, 5.25)).unwrap();
    assert!(q.lo() < 0.5e-6, "{q}");
    // It keeps nearly everything below 1 V.
    assert!(d.at(&bias(0.0, 0.5)).unwrap().lo() > 0.85e-6);
}

fn led_by(lcsc: &str) -> Candidate {
    catalog("led")
        .candidates
        .iter()
        .find(|c| c.lcsc.as_deref() == Some(lcsc))
        .unwrap_or_else(|| panic!("{lcsc} is not offered"))
        .clone()
}

fn vf_of(c: &Candidate) -> (f64, f64) {
    match c.column("forward_voltage") {
        Some(Value::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_leds_have_the_datasheet_forward_voltage_color_and_package() {
    let cat = catalog("led");
    // Basic first: the red and the white KENTO parts, by price.
    assert_eq!(&lcscs("led")[..2], ["C2286", "C2290"]);
    assert!(
        cat.candidates[..2]
            .iter()
            .all(|c| c.library == Some(Library::Basic))
    );
    assert!(
        cat.candidates[2..]
            .iter()
            .all(|c| c.library == Some(Library::Extended))
    );
    assert_eq!(vf_of(&led_by("C2286")), (1.8, 2.4));
    assert_eq!(vf_of(&led_by("C125094")), (1.9, 2.4));
    assert_eq!(vf_of(&led_by("C125103")), (2.65, 3.05));
    assert_eq!(vf_of(&led_by("C965799")), (1.9, 2.2));
    assert_eq!(vf_of(&led_by("C965804")), (2.6, 3.4));
    // The yellow-green and the emerald green are both `Green`.
    for (lcsc, wanted, package) in [
        ("C2286", "Red", "0603"),
        ("C25503345", "Red", "0402"),
        ("C125094", "Green", "0603"),
        ("C965804", "Green", "0603"),
        ("C965793", "Green", "0402"),
        ("C965802", "Yellow", "0603"),
        ("C25503856", "Yellow", "0402"),
        ("C125103", "Blue", "0603"),
        ("C22355736", "Blue", "0402"),
        ("C2290", "White", "0603"),
        ("C20613596", "White", "0402"),
    ] {
        let c = led_by(lcsc);
        assert_eq!(c.column("color"), Some(&Value::Text(wanted.into())));
        assert_eq!(c.column("package"), Some(&Value::Text(package.into())));
    }
}

/// Every LED's datasheet is in its source, and its range is the one in its notes, so the
/// numbers the solver uses can be traced to the document they were read from.
#[test]
fn every_led_has_a_datasheet_in_its_source_and_a_range_in_order() {
    for row in records("jlcpcb/catalog/led.csv") {
        let lcsc = &row["lcsc"];
        let c = led_by(lcsc);
        let (lo, hi) = vf_of(&c);
        assert!(lo < hi, "{lcsc}");
        assert!(
            row["source"].contains("datasheet:https://"),
            "{lcsc} has no datasheet in its source"
        );
        assert!(
            row["notes"].contains("datasheet minimum to maximum at the test current"),
            "{lcsc}"
        );
    }
}

#[test]
fn the_only_regulator_is_the_xc6206_in_sot23() {
    let cat = catalog("ldo");
    assert_eq!(cat.candidates.len(), 1);
    let c = &cat.candidates[0];
    assert_eq!(c.lcsc.as_deref(), Some("C5446"));
    assert_eq!(c.mpn.as_deref(), Some("XC6206P332MR-G"));
    assert_eq!(c.library, Some(Library::Basic));
    assert_eq!(
        c.column("package"),
        Some(&Value::Text("SOT-23".to_string()))
    );
    // The pads are the part's pin table's, not the catalog's (ADR 0016).
    assert!(c.columns.iter().all(|(k, _)| !k.ends_with(".pad")));
    let q = |k: &str| match c.column(k) {
        Some(Value::Quantity(q)) => *q,
        other => panic!("{k}: {other:?}"),
    };
    assert_eq!(q("dropout"), quantity(0.68, "V"));
    assert_eq!(q("max_current"), quantity(0.1, "A"));
    // The 250 mW is the SOT-23 figure the row's notes took from the datasheet.
    assert_eq!(q("max_power"), quantity(0.25, "W"));
    assert!(records("jlcpcb/catalog/ldo.csv")[0]["notes"].contains("SOT-23 Pd 250mW"));
    assert_eq!(q("vout.voltage"), tolerance(3.3, "V", 5.0));
}

#[test]
fn the_sample_catalogs_are_a_full_e24_set_and_ascending() {
    let sample = Catalogs::sample();
    let resistors = sample
        .resolve("resistor", &columns("resistor"), &[], &[])
        .unwrap()
        .unwrap();
    let values: Vec<f64> = resistors
        .candidates
        .iter()
        .map(|c| match c.column("resistance") {
            Some(Value::Quantity(q)) => q.mid(),
            _ => panic!(),
        })
        .collect();
    assert!(values.windows(2).all(|w| w[0] < w[1]), "ascending");
    assert!(values.contains(&300.0));
    assert_eq!(resistors.candidates.len(), 24 * 7);
    assert!(resistors.candidates.iter().all(|c| c.lcsc.is_none()));
    assert!(sample.resolve("led", &columns("led"), &[], &[]).is_none());
    // `3.3V +/- 2%` in source and the sample's 3.3 V regulator are the same interval, or the
    // regulator would never match.
    let ldos = sample
        .resolve("ldo", &columns("ldo"), &[], &[])
        .unwrap()
        .unwrap();
    let Some(Value::Quantity(v)) = ldos.candidates[2].column("vout.voltage") else {
        panic!()
    };
    assert_eq!(*v, tolerance(3.3, "V", 2.0));
}

#[test]
fn a_candidate_is_what_the_catalog_says_and_nothing_else() {
    let c: Candidate = catalog("led").candidates[0].clone();
    assert_eq!(c.name, "KT-0603R RED LED 0603 C2286");
    assert_eq!(c.label, "KT-0603R");
}

#[test]
fn every_resistor_has_the_power_both_suppliers_list() {
    // The rating is the `Power(Watts)` of the JLCPCB record, read again on 2026-11-01 and
    // checked against the LCSC record. The name of the part in the notes says the same.
    let rows = records("jlcpcb/catalog/resistor.csv");
    assert!(!rows.is_empty());
    for row in &rows {
        let power = row["max_power"].as_str();
        assert!(!power.is_empty(), "{} has no max_power", row["lcsc"]);
        assert!(
            row["source"]
                .contains("max_power: Power(Watts) of the JLCPCB record and of the LCSC record"),
            "{} does not say where its max_power was read",
            row["lcsc"]
        );
        assert!(
            row["notes"].contains(power),
            "{}: the notes say {} but max_power is {power}",
            row["lcsc"],
            row["notes"]
        );
        let expected = match row["package"].as_str() {
            "0402" => ["62.5mW", "63mW"].as_slice(),
            "0603" => ["100mW"].as_slice(),
            "0805" => ["250mW"].as_slice(),
            other => panic!("{other}"),
        };
        assert!(expected.contains(&power), "{}: {power}", row["lcsc"]);
    }
}

/// The kinds of ADR 0023 that gained a catalog (ADR 0025), with the file that holds it.
const STD_KINDS: [(&str, &str); 9] = [
    ("schottky", "Schottky"),
    ("zener", "Zener"),
    ("tvs", "Tvs"),
    ("tvs_bidirectional", "TvsBidirectional"),
    ("ferrite_bead", "FerriteBead"),
    ("inductor", "Inductor"),
    ("fuse", "Fuse"),
    ("n_mosfet", "NMosfet"),
    ("crystal", "Crystal"),
];

#[test]
fn every_row_of_a_standard_kind_has_a_package_that_the_kicad_mapping_has_a_footprint_for() {
    // A catalog row in a package the mapping does not know would be picked by the solver
    // and stop `build` at the export.
    let mapping = fs::read_to_string(data("../std/kicad.toml")).unwrap();
    let blocks: Vec<&str> = mapping.split("[[part]]").collect();
    for (name, kind) in STD_KINDS {
        for row in records(&format!("jlcpcb/catalog/{name}.csv")) {
            let package = row["package"].as_str();
            let known = blocks.iter().any(|b| {
                b.contains(&format!("kind = \"{kind}\""))
                    && b.contains(&format!("package = \"{package}\""))
            });
            assert!(
                known,
                "{name} {}: no `{kind}` mapping for {package}",
                row["lcsc"]
            );
        }
    }
}

#[test]
fn the_standard_kind_rows_say_where_every_number_was_read() {
    for (name, _) in STD_KINDS {
        for row in records(&format!("jlcpcb/catalog/{name}.csv")) {
            let lcsc = &row["lcsc"];
            assert!(
                row["source"].contains("read 2026-11-"),
                "{name} {lcsc}: {}",
                row["source"]
            );
            assert!(
                row["source"].contains("datasheet:https://"),
                "{name} {lcsc} cites no datasheet"
            );
            assert!(
                row["notes"].contains("JLCPCB lists") && row["notes"].contains("LCSC cross-check"),
                "{name} {lcsc}"
            );
        }
    }
}

fn row_of(name: &str, lcsc: &str) -> Candidate {
    catalog(name)
        .candidates
        .into_iter()
        .find(|c| c.lcsc.as_deref() == Some(lcsc))
        .unwrap_or_else(|| panic!("{lcsc} is not in {name}"))
}

fn value(c: &Candidate, key: &str) -> Quantity {
    match c.column(key) {
        Some(Value::Quantity(q)) => *q,
        other => panic!("{key}: {other:?}"),
    }
}

#[test]
fn the_schottky_diodes_carry_the_datasheet_ratings_and_the_ss_series_is_basic() {
    let ss14 = row_of("schottky", "C2480");
    assert_eq!(ss14.mpn.as_deref(), Some("SS14"));
    assert_eq!(ss14.library, Some(Library::Basic));
    assert_eq!(value(&ss14, "forward_voltage"), quantity(0.55, "V"));
    assert_eq!(value(&ss14, "max_reverse_voltage"), quantity(40.0, "V"));
    assert_eq!(value(&ss14, "max_forward_current"), quantity(1.0, "A"));
    let ss34 = row_of("schottky", "C8678");
    assert_eq!(value(&ss34, "max_forward_current"), quantity(3.0, "A"));
    assert_eq!(ss34.column("package"), Some(&Value::Text("SMA".into())));
    // The SS3H10 (100 V, 3 A, SMC) is the one Extended Schottky, read from Vishay's datasheet
    // 88752 (issue 179); the rest are Basic.
    let ss3h10 = row_of("schottky", "C467997");
    assert_eq!(ss3h10.mpn.as_deref(), Some("SS3H10-E3/9AT"));
    assert_eq!(ss3h10.library, Some(Library::Extended));
    assert_eq!(value(&ss3h10, "forward_voltage"), quantity(0.8, "V"));
    assert_eq!(value(&ss3h10, "max_reverse_voltage"), quantity(100.0, "V"));
    assert_eq!(value(&ss3h10, "max_forward_current"), quantity(3.0, "A"));
    assert_eq!(ss3h10.column("package"), Some(&Value::Text("SMC".into())));
    assert!(
        catalog("schottky")
            .candidates
            .iter()
            .filter(|c| c.lcsc.as_deref() != Some("C467997"))
            .all(|c| c.library == Some(Library::Basic))
    );
}

#[test]
fn a_zener_row_is_the_range_of_the_datasheet_at_the_test_current() {
    let z = row_of("zener", "C2117");
    assert_eq!(z.mpn.as_deref(), Some("BZT52C5V1"));
    let v = value(&z, "zener_voltage");
    assert_eq!((v.lo(), v.hi()), (4.8, 5.4));
    assert_eq!(value(&z, "max_power"), quantity(0.5, "W"));
}

/// The BZT52C39 (C2111) is the range of the same datasheet table at its 2 mA test current.
#[test]
fn the_bzt52c39_is_37_to_41_volts_at_2_milliamps() {
    let z = row_of("zener", "C2111");
    assert_eq!(z.mpn.as_deref(), Some("BZT52C39"));
    assert_eq!(z.library, Some(Library::Extended));
    let v = value(&z, "zener_voltage");
    assert_eq!((v.lo(), v.hi()), (37.0, 41.0));
    assert_eq!(value(&z, "max_power"), quantity(0.5, "W"));
    assert_eq!(z.column("package"), Some(&Value::Text("SOD-123".into())));
}

/// Every standard Zener row that states a figure the clamp over temperature reads cites the
/// datasheet in its source and the table the figures come from in its notes, and states all
/// four together: a half-filled row would be offered to a design that asks for one of them
/// and read wrongly by one that asks for another.
#[test]
fn every_zener_row_with_a_temperature_figure_cites_the_datasheet_table() {
    let columns = [
        "test_current",
        "temperature_coefficient",
        "leakage_current",
        "leakage_test_voltage",
    ];
    let mut stating = 0;
    for row in records("jlcpcb/catalog/zener.csv") {
        let lcsc = &row["lcsc"];
        let filled = columns.iter().filter(|c| !row[**c].is_empty()).count();
        if filled == 0 {
            continue;
        }
        stating += 1;
        assert_eq!(filled, columns.len(), "{lcsc} states only some of the four");
        assert!(
            row["source"].contains("datasheet:https://"),
            "{lcsc} states temperature figures and cites no datasheet"
        );
        assert!(
            row["notes"].contains("ELECTRICAL CHARACTERISTICS table (page 2"),
            "{lcsc}: the notes do not say which table the figures were read from"
        );
    }
    assert_eq!(stating, 8);
}

/// The figures are the datasheet's, each read from the ELECTRICAL CHARACTERISTICS table of
/// the Changjing BZT52C2V4-BZT52C43 datasheet (Rev. - 2.1, page 2): IZT, the typical
/// temperature coefficient (min to max, mV/C), IR at VR.
#[test]
fn the_zener_rows_state_the_datasheet_test_current_coefficient_and_leakage() {
    let cases = [
        ("C2112", (-0.0035, 0.0), (5e-6, 1.0)),
        ("C2113", (-0.0035, 0.0), (5e-6, 1.0)),
        ("C2117", (-0.0027, 0.0012), (2e-6, 2.0)),
        ("C2123", (0.0038, 0.007), (0.5e-6, 6.0)),
        ("C43491", (0.006, 0.010), (0.1e-6, 8.0)),
        ("C2104", (0.0092, 0.013), (0.1e-6, 10.5)),
        ("C2109", (0.0184, 0.022), (0.1e-6, 16.8)),
        ("C2111", (0.0334, 0.0412), (0.1e-6, 27.3)),
    ];
    for (lcsc, (lo, hi), (leak, at)) in cases {
        let z = row_of("zener", lcsc);
        // The types from BZT52C27 to BZT52C39 are tested at 2 mA, the others at 5 mA.
        let izt = if lcsc == "C2111" { 2.0 } else { 5.0 };
        assert_eq!(value(&z, "test_current"), quantity(izt, "mA"), "{lcsc}");
        let c = value(&z, "temperature_coefficient");
        assert!(
            (c.lo() - lo).abs() < 1e-12 && (c.hi() - hi).abs() < 1e-12,
            "{lcsc}: {c:?}"
        );
        let i = value(&z, "leakage_current");
        assert!((i.lo() - leak).abs() < 1e-12, "{lcsc}: {i:?}");
        assert_eq!(
            value(&z, "leakage_test_voltage"),
            quantity(at, "V"),
            "{lcsc}"
        );
    }
}

#[test]
fn a_tvs_row_has_the_standoff_the_clamp_at_the_pulse_current() {
    let t = row_of("tvs", "C113957");
    assert_eq!(t.mpn.as_deref(), Some("SMAJ12A"));
    assert_eq!(value(&t, "standoff_voltage"), quantity(12.0, "V"));
    assert_eq!(value(&t, "clamping_voltage"), quantity(19.9, "V"));
    assert_eq!(value(&t, "peak_pulse_current"), quantity(20.1, "A"));
}

/// The Littelfuse SMDJ36A (C108363), read from the Littelfuse SMDJ series datasheet: the one SMC
/// row, and the one row that states a minimum breakdown voltage (40 V at 1 mA).
#[test]
fn the_smdj36a_carries_the_littelfuse_table_row() {
    let t = row_of("tvs", "C108363");
    assert_eq!(t.mpn.as_deref(), Some("SMDJ36A"));
    assert_eq!(t.library, Some(Library::Extended));
    assert_eq!(value(&t, "standoff_voltage"), quantity(36.0, "V"));
    assert_eq!(value(&t, "breakdown_voltage"), quantity(40.0, "V"));
    assert_eq!(value(&t, "clamping_voltage"), quantity(58.1, "V"));
    assert_eq!(value(&t, "peak_pulse_current"), quantity(51.6, "A"));
    assert_eq!(t.column("package"), Some(&Value::Text("SMC".into())));
}

/// The two Littelfuse bidirectional rows (ADR 0089), read from the table of the Littelfuse SMDJ
/// series datasheet, where each is the "Bi" part number beside the unidirectional one of the
/// same line. Both state the minimum breakdown voltage, as the datasheet table does.
#[test]
fn the_bidirectional_tvs_rows_carry_the_littelfuse_table_lines() {
    for (lcsc, mpn, vr, vbr, vc, ipp) in [
        ("C224064", "SMDJ12CA", 12.0, 13.3, 19.9, 150.8),
        ("C224075", "SMDJ36CA", 36.0, 40.0, 58.1, 51.6),
    ] {
        let t = row_of("tvs_bidirectional", lcsc);
        assert_eq!(t.mpn.as_deref(), Some(mpn));
        assert_eq!(t.library, Some(Library::Extended));
        assert_eq!(value(&t, "standoff_voltage"), quantity(vr, "V"), "{mpn}");
        assert_eq!(value(&t, "breakdown_voltage"), quantity(vbr, "V"), "{mpn}");
        assert_eq!(value(&t, "clamping_voltage"), quantity(vc, "V"), "{mpn}");
        assert_eq!(value(&t, "peak_pulse_current"), quantity(ipp, "A"), "{mpn}");
        assert_eq!(t.column("package"), Some(&Value::Text("SMC".into())));
    }
}

#[test]
fn the_inductors_give_the_saturation_current_and_the_largest_resistance() {
    let l = row_of("inductor", "C285758");
    assert_eq!(l.mpn.as_deref(), Some("MHCD322512A-2R2M-A8S"));
    assert_eq!(value(&l, "saturation_current"), quantity(3.6, "A"));
    assert_eq!(value(&l, "dc_resistance"), quantity(0.085, "ohm"));
    assert_eq!(value(&l, "inductance"), tolerance(2.2, "uH", 20.0));
    let m = row_of("inductor", "C337905");
    assert_eq!(value(&m, "saturation_current"), quantity(1.25, "A"));
    assert_eq!(value(&m, "inductance"), tolerance(3.3, "uH", 30.0));
}

/// The molded 1210 inductors that a buck above 5.6 uH and 1 A needs (issue #703): the rating a
/// row states is the datasheet's maximum column (Isat for a 30 % drop, the largest Rdc), and not
/// the typical figure that the LCSC record lists as `Isat`.
#[test]
fn the_molded_1210_inductors_give_the_datasheets_maximum_isat_and_rdc() {
    // (lcsc, mpn, microhenries, Isat max, Rdc max)
    let rows = [
        ("C17701262", "MTQH322512S4R7MBT", 4.7, 2.6, 0.135),
        ("C17701263", "MTQH322512S6R8MBT", 6.8, 2.4, 0.21),
        ("C17701264", "MTQH322512S100MBT", 10.0, 1.9, 0.23),
        ("C39846661", "XRTC322512S100MBCA", 10.0, 1.9, 0.23),
        ("C48945755", "ZETC322512S4R7MBT-LF", 4.7, 2.8, 0.135),
        ("C48945791", "ZETC322512S6R8MBT-LF", 6.8, 2.6, 0.21),
        ("C48945862", "ZEPIM322520T220MLF", 22.0, 1.8, 0.364),
    ];
    for (lcsc, mpn, uh, isat, rdc) in rows {
        let r = row_of("inductor", lcsc);
        assert_eq!(r.mpn.as_deref(), Some(mpn));
        assert_eq!(value(&r, "inductance"), tolerance(uh, "uH", 20.0), "{mpn}");
        assert_eq!(value(&r, "saturation_current"), quantity(isat, "A"), "{mpn}");
        assert_eq!(value(&r, "dc_resistance"), quantity(rdc, "ohm"), "{mpn}");
        assert_eq!(r.column("package"), Some(&Value::Text("1210".into())));
    }
}

/// Every standard capacitor row states the class its listing prints, as a variant of
/// `Dielectric` (ADR 0055): the class is also in the row's `name`, and the two agree.
#[test]
fn every_standard_capacitor_states_the_class_its_listing_prints() {
    let cat = catalog("capacitor");
    let mut seen = std::collections::BTreeSet::new();
    for c in &cat.candidates {
        let Some(Value::Text(class)) = c.column("dielectric") else {
            panic!("{} has no dielectric", c.name);
        };
        assert!(
            ["C0G", "X5R", "X7R"].contains(&class.as_str()),
            "{}",
            c.name
        );
        assert!(
            c.name.split_whitespace().any(|w| w == class),
            "{} says {class} in its dielectric but not in its name",
            c.name
        );
        seen.insert(class.clone());
    }
    assert_eq!(seen.len(), 3, "{seen:?}");
}

/// A row's own temperature range is its datasheet's, and a class's range is what the makers
/// print for the class code (ADR 0099): every class a standard capacitor row names has a cited
/// range, and no row's own range reaches beyond its class's (a datasheet that said so would be
/// a row or a class entry to look at again). A row that states a range is judged by it alone.
#[test]
fn every_standard_capacitor_class_has_a_cited_range_that_covers_the_rows_own() {
    let cat = catalog("capacitor");
    for c in &cat.candidates {
        let Some(Value::Text(class)) = c.column("dielectric") else {
            panic!("{} has no dielectric", c.name);
        };
        let range = netsmith_check::environment::class_range(class).unwrap_or_else(|| {
            panic!(
                "{} is of the class {class}, which has no cited range",
                c.name
            )
        });
        assert!(!range.source.is_empty(), "{class}");
        assert_eq!(c.class_range().map(|r| r.class), Some(range.class));
        let (own_min, own_max) = (c.temp_min.unwrap(), c.temp_max.unwrap());
        assert!(
            own_min >= range.min - 1e-9 && own_max <= range.max + 1e-9,
            "{} states {own_min} K to {own_max} K, wider than the class {class} ({} K to {} K)",
            c.name,
            range.min,
            range.max
        );
        // The row's own range is the one used.
        assert_eq!(c.temperatures(), (Some(own_min), Some(own_max)));
    }
}

/// A candidate with a class and no range of its own has the class's; an end it states is its own;
/// a class with no cited range gives nothing.
#[test]
fn a_candidate_takes_the_classs_range_only_where_it_states_none() {
    let with = |class: &str, min: Option<f64>, max: Option<f64>| Candidate {
        columns: vec![("dielectric".into(), Value::Text(class.into()))],
        temp_min: min,
        temp_max: max,
        ..Default::default()
    };
    assert_eq!(
        with("X5R", None, None).temperatures(),
        (Some(218.15), Some(358.15))
    );
    assert_eq!(
        with("X5R", None, Some(398.15)).temperatures(),
        (Some(218.15), Some(398.15))
    );
    assert_eq!(
        with("X7R", Some(233.15), None).temperatures(),
        (Some(233.15), Some(398.15))
    );
    assert_eq!(with("Y5V", None, None).temperatures(), (None, None));
    assert!(with("Y5V", None, None).class_range().is_none());
    assert_eq!(
        Candidate::default().temperatures(),
        (None, None),
        "a row with no class and no range has none"
    );
}

/// The heating current of an inductor is a column that no standard row fills: no value has a
/// recorded rise (ADR 0055).
#[test]
fn no_standard_inductor_states_a_heating_current() {
    for c in &catalog("inductor").candidates {
        assert_eq!(c.column("rms_current"), None, "{}", c.name);
    }
}

/// The minimum breakdown voltage of a TVS diode is a column that one standard row fills: only
/// the SMDJ36A has a datasheet table row read for it (ADR 0073).
#[test]
fn only_the_smdj36a_states_a_breakdown_voltage() {
    let tvs = catalog("tvs");
    assert!(!tvs.candidates.is_empty());
    for c in &tvs.candidates {
        if c.lcsc.as_deref() == Some("C108363") {
            assert!(c.column("breakdown_voltage").is_some(), "{}", c.name);
        } else {
            assert_eq!(c.column("breakdown_voltage"), None, "{}", c.name);
        }
    }
}

#[test]
fn the_crystals_cover_32khz_8_16_and_25_mhz() {
    let f = |lcsc: &str| {
        let q = value(&row_of("crystal", lcsc), "frequency");
        (q.lo(), q.hi())
    };
    // 32.768 kHz over -40..+85 C: +20 ppm and -216 ppm (ADR 0025).
    let (lo, hi) = f("C32346");
    assert!(
        (lo - 32768.0 * (1.0 - 216e-6)).abs() < 0.1 && (hi - 32768.0 * (1.0 + 20e-6)).abs() < 0.1
    );
    for (lcsc, hz) in [("C115962", 8e6), ("C112572", 16e6), ("C112569", 25e6)] {
        let (lo, hi) = f(lcsc);
        assert!(
            (lo - hz * (1.0 - 30e-6)).abs() < 1.0 && (hi - hz * (1.0 + 30e-6)).abs() < 1.0,
            "{lcsc}"
        );
    }
}

#[test]
fn the_fuses_the_beads_and_the_mosfets_have_their_rows() {
    let fuse = row_of("fuse", "C20800");
    assert_eq!(value(&fuse, "rated_current"), quantity(1.0, "A"));
    assert_eq!(value(&fuse, "rated_voltage"), quantity(6.0, "V"));
    let r = value(&fuse, "resistance");
    assert_eq!((r.lo(), r.hi()), (0.05, 0.27));
    let bead = row_of("ferrite_bead", "C14709");
    assert_eq!(bead.library, Some(Library::Basic));
    assert_eq!(value(&bead, "rated_current"), quantity(2.0, "A"));
    // The BLM31PG601SN1L: Murata's reference specification (0.08 ohm initial maximum), which the
    // LCSC and JLCPCB records agree with; the 2009 catalog sheet's 0.09 ohm is in the notes.
    let big = row_of("ferrite_bead", "C16902");
    assert_eq!(big.mpn.as_deref(), Some("BLM31PG601SN1L"));
    assert_eq!(value(&big, "dc_resistance"), quantity(0.08, "ohm"));
    assert_eq!(value(&big, "rated_current"), quantity(1.5, "A"));
    assert_eq!(big.column("package"), Some(&Value::Text("1206".into())));
    let fet = row_of("n_mosfet", "C20917");
    assert_eq!(fet.mpn.as_deref(), Some("AO3400A"));
    let th = value(&fet, "gate_threshold");
    assert_eq!((th.lo(), th.hi()), (0.65, 1.45));
    assert_eq!(row_of("n_mosfet", "C8545").library, Some(Library::Basic));
    // The gate-source voltage each row's on-resistance is guaranteed at (ADR 0092): the test
    // condition of the datasheet's RDS(on) row that `on_resistance` is the maximum of.
    for (lcsc, ohms, volts) in [("C8545", 7.0, 5.0), ("C20917", 0.048, 2.5)] {
        let fet = row_of("n_mosfet", lcsc);
        assert_eq!(value(&fet, "on_resistance"), quantity(ohms, "ohm"));
        assert_eq!(value(&fet, "on_resistance_vgs"), quantity(volts, "V"));
    }
}

#[test]
fn every_header_and_socket_row_states_the_contact_rating_from_its_datasheet() {
    // The catalog rates the contacts of a connector (ADR 0040): one row for each series and
    // pin count, for the single-row 2.54 mm headers and sockets of the standard library.
    let rows = records("jlcpcb/catalog/header.csv");
    let mut found: Vec<(String, String)> = Vec::new();
    for row in &rows {
        let lcsc = &row["lcsc"];
        assert!(
            row["source"].contains("datasheet:https://"),
            "{lcsc} cites no datasheet"
        );
        assert_eq!(row["mating.pitch"], "2.54mm", "{lcsc}");
        assert_eq!(row["mating.rows"], "1", "{lcsc}");
        // The rating of one contact is the datasheet's, and the notes say what it is and what
        // it leaves out.
        assert_eq!(row["mating.pin_current"], "3A", "{lcsc}");
        assert!(
            row["notes"].contains("Current Rating: 3 Amps")
                && row["notes"].contains("JLCPCB lists")
                && row["notes"].contains("LCSC cross-check"),
            "{lcsc}"
        );
        found.push((
            row["mating.gender"].clone(),
            row["mating.positions"].clone(),
        ));
    }
    for gender in ["header", "socket"] {
        for positions in ["2", "4", "8", "10"] {
            assert!(
                found.contains(&(gender.to_string(), positions.to_string())),
                "no {gender} row for {positions} positions"
            );
        }
    }
    // A candidate carries the rating as a quantity the solver can compare.
    let c = row_of("header", "C42431810");
    assert_eq!(value(&c, "mating.pin_current"), quantity(3.0, "A"));
    assert_eq!(c.label, "Conn_01x10");
}

#[test]
fn every_header_and_socket_row_states_its_seat_from_the_makers_drawing() {
    // The seat (ADR 0110): how far above its board a mated connector's partner rests. A
    // header's is the top of its plastic, 2.50 +/- 0.20 mm, and a socket's the top of its body,
    // 8.50 +/- 0.15 mm, each from the drawing its `source` names, so the std pair mates at
    // 10.65 mm to 11.35 mm.
    for row in &records("jlcpcb/catalog/header.csv") {
        let lcsc = &row["lcsc"];
        let (cell, figure) = match row["mating.gender"].as_str() {
            "header" => ("2.5mm +/- 0.2mm", "mating.seat: 2.50 +/- 0.20 mm"),
            "socket" => ("8.5mm +/- 0.15mm", "mating.seat: 8.50 +/- 0.15 mm"),
            other => panic!("{lcsc}: gender {other}"),
        };
        assert_eq!(row["mating.seat"], cell, "{lcsc}");
        let entry = row["source"]
            .split(" ; ")
            .find(|e| e.starts_with("mating.seat:"))
            .unwrap_or_else(|| panic!("{lcsc} has no `mating.seat:` source"));
        assert!(
            entry.starts_with(figure) && entry.contains("drawing") && entry.contains("side view"),
            "{lcsc}: {entry}"
        );
    }
    let header = value(&row_of("header", "C42431798"), "mating.seat");
    let socket = value(&row_of("header", "C42431841"), "mating.seat");
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(
        near(header.lo(), 2.3e-3) && near(header.hi(), 2.7e-3),
        "{header}"
    );
    assert!(
        near(socket.lo(), 8.35e-3) && near(socket.hi(), 8.65e-3),
        "{socket}"
    );
    assert!(near(header.lo() + socket.lo(), 10.65e-3));
    assert!(near(header.hi() + socket.hi(), 11.35e-3));
}
