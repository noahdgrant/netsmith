//! Height limits (ADR 0045, "Height limits"): a part states how tall it stands,
//! a keepout states the tallest a part may be where it is, the annealer keeps a tall part out
//! from under a low zone, and `check_placement` and `fab` name a part that is too tall, with
//! the numbers. A part with no stated height is reported as unchecked, never taken as flat.
//! Every board here is invented: a 40 by 30 mm board with a low lid over its top 18 mm.

use std::fs;

use netsmith::fab::{FabOptions, Fabricated, fab_with_options};
use netsmith::{Libraries, build_with};
use netsmith_check::Loader;
use netsmith_pcb::{Board, PlacementProblem, Pt, Side, place_options};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

/// The board: a rail and `parts` resistors between its nets.
fn source(parts: usize) -> String {
    let mut body = String::new();
    for n in 1..=parts {
        body += &format!(
            "    .r{n}: Resistor {{ .resistance &= 15kohm +/- 1% }}\n    .rail.pos ~> .r{n} ~> .rail.neg\n"
        );
    }
    format!(
        "import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Widget {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .rail: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }}
{body}}}
"
    )
}

/// The physical file of the board: its outline, and `extra`, the rest of its physical
/// statements (the zones with a height limit, and the heights of parts, which a physical
/// file may state for the board).
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

/// The standard library and the physical file of `extra`.
fn loader(extra: &str) -> Loader {
    Loader::std().with_physical("widget.nsm", "widget.physical.nsm", &physical(extra))
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

/// The board `extra` makes, placed by the placer on the sample libraries.
fn place(parts: usize, extra: &str, sides: &[(&str, Side)]) -> Board {
    let libraries = Libraries::sample();
    let built = build_with(
        "widget.nsm",
        &source(parts),
        &loader(extra),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let design = &built[0].design;
    let mut options = place_options(design, None);
    for (refdes, side) in sides {
        options.sides.insert((*refdes).to_string(), *side);
    }
    netsmith_place_route::board_from_design_placed(
        design,
        &libraries.mapping,
        &libraries.footprints,
        &options,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// The tall part and the others: `r1` is 9 mm tall and `r2` to `r5` 1 mm.
const HEIGHTS: &str = "    .r1.height &= 9mm
    .r2.height &= 1mm
    .r3.height &= 1mm
    .r4.height &= 1mm
    .r5.height &= 1mm
";

fn too_tall(board: &Board) -> Vec<PlacementProblem> {
    board
        .check_placement()
        .into_iter()
        .filter(|p| matches!(p, PlacementProblem::TooTall { .. }))
        .collect()
}

#[test]
fn a_part_states_its_height_and_a_keepout_its_limit() {
    let libraries = Libraries::sample();
    let built = build_with(
        "widget.nsm",
        &source(5),
        &loader(&format!("{HEIGHTS}{LID}")),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let features = &built[0].design.features;
    let heights: Vec<(&str, f64)> = features
        .heights
        .iter()
        .map(|h| (h.path.as_str(), h.height))
        .collect();
    assert_eq!(
        heights,
        [
            ("r1", 9.0),
            ("r2", 1.0),
            ("r3", 1.0),
            ("r4", 1.0),
            ("r5", 1.0)
        ]
    );
    assert_eq!(features.keepouts.len(), 1);
    let lid = &features.keepouts[0];
    assert_eq!(lid.max_height, Some(3.0));
    // A zone that only limits height forbids nothing else.
    assert!(!lid.tracks && !lid.vias && !lid.copper && !lid.parts);
}

#[test]
fn the_annealer_keeps_a_tall_part_out_from_under_the_low_lid() {
    let extra = format!("{HEIGHTS}{LID}");
    let board = place(5, &extra, &[]);
    assert_eq!(board.check_placement(), vec![], "a clean placement");
    let tall = board.part("R1").expect("R1");
    assert_eq!(tall.height, Some(9.0));
    // The lid covers y from 0 to 18 mm of the board; the tall part is below it.
    let lid_bottom = board.outline.min.y + 18.0;
    assert!(
        tall.body().min.y >= lid_bottom - 1e-6,
        "R1's body is at y = {:.2} mm from the top, under the lid",
        tall.body().min.y - board.outline.min.y
    );
    // The lid does not keep the low parts out: some of them are under it.
    assert!(
        board
            .parts
            .iter()
            .filter(|p| p.refdes != "R1")
            .any(|p| p.body().min.y < lid_bottom),
        "no low part stands under the lid"
    );
}

#[test]
fn without_the_limit_the_same_tall_part_is_placed_under_where_the_lid_would_be() {
    // The same board without the lid puts the tall part in the top rows, so the test above
    // is the limit's doing and not an accident of the rows.
    let board = place(5, HEIGHTS, &[]);
    let tall = board.part("R1").expect("R1");
    assert!(tall.body().min.y < board.outline.min.y + 18.0);
}

#[test]
fn the_same_inputs_give_the_same_board() {
    let extra = format!("{HEIGHTS}{LID}");
    let poses = |b: &Board| -> Vec<(String, Pt, f64, Side)> {
        b.parts
            .iter()
            .map(|p| (p.refdes.clone(), p.at, p.rotation, p.side))
            .collect()
    };
    assert_eq!(poses(&place(5, &extra, &[])), poses(&place(5, &extra, &[])));
}

#[test]
fn a_tall_part_where_it_stands_under_the_lid_is_named_with_the_numbers() {
    let extra = format!("{HEIGHTS}{LID}");
    let mut board = place(5, &extra, &[]);
    // Put the tall part under the lid, as a person who moved it by hand would.
    let top = board.outline.min;
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R1")
        .unwrap()
        .at = Pt::new(top.x + 20.0, top.y + 6.0);
    let found = too_tall(&board);
    assert_eq!(found.len(), 1, "{found:?}");
    let text = found[0].to_string();
    for want in [
        "`r1` (R1)",
        "stands 9.00 mm tall on the top face",
        "the keepout `lid` allows 3.00 mm there (6.00 mm too tall)",
        "around (20.0, 6.0) mm",
    ] {
        assert!(text.contains(want), "{want:?} is not in {text:?}");
    }
    // A part exactly as tall as the limit is not too tall.
    let mut board = place(5, &format!("{LID}    .r1.height &= 3mm\n"), &[]);
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R1")
        .unwrap()
        .at = Pt::new(top.x + 20.0, top.y + 6.0);
    assert_eq!(too_tall(&board), vec![]);
}

#[test]
fn a_part_with_no_height_is_reported_by_name_and_not_as_too_tall() {
    // Only r1 states a height. The others stand where they stand, maybe under the lid.
    let extra = format!("    .r1.height &= 2mm\n{LID}");
    let mut board = place(4, &extra, &[]);
    // Put R2 under the lid.
    let top = board.outline.min;
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R2")
        .unwrap()
        .at = Pt::new(top.x + 20.0, top.y + 6.0);
    assert_eq!(too_tall(&board), vec![]);
    let unchecked = board.heights_unchecked();
    let r2 = unchecked
        .iter()
        .find(|u| u.refdes == "R2")
        .expect("R2 is unchecked");
    let text = r2.to_string();
    assert!(
        text.contains("height of `r2` (R2) is unchecked")
            && text.contains("the keepout `lid` limits parts on the top face to 3.00 mm")
            && text.contains("states no height"),
        "{text}"
    );
    // R1 states one, so it is not unchecked.
    assert!(unchecked.iter().all(|u| u.refdes != "R1"));
}

#[test]
fn a_board_with_no_height_limit_reports_nothing() {
    let board = place(4, "    .r1.height &= 9mm\n", &[]);
    assert_eq!(board.heights_unchecked(), vec![]);
    assert_eq!(too_tall(&board), vec![]);
}

#[test]
fn a_back_face_part_has_its_own_limit() {
    let lids = format!(
        "{HEIGHTS}{LID}    .base: Keepout {{
        .x &= 0mm
        .y &= 0mm
        .width &= 40mm
        .height &= 18mm
        .layers &= \"bottom\"
        .max_height &= 8mm
    }}
"
    );
    // The tall part on the back is limited by the bottom zone (8 mm), not the top one (3 mm).
    let mut board = place(5, &lids, &[("R1", Side::Back)]);
    assert_eq!(
        board.check_placement(),
        vec![],
        "the annealer keeps clear of it"
    );
    let tall = board.part("R1").unwrap();
    assert_eq!(tall.side, Side::Back);
    assert!(tall.body().min.y >= board.outline.min.y + 18.0 - 1e-6);
    let top = board.outline.min;
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R1")
        .unwrap()
        .at = Pt::new(top.x + 20.0, top.y + 6.0);
    let found = too_tall(&board);
    assert_eq!(found.len(), 1, "{found:?}");
    let text = found[0].to_string();
    assert!(
        text.contains("on the bottom face") && text.contains("the keepout `base` allows 8.00 mm"),
        "{text}"
    );
    // On the back, a limit on the top face alone does not apply.
    let mut board = place(5, &format!("{HEIGHTS}{LID}"), &[("R1", Side::Back)]);
    board
        .parts
        .iter_mut()
        .find(|p| p.refdes == "R1")
        .unwrap()
        .at = Pt::new(top.x + 20.0, top.y + 6.0);
    assert_eq!(too_tall(&board), vec![]);
}

/// The error text of a source that must be refused.
fn refused(extra: &str) -> String {
    build_with(
        "widget.nsm",
        &source(2),
        &loader(extra),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .map(|_| ())
    .expect_err("this source must be refused")
}

#[test]
fn a_height_that_is_not_one_positive_value_is_refused_with_its_name() {
    let e = refused("    .r1.height &= 2mm..5mm\n");
    assert!(e.contains("`r1.height` is 2mm..5mm, a range"), "{e}");
    let e = refused("    .r1.height &= -1mm\n");
    assert!(
        e.contains("`r1.height` is -1 mm, and a part does not stand below the board"),
        "{e}"
    );
}

#[test]
fn a_limit_that_is_nothing_or_names_no_face_is_refused() {
    let zone = |lines: &str| {
        format!(
            "    .lid: Keepout {{\n        .x &= 0mm\n        .y &= 0mm\n        .width &= 10mm\n        .height &= 10mm\n{lines}    }}\n"
        )
    };
    let e = refused(&zone("        .max_height &= 0mm\n"));
    assert!(e.contains("`lid.max_height` is 0 mm"), "{e}");
    let e = refused(&zone("        .max_height &= 1mm..2mm\n"));
    assert!(e.contains("`lid.max_height` is 1mm..2mm, a range"), "{e}");
    let e = refused(&zone(
        "        .max_height &= 3mm\n        .layers &= \"inner1\"\n",
    ));
    assert!(e.contains("`lid`"), "{e}");
    // A keepout that states nothing at all is still the error it was.
    let e = refused(&zone(
        "        .parts &= \"allow\"\n        .tracks &= \"allow\"\n        .vias &= \"allow\"\n        .copper &= \"allow\"\n",
    ));
    assert!(e.contains("forbids nothing"), "{e}");
}

/// `fab` of the board in a fresh folder, with `layout` as the layout file.
fn fab(name: &str, extra: &str, layout: &str) -> (TempDir, Result<Vec<Fabricated>, String>) {
    fab_with(name, extra, layout, false, false)
}

/// [`fab`], with strict constraints as `strict` and `--allow-drc-violations` as `allow`.
fn fab_with(
    name: &str,
    extra: &str,
    layout: &str,
    strict: bool,
    allow: bool,
) -> (TempDir, Result<Vec<Fabricated>, String>) {
    let dir = TempDir::unused(&format!("height-{name}"));
    let file = dir.join("source").join("widget.pcb.toml");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    if !layout.is_empty() {
        fs::write(&file, layout).unwrap();
    }
    let made = fab_with_options(
        "widget.nsm",
        &source(5),
        &loader(extra),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
        None,
        &FabOptions {
            layout: Some(file),
            strict_heights: strict,
            allow_drc_violations: allow,
            ..FabOptions::default()
        },
    );
    (dir, made)
}

#[test]
fn fab_places_a_tall_part_clear_of_the_lid_and_names_the_parts_it_cannot_check() {
    // r1 is tall and r2 low; r3 to r5 state nothing, and stand where the placer put them.
    let extra = format!("    .r1.height &= 9mm\n    .r2.height &= 1mm\n{LID}");
    let (_dir, made) = fab("clear", &extra, "");
    let made = made.unwrap_or_else(|e| panic!("{e}"));
    let notes = &made[0].notes;
    // Whichever of r3 to r5 stand under the lid are named; a part is never taken as flat.
    let unchecked: Vec<&str> = notes
        .lines()
        .filter(|l| {
            l.contains("is unchecked: the keepout `lid` limits parts on the top face to 3.00 mm")
        })
        .collect();
    assert!(notes.contains("design rules check: passed"), "{notes}");
    assert!(!unchecked.is_empty(), "{notes}");
    assert!(
        unchecked
            .iter()
            .all(|l| l.starts_with("warning: height of `r")),
        "{notes}"
    );
    assert!(
        !notes.contains("height of `r1` (R1)") && !notes.contains("height of `r2` (R2)"),
        "{notes}"
    );
}

#[test]
fn fab_stops_on_a_tall_part_the_layout_file_locks_under_the_lid() {
    let extra = format!("    .r1.height &= 9mm\n{LID}");
    let layout = "version = 1\n\n[Widget.parts.r1]\nat = [20.0, 6.0]\nrotation = 0.0\nside = \"top\"\nlock = true\n";
    let (dir, made) = fab("locked", &extra, layout);
    let e = made.expect_err("a 9 mm part under a 3 mm lid must stop fab");
    assert!(
        e.contains("`r1` (R1) stands 9.00 mm tall on the top face")
            && e.contains("the keepout `lid` allows 3.00 mm there (6.00 mm too tall)"),
        "{e}"
    );
    assert!(!dir.join("fab/Widget/Widget-gerbers.zip").exists());
}

#[test]
fn a_height_nobody_stated_is_a_warning_by_default_and_an_error_when_constraints_are_strict() {
    // Only r1 states a height; the others stand under the lid or not, wherever the placer put
    // them, and at least one of them is under it (the default run names them).
    let extra = format!("    .r1.height &= 9mm\n{LID}");
    let (_dir, made) = fab("unknown-default", &extra, "");
    let notes = made.unwrap_or_else(|e| panic!("{e}"))[0].notes.clone();
    assert!(
        notes.contains("warning: height of `r"),
        "a warning, and the package is made: {notes}"
    );
    let (dir, made) = fab_with("unknown-strict", &extra, "", true, false);
    let e = made.expect_err("strict constraints make an unknown height an error");
    assert!(
        e.contains("error: Widget: height of `r")
            && e.contains(
                "is unchecked: the keepout `lid` limits parts on the top face to 3.00 mm"
            )
            && e.contains("constraints are strict")
            && e.contains("does not waive it"),
        "{e}"
    );
    assert!(!dir.join("fab/Widget/Widget-gerbers.zip").exists());
    // `--allow-drc-violations` does not waive it.
    let (_dir, made) = fab_with("unknown-allow", &extra, "", true, true);
    assert!(made.is_err(), "not waived by --allow-drc-violations");
    // Strict constraints with every height stated: nothing to refuse.
    let all = format!("{HEIGHTS}{LID}");
    let (_dir, made) = fab_with("known-strict", &all, "", true, false);
    made.unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn constraints_are_strict_only_when_the_run_or_the_manifest_says_error() {
    use netsmith::fab::constraints_strict as strict;
    // Unset: a violated requirement is an error, but an unknown height stays a warning.
    assert!(!strict(None, None));
    assert!(strict(Some("error"), None));
    assert!(!strict(Some("warn"), None));
    // `--constraints error|warn` (`Some(warn)`) wins over the manifest, both ways.
    assert!(strict(Some("warn"), Some(false)));
    assert!(strict(None, Some(false)));
    assert!(!strict(Some("error"), Some(true)));
}
