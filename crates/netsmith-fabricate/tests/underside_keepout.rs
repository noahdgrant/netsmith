//! A part's underside keepout (issue #1335): a part states `.underside_keepout`, the area under
//! its body in its own frame (a rectangle or a `polygon`) and the tallest a part may stand
//! there, and the height check measures every other part on its face against it, naming both
//! parts. It turns over with the part, limits nothing on the other face and never the part
//! itself. Every board and number here is invented: a 40 by 30 mm board, a "module" (a resistor
//! that stands for one) whose body covers 20 by 10 mm round its footprint, and two low parts.

use netsmith_check::Loader;
use netsmith_pcb::{Board, PlacementProblem, Pt, Side};
use netsmith_pipeline::{Built, Libraries, build_with};
use netsmith_solve::Catalogs;

/// The board, with `module` in the module's block and `extra` in the board's.
fn source(module: &str, extra: &str) -> String {
    format!(
        "import std::parts::passive::Resistor
import std::pcb::UndersideKeepout
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Carrier {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .rail: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }}
    .module: Resistor {{
        .resistance &= 15kohm +/- 1%
{module}    }}
    .low: Resistor {{ .resistance &= 15kohm +/- 1% }}
    .tall: Resistor {{ .resistance &= 15kohm +/- 1% }}
    .rail.pos ~> .module ~> .rail.neg
    .rail.pos ~> .low ~> .rail.neg
    .rail.pos ~> .tall ~> .rail.neg
    .module.height &= 6mm
    .low.height &= 0.4mm
    .tall.height &= 1.2mm
{extra}}}
"
    )
}

const PHYSICAL: &str = "import std::pcb::Outline

physical for Carrier {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 30mm
        }
    }
}
";

/// A rectangle 20 by 10 mm centred on the module's footprint, 0.5 mm under it.
const RECT: &str = "        .underside_keepout: UndersideKeepout {
            .x &= -10mm
            .y &= -5mm
            .width &= 20mm
            .height &= 10mm
            .max_height &= 0.5mm
        }
";

fn build(module: &str, extra: &str) -> Result<Built, String> {
    let loader = Loader::std().with_physical("carrier.nsm", "carrier.physical.nsm", PHYSICAL);
    build_with(
        "carrier.nsm",
        &source(module, extra),
        &loader,
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .map(|mut b| b.remove(0))
}

fn built(module: &str) -> Built {
    build(module, "").unwrap_or_else(|e| panic!("{e}"))
}

fn placed(b: &Built) -> Board {
    let libraries = Libraries::sample();
    netsmith_placer::board_from_design_placed(
        &b.design,
        &libraries.mapping,
        &libraries.footprints,
        &netsmith_pcb::place_options(&b.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// The board of `b` with the module at (20, 15) and the two low parts at `low` and `tall` from
/// the module's centre, as a person who moved them by hand would.
fn arranged(b: &Built, low: (f64, f64), tall: (f64, f64)) -> Board {
    let mut board = placed(b);
    let o = board.outline.min;
    let path = |board: &mut Board, path: &str, at: (f64, f64)| {
        let p = board.parts.iter_mut().find(|p| p.path == path).unwrap();
        p.at = Pt::new(o.x + 20.0 + at.0, o.y + 15.0 + at.1);
        p.rotation = 0.0;
        p.side = Side::Front;
    };
    path(&mut board, "module", (0.0, 0.0));
    path(&mut board, "low", low);
    path(&mut board, "tall", tall);
    board
}

fn too_tall(board: &Board) -> Vec<(String, String, String)> {
    board
        .check_placement()
        .into_iter()
        .filter_map(|p| match &p {
            PlacementProblem::TooTall { path, keepout, .. } => {
                Some((path.clone(), keepout.clone(), p.to_string()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_part_states_its_underside_keepout_and_the_board_carries_it_on_the_parts_face() {
    let b = built(RECT);
    let k = b
        .design
        .features
        .keepouts
        .iter()
        .find(|k| k.name == "module.underside_keepout")
        .expect("the underside keepout");
    assert_eq!(k.part.as_deref(), Some("module"));
    assert_eq!((k.at, k.size), (Some((-10.0, -5.0)), (20.0, 10.0)));
    assert_eq!(k.max_height, Some(0.5));
    assert!(k.on_carrier_face && k.polygon.is_none());
    assert!(!(k.tracks || k.vias || k.copper || k.parts), "it forbids nothing else");
}

#[test]
fn a_part_too_tall_under_the_body_is_named_with_the_part_whose_keepout_it_is() {
    // `low` (0.4 mm) and `tall` (1.2 mm) both stand under the body, 6 mm either side of it.
    let board = arranged(&built(RECT), (-6.0, 0.0), (6.0, 0.0));
    let found = too_tall(&board);
    assert_eq!(found.len(), 1, "{found:?}");
    let (path, keepout, text) = &found[0];
    assert_eq!((path.as_str(), keepout.as_str()), ("tall", "module.underside_keepout"));
    assert!(
        text.contains("`tall`")
            && text.contains("stands 1.20 mm tall on the top face")
            && text.contains("the underside keepout of `module` allows 0.50 mm there"),
        "{text}"
    );
}

#[test]
fn a_part_beside_the_body_or_on_the_other_face_is_not_under_it() {
    let b = built(RECT);
    // 14 mm off the centre is past the 10 mm half-width.
    assert!(too_tall(&arranged(&b, (-6.0, 0.0), (14.0, 0.0))).is_empty());
    // Under the body but on the bottom face.
    let mut board = arranged(&b, (-6.0, 0.0), (6.0, 0.0));
    board.parts.iter_mut().find(|p| p.path == "tall").unwrap().side = Side::Back;
    assert!(too_tall(&board).is_empty());
}

#[test]
fn the_keepout_turns_over_with_a_part_on_the_bottom_face() {
    let mut board = arranged(&built(RECT), (-6.0, 0.0), (6.0, 0.0));
    for p in board.parts.iter_mut() {
        p.side = Side::Back;
    }
    let found = too_tall(&board);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].2.contains("on the bottom face"), "{}", found[0].2);
}

#[test]
fn a_polygon_keepout_limits_only_the_parts_inside_it() {
    // A triangle: its long edge runs from (10, -5) to (-10, 5), so (-6, -3) is inside and
    // (6, 3) is past it, though both are in its box.
    let polygon = "        .underside_keepout: UndersideKeepout {
            polygon (-10mm, -5mm) (10mm, -5mm) (-10mm, 5mm)
            .max_height &= 0.5mm
        }
";
    let b = built(polygon);
    let k = b
        .design
        .features
        .keepouts
        .iter()
        .find(|k| k.name == "module.underside_keepout")
        .expect("the underside keepout");
    assert_eq!((k.at, k.size), (Some((-10.0, -5.0)), (20.0, 10.0)));
    assert_eq!(k.polygon.as_ref().map(Vec::len), Some(3));
    assert!(too_tall(&arranged(&b, (0.0, 12.0), (6.0, 3.0))).is_empty());
    let found = too_tall(&arranged(&b, (0.0, 12.0), (-6.0, -3.0)));
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn the_placer_keeps_a_part_too_tall_for_it_out_from_under_the_body() {
    let board = placed(&built(RECT));
    assert!(too_tall(&board).is_empty(), "{:?}", too_tall(&board));
}

#[test]
fn an_underside_keepout_that_is_not_a_parts_or_says_no_limit_is_refused() {
    let e = build(
        "",
        "    .area: UndersideKeepout { .x &= 0mm; .y &= 0mm; .width &= 1mm; .height &= 1mm; .max_height &= 1mm }\n",
    )
    .map(|_| ())
    .expect_err("on the board");
    assert!(
        e.contains("`area` is an `UndersideKeepout`, and it is the area under a part's body"),
        "{e}"
    );
    let e = build(
        "        .underside_keepout: UndersideKeepout { .x &= 0mm; .y &= 0mm; .width &= 1mm; .height &= 1mm }\n",
        "",
    )
    .map(|_| ())
    .expect_err("no limit");
    assert!(e.contains("`module.underside_keepout` states no `.max_height`"), "{e}");
    let e = build(
        "        .underside_keepout: UndersideKeepout {
            polygon (0mm, 0mm) (2mm, 0mm) (0mm, 2mm)
            .width &= 2mm
            .max_height &= 1mm
        }
",
        "",
    )
    .map(|_| ())
    .expect_err("both shapes");
    assert!(e.contains("has a `polygon` and states `.width`"), "{e}");
    let e = build(
        "        .underside_keepout: UndersideKeepout {
            polygon (0mm, 0mm) round 1mm (2mm, 0mm) (0mm, 2mm)
            .max_height &= 1mm
        }
",
        "",
    )
    .map(|_| ())
    .expect_err("a fillet");
    assert!(e.contains("has straight edges"), "{e}");
}

/// An underside of two depths (issue #1726): a pocket 1.5 mm deep left of the footprint and a
/// bare area 0.3 mm deep right of it, under a module its solder lifts 0.1 mm, with 0.2 mm kept
/// clear: a part may stand 1.4 mm in the pocket and 0.2 mm in the bare area.
const AREAS: &str = "        .underside_keepout: UndersideKeepout {
            .x &= -10mm
            .y &= -5mm
            .width &= 10mm
            .height &= 10mm
            .max_height &= 1.5mm
            .solder_allowance &= 0.1mm
            .margin &= 0.2mm
        }
        .underside_keepout_bare: UndersideKeepout {
            .x &= 0mm
            .y &= -5mm
            .width &= 10mm
            .height &= 10mm
            .max_height &= 0.3mm
            .solder_allowance &= 0.1mm
            .margin &= 0.2mm
        }
";

#[test]
fn each_area_of_an_underside_has_its_own_depth_with_the_allowance_and_margin() {
    let b = built(AREAS);
    let limit = |name: &str| {
        b.design
            .features
            .keepouts
            .iter()
            .find(|k| k.name == name)
            .unwrap_or_else(|| panic!("{name}"))
            .max_height
    };
    assert_eq!(limit("module.underside_keepout"), Some(1.4));
    assert_eq!(limit("module.underside_keepout_bare"), Some(0.2));
}

#[test]
fn a_part_under_the_module_is_refused_where_it_exceeds_the_depth_there_and_accepted_where_not() {
    let b = built(AREAS);
    // `tall` (1.2 mm) in the pocket, `low` (0.4 mm) also in the pocket: both fit.
    assert!(too_tall(&arranged(&b, (-6.0, 3.0), (-6.0, -3.0))).is_empty());
    // `tall` in the bare area, 0.2 mm deep: refused, by that area's name.
    let found = too_tall(&arranged(&b, (-6.0, 0.0), (6.0, 0.0)));
    assert_eq!(found.len(), 1, "{found:?}");
    let (path, keepout, text) = &found[0];
    assert_eq!((path.as_str(), keepout.as_str()), ("tall", "module.underside_keepout_bare"));
    assert!(
        text.contains("the underside keepout `.underside_keepout_bare` of `module` allows 0.20 mm there"),
        "{text}"
    );
    // `low` (0.4 mm) in the bare area is too tall too, by the margin: 0.3 + 0.1 - 0.2 = 0.2.
    let found = too_tall(&arranged(&b, (6.0, 3.0), (-6.0, 0.0)));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "low");
}

#[test]
fn the_placer_keeps_parts_out_of_an_area_too_shallow_for_them() {
    let board = placed(&built(AREAS));
    assert!(too_tall(&board).is_empty(), "{:?}", too_tall(&board));
}

#[test]
fn an_area_named_otherwise_or_left_no_room_by_its_margin_is_refused() {
    let e = build(
        "        .bare: UndersideKeepout { .x &= 0mm; .y &= 0mm; .width &= 1mm; .height &= 1mm; .max_height &= 1mm }\n",
        "",
    )
    .map(|_| ())
    .expect_err("another name");
    assert!(e.contains("`.underside_keepout_sim`) for each more area"), "{e}");
    let e = build(
        "        .underside_keepout_bare: UndersideKeepout { .x &= 0mm; .y &= 0mm; .width &= 1mm; .height &= 1mm; .max_height &= 0.2mm; .margin &= 0.2mm }\n",
        "",
    )
    .map(|_| ())
    .expect_err("no room");
    assert!(
        e.contains("`module.underside_keepout_bare.max_height` is 0.2 mm, which with its 0 mm `.solder_allowance` and less its 0.2 mm `.margin` leaves 0 mm"),
        "{e}"
    );
    let e = build(
        "        .underside_keepout: UndersideKeepout { .x &= 0mm; .y &= 0mm; .width &= 1mm; .height &= 1mm; .max_height &= 1mm; .margin &= -0.1mm }\n",
        "",
    )
    .map(|_| ())
    .expect_err("a negative margin");
    assert!(e.contains("`module.underside_keepout.margin` is -0.1 mm"), "{e}");
}
