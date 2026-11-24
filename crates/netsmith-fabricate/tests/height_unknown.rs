//! The parts the height check cannot check (issue #1117): one warning, `height_unknown`, lists
//! every part of unknown height under a height limit or under another part's body, with the
//! limit that would apply, in `board fab` and `dev board3d`, and `fab` still passes; and an
//! overlap with a module on pin strips names the `.standoff` the design has not stated.
//! Every board here is invented: a 40 by 30 mm board with a low lid over its top 18 mm.

use std::fs;

use netsmith_check::Loader;
use netsmith_fabricate::fab::{FabOptions, fab_with_options};
use netsmith_pcb::{Board, Pt, place_options};
use netsmith_pipeline::{Built, Libraries, build_with};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

/// The shipped catalogs and an invented resistor row of 31.7 kohm (a value the shipped catalog
/// has no row of) whose `height` cell is empty: a part picked as it has no height, which is what
/// these tests are about. Every chip row of the shipped catalogs has a height (issue #1421).
fn catalogs() -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    netsmith_solve::migrate::add_to(&mut c, "resistor", "catalogs/resistor.csv", NO_HEIGHT_ROW)
        .unwrap();
    c
}

/// The invented row (an invented part number too), with no `height`.
const NO_HEIGHT_ROW: &str = "\
lcsc,mpn,resistance,max_power,package,height,library,source
C900317,ACME-R317,31.7kohm +/- 1%,62.5mW,0402,,Basic,https://example.invalid/acme.pdf
";

/// Three resistors and a 1x4 pin header standing for a module on pin strips.
const SOURCE: &str = "\
import std::parts::connector::Header1x4
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .rail: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .module: Header1x4
    .rail.pos ~ .module.p1
    .rail.neg ~ .module.p2
    .r1: Resistor { .resistance &= 31.7kohm +/- 1% }
    .r2: Resistor { .resistance &= 31.7kohm +/- 1% }
    .r3: Resistor { .resistance &= 31.7kohm +/- 1% }
    .rail.pos ~> .r1 ~> .rail.neg
    .rail.pos ~> .r2 ~> .rail.neg
    .rail.pos ~> .r3 ~> .rail.neg
}
";

/// The outline, and `extra`.
fn physical(extra: &str) -> String {
    format!(
        "import std::pcb::{{Outline, Keepout}}

physical for Widget {{
    case {{
        .outline: Outline {{
            .width &= 40mm
            .height &= 30mm
        }}
    }}
{extra}}}
"
    )
}

/// A lid over the top 18 mm that allows 3 mm on the top face.
const LID: &str = "    .lid: Keepout {
        .x &= 0mm
        .y &= 0mm
        .width &= 40mm
        .height &= 18mm
        .layers &= \"top\"
        .max_height &= 3mm
    }
";

fn built(extra: &str) -> Built {
    let loader = Loader::std().with_physical("widget.nsm", "widget.physical.nsm", &physical(extra));
    build_with(
        "widget.nsm",
        SOURCE,
        &loader,
        &catalogs(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0)
}

/// The board the placer makes of `built`, with every part moved to `(x, y)` from the
/// outline's top left corner when `stack` says so (a module over the parts under its body).
fn placed(built: &Built, stack: Option<(f64, f64)>) -> Board {
    let libraries = Libraries::sample();
    let mut board = netsmith_placer::board_from_design_placed(
        &built.design,
        &libraries.mapping,
        &libraries.footprints,
        &place_options(&built.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    if let Some((x, y)) = stack {
        let top = board.outline.min;
        for p in &mut board.parts {
            p.at = Pt::new(top.x + x, top.y + y);
        }
    }
    board
}

#[test]
fn a_part_of_unknown_height_under_a_limit_is_one_height_unknown_warning_with_the_limit() {
    // Only the module states a height; the resistors stand where the placer puts them.
    let b = built(&format!("    .module.height &= 2mm\n{LID}"));
    let mut board = placed(&b, None);
    // R2 is put under the lid; the other two are placed where they are.
    let top = board.outline.min;
    board.parts.iter_mut().find(|p| p.refdes == "R2").unwrap().at =
        Pt::new(top.x + 20.0, top.y + 6.0);
    let unknown = board.heights_unknown();
    let r2 = unknown.iter().find(|u| u.refdes == "R2").expect("R2");
    assert_eq!(
        r2.under,
        [netsmith_pcb::UnknownUnder::Limit {
            keepout: "lid".into(),
            limit: 3.0
        }]
    );
    // The module states its height, so it is not unknown.
    assert!(unknown.iter().all(|u| u.refdes != "J1"), "{unknown:?}");
    let warning = netsmith_pcb::heights_unknown_warning(&unknown).expect("a warning");
    assert!(
        warning.starts_with("height_unknown: ")
            && warning.contains("`r2` (R2) on the top face under a 3.00 mm limit (the keepout `lid`)"),
        "{warning}"
    );
    assert_eq!(warning.matches("height_unknown").count(), 1, "{warning}");

    // `dev board3d` reports the same warning once, in the report and in the text.
    let scene = netsmith_fabricate::board3d::scene(&b, &board);
    let report = netsmith_fabricate::board3d::report(&b, &scene, &b.sourcing).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["height_unknown"]["message"], warning.as_str());
    let part = json["height_unknown"]["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["refdes"] == "R2")
        .expect("R2 is listed");
    assert_eq!(part["face"], "top");
    assert_eq!(
        part["under"],
        serde_json::json!([{ "kind": "limit", "name": "lid", "limit": 3.0 }])
    );
    let text = report.summary();
    assert_eq!(text.matches("warning: height_unknown").count(), 1, "{text}");
    // The `.glb` carries it as a finding of its own.
    let glb = netsmith_fabricate::board3d::glb(&b, &board);
    let doc = netsmith_3d::json_chunk(&glb).unwrap();
    let findings = doc["nodes"][0]["extras"]["netsmith"]["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "height_unknown")
        .count();
    assert_eq!(findings, 1);
}

#[test]
fn a_board_with_no_part_of_unknown_height_under_anything_has_no_height_unknown() {
    let b = built("    .module.height &= 2mm\n    .r1.height &= 1mm\n    .r2.height &= 1mm\n    .r3.height &= 1mm\n");
    let board = placed(&b, None);
    assert_eq!(board.heights_unknown(), vec![]);
    let scene = netsmith_fabricate::board3d::scene(&b, &board);
    let report = netsmith_fabricate::board3d::report(&b, &scene, &b.sourcing).unwrap();
    assert!(serde_json::to_value(&report).unwrap().get("height_unknown").is_none());
}

#[test]
fn fab_says_height_unknown_once_in_its_notes_and_still_passes() {
    let dir = TempDir::unused("height-unknown-fab");
    let loader = Loader::std().with_physical(
        "widget.nsm",
        "widget.physical.nsm",
        &physical(&format!(
            "    .module.height &= 2mm\n{}",
            LID.replace("18mm", "30mm")
        )),
    );
    let made = fab_with_options(
        "widget.nsm",
        SOURCE,
        &loader,
        &dir.join("fab"),
        &catalogs(),
        &Libraries::sample(),
        None,
        &FabOptions::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let notes = &made[0].notes;
    let lines: Vec<&str> = notes
        .lines()
        .filter(|l| l.starts_with("warning: height_unknown: "))
        .collect();
    // The lid covers the whole board, so the three resistors that state nothing are under it:
    // one warning names all three, with the lid's limit.
    assert_eq!(lines.len(), 1, "{notes}");
    for want in [
        "`r1` (R1) on the top face under a 3.00 mm limit (the keepout `lid`)",
        "`r2` (R2)",
        "`r3` (R3)",
    ] {
        assert!(lines[0].contains(want), "{want}: {notes}");
    }
    assert!(notes.contains("design rules check: passed"), "{notes}");
    assert!(fs::metadata(dir.join("fab")).is_ok());
}

#[test]
fn a_part_of_unknown_height_under_a_module_that_states_no_standoff_names_the_missing_limit() {
    // The module is 8 mm tall and states no standoff; the resistors are under its body.
    let b = built("    .module.height &= 8mm\n");
    let board = placed(&b, Some((20.0, 15.0)));
    let unknown = board.heights_unknown();
    let r1 = unknown.iter().find(|u| u.refdes == "R1").expect("R1");
    assert_eq!(
        r1.under,
        [netsmith_pcb::UnknownUnder::Body {
            refdes: "J1".into(),
            path: "module".into(),
            height: 8.0
        }]
    );
    let warning = netsmith_pcb::heights_unknown_warning(&unknown).unwrap();
    assert!(
        warning.contains("under the body of `module` (J1) (8.00 mm tall), which states no `.standoff`"),
        "{warning}"
    );
    // Stating the standoff turns it into the limit of the standoff.
    let b = built("    .module.height &= 8mm\n    .module.standoff &= 2mm\n");
    let board = placed(&b, Some((20.0, 15.0)));
    let unknown = board.heights_unknown();
    let r1 = unknown.iter().find(|u| u.refdes == "R1").expect("R1");
    assert_eq!(
        r1.under,
        [netsmith_pcb::UnknownUnder::Limit {
            keepout: "standoff of J1".into(),
            limit: 2.0
        }]
    );
}

#[test]
fn an_overlap_with_a_module_on_pin_strips_names_the_standoff_it_lacks() {
    // The resistors state 1 mm, so the scene can measure them: the module's body fills the
    // space down to the board, and they overlap it.
    let extra = "    .module.height &= 8mm\n    .r1.height &= 1mm\n    .r2.height &= 1mm\n    .r3.height &= 1mm\n";
    let b = built(extra);
    let board = placed(&b, Some((20.0, 15.0)));
    let scene = netsmith_fabricate::board3d::scene(&b, &board);
    let overlap = scene
        .findings
        .overlaps
        .iter()
        .find(|o| o.a_refdes == "J1" || o.b_refdes == "J1")
        .expect("the module overlaps a resistor");
    let missing = overlap.missing_standoff.as_ref().expect("the missing standoff");
    assert_eq!((missing.path.as_str(), missing.refdes.as_str()), ("module", "J1"));
    let statement = missing.statement();
    assert!(
        statement.contains("`.module.standoff &= 2mm`") && statement.contains("`.module.pin_protrusion`"),
        "{statement}"
    );
    let report = netsmith_fabricate::board3d::report(&b, &scene, &b.sourcing).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    let found = json["overlaps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["missing_standoff"]["path"] == "module")
        .expect("the report names the module");
    assert_eq!(found["missing_standoff"]["statement"], statement.as_str());
    assert!(report.summary().contains("overlap: ") && report.summary().contains(".module.standoff"));
    // Two resistors that overlap each other on the board name no standoff.
    assert!(
        scene
            .findings
            .overlaps
            .iter()
            .filter(|o| o.a_refdes != "J1" && o.b_refdes != "J1")
            .all(|o| o.missing_standoff.is_none())
    );
    // With the standoff stated, the overlap with the low parts is gone.
    let b = built(&format!("{extra}    .module.standoff &= 2mm\n"));
    let board = placed(&b, Some((20.0, 15.0)));
    let scene = netsmith_fabricate::board3d::scene(&b, &board);
    assert!(
        scene
            .findings
            .overlaps
            .iter()
            .all(|o| o.a_refdes != "J1" && o.b_refdes != "J1"),
        "{:?}",
        scene.findings.overlaps
    );
}

/// A lid of `limit` mm over a 10 mm wide strip from `x`, on the top face.
fn strip(name: &str, x: f64, limit: f64) -> String {
    format!(
        "    .{name}: Keepout {{
        .x &= {x}mm
        .y &= 0mm
        .width &= 10mm
        .height &= 30mm
        .layers &= \"top\"
        .max_height &= {limit}mm
    }}
"
    )
}

#[test]
fn the_parts_of_unknown_height_are_ranked_by_the_room_under_their_limit() {
    // R1 under a 5 mm lid, R2 under a 1.2 mm lid, R3 under no limit and no body.
    let extra = format!(
        "    .module.height &= 2mm\n{}{}",
        strip("high", 0.0, 5.0),
        strip("low", 15.0, 1.2)
    );
    let b = built(&extra);
    let mut board = placed(&b, None);
    let top = board.outline.min;
    for (path, x) in [("r1", 5.0), ("r2", 20.0), ("r3", 35.0)] {
        board.parts.iter_mut().find(|p| p.path == path).unwrap().at = Pt::new(top.x + x, top.y + 6.0);
    }
    board.parts.iter_mut().find(|p| p.path == "module").unwrap().at = Pt::new(top.x + 35.0, top.y + 25.0);
    let unknown = board.heights_unknown();
    // R3 is under nothing, so it is not listed; the 1.2 mm one comes first, though R1 sorts first by name.
    let order: Vec<&str> = unknown.iter().map(|u| u.refdes.as_str()).collect();
    assert_eq!(order, ["R2", "R1"], "{unknown:?}");
    let warning = netsmith_pcb::heights_unknown_warning(&unknown).unwrap();
    let low = warning.find("`r2` (R2) on the top face under a 1.20 mm limit (the keepout `low`)").expect(&warning);
    let high = warning.find("`r1` (R1) on the top face under a 5.00 mm limit (the keepout `high`)").expect(&warning);
    assert!(low < high, "{warning}");
    assert!(warning.contains("the tightest first"), "{warning}");
    // The example names the part to state first: the tightest.
    assert!(warning.contains("`.r2.height &= 8mm`"), "{warning}");
    // The report lists them in the same order, each with its tightest limit.
    let scene = netsmith_fabricate::board3d::scene(&b, &board);
    let report = netsmith_fabricate::board3d::report(&b, &scene, &b.sourcing).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    let parts = json["height_unknown"]["parts"].as_array().unwrap();
    assert_eq!(parts[0]["refdes"], "R2");
    assert_eq!(parts[0]["tightest_limit"], 1.2);
    assert_eq!(parts[1]["tightest_limit"], 5.0);
}

#[test]
fn the_parts_under_no_limit_come_after_the_limits_by_package_class() {
    use netsmith_core::PartKind;
    use netsmith_pcb::{HeightUnknown, UnknownUnder, heights_unknown_warning};
    let body = |path: &str, kind| HeightUnknown {
        refdes: path.to_uppercase(),
        path: path.into(),
        side: netsmith_pcb::Side::Front,
        under: vec![UnknownUnder::Body {
            refdes: "J1".into(),
            path: "module".into(),
            height: 8.0,
        }],
        kind: Some(kind),
        body_area: 1.0,
    };
    let limited = |path: &str, limit| HeightUnknown {
        under: vec![UnknownUnder::Limit { keepout: "lid".into(), limit }],
        ..body(path, PartKind::Resistor)
    };
    let parts = [
        body("r1", PartKind::Resistor),
        body("j2", PartKind::Connector),
        limited("c1", 7.0),
        body("l1", PartKind::Inductor),
    ];
    let warning = heights_unknown_warning(&parts).unwrap();
    let at = |what: &str| warning.find(what).unwrap_or_else(|| panic!("{what}: {warning}"));
    assert!(at("`c1`") < at("`j2`") && at("`j2`") < at("`l1`") && at("`l1`") < at("`r1`"), "{warning}");
}

#[test]
fn the_parts_past_the_listed_ones_are_one_count_line() {
    use netsmith_core::PartKind;
    use netsmith_pcb::{HeightUnknown, UnknownUnder, heights_unknown_warning};
    let part = |i: usize, limit: f64| HeightUnknown {
        refdes: format!("R{i}"),
        path: format!("r{i}"),
        side: netsmith_pcb::Side::Front,
        under: vec![UnknownUnder::Limit { keepout: "lid".into(), limit }],
        kind: Some(PartKind::Resistor),
        body_area: 1.0,
    };
    // Eight parts, the limits 1.0, 1.5, ... 4.5 mm, given in the reverse order.
    let parts: Vec<HeightUnknown> = (1..=8).rev().map(|i| part(i, 0.5 + 0.5 * i as f64)).collect();
    let warning = heights_unknown_warning(&parts).unwrap();
    assert!(warning.contains("whether 8 parts"), "{warning}");
    for listed in 1..=5 {
        assert!(warning.contains(&format!("`r{listed}` (R{listed})")), "{warning}");
    }
    for rest in 6..=8 {
        assert!(!warning.contains(&format!("`r{rest}` (R{rest})")), "{warning}");
    }
    assert!(
        warning.contains("and 3 more, 3 under a limit of 3.50 mm or more"),
        "{warning}"
    );
}
