//! `netsmith dxf import` (ADR 0045, C0 piece 3): an enclosure's DXF becomes an outline, a
//! cutout and mounting holes that the fab pipeline accepts. The DXF is written here by hand.

use netsmith_kicad::testlib::TempDir;
use std::fs;

use netsmith::Libraries;
use netsmith::dxf::{Request, import, render, write_physical};
use netsmith::fab::{FabOptions, fab_with_options};
use netsmith_dxf::Units;
use netsmith_fab::KicadCliExe;
use netsmith_solve::Catalogs;

const OUTLINE_AND_POURS: &str = include_str!("../../../examples/outline_and_pours.nsm");
const OUTLINE_AND_POURS_PHYSICAL: &str =
    include_str!("../../../examples/outline_and_pours.physical.nsm");
const OUTLINE_AND_POURS_LAYOUT: &str = include_str!("../../../examples/outline_and_pours.pcb.toml");

fn pair(code: i32, value: impl std::fmt::Display) -> String {
    format!("{code}\n{value}\n")
}

fn line(layer: &str, a: (f64, f64), b: (f64, f64)) -> String {
    pair(0, "LINE")
        + &pair(8, layer)
        + &pair(10, a.0)
        + &pair(20, a.1)
        + &pair(11, b.0)
        + &pair(21, b.1)
}

fn arc(layer: &str, c: (f64, f64), r: f64, start: f64, end: f64) -> String {
    pair(0, "ARC")
        + &pair(8, layer)
        + &pair(10, c.0)
        + &pair(20, c.1)
        + &pair(40, r)
        + &pair(50, start)
        + &pair(51, end)
}

fn circle(layer: &str, c: (f64, f64), r: f64) -> String {
    pair(0, "CIRCLE") + &pair(8, layer) + &pair(10, c.0) + &pair(20, c.1) + &pair(40, r)
}

fn rounded(layer: &str, x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> String {
    line(layer, (x0 + r, y0), (x1 - r, y0))
        + &arc(layer, (x1 - r, y0 + r), r, 270.0, 360.0)
        + &line(layer, (x1, y0 + r), (x1, y1 - r))
        + &arc(layer, (x1 - r, y1 - r), r, 0.0, 90.0)
        + &line(layer, (x1 - r, y1), (x0 + r, y1))
        + &arc(layer, (x0 + r, y1 - r), r, 90.0, 180.0)
        + &line(layer, (x0, y1 - r), (x0, y0 + r))
        + &arc(layer, (x0 + r, y0 + r), r, 180.0, 270.0)
}

fn file(insunits: Option<i32>, entities: &str) -> String {
    let mut s = pair(0, "SECTION") + &pair(2, "HEADER");
    if let Some(u) = insunits {
        s += &(pair(9, "$INSUNITS") + &pair(70, u));
    }
    s + &pair(0, "ENDSEC")
        + &pair(0, "SECTION")
        + &pair(2, "ENTITIES")
        + entities
        + &pair(0, "ENDSEC")
        + &pair(0, "EOF")
}

/// An enclosure section 50 x 36 with rounded corners, a rounded slot and four holes, placed
/// away from the drawing's origin and with y up.
fn enclosure() -> String {
    let mut e = rounded("EDGE", 100.0, 200.0, 150.0, 236.0, 3.0);
    e += &rounded("EDGE", 106.0, 212.0, 109.0, 224.0, 1.0);
    for (x, y) in [
        (105.0, 230.5),
        (145.0, 230.5),
        (105.0, 205.5),
        (145.0, 205.5),
    ] {
        e += &circle("HOLES", (x, y), 1.6);
    }
    file(Some(4), &e)
}

fn request() -> Request {
    Request {
        layers: vec!["EDGE".into()],
        holes_layers: vec!["HOLES".into()],
        units: None,
    }
}

fn temp(name: &str) -> TempDir {
    TempDir::new(&format!("dxf-test-{name}"))
}

#[test]
fn the_import_prints_source_for_the_outline_cutout_and_holes() {
    let imported = import(&enclosure(), "case.dxf", &request()).unwrap();
    assert_eq!(
        imported.imports,
        "import std::pcb::{Cutout, Outline}\nimport std::parts::mechanical::MountingHole\n"
    );
    // The places are requirements the case fixes, in the board's frame (origin top left,
    // y down): the cutout states `.x` and `.y`, a hole the centre of its body.
    assert_eq!(
        imported.members,
        ".outline: Outline {\n    .width &= 50mm\n    .height &= 36mm\n    .corner_radius &= 3mm\n}\n\n\
         .cutout1: Cutout {\n    .x &= 6mm\n    .y &= 12mm\n    .width &= 3mm\n    .height &= 12mm\n    .corner_radius &= 1mm\n}\n\n\
         .hole1: MountingHole { .package &= \"M3\" }\n.hole1.place.x &= 5mm\n.hole1.place.y &= 5.5mm\n\
         .hole2: MountingHole { .package &= \"M3\" }\n.hole2.place.x &= 45mm\n.hole2.place.y &= 5.5mm\n\
         .hole3: MountingHole { .package &= \"M3\" }\n.hole3.place.x &= 5mm\n.hole3.place.y &= 30.5mm\n\
         .hole4: MountingHole { .package &= \"M3\" }\n.hole4.place.x &= 45mm\n.hole4.place.y &= 30.5mm\n"
    );
    let text = render(&imported, Some("Case"), "case.dxf", false);
    assert!(
        text.contains("physical for Case {\n    case {\n        .outline: Outline {"),
        "{text}"
    );
    assert!(text.contains("        .hole4.place.y &= 30.5mm\n    }\n}\n"), "{text}");
    assert!(
        !text.contains("pcb.toml") && !text.contains("lock = true"),
        "no place is for the layout file: {text}"
    );
    assert!(
        text.contains("(x - 100, 236 - y)"),
        "the transform is printed: {text}"
    );
}

#[test]
fn an_l_shaped_outline_is_refused_with_its_size_and_the_roadmap() {
    let pts = [
        (0.0, 0.0),
        (40.0, 0.0),
        (40.0, 10.0),
        (15.0, 10.0),
        (15.0, 30.0),
        (0.0, 30.0),
    ];
    let e = file(
        Some(4),
        &(0..6)
            .map(|i| line("EDGE", pts[i], pts[(i + 1) % 6]))
            .collect::<String>(),
    );
    let err = import(&e, "l.dxf", &Request::default()).unwrap_err();
    assert!(
        err.contains("the outline is not a rectangle with equal rounded corners"),
        "{err}"
    );
    assert!(
        err.contains("40.000 x 30.000 mm") && err.contains("6 edges"),
        "{err}"
    );
    assert!(
        err.contains("polygon outline") && err.contains("C8"),
        "{err}"
    );
}

#[test]
fn a_hole_that_is_no_mounting_hole_package_is_refused_with_the_packages() {
    let mut e = rounded("EDGE", 0.0, 0.0, 50.0, 30.0, 2.0);
    e += &circle("HOLES", (5.0, 5.0), 2.25);
    let err = import(&file(Some(4), &e), "x.dxf", &request()).unwrap_err();
    assert!(
        err.contains("diameter of 4.5 mm") && err.contains("M2 (2.2 mm) and M3 (3.2 mm)"),
        "{err}"
    );
}

#[test]
fn inches_without_a_flag_and_a_file_with_no_units_with_one() {
    let mut e = String::new();
    e += &line("EDGE", (0.0, 0.0), (2.0, 0.0));
    e += &line("EDGE", (2.0, 0.0), (2.0, 1.0));
    e += &line("EDGE", (2.0, 1.0), (0.0, 1.0));
    e += &line("EDGE", (0.0, 1.0), (0.0, 0.0));
    let inch = import(&file(Some(1), &e), "i.dxf", &Request::default()).unwrap();
    assert!(
        inch.members.contains(".width &= 50.8mm") && inch.members.contains(".height &= 25.4mm")
    );
    let none = import(&file(None, &e), "n.dxf", &Request::default()).unwrap_err();
    assert!(none.contains("--units"), "{none}");
    let given = import(
        &file(None, &e),
        "n.dxf",
        &Request {
            units: Some(Units::Inches),
            ..Request::default()
        },
    )
    .unwrap();
    assert_eq!(given.members, inch.members);
}

#[test]
fn write_edits_the_physical_file_keeps_its_comments_and_leaves_the_source_alone() {
    let dir = temp("write");
    let source = dir.join("case.nsm");
    fs::write(&source, OUTLINE_AND_POURS).unwrap();
    let physical = dir.join("case.physical.nsm");
    // The example's physical file, with a comment of its own that has to survive.
    let old = OUTLINE_AND_POURS_PHYSICAL.replace(
        "    /// No copper and no parts on the top layer",
        "    // Kept: the antenna is the radio team's.\n    /// No copper and no parts on the top layer",
    );
    fs::write(&physical, &old).unwrap();
    let imported = import(&enclosure(), "case.dxf", &request()).unwrap();
    let (board, diff) = write_physical(&source, None, &imported).unwrap();
    assert_eq!(board, "OutlineAndPours");
    assert_eq!(
        fs::read_to_string(&source).unwrap(),
        OUTLINE_AND_POURS,
        "the source is never written"
    );
    let text = fs::read_to_string(&physical).unwrap();
    // What was there is kept, the new members are in the case section, and the file parses.
    assert!(text.contains("// Kept: the antenna is the radio team's."), "{text}");
    assert!(text.contains(".antenna: Keepout {"), "{text}");
    assert!(text.contains(".fiducial1: Fiducial"), "{text}");
    assert!(text.contains("        .hole2.place.x &= 45mm\n"), "{text}");
    assert!(text.contains(".cutout1: Cutout {"), "{text}");
    assert_eq!(text.matches(".hole1: MountingHole").count(), 1, "{text}");
    netsmith_lang::parse(&text).unwrap();
    assert!(diff.contains("+        .hole1.place.x &= 5mm"), "{diff}");
    assert!(!diff.contains("-    // Kept"), "the comment is not in the diff: {diff}");
    assert!(
        !dir.join("case.pcb.toml").exists(),
        "no place is for the layout file"
    );
    // The same import again changes nothing, and a hole moved in the drawing is an update
    // shown in the diff.
    let (_, again) = write_physical(&source, Some("OutlineAndPours"), &imported).unwrap();
    assert!(again.contains("no change"), "{again}");
    let moved = import(
        &enclosure().replace("105\n20\n230.5", "106\n20\n230.5"),
        "case.dxf",
        &request(),
    )
    .unwrap();
    assert!(moved.members.contains(".hole1.place.x &= 6mm"), "{}", moved.members);
    let (_, update) = write_physical(&source, None, &moved).unwrap();
    assert!(
        update.contains("-        .hole1.place.x &= 5mm")
            && update.contains("+        .hole1.place.x &= 6mm"),
        "{update}"
    );
}

#[test]
fn write_makes_the_physical_file_when_there_is_none() {
    let dir = temp("write-new");
    let source = dir.join("case.nsm");
    fs::write(&source, "board Case {\n}\n").unwrap();
    let imported = import(&enclosure(), "case.dxf", &request()).unwrap();
    write_physical(&source, None, &imported).unwrap();
    let text = fs::read_to_string(dir.join("case.physical.nsm")).unwrap();
    assert!(
        text.starts_with("//! The physical intent of Case.\n\nimport std::pcb::Outline\nimport std::pcb::Cutout\nimport std::parts::mechanical::MountingHole\n\nphysical for Case {\n    case {\n        .outline: Outline {"),
        "{text}"
    );
    netsmith_lang::parse(&text).unwrap();
}

#[test]
fn write_names_the_boards_when_the_choice_is_wrong() {
    let dir = temp("board");
    let source = dir.join("two.nsm");
    fs::write(&source, "board A {\n}\nboard B {\n}\n").unwrap();
    let imported = import(&enclosure(), "case.dxf", &request()).unwrap();
    let e = write_physical(&source, None, &imported).unwrap_err();
    assert!(
        e.contains("2 boards (A, B)") && e.contains("--board"),
        "{e}"
    );
    let e = write_physical(&source, Some("C"), &imported).unwrap_err();
    assert!(e.contains("no board `C`") && e.contains("A, B"), "{e}");
    assert!(!dir.join("two.physical.nsm").exists());
}

/// The example's physical file with its outline, slot and holes replaced by what the DXF said,
/// made as `dxf import --write` makes it: on a physical file that has the old ones.
fn imported_physical(dir: &std::path::Path) -> String {
    let imported = import(&enclosure(), "case.dxf", &request()).unwrap();
    let nsm = dir.join("case.nsm");
    fs::write(&nsm, OUTLINE_AND_POURS).unwrap();
    // The slot is another loop than the import's `cutout1`: take it out first.
    let start = OUTLINE_AND_POURS_PHYSICAL.find("        /// A rounded slot").unwrap();
    let end = OUTLINE_AND_POURS_PHYSICAL.find("        /// Mounting holes go").unwrap();
    let old = format!(
        "{}{}",
        &OUTLINE_AND_POURS_PHYSICAL[..start],
        &OUTLINE_AND_POURS_PHYSICAL[end..]
    );
    fs::write(dir.join("case.physical.nsm"), old).unwrap();
    write_physical(&nsm, None, &imported).unwrap();
    fs::read_to_string(dir.join("case.physical.nsm")).unwrap()
}

#[test]
fn a_board_with_the_imported_outline_goes_through_fab_with_the_holes_where_the_case_has_them() {
    let dir = temp("fab");
    let physical = imported_physical(&dir);
    assert!(
        physical.contains(".cutout1: Cutout") && !physical.contains(".slot:"),
        "{physical}"
    );
    fs::write(dir.join("case.pcb.toml"), OUTLINE_AND_POURS_LAYOUT).unwrap();

    let cli = KicadCliExe::locate();
    let made = fab_with_options(
        "case.nsm",
        OUTLINE_AND_POURS,
        &netsmith_check::Loader::std().with_physical("case.nsm", "case.physical.nsm", &physical),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        cli.as_ref().map(|c| c as &dyn netsmith_fab::KicadCli),
        &FabOptions {
            layout: Some(dir.join("case.pcb.toml")),
            ..FabOptions::default()
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.len(), 1);
    assert!(
        made[0].notes.contains("design rules check: passed"),
        "{}",
        made[0].notes
    );
    let pcb =
        fs::read_to_string(dir.join("fab/OutlineAndPours/OutlineAndPours.kicad_pcb")).unwrap();
    // The board's corner is at (20, 20) in the file; the holes are 5 mm in x and 5.5 mm in y from it, where the
    // case has them, not where the placer would have put them.
    for (x, y) in [(25, 25.5), (65, 25.5), (25, 50.5), (65, 50.5)] {
        assert!(pcb.contains(&format!("(at {x} {y})")), "hole at {x} {y}");
    }
    // Eight arcs: the corners of the outline and of the slot.
    assert_eq!(pcb.matches("(gr_arc").count(), 8);
    if let Some(cli) = cli {
        let report = netsmith_fab::crosscheck::check(
            &dir.join("fab/OutlineAndPours/OutlineAndPours.kicad_pcb"),
            &cli,
            &Default::default(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert!(report.ok(), "{report}");
    }
}

fn netsmith(args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn the_command_prints_the_source_to_paste_and_writes_only_the_physical_file() {
    let dir = temp("cli");
    let dxf = dir.join("case.dxf");
    fs::write(&dxf, enclosure()).unwrap();
    let nsm = dir.join("case.nsm");
    fs::write(&nsm, "board Case {\n}\n").unwrap();
    let args = |extra: &[&str]| {
        let mut v = vec![
            "dxf",
            "import",
            dxf.to_str().unwrap(),
            "--layer",
            "EDGE",
            "--holes-layer",
            "HOLES",
        ];
        v.extend_from_slice(extra);
        netsmith(&v)
    };
    let printed = args(&[]);
    assert!(printed.status.success(), "{printed:?}");
    let text = String::from_utf8(printed.stdout).unwrap();
    assert!(
        text.contains(".outline: Outline {") && text.contains("physical for YourBoard {"),
        "{text}"
    );
    assert!(
        !dir.join("case.physical.nsm").exists(),
        "nothing is written without --write"
    );

    let written = args(&["--write", nsm.to_str().unwrap()]);
    assert!(written.status.success(), "{written:?}");
    let text = String::from_utf8(written.stdout).unwrap();
    assert!(text.contains("+        .hole1.place.x &= 5mm"), "{text}");
    let physical = fs::read_to_string(dir.join("case.physical.nsm")).unwrap();
    assert!(physical.contains(".cutout1: Cutout {"), "{physical}");
    assert!(!dir.join("case.pcb.toml").exists());
    assert_eq!(fs::read_to_string(&nsm).unwrap(), "board Case {\n}\n");

    // Without --layer the reader asks which layer: it lists them.
    let asked = netsmith(&["dxf", "import", dxf.to_str().unwrap()]);
    assert!(!asked.status.success());
    let err = String::from_utf8(asked.stderr).unwrap();
    assert!(
        err.contains("choose with --layer") && err.contains("`HOLES` (4 CIRCLE)"),
        "{err}"
    );

    // A binary file is refused with a message and exit code 1.
    fs::write(&dxf, b"AutoCAD Binary DXF\r\n\x1a\0rest").unwrap();
    let binary = netsmith(&["dxf", "import", dxf.to_str().unwrap()]);
    assert_eq!(binary.status.code(), Some(1));
    assert!(
        String::from_utf8(binary.stderr)
            .unwrap()
            .contains("binary DXF")
    );
}
