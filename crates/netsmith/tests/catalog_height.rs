//! A part's height from the catalog row it was picked as (ADR 0094): the row's
//! `height` cell is the part's `.height` when the design states none; a stated `.height`
//! wins; a row with no cell leaves the part with no height, which is reported unchecked and
//! never taken as flat; and a board with a height limit says how many of its parts have a
//! known height and which came from a row. Every board, row and number here is invented.

use netsmith::heights::{CatalogHeight, Coverage, notes};
use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_pcb::{Board, place_options};
use netsmith_solve::Catalogs;

/// Three resistors, each of a value only the project's file has.
const SOURCE: &str = "\
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
    .r1: Resistor { .resistance &= 31.6kohm +/- 1% }
    .r2: Resistor { .resistance &= 31.7kohm +/- 1% }
    .r3: Resistor { .resistance &= 31.8kohm +/- 1% }
    .rail.pos ~> .r1 ~> .rail.neg
    .rail.pos ~> .r2 ~> .rail.neg
    .rail.pos ~> .r3 ~> .rail.neg
}
";

/// The project's file: `r1`'s row is 0.55 mm tall, `r2`'s states no height and `r3`'s is 2.0 mm.
const ROWS: &str = "\
mpn,resistance,max_power,package,height,library,source
ACME-R316,31.6kohm +/- 1%,62.5mW,0402,0.55mm,Basic,https://example.invalid/acme.pdf height: page 2
ACME-R317,31.7kohm +/- 1%,62.5mW,0402,,Basic,https://example.invalid/acme.pdf
ACME-R318,31.8kohm +/- 1%,62.5mW,0402,2.0mm,Basic,https://example.invalid/acme.pdf height: page 2
";

/// The outline, a lid of 3 mm over the top 18 mm, and `extra`.
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
    .lid: Keepout {{
        .x &= 0mm
        .y &= 0mm
        .width &= 40mm
        .height &= 18mm
        .layers &= \"top\"
        .max_height &= 3mm
    }}
{extra}}}
"
    )
}

fn catalogs() -> Catalogs {
    let mut c = Catalogs::jlcpcb();
    c.add_file("resistor", "catalogs/resistor.csv", ROWS)
        .unwrap();
    c
}

fn built(extra: &str) -> netsmith::Built {
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

fn heights(b: &netsmith::Built) -> Vec<(&str, f64)> {
    b.design
        .features
        .heights
        .iter()
        .map(|h| (h.path.as_str(), h.height))
        .collect()
}

#[test]
fn a_part_has_the_height_of_its_row_and_one_with_no_cell_has_none() {
    let b = built("");
    // The rows are the project's: the pick is the row named by the value.
    let mpns: Vec<_> = b.sourcing.iter().map(|s| s.mpn.as_deref()).collect();
    assert_eq!(
        mpns,
        [Some("ACME-R316"), Some("ACME-R317"), Some("ACME-R318")]
    );
    // r2's row states none: it is not here, which is not a height of zero.
    assert_eq!(heights(&b), [("r1", 0.55), ("r3", 2.0)]);
    assert_eq!(
        b.catalog_heights,
        [
            CatalogHeight {
                refdes: "R1".into(),
                path: "r1".into(),
                height: 0.55,
                lcsc: None,
                mpn: Some("ACME-R316".into()),
            },
            CatalogHeight {
                refdes: "R3".into(),
                path: "r3".into(),
                height: 2.0,
                lcsc: None,
                mpn: Some("ACME-R318".into()),
            }
        ]
    );
    assert_eq!(b.catalog_heights[0].row(), "ACME-R316");
}

/// The board of `b`, placed.
fn placed(b: &netsmith::Built) -> Board {
    let libraries = Libraries::sample();
    netsmith_placer::board_from_design_placed(
        &b.design,
        &libraries.mapping,
        &libraries.footprints,
        &place_options(&b.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_placed_board_knows_the_height_of_a_part_from_a_row_and_not_of_the_other() {
    let b = built("");
    let board = placed(&b);
    assert_eq!(board.part("R1").unwrap().height, Some(0.55));
    assert_eq!(board.part("R2").unwrap().height, None);
    assert_eq!(board.part("R3").unwrap().height, Some(2.0));
}

#[test]
fn a_height_the_design_states_wins_over_the_row_and_is_not_a_catalog_height() {
    // The physical file says `r3` is 4 mm (a taller variant is fitted, say): the row's 2.0 mm
    // is not compared with it, and `r3` is not listed as a catalog height. `r2` gets one too.
    let b = built("    .r3.height &= 4mm\n    .r2.height &= 1.1mm\n");
    assert_eq!(heights(&b), [("r1", 0.55), ("r2", 1.1), ("r3", 4.0)]);
    let from_rows: Vec<&str> = b.catalog_heights.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(from_rows, ["r1"]);
}

#[test]
fn a_board_with_a_limit_says_how_many_parts_have_a_height_and_from_where() {
    let b = built("    .r3.height &= 4mm\n");
    let coverage =
        Coverage::of(&b.design, &b.catalog_heights, &placed(&b)).expect("the lid is a limit");
    assert_eq!(
        coverage,
        Coverage {
            parts: 3,
            stated: 1,
            catalog: 1,
            standard: 0,
        }
    );
    assert_eq!(
        coverage.line(),
        "note: 2 of 3 parts have a known height (1 stated in the design, 1 from catalog rows, 0 the package standard's maximum); a part under a height limit with none is reported unchecked, never as a pass"
    );
}

#[test]
fn a_board_with_no_height_limit_reports_nothing_about_heights() {
    let loader = Loader::std().with_physical(
        "widget.nsm",
        "widget.physical.nsm",
        "import std::pcb::Outline

physical for Widget {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 30mm
        }
    }
}
",
    );
    let b = build_with(
        "widget.nsm",
        SOURCE,
        &loader,
        &catalogs(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0);
    // The heights are known all the same; nothing limits them, so nothing is said.
    assert_eq!(b.catalog_heights.len(), 2);
    assert_eq!(
        Coverage::of(&b.design, &b.catalog_heights, &placed(&b)),
        None
    );
}

#[test]
fn the_notes_of_fab_name_each_row_a_height_was_read_from() {
    let b = built("");
    let lines = notes(&b.design, &b.catalog_heights, &placed(&b));
    assert_eq!(lines.len(), 3, "{lines:#?}");
    assert!(lines[0].starts_with("note: 2 of 3 parts have a known height (0 stated"));
    assert_eq!(
        lines[1],
        "note: height of R1 (`r1`) is 0.55 mm, from the `height` of catalog row ACME-R316 (the row's `source` cell says which document and page); the design states none"
    );
    assert!(lines[2].contains("R3 (`r3`) is 2.00 mm"), "{}", lines[2]);
}

/// `r2`, which has no height of its own, as if its footprint were a SOIC-8 (JEDEC MS-012, at most
/// 1.75 mm, ADR 0106); `r1` (catalog) and `r3` (stated) on the same footprint keep their own.
fn on_soic(b: &netsmith::Built) -> Board {
    let mut board = placed(b);
    for p in &mut board.parts {
        p.library_id = "Package_SO:SOIC-8_3.9x4.9mm_P1.27mm".into();
    }
    board
}

#[test]
fn a_standard_height_is_counted_as_known_and_apart_and_named_in_the_notes() {
    let b = built("    .r3.height &= 4mm\n");
    let board = on_soic(&b);
    assert_eq!(board.part("R1").unwrap().standard_height(), None);
    assert_eq!(board.part("R3").unwrap().standard_height(), None);
    assert_eq!(
        board.part("R2").unwrap().standard_height().map(|s| s.max),
        Some(1.75)
    );
    let coverage = Coverage::of(&b.design, &b.catalog_heights, &board).unwrap();
    assert_eq!(
        coverage,
        Coverage {
            parts: 3,
            stated: 1,
            catalog: 1,
            standard: 1,
        }
    );
    assert!(
        coverage
            .line()
            .starts_with("note: 3 of 3 parts have a known height (1 stated in the design, 1 from catalog rows, 1 the package standard's maximum)"),
        "{}",
        coverage.line()
    );
    let lines = notes(&b.design, &b.catalog_heights, &board);
    assert!(
        lines.contains(&"note: height of R2 (`r2`) is 1.75 mm, the greatest its package standard allows (JEDEC MS-012, drawing TI D0008A), not a figure of the part: the design states none and its catalog row has none".to_string()),
        "{lines:#?}"
    );
    // Under the 3 mm lid, 1.75 mm fits: a pass said to be the standard's.
    let mut board = board;
    let origin = board.outline.min;
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R2")
        .unwrap()
        .at = origin + netsmith_pcb::Pt::new(20.0, 5.0);
    let passes = board.standard_passes();
    assert_eq!(passes.len(), 1, "{passes:?}");
    assert!(passes[0]
        .to_string()
        .starts_with("`r2` (R2) fits the 3.00 mm the keepout `lid` allows on the top face by its package standard (JEDEC MS-012"));
    assert!(board.heights_unchecked().is_empty());
    let lines = notes(&b.design, &b.catalog_heights, &board);
    assert!(
        lines.contains(&format!("note: {}", passes[0])),
        "{lines:#?}"
    );
}

#[test]
fn the_board3d_report_and_the_glb_label_a_standard_height_and_count_it_apart() {
    let b = built("    .r3.height &= 4mm\n");
    let mut board = on_soic(&b);
    let origin = board.outline.min;
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R2")
        .unwrap()
        .at = origin + netsmith_pcb::Pt::new(20.0, 5.0);
    let scene = netsmith::board3d::scene(&b, &board);
    let report = netsmith::board3d::report(&b, &scene, &b.sourcing).unwrap();
    let json = serde_json::to_value(&report).unwrap();
    let r2 = json["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["path"] == "r2")
        .unwrap();
    assert_eq!(r2["provenance"], "standard");
    assert_eq!(r2["height"], 1.75);
    assert_eq!(r2["standard"], "JEDEC MS-012, drawing TI D0008A");
    let r1 = json["parts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["path"] == "r1")
        .unwrap();
    assert_eq!(r1["provenance"], "catalog");
    assert!(r1.get("standard").is_none(), "{r1}");
    assert_eq!(
        json["coverage"],
        serde_json::json!({ "known": 3, "total": 3, "stated": 1, "catalog": 1, "standard": 1 })
    );
    assert_eq!(json["standard_passes"].as_array().unwrap().len(), 1);
    assert_eq!(json["standard_passes"][0]["path"], "r2");
    assert_eq!(json["standard_passes"][0]["limit_name"], "lid");
    assert_eq!(json["standard_passes"][0]["height"], 1.75);
    assert!(json["unknown_heights"].as_array().unwrap().is_empty());

    let glb = netsmith::board3d::glb(&b, &board);
    let doc = netsmith_3d::gltf::json_chunk(&glb).unwrap();
    let node = |name: &str| {
        doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["name"] == name)
            .unwrap_or_else(|| panic!("no node `{name}`"))
            .clone()
    };
    let r2 = node("R2");
    assert_eq!(r2["extras"]["netsmith"]["provenance"], "standard");
    assert_eq!(
        r2["extras"]["netsmith"]["standard"],
        "JEDEC MS-012, drawing TI D0008A"
    );
    assert_eq!(r2["extras"]["netsmith"]["height_mm"], 1.75);
    assert_eq!(
        node("R1")["extras"]["netsmith"]["standard"],
        serde_json::Value::Null
    );
    let root = doc["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["extras"]["netsmith"]["kind"] == "board")
        .unwrap();
    assert_eq!(
        root["extras"]["netsmith"]["coverage"],
        serde_json::json!({ "known": 3, "total": 3, "standard": 1 })
    );
}
