//! A sheet whose wires cross when it is drawn the usual way is drawn again with other choices of
//! the engine ([`Tuning`]), and the drawing with fewer crossings (or as many with less wire) is
//! kept; a sheet that does not cross is never drawn again. The board here is a current monitor: a chip with its
//! supply on the left and its ground on the right, two address pins and three open-drain
//! signals on the bottom, a shunt resistor across its two sense supplies on the top, and a
//! pull-up on each signal.

use crate::common;

use netsmith_core::Design;
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_testkit::TempDir;
use netsmith_view::check::check;
use netsmith_view::geom::Pt;
use netsmith_view::layout::{Layout, PowerKind};
use netsmith_view::metrics::measure;
use netsmith_view::{Rails, Tuning, layout_rails, layout_tuned, layout_tuned_touched};

const STEP: f64 = 1.27;

fn pin(kind: &str, x: f64, y: f64, angle: u32, name: &str, number: u32) -> String {
    format!(
        "(pin {kind} line (at {x} {y} {angle}) (length 2.54) \
         (name \"{name}\" (effects (font (size 1.27 1.27)))) \
         (number \"{number}\" (effects (font (size 1.27 1.27)))))"
    )
}

/// `Mon`: `VS` on the left; `GND` on the right; `A1`, `A0`, `SDA`, `SCL` and `ALERT` on the
/// bottom; `Vbus`, `Vin+` and `Vin-` on the top.
fn library() -> String {
    let mut p = String::new();
    p += &pin("power_in", -12.7, 0.0, 0, "VS", 6);
    p += &pin("power_in", 12.7, 0.0, 180, "GND", 7);
    for (i, (name, n)) in [("A1", 1), ("A0", 2), ("SDA", 4), ("SCL", 5), ("ALERT", 3)]
        .into_iter()
        .enumerate()
    {
        p += &pin("bidirectional", -5.08 + 2.54 * i as f64, -12.7, 90, name, n);
    }
    for (i, (name, n)) in [("Vbus", 8), ("Vin+", 10), ("Vin-", 9)]
        .into_iter()
        .enumerate()
    {
        p += &pin("power_in", -2.54 + 2.54 * i as f64, 12.7, 270, name, n);
    }
    format!(
        "(kicad_symbol_lib (symbol \"Mon\" (property \"Reference\" \"U\" (at 0 15 0)) \
         (symbol \"Mon_0_1\" (rectangle (start -10.16 10.16) (end 10.16 -10.16) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Mon_1_1\" {p})))"
    )
}

fn setup() -> (TempDir, Mapping, SymbolLib) {
    let dir = TempDir::new("view-tuning");
    let file = dir.path().join("tuning.kicad_sym");
    std::fs::write(&file, library()).unwrap();
    let rows = "[[part]]\nkind = \"Mon\"\npackage = \"P\"\nsymbol = \"tuning:Mon\"\nfootprint = \"tuning:F\"\n";
    let mapping = Mapping::standard()
        .merged(Mapping::parse(rows, "kicad.toml", dir.path()).unwrap())
        .unwrap();
    let lib = SymbolLib::at(netsmith_testkit::sample_symbols()).with_library("tuning", file);
    (dir, mapping, lib)
}

fn board() -> Design {
    common::design(
        "Monitor",
        &[
            ("U1", "Mon", "INA", Some("P")),
            ("R1", "Resistor", "100m", Some("0402")),
            ("R2", "Resistor", "4.7k", Some("0402")),
            ("R3", "Resistor", "4.7k", Some("0402")),
            ("R4", "Resistor", "10k", Some("0402")),
            ("C1", "Capacitor", "100n", Some("0402")),
        ],
        &[
            ("+3V3", &["U1.6", "R2.1", "R3.1", "R4.1", "C1.1"]),
            ("GND", &["U1.7", "U1.1", "U1.2", "C1.2"]),
            ("SAFE", &["U1.8", "U1.10", "R1.1"]),
            ("LOAD", &["U1.9", "R1.2"]),
            ("SDA", &["U1.4", "R2.2"]),
            ("SCL", &["U1.5", "R3.2"]),
            ("ALERT", &["U1.3", "R4.2"]),
        ],
    )
}

fn rails() -> Rails {
    let mut r = Rails::default();
    r.ground.insert("GND".into());
    for n in ["+3V3", "SAFE", "LOAD"] {
        r.positive.insert(n.into());
    }
    r
}

fn draw() -> (Design, Layout, TempDir) {
    let (dir, mapping, lib) = setup();
    let design = board();
    let layout = layout_rails(&design, &rails(), &mapping, &lib).unwrap();
    (design, layout, dir)
}

fn pin_at(l: &Layout, d: &Design, refdes: &str, pad: &str) -> Pt {
    let part = d.parts.iter().position(|p| p.refdes == refdes).unwrap();
    l.parts[part].pins.iter().find(|p| p.id == pad).unwrap().at
}

#[test]
fn the_drawing_keeps_every_rule() {
    let (d, l, _dir) = draw();
    let found = check(&d, &l);
    assert!(found.is_empty(), "{found:?}");
}

fn tuned(tuning: Tuning) -> (Design, Layout, TempDir) {
    let (dir, mapping, lib) = setup();
    let design = board();
    let layout = layout_tuned(&design, &rails(), &mapping, &lib, tuning).unwrap();
    (design, layout, dir)
}

fn steps(a: Pt, b: Pt) -> f64 {
    ((a.x - b.x).abs() + (a.y - b.y).abs()) / STEP
}

/// The ground symbols of the layout and where they are.
fn grounds(l: &Layout) -> Vec<Pt> {
    l.symbols
        .iter()
        .filter(|s| s.kind == PowerKind::Ground)
        .map(|s| s.at)
        .collect()
}

#[test]
fn a_main_parts_few_ground_pins_have_symbols_beside_them_when_so_drawn() {
    let local = Tuning {
        local_main: true,
        ..Tuning::default()
    };
    let (d, l, _dir) = tuned(local);
    let found = check(&d, &l);
    assert!(found.is_empty(), "{found:?}");
    // The chip's ground pin 7 has a ground symbol within eight steps of its tip: not the one
    // symbol at the end of a rail across the sheet.
    let at = pin_at(&l, &d, "U1", "7");
    let near = |l: &Layout| {
        grounds(l)
            .into_iter()
            .map(|g| steps(at, g))
            .fold(f64::MAX, f64::min)
    };
    assert!(
        near(&l) <= 8.0,
        "the nearest ground symbol is {} steps away",
        near(&l)
    );
    // The usual drawing has fewer ground symbols: the pins share the rail's.
    let (_, usual, _dir) = tuned(Tuning::default());
    assert!(grounds(&usual).len() < grounds(&l).len());
}

#[test]
fn a_part_across_two_supplies_stands_beside_the_two_pins_when_so_drawn() {
    let across = Tuning {
        pair_rails: true,
        ..Tuning::default()
    };
    let (d, l, _dir) = tuned(across);
    let found = check(&d, &l);
    assert!(found.is_empty(), "{found:?}");
    // The shunt R1 is joined to the chip's `Vin+` (10) and `Vin-` (9) by short wires.
    for (pad, chip_pad) in [("1", "10"), ("2", "9")] {
        let (a, b) = (pin_at(&l, &d, "R1", pad), pin_at(&l, &d, "U1", chip_pad));
        assert!(
            steps(a, b) <= 16.0,
            "R1.{pad} is {} steps from U1.{chip_pad}",
            steps(a, b)
        );
    }
    // Drawn the usual way it is a satellite of one pin like another, and stands farther out.
    let (_, usual, _dir) = tuned(Tuning::default());
    let far = steps(
        pin_at(&usual, &d, "R1", "1"),
        pin_at(&usual, &d, "U1", "10"),
    );
    assert!(far > 16.0, "{far}");
}

#[test]
fn a_sheet_is_never_left_with_more_crossings_or_more_broken_rules_by_the_search() {
    let (d, searched, _dir) = draw();
    let (_, usual, _dir2) = tuned(Tuning::default());
    let (ms, mu) = (&measure(&searched).sheets[0], &measure(&usual).sheets[0]);
    assert!(
        ms.crossings <= mu.crossings,
        "{} against {}",
        ms.crossings,
        mu.crossings
    );
    assert!(check(&d, &searched).len() <= check(&d, &usual).len());
    // The same input draws the same bytes.
    let (_, again, _dir3) = draw();
    assert_eq!(format!("{searched:?}"), format!("{again:?}"));
}

fn all_tunings() -> Vec<Tuning> {
    (0..8)
        .map(|bits| Tuning {
            local_main: bits & 1 != 0,
            bottom_first: bits & 2 != 0,
            pair_rails: bits & 4 != 0,
        })
        .collect()
}

/// Whether `a` and `b` choose alike in each choice that `on` has.
fn agree_on(a: Tuning, b: Tuning, on: Tuning) -> bool {
    (!on.local_main || a.local_main == b.local_main)
        && (!on.bottom_first || a.bottom_first == b.bottom_first)
        && (!on.pair_rails || a.pair_rails == b.pair_rails)
}

/// Draws the board with each of the eight sets of choices and says, for each pair of drawings
/// that agree on the choices the first one told as mattering, that they are the same bytes.
/// Returns what each of the eight told.
fn assert_told_choices_are_all_that_matter(
    name: &str,
    design: &Design,
    rails: &Rails,
    mapping: &Mapping,
    lib: &SymbolLib,
) -> Vec<(Tuning, Tuning)> {
    let drawn: Vec<(Tuning, String, Tuning)> = all_tunings()
        .into_iter()
        .map(|t| {
            let (layout, told) = layout_tuned_touched(design, rails, mapping, lib, t).unwrap();
            (t, format!("{layout:?}"), told)
        })
        .collect();
    for (t, a, told) in &drawn {
        for (u, b, _) in &drawn {
            if agree_on(*t, *u, *told) {
                assert!(
                    a == b,
                    "{name}: {t:?} and {u:?} agree on every choice that mattered ({told:?}) and draw differently"
                );
            }
        }
    }
    drawn.into_iter().map(|(t, _, told)| (t, told)).collect()
}

#[test]
fn a_choice_the_drawing_never_came_to_a_place_for_changes_nothing() {
    // The search draws a sheet again only with choices that mattered somewhere in a drawing
    // it has; this is what lets it leave the others out, so it must hold on every board.
    let (_dir, mapping, lib) = setup();
    let told =
        assert_told_choices_are_all_that_matter("Monitor", &board(), &rails(), &mapping, &lib);
    // The monitor has a part across two supplies and a chip with ground pins to spare: the
    // two choices are told, so the test above is not holding for want of being told.
    let (_, usual) = told
        .iter()
        .find(|(t, _)| *t == Tuning::default())
        .copied()
        .unwrap();
    assert!(usual.pair_rails && usual.local_main, "{usual:?}");
    // The test boards of up to thirty parts (each is drawn eight times, as one sheet).
    for case in common::corpus::corpus()
        .iter()
        .filter(|c| c.design.parts.len() <= 30)
    {
        assert_told_choices_are_all_that_matter(
            &case.name,
            &case.design,
            &case.rails,
            &case.mapping,
            &case.lib,
        );
    }
}

#[test]
fn a_board_with_no_part_across_two_supplies_has_no_place_for_that_choice() {
    let led = common::led();
    let (usual, told) = layout_tuned_touched(
        &led,
        &Rails::default(),
        &Mapping::standard(),
        &SymbolLib::at(netsmith_testkit::sample_symbols()),
        Tuning::default(),
    )
    .map(|(l, t)| (l.parts.len(), t))
    .unwrap();
    assert!(usual > 0);
    assert!(!told.pair_rails, "{told:?}");
}
