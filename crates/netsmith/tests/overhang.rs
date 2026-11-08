//! A part that hangs over the board edge on purpose (ADR 0045, "Built: overhang"), from the
//! source to the package. The board is an invented 35 by 60 mm one with a wide plug whose body
//! sticks out of the bottom edge by a stated requirement, and 30 small parts. The libraries
//! are our sample ones plus an invented project library for the plug, so no KiCad is needed.

use netsmith_kicad::testlib::TempDir;
use std::fs;
use std::time::Instant;

use netsmith::Libraries;
use netsmith::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith_check::Loader;
use netsmith_fab::KicadCliExe;
use netsmith_kicad::{Mapping, SymbolLib, testlib};
use netsmith_pcb::{Board, FootprintLibrary, Overhang, place_options};
use netsmith_solve::Catalogs;

/// The plug's footprint: a courtyard of 42 x 28 mm, a body (F.Fab) of 40 x 26 mm, and four
/// pads at its +x end. Turned 90 degrees (counterclockwise on screen) the +x end is the top
/// and the body is 26 mm wide and 40 mm high.
const PLUG_FOOTPRINT: &str = "(footprint \"WidePlug-4\" (layer \"F.Cu\") \
    (property \"Reference\" \"REF**\" (at 0 -16 0) (layer \"F.SilkS\") (effects (font (size 1 1)))) \
    (attr smd) \
    (pad \"1\" smd rect (at 17 -4.5) (size 2 1.5) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"2\" smd rect (at 17 -1.5) (size 2 1.5) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"3\" smd rect (at 17 1.5) (size 2 1.5) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (pad \"4\" smd rect (at 17 4.5) (size 2 1.5) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
    (fp_rect (start -20 -13) (end 20 13) (stroke (width 0.1) (type solid)) (fill none) (layer \"F.Fab\")) \
    (fp_rect (start -21 -14) (end 21 14) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))";

fn plug_symbol() -> String {
    let font = "(effects (font (size 1.27 1.27)))";
    let prop = |name: &str, value: &str, y: f64| {
        format!("(property \"{name}\" \"{value}\" (at 0 {y} 0) {font})")
    };
    let pins: String = (1..=4)
        .map(|n| {
            format!(
                "(pin passive line (at -7.62 {} 0) (length 2.54) (name \"P{n}\" {font}) \
                 (number \"{n}\" {font})) ",
                5.08 - 2.54 * f64::from(n)
            )
        })
        .collect();
    format!(
        "(kicad_symbol_lib (version 20241209) (generator \"netsmith-test\") \
         (symbol \"WidePlug\" (exclude_from_sim no) (in_bom yes) (on_board yes) {} {} {} {} {} \
         (symbol \"WidePlug_0_1\" (rectangle (start -5.08 7.62) (end 5.08 -7.62) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"WidePlug_1_1\" {pins})))\n",
        prop("Reference", "J", 8.89),
        prop("Value", "WidePlug", -8.89),
        prop("Footprint", "", 0.0),
        prop("Datasheet", "~", 0.0),
        prop("Description", "", 0.0),
    )
}

/// The sample libraries and the project library `wide` holding the plug.
fn libraries(dir: &std::path::Path, footprint: &str) -> Libraries {
    fs::create_dir_all(dir.join("libs/wide.pretty")).unwrap();
    fs::write(
        dir.join("kicad.toml"),
        "[[library]]\nname = \"wide\"\nsymbols = \"libs/wide.kicad_sym\"\n\
         footprints = \"libs/wide.pretty\"\n\n\
         [[part]]\nkind = \"WidePlug\"\npackage = \"wide\"\ndefault = true\n\
         symbol = \"wide:WidePlug\"\nfootprint = \"wide:WidePlug-4\"\n",
    )
    .unwrap();
    fs::write(dir.join("libs/wide.kicad_sym"), plug_symbol()).unwrap();
    fs::write(dir.join("libs/wide.pretty/WidePlug-4.kicad_mod"), footprint).unwrap();
    Libraries::with_mapping(
        Mapping::for_project(Some(dir)).unwrap_or_else(|e| panic!("{e}")),
        SymbolLib::at(testlib::sample_symbols()),
        FootprintLibrary::at(testlib::sample_footprints()),
    )
}

/// A board's circuit source and its physical file.
struct Design {
    circuit: String,
    physical: String,
}

impl Design {
    /// The circuit source with `from` replaced by `to`.
    fn replace(mut self, from: &str, to: &str) -> Self {
        self.circuit = self.circuit.replace(from, to);
        self
    }

    fn loader(&self) -> Loader {
        Loader::std().with_physical("widget.nsm", "widget.physical.nsm", &self.physical)
    }
}

/// The board: the plug, `small` small parts (20 resistors and the rest capacitors) all between
/// the two supply nets, and in its physical file the outline and `extra` lines.
fn source(width: u32, height: u32, small: usize, extra: &str) -> Design {
    let mut parts = String::new();
    for n in 1..=small {
        if n % 3 == 0 {
            parts += &format!(
                "    .c{n}: Capacitor {{ .capacitance &= 100nF +/- 20% }}\n    .rail.pos ~> .c{n} ~> .rail.neg\n"
            );
        } else {
            parts += &format!(
                "    .r{n}: Resistor {{ .resistance &= 15kohm +/- 1% }}\n    .rail.pos ~> .r{n} ~> .rail.neg\n"
            );
        }
    }
    let circuit = format!(
        "import std::parts::passive::{{Resistor, Capacitor}}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

/// An invented wide plug whose body sticks out of the case.
part WidePlug {{
    .designator &= \"J\"
    pins {{
        .p1 passive \"P1\" \"1\"
        .p2 passive \"P2\" \"2\"
        .p3 passive \"P3\" \"3\"
        .p4 passive \"P4\" \"4\"
    }}
}}

board Widget {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .rail: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }}
    .plug: WidePlug {{
        .assembly &= AssemblyMode::Hand
    }}
    .rail.pos ~ .plug.p1
    .rail.pos ~ .plug.p2
    .rail.neg ~ .plug.p3
    .rail.neg ~ .plug.p4
{parts}}}
"
    );
    let physical = format!(
        "import std::pcb::Outline

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= {width}mm
            .height &= {height}mm
        }}
    }}
{extra}}}
"
    );
    Design { circuit, physical }
}

/// The plug turned so that its pads are at the top and its body ends 15 mm (14.5 to 15.5)
/// past the bottom edge, as the case drawing would say.
const DECLARED: &str =
    "    // Case drawing, slot B: the plug's body sticks 15 mm out of the bottom edge.
    .plug.place.rotation &= 90
    .plug.place.bottom >= .outline.height + 14.5mm
    .plug.place.bottom <= .outline.height + 15.5mm
";

struct Run {
    dir: TempDir,
    made: Result<Vec<Fabricated>, String>,
}

fn fab(name: &str, source: &Design, layout: &str) -> Run {
    fab_checked(name, source, layout, false)
}

/// [`fab`], with KiCad's plot and design rules check compared with netsmith's if `cross`.
fn fab_checked(name: &str, source: &Design, layout: &str, cross: bool) -> Run {
    fab_with_footprint(name, PLUG_FOOTPRINT, source, layout, cross)
}

/// [`fab_checked`], with the plug's footprint `footprint`.
fn fab_with_footprint(
    name: &str,
    footprint: &str,
    source: &Design,
    layout: &str,
    cross: bool,
) -> Run {
    let dir = TempDir::unused(&format!("overhang-{name}"));
    let project = dir.join("project");
    let libs = libraries(&project, footprint);
    let file = project.join("widget.pcb.toml");
    if !layout.is_empty() {
        fs::write(&file, layout).unwrap();
    }
    let cli = if cross {
        KicadCliExe::locate_runnable()
    } else {
        None
    };
    let made = fab_with_options(
        "widget.nsm",
        &source.circuit,
        &source.loader(),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &libs,
        cli.as_ref().map(|c| c as &dyn netsmith_fab::KicadCli),
        &FabOptions {
            layout: Some(file),
            cross_check: cli.is_some(),
            ..FabOptions::default()
        },
    );
    Run { dir, made }
}

/// The error text of a run that must fail.
fn failure(run: &Run) -> String {
    run.made
        .as_ref()
        .expect_err("this board must be refused")
        .clone()
}

#[test]
fn a_plug_that_hangs_over_the_bottom_edge_by_a_stated_requirement_places_routes_and_passes_the_check()
 {
    let started = Instant::now();
    let run = fab("declared", &source(35, 60, 30, DECLARED), "");
    let made = run.made.as_ref().unwrap_or_else(|e| panic!("{e}"));
    eprintln!(
        "30 small parts and the plug: placed, routed and checked in {:?}",
        started.elapsed()
    );
    let notes = &made[0].notes;
    assert!(
        notes.contains("design rules check: passed")
            && notes.contains("constraints: 2 of 2 satisfied"),
        "{notes}"
    );
    // The notes say what hangs over, how far, and which requirement said it may.
    assert!(
        notes.contains("warning: J1 hangs over the bottom edge: its courtyard is ")
            && notes.contains("mm past it (`plug.place.bottom (widget.physical.nsm:")
            && notes.contains(")` allows 74.5 to 75.5 mm)"),
        "{notes}"
    );
    // The small parts are all on the top face: nothing was moved to the back to make room.
    assert!(!notes.contains("bottom side unverified"), "{notes}");
    // The body ends 74.5 mm from the outline's top, 20 mm below the plug's origin.
    let pcb = fs::read_to_string(run.dir.join("fab/Widget/Widget.kicad_pcb")).unwrap();
    let plug = pcb
        .split("(footprint ")
        .find(|f| f.contains("WidePlug-4"))
        .expect("the plug");
    let at = plug
        .lines()
        .find_map(|l| l.trim().strip_prefix("(at "))
        .expect("its place");
    let numbers: Vec<f64> = at
        .trim_end_matches(')')
        .split_whitespace()
        .map(|n| n.parse().unwrap())
        .collect();
    assert_eq!(numbers[2], 90.0, "{at}");
    // The sheet's origin is 20 mm from the page corner (ADR 0032).
    let y = numbers[1] - 20.0;
    assert!(
        (54.4..=55.6).contains(&y),
        "the plug's origin is at y = {y} mm: {at}"
    );
}

#[test]
fn kicad_agrees_that_the_board_with_the_overhanging_plug_is_clean() {
    if KicadCliExe::locate_runnable().is_none() {
        return;
    }
    let run = fab_checked("kicad", &source(35, 60, 30, DECLARED), "", true);
    let made = run.made.as_ref().unwrap_or_else(|e| panic!("{e}"));
    eprintln!("{}", made[0].notes);
    assert!(
        made[0].notes.contains("design rules check: passed"),
        "{}",
        made[0].notes
    );
}

#[test]
fn the_same_source_gives_the_same_board_with_an_overhang() {
    let read = |name: &str| {
        let run = fab(name, &source(35, 60, 30, DECLARED), "");
        run.made.as_ref().unwrap_or_else(|e| panic!("{e}"));
        fs::read_to_string(run.dir.join("fab/Widget/Widget.kicad_pcb")).unwrap()
    };
    assert_eq!(read("same-a"), read("same-b"));
}

#[test]
fn an_overhang_nobody_stated_stops_fab_with_the_edge_and_the_numbers() {
    // The same plug, put there by where its centre is: no requirement says its body reaches
    // the edge, so nothing says it may hang over.
    let extra = "    .plug.place.rotation &= 90
    .plug.place.x >= 17.5mm
    .plug.place.x <= 17.5mm
    .plug.place.y >= 55mm
    .plug.place.y <= 55mm
";
    let run = fab("undeclared", &source(35, 60, 30, extra), "");
    let e = failure(&run);
    assert!(
        e.contains("J1 is not wholly inside the board outline: its courtyard is 16.00 mm past the bottom edge, ")
            && e.contains("the design does not say it may hang over an edge")
            && e.contains("to allow it past the bottom edge, require the body to reach it, for example `.plug.place.bottom >= outline.height`"),
        "{e}"
    );
    assert!(!run.dir.join("fab/Widget/Widget-gerbers.zip").exists());
}

#[test]
fn an_overhang_in_a_direction_nobody_stated_is_still_an_error_beside_the_stated_one() {
    // The plug's centre is at x = 25 mm, so its courtyard (14 mm either side) is 4 mm past the
    // right edge, which the bottom requirement does not say.
    let extra = format!(
        "{DECLARED}    .plug.place.x >= 25mm
    .plug.place.x <= 25mm
"
    );
    let run = fab("direction", &source(35, 60, 30, &extra), "");
    let e = failure(&run);
    assert!(
        e.contains("J1 is not wholly inside the board outline: its courtyard is 4.00 mm past the right edge, ")
            && e.contains("and the design says it may hang over only the bottom edge (`plug.place.bottom (widget.physical.nsm:")
            && e.contains("for example `.plug.place.right >= outline.width`"),
        "{e}"
    );
    assert!(
        !e.contains("past the bottom edge"),
        "the stated direction is not reported: {e}"
    );
}

#[test]
fn a_pad_outside_the_outline_is_still_an_error_whatever_is_stated() {
    // Turned 270 degrees the plug's pads are at the end that hangs over, and the requirements
    // pin it so that the placer cannot move it.
    let extra = "    .plug.place.rotation &= 270
    .plug.place.x >= 17mm
    .plug.place.x <= 17mm
    .plug.place.bottom >= .outline.height + 15mm
    .plug.place.bottom <= .outline.height + 15mm
";
    let run = fab("pads", &source(35, 60, 30, extra), "");
    let e = failure(&run);
    assert!(
        e.contains("a pad of J1 is ") && e.contains(" mm outside the board edge"),
        "{e}"
    );
    assert!(
        !e.contains("is not wholly inside the board outline"),
        "the courtyard past the bottom edge is the stated overhang: {e}"
    );
}

#[test]
fn a_plug_whose_pads_cannot_stay_inside_is_not_placed_and_the_placer_says_what_it_ended_with() {
    // The same, with the plug free to move along the window: no position keeps the pads on
    // the board.
    let extra = "    .plug.place.rotation &= 270
    .plug.place.bottom >= .outline.height + 14.5mm
    .plug.place.bottom <= .outline.height + 15.5mm
";
    let run = fab("pads-free", &source(35, 60, 30, extra), "");
    let e = failure(&run);
    assert!(
        e.contains(" part(s) do not fit the outline of 35.0 x 60.0 mm: ")
            && e.contains("placing them in the outline anyway ended with ")
            && e.contains("part(s) outside the outline or inside its 0.4 mm margin"),
        "{e}"
    );
    // The rows' size is not what is wrong, and is not given; J1 leads, with how far.
    assert!(!e.contains("the parts need "), "{e}");
    assert!(e.contains("by mm past what is kept clear: J1 "), "{e}");
    // The full list is in a file beside the board's output, and the message names it.
    let file = run.dir.join("fab").join("placement-failure.txt");
    assert!(
        e.contains(&format!("is in {}", file.display())),
        "the message names {}: {e}",
        file.display()
    );
    let list = fs::read_to_string(&file).unwrap_or_else(|err| panic!("{}: {err}", file.display()));
    assert!(
        list.contains("part(s) do not fit the outline of 35.0 x 60.0 mm"),
        "{list}"
    );
    // One line for every part: the plug, the 30 resistors and the capacitor.
    let parts = list.lines().filter(|l| l.contains(" courtyard ")).count();
    assert_eq!(parts, 32, "{list}");
    assert!(
        list.lines()
            .any(|l| l.starts_with("J1 (") && l.contains("a pad is ")),
        "{list}"
    );
    // The part is named, with the way it is outside.
    assert!(
        e.contains("parts outside the outline or inside its margin (the furthest first):\n    J1: a pad is "),
        "{e}"
    );
}

#[test]
fn a_plug_locked_in_the_layout_file_may_hang_over_the_edge_its_requirement_states() {
    // The lock puts the plug's origin at (17.5, 55) turned 90 degrees, and the body, 40 mm
    // long, ends 15 mm past the bottom edge. The design says that edge may be passed.
    let layout =
        "version = 1\n[Widget.parts.plug]\nat = [17.5, 55.0]\nrotation = 90.0\nlock = true\n";
    let said = "    .plug.place.bottom >= .outline.height\n";
    let run = fab("locked", &source(35, 60, 30, said), layout);
    let made = run.made.as_ref().unwrap_or_else(|e| panic!("{e}"));
    assert!(
        made[0].notes.contains("design rules check: passed")
            && made[0]
                .notes
                .contains("warning: J1 hangs over the bottom edge: its courtyard is 16.00 mm past it (`plug.place.bottom (widget.physical.nsm:"),
        "{}",
        made[0].notes
    );
}

#[test]
fn a_locked_plug_past_an_edge_nobody_stated_is_an_undeclared_overhang() {
    // Owner decision, 2026-11-03 (ADR 0045, section 10): a lock does not allow an edge. The
    // same lock as above, with no requirement, is the error, which names the line to write.
    let layout =
        "version = 1\n[Widget.parts.plug]\nat = [17.5, 55.0]\nrotation = 90.0\nlock = true\n";
    let run = fab("locked-unstated", &source(35, 60, 30, ""), layout);
    let e = failure(&run);
    assert!(
        e.contains("J1 is not wholly inside the board outline: its courtyard is 16.00 mm past the bottom edge")
            && e.contains("does not say it may hang over an edge")
            && e.contains("outline.height"),
        "{e}"
    );
    // A requirement the lock meets, but whose window reaches inside the board, does not
    // state the overhang either.
    let said = "    .plug.place.bottom >= .outline.height - 20mm\n";
    let run = fab("locked-inside-window", &source(35, 60, 30, said), layout);
    let e = failure(&run);
    assert!(
        e.contains("its courtyard is 16.00 mm past the bottom edge"),
        "{e}"
    );
}

#[test]
fn a_plug_moved_by_hand_without_a_lock_is_an_undeclared_overhang() {
    let layout = "version = 1\n[Widget.parts.plug]\nat = [17.5, 55.0]\nrotation = 90.0\n";
    let run = fab("moved", &source(35, 60, 30, ""), layout);
    let e = failure(&run);
    assert!(
        e.contains("J1 is not wholly inside the board outline: its courtyard is 16.00 mm past the bottom edge"),
        "{e}"
    );
}

#[test]
fn a_locked_plug_with_a_pad_off_the_board_is_still_an_error() {
    // Turned 270 degrees the pads are at the bottom end, 17 mm below the origin.
    let layout =
        "version = 1\n[Widget.parts.plug]\nat = [17.5, 55.0]\nrotation = 270.0\nlock = true\n";
    let run = fab("locked-pads", &source(35, 60, 30, ""), layout);
    let e = failure(&run);
    assert!(
        e.contains("a pad of J1 is 13.00 mm outside the board edge"),
        "{e}"
    );
}

#[test]
fn a_part_wider_than_the_board_is_turned_by_the_placer_and_the_rows_do_not_veto() {
    // The plug is 42 mm wide, and the outline 35: the rows (which never turn a part) need
    // 46 mm, and the board used to be refused before the placer tried. Nothing is stated here,
    // so the plug stays wholly inside.
    let run = fab("turned", &source(35, 60, 30, ""), "");
    let made = run.made.as_ref().unwrap_or_else(|e| panic!("{e}"));
    assert!(
        made[0].notes.contains("design rules check: passed")
            && !made[0].notes.contains("hangs over"),
        "{}",
        made[0].notes
    );
}

/// Place `source` (no routing), and say how long it took.
fn placed(source: &Design, layout: Option<&netsmith_pcb::Layout>) -> (Result<Board, String>, f64) {
    let dir = TempDir::new("overhang-placed");
    let libs = libraries(dir.path(), PLUG_FOOTPRINT);
    let built = netsmith::build_with(
        "widget.nsm",
        &source.circuit,
        &source.loader(),
        &Catalogs::jlcpcb(),
        &libs,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let design = &built[0].design;
    let started = Instant::now();
    let board = Board::from_design_placed(
        design,
        &libs.mapping,
        &libs.footprints,
        &place_options(design, layout),
    )
    .map_err(|e| e.to_string());
    (board, started.elapsed().as_secs_f64())
}

#[test]
fn the_declared_overhang_is_kept_on_the_board_and_a_stated_one_is_a_stated_one() {
    let (board, _) = placed(&source(35, 60, 30, DECLARED), None);
    let board = board.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(board.check_placement(), vec![]);
    assert!(matches!(
        board.overhang.get("J1"),
        Some(Overhang::Stated { .. })
    ));
    let hangs = board.hangs();
    assert_eq!(hangs.len(), 1, "{hangs:?}");
    assert_eq!(hangs[0].edge, netsmith_pcb::Edge::Bottom);
}

#[test]
fn a_crowded_board_places_and_reports_how_long_it_took() {
    // 100 small parts in 35 x 60 mm: the rows (about 30 mm wide) overflow the outline, and
    // the annealer places them anyway. The timing is for the report, not an assertion.
    for n in [30usize, 100, 200] {
        let plain = source(35, 60, n, "")
            .replace("    .plug: WidePlug {\n        .assembly &= AssemblyMode::Hand\n    }\n", "")
            .replace("    .rail.pos ~ .plug.p1\n    .rail.pos ~ .plug.p2\n    .rail.neg ~ .plug.p3\n    .rail.neg ~ .plug.p4\n", "");
        let (board, seconds) = placed(&plain, None);
        let board = board.unwrap_or_else(|e| panic!("{n} parts: {e}"));
        assert_eq!(board.check_placement(), vec![], "{n} parts");
        let back = board
            .parts
            .iter()
            .filter(|p| p.side == netsmith_pcb::Side::Back)
            .count();
        eprintln!("{n} small parts in 35 x 60 mm: placed in {seconds:.2} s ({back} on the back)");
    }
    // The same with the wide plug hanging over the bottom edge: the rows are too big, so the
    // placer runs the annealer on the faces the parts are on, and there is no second try.
    let (board, seconds) = placed(&source(35, 60, 100, DECLARED), None);
    let board = board.unwrap_or_else(|e| panic!("100 parts and the plug: {e}"));
    assert_eq!(board.check_placement(), vec![]);
    eprintln!("100 small parts and the plug in 35 x 60 mm: placed in {seconds:.2} s");
}

#[test]
fn too_many_parts_for_the_outline_are_refused_with_what_the_placer_ended_with() {
    let plain = source(12, 12, 100, "")
        .replace("    .plug: WidePlug {\n        .assembly &= AssemblyMode::Hand\n    }\n", "")
        .replace("    .rail.pos ~ .plug.p1\n    .rail.pos ~ .plug.p2\n    .rail.neg ~ .plug.p3\n    .rail.neg ~ .plug.p4\n", "");
    let (board, seconds) = placed(&plain, None);
    let e = board.expect_err("100 parts do not fit 12 x 12 mm");
    eprintln!("100 parts in 12 x 12 mm: refused in {seconds:.2} s: {e}");
    assert!(
        e.contains(" part(s) do not fit the outline of 12.0 x 12.0 mm: ")
            && e.contains("placing them in the outline anyway ended with "),
        "{e}"
    );
}

#[test]
fn silkscreen_over_the_edge_stays_a_design_rules_warning() {
    // The overhang is about the courtyard. A footprint that draws silkscreen across the board
    // edge still gets the design rules check's silk to edge warning (a warning, as in KiCad),
    // which is the footprint's to fix: draw the silkscreen inside, or none.
    let with_silk = PLUG_FOOTPRINT.replace(
        "(fp_rect (start -20 -13)",
        "(fp_line (start -20 0) (end 0 0) (stroke (width 0.12) (type solid)) (layer \"F.SilkS\")) \
         (fp_rect (start -20 -13)",
    );
    let run = fab_with_footprint("silk", &with_silk, &source(35, 60, 30, DECLARED), "", false);
    let made = run.made.as_ref().unwrap_or_else(|e| panic!("{e}"));
    assert!(
        made[0].notes.contains("warning [silk_edge_clearance]"),
        "{}",
        made[0].notes
    );
}

/// The plug turned so that its pads are at the top, with its body 25 mm (24.5 to 25.5) past
/// the bottom edge: its centre is then 5 mm below the board.
const CENTRE_OUTSIDE: &str = "    .plug.place.rotation &= 90
    .plug.place.bottom >= .outline.height + 24.5mm
    .plug.place.bottom <= .outline.height + 25.5mm
";

#[test]
fn a_body_whose_centre_is_off_the_board_may_be_required_when_the_edge_field_states_the_overhang() {
    // PROVISIONAL (ADR 0045, section 10): the body is 40 mm long and ends 25 mm past the
    // bottom edge, so its centre is 5 mm outside; the pads, at the other end, are inside.
    let run = fab("centre-outside", &source(35, 60, 30, CENTRE_OUTSIDE), "");
    let made = run.made.as_ref().unwrap_or_else(|e| panic!("{e}"));
    let notes = &made[0].notes;
    assert!(
        notes.contains("design rules check: passed")
            && notes.contains("constraints: 2 of 2 satisfied")
            && notes.contains("warning: J1 hangs over the bottom edge"),
        "{notes}"
    );
}

#[test]
fn a_window_on_y_whose_centre_is_off_the_board_is_still_refused() {
    // The old rule stays for `.x` and `.y`: the centre 5 mm below a 60 mm board.
    let extra = "    .plug.place.rotation &= 90
    .plug.place.y >= 65mm
    .plug.place.y <= 66mm
";
    let run = fab("y-outside", &source(35, 60, 30, extra), "");
    let e = failure(&run);
    assert!(
        e.contains("is required to be where its body's centre is 65 to 66 mm from the top edge")
            && e.contains("and the outline is 60 mm high"),
        "{e}"
    );
}

#[test]
fn an_overhang_stated_on_the_other_edge_does_not_let_the_centre_leave_by_this_one() {
    // The bottom edge states an overhang; the window puts the centre past the right edge
    // (x between 36 and 37 on a 35 mm board), which nothing states.
    let extra = format!(
        "{CENTRE_OUTSIDE}    .plug.place.x >= 36mm
    .plug.place.x <= 37mm
"
    );
    let run = fab("x-outside", &source(35, 60, 30, &extra), "");
    let e = failure(&run);
    assert!(
        e.contains("is required to be where its body's centre is 36 to 37 mm from the left edge")
            && e.contains("and the outline is 35 mm wide"),
        "{e}"
    );
}
