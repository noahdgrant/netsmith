//! `netsmith case import` (ADR 0109 "Left open"): the outline, cutouts and mounting holes of an
//! invented case (`netsmith-3d/tests/models/case_box.step`, written for these tests: a wall
//! round a 60 x 40 mm pocket with a rib, a pin and a block standing through the board's plane,
//! and two standoffs below it) become source that the fab pipeline accepts.

use std::fs;

use netsmith_fabricate::fab::{FabOptions, fab_with_options};
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;
use netsmith_tools::case_import::{Request, import};
use netsmith_tools::dxf::{render, write_physical};

const CASE_BOX: &[u8] = include_bytes!("../../netsmith-3d/tests/models/case_box.step");

fn with_holes() -> Request {
    Request {
        holes_at: Some(-4.0),
        ..Request::default()
    }
}

#[test]
fn the_section_at_the_board_prints_the_outline_with_its_rib_cutouts_and_holes() {
    let imported = import(CASE_BOX, "case_box.step", &with_holes()).unwrap();
    let text = render(&imported, Some("Boxed"), "case_box.step", false);
    assert!(
        text.starts_with("// Imported from case_box.step by `netsmith case import`.\n"),
        "{text}"
    );
    assert!(text.contains("physical for Boxed {"), "{text}");
    // The pocket is 60 x 40 with corners of 2 mm and a rib 4 wide standing 6 mm out of its
    // bottom edge, which is the board's bottom (y = 40 down from the top left corner).
    assert!(
        text.contains(
            ".outline: Outline {
            polygon (60mm, 38mm) arc through (59.4114mm, 39.4114mm) (58mm, 40mm) (32mm, 40mm)
                    (32mm, 34mm) (28mm, 34mm) (28mm, 40mm)
                    (2mm, 40mm) arc through (0.5886mm, 39.4114mm) (0mm, 38mm) (0mm, 2mm) arc through (0.5886mm, 0.5886mm)
                    (2mm, 0mm) (58mm, 0mm) arc through (59.4114mm, 0.5886mm) (60mm, 2mm)
        }"
        ),
        "{text}"
    );
    // A block and a pin standing through the board: a rectangle and a round cutout.
    assert!(
        text.contains(
            ".cutout1: Cutout {
            .x &= 41mm
            .y &= 18mm
            .width &= 8mm
            .height &= 4mm
        }"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            ".cutout2: Cutout {
            .x &= 13.5mm
            .y &= 18.5mm
            .width &= 3mm
            .height &= 3mm
            .corner_radius &= 1.5mm
        }"
        ),
        "{text}"
    );
    // Each standoff's hole is a mounting hole at its centre, stated by its diameter, in order
    // of position (top to bottom): 2.5 mm at (54, 6) and 3.2 mm at (6, 34).
    assert!(
        text.contains(
            ".hole1: MountingHole { .diameter &= 2.5mm }
        .hole1.place.x &= 54mm
        .hole1.place.y &= 6mm
        .hole2: MountingHole { .diameter &= 3.2mm }
        .hole2.place.x &= 6.001mm
        .hole2.place.y &= 34mm"
        ),
        "{text}"
    );
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    // The report says where the board's frame is in the model's, and the pose that matches.
    let report = imported.report.join("\n");
    assert!(
        report.contains("pocket's bottom left corner is at (0, 0)")
            && report.contains(".x &= 0mm and .y &= 0mm"),
        "{report}"
    );
    assert!(report.contains("holes: 2 holes"), "{report}");
}

#[test]
fn a_posed_model_is_read_where_the_pose_puts_it() {
    // The same model turned half a turn about z and moved: its pocket's bottom left corner
    // is where the pose says, and the outline is the same shape turned.
    let imported = import(
        CASE_BOX,
        "case_box.step",
        &Request {
            rotation: [0.0, 0.0, 180.0],
            offset: [100.0, 50.0, 0.0],
            ..Request::default()
        },
    )
    .unwrap();
    let report = imported.report.join("\n");
    assert!(
        report.contains("pocket's bottom left corner is at (40, 10)")
            && report.contains(".x &= 60mm and .y &= 40mm"),
        "{report}"
    );
    // The rib is at the top now: the outline's top left corner is at y = 0 and the rib stands
    // 6 mm into the pocket from it.
    assert!(imported.members.contains("(28mm, 6mm)"), "{}", imported.members);
}

#[test]
fn a_plane_that_cuts_no_pocket_is_an_error_that_lists_what_it_cuts() {
    let at_the_standoffs = Request {
        at: 20.0,
        ..Request::default()
    };
    let e = import(CASE_BOX, "case_box.step", &at_the_standoffs).unwrap_err();
    assert!(
        e.contains("cuts no pocket") && e.contains("it cuts nothing") && e.contains("--at"),
        "{e}"
    );
    // Below the board the wall still has its pocket, and the standoffs are posts in it.
    let e = import(
        CASE_BOX,
        "case_box.step",
        &Request {
            at: -4.0,
            ..Request::default()
        },
    )
    .unwrap();
    assert!(e.members.contains(".cutout4: Cutout"), "{}", e.members);
}

#[test]
fn a_file_that_is_not_step_is_an_error() {
    let e = import(b"not a model", "bad.step", &Request::default()).unwrap_err();
    assert!(e.starts_with("bad.step: "), "{e}");
}

#[test]
fn the_imported_outline_and_cutouts_go_through_fab() {
    // No holes: the sample footprint libraries of the tests have no `MountingHole`.
    let dir = TempDir::new("case-import-fab");
    let imported = import(CASE_BOX, "case_box.step", &Request::default()).unwrap();
    let source = "import std::parts::connector::Header1x2\nimport std::mfg::jlcpcb::TwoLayer\n\n\
        board Boxed {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .a: Header1x2\n    .a.p1.net &= \"N1\"\n    .a.p2.net &= \"N2\"\n}\n";
    let nsm = dir.join("boxed.nsm");
    fs::write(&nsm, source).unwrap();
    let (board, diff) = write_physical(&nsm, None, &imported).unwrap();
    assert_eq!(board, "Boxed");
    assert!(diff.contains("+            polygon (60mm, 38mm)"), "{diff}");
    let physical = fs::read_to_string(dir.join("boxed.physical.nsm")).unwrap();
    let made = fab_with_options(
        "boxed.nsm",
        source,
        &netsmith_check::Loader::std().with_physical("boxed.nsm", "boxed.physical.nsm", &physical),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
        None,
        &FabOptions::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.len(), 1);
    let pcb = fs::read_to_string(dir.join("fab/Boxed/Boxed.kicad_pcb")).unwrap();
    // The rib's three sides are three of the outline's edges.
    assert!(pcb.contains("(gr_line"), "{pcb}");
    assert!(pcb.contains("(gr_arc"), "{pcb}");
}
