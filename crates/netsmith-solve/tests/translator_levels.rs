//! `LevelTranslator`'s input and output are `Logic` lines, and its supply draw is the picked
//! row's own (ADR 0066). The rows, the driver and every number here are invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

/// Two invented translators as a project's rows of the standard catalog: a 3.3 V one that
/// reads a high from 1.1 V, and a 1.8 V one that survives 2.3 V on its input.
const ROWS: &str = "\
mpn,name,vcc_min,vcc_max,vcc_abs_max,a_abs_max,vt_plus_max,vt_minus_min,max_output_current,max_supply_current,package,library,source
ZT-17,ZT-17 translator,2.5V,3.6V,4.2V,4.2V,1.1V,0.45V,3mA,750nA,SOT-23-5,Extended,https://example.invalid/zt-17
ZT-18,ZT-18 translator,1.65V,1.95V,2.5V,2.3V,0.9V,0.3V,2mA,400nA,SOT-23-5,Extended,https://example.invalid/zt-18
";

/// An invented driver that drives within `drop` of its supply and to 0.1 V at the bottom.
const DRIVER: &str = "\
import std::logic::Logic
import std::parts::logic::LevelTranslator
import std::sources::Rail

part Driver {
    .designator &= \"U\"
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.voh &= .vdd.voltage - 0.2V
    .out.vol &= 0.1V
    pins {
        .vdd.pos   power  \"VDD\" \"1\"
        .vdd.neg   power  \"GND\" \"2\"
        .out.line  output \"OUT\" \"3\"
    }
}
";

/// A driver on a rail of `driver` volts, a translator on a rail of `supply` volts, and the
/// driver's output joined to the translator's input. `stress` is the translator's statements.
fn board(driver: &str, supply: &str, stress: &str) -> String {
    format!(
        "{DRIVER}\nboard Shifting {{\n    .low: Rail {{\n        .voltage &= {driver}\n        \
         .pos.net &= \"LOW\"\n        .neg.net &= \"GND\"\n    }}\n    \
         .high: Rail {{\n        .voltage &= {supply}\n        .pos.net &= \"HIGH\"\n        \
         .neg.net &= \"GND\"\n    }}\n    .d: Driver\n    .u: LevelTranslator {{\n{stress}    }}\n    \
         .d.vdd ~ .low\n    .u.vcc ~ .high\n    .d.out.line ~ .u.a.line\n}}\n"
    )
}

/// The stresses on the 1.8 V row: the supply to 2 V at its peak and the input to 2 V.
const STRESS_LOW: &str =
    "        .vcc_peak &= 2V\n        .a_peak &= 2V\n        .output_current &= 1mA\n";

const STRESS: &str =
    "        .vcc_peak &= 3.6V\n        .a_peak &= 1.98V\n        .output_current &= 1mA\n";

fn run(src: &str) -> Solution {
    let checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file("level_translator", "catalogs/level_translator.csv", ROWS)
        .unwrap();
    let board = checked.standalone_boards().next().expect("one board");
    solve(board, &catalogs)
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

#[test]
fn the_input_takes_its_levels_from_the_rows_thresholds() {
    // A 1.8 V driver, 5 % either way, 0.2 V below its supply: a high of 1.5 V at the least,
    // above the 1.1 V that the row reads as a high for sure; a low of 0.1 V, below 0.45 V.
    let s = run(&board("1.8V +/- 5%", "3.3V +/- 5%", STRESS));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(picked(&s, "u").contains("ZT-17"), "{}", picked(&s, "u"));
    assert_eq!(range(&s, "u.a.vih"), range(&s, "u.vt_plus_max"));
    assert_eq!(range(&s, "u.a.vil"), range(&s, "u.vt_minus_min"));
    assert!((range(&s, "u.a.vih").lo() - 1.1).abs() < 1e-12);
    assert!((range(&s, "u.a.vil").lo() - 0.45).abs() < 1e-12);
    assert!((range(&s, "u.a.line.abs_max").lo() - 4.2).abs() < 1e-12);
}

#[test]
fn a_driver_too_low_for_the_input_is_refused_with_no_statement_of_the_design() {
    // 1.2 V less 5 % less 0.2 V is 0.94 V: not a high for sure to an input that needs 1.1 V.
    let s = run(&board("1.2V +/- 5%", "3.3V +/- 5%", STRESS));
    let m = messages(&s).join("\n");
    assert!(
        m.contains("`d.out.voh` is 940mV..1.06V but `u.a.vih` needs 1.1V"),
        "{m}"
    );
}

#[test]
fn a_high_above_the_inputs_absolute_maximum_is_refused_with_the_numbers() {
    // A 3.3 V driver onto the input of the 1.8 V translator: the row survives 2.3 V there.
    // The design states 2 V as the peak on the input, which the driver does not keep to.
    let s = run(&board("3.3V +/- 5%", "1.8V +/- 5%", STRESS_LOW));
    assert!(picked(&s, "u").contains("ZT-18"), "{}", picked(&s, "u"));
    let m = messages(&s).join("\n");
    assert!(m.contains("u.a.line") && m.contains("2.3V"), "{m}");
}

#[test]
fn the_row_gives_its_supply_current_to_the_rail_with_nothing_stated() {
    let s = run(&board("1.8V +/- 5%", "3.3V +/- 5%", STRESS));
    let draw = range(&s, "u.vcc.current");
    assert!(
        draw.lo() == 0.0 && (draw.hi() - 750e-9).abs() < 1e-15,
        "{draw}"
    );
    // The other row, on the other rail, gives its own.
    let s = run(&board("1.2V +/- 5%", "1.8V +/- 5%", STRESS_LOW));
    assert!(picked(&s, "u").contains("ZT-18"));
    let draw = range(&s, "u.vcc.current");
    assert!(
        draw.lo() == 0.0 && (draw.hi() - 400e-9).abs() < 1e-15,
        "{draw}"
    );
}

#[test]
fn a_draw_the_design_states_is_narrowed_to_the_rows_and_one_wholly_above_it_is_refused() {
    // A wider statement than the datasheet's maximum is narrowed to it.
    let wide = format!("{STRESS}        .vcc.current &= 0A..2uA\n");
    let s = run(&board("1.8V +/- 5%", "3.3V +/- 5%", &wide));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!((range(&s, "u.vcc.current").hi() - 750e-9).abs() < 1e-15);
    // A narrower one is the design's claim and stays.
    let narrow = format!("{STRESS}        .vcc.current &= 0A..500nA\n");
    let s = run(&board("1.8V +/- 5%", "3.3V +/- 5%", &narrow));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!((range(&s, "u.vcc.current").hi() - 500e-9).abs() < 1e-15);
    // A draw that no row can have: no row serves it.
    let above = format!("{STRESS}        .vcc.current &= 2uA..3uA\n");
    let s = run(&board("1.8V +/- 5%", "3.3V +/- 5%", &above));
    let m = messages(&s).join("\n");
    assert!(m.contains("u.vcc.current"), "{m}");
}

#[test]
fn the_output_states_no_level_so_a_receiver_on_it_is_not_evaluated() {
    let src = format!(
        "{}\npart Reader {{\n    .designator &= \"U\"\n    .vdd: Rail\n    .in: Logic\n    \
         .in.reference ~ .vdd\n    .in.vih &= 2V\n    .in.vil &= 0.8V\n    pins {{\n        \
         .vdd.pos  power \"VDD\" \"1\"\n        .vdd.neg  power \"GND\" \"2\"\n        \
         .in.line  input \"IN\"  \"3\"\n    }}\n}}\n",
        board("1.8V +/- 5%", "3.3V +/- 5%", STRESS)
            .replace(
                "    .d.out.line ~ .u.a.line\n}\n",
                "    .d.out.line ~ .u.a.line\n    .r: Reader\n    .r.vdd ~ .high\n    .u.y.line ~ .r.in.line\n}\n"
            )
    );
    let s = run(&src);
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    let open: Vec<String> = s.not_evaluated.iter().map(|n| n.message.clone()).collect();
    assert!(
        open.iter().any(|m| m.contains("u.y.voh")),
        "the output's level is named as missing: {open:#?}"
    );
}

#[test]
fn a_supply_that_may_give_less_than_the_row_draws_is_refused_with_the_numbers() {
    let src = board("1.8V +/- 5%", "3.3V +/- 5%", STRESS).replace(
        "        .pos.net &= \"HIGH\"\n",
        "        .pos.net &= \"HIGH\"\n        .current &= -500nA..0A\n",
    );
    assert!(src.contains("-500nA"));
    let s = run(&src);
    let m = messages(&s).join("\n");
    assert!(
        m.contains("`high.current` must stay within -500nA..0A, but the others on it work out to -750nA..0A"),
        "{m}"
    );
}
