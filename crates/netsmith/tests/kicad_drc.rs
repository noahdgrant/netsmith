//! KiCad's own design rule check on the files netsmith writes, using the rules in the
//! `.kicad_pro` written beside them, for the boards that the whole pipeline builds from source
//! text: outlines, cutouts, pours, keepouts and layout files (the example boards from their
//! design files, and the invented ones, are in netsmith-router's `kicad_drc`). These run
//! `kicad-cli` (found as `netsmith_fab::KicadCliExe::locate` does: `$KICAD_CLI`, `PATH`, Windows
//! KiCad under WSL), and are skipped where it is not installed.

use std::process::Command;

use netsmith_fab::KicadCliExe;
use netsmith_pcb::{
    Board, CopperLayer, Layout, PlaceOptions, Pt, Rect, Track, Via, apply_layout, kicad_pcb,
    kicad_pro,
};
use netsmith_router::route;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

use crate::drc_kit::{Report, check, copper_only, plane_drc};

/// An example as a board, with KiCad's own footprints when they are installed, so that
/// KiCad finds the footprints as they are in its library.
fn board(example: &str) -> Board {
    let path = format!(
        "{}/../../examples/{example}.nsm",
        env!("CARGO_MANIFEST_DIR")
    );
    let physical = std::fs::read_to_string(path.replace(".nsm", ".physical.nsm")).ok();
    board_with(
        &path,
        &std::fs::read_to_string(&path).unwrap(),
        physical.as_deref(),
    )
}

/// The standard library and, if there is one, the physical file `physical` paired with
/// `path`.
fn loader(path: &str, physical: Option<&str>) -> netsmith_check::Loader {
    let mut loader = netsmith_check::Loader::std();
    if let Some(physical) = physical {
        loader.attach_physical(path, "board.physical.nsm", physical);
    }
    loader
}

/// A source that states its physical intent inside the board (see
/// `netsmith_testkit::split_physical`) as the circuit source and its physical file.
fn split(path: &str, text: &str) -> (String, netsmith_check::Loader) {
    let (circuit, physical) = netsmith_testkit::split_physical(text);
    let loader = loader(path, physical.as_deref());
    (circuit, loader)
}

/// A board from source text.
fn board_from(path: &str, text: &str) -> Board {
    let (circuit, physical) = netsmith_testkit::split_physical(text);
    board_with(path, &circuit, physical.as_deref())
}

/// A board from source text and its physical file.
fn board_with(path: &str, text: &str, physical: Option<&str>) -> Board {
    let installed = netsmith::Libraries::standard();
    let libraries = if installed.footprints.root().is_dir() {
        installed
    } else {
        netsmith::Libraries::sample()
    };
    let built = netsmith::build_with(
        path,
        text,
        &loader(path, physical),
        &Catalogs::sample(),
        &libraries,
    )
    .unwrap();
    // `NETSMITH_TEST_SEED` asks the placer for another arrangement of every board, to see how
    // a test depends on the arrangement it happens to get.
    let seed = std::env::var("NETSMITH_TEST_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    netsmith_placer::board_from_design_placed(
        &built[0].design,
        &libraries.mapping,
        &libraries.footprints,
        &PlaceOptions {
            seed,
            ..PlaceOptions::default()
        },
    )
    .unwrap()
}

/// The regulator example with its input rail asking for a wide track.
fn wide_regulator() -> Board {
    let path = format!(
        "{}/../../examples/linear_regulator.nsm",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap().replace(
        "        .voltage &= 5V +/- 5%\n",
        "        .voltage &= 5V +/- 5%\n        .min_track &= 0.6mm..1m\n",
    );
    board_from(&path, &text)
}

#[test]
fn a_rail_that_asks_for_a_wide_track_has_its_nets_routed_wide_and_passes_kicads_check() {
    let mut b = wide_regulator();
    assert_eq!(
        b.net_widths,
        [("GND".to_string(), 0.6), ("VIN".to_string(), 0.6)]
    );
    assert!(route(&mut b).is_empty());
    for net in ["VIN", "GND"] {
        let widths: Vec<f64> = b
            .tracks
            .iter()
            .filter(|t| t.net == net)
            .map(|t| t.width)
            .collect();
        assert!(!widths.is_empty(), "{net} has tracks");
        assert!(widths.iter().all(|&w| w >= 0.6 - 1e-9), "{net}: {widths:?}");
    }
    assert!(
        b.tracks
            .iter()
            .filter(|t| t.net == "3V3")
            .all(|t| t.width == b.rules.track_width),
        "a net no rail speaks for stays at the rules' width"
    );
    let Some(report) = check("wide", &b) else {
        return;
    };
    assert_eq!(report.violations, Vec::<String>::new());
    assert_eq!(report.unconnected, 0);
    // The project file tells KiCad which nets are wide.
    let pro = kicad_pro(&b);
    assert!(pro.contains("\"name\": \"Track_0.6mm\""), "{pro}");
    assert!(pro.contains("\"pattern\": \"VIN\""), "{pro}");
}

// ---- outline, cutouts, mechanical parts (ADR 0020) ------------------------------------------

const REGULATOR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/linear_regulator.nsm"
);

/// The regulator example's source with `imports` after its doc comment and `members` added
/// to its board.
fn regulator_source(imports: &str, members: &str) -> String {
    let text = std::fs::read_to_string(REGULATOR).unwrap();
    let end = text.rfind('}').unwrap();
    let first_import = text.find("import ").unwrap();
    format!(
        "{}{imports}{}{members}}}\n",
        &text[..first_import],
        &text[first_import..end]
    )
}

/// The regulator example with `members` added to its board, and the `std::pcb` and
/// mechanical imports they may need.
fn regulator_with(members: &str) -> Board {
    let source = regulator_source(
        "import std::pcb::{Cutout, Keepout, Outline}\nimport std::parts::mechanical::{Fiducial, MountingHole}\n",
        members,
    );
    board_from(REGULATOR, &source)
}

fn problems(b: &Board) -> Vec<String> {
    b.check_placement()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn a_fixed_outline_is_the_boards_size_with_the_parts_centred_in_it() {
    let b = regulator_with(
        "    .outline: Outline {\n        .width &= 60mm\n        .height &= 40mm\n    }\n",
    );
    assert_eq!((b.outline.width(), b.outline.height()), (60.0, 40.0));
    assert_eq!(problems(&b), Vec::<String>::new());
    // Centred: as much space on the left as on the right.
    let courtyards: Vec<_> = b.parts.iter().map(|p| p.courtyard()).collect();
    let left = courtyards.iter().map(|c| c.min.x).fold(f64::MAX, f64::min) - b.outline.min.x;
    let right = b.outline.max.x - courtyards.iter().map(|c| c.max.x).fold(0.0, f64::max);
    // (The margin on each side is the wall or, where the reference text sticks out of the
    // courtyards, the text and its edge space, so the two differ by the text.)
    let wall = b.rules.edge_wall();
    assert!(
        left >= wall - 1e-9 && right >= wall - 1e-9,
        "{left} {right}"
    );
    assert!((left - right).abs() < 3.0, "{left} {right}");
}

#[test]
fn a_size_range_takes_the_least_that_holds_the_parts() {
    let sized = board("linear_regulator");
    let b = regulator_with(
        "    .outline: Outline {\n        .width &= 0mm..90mm\n        .height &= 0mm..90mm\n    }\n",
    );
    assert!(
        b.outline.width() <= sized.outline.width() + 0.1,
        "{}",
        b.outline.width()
    );
    assert!(b.outline.width() > 0.0 && b.outline.width() < 90.0);
    assert_eq!(problems(&b), Vec::<String>::new());
}

#[test]
fn an_outline_too_small_for_the_parts_is_refused_with_the_overflow() {
    let text = regulator_source(
        "import std::pcb::Outline\n",
        "    .outline: Outline {\n        .width &= 5mm\n        .height &= 40mm\n    }\n",
    );
    let path = REGULATOR;
    let libraries = netsmith::Libraries::sample();
    let (text, loader) = split(path, &text);
    let built =
        netsmith::build_with(path, &text, &loader, &Catalogs::sample(), &libraries).unwrap();
    let err = netsmith_placer::board_from_design(
        &built[0].design,
        &libraries.mapping,
        &libraries.footprints,
    )
    .unwrap_err()
    .to_string();
    assert!(
        // Parts wider than the outline in every turn are named with their sizes (the rows'
        // size, which used to be given, says nothing about the least the parts need).
        err.contains("the outline is 5.0 x 40.0 mm, which leaves 4.2 x 39.2 mm")
            && err.contains(" has a courtyard of ")
            && err.contains(" turned 90 degrees"),
        "{err}"
    );
    assert!(!err.contains("the parts need"), "{err}");
}

#[test]
fn an_outline_the_rows_overflow_is_placed_if_the_annealer_fits_the_parts() {
    // The rows (which turn nothing) need more than 12 mm across; the annealer turns and packs
    // the parts, and the placement check finds nothing wrong (ADR 0045, section 10).
    let b = regulator_with(
        "    .outline: Outline {\n        .width &= 12mm\n        .height &= 40mm\n    }\n",
    );
    assert_eq!((b.outline.width(), b.outline.height()), (12.0, 40.0));
    assert_eq!(problems(&b), Vec::<String>::new());
}

const ROUNDED: &str = "    .outline: Outline {\n        .width &= 50mm\n        .height &= 36mm\n        .corner_radius &= 4mm\n    }\n";

#[test]
fn a_rounded_outline_is_written_as_lines_and_arcs_and_passes_kicads_check() {
    let mut b = regulator_with(ROUNDED);
    assert_eq!(b.corner_radius, 4.0);
    assert_eq!(problems(&b), Vec::<String>::new());
    assert!(route(&mut b).is_empty());
    let pcb = kicad_pcb(&b);
    assert_eq!(pcb.matches("(gr_arc").count(), 4, "four corners");
    assert_eq!(pcb.matches("(gr_line").count(), 4);
    assert!(!pcb.contains("(gr_rect"));
    let Some(report) = check("rounded", &b) else {
        return;
    };
    assert_eq!(report.violations, Vec::<String>::new());
    assert_eq!(report.unconnected, 0);
}

#[test]
fn a_part_in_a_rounded_corner_is_outside_the_outline() {
    let mut b = regulator_with(ROUNDED);
    // The courtyard's corner is within 2 mm of both edges at the corner: inside the sharp
    // rectangle and outside the arc.
    let part = &mut b.parts[0];
    let c = part.footprint.courtyard;
    part.at = Pt::new(
        b.outline.min.x + 1.0 - c.min.x,
        b.outline.min.y + 1.0 - c.min.y,
    );
    let found = problems(&b);
    assert!(
        found
            .iter()
            .any(|p| p.contains("is not wholly inside the board outline")),
        "{found:?}"
    );
}

const CUTOUT: &str = "    .outline: Outline {\n        .width &= 60mm\n        .height &= 40mm\n        .corner_radius &= 3mm\n    }\n    .slot: Cutout {\n        .x &= 5mm\n        .y &= 4mm\n        .width &= 6mm\n        .height &= 10mm\n        .corner_radius &= 1.5mm\n    }\n";

#[test]
fn a_cutout_is_edge_cuts_and_the_router_keeps_clear_of_it() {
    let mut b = regulator_with(CUTOUT);
    assert_eq!(problems(&b), Vec::<String>::new());
    assert!(route(&mut b).is_empty());
    let hole = b.cutout_shapes()[0].1.rect;
    let near = hole.expanded(b.rules.copper_to_edge);
    for t in &b.tracks {
        let r = Rect::new(
            Pt::new(t.from.x.min(t.to.x), t.from.y.min(t.to.y)),
            Pt::new(t.from.x.max(t.to.x), t.from.y.max(t.to.y)),
        )
        .expanded(t.width / 2.0);
        assert!(!r.overlaps(&near), "{t:?}");
    }
    let pcb = kicad_pcb(&b);
    assert_eq!(pcb.matches("(gr_arc").count(), 8, "four corners of each");
    let Some(report) = check("cutout", &b) else {
        return;
    };
    assert_eq!(report.violations, Vec::<String>::new());
    assert_eq!(report.unconnected, 0);
}

#[test]
fn a_part_over_a_cutout_and_a_cutout_off_the_board_are_found_and_named() {
    let mut b = regulator_with(CUTOUT);
    let hole = b.cutout_shapes()[0].1.rect;
    let part = &mut b.parts[0];
    let c = part.footprint.courtyard;
    part.at = Pt::new(hole.min.x - c.min.x, hole.min.y - c.min.y);
    let r = netsmith_pcb::board_member(&part.path, &part.refdes);
    let found = problems(&b);
    assert!(
        found.contains(&format!("{r} overlaps the cutout `slot`")),
        "{found:?}"
    );
    b.cutouts[0].at = Some(Pt::new(58.0, 4.0));
    let found = problems(&b);
    assert!(
        found.contains(&"the cutout `slot` is not wholly inside the board outline".to_string()),
        "{found:?}"
    );
    b.cutouts[0].at = None;
    let found = problems(&b);
    assert!(
        found
            .iter()
            .any(|p| p.starts_with("the cutout `slot` has no position")),
        "{found:?}"
    );
}

#[test]
fn holes_and_fiducials_go_in_the_corners_of_a_fixed_outline() {
    // The sample libraries have no fiducial footprint; the stock one comes with KiCad.
    if netsmith_kicad::skip_without_kicad_libraries() {
        return;
    }
    let mut b = regulator_with(&format!(
        "{ROUNDED}    .h1: MountingHole\n    .h2: MountingHole\n    .h3: MountingHole\n    .h4: MountingHole\n    .f1: Fiducial\n"
    ));
    assert_eq!(problems(&b), Vec::<String>::new());
    let holes: Vec<_> = b
        .parts
        .iter()
        .filter(|p| p.kind == "MountingHole")
        .collect();
    assert_eq!(holes.len(), 4);
    let mid = b.outline.centre();
    let mut quadrants: Vec<(bool, bool)> = holes
        .iter()
        .map(|h| {
            let c = h.courtyard().centre();
            (c.x < mid.x, c.y < mid.y)
        })
        .collect();
    quadrants.sort();
    assert_eq!(
        quadrants,
        [(false, false), (false, true), (true, false), (true, true)],
        "one in each corner"
    );
    assert!(route(&mut b).is_empty());
    let Some(report) = check("holes", &b) else {
        return;
    };
    assert_eq!(report.violations, Vec::<String>::new());
    assert_eq!(report.unconnected, 0);
}

// ---- outer pours and stitching vias (ADR 0020) ----------------------------------------------

/// The four-layer example with `process` lines added inside its `.process: FourLayer`, and
/// `members` on the board.
fn four_layer_with(process: &str, members: &str) -> Board {
    let path = format!(
        "{}/../../examples/four_layer.nsm",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let text = text.replace(
        "        .inner2.net &= \"3V3\"\n",
        &format!("        .inner2.net &= \"3V3\"\n{process}"),
    );
    let first_import = text.find("import ").unwrap();
    let end = text.rfind('}').unwrap();
    let text = format!(
        "{}import std::pcb::{{Cutout, Keepout, Outline}}\n{}{members}}}\n",
        &text[..first_import],
        &text[first_import..end]
    );
    board_from(&path, &text)
}

const POURS: &str = "        .top.net &= \"GND\"\n        .bottom.net &= \"GND\"\n        .top.stitch_pitch &= 0mm..7mm\n        .bottom.stitch_pitch &= 0mm..7mm\n";

#[test]
fn outer_pours_are_zones_on_the_outer_layers() {
    let b = four_layer_with(POURS, "");
    assert_eq!(b.pours.len(), 2);
    let pcb = kicad_pcb(&b);
    let zones: Vec<&str> = pcb.split("\n\t(zone").skip(1).collect();
    let on = |layer: &str| {
        zones
            .iter()
            .filter(|z| z.contains(&format!("(net_name \"GND\")\n\t\t(layer \"{layer}\")")))
            .count()
    };
    assert_eq!((on("F.Cu"), on("B.Cu"), on("In1.Cu")), (1, 1, 1));
    assert_eq!(zones.len(), 4, "two planes and two pours");
}

#[test]
fn stitching_vias_join_the_pours_to_the_ground_plane_on_a_grid() {
    let mut b = four_layer_with(POURS, "");
    let before = b.vias.len();
    assert!(route(&mut b).is_empty());
    let stitching: Vec<&Via> = b.vias.iter().filter(|v| v.net == "GND").collect();
    assert!(stitching.len() > before, "vias were added");
    // The vias the grid added are at least the rules' space from the pads and the edge.
    let rules = &b.rules;
    for v in &stitching {
        let r = Rect::new(v.at, v.at).expanded(v.diameter / 2.0);
        assert!(b.edge_distance(&r) >= rules.copper_to_edge - 1e-9, "{v:?}");
    }
    // No two vias are nearer than their holes allow.
    for (i, a) in b.vias.iter().enumerate() {
        for c in &b.vias[i + 1..] {
            assert!(
                a.at.distance(c.at) >= rules.via_hole_to_hole + rules.via_drill - 1e-9,
                "{a:?} {c:?}"
            );
        }
    }
}

#[test]
fn a_pour_with_no_pitch_or_no_other_layer_is_not_stitched() {
    let mut none = four_layer_with("        .top.net &= \"GND\"\n", "");
    let mut with = four_layer_with(POURS, "");
    assert!(route(&mut none).is_empty());
    assert!(route(&mut with).is_empty());
    assert!(with.vias.len() > none.vias.len());
    // A pour on a net no other layer carries has nothing to be stitched to.
    let mut lone = four_layer_with(
        "        .top.net &= \"VIN\"\n        .top.stitch_pitch &= 0mm..7mm\n",
        "",
    );
    assert!(route(&mut lone).is_empty());
    assert_eq!(lone.vias.len(), none.vias.len());
}

#[test]
fn a_board_with_outer_pours_passes_kicads_check_with_the_zones_filled() {
    let mut b = four_layer_with(POURS, "");
    assert!(route(&mut b).is_empty());
    let Some(counts) = plane_drc("pours", &b) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert_eq!(counts, Default::default(), "violations, unconnected pads");
}

// ---- keepouts (ADR 0029) ---------------------------------------------------------------------

/// A keepout on the regulator board, 8 by 8 mm at (40, 4) from the outline's corner.
fn keepout_source(fields: &str) -> String {
    format!(
        "    .outline: Outline {{\n        .width &= 60mm\n        .height &= 40mm\n    }}\n    .antenna: Keepout {{\n        .x &= 40mm\n        .y &= 4mm\n        .width &= 8mm\n        .height &= 8mm\n{fields}    }}\n"
    )
}

fn count(report: &Report, kind: &str) -> usize {
    report.violations.iter().filter(|v| *v == kind).count()
}

#[test]
fn kicad_reads_a_keepout_as_a_rule_area_and_flags_what_it_forbids() {
    let allow = |keep: &str| -> String {
        ["tracks", "vias", "copper", "parts"]
            .iter()
            .filter(|f| **f != keep)
            .map(|f| format!("        .{f} &= \"allow\"\n"))
            .collect()
    };
    for (forbids, intruder) in [("tracks", "track"), ("vias", "via"), ("parts", "part")] {
        let mut b = regulator_with(&keepout_source(&allow(forbids)));
        let c = b.keepout_areas()[0].rect.centre();
        match intruder {
            "track" => b.tracks.push(Track {
                net: "VIN".into(),
                layer: CopperLayer::Front,
                from: Pt::new(c.x - 2.0, c.y),
                to: Pt::new(c.x + 2.0, c.y),
                width: 0.25,
                mid: None,
            }),
            "via" => b.vias.push(Via {
                net: "VIN".into(),
                at: c,
                diameter: 0.6,
                drill: 0.3,
            }),
            _ => {
                let p = &mut b.parts[0];
                let cy = p.footprint.courtyard.centre();
                p.at = Pt::new(c.x - cy.x, c.y - cy.y);
            }
        }
        let Some(report) = check(forbids, &b) else {
            return;
        };
        assert!(
            count(&report, "items_not_allowed") > 0,
            "a {intruder} in a keepout that forbids {forbids}: {:?}",
            report.violations
        );
        // The same intruder in a keepout that allows it is not flagged.
        let mut ok = regulator_with(&keepout_source(&allow("copper")));
        ok.tracks = b.tracks.clone();
        ok.vias = b.vias.clone();
        if intruder == "part" {
            ok.parts[0].at = b.parts[0].at;
        }
        let report = check(&format!("{forbids}-allowed"), &ok).unwrap();
        assert_eq!(count(&report, "items_not_allowed"), 0, "{intruder}");
    }
}

#[test]
fn a_part_or_a_pad_in_a_keepout_is_found_and_named() {
    let mut b = regulator_with(&keepout_source(""));
    assert_eq!(problems(&b), Vec::<String>::new());
    let c = b.keepout_areas()[0].rect.centre();
    let p = &mut b.parts[0];
    let cy = p.footprint.courtyard.centre();
    p.at = Pt::new(c.x - cy.x, c.y - cy.y);
    let r = netsmith_pcb::board_member(&p.path, &p.refdes);
    let found = problems(&b);
    assert!(
        found.contains(&format!(
            "{r} overlaps the keepout `antenna`, which forbids parts"
        )),
        "{found:?}"
    );
    assert!(
        found.contains(&format!(
            "a pad of {r} is inside the keepout `antenna`, which forbids tracks, so none can reach it"
        )),
        "{found:?}"
    );
    b.keepouts[0].at = None;
    assert!(
        problems(&b)
            .iter()
            .any(|p| p.starts_with("the keepout `antenna` has no position")),
    );
}

#[test]
fn a_keepout_a_part_carries_moves_with_it() {
    let mut b = regulator_with(&keepout_source(""));
    // Make the board's keepout one a part carries, 3 mm to the part's left.
    let path = b.parts[1].path.clone();
    b.keepouts[0].part = Some(path);
    b.keepouts[0].at = Some(Pt::new(-12.0, -4.0));
    let first = b.keepout_areas()[0].rect;
    b.parts[1].at.x += 10.0;
    let second = b.keepout_areas()[0].rect;
    assert!((second.min.x - first.min.x - 10.0).abs() < 1e-9);
    assert!((first.width() - second.width()).abs() < 1e-9);
}

/// Whether KiCad's own fill of the top pour on `net` covers each point, or `None` if KiCad's
/// Python is not here.
fn pour_covers(name: &str, b: &Board, net: &str, points: &[Pt]) -> Option<Vec<bool>> {
    let Some(cli) = KicadCliExe::locate_runnable() else {
        eprintln!("skipped: no kicad-cli (set KICAD_CLI, or install KiCad)");
        return None;
    };
    // Windows KiCad has its Python beside kicad-cli.exe; elsewhere it is the system's, which
    // must have KiCad's `pcbnew` module.
    let python = cli.python();
    if cli.windows() {
        if !python.is_file() {
            eprintln!("skipped: no KiCad Python at {}", python.display());
            return None;
        }
    } else if !netsmith_fab::exclusive::run(|| {
        let mut command = Command::new(&python);
        command.args(["-c", "import pcbnew"]);
        command
    })
    .is_ok_and(|ran| ran.success())
    {
        eprintln!("skipped: {} has no pcbnew module", python.display());
        return None;
    }
    let base = cli.scratch_base().unwrap_or_else(|e| panic!("{e}"));
    let dir = TempDir::new_in(&base, &format!("pcb-pour-{name}"));
    std::fs::write(dir.join("b.kicad_pcb"), kicad_pcb(b)).ok()?;
    std::fs::write(dir.join("b.kicad_pro"), kicad_pro(b)).ok()?;
    let spots: Vec<String> = points
        .iter()
        .map(|p| format!("({}, {})", p.x, p.y))
        .collect();
    let script = format!(
        "import sys, pcbnew\n\
         board = pcbnew.LoadBoard(sys.argv[1])\n\
         pcbnew.ZONE_FILLER(board).Fill(board.Zones())\n\
         zones = [z for z in board.Zones() if not z.GetIsRuleArea() and z.GetNetname() == '{net}' and z.IsOnLayer(pcbnew.F_Cu)]\n\
         for (x, y) in [{}]:\n\
         \tp = pcbnew.VECTOR2I(pcbnew.FromMM(x), pcbnew.FromMM(y))\n\
         \tprint('covered' if any(z.HitTestFilledArea(pcbnew.F_Cu, p, 0) for z in zones) else 'bare')\n",
        spots.join(", ")
    );
    std::fs::write(dir.join("hit.py"), script).ok()?;
    let script = cli.arg(&dir.join("hit.py")).ok()?;
    let board = cli.arg(&dir.join("b.kicad_pcb")).ok()?;
    let ran = netsmith_fab::exclusive::run(|| {
        let mut command = Command::new(&python);
        command.arg(&script).arg(&board);
        command
    })
    .ok()?;
    assert!(ran.success(), "KiCad's python failed:\n{}", ran.report());
    Some(
        String::from_utf8_lossy(&ran.output.stdout)
            .lines()
            .map(|l| l.trim() == "covered")
            .collect(),
    )
}

#[test]
fn kicads_fill_leaves_the_pour_out_of_a_keepout_that_forbids_copper() {
    let mut b = four_layer_with(
        "        .top.net &= \"GND\"\n",
        "    .outline: Outline {\n        .width &= 60mm\n        .height &= 40mm\n    }\n    .k: Keepout {\n        .x &= 5mm\n        .y &= 5mm\n        .width &= 6mm\n        .height &= 6mm\n        .layers &= \"top\"\n        .tracks &= \"allow\"\n        .vias &= \"allow\"\n        .parts &= \"allow\"\n    }\n",
    );
    assert!(route(&mut b).is_empty());
    let inside = b.outline.min + Pt::new(8.0, 8.0);
    let outside = b.outline.min + Pt::new(15.0, 8.0);
    let Some(covered) = pour_covers("copper", &b, "GND", &[inside, outside]) else {
        return;
    };
    assert_eq!(
        covered,
        [false, true],
        "bare inside the keepout, poured outside it"
    );
}

// ---- the layout file ---------------------------------------------------------------------------

const FIXED: &str =
    "    .outline: Outline {\n        .width &= 60mm\n        .height &= 40mm\n    }\n";

fn layout(text: &str) -> Layout {
    Layout::parse(text, "demo.pcb.toml").unwrap_or_else(|e| panic!("{e}"))
}

/// The regulator board with `members`, placed as `fab` does it: the layout file tells the
/// placer which face each part is on and what to keep clear of, and is then put on the board.
fn laid(members: &str, layout_text: &str) -> Board {
    let source = regulator_source(
        "import std::pcb::{Cutout, Keepout, Outline}\nimport std::parts::mechanical::{Fiducial, MountingHole}\n",
        members,
    );
    let libraries = netsmith::Libraries::sample();
    let (source, loader) = split(REGULATOR, &source);
    let built =
        netsmith::build_with(REGULATOR, &source, &loader, &Catalogs::sample(), &libraries).unwrap();
    let design = &built[0].design;
    let layout = layout(&layout_text.replace('@', &design.name));
    let mut b = netsmith_placer::board_from_design_placed(
        design,
        &libraries.mapping,
        &libraries.footprints,
        &netsmith_pcb::place_options(design, Some(&layout)),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    apply_layout(&mut b, &layout, "demo.pcb.toml").unwrap_or_else(|e| panic!("{e}"));
    b
}

fn apply(b: &mut Board, text: &str) -> Result<(), String> {
    let text = text.replace("@", &b.name);
    apply_layout(b, &layout(&text), "demo.pcb.toml").map_err(|e| e.to_string())
}

#[test]
fn a_layout_entry_places_its_part_from_the_outlines_corner_and_the_rest_stay() {
    let mut b = regulator_with(FIXED);
    let before: Vec<Pt> = b.parts.iter().map(|p| p.at).collect();
    let path = b.parts[1].path.clone();
    apply(
        &mut b,
        &format!(
            "version = 1\n[@.parts.\"{path}\"]\nat = [10.0, 12.5]\nrotation = 90\nlock = true\n"
        ),
    )
    .unwrap();
    assert_eq!(b.parts[1].at, b.outline.min + Pt::new(10.0, 12.5));
    assert_eq!(b.parts[1].rotation, 90.0);
    for (i, p) in b.parts.iter().enumerate().filter(|(i, _)| *i != 1) {
        assert_eq!(p.at, before[i], "{}", p.refdes);
    }
}

/// A saved placement is keyed by the part's path (ADR 0032), so the letters of a designator
/// (ADR 0046) can change under it without moving the part: the same design, with every
/// designator renamed, is placed where the layout file says.
#[test]
fn a_layout_entry_still_places_its_part_when_the_designator_letters_change() {
    let source = regulator_source("import std::pcb::{Cutout, Keepout, Outline}\n", FIXED);
    let libraries = netsmith::Libraries::sample();
    let (source, loader) = split(REGULATOR, &source);
    let built =
        netsmith::build_with(REGULATOR, &source, &loader, &Catalogs::sample(), &libraries).unwrap();
    let design = &built[0].design;
    let path = design.parts[1].path.clone();
    let text = format!(
        "version = 1\n[{}.parts.\"{path}\"]\nat = [10.0, 12.5]\nrotation = 90\nlock = true\n",
        design.name
    );
    let at = |design: &netsmith_core::Design| {
        let layout = layout(&text);
        let mut b = netsmith_placer::board_from_design_placed(
            design,
            &libraries.mapping,
            &libraries.footprints,
            &netsmith_pcb::place_options(design, Some(&layout)),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        apply_layout(&mut b, &layout, "demo.pcb.toml").unwrap_or_else(|e| panic!("{e}"));
        let part = b.parts.iter().find(|p| p.path == path).unwrap().clone();
        (part.at - b.outline.min, part.rotation, part.refdes)
    };
    let before = at(design);
    let mut renamed = design.clone();
    for p in &mut renamed.parts {
        p.refdes = format!("Z{}", p.refdes);
    }
    for n in &mut renamed.nets {
        for pin in &mut n.pins {
            pin.refdes = format!("Z{}", pin.refdes);
        }
    }
    let after = at(&renamed);
    assert_eq!(before.0, Pt::new(10.0, 12.5));
    assert_eq!((before.0, before.1), (after.0, after.1));
    assert_eq!(after.2, format!("Z{}", before.2));
}

#[test]
fn a_layout_for_another_board_changes_nothing() {
    let mut b = regulator_with(FIXED);
    let before = b.clone();
    apply_layout(
        &mut b,
        &layout("version = 1\n[Other.parts.r1]\nat = [1, 1]\n"),
        "demo.pcb.toml",
    )
    .unwrap();
    assert_eq!(b, before);
}

#[test]
fn an_entry_for_a_part_that_is_gone_is_an_error_naming_it_and_changes_nothing() {
    let mut b = regulator_with(FIXED);
    let before = b.clone();
    let path = b.parts[0].path.clone();
    let err = apply(
        &mut b,
        &format!("version = 1\n[@.parts.\"{path}\"]\nat = [5, 5]\n[@.parts.\"stage.gone\"]\nat = [1, 1]\n[@.parts.\"also.gone\"]\nat = [2, 2]\n"),
    )
    .unwrap_err();
    assert!(
        err.starts_with("demo.pcb.toml: the board `RegulatorExample` has no part `also.gone`, `stage.gone` (its parts: ")
            && err.contains(&format!("`{path}`"))
            && err.contains("remove the entries or rename them"),
        "{err}"
    );
    assert_eq!(b, before, "nothing was moved");
    let err = apply(&mut b, "version = 1\n[@.keepouts.nope]\nat = [1, 1]\n").unwrap_err();
    assert!(
        err.contains("has no keepout `nope` (its keepouts: none)"),
        "{err}"
    );
}

#[test]
fn a_part_the_layout_puts_on_the_bottom_is_on_the_back_and_nothing_else_moves_there() {
    let plain = regulator_with(FIXED);
    let i = plain
        .parts
        .iter()
        .position(|p| p.kind == "Capacitor")
        .unwrap();
    let path = plain.parts[i].path.clone();
    let b = laid(
        FIXED,
        &format!("version = 1\n[@.parts.\"{path}\"]\nat = [5, 5]\nside = \"bottom\"\n"),
    );
    assert_eq!(b.parts[i].side, netsmith_pcb::Side::Back);
    assert_eq!(b.parts[i].at, b.outline.min + Pt::new(5.0, 5.0));
    assert!(
        b.parts
            .iter()
            .enumerate()
            .filter(|(k, _)| *k != i)
            .all(|(_, p)| p.side == netsmith_pcb::Side::Front)
    );
    assert_eq!(problems(&b), Vec::<String>::new());
    // Captured again, the layout says bottom.
    let kept = netsmith_pcb::Layout::capture(&b, None);
    assert_eq!(kept.parts[&path].side, netsmith_pcb::LayoutFace::Bottom);
}

#[test]
fn a_keepout_and_a_cutout_without_a_place_take_the_files_and_nothing_is_missing() {
    let members = format!(
        "{FIXED}    .antenna: Keepout {{\n        .width &= 8mm\n        .height &= 8mm\n    }}\n    .slot: Cutout {{\n        .width &= 6mm\n        .height &= 10mm\n    }}\n"
    );
    let b = regulator_with(&members);
    let found = problems(&b);
    assert!(
        found
            .iter()
            .any(|p| p.starts_with("the keepout `antenna` has no position")),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .any(|p| p.starts_with("the cutout `slot` has no position")),
        "{found:?}"
    );
    // The placer keeps clear of the places the file gives.
    let b = laid(
        &members,
        "version = 1\n[@.keepouts.antenna]\nat = [40, 4]\n[@.cutouts.slot]\nat = [5, 4]\n",
    );
    assert_eq!(problems(&b), Vec::<String>::new());
    assert_eq!(
        b.keepout_areas()[0].rect.min,
        b.outline.min + Pt::new(40.0, 4.0)
    );
    assert_eq!(
        b.cutout_shapes()[0].1.rect.min,
        b.outline.min + Pt::new(5.0, 4.0)
    );
}

#[test]
fn a_place_stated_in_the_source_and_in_the_file_is_refused() {
    let mut b = regulator_with(&format!(
        "{FIXED}    .slot: Cutout {{\n        .x &= 5mm\n        .y &= 4mm\n        .width &= 6mm\n        .height &= 10mm\n    }}\n"
    ));
    let err = apply(&mut b, "version = 1\n[@.cutouts.slot]\nat = [9, 9]\n").unwrap_err();
    assert!(
        err.contains(
            "the cutout `slot` states `.x` and `.y` in the source, and the file places it as well"
        ),
        "{err}"
    );
}

#[test]
fn a_captured_layout_puts_the_board_back_as_it_was() {
    let mut b = regulator_with(FIXED);
    let mut moved = b.clone();
    moved.parts[0].at.x += 3.0;
    moved.parts[0].rotation = 90.0;
    let mut layout = Layout::default();
    layout
        .boards
        .insert(b.name.clone(), Layout::capture(&moved, None));
    let text = layout.to_text();
    let again = Layout::parse(&text, "x").unwrap();
    apply_layout(&mut b, &again, "x").unwrap();
    for (a, c) in b.parts.iter().zip(&moved.parts) {
        assert!(
            a.at.distance(c.at) < 0.006,
            "{} {:?} {:?}",
            a.refdes,
            a.at,
            c.at
        );
        assert_eq!(a.rotation, c.rotation);
    }
    assert!(text.contains("source = \"auto\""), "{text}");
}

#[test]
fn a_board_laid_out_by_a_file_passes_kicads_check() {
    let mut b = laid(
        &format!(
            "{FIXED}    .slot: Cutout {{\n        .width &= 6mm\n        .height &= 10mm\n    }}\n"
        ),
        "version = 1\n[@.cutouts.slot]\nat = [5, 4]\n",
    );
    assert_eq!(problems(&b), Vec::<String>::new());
    assert!(route(&mut b).is_empty());
    let Some(report) = check("laid", &b) else {
        return;
    };
    // A library footprint's silkscreen can be clipped by the mask wherever the placer puts it
    // (issue #58, silkscreen of library footprints); the copper is what is tested.
    assert_eq!(copper_only(&report), Vec::<&String>::new());
    assert_eq!(report.unconnected, 0);
}

// ---- silkscreen labels -------------------------------------------------------------------------

#[test]
fn a_label_is_silkscreen_text_that_passes_kicads_check() {
    let path = format!("{}/../../examples/led.nsm", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap();
    let first_import = text.find("import ").unwrap();
    let end = text.rfind('}').unwrap();
    let text = format!(
        "{}import std::parts::mechanical::Label\n{}    .rev: Label {{\n        .text &= \"REV A\"\n    }}\n}}\n",
        &text[..first_import],
        &text[first_import..end]
    );
    let mut b = board_from(&path, &text);
    let label = b.parts.iter().find(|p| p.kind == "Label").unwrap();
    assert_eq!(label.value, "REV A");
    assert!(label.footprint.pads.is_empty());
    assert_eq!(problems(&b), Vec::<String>::new());
    assert!(route(&mut b).is_empty());
    let pcb = kicad_pcb(&b);
    assert!(pcb.contains("(fp_text user \"REV A\""), "{pcb}");
    let Some(report) = check("label", &b) else {
        return;
    };
    assert_eq!(report.violations, Vec::<String>::new());
    assert_eq!(report.unconnected, 0);
}

/// A drawn keepout, 10 by 6 mm at (20, 4), that forbids parts and tracks on the top layer.
const DRAWN: &str = "version = 1\n[@.keepouts.drawn1]\nat = [20.0, 4.0]\nsize = [10.0, 6.0]\nlayers = \"top\"\nvias = \"allow\"\ncopper = \"allow\"\n";

#[test]
fn a_keepout_drawn_in_the_layout_file_is_a_keepout_of_the_board() {
    let b = laid(FIXED, DRAWN);
    let k = b.keepouts.iter().find(|k| k.name == "drawn1").unwrap();
    assert!(k.drawn && k.part.is_none());
    assert_eq!(k.size, (10.0, 6.0));
    assert_eq!(k.layers, ["top"]);
    assert_eq!(
        (k.tracks, k.vias, k.copper, k.parts),
        (true, false, false, true)
    );
    let area = b.keepout_areas()[0].rect;
    assert_eq!(area.min, b.outline.min + Pt::new(20.0, 4.0));
    assert_eq!((area.width(), area.height()), (10.0, 6.0));
    // The placer kept the parts out of it, since it forbids parts.
    assert_eq!(problems(&b), Vec::<String>::new());
    for p in &b.parts {
        assert!(!p.courtyard().overlaps(&area), "{} is in it", p.refdes);
    }
}

#[test]
fn a_part_put_in_a_drawn_keepout_is_found_and_named() {
    let mut b = laid(FIXED, DRAWN);
    let path = b.parts[0].path.clone();
    apply(
        &mut b,
        &format!("version = 1\n[@.parts.\"{path}\"]\nat = [24.0, 6.0]\n"),
    )
    .unwrap();
    let found = problems(&b);
    assert!(
        found
            .iter()
            .any(|p| p.contains("overlaps the keepout `drawn1`, which forbids parts")),
        "{found:?}"
    );
}

#[test]
fn a_drawn_keepout_is_captured_whole_and_read_back_the_same() {
    let b = laid(FIXED, DRAWN);
    let kept = Layout::capture(&b, None);
    let entry = &kept.keepouts["drawn1"];
    assert_eq!(entry.size, Some([10.0, 6.0]));
    assert_eq!(entry.layers.as_deref(), Some("top"));
    assert_eq!(
        (entry.vias.as_deref(), entry.tracks.as_deref()),
        (Some("allow"), None),
        "what is forbidden is the default and is left out"
    );
    let mut layout = Layout::default();
    layout.boards.insert(b.name.clone(), kept);
    let text = layout.to_text();
    assert_eq!(Layout::parse(&text, "x").unwrap(), layout);
    let mut again = regulator_with(FIXED);
    apply_layout(&mut again, &layout, "x").unwrap();
    assert_eq!(again.keepout_areas(), b.keepout_areas());
}

#[test]
fn a_drawn_keepout_that_is_wrong_is_refused_with_the_field_and_changes_nothing() {
    let members = format!(
        "{FIXED}    .antenna: Keepout {{\n        .x &= 40mm\n        .y &= 4mm\n        .width &= 8mm\n        .height &= 8mm\n    }}\n    .left: Keepout {{\n        .width &= 4mm\n        .height &= 4mm\n    }}\n"
    );
    let mut b = regulator_with(&members);
    let before = b.clone();
    let mut e = |text: &str| apply(&mut b, text).unwrap_err();
    let drawn = |fields: &str| format!("version = 1\n[@.keepouts.k]\nat = [1.0, 1.0]\n{fields}");
    assert!(
        e(&drawn("size = [0.0, 3.0]\n")).contains("the keepout `k` has the size 0 by 3 mm"),
        "size"
    );
    let layers = e(&drawn("size = [3.0, 3.0]\nlayers = \"top,inner1\"\n"));
    assert!(
        layers
            .contains("names the layer \"inner1\", and this 2-layer board has \"top\", \"bottom\""),
        "{layers}"
    );
    assert!(
        e(&drawn("size = [3.0, 3.0]\ntracks = \"never\"\n"))
            .contains("has `tracks` that is \"never\"; say \"forbid\" or \"allow\""),
    );
    assert!(
        e(&drawn(
            "size = [3.0, 3.0]\ntracks = \"allow\"\nvias = \"allow\"\ncopper = \"allow\"\nparts = \"allow\"\n"
        ))
        .contains("forbids nothing"),
    );
    // A source keepout takes only a place; a drawn one must not have the name of one.
    assert!(
        e("version = 1\n[@.keepouts.left]\nat = [1.0, 1.0]\nlayers = \"top\"\n")
            .contains("the keepout `left` gives `layers` and no `size`"),
    );
    let clash = e("version = 1\n[@.keepouts.antenna]\nat = [1.0, 1.0]\nsize = [3.0, 3.0]\n");
    assert!(
        clash.contains("the keepout `antenna` is drawn in the layout file, and the source has a keepout of that name"),
        "{clash}"
    );
    assert_eq!(b, before, "nothing changed");
}

#[test]
fn a_board_with_a_drawn_keepout_routes_around_it_and_passes_kicads_check() {
    let mut b = laid(FIXED, DRAWN);
    let area = b.keepout_areas()[0].rect;
    assert!(route(&mut b).is_empty());
    for t in b.tracks.iter().filter(|t| t.layer == CopperLayer::Front) {
        let seg = Rect::around([t.from, t.to])
            .unwrap()
            .expanded(t.width / 2.0);
        assert!(!seg.overlaps(&area), "a track crosses the keepout: {t:?}");
    }
    let Some(report) = check("drawn-keepout", &b) else {
        return;
    };
    assert_eq!(
        count(&report, "items_not_allowed"),
        0,
        "{:?}",
        report.violations
    );
    // KiCad has it as a rule area.
    assert!(kicad_pcb(&b).contains("(keepout"), "a rule area is written");
}
