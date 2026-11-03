//! A synthetic board (`tests/data/features.kicad_pcb`) with every shape and feature the
//! writers handle: pads turned at odd angles, oval, rounded, chamfered, trapezoid and custom
//! pads, slots, holes with and without copper, a part on the back, arcs and vias, planes and
//! a pour with a rule area and a cutout, text and drawings.

use netsmith_fab::artwork::Artwork;
use netsmith_fab::crosscheck::{Limits, check};
use netsmith_fab::raster::{self, View};
use netsmith_fab::{KicadCliExe, drill, plot};

const BOARD: &str = include_str!("data/features.kicad_pcb");

fn plotted() -> (Artwork, plot::Plot) {
    let art = Artwork::parse(BOARD).unwrap();
    let rules = plot::Rules {
        edge_clearance: 0.2,
        hole_clearance: Some(0.1),
    };
    let p = plot::plot(&art, "features", &rules).unwrap();
    (art, p)
}

fn file<'a>(files: &'a [netsmith_fab::Output], suffix: &str) -> &'a str {
    std::str::from_utf8(
        &files
            .iter()
            .find(|f| f.name.ends_with(suffix))
            .unwrap_or_else(|| panic!("no {suffix}"))
            .bytes,
    )
    .unwrap()
}

#[test]
fn every_gerber_of_the_board_loads_in_the_rasteriser_and_has_something_on_it() {
    let (_, p) = plotted();
    let outline = raster::gerber_extent(file(&p.files, "-Edge_Cuts.gbr")).unwrap();
    let view: View = outline.grow(1.0);
    for f in &p.files {
        let text = std::str::from_utf8(&f.bytes).unwrap();
        let image =
            raster::render_gerber(text, &view, 20.0).unwrap_or_else(|e| panic!("{}: {e}", f.name));
        assert!(image.lit() > 0, "{} is empty", f.name);
        assert!(
            text.starts_with("%TF.GenerationSoftware,netsmith"),
            "{}",
            f.name
        );
        assert!(text.trim_end().ends_with("M02*"), "{}", f.name);
    }
    let names: Vec<&str> = p.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "features-B_Cu.gbr",
            "features-B_Mask.gbr",
            "features-B_Paste.gbr",
            "features-B_Silkscreen.gbr",
            "features-Edge_Cuts.gbr",
            "features-F_Cu.gbr",
            "features-F_Mask.gbr",
            "features-F_Paste.gbr",
            "features-F_Silkscreen.gbr",
            "features-In1_Cu.gbr",
            "features-In2_Cu.gbr",
        ]
    );
    // The text the font lacks is reported, not dropped.
    assert!(
        p.notes
            .iter()
            .any(|n| n.contains("`\u{e9}`") && n.contains("F.SilkS")),
        "{:?}",
        p.notes
    );
}

#[test]
fn the_holes_of_the_board_are_all_in_the_drill_files() {
    let (art, _) = plotted();
    let files = drill::files(&art, "features");
    let holes =
        |i: usize| raster::parse_excellon(std::str::from_utf8(&files[i].bytes).unwrap()).unwrap();
    let pth = holes(0);
    let npth = holes(1);
    // Three header pins (one a slot) and five vias; two holes without plating.
    assert_eq!(pth.len(), 3 + 5, "{pth:?}");
    assert_eq!(pth.iter().filter(|h| h.slot_to.is_some()).count(), 1);
    assert!(pth.iter().all(|h| h.plated));
    assert_eq!(npth.len(), 2, "{npth:?}");
    assert_eq!(npth.iter().filter(|h| h.slot_to.is_some()).count(), 1);
    let slot = pth.iter().find(|h| h.slot_to.is_some()).unwrap();
    assert!(
        (slot.diameter - 0.8).abs() < 1e-9,
        "a slot is routed with a tool as wide as it"
    );
}

#[test]
fn the_board_agrees_with_kicads_own_plot() {
    let Some(kicad) = KicadCliExe::locate() else {
        eprintln!("skipped: no KiCad");
        return;
    };
    let dir = netsmith_kicad::testlib::TempDir::new("fab-features");
    std::fs::write(dir.join("features.kicad_pcb"), BOARD).unwrap();
    std::fs::write(
        dir.join("features.kicad_pro"),
        include_str!("data/features.kicad_pro"),
    )
    .unwrap();
    let report = check(&dir.join("features.kicad_pcb"), &kicad, &Limits::default())
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(report.ok(), "{report}");
    // Every layer was compared: four copper, mask, paste and silkscreen of both faces
    // (two of them empty at the back are still compared), and the outline.
    assert!(report.layers.len() >= 9, "{report}");
}
