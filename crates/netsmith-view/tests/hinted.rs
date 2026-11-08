//! The layout engine applies the schematic layout file's hints (ADR 0061, stage 2): a block is
//! moved, turned, mirrored or pinned, a note is moved, an entry that cannot be honoured is a
//! finding and is ignored, and the drawing stays the design's netlist whatever the file says.
//!
//! The properties are tried on every board the tests draw with seeded random files (std-only,
//! deterministic): random offsets, pins, turns, mirrors, notes and stale entries.

use crate::common;

use std::collections::BTreeMap;
use std::fmt::Write;

use common::corpus::{Case, corpus};
use netsmith_core::Design;
use netsmith_view::check::check;
use netsmith_view::geom::{Orient, Pt};
use netsmith_view::hints::parse;
use netsmith_view::layout::Layout;
use netsmith_view::{
    FindingKind, HintFinding, Marks, NoteHint, ResolvedHints, ResolvedNote, Steps,
    layout_sheets_hinted, layout_sheets_hinted_findings, layout_sheets_rails, render_layout,
};

const GRID: f64 = 1.27;

// --- a small seeded generator ------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 11) % n.max(1)
    }

    /// A whole number in `lo..=hi`.
    fn between(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.below((hi - lo + 1) as u64) as i32
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

// --- the boards ----------------------------------------------------------------------------

/// The case with a path for every part (a lower-case reference under its module's name), as a
/// design from the source has, so that the file can name them.
fn with_paths(case: &Case) -> Design {
    let mut design = case.design.clone();
    for (i, p) in design.parts.iter_mut().enumerate() {
        if p.path.is_empty() {
            let group = case.groups.iter().find(|g| g.parts.contains(&i));
            p.path = match group {
                Some(g) => format!("{}.{}", g.name, p.refdes.to_lowercase()),
                None => p.refdes.to_lowercase(),
            };
        }
    }
    design
}

fn draw(
    case: &Case,
    design: &Design,
    groups: &[netsmith_view::Group],
    rails: bool,
    hints: &ResolvedHints,
) -> (Layout, Vec<HintFinding>) {
    let none = netsmith_view::Rails::default();
    layout_sheets_hinted_findings(
        design,
        groups,
        if rails { &case.rails } else { &none },
        &Marks::default(),
        hints,
        &case.mapping,
        &case.lib,
    )
    .unwrap()
}

/// The text of a random file for `design`: entries for some of its parts (a place or a pin, a
/// turn, a mirror), a few that match nothing, and the offsets of some notes.
fn random_file(design: &Design, groups: &[netsmith_view::Group], rng: &mut Rng) -> String {
    let mut out = String::from("version = 1\n");
    let mut tables: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let board = &design.name;
    for (i, p) in design.parts.iter().enumerate() {
        if !rng.chance(35) {
            continue;
        }
        let group = groups.iter().find(|g| g.parts.contains(&i));
        let (header, key) = match group {
            Some(g) => (
                format!("{board}.sheets.{}.parts", g.name),
                p.path
                    .strip_prefix(&format!("{}.", g.name))
                    .unwrap_or(&p.path)
                    .to_string(),
            ),
            None => (format!("{board}.parts"), p.path.clone()),
        };
        let mut body = String::new();
        if rng.chance(20) {
            let _ = writeln!(
                body,
                "pin = [{:.2}, {:.2}]",
                f64::from(rng.between(-60, 60)) * GRID,
                f64::from(rng.between(-40, 40)) * GRID
            );
        } else if rng.chance(85) {
            // Mostly a few steps, sometimes a long way (the reach is 127 mm, 100 steps).
            let far = if rng.chance(10) { 140 } else { 14 };
            let _ = writeln!(
                body,
                "dx = {:.2}\ndy = {:.2}",
                f64::from(rng.between(-far, far)) * GRID,
                f64::from(rng.between(-far, far)) * GRID
            );
        }
        if rng.chance(35) {
            let _ = writeln!(body, "rotate = {}", 90 * rng.between(-1, 3));
        }
        if rng.chance(25) {
            let _ = writeln!(body, "mirror = {}", rng.chance(50));
        }
        tables
            .entry(format!("[{header}.\"{key}\"]"))
            .or_default()
            .push(body);
    }
    // Entries that match nothing: a part that is gone, a sheet that is gone.
    if rng.chance(50) {
        tables
            .entry(format!("[{board}.parts.\"gone.part\"]"))
            .or_default()
            .push("dx = 1.27\ndy = 0\n".into());
    }
    if rng.chance(30) {
        tables
            .entry(format!("[{board}.sheets.nowhere.parts.x]"))
            .or_default()
            .push("rotate = 90\n".into());
    }
    for (header, bodies) in tables {
        let _ = write!(out, "\n{header}\n{}", bodies.concat());
    }
    out
}

/// The resolved hints of a random file, with the offsets of some of the design's notes added
/// (a note's key needs the sheet's own counting; the notes are put in directly).
fn random_hints(
    design: &Design,
    groups: &[netsmith_view::Group],
    seed: u64,
) -> (ResolvedHints, Vec<HintFinding>, String) {
    let mut rng = Rng::new(seed);
    let text = random_file(design, groups, &mut rng);
    let file = parse(&text).unwrap_or_else(|e| panic!("seed {seed}: {e:?}\n{text}"));
    let (mut resolved, findings) = file.resolve(design, groups);
    for i in 0..design.notes.len() {
        if rng.chance(50) {
            let far = if rng.chance(15) { 120 } else { 20 };
            resolved.notes.insert(
                i,
                ResolvedNote {
                    sheet: design.name.clone(),
                    key: format!("note{i}#doc"),
                    hint: NoteHint {
                        by: Steps::new(rng.between(-far, far), rng.between(-far, far)),
                    },
                    line: 1000 + i as u32,
                    entry: format!("[{}.notes.\"note{i}#doc\"]", design.name),
                },
            );
        }
    }
    (resolved, findings, text)
}

/// Where each part is and how it is turned, by reference.
fn places(design: &Design, l: &Layout) -> BTreeMap<String, (Pt, Orient)> {
    design
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.clone(), (l.parts[i].at, l.parts[i].orient)))
        .collect()
}

/// Every wire as its net's name and its points to the hundredth of a millimetre, a run of
/// wire the same whichever way it is listed.
fn wires_of(design: &Design, l: &Layout) -> std::collections::BTreeSet<(String, Vec<(i64, i64)>)> {
    l.wires
        .iter()
        .map(|w| {
            let mut points: Vec<(i64, i64)> = w
                .points
                .iter()
                .map(|p| ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64))
                .collect();
            if points.first() > points.last() {
                points.reverse();
            }
            (design.nets[w.net].name.clone(), points)
        })
        .collect()
}

fn reversed(
    design: &Design,
    groups: &[netsmith_view::Group],
) -> (Design, Vec<netsmith_view::Group>) {
    let n = design.parts.len();
    let mut d = design.clone();
    d.parts.reverse();
    d.nets.reverse();
    for net in &mut d.nets {
        net.pins.reverse();
    }
    let g = groups
        .iter()
        .map(|g| netsmith_view::Group {
            parts: g.parts.iter().rev().map(|&p| n - 1 - p).collect(),
            ..g.clone()
        })
        .collect();
    (d, g)
}

// --- the properties ------------------------------------------------------------------------

#[derive(Default)]
struct Tally {
    drawings: usize,
    honoured: usize,
    refused: usize,
    stale: usize,
    entries: usize,
}

/// For one board, drawn with and without rails, under `seeds` random files: every rule holds,
/// the drawing and the findings are the same twice and in another order of the design, and the
/// findings are sorted.
fn hold_for(case_index: usize, seeds: std::ops::Range<u64>, tally: &mut Tally) {
    let case = &corpus()[case_index];
    let design = with_paths(case);
    for rails in [true, false] {
        let plain = draw(
            case,
            &design,
            &case.groups,
            rails,
            &ResolvedHints::default(),
        )
        .0;
        let base = check(&design, &plain);
        assert!(base.is_empty(), "{} baseline: {base:?}", case.name);
        for seed in seeds.clone() {
            let (hints, stale, text) = random_hints(&design, &case.groups, seed);
            let what = format!("{} (rails {rails}) seed {seed}\n{text}", case.name);
            let (layout, found) = draw(case, &design, &case.groups, rails, &hints);
            tally.drawings += 1;
            tally.entries += hints.parts.len() + hints.notes.len();
            tally.stale += stale.len();
            tally.refused += found.len();

            // The drawing is the design's netlist, and every rule holds.
            let broken = check(&design, &layout);
            assert!(
                broken.is_empty(),
                "{what}\n{}",
                broken
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join("\n")
            );

            // The findings the engine raises are about entries of the file, sorted.
            for f in &found {
                assert_eq!(f.kind, FindingKind::Unusable, "{what}\n{f:?}");
                assert!(f.message.starts_with(&f.entry), "{what}\n{f:?}");
            }
            let keys: Vec<(u32, &String)> = found.iter().map(|f| (f.line, &f.message)).collect();
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(
                keys, sorted,
                "{what}: the findings are in the order of the file"
            );

            // Twice is the same drawing, byte for byte.
            let (again, found_again) = draw(case, &design, &case.groups, rails, &hints);
            assert_eq!(layout, again, "{what}: drawn twice");
            assert_eq!(found, found_again, "{what}: findings twice");
            assert_eq!(
                render_layout(&design, &layout),
                render_layout(&design, &again),
                "{what}: pictures"
            );

            // The design listed the other way round, with the same file, is drawn the same.
            let (d2, g2) = reversed(&design, &case.groups);
            let (h2, _) = parse(&text).unwrap().resolve(&d2, &g2);
            let mut h2 = h2;
            // The notes are by index in the design's own order, which reversing leaves.
            h2.notes = hints.notes.clone();
            let (l2, found2) = draw(case, &d2, &g2, rails, &h2);
            assert_eq!(
                places(&design, &layout),
                places(&d2, &l2),
                "{what}: the order of the design's parts"
            );
            assert_eq!(found, found2, "{what}: the findings, in another order");
            assert_eq!(
                wires_of(&design, &layout),
                wires_of(&d2, &l2),
                "{what}: the wires, in another order of the design"
            );

            let moved = hints
                .parts
                .keys()
                .filter(|&&p| {
                    let (a, b) = (&plain.parts[p], &layout.parts[p]);
                    a.at != b.at || a.orient != b.orient
                })
                .count();
            tally.honoured += moved;
        }
    }
}

fn report(name: &str, t: &Tally) {
    println!(
        "{name}: {} drawings, {} entries, {} parts moved or turned, {} findings of the engine, {} stale entries",
        t.drawings, t.entries, t.honoured, t.refused, t.stale
    );
}

/// Seeds for each kind of board: the small ones are cheap, the bench and the processor are not
/// (a drawing is made ten times for each seed in a debug build).
const SEEDS: u64 = 4;

#[test]
fn random_hints_keep_every_rule_on_the_small_boards() {
    let mut t = Tally::default();
    for i in 0..3 {
        hold_for(i, 0..SEEDS * 2, &mut t);
    }
    report("small", &t);
    assert!(t.honoured > 10, "the files move something");
}

#[test]
fn random_hints_keep_every_rule_on_the_boards_of_several_modules() {
    let mut t = Tally::default();
    for i in 3..5 {
        hold_for(i, 0..SEEDS, &mut t);
    }
    report("modules", &t);
    assert!(t.honoured > 10, "the files move something");
}

#[test]
fn random_hints_keep_every_rule_on_the_bench_and_the_processor() {
    let mut t = Tally::default();
    for i in 5..7 {
        hold_for(i, 0..SEEDS / 2, &mut t);
    }
    report("bench", &t);
    assert!(t.honoured > 5, "the files move something");
}

#[test]
fn random_hints_keep_every_rule_on_the_buses_and_the_notes() {
    let mut t = Tally::default();
    for i in 7..corpus().len() {
        hold_for(i, 0..SEEDS, &mut t);
    }
    report("buses and notes", &t);
    assert!(t.honoured > 10, "the files move something");
}

#[test]
fn no_hints_draw_the_drawing_there_is_without_a_file() {
    for case in corpus() {
        for rails in [true, false] {
            let design = &case.design;
            let none = netsmith_view::Rails::default();
            let rails_told = if rails { &case.rails } else { &none };
            let plain =
                layout_sheets_rails(design, &case.groups, rails_told, &case.mapping, &case.lib)
                    .unwrap();
            let (hinted, found) = layout_sheets_hinted_findings(
                design,
                &case.groups,
                rails_told,
                &Marks::default(),
                &ResolvedHints::default(),
                &case.mapping,
                &case.lib,
            )
            .unwrap();
            assert_eq!(plain, hinted, "{} (rails {rails})", case.name);
            assert!(found.is_empty());
            // A file with a table for another board resolves to nothing at all.
            let file = parse("version = 1\n[Elsewhere.parts.j1]\ndx = 1.27\ndy = 0\n").unwrap();
            let (resolved, _) = file.resolve(design, &case.groups);
            assert!(resolved.is_empty());
            let again = layout_sheets_hinted(
                design,
                &case.groups,
                rails_told,
                &Marks::default(),
                &resolved,
                &case.mapping,
                &case.lib,
            )
            .unwrap();
            assert_eq!(plain, again);
        }
    }
}

// --- each rule, on its own -------------------------------------------------------------------

use netsmith_view::stability::{self, Board};
use netsmith_view::{PartHint, Place, ResolvedPart};

/// A file entry for `refdes`, put in directly.
fn entry(design: &Design, refdes: &str, hint: PartHint) -> (usize, ResolvedPart) {
    let p = design
        .parts
        .iter()
        .position(|p| p.refdes == refdes)
        .unwrap_or_else(|| panic!("no part {refdes}"));
    (
        p,
        ResolvedPart {
            sheet: design.name.clone(),
            path: design.parts[p].path.clone(),
            hint,
            line: 10 + p as u32,
            entry: format!("[{}.parts.\"{}\"]", design.name, design.parts[p].path),
        },
    )
}

fn hints_of(design: &Design, entries: &[(&str, PartHint)]) -> ResolvedHints {
    let mut h = ResolvedHints::default();
    for (refdes, hint) in entries {
        let (p, r) = entry(design, refdes, *hint);
        h.parts.insert(p, r);
    }
    h
}

fn offset(x: i32, y: i32) -> PartHint {
    PartHint {
        place: Some(Place::Offset(Steps::new(x, y))),
        ..PartHint::default()
    }
}

fn pinned(x: i32, y: i32) -> PartHint {
    PartHint {
        place: Some(Place::Pinned(Steps::new(x, y))),
        ..PartHint::default()
    }
}

fn turn(mirror: Option<bool>, degrees: Option<u16>) -> PartHint {
    PartHint {
        place: None,
        rotate: degrees,
        mirror,
    }
}

/// Equal to the micron: the drawing rounds each coordinate to a millionth of a millimetre.
fn near(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4
}

fn steps(x: i32, y: i32) -> Pt {
    Pt::new(f64::from(x) * GRID, f64::from(y) * GRID)
}

/// The small boards, with a path for every part, each drawn plain.
fn small() -> Vec<(Case, Design)> {
    corpus()
        .into_iter()
        .take(3)
        .map(|c| {
            let d = with_paths(&c);
            (c, d)
        })
        .collect()
}

#[test]
fn an_offset_moves_the_block_it_names_by_the_steps_and_nothing_else() {
    let mut moved_parts = 0;
    for (case, design) in small() {
        let (plain, _) = draw(
            &case,
            &design,
            &case.groups,
            true,
            &ResolvedHints::default(),
        );
        for part in &design.parts {
            for (dx, dy) in [(3, 0), (0, -4), (-2, 5)] {
                let h = hints_of(&design, &[(&part.refdes, offset(dx, dy))]);
                let (l, found) = draw(&case, &design, &case.groups, true, &h);
                let i = design
                    .parts
                    .iter()
                    .position(|p| p.refdes == part.refdes)
                    .unwrap();
                let what = format!("{}: {} by ({dx}, {dy})", case.name, part.refdes);
                if !found.is_empty() {
                    // Refused: the block stays where it was, and that is all that happens.
                    assert!(found[0].message.contains(&part.path), "{what}: {found:?}");
                    for (a, b) in plain.parts.iter().zip(&l.parts) {
                        assert_eq!(a.at, b.at, "{what}: refused, so nothing moves");
                    }
                    continue;
                }
                assert!(
                    near(
                        l.parts[i].at,
                        Pt::new(
                            plain.parts[i].at.x + steps(dx, dy).x,
                            plain.parts[i].at.y + steps(dx, dy).y
                        )
                    ),
                    "{what}: {:?} from {:?}",
                    l.parts[i].at,
                    plain.parts[i].at
                );
                // Every other part is where it was, or carried by the same steps (its block).
                for (k, (a, b)) in plain.parts.iter().zip(&l.parts).enumerate() {
                    let by = Pt::new(b.at.x - a.at.x, b.at.y - a.at.y);
                    assert!(
                        near(by, Pt::new(0.0, 0.0)) || near(by, steps(dx, dy)),
                        "{what}: part {} moved by {by:?}",
                        design.parts[k].refdes
                    );
                    assert_eq!(a.orient, b.orient, "{what}: no turn");
                }
                moved_parts += 1;
            }
        }
    }
    assert!(moved_parts >= 15, "{moved_parts} moves were honoured");
}

#[test]
fn a_turn_and_a_mirror_replace_the_automatic_orientation() {
    let (mut honoured, mut refused) = (0, 0);
    for (case, design) in small() {
        let (plain, _) = draw(
            &case,
            &design,
            &case.groups,
            true,
            &ResolvedHints::default(),
        );
        for part in &design.parts {
            let i = design
                .parts
                .iter()
                .position(|p| p.refdes == part.refdes)
                .unwrap();
            for (mirror, degrees) in [
                (Some(false), Some(0)),
                (Some(true), Some(90)),
                (Some(false), Some(180)),
                (Some(true), Some(270)),
            ] {
                let h = hints_of(&design, &[(&part.refdes, turn(mirror, degrees))]);
                let (l, found) = draw(&case, &design, &case.groups, true, &h);
                let what = format!("{}: {} {mirror:?} {degrees:?}", case.name, part.refdes);
                if !found.is_empty() {
                    // The drawing with the turn broke a rule: it is as if the file had not
                    // said it.
                    assert!(
                        found[0].message.contains("breaks a rule"),
                        "{what}: {found:?}"
                    );
                    assert_eq!(plain, l, "{what}");
                    refused += 1;
                    continue;
                }
                let want = Orient::turned(mirror.unwrap(), (degrees.unwrap() / 90) as u8);
                assert_eq!(l.parts[i].orient, want, "{what}");
                honoured += 1;
            }
            // Only the turn: the automatic mirror stays.
            let (auto_mirror, _) = plain.parts[i].orient.mirror_and_turns();
            let h = hints_of(&design, &[(&part.refdes, turn(None, Some(90)))]);
            let (l, found) = draw(&case, &design, &case.groups, true, &h);
            if found.is_empty() {
                assert_eq!(
                    l.parts[i].orient,
                    Orient::turned(auto_mirror, 1),
                    "{}",
                    part.refdes
                );
            }
        }
    }
    println!("turns: {honoured} honoured, {refused} refused by the rules");
    assert!(
        honoured >= 20 && honoured > 3 * refused,
        "{honoured} honoured, {refused} refused"
    );
}

#[test]
fn a_pin_puts_the_block_at_a_place_from_the_reference_part() {
    for (case, design) in small() {
        let (plain, _) = draw(
            &case,
            &design,
            &case.groups,
            true,
            &ResolvedHints::default(),
        );
        // The frame starts at the reference part: it is the part at the origin.
        let reference = plain
            .parts
            .iter()
            .position(|p| near(p.at, Pt::new(0.0, 0.0)))
            .unwrap_or_else(|| panic!("{}: a part at the origin", case.name));
        let mut honoured = 0;
        for (k, part) in design.parts.iter().enumerate() {
            let h = hints_of(&design, &[(&part.refdes, pinned(30, -12))]);
            let (l, found) = draw(&case, &design, &case.groups, true, &h);
            if !found.is_empty() {
                continue;
            }
            // Wherever the reference part is, the pinned block's origin is 30 steps right
            // and 12 up from it.
            assert!(
                near(
                    l.parts[k].at,
                    Pt::new(
                        l.parts[reference].at.x + steps(30, -12).x,
                        l.parts[reference].at.y + steps(30, -12).y
                    )
                ) || k == reference && near(l.parts[k].at, steps(30, -12)),
                "{}: {} pinned: {:?}, reference {:?}",
                case.name,
                part.refdes,
                l.parts[k].at,
                l.parts[reference].at
            );
            if k != reference {
                // The frame did not move: the reference part is where it was.
                assert!(
                    near(l.parts[reference].at, plain.parts[reference].at),
                    "{}",
                    case.name
                );
                assert!(near(l.parts[k].at, steps(30, -12)), "{}", case.name);
            }
            honoured += 1;
        }
        assert!(honoured >= 1, "{}", case.name);
    }
}

#[test]
fn a_pinned_part_stays_where_the_file_puts_it_when_the_sheet_grows() {
    // The automatic drawing changes (a capacitor is added beside a pin and the parts round it
    // make room); the pinned part does not.
    let case = &corpus()[2];
    let design = with_paths(case);
    let board = Board {
        design: design.clone(),
        groups: case.groups.clone(),
        rails: case.rails.clone(),
        marks: Marks::default(),
    };
    let plain = board.draw(&case.mapping, &case.lib);
    let sheet = stability::sheet_names(&plain).remove(0);
    let edit = stability::plan(&board, &plain, &sheet, stability::Kind::AddDecouplingCap)
        .expect("a capacitor can be added");
    let edited = stability::apply(&board, &edit);
    let moved = |d: &Design, hints: &ResolvedHints| draw(case, d, &case.groups, true, hints).0;
    let before = moved(&design, &ResolvedHints::default());
    let after = moved(&edited.board.design, &ResolvedHints::default());
    let shifted = design
        .parts
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            let j = edited
                .board
                .design
                .parts
                .iter()
                .position(|q| q.refdes == p.refdes)
                .unwrap();
            !near(before.parts[*i].at, after.parts[j].at)
        })
        .count();
    assert!(shifted > 0, "the edit makes room: something moves");
    // Pin the farthest part from the origin.
    let (far, _) = before
        .parts
        .iter()
        .enumerate()
        .max_by(|a, b| {
            (a.1.at.x.abs() + a.1.at.y.abs()).total_cmp(&(b.1.at.x.abs() + b.1.at.y.abs()))
        })
        .unwrap();
    let r = design.parts[far].refdes.clone();
    let at = |d: &Design| {
        let h = hints_of(d, &[(&r, pinned(-20, 14))]);
        let l = moved(d, &h);
        let i = d.parts.iter().position(|p| p.refdes == r).unwrap();
        l.parts[i].at
    };
    assert!(
        near(at(&design), at(&edited.board.design)),
        "pinned at the same place in both"
    );
}

#[test]
fn a_block_moved_onto_another_part_is_not_moved_and_the_finding_names_the_entry() {
    let (case, design) = small().remove(2);
    let (plain, _) = draw(
        &case,
        &design,
        &case.groups,
        true,
        &ResolvedHints::default(),
    );
    // Move the first part onto the second.
    let (a, b) = (&design.parts[0], &design.parts[1]);
    let (pa, pb) = (plain.parts[0].at, plain.parts[1].at);
    let (dx, dy) = (
        ((pb.x - pa.x) / GRID).round() as i32,
        ((pb.y - pa.y) / GRID).round() as i32,
    );
    let h = hints_of(&design, &[(&a.refdes, offset(dx, dy))]);
    let (l, found) = draw(&case, &design, &case.groups, true, &h);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].kind, FindingKind::Unusable);
    assert!(found[0].entry.contains(&a.path), "{:?}", found[0]);
    assert!(
        found[0]
            .message
            .contains(&format!("`{}`", design.parts[1].path))
            || found[0].message.contains("lie on"),
        "{}",
        found[0].message
    );
    assert!(found[0].message.contains("it is ignored"));
    assert_eq!(found[0].line, 10, "the line of the entry");
    for (x, y) in plain.parts.iter().zip(&l.parts) {
        assert_eq!(x.at, y.at, "the block is where the drawing put it");
    }
    let _ = b;
}

#[test]
fn of_two_hinted_blocks_that_lie_on_each_other_the_earlier_path_wins() {
    let (case, design) = small().remove(2);
    let (plain, _) = draw(
        &case,
        &design,
        &case.groups,
        true,
        &ResolvedHints::default(),
    );
    // Two free-standing parts of the board moved to one place: the one whose path sorts first
    // is honoured.
    let mut order: Vec<usize> = (0..design.parts.len()).collect();
    order.sort_by(|&x, &y| design.parts[x].path.cmp(&design.parts[y].path));
    let (first, second) = (order[0], order[1]);
    let target = Pt::new(
        plain.parts[first].at.x + 3.0 * GRID,
        plain.parts[first].at.y - 25.0 * GRID,
    );
    let by = |i: usize| {
        (
            ((target.x - plain.parts[i].at.x) / GRID).round() as i32,
            ((target.y - plain.parts[i].at.y) / GRID).round() as i32,
        )
    };
    let h = hints_of(
        &design,
        &[
            (
                &design.parts[second].refdes,
                offset(by(second).0, by(second).1),
            ),
            (
                &design.parts[first].refdes,
                offset(by(first).0, by(first).1),
            ),
        ],
    );
    let (l, found) = draw(&case, &design, &case.groups, true, &h);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].entry.contains(&design.parts[second].path),
        "{:?}",
        found[0]
    );
    assert!(
        near(l.parts[first].at, target),
        "{:?} {:?}",
        l.parts[first].at,
        target
    );
    assert!(near(l.parts[second].at, plain.parts[second].at));
}

#[test]
fn a_block_too_far_from_the_drawing_is_a_finding() {
    let (case, design) = small().remove(0);
    let h = hints_of(&design, &[(&design.parts[0].refdes, offset(900, 0))]);
    let (l, found) = draw(&case, &design, &case.groups, true, &h);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].message.contains("outside the drawing"),
        "{}",
        found[0].message
    );
    assert!(l.bounds.width() < 600.0);
}

#[test]
fn a_note_is_moved_by_its_offset_when_the_place_is_free() {
    let case = &corpus()[9];
    assert_eq!(case.name, "LedNotes");
    let design = &case.design;
    let (plain, _) = draw(case, design, &case.groups, true, &ResolvedHints::default());
    assert!(plain.notes.len() >= 3);
    let n = plain
        .notes
        .iter()
        .find(|n| n.path == "r1")
        .expect("the note of r1")
        .note;
    let mut h = ResolvedHints::default();
    let hint = |dx, dy| ResolvedNote {
        sheet: design.name.clone(),
        key: "r1#doc".into(),
        hint: NoteHint {
            by: Steps::new(dx, dy),
        },
        line: 7,
        entry: format!("[{}.notes.\"r1#doc\"]", design.name),
    };
    // Somewhere free: the first of a few moves of the frame that the drawing allows.
    let mut moved = None;
    for (dx, dy) in [
        (0, -12),
        (12, 0),
        (0, 12),
        (-12, 0),
        (0, -30),
        (30, 0),
        (0, 30),
    ] {
        h.notes.insert(n, hint(dx, dy));
        let (l, found) = draw(case, design, &case.groups, true, &h);
        if found.is_empty() {
            moved = Some((dx, dy, l));
            break;
        }
    }
    let (dx, dy, l) = moved.expect("a note can be moved somewhere");
    let (a, b) = (
        plain.notes.iter().find(|x| x.note == n).unwrap().frame,
        l.notes.iter().find(|x| x.note == n).unwrap().frame,
    );
    assert!(
        near(Pt::new(b.min.x - a.min.x, b.min.y - a.min.y), steps(dx, dy)),
        "{a:?} {b:?}"
    );
    assert!(check(design, &l).is_empty());
    // Everything else, the notes and the parts, is where it was.
    assert_eq!(plain.parts, l.parts);
    for (x, y) in plain.notes.iter().zip(&l.notes) {
        if x.note != n {
            assert_eq!(x.frame, y.frame);
        }
    }
    // Onto the part it is about: refused, and the note stays where it was put.
    let part = plain
        .parts
        .iter()
        .find(|p| design.parts[p.part].path == "r1")
        .unwrap();
    let to = Pt::new((part.at.x - a.min.x) / GRID, (part.at.y - a.min.y) / GRID);
    h.notes
        .insert(n, hint(to.x.round() as i32, to.y.round() as i32));
    let (l, found) = draw(case, design, &case.groups, true, &h);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].message.contains("would lie on"),
        "{}",
        found[0].message
    );
    assert_eq!(found[0].line, 7);
    assert_eq!(plain.notes, l.notes);
}

#[test]
fn moving_the_reference_part_does_not_move_the_frame() {
    // The frame starts at the reference part where the drawing put it: an offset of the
    // reference part moves it, and nothing else, in the frame the page shows.
    let (case, design) = small().remove(2);
    let (plain, _) = draw(
        &case,
        &design,
        &case.groups,
        true,
        &ResolvedHints::default(),
    );
    let reference = plain
        .parts
        .iter()
        .position(|p| near(p.at, Pt::new(0.0, 0.0)))
        .unwrap();
    let h = hints_of(
        &design,
        &[(&design.parts[reference].refdes, offset(0, -14))],
    );
    let (l, found) = draw(&case, &design, &case.groups, true, &h);
    assert!(found.is_empty(), "{found:?}");
    assert!(
        near(l.parts[reference].at, steps(0, -14)),
        "{:?}",
        l.parts[reference].at
    );
    for (k, (a, b)) in plain.parts.iter().zip(&l.parts).enumerate() {
        let by = Pt::new(b.at.x - a.at.x, b.at.y - a.at.y);
        assert!(
            near(by, Pt::new(0.0, 0.0)) || near(by, steps(0, -14)),
            "{} {by:?}",
            design.parts[k].refdes
        );
    }
}

#[test]
fn what_a_move_does_to_the_rest_of_a_sheet_is_measured() {
    // Moving one block by four steps moves that block (the main part and the satellites that
    // serve it) and what must make room for it. For every main part of the boards of several
    // sheets, in the sheet's own frame: the parts of the sheet that are not in the block, and
    // how many of them are where they were. (The page puts each sheet's box at its place in the
    // arrangement of sheets, and a block that was the leftmost thing of its sheet moves the
    // box's edge, so every part of the sheet is shifted by the same steps on the page: the
    // parts that moved the same as most of the sheet did not move in the sheet.)
    let (mut others, mut still, mut blocks, mut cases) = (0usize, 0usize, 0usize, 0usize);
    let (mut worst, mut worst_at) = (1.0f64, String::new());
    for case in corpus()
        .iter()
        .filter(|c| matches!(c.name.as_str(), "Supplies3" | "Supplies7" | "SuppliesNotes"))
    {
        let design = with_paths(case);
        let (plain, _) = draw(case, &design, &case.groups, true, &ResolvedHints::default());
        let sheet_of = |i: usize| case.groups.iter().position(|g| g.parts.contains(&i));
        for (i, part) in design.parts.iter().enumerate() {
            // The parts the sheet draws as main parts: those with several pins.
            if plain.parts[i].pins.len() < 3 {
                continue;
            }
            let by = steps(4, 0);
            let h = hints_of(&design, &[(&part.refdes, offset(4, 0))]);
            let (l, found) = draw(case, &design, &case.groups, true, &h);
            if !found.is_empty() {
                continue;
            }
            let on_sheet: Vec<usize> = (0..design.parts.len())
                .filter(|&k| sheet_of(k) == sheet_of(i))
                .collect();
            let delta = |k: usize| {
                Pt::new(
                    l.parts[k].at.x - plain.parts[k].at.x,
                    l.parts[k].at.y - plain.parts[k].at.y,
                )
            };
            // The step most of the sheet moved by: where the sheet's box went.
            let frame = on_sheet
                .iter()
                .map(|&k| delta(k))
                .max_by_key(|d| on_sheet.iter().filter(|&&k| near(delta(k), *d)).count())
                .unwrap();
            let (mut here, mut kept, mut carried) = (0, 0, 0);
            for &k in &on_sheet {
                let d = delta(k);
                let relative = Pt::new(d.x - frame.x, d.y - frame.y);
                if near(relative, by) {
                    carried += 1;
                } else {
                    here += 1;
                    if near(relative, Pt::new(0.0, 0.0))
                        && plain.parts[k].orient == l.parts[k].orient
                    {
                        kept += 1;
                    }
                }
            }
            others += here;
            still += kept;
            blocks += carried;
            cases += 1;
            let fraction = kept as f64 / here.max(1) as f64;
            if fraction < worst {
                worst = fraction;
                worst_at = format!("{} {}", case.name, part.path);
            }
        }
    }
    println!(
        "one main part's block moved by 4 steps: {cases} cases, {} parts carried with it (mean {:.1}); of the {others} other parts of its sheet {still} stay ({:.0} %), worst case {:.0} % ({worst_at})",
        blocks,
        blocks as f64 / cases.max(1) as f64,
        100.0 * still as f64 / others.max(1) as f64,
        100.0 * worst
    );
    assert!(cases >= 8, "{cases}");
    assert!(blocks >= cases, "every move carries at least its main part");
    assert!(still as f64 >= 0.9 * others as f64, "{still}/{others}");
}

// --- what the drawing says to a page that moves parts (stage 4) ------------------------------

/// The `<g class="part" ...>` tags of an SVG, one string each.
fn part_tags(svg: &str) -> Vec<&str> {
    svg.match_indices("<g class=\"part")
        .map(|(at, _)| &svg[at..at + svg[at..].find('>').unwrap()])
        .filter(|tag| attr(tag, "data-ref").is_some())
        .collect()
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    Some(&tag[at..at + tag[at..].find('"')?])
}

#[test]
fn a_part_says_where_its_origin_is_and_how_it_is_turned() {
    for (case, design) in small() {
        // A turn and a move, so that the attributes are not the defaults.
        let hints = hints_of(
            &design,
            &[(
                &design.parts[0].refdes,
                PartHint {
                    place: Some(Place::Offset(Steps::new(2, 1))),
                    rotate: Some(90),
                    mirror: Some(true),
                },
            )],
        );
        let (layout, found) = draw(&case, &design, &case.groups, true, &hints);
        let svg = render_layout(&design, &layout);
        let tags = part_tags(&svg);
        assert_eq!(tags.len(), layout.parts.len(), "{}", case.name);
        for (tag, p) in tags.iter().zip(&layout.parts) {
            let origin: Vec<f64> = attr(tag, "data-origin")
                .unwrap_or_else(|| panic!("{}: {tag}", case.name))
                .split(' ')
                .map(|n| n.parse().unwrap())
                .collect();
            assert!(
                near(Pt::new(origin[0], origin[1]), p.at),
                "{}: {tag} against {:?}",
                case.name,
                p.at
            );
            let (mirror, turns) = p.orient.mirror_and_turns();
            assert_eq!(
                attr(tag, "data-orient"),
                Some(format!("{} {turns}", u8::from(mirror)).as_str()),
                "{}: {tag}",
                case.name
            );
        }
        // The part that was turned says so: mirrored, a quarter turn (unless the drawing refused the entry).
        if found.is_empty() {
            assert_eq!(attr(tags[0], "data-orient"), Some("1 1"), "{}", case.name);
        }
    }
}

#[test]
fn the_reference_part_of_each_sheet_is_marked_and_is_the_one_the_frame_starts_at() {
    for (case, design) in small() {
        let (layout, _) = draw(
            &case,
            &design,
            &case.groups,
            true,
            &ResolvedHints::default(),
        );
        let svg = render_layout(&design, &layout);
        let marked: Vec<&str> = part_tags(&svg)
            .into_iter()
            .filter(|t| attr(t, "data-reference").is_some())
            .collect();
        // One for each sheet the drawing has: the board's own, and a module's.
        let sheets = design
            .parts
            .iter()
            .map(|p| p.path.split_once('.').map_or("", |(m, _)| m))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        assert!(
            !marked.is_empty() && marked.len() <= sheets.max(1),
            "{}: {} marked of {sheets} sheets",
            case.name,
            marked.len()
        );
        // The frame starts at the reference part: for a drawing of one sheet it is at the origin.
        if sheets == 1 {
            let origin: Vec<f64> = attr(marked[0], "data-origin")
                .unwrap()
                .split(' ')
                .map(|n| n.parse().unwrap())
                .collect();
            assert!(
                near(Pt::new(origin[0], origin[1]), Pt::new(0.0, 0.0)),
                "{}: {}",
                case.name,
                marked[0]
            );
        }
        // And what a hint does to the reference part's offset changes no mark.
        let reference = layout
            .parts
            .iter()
            .position(|p| near(p.at, Pt::new(0.0, 0.0)))
            .unwrap();
        if sheets == 1 {
            let again = draw(
                &case,
                &design,
                &case.groups,
                true,
                &hints_of(
                    &design,
                    &[(
                        &design.parts[(reference + 1) % design.parts.len()].refdes,
                        offset(3, 0),
                    )],
                ),
            )
            .0;
            let svg2 = render_layout(&design, &again);
            assert_eq!(
                part_tags(&svg2)
                    .iter()
                    .filter(|t| attr(t, "data-reference").is_some())
                    .count(),
                1,
                "{}",
                case.name
            );
        }
    }
}
