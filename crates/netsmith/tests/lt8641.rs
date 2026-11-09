//! `std::parts::regulator::Lt8641`, the LT8641 buck converter IC (Analog Devices, datasheet
//! Rev. C), built on `Buck` and `BuckStage` (ADR 0025). The numbers asserted here are the
//! datasheet's, as the part's doc comment cites them. A board names its ordering code from
//! its own catalog row, because the standard `lt8641` catalog has no row (no stock above
//! zero is recorded): the one here is a project's file, whose `source` is the datasheet's
//! Order Information table.
//!
//! Nothing here builds or exports, so nothing needs KiCad's libraries.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

const REGULATOR: &str = include_str!("../../../std/parts/regulator.nsm");
const STANDARD_CATALOG: &str = include_str!("../../../data/jlcpcb/catalog/lt8641.csv");

/// A project's rows for the `lt8641` catalog: the ordering code of the datasheet's table.
const ROWS: &str = "\
mpn,package,library,source
LT8641IUDC#WPBF,UDC20(18),Extended,\"datasheet Rev. C, Order Information p.2 (automotive products)\"
";

/// A project's own inductor, invented: the four of the standard catalog all have a DC
/// resistance over the 0.04 ohm that the datasheet asks for (p.16), so no stage around the IC
/// can be solved with them alone.
const INDUCTORS: &str = "\
mpn,name,inductance,saturation_current,dc_resistance,package,library,source
ZL-3R3,ZL-3R3 3.3uH 4.5A inductor 1210,3.3uH +/- 20%,4.5A,0.03ohm,1210,Extended,https://example.invalid/zl-3r3
";

fn solved(src: &str) -> (Solution, String) {
    let checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file("lt8641", "catalogs/lt8641.csv", ROWS)
        .unwrap();
    catalogs
        .add_file("inductor", "catalogs/inductor.csv", INDUCTORS)
        .unwrap();
    let solution = solve(board, &catalogs);
    let text = solution
        .diagnostics
        .iter()
        .map(|d| d.render(&checked.sources))
        .collect::<String>();
    (solution, text)
}

fn range(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

/// A 12 V automotive-style input (9 V to 16 V, a 36 V peak) to `vout`, with the IC around a
/// `BuckStage`, a 47 kohm resistor on RT (0.89 MHz, Equation 3), and `extra` inside the board.
fn board(vout: &str, load: &str, extra: &str) -> String {
    format!(
        "import std::parts::passive::{{Capacitor, Resistor}}\n\
         import std::parts::regulator::{{BuckStage, Lt8641}}\n\
         import std::sources::Rail\n\n\
         board Converter {{\n\
         \x20   .input: Rail {{\n        .voltage &= 9V..16V\n        .peak &= 36V\n        \
         .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }}\n\
         \x20   .output: Rail {{\n        .pos.net &= \"VOUT\"\n        .neg.net &= \"GND\"\n        \
         .current &= {load}\n    }}\n\
         \x20   .u: Lt8641 {{ .mpn &= \"LT8641IUDC#WPBF\" }}\n\
         \x20   .stage: BuckStage {{ .vout.voltage &= {vout} }}\n\
         \x20   .rt: Resistor {{ .resistance &= 47kohm +/- 1% }}\n\
         \x20   .boost: Capacitor {{ .capacitance &= 100nF +/- 20% }}\n\
         \x20   .intvcc_cap: Capacitor {{ .capacitance &= 1uF +/- 20% }}\n\n\
         \x20   .input ~ .stage.vin\n    .input ~ .u.vin\n    .stage.vout ~ .output\n    \
         .stage.buck ~ .u.buck\n    .u.en_uv ~ .input.pos\n    .u.bias ~ .output.pos\n    \
         .u.bias_voltage == .output.voltage\n    .u.sync_mode ~ .input.neg\n    \
         .u.rt ~> .rt ~> .input.neg\n    .u.rt_resistance == .rt.resistance\n    \
         .u.bst ~> .boost ~> .u.buck.sw.pos\n    .u.intvcc ~> .intvcc_cap ~> .input.neg\n\
         {extra}}}\n"
    )
}

fn converter() -> String {
    board("3.3V +/- 4%", "0A..2A", "")
}

#[test]
fn the_pin_table_is_the_datasheets_pin_function_table() {
    let from = REGULATOR.find("part Lt8641 {").expect("the part");
    let table = &REGULATOR[from..];
    let table = &table[table.find("pins {").expect("a pin table")..];
    let table = &table[..table.find('}').unwrap()];
    // (pin, kind, datasheet name, pads), Pin Functions p.10 and p.11: pads 5 and 12 are
    // not present, 21 and 22 are the exposed pads, which are SW.
    let expected = [
        (".bias", "input", "BIAS", "1"),
        (".intvcc", "passive", "INTVCC", "2"),
        (".bst", "passive", "BST", "3"),
        (".vin.pos", "power", "VIN1", "4"),
        (".gnd1", "power", "GND1", "6 7"),
        (".buck.sw.pos", "power", "SW", "8 9 21 22"),
        (".gnd2", "power", "GND2", "10 11"),
        (".vin2", "power", "VIN2", "13"),
        (".en_uv", "input", "EN/UV", "14"),
        (".rt", "passive", "RT", "15"),
        (".tr_ss", "passive", "TR/SS", "16"),
        (".sync_mode", "input", "SYNC/MODE", "17"),
        (".vin.neg", "power", "GND", "18"),
        (".pg", "output", "PG", "19"),
        (".buck.fb", "input", "FB", "20"),
    ];
    let rows: Vec<(String, String, String, String)> = table
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('.'))
        .map(|l| {
            let words: Vec<&str> = l.split_whitespace().collect();
            let pads: Vec<String> = words[3..]
                .iter()
                .take_while(|w| w.starts_with('"'))
                .map(|w| w.trim_matches('"').to_string())
                .collect();
            (
                words[0].to_string(),
                words[1].to_string(),
                words[2].trim_matches('"').to_string(),
                pads.join(" "),
            )
        })
        .collect();
    assert_eq!(rows.len(), expected.len(), "{rows:#?}");
    for (row, want) in rows.iter().zip(expected) {
        assert_eq!(
            (
                row.0.as_str(),
                row.1.as_str(),
                row.2.as_str(),
                row.3.as_str()
            ),
            want
        );
    }
    // Twenty positions less the two that are removed.
    let pads: usize = rows.iter().map(|r| r.3.split(' ').count()).sum();
    assert_eq!(pads, 20, "18 leads and 2 exposed pads");
}

#[test]
fn the_standard_catalog_has_a_header_and_no_row() {
    // No row of it has a stock above zero on a record that was read (data/jlcpcb/README.md).
    assert_eq!(STANDARD_CATALOG.lines().count(), 1, "{STANDARD_CATALOG}");
    assert!(STANDARD_CATALOG.starts_with("lcsc,mpn,name,label,package,library,"));
}

#[test]
fn a_stage_around_it_solves_for_a_12_v_input_and_3_3_v_out() {
    let (s, text) = solved(&converter());
    assert!(text.is_empty(), "{text}");
    // 0.79 V to 0.822 V (p.3) and the datasheet's frequency from RT = 41.2 kohm, 1 %:
    // 46.5 / (41.2 + 5.2) = 1.002 MHz, over the accuracy of 0.93 to 1.09.
    let vref = range(&s, "stage.divider.vref");
    assert!(
        (vref.lo() - 0.79).abs() < 1e-9 && (vref.hi() - 0.822).abs() < 1e-9,
        "{vref}"
    );
    let fsw = range(&s, "u.fsw");
    assert!(
        fsw.lo() > 0.81e6 && fsw.lo() < 0.83e6 && fsw.hi() > 0.97e6 && fsw.hi() < 0.99e6,
        "{fsw}"
    );
    // The input current: 2 A at 3.432 V (3.3 V + 4 %) from 9 V at the least efficiency, 70 %, is the most
    // it draws.
    let drawn = range(&s, "u.vin.current");
    assert!(
        drawn.lo() <= 1e-9 && (drawn.hi() - 2.0 * 3.432 / (9.0 * 0.70)).abs() < 5e-3,
        "{drawn}"
    );
    // The stage's input capacitor is rated for the 36 V peak, and keeps 2.2 uF at 16 V.
    assert!(range(&s, "stage.input_cap.max_voltage").lo() >= 36.0 - 1e-9);
    let cin = range(&s, "stage.input_cap.effective_capacitance");
    assert!(cin.lo() * range(&s, "stage.input_count").lo() >= 2.2e-6 - 1e-12);
    // The output asks for 47 uF, as every Typical Application of the datasheet has.
    assert_eq!(range(&s, "u.buck.min_output_capacitance").lo(), 47e-6);
}

#[test]
fn the_stage_asks_for_the_inductance_the_datasheets_equation_10_gives() {
    // 9 V in, 5 V out at about 1 MHz: DC = 0.556, so VIN * (2 DC - 1) / (2.5 fSW) is above
    // zero, and the inductor is at least that. Below DC = 0.5 the equation asks for nothing.
    let (s, text) = solved(&board("5V +/- 4%", "0A..1A", ""));
    assert!(text.is_empty(), "{text}");
    let least = range(&s, "u.buck.min_inductance");
    assert!(least.hi() > 0.0, "{least}");
    let l = range(&s, "stage.inductor.inductance");
    assert!(l.lo() >= least.hi() - 1e-12, "{l} against {least}");
}

#[test]
fn the_frequency_follows_the_resistor_on_rt_and_is_refused_outside_what_the_datasheet_gives() {
    // 20 kohm is 1.85 MHz (Equation 3), inside the span that is allowed.
    let faster = converter().replace("47kohm", "20kohm");
    let (s, text) = solved(&faster);
    // At that frequency the 3.3 V rail needs 4.8 V at least to hold it (Equation 5).
    assert!(text.is_empty(), "{text}");
    let fsw = range(&s, "u.fsw");
    assert!(fsw.lo() > 1.65e6 && fsw.hi() < 2.10e6, "{fsw}");
    // 200 kHz is 232 kohm in the table, but the datasheet gives no accuracy for it.
    let slow = converter().replace("47kohm", "232kohm");
    let (_, text) = solved(&slow);
    assert!(
        text.contains("u.rt_resistance must always be at most 60.4kΩ"),
        "{text}"
    );
    // And below 18.2 kohm (3 MHz is 10.7 kohm in the table).
    let (_, text) = solved(&converter().replace("47kohm", "10.7kohm"));
    assert!(
        text.contains("u.rt_resistance must always be at least 18.2kΩ"),
        "{text}"
    );
}

#[test]
fn a_transient_above_the_absolute_maximum_is_refused() {
    let (_, text) = solved(&converter().replace("36V", "70V"));
    assert!(
        text.contains("u.vin.peak must always be at most u.vin_abs_max"),
        "{text}"
    );
}

#[test]
fn an_input_the_ic_cannot_take_is_refused_at_the_stage() {
    let (_, text) = solved(&converter().replace("9V..16V", "9V..70V"));
    assert!(
        text.contains("stage.vin.voltage must always be at most stage.buck.max_input_voltage"),
        "{text}"
    );
}

#[test]
fn a_high_input_to_a_low_output_at_a_high_frequency_is_refused_by_equation_4() {
    // The shortest on-time, 50 ns, allows fSW = (VOUT + 0.15 V) / (50 ns * (VIN - 0.15 V)):
    // 3.58 V from 60 V is 1.2 MHz, under the 1.85 MHz of 20 kohm.
    let src = board("3.3V +/- 4%", "0A..1A", "")
        .replace("47kohm", "20kohm")
        .replace("9V..16V", "9V..60V")
        .replace("36V", "60V");
    let (_, text) = solved(&src);
    assert!(text.contains("u.fsw must always be at most"), "{text}");
}

#[test]
fn a_low_input_to_a_high_output_is_refused_by_equation_5() {
    // 5 V out (5.2 V at most) at about 1 MHz holds its frequency from 6 V (Equation 5): 5.5 V
    // is below it.
    let src = board("5V +/- 4%", "0A..1A", "").replace("9V..16V", "5.5V..16V");
    let (_, text) = solved(&src);
    assert!(
        text.contains("u.vin.voltage must always be at least"),
        "{text}"
    );
}

#[test]
fn a_board_replaces_the_default_output_capacitance() {
    let (s, text) = solved(&board(
        "3.3V +/- 4%",
        "0A..2A",
        "    .u.buck.min_output_capacitance &= 22uF\n",
    ));
    assert!(text.is_empty(), "{text}");
    assert_eq!(range(&s, "u.buck.min_output_capacitance").lo(), 22e-6);
}

#[test]
fn a_load_over_3_5_a_is_refused() {
    let (_, text) = solved(&board("3.3V +/- 4%", "0A..4A", ""));
    assert!(
        text.contains("must always be at most stage.buck.max_output_current"),
        "{text}"
    );
}

#[test]
fn the_pins_the_datasheet_asks_for_must_be_connected() {
    // RT sets the frequency and cannot float (required); EN/UV is tied to VIN when the
    // shutdown is not used.
    let src = converter().replace("    .u.en_uv ~ .input.pos\n", "");
    let checked = check("test.nsm", &src, &Loader::std());
    assert!(!checked.is_ok());
    assert!(checked.render().contains("en_uv"), "{}", checked.render());
}
