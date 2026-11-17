//! `netsmith import case model` (ADR 0109 "Left open"): the outline, cutouts and mounting holes of an
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
        text.starts_with("// Imported from case_box.step by `netsmith import case model`.\n"),
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

const CASE_LID: &[u8] = include_bytes!("../../netsmith-3d/tests/models/case_lid.step");

fn with_limits() -> Request {
    Request {
        limits: true,
        ..Request::default()
    }
}

/// The `.max_height` and the box of each `Keepout` of the text: `(height, x, y, width, height)`
/// in mm, and its layer.
fn keepouts(members: &str) -> Vec<(String, [f64; 5])> {
    let mut out = Vec::new();
    let mut cur = [0.0; 5];
    let mut layer = String::new();
    let mut limit = false;
    for line in members.lines().map(str::trim) {
        if line.starts_with(".") && line.ends_with("{") {
            limit = line.starts_with(".limit");
            continue;
        }
        if !limit {
            continue;
        }
        let value = |key: &str| {
            line.strip_prefix(key)
                .and_then(|v| v.strip_suffix("mm"))
                .map(|v| v.parse::<f64>().unwrap())
        };
        if let Some(v) = value(".x &= ") {
            cur[1] = v;
        } else if let Some(v) = value(".y &= ") {
            cur[2] = v;
        } else if let Some(v) = value(".width &= ") {
            cur[3] = v;
        } else if let Some(v) = value(".height &= ") {
            cur[4] = v;
        } else if let Some(v) = value(".max_height &= ") {
            cur[0] = v;
        } else if let Some(l) = line.strip_prefix(".layers &= ") {
            layer = l.trim_matches('"').to_string();
        } else if line == "}" && line.len() == 1 && cur[3] > 0.0 {
            out.push((layer.clone(), cur));
            cur = [0.0; 5];
        }
    }
    out
}

#[test]
fn the_undersides_over_the_board_are_keepouts_that_limit_the_height() {
    let imported = import(CASE_LID, "case_lid.step", &with_limits()).unwrap();
    let text = render(&imported, Some("Lidded"), "case_lid.step", false);
    // The pocket is 60 x 40 mm with its bottom left corner at the origin, so the board's y is
    // 40 minus the model's: the rib (x 20 to 26, the pocket's whole depth) stands at y 0, the
    // boss (a circle of 8 mm at (45, 30)) at y 6 to 14.
    assert!(
        text.contains("import std::pcb::{Cutout, Keepout, Outline}\n"),
        "{text}"
    );
    assert!(
        text.contains(
            ".limit1: Keepout {
            .x &= 20mm
            .y &= 0mm
            .width &= 6mm
            .height &= 40mm
            .layers &= \"top\"
            .max_height &= 4mm
        }"
        ),
        "{text}"
    );
    let all = keepouts(&imported.members);
    let n = all.len();
    // The rib, the boss as slabs of its circle (not its 8 x 8 box), and the lid, lowest first.
    assert!(all.iter().all(|(layer, _)| layer == "top"), "{all:?}");
    let heights: Vec<f64> = all.iter().map(|(_, k)| k[0]).collect();
    assert!(heights.windows(2).all(|w| w[0] <= w[1]), "{heights:?}");
    assert_eq!(all[0].1, [4.0, 20.0, 0.0, 6.0, 40.0]);
    assert_eq!(all[n - 1].1, [10.0, 0.0, 0.0, 60.0, 40.0]);
    let boss: Vec<&[f64; 5]> = all.iter().map(|(_, k)| k).filter(|k| k[0] == 7.0).collect();
    assert_eq!(boss.len(), n - 2);
    let area: f64 = boss.iter().map(|k| k[3] * k[4]).sum();
    assert!(
        area > std::f64::consts::PI * 16.0 && area < 64.0 * 0.9,
        "{area}"
    );
    // The block that stands from the board to the lid is a cutout, not a limit.
    assert!(text.contains(".cutout1: Cutout {"), "{text}");
    assert_eq!(text.matches("Keepout {").count(), n, "{text}");
    let report = imported.report.join("\n");
    assert!(
        report.contains(&format!("limits: {n} height limits over and under the board"))
            && report.contains("limit1: on the top face at most 4 mm over 6 x 40 mm")
            && report.contains("the underside of `lid`"),
        "{report}"
    );
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
}

const CASE_SHAPED: &[u8] = include_bytes!("../../netsmith-3d/tests/models/case_shaped.step");

#[test]
fn an_l_a_slope_a_floor_and_a_pad_are_limits_on_the_faces_they_are_over() {
    let request = Request {
        limit_step: 2.0,
        ..with_limits()
    };
    let imported = import(CASE_SHAPED, "case_shaped.step", &request).unwrap();
    let all = keepouts(&imported.members);
    assert!(imported.warnings.is_empty(), "{:?}", imported.warnings);
    let top: Vec<[f64; 5]> = all
        .iter()
        .filter(|(l, _)| l == "top")
        .map(|(_, k)| *k)
        .collect();
    let bottom: Vec<[f64; 5]> = all
        .iter()
        .filter(|(l, _)| l == "bottom")
        .map(|(_, k)| *k)
        .collect();
    // The numbers run on from the top face's to the bottom face's.
    assert_eq!(top.len() + bottom.len(), all.len());
    // The L (the model's y 5 to 35 is the board's y 40 - 35 = 5 to 35): two rectangles at 9 mm.
    assert!(top.contains(&[9.0, 5.0, 25.0, 30.0, 10.0]), "{top:?}");
    assert!(top.contains(&[9.0, 5.0, 5.0, 10.0, 20.0]), "{top:?}");
    assert!(
        !top.iter().any(|k| k[0] == 9.0 && k[3] == 30.0 && k[4] == 30.0),
        "the L is not its box: {top:?}"
    );
    // The ramp: steps of 2 mm from the lowest end, 8, 10 and 12 mm.
    let ramp: Vec<f64> = top
        .iter()
        .filter(|k| k[1] > 39.0 && k[3] < 20.0 && k[2] == 5.0 && k[4] == 30.0)
        .map(|k| k[0])
        .collect();
    assert_eq!(ramp, [8.0, 10.0, 12.0], "{top:?}");
    // The floor limits the bottom face at 6.4 mm over the whole board; the pad at 3 mm; the
    // post that reaches the board is none.
    assert!(bottom.contains(&[6.4, 0.0, 0.0, 60.0, 40.0]), "{bottom:?}");
    assert!(bottom.iter().any(|k| k[0] == 3.0), "{bottom:?}");
    assert!(
        bottom.iter().all(|k| k[0] == 3.0 || k[0] == 6.4),
        "{bottom:?}"
    );
    let report = imported.report.join("\n");
    assert!(
        report.contains("on the bottom face at most 6.4 mm over 60 x 40 mm")
            && report.contains("the top of `floor`")
            && report.contains("a step of a slope"),
        "{report}"
    );
    // A thinner board has its bottom face higher: the floor is 0.4 mm nearer.
    let thin = Request {
        thickness: 1.0,
        ..request.clone()
    };
    let imported = import(CASE_SHAPED, "case_shaped.step", &thin).unwrap();
    assert!(
        keepouts(&imported.members)
            .iter()
            .any(|(l, k)| l == "bottom" && k[0] == 7.0 && k[3] == 60.0),
        "{}",
        imported.members
    );
}

#[test]
fn a_step_or_a_thickness_that_is_not_a_length_is_refused() {
    for request in [
        Request {
            limit_step: 0.0,
            ..with_limits()
        },
        Request {
            limit_step: f64::NAN,
            ..with_limits()
        },
        Request {
            thickness: 0.0,
            ..with_limits()
        },
    ] {
        let e = import(CASE_LID, "case_lid.step", &request).unwrap_err();
        assert!(e.contains("must be"), "{e}");
    }
    // Without `--limits` neither is looked at.
    let plain = Request {
        limit_step: 0.0,
        ..Request::default()
    };
    assert!(import(CASE_LID, "case_lid.step", &plain).is_ok());
}

#[test]
fn limits_are_printed_only_when_asked_and_a_case_without_a_lid_has_none() {
    let plain = import(CASE_LID, "case_lid.step", &Request::default()).unwrap();
    assert!(!plain.members.contains("Keepout"), "{}", plain.members);
    assert_eq!(plain.imports, "import std::pcb::{Cutout, Outline}\n");
    let open = import(CASE_BOX, "case_box.step", &with_limits()).unwrap();
    assert!(!open.members.contains("Keepout"), "{}", open.members);
    assert!(
        open.report
            .iter()
            .any(|l| l.starts_with("limits: no underside of the case above")),
        "{:?}",
        open.report
    );
}

#[test]
fn the_imported_limits_go_through_fab() {
    let dir = TempDir::new("case-import-limits-fab");
    let imported = import(CASE_LID, "case_lid.step", &with_limits()).unwrap();
    let source = "import std::parts::connector::Header1x2\nimport std::mfg::jlcpcb::TwoLayer\n\n\
        board Lidded {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .a: Header1x2\n    .a.p1.net &= \"N1\"\n    .a.p2.net &= \"N2\"\n}\n";
    let nsm = dir.join("lidded.nsm");
    fs::write(&nsm, source).unwrap();
    write_physical(&nsm, None, &imported).unwrap();
    let physical = fs::read_to_string(dir.join("lidded.physical.nsm")).unwrap();
    assert!(
        physical.contains("import std::pcb::Keepout")
            && physical.contains(".max_height &= 4mm"),
        "{physical}"
    );
    // Importing again replaces the members of the first import and leaves one of each.
    write_physical(&nsm, None, &imported).unwrap();
    let again = fs::read_to_string(dir.join("lidded.physical.nsm")).unwrap();
    assert_eq!(
        again.matches("Keepout {").count(),
        imported.members.matches("Keepout {").count(),
        "{again}"
    );
    assert_eq!(again.matches("Outline {").count(), 1, "{again}");
    let made = fab_with_options(
        "lidded.nsm",
        source,
        &netsmith_check::Loader::std().with_physical("lidded.nsm", "lidded.physical.nsm", &physical),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
        None,
        &FabOptions::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made.len(), 1);
}

#[test]
fn an_import_of_fewer_members_leaves_none_of_the_earlier_import() {
    let dir = TempDir::new("case-import-surplus");
    let source = "board Lidded {\n}\n";
    let nsm = dir.join("lidded.nsm");
    fs::write(&nsm, source).unwrap();
    let many = import(CASE_SHAPED, "case_shaped.step", &with_limits()).unwrap();
    let before = many.members.matches("Keepout {").count();
    assert!(before > 12, "{before}");
    write_physical(&nsm, None, &many).unwrap();
    // A hand-written member of another name and a comment stay.
    let path = dir.join("lidded.physical.nsm");
    let text = fs::read_to_string(&path).unwrap();
    let text = text.replacen("    case {\n", "    case {\n        // mine\n        .keep: Keepout {\n            .x &= 1mm\n            .y &= 1mm\n            .width &= 1mm\n            .height &= 1mm\n        }\n", 1);
    fs::write(&path, text).unwrap();
    let few = import(CASE_LID, "case_lid.step", &with_limits()).unwrap();
    let after = few.members.matches("Keepout {").count();
    assert!(after < before);
    write_physical(&nsm, None, &few).unwrap();
    let physical = fs::read_to_string(&path).unwrap();
    assert_eq!(physical.matches("Keepout {").count(), after + 1, "{physical}");
    assert!(physical.contains("// mine") && physical.contains(".keep: Keepout"), "{physical}");
    assert!(!physical.contains(&format!(".limit{}:", after + 1)), "{physical}");
    // Without --limits the limits already in the file are not touched.
    let plain = import(CASE_LID, "case_lid.step", &Request::default()).unwrap();
    write_physical(&nsm, None, &plain).unwrap();
    let kept = fs::read_to_string(&path).unwrap();
    assert_eq!(kept.matches("Keepout {").count(), after + 1, "{kept}");
    // With --limits and nothing to find, they all go.
    let none = import(CASE_BOX, "case_box.step", &with_limits()).unwrap();
    write_physical(&nsm, None, &none).unwrap();
    let gone = fs::read_to_string(&path).unwrap();
    assert_eq!(gone.matches("Keepout {").count(), 1, "{gone}");
    assert!(gone.contains(".keep: Keepout"), "{gone}");
    assert!(!gone.contains(".limit"), "{gone}");
}
