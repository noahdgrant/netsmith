//! Panels (ADR 0040): a `panel` repeats one board in a grid with V-cuts. The package, its
//! checks and the order that names a panel need no KiCad; the cross-check with KiCad's plot of
//! the panel board skips when KiCad is not installed.

use netsmith_testkit::TempDir;
use std::fs;
use std::io::Read as _;
use std::path::Path;

use netsmith_pipeline::Libraries;
use netsmith_fabricate::fab::{FabOptions, fab_with_options};
use netsmith_check::Loader;
use netsmith_fab::{KicadCli, KicadCliExe};
use netsmith_solve::Catalogs;

const PANEL: &str = include_str!("../../../examples/led_panel.nsm");
/// The physical file of the board the panel repeats: its outline.
const PANEL_PHYSICAL: &str = include_str!("../../../examples/led_panel.physical.nsm");

/// The standard library and the example's physical file, paired with `panel.nsm`.
fn loader() -> Loader {
    Loader::std().with_physical("panel.nsm", "panel.physical.nsm", PANEL_PHYSICAL)
}

fn out(name: &str) -> TempDir {
    TempDir::unused(&format!("panel-test-{name}"))
}

fn make(text: &str, dir: &Path) -> Result<Vec<netsmith_fabricate::fab::Fabricated>, String> {
    make_with(text, PANEL_PHYSICAL, dir)
}

/// [`make`], with `physical` as the board's physical file.
fn make_with(
    text: &str,
    physical: &str,
    dir: &Path,
) -> Result<Vec<netsmith_fabricate::fab::Fabricated>, String> {
    netsmith_fabricate::fab::fab_with(
        "panel.nsm",
        text,
        &Loader::std().with_physical("panel.nsm", "panel.physical.nsm", physical),
        dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
    )
}

fn zipped(dir: &Path, folder: &str, name: &str, file: &str) -> String {
    let zip = fs::File::open(dir.join(folder).join(format!("{name}-gerbers.zip"))).unwrap();
    let mut archive = zip::ZipArchive::new(zip).unwrap();
    let mut text = String::new();
    archive
        .by_name(&format!("gerber/{name}-{file}"))
        .unwrap_or_else(|e| panic!("{file}: {e}"))
        .read_to_string(&mut text)
        .unwrap();
    text
}

fn holes(drill: &str) -> usize {
    drill.lines().filter(|l| l.starts_with('X')).count()
}

#[test]
fn a_panel_of_six_boards_has_six_times_the_holes_parts_and_one_outline() {
    if netsmith_kicad::skip_without_kicad_libraries() {
        return;
    }
    let dir = out("six");
    let made = make(PANEL, &dir).unwrap_or_else(|e| panic!("{e}"));
    let labels: Vec<&str> = made.iter().map(|m| m.board.as_str()).collect();
    assert_eq!(labels, ["LedTile", "LedPanel"]);

    // The board alone, then the panel and the board it was made from, in `cell/`.
    let board_holes = holes(&zipped(&dir, "LedTile", "LedTile", "PTH.drl"));
    assert_eq!(board_holes, 2, "the header's two holes");
    assert_eq!(
        holes(&zipped(&dir, "LedPanel", "LedPanel", "PTH.drl")),
        6 * board_holes
    );
    // Four tooling holes on the rails, and nothing else is unplated.
    assert_eq!(holes(&zipped(&dir, "LedPanel", "LedPanel", "NPTH.drl")), 4);
    assert!(
        dir.join("LedPanel/cell/LedTile/LedTile-gerbers.zip")
            .is_file()
    );

    // The panel is 3 x 2 boards of 40 x 30 mm and 5 mm rails: 130 x 70 mm.
    let pcb = fs::read_to_string(dir.join("LedPanel/LedPanel.kicad_pcb")).unwrap();
    let art = netsmith_fab::artwork::Artwork::parse(&pcb).unwrap();
    let outline = netsmith_fab::edge::outline(&art).unwrap();
    let bounds = netsmith_fab::geom::Bounds::of(outline.outer.iter().copied()).unwrap();
    let (w, h) = (bounds.max.x - bounds.min.x, bounds.max.y - bounds.min.y);
    assert_eq!((w, h), (130.0, 70.0));
    assert_eq!(netsmith_fab::edge::cut_lines(&art).len(), 4 + 3);
    // The Gerber outline holds that, and the marks beside it; the copper lies inside it.
    let edge =
        netsmith_fab::raster::gerber_extent(&zipped(&dir, "LedPanel", "LedPanel", "Edge_Cuts.gbr"))
            .unwrap();
    assert!(
        edge.max.0 - edge.min.0 >= 130.0 && edge.max.1 - edge.min.1 >= 70.0,
        "{edge:?}"
    );
    let copper =
        netsmith_fab::raster::gerber_extent(&zipped(&dir, "LedPanel", "LedPanel", "F_Cu.gbr"))
            .unwrap();
    assert!(
        copper.min.0 > edge.min.0 && copper.max.0 < edge.max.0,
        "{copper:?} inside {edge:?}"
    );
    assert!(
        copper.max.0 - copper.min.0 <= 130.0 && copper.max.1 - copper.min.1 <= 70.0,
        "{copper:?}"
    );

    // The bill of materials: six of each part, a designator for each cell.
    let bom = fs::read_to_string(dir.join("LedPanel/bom.csv")).unwrap();
    let lines: Vec<&str> = bom.lines().skip(1).collect();
    assert_eq!(lines.len(), 2, "{bom}");
    for line in lines {
        let designators = line.split('"').nth(1).unwrap_or_else(|| panic!("{line}"));
        assert_eq!(designators.split(',').count(), 6, "{line}");
    }
    assert!(
        bom.contains("R1_1") && bom.contains("R1_6") && bom.contains("D1_4"),
        "{bom}"
    );
    let cpl = fs::read_to_string(dir.join("LedPanel/cpl.csv")).unwrap();
    assert_eq!(cpl.lines().count(), 1 + 12, "{cpl}");

    let notes = fs::read_to_string(dir.join("LedPanel/fab-notes.txt")).unwrap();
    assert!(notes.contains("design rules check: passed"), "{notes}");
    assert!(
        notes.contains("3 columns x 2 rows of LedTile (6 boards of 40 x 30 mm)"),
        "{notes}"
    );
    // The rotation of the LED is unverified once for the board, not for each cell.
    assert_eq!(made[1].facts.unverified, ["D1"]);
    assert_eq!(
        made[1]
            .facts
            .bom
            .iter()
            .map(|e| e.designators.len())
            .sum::<usize>(),
        12
    );
    assert!(made[1].facts.panel.is_some() && made[0].facts.panel.is_none());
}

#[test]
fn the_panel_board_agrees_with_kicads_plot_of_it() {
    let Some(cli) = KicadCliExe::locate_runnable() else {
        return;
    };
    let dir = out("cross-check");
    let options = FabOptions {
        cross_check: true,
        ..FabOptions::default()
    };
    let made = fab_with_options(
        "panel.nsm",
        PANEL,
        &loader(),
        &dir,
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        Some(&cli as &dyn KicadCli),
        &options,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let panel = made.iter().find(|m| m.board == "LedPanel").unwrap();
    assert!(
        panel
            .notes
            .contains("cross-check against KiCad: the files agree"),
        "{}",
        panel.notes
    );
    assert!(panel.notes.contains("Edge_Cuts.gbr"), "{}", panel.notes);
}

/// `PANEL` with `from` replaced by `to`, which must be there.
fn changed(from: &str, to: &str) -> String {
    assert!(PANEL.contains(from), "{from}");
    PANEL.replace(from, to)
}

#[test]
fn a_panel_that_the_board_house_would_refuse_says_why_with_the_numbers() {
    if netsmith_kicad::skip_without_kicad_libraries() {
        return;
    }
    let dir = out("refused");
    // Copper only 0.2 mm from the edge.
    let e = make(
        &changed(
            ".min_edge_clearance &= 0.4mm..1m",
            ".min_edge_clearance &= 0.2mm..1m",
        ),
        &dir,
    )
    .unwrap_err();
    assert!(
        e.contains("error: panel LedPanel: panel LedPanel: LedTile keeps copper 0.2 mm from its edge and copper is at least 0.4 mm from a V-cut edge")
            || e.contains("LedTile keeps copper 0.2 mm from its edge and copper is at least 0.4 mm from a V-cut edge"),
        "{e}"
    );
    // A rail that is too narrow to carry the marks.
    let e = make(&changed(".rail &= 5mm", ".rail &= 3mm"), &dir).unwrap_err();
    assert!(e.contains("the rail is 3 mm wide and a rail that carries tooling holes and fiducials is at least 5 mm"), "{e}");
    // Assembled with no rail.
    let e = make(&changed(".rail &= 5mm", ".rail &= 0mm"), &dir).unwrap_err();
    assert!(
        e.contains("needs tooling holes and fiducials on its rails: add `.rail &= 5mm`"),
        "{e}"
    );
    // Too small: 3 x 1 boards with rails are 130 x 40 mm.
    let e = make(&changed(".rows &= 2", ".rows &= 1"), &dir).unwrap_err();
    assert!(
        e.contains("the panel is 40 mm high and a V-cut panel is at least 70 mm each way"),
        "{e}"
    );
    // Too big to be assembled: 7 boards across are 290 mm.
    let e = make(&changed(".columns &= 3", ".columns &= 7"), &dir).unwrap_err();
    assert!(
        e.contains(
            "the panel is 290 mm wide and a panel that is assembled is at most 250 mm each way"
        ),
        "{e}"
    );
    // The board's corners.
    assert!(PANEL_PHYSICAL.contains(".corner_radius &= 0mm"));
    let e = make_with(
        PANEL,
        &PANEL_PHYSICAL.replace(".corner_radius &= 0mm", ".corner_radius &= 2mm"),
        &dir,
    )
    .unwrap_err();
    assert!(e.contains("not a rectangle with square corners"), "{e}");
    // A board too thin to be cut (a board that leaves its thickness open is refused before).
    let e = make(
        &changed(".stackup.thickness &= 1.6mm", ".stackup.thickness &= 0.4mm"),
        &dir,
    )
    .unwrap_err();
    assert!(
        e.contains("LedTile is 0.4 mm thick and a V-cut board is at least 0.6 mm thick"),
        "{e}"
    );
}

#[test]
fn the_panel_definition_is_checked_like_the_rest_of_the_language() {
    if netsmith_kicad::skip_without_kicad_libraries() {
        return;
    }
    let dir = out("language");
    // No board.
    let e = make(&changed("    .cell: LedTile\n", ""), &dir).unwrap_err();
    assert!(
        e.contains("panel LedPanel: it has no board to repeat: add `.cell: SomeBoard`"),
        "{e}"
    );
    // No rules of a board house.
    let e = make(&changed("    .fab: VCutPanel {\n        .columns &= 3\n        .rows &= 2\n        .rail &= 5mm\n    }\n", ""), &dir).unwrap_err();
    assert!(
        e.contains("it does not say who makes it") && e.contains("VCutPanel"),
        "{e}"
    );
    // The grid is not stated.
    let e = make(&changed("        .columns &= 3\n", ""), &dir).unwrap_err();
    assert!(e.contains("no value for `columns`: say how many"), "{e}");
    // A grid of no boards.
    let e = make(&changed(".columns &= 3", ".columns &= 0"), &dir).unwrap_err();
    assert!(e.contains("a panel has at least one column and one row, but `.fab.columns` is 0 and `.fab.rows` is 2"), "{e}");
    // A panel holds boards and its rules, nothing else.
    let e = make(
        &changed(
            "    .cell: LedTile\n",
            "    .cell: LedTile\n    .r: Resistor {}\n",
        ),
        &dir,
    )
    .unwrap_err();
    assert!(
        e.contains("a panel holds boards, but `Resistor` is not one")
            || e.contains("a panel holds the board it repeats"),
        "{e}"
    );
    let e = make(
        &changed(
            "    .cell: LedTile\n",
            "    .cell: LedTile\n    .other: LedTile\n",
        ),
        &dir,
    )
    .unwrap_err();
    assert!(
        e.contains("a panel repeats one board; this panel already names its board"),
        "{e}"
    );
    // A panel joins nothing, and does not reach into its board.
    let e = make(
        &changed(
            "    .cell: LedTile\n",
            "    .cell: LedTile\n    .cell.supply ~ .cell.supply\n",
        ),
        &dir,
    )
    .unwrap_err();
    assert!(e.contains("a panel joins nothing"), "{e}");
    let e = make(
        &changed(
            "    .cell: LedTile\n",
            "    .cell: LedTile\n    .cell.r1.resistance &= 1kohm\n",
        ),
        &dir,
    )
    .unwrap_err();
    assert!(
        e.contains("is inside board `LedTile`; a panel repeats its board as it is"),
        "{e}"
    );
    // The limits of the board house cannot be widened.
    let e = make(&changed(".rail &= 5mm", ".rail &= 9mm"), &dir).unwrap_err();
    assert!(e.contains("rail"), "{e}");
    // A board is placed in a system or a panel only.
    let e = make(
        &format!("{PANEL}\nboard Wrong {{\n    .p: LedTile\n}}\n"),
        &dir,
    )
    .unwrap_err();
    assert!(
        e.contains(
            "a board cannot be placed inside a board; boards are placed only in a system or a panel"
        ),
        "{e}"
    );
    // The board a panel repeats is stand-alone and in the same file.
    let two = "system S {\n    .a: LedTile\n}\npanel P {\n    .cell: LedTile\n    .fab: VCutPanel {\n        .columns &= 2\n        .rows &= 2\n    }\n}\n";
    let body = PANEL.split("/// Six `LedTile`").next().unwrap();
    let e = make(&format!("{body}\n{two}"), &dir).unwrap_err();
    assert!(
        e.contains("panel P: `LedTile` is not a stand-alone board of this file"),
        "{e}"
    );
}

// ---- orders ------------------------------------------------------------------------------
