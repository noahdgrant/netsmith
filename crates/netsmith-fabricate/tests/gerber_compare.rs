//! The check `fab --cross-check` runs with no KiCad (issue #1158): the copper of the board model
//! against the copper of the Gerbers written for it, each layer
//! (`netsmith_fabricate::gerber_compare::check_board`). The board is the one `pcb_picture`'s
//! tests place and route by hand, so that neither the placer nor the router moves it.

use netsmith_fab::{BomEntry, Options, package};
use netsmith_fabricate::gerber_compare::{Only, check_board};
use netsmith_pcb::{Pt, kicad_pcb};
use netsmith_testkit::TempDir;

/// The Gerbers of the hand-routed board, and the board.
fn made() -> (TempDir, netsmith_pcb::Board, Vec<u8>) {
    let board = crate::pcb_picture::board();
    let dir = TempDir::new("gerber-compare-check");
    let pcb = dir.join("Picture.kicad_pcb");
    std::fs::write(&pcb, kicad_pcb(&board)).unwrap();
    // GND is left open on purpose, so the check finds an unconnected pad: package it anyway.
    let entries: Vec<BomEntry> = board
        .parts
        .iter()
        .filter(|p| p.refdes.starts_with(['R', 'D']))
        .map(|p| BomEntry {
            designators: vec![p.refdes.clone()],
            value: "x".to_string(),
            footprint: p.footprint.name.clone(),
            lcsc: Some("C1".to_string()),
        })
        .collect();
    let package = package(
        &pcb,
        &entries,
        &Options {
            allow_drc_violations: true,
            ..Options::default()
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));
    (dir, board, package.gerber_zip)
}

#[test]
fn the_gerbers_of_the_board_agree_with_it_on_every_layer() {
    let (_dir, board, zip) = made();
    let check = check_board(&board, &zip).unwrap_or_else(|e| panic!("{e}"));
    let layers: Vec<&str> = check.layers.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(layers, ["top", "bottom"]);
    assert!(check.ok(), "{check}");
    assert!(check.layers.iter().all(|(_, a)| a.model > 0), "{check}");
    assert!(
        check
            .to_string()
            .starts_with("cross-check against netsmith's viewer: the Gerbers agree"),
        "{check}"
    );
}

#[test]
fn a_pad_moved_after_the_gerbers_were_written_is_named_with_its_place() {
    let (_dir, mut board, zip) = made();
    let resistor = board
        .parts
        .iter_mut()
        .find(|p| p.refdes.starts_with('R'))
        .unwrap();
    let was = resistor.at;
    resistor.at = was + Pt::new(0.4, 0.0);
    let check = check_board(&board, &zip).unwrap_or_else(|e| panic!("{e}"));
    assert!(!check.ok(), "{check}");
    let text = check.to_string();
    assert!(text.contains("DIFFERENCES"), "{text}");
    assert!(text.contains("<- too different"), "{text}");
    // Each place is a box to draw with `--region`, on the top, where the resistor is.
    let (layer, top) = &check.layers[0];
    assert_eq!(layer, "top");
    assert!(top.places.iter().any(|p| p.only == Only::Model), "{text}");
    assert!(top.places.iter().any(|p| p.only == Only::Files), "{text}");
    assert!(text.contains("--region"), "{text}");
    // The place is near the resistor (a box in mm from the outline's top left corner).
    let near = was - board.outline.min;
    for p in &top.places {
        let [x, y, w, h] = p.region;
        assert!(
            (x - near.x).abs() < 3.0 && (y - near.y).abs() < 3.0 && w < 3.0 && h < 3.0,
            "{text}"
        );
    }
    // The layer the part is not on agrees.
    assert!(check.layers[1].1.ok(&check.limits), "{text}");
}
