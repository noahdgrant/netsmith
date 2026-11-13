//! The copper as data (`netsmith_pcb::copper_of`, ADR 0105) is the copper of the Gerbers: for
//! routed example boards, each copper layer drawn from the data and the layer fab writes are
//! rasterised and compared, and so are the pours alone against the filler's own output.

use netsmith_fab::gbr::{Contour, Gerber};
use netsmith_fab::geom::Pt as FabPt;
use netsmith_fab::raster::{self, Image, View};
use netsmith_pcb::{copper_of, Area, Copper, Board, kicad_pcb, kicad_pro};
use netsmith_router::route;

use crate::common;

/// Pixels per millimetre: 25 µm pixels.
const SCALE: f64 = 40.0;

/// An example as a routed board: its design file, with `pours` as the lines added inside the
/// four-layer example's `.process: FourLayer` (the outer pours, as `kicad_drc.rs` adds them),
/// resolved by the design stage as `common::four_layer_process_with` does.
fn routed(example: &str, pours: &[&str]) -> Board {
    let mut design = common::example_design(example);
    if !pours.is_empty() {
        assert_eq!(example, "four_layer");
        design.process = Some(common::four_layer_process_with(pours));
    }
    let mut board = netsmith_placer::board_from_design(
        &design,
        &netsmith_kicad::Mapping::standard(),
        &common::fixtures(),
    )
    .unwrap();
    assert!(route(&mut board).is_empty(), "{example} routes");
    board
}

fn fab_pt(p: netsmith_pcb::Pt) -> FabPt {
    FabPt::new(p.x, p.y)
}

/// One area drawn alone: its outline dark, its holes clear.
fn area_gerber(origin: FabPt, a: &Area) -> String {
    let ring = |r: &[netsmith_pcb::Pt]| {
        Contour::polygon(&r.iter().map(|p| fab_pt(*p)).collect::<Vec<_>>())
    };
    let mut g = Gerber::new(origin);
    g.region(ring(&a.outer), None, &[]);
    g.polarity(false);
    for h in &a.holes {
        g.region(ring(h), None, &[]);
    }
    g.text("Other", false, "copper")
}

/// The union of the areas, rasterised on `view`: each drawn alone, so one area's hole never
/// clears another area.
fn render_areas<'a>(origin: FabPt, areas: impl Iterator<Item = &'a Area>, view: &View) -> Image {
    let mut out =
        raster::render_gerber(&Gerber::new(origin).text("Other", false, "x"), view, SCALE).unwrap();
    for a in areas {
        let img = raster::render_gerber(&area_gerber(origin, a), view, SCALE).unwrap();
        for (o, b) in out.bits.iter_mut().zip(&img.bits) {
            *o |= b;
        }
    }
    out
}

/// A copper layer from the data: pours, pads, drawings, tracks and vias.
fn render_layer(c: &Copper, layer: &str, origin: FabPt, view: &View) -> Image {
    let l = c.layers.iter().find(|l| l.layer == layer).unwrap();
    let areas = l
        .pours
        .iter()
        .map(|p| &p.area)
        .chain(l.pads.iter().flat_map(|p| &p.areas))
        .chain(l.fills.iter().map(|f| &f.area))
        .chain(&l.drawings);
    let mut img = render_areas(origin, areas, view);
    let mut g = Gerber::new(origin);
    for t in &l.tracks {
        g.line(t.width, None, fab_pt(t.from), fab_pt(t.to), &[]);
    }
    for v in &c.vias {
        g.flash(
            netsmith_fab::gbr::Ap::Circle(v.diameter),
            None,
            fab_pt(v.at),
            &[],
        );
    }
    let strokes = raster::render_gerber(&g.text("Other", false, "x"), view, SCALE).unwrap();
    for (o, b) in img.bits.iter_mut().zip(&strokes.bits) {
        *o |= b;
    }
    img
}

fn suffix(layer: &str) -> String {
    layer.replace('.', "_")
}

/// Compares every copper layer of the board, whole and pours alone.
fn compare(example: &str, pours: &[&str]) {
    let board = routed(example, pours);
    let c = copper_of(&board).unwrap();
    let art = netsmith_fab::artwork::Artwork::parse(&kicad_pcb(&board)).unwrap();
    let rules = netsmith_fab::plot_rules_of(Some(&kicad_pro(&board)));
    let plot = netsmith_fab::plot::plot(&art, "b", &rules).unwrap();
    let edge = netsmith_fab::edge::outline(&art).unwrap();
    let mut pours = 0;
    for layer in &art.copper {
        let file = plot
            .files
            .iter()
            .find(|f| f.name == format!("b-{}.gbr", suffix(layer)))
            .unwrap();
        let text = String::from_utf8(file.bytes.clone()).unwrap();
        let view = raster::gerber_extent(&text).unwrap().grow(0.5);

        let want = raster::render_gerber(&text, &view, SCALE).unwrap();
        let got = render_layer(&c, layer, art.origin, &view);
        let diff = raster::compare_tolerant(&want, &got, 1).unwrap();
        assert!(
            diff.only_a + diff.only_b == 0 && want.lit() > 0,
            "{example} {layer}: {diff:?}"
        );

        // The pours alone, against the filler writing them into a Gerber of their own.
        let mut g = Gerber::new(art.origin);
        netsmith_fab::fill::pour(&mut g, &art, layer, &edge, &rules).unwrap();
        let want = raster::render_gerber(&g.text("Copper", false, "b"), &view, SCALE).unwrap();
        let l = c.layers.iter().find(|l| l.layer == *layer).unwrap();
        let got = render_areas(art.origin, l.pours.iter().map(|p| &p.area), &view);
        let diff = raster::compare_tolerant(&want, &got, 1).unwrap();
        assert!(
            diff.only_a + diff.only_b == 0,
            "{example} {layer} pours: {diff:?}"
        );
        pours += l.pours.len();
    }
    // The pours the board has, and no other.
    let zones = board.planes.len() + board.pours.len();
    assert_eq!(
        pours > 0,
        zones > 0,
        "{example}: {pours} pieces of {zones} zones"
    );
}

#[test]
fn the_planes_of_the_four_layer_example_are_its_gerbers_copper() {
    compare("four_layer", &[]);
}

#[test]
fn the_planes_and_empty_signal_layers_of_the_six_layer_example_are_its_gerbers_copper() {
    compare("six_layer", &[]);
}

#[test]
fn the_planes_and_empty_signal_layers_of_the_ten_layer_example_are_its_gerbers_copper() {
    compare("ten_layer", &[]);
}

#[test]
fn outer_pours_with_stitching_vias_are_the_gerbers_copper() {
    compare(
        "four_layer",
        &[
            ".top.net &= \"GND\"",
            ".bottom.net &= \"GND\"",
            ".top.stitch_pitch &= 0mm..7mm",
            ".bottom.stitch_pitch &= 0mm..7mm",
        ],
    );
}

#[test]
fn a_board_with_no_pour_is_its_gerbers_copper() {
    compare("led", &[]);
}
