//! `std::parts::regulator::Lt8641`, the LT8641 buck converter IC (Analog Devices, datasheet
//! Rev. C), built on `Buck` and `BuckStage` (ADR 0025). The numbers asserted here are the
//! datasheet's, as the part's doc comment cites them. A board names its ordering code from
//! its own catalog row, because the standard `lt8641` catalog has no row (no stock above
//! zero is recorded): the one here is a project's file, whose `source` is the datasheet's
//! Order Information table.
//!
//! The solver tests build nothing and need no KiCad. The last tests build a board around the
//! IC and export it (schematic, library tables, `.kicad_pcb`) with the sample libraries of
//! `netsmith_kicad::testlib`, the generators and netsmith's own footprint library
//! (`std/libs/`, ADR 0087): they need no KiCad either.

use netsmith::{Libraries, build_with};
use netsmith_check::{Domain, Loader, check};
use netsmith_kicad::testlib::{self, TempDir};
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_pcb::{Board, FootprintLibrary, kicad_pcb};
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
    // it draws. The least is the feedback divider's draw, which the DC analysis of the output side
    // gives (the inductor is its winding there, ADR 0076, amendment 6): tens of microamperes.
    let drawn = range(&s, "u.vin.current");
    assert!(
        drawn.lo() <= 1e-3 && (drawn.hi() - 2.0 * 3.432 / (9.0 * 0.70)).abs() < 5e-3,
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

// ---- the footprint and the export (ADR 0087) ----

const OWN_FOOTPRINT: &str = "netsmith:UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA";

/// The stock-like `Device:L` that `testlib`'s sample `Device` library lacks: a vertical
/// two-pin passive, as KiCad draws it.
const INDUCTOR_SYMBOL: &str = "(symbol \"L\" (pin_numbers (hide yes)) (pin_names (offset 0)) \
    (exclude_from_sim no) (in_bom yes) (on_board yes) \
    (property \"Reference\" \"L\" (at 2.032 0 90) (effects (font (size 1.27 1.27)))) \
    (property \"Value\" \"L\" (at 0 0 90) (effects (font (size 1.27 1.27)))) \
    (property \"Footprint\" \"\" (at 0 0 0) (effects (font (size 1.27 1.27)))) \
    (property \"Datasheet\" \"~\" (at 0 0 0) (effects (font (size 1.27 1.27)))) \
    (property \"Description\" \"\" (at 0 0 0) (effects (font (size 1.27 1.27)))) \
    (symbol \"L_0_1\" (rectangle (start -1.016 2.54) (end 1.016 -2.54) \
    (stroke (width 0.254) (type default)) (fill (type none)))) \
    (symbol \"L_1_1\" \
    (pin passive line (at 0 3.81 270) (length 1.27) (name \"~\" (effects (font (size 1.27 1.27)))) \
    (number \"1\" (effects (font (size 1.27 1.27))))) \
    (pin passive line (at 0 -3.81 90) (length 1.27) (name \"~\" (effects (font (size 1.27 1.27)))) \
    (number \"2\" (effects (font (size 1.27 1.27)))))))";

/// The sample symbols of `testlib` and an inductor, the generators for the passives and
/// netsmith's own library, which is in the binary: no KiCad is read. The folder is the
/// test's, removed when `TempDir` goes.
fn libraries(dir: &TempDir) -> Libraries {
    let symbols = dir.path().join("symbols");
    std::fs::create_dir_all(&symbols).unwrap();
    for entry in std::fs::read_dir(testlib::sample_symbols()).unwrap() {
        let path = entry.unwrap().path();
        let file = symbols.join(path.file_name().unwrap());
        let mut text = std::fs::read_to_string(&path).unwrap();
        if path.file_name().unwrap() == "Device.kicad_sym" {
            let end = text
                .trim_end()
                .strip_suffix(')')
                .expect("a library ends in `)`")
                .len();
            text.truncate(end);
            text.push_str(INDUCTOR_SYMBOL);
            text.push_str(")\n");
        }
        std::fs::write(file, text).unwrap();
    }
    Libraries::with_mapping(
        Mapping::standard(),
        SymbolLib::at(symbols),
        FootprintLibrary::at(testlib::sample_footprints()).with_generators(),
    )
}

/// The converter on a two layer board, which a PCB needs.
fn board_with_process() -> String {
    board("3.3V +/- 4%", "0A..2A", "")
        .replace(
            "import std::sources::Rail\n",
            "import std::sources::Rail\nimport std::mfg::jlcpcb::TwoLayer\n",
        )
        .replace(
            "board Converter {\n",
            "board Converter {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n",
        )
}

fn built(libraries: &Libraries) -> netsmith::Built {
    let mut catalogs = Catalogs::jlcpcb();
    catalogs
        .add_file("lt8641", "catalogs/lt8641.csv", ROWS)
        .unwrap();
    catalogs
        .add_file("inductor", "catalogs/inductor.csv", INDUCTORS)
        .unwrap();
    build_with(
        "converter.nsm",
        &board_with_process(),
        &Loader::std(),
        &catalogs,
        libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
}

#[test]
fn the_standard_mapping_gives_the_ic_the_footprint_of_its_own_and_its_symbol() {
    let mapped = Mapping::standard()
        .map("Lt8641", Some("UDC20(18)"))
        .unwrap();
    assert_eq!(mapped.lib_id, "netsmith:LT8641");
    assert_eq!(mapped.footprint, OWN_FOOTPRINT);
    // Both are found with no folder at all.
    let libraries = Libraries::with_mapping(
        Mapping::standard(),
        SymbolLib::at("/nonexistent"),
        FootprintLibrary::at("/nonexistent"),
    );
    assert!(libraries.symbols.symbol(&mapped.lib_id).is_ok());
    assert_eq!(
        libraries
            .footprints
            .load(&mapped.footprint)
            .unwrap()
            .pads
            .len(),
        20
    );
}

#[test]
fn a_board_around_the_ic_builds_with_the_symbol_and_exports_the_footprint_and_the_library() {
    let dir = TempDir::new("lt8641-libraries");
    let libraries = libraries(&dir);
    let b = built(&libraries);
    // The schematic draws the IC with netsmith's symbol, all its pins named as the datasheet does.
    assert!(
        b.schematic.contains("\"netsmith:LT8641\""),
        "the schematic does not hold the symbol"
    );
    for pin in [
        "EN/UV",
        "SYNC/MODE",
        "TR/SS",
        "INTVCC",
        "GND1",
        "GND2",
        "VIN1",
        "VIN2",
    ] {
        assert!(b.schematic.contains(&format!("\"{pin}\"")), "{pin}");
    }
    assert!(libraries.uses_std_library(&b.design));

    // The PCB places the 20 pads of the footprint, and the `.kicad_pcb` carries it by its name.
    let board = Board::from_design(&b.design, &libraries.mapping, &libraries.footprints)
        .unwrap_or_else(|e| panic!("{e}"));
    let pcb = kicad_pcb(&board);
    assert!(
        pcb.contains(OWN_FOOTPRINT),
        "the board does not name the footprint"
    );
    let ic = board
        .parts
        .iter()
        .find(|p| p.footprint.name == "UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA")
        .expect("the IC is on the board");
    assert_eq!(ic.footprint.pads.len(), 20);

    // The output folder gets the library, and KiCad's tables list it like a project's own.
    let out = TempDir::new("lt8641-export");
    let dir = b.write(out.path()).unwrap();
    libraries.write_tables(&dir, &b.design).unwrap();
    let footprint = std::fs::read_to_string(
        dir.join("netsmith.pretty/UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA.kicad_mod"),
    )
    .unwrap();
    assert_eq!(
        footprint,
        include_str!(
            "../../../std/libs/netsmith.pretty/UDC-QFN-18-2EP_3x4mm_P0.5mm_ExpPadAA.kicad_mod"
        )
    );
    assert!(dir.join("netsmith.kicad_sym").is_file());
    let fp_table = std::fs::read_to_string(dir.join("fp-lib-table")).unwrap();
    assert!(
        fp_table.contains("(name \"netsmith\")")
            && fp_table.contains("${KIPRJMOD}/netsmith.pretty"),
        "{fp_table}"
    );
    let sym_table = std::fs::read_to_string(dir.join("sym-lib-table")).unwrap();
    assert!(
        sym_table.contains("${KIPRJMOD}/netsmith.kicad_sym"),
        "{sym_table}"
    );
}

#[test]
fn a_board_without_a_part_of_the_library_gets_no_library_files() {
    let src = "import std::parts::passive::Resistor\n\
               import std::sources::Rail\n\n\
               board Plain {\n\
               \x20   .supply: Rail {\n        .voltage &= 5V +/- 5%\n        .pos.net &= \"VCC\"\n        .neg.net &= \"GND\"\n    }\n\
               \x20   .r: Resistor { .resistance &= 1kohm +/- 1% }\n\
               \x20   .supply.pos ~> .r ~> .supply.neg\n}\n";
    let dir = TempDir::new("plain-libraries");
    let libraries = libraries(&dir);
    let b = build_with(
        "plain.nsm",
        src,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0);
    assert!(!libraries.uses_std_library(&b.design));
    let out = TempDir::new("plain-export");
    let dir = b.write(out.path()).unwrap();
    libraries.write_tables(&dir, &b.design).unwrap();
    assert!(!dir.join("netsmith.pretty").exists());
    assert!(!dir.join("netsmith.kicad_sym").exists());
    assert!(!dir.join("fp-lib-table").exists());
}
