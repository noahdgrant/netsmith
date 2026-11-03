//! The reader on small DXF files written here by hand: no file of a real design.

use netsmith_dxf::{Error, Options, Outline, Polygon, Units, read_outline};

/// A DXF file under construction.
#[derive(Default)]
struct File {
    insunits: Option<i32>,
    blocks: String,
    entities: String,
}

fn pair(code: i32, value: impl std::fmt::Display) -> String {
    format!("{code}\n{value}\n")
}

impl File {
    fn mm() -> File {
        File::units(Some(4))
    }

    fn units(insunits: Option<i32>) -> File {
        File {
            insunits,
            ..File::default()
        }
    }

    fn text(&self) -> String {
        let mut s = String::new();
        s += &pair(0, "SECTION");
        s += &pair(2, "HEADER");
        s += &pair(9, "$ACADVER");
        s += &pair(1, "AC1015");
        if let Some(u) = self.insunits {
            s += &pair(9, "$INSUNITS");
            s += &pair(70, u);
        }
        s += &pair(0, "ENDSEC");
        s += &pair(0, "SECTION");
        s += &pair(2, "BLOCKS");
        s += &self.blocks;
        s += &pair(0, "ENDSEC");
        s += &pair(0, "SECTION");
        s += &pair(2, "ENTITIES");
        s += &self.entities;
        s += &pair(0, "ENDSEC");
        s += &pair(0, "EOF");
        s
    }

    fn add(&mut self, text: String) -> &mut Self {
        self.entities += &text;
        self
    }

    fn line(&mut self, layer: &str, a: (f64, f64), b: (f64, f64)) -> &mut Self {
        self.add(line(layer, a, b))
    }

    fn rect(&mut self, layer: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> &mut Self {
        self.line(layer, (x0, y0), (x1, y0))
            .line(layer, (x1, y0), (x1, y1))
            .line(layer, (x1, y1), (x0, y1))
            .line(layer, (x0, y1), (x0, y0))
    }

    fn polygon(&mut self, layer: &str, pts: &[(f64, f64)]) -> &mut Self {
        for i in 0..pts.len() {
            self.line(layer, pts[i], pts[(i + 1) % pts.len()]);
        }
        self
    }

    fn circle(&mut self, layer: &str, c: (f64, f64), r: f64) -> &mut Self {
        self.add(format!(
            "{}{}{}{}{}{}",
            pair(0, "CIRCLE"),
            pair(8, layer),
            pair(10, c.0),
            pair(20, c.1),
            pair(30, 0),
            pair(40, r)
        ))
    }
}

fn line(layer: &str, a: (f64, f64), b: (f64, f64)) -> String {
    format!(
        "{}{}{}{}{}{}{}{}",
        pair(0, "LINE"),
        pair(8, layer),
        pair(10, a.0),
        pair(20, a.1),
        pair(11, b.0),
        pair(21, b.1),
        pair(30, 0),
        pair(31, 0)
    )
}

fn arc(layer: &str, c: (f64, f64), r: f64, start: f64, end: f64) -> String {
    format!(
        "{}{}{}{}{}{}{}{}",
        pair(0, "ARC"),
        pair(8, layer),
        pair(10, c.0),
        pair(20, c.1),
        pair(40, r),
        pair(50, start),
        pair(51, end),
        ""
    )
}

fn lwpolyline(layer: &str, closed: bool, pts: &[(f64, f64, f64)]) -> String {
    let mut s = pair(0, "LWPOLYLINE") + &pair(8, layer) + &pair(90, pts.len());
    s += &pair(70, i32::from(closed));
    for (x, y, b) in pts {
        s += &pair(10, x);
        s += &pair(20, y);
        if *b != 0.0 {
            s += &pair(42, b);
        }
    }
    s
}

/// The edges of a rounded rectangle from (x0, y0) to (x1, y1) with radius r, counterclockwise,
/// as four lines and four arcs.
fn rounded(layer: &str, x0: f64, y0: f64, x1: f64, y1: f64, r: f64) -> String {
    let mut s = String::new();
    s += &line(layer, (x0 + r, y0), (x1 - r, y0));
    s += &arc(layer, (x1 - r, y0 + r), r, 270.0, 360.0);
    s += &line(layer, (x1, y0 + r), (x1, y1 - r));
    s += &arc(layer, (x1 - r, y1 - r), r, 0.0, 90.0);
    s += &line(layer, (x1 - r, y1), (x0 + r, y1));
    s += &arc(layer, (x0 + r, y1 - r), r, 90.0, 180.0);
    s += &line(layer, (x0, y1 - r), (x0, y0 + r));
    s += &arc(layer, (x0 + r, y0 + r), r, 180.0, 270.0);
    s
}

fn read(file: &File) -> Result<Outline, Error> {
    read_outline(&file.text(), &Options::default())
}

fn with(layer: &str) -> Options {
    Options {
        outline_layers: vec![layer.to_string()],
        ..Options::default()
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn bounds(p: &Polygon) -> ((f64, f64), (f64, f64)) {
    p.bounds()
}

#[test]
fn a_rectangle_is_the_outline_with_its_corner_at_the_origin_and_y_down() {
    let mut f = File::mm();
    f.rect("EDGE", 100.0, 200.0, 160.0, 230.0);
    let o = read(&f).unwrap();
    assert_eq!(o.units, Units::Millimetres);
    assert!(o.units_from_file);
    assert_eq!(o.outline_layers, ["EDGE"]);
    let rr = o.outer.as_rounded_rect().unwrap();
    assert!(close(rr.width, 60.0) && close(rr.height, 30.0) && rr.radius == 0.0);
    assert!(close(rr.x, 0.0) && close(rr.y, 0.0));
    // The origin is the drawing's left edge and top (greatest y).
    assert_eq!(o.origin, (100.0, 230.0));
    assert!(o.cutouts.is_empty() && o.holes.is_empty());
    assert_eq!(o.outer.edge_counts(), (0, 4));
    assert!(close(o.outer.area(), 1800.0));
}

#[test]
fn a_rounded_rectangle_of_lines_and_arcs_is_recognised_with_its_radius() {
    let mut f = File::mm();
    f.add(rounded("EDGE", 0.0, 0.0, 50.0, 36.0, 3.0));
    let o = read(&f).unwrap();
    let rr = o.outer.as_rounded_rect().unwrap();
    assert!(
        close(rr.width, 50.0) && close(rr.height, 36.0) && close(rr.radius, 3.0),
        "{rr:?}"
    );
    assert_eq!(o.outer.edge_counts(), (4, 4));
    // The area of a rounded rectangle: 50 * 36 - (4 - pi) * 3^2.
    let expect = 50.0 * 36.0 - (4.0 - std::f64::consts::PI) * 9.0;
    assert!((o.outer.area() - expect).abs() < 1e-6, "{}", o.outer.area());
}

#[test]
fn an_l_shape_is_a_polygon_that_is_not_a_rounded_rectangle() {
    let mut f = File::mm();
    f.polygon(
        "EDGE",
        &[
            (0.0, 0.0),
            (40.0, 0.0),
            (40.0, 10.0),
            (15.0, 10.0),
            (15.0, 30.0),
            (0.0, 30.0),
        ],
    );
    let o = read(&f).unwrap();
    assert_eq!(o.outer.vertices.len(), 6);
    assert!(o.outer.as_rounded_rect().is_none());
    assert!(close(o.outer.area(), 40.0 * 10.0 + 15.0 * 20.0));
    assert_eq!(bounds(&o.outer), ((0.0, 0.0), (40.0, 30.0)));
    // The drawing has y up, so its bar along the x axis is the board's bottom bar: the inner
    // corner of the L is at y = 30 - 10 = 20.
    assert!(
        o.outer
            .vertices
            .iter()
            .any(|v| close(v.x, 15.0) && close(v.y, 20.0))
    );
}

#[test]
fn a_rounded_cutout_and_circle_holes_come_out_in_the_boards_frame() {
    let mut f = File::mm();
    f.add(rounded("EDGE", 0.0, 0.0, 60.0, 30.0, 2.0));
    // A slot 3 by 12 whose corner nearest the origin is at (6, 4) on the board.
    f.add(rounded("EDGE", 6.0, 14.0, 9.0, 26.0, 1.5));
    // Holes at the board's (4, 4), (56, 4), (4, 26) and (56, 26); y up in the drawing.
    for (x, y) in [(4.0, 26.0), (56.0, 26.0), (4.0, 4.0), (56.0, 4.0)] {
        f.circle("HOLES", (x, y), 1.6);
    }
    let o = read_outline(
        &f.text(),
        &Options {
            outline_layers: vec!["edge".into()], // case does not matter
            holes_layers: vec!["HOLES".into()],
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(o.cutouts.len(), 1);
    let rr = o.cutouts[0].as_rounded_rect().unwrap();
    assert!(
        close(rr.width, 3.0) && close(rr.height, 12.0) && close(rr.radius, 1.5),
        "{rr:?}"
    );
    assert!(close(rr.x, 6.0) && close(rr.y, 4.0), "{rr:?}");
    let at: Vec<(f64, f64, f64)> = o
        .holes
        .iter()
        .map(|h| (h.centre.0, h.centre.1, h.diameter))
        .collect();
    assert_eq!(
        at,
        [
            (4.0, 4.0, 3.2),
            (56.0, 4.0, 3.2),
            (4.0, 26.0, 3.2),
            (56.0, 26.0, 3.2)
        ]
    );
}

#[test]
fn a_circle_on_the_outline_layer_is_a_round_cutout_not_a_hole() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 40.0, 30.0)
        .circle("EDGE", (20.0, 15.0), 5.0);
    let o = read(&f).unwrap();
    assert!(o.holes.is_empty());
    let (centre, d) = o.cutouts[0].as_circle().unwrap();
    assert!(close(d, 10.0) && close(centre.0, 20.0) && close(centre.1, 15.0));
    let rr = o.cutouts[0].as_rounded_rect().unwrap();
    assert!(close(rr.radius, 5.0));
}

#[test]
fn a_closed_lwpolyline_with_bulges_is_a_rounded_rectangle() {
    let b = (std::f64::consts::PI / 8.0).tan(); // a quarter circle
    let r = 4.0;
    let mut f = File::mm();
    f.add(lwpolyline(
        "EDGE",
        true,
        &[
            (r, 0.0, 0.0),
            (30.0 - r, 0.0, b),
            (30.0, r, 0.0),
            (30.0, 20.0 - r, b),
            (30.0 - r, 20.0, 0.0),
            (r, 20.0, b),
            (0.0, 20.0 - r, 0.0),
            (0.0, r, b),
        ],
    ));
    let o = read(&f).unwrap();
    let rr = o.outer.as_rounded_rect().unwrap();
    assert!(
        close(rr.width, 30.0) && close(rr.height, 20.0) && close(rr.radius, 4.0),
        "{rr:?}"
    );
}

#[test]
fn an_old_style_polyline_with_vertices_and_a_bulge_is_read() {
    // R12 has POLYLINE, VERTEX, SEQEND. A half circle closes a 10 x 10 square into a stadium.
    let mut e = pair(0, "POLYLINE") + &pair(8, "EDGE") + &pair(66, 1) + &pair(70, 1);
    for (x, y, b) in [
        (0.0, 0.0, 0.0),
        (10.0, 0.0, 1.0),
        (10.0, 10.0, 0.0),
        (0.0, 10.0, 1.0),
    ] {
        e += &(pair(0, "VERTEX") + &pair(8, "EDGE") + &pair(10, x) + &pair(20, y));
        if b != 0.0 {
            e += &pair(42, b);
        }
    }
    e += &pair(0, "SEQEND");
    let mut f = File::mm();
    f.add(e);
    let o = read(&f).unwrap();
    // 10 high, 10 + 5 + 5 wide: the bulged sides are the right and the left.
    let rr = o.outer.as_rounded_rect().unwrap();
    assert!(
        close(rr.width, 20.0) && close(rr.height, 10.0) && close(rr.radius, 5.0),
        "{rr:?}"
    );
}

fn block(name: &str, entities: &str) -> String {
    pair(0, "BLOCK")
        + &pair(2, name)
        + &pair(70, 0)
        + &pair(10, 0)
        + &pair(20, 0)
        + entities
        + &pair(0, "ENDBLK")
}

fn insert(layer: &str, name: &str, at: (f64, f64), scale: (f64, f64), rotation: f64) -> String {
    pair(0, "INSERT")
        + &pair(8, layer)
        + &pair(2, name)
        + &pair(10, at.0)
        + &pair(20, at.1)
        + &pair(41, scale.0)
        + &pair(42, scale.1)
        + &pair(43, 1)
        + &pair(50, rotation)
}

#[test]
fn a_block_insert_is_moved_scaled_and_turned() {
    let mut f = File::mm();
    // The block is drawn on layer 0, which takes the layer of the insert. A 10 x 5 rectangle.
    f.blocks = block(
        "BOARD",
        &(line("0", (0.0, 0.0), (10.0, 0.0))
            + &line("0", (10.0, 0.0), (10.0, 5.0))
            + &line("0", (10.0, 5.0), (0.0, 5.0))
            + &line("0", (0.0, 5.0), (0.0, 0.0))),
    );
    f.add(insert("EDGE", "BOARD", (100.0, 100.0), (2.0, 2.0), 90.0));
    let o = read(&f).unwrap();
    // Scaled to 20 x 10, then turned a quarter: 10 wide and 20 high.
    let rr = o.outer.as_rounded_rect().unwrap();
    assert!(close(rr.width, 10.0) && close(rr.height, 20.0), "{rr:?}");
    // The drawing's corner after the turn: x from 90 to 100, y from 100 to 120.
    assert!(
        close(o.origin.0, 90.0) && close(o.origin.1, 120.0),
        "{:?}",
        o.origin
    );
}

#[test]
fn a_mirrored_insert_mirrors_arcs_and_corners() {
    let mut f = File::mm();
    // An L with a rounded outer corner at (20, 0)-(20, 10): bulge sign matters.
    f.blocks = block(
        "L",
        &(line("0", (0.0, 0.0), (16.0, 0.0))
            + &arc("0", (16.0, 4.0), 4.0, 270.0, 360.0)
            + &line("0", (20.0, 4.0), (20.0, 10.0))
            + &line("0", (20.0, 10.0), (10.0, 10.0))
            + &line("0", (10.0, 10.0), (10.0, 5.0))
            + &line("0", (10.0, 5.0), (0.0, 5.0))
            + &line("0", (0.0, 5.0), (0.0, 0.0))),
    );
    f.add(insert("EDGE", "L", (0.0, 0.0), (1.0, 1.0), 0.0));
    let plain = read(&f).unwrap();
    let mut g = File::mm();
    g.blocks = f.blocks.clone();
    g.add(insert("EDGE", "L", (0.0, 0.0), (-1.0, 1.0), 0.0));
    let mirrored = read(&g).unwrap();
    assert!(close(plain.outer.area(), mirrored.outer.area()));
    // The rounded corner was at the board's top right; mirrored, it is at the top left, so the
    // top left of the mirrored board is cut away and its bounding box starts at x = -20.
    assert_eq!(mirrored.origin, (-20.0, 10.0));
    assert_eq!(plain.origin, (0.0, 10.0));
    assert_eq!(plain.outer.edge_counts(), mirrored.outer.edge_counts());
    let xs: Vec<f64> = mirrored.outer.vertices.iter().map(|v| v.x).collect();
    assert!(xs.iter().cloned().fold(f64::MAX, f64::min) >= -1e-9);
    // Corner of the rounding on the plain board: the straight edge ends at x = 16 along the top.
    // The drawing's y = 0 edge is the board's bottom, y = 10.
    assert!(
        plain
            .outer
            .vertices
            .iter()
            .any(|v| close(v.x, 16.0) && close(v.y, 10.0))
    );
    assert!(
        mirrored
            .outer
            .vertices
            .iter()
            .any(|v| close(v.x, 4.0) && close(v.y, 10.0))
    );
}

#[test]
fn an_arc_with_a_downward_extrusion_is_mirrored() {
    // The same quarter arc drawn with a normal of (0, 0, -1) is the mirror image in x.
    let mut e = arc("EDGE", (0.0, 5.0), 5.0, 90.0, 270.0);
    e += &(pair(210, 0) + &pair(220, 0) + &pair(230, -1));
    let mut g = File::mm();
    g.line("EDGE", (0.0, 0.0), (10.0, 0.0))
        .line("EDGE", (10.0, 0.0), (10.0, 10.0))
        .line("EDGE", (10.0, 10.0), (0.0, 10.0))
        .add(e);
    let o = read(&g).unwrap();
    // The right half circle of radius 5 at (0, 5) bulges into the rectangle: a 10 x 10 board
    // with a half-disc bite out of its left edge.
    let expect = 100.0 - std::f64::consts::PI * 25.0 / 2.0;
    assert!((o.outer.area() - expect).abs() < 1e-6, "{}", o.outer.area());
}

#[test]
fn inches_are_converted_to_millimetres() {
    let mut f = File::units(Some(1));
    f.rect("EDGE", 0.0, 0.0, 2.0, 1.0)
        .circle("HOLES", (0.25, 0.25), 0.0625);
    let o = read_outline(
        &f.text(),
        &Options {
            holes_layers: vec!["HOLES".into()],
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(o.units, Units::Inches);
    let rr = o.outer.as_rounded_rect().unwrap();
    assert!(close(rr.width, 50.8) && close(rr.height, 25.4), "{rr:?}");
    assert!(close(o.holes[0].diameter, 3.175) && close(o.holes[0].centre.0, 6.35));
    assert!(close(o.holes[0].centre.1, 25.4 - 6.35));
}

#[test]
fn centimetres_and_metres_are_read() {
    for (code, factor) in [(5, 10.0), (6, 1000.0)] {
        let mut f = File::units(Some(code));
        f.rect("EDGE", 0.0, 0.0, 0.5, 0.25);
        let rr = read(&f).unwrap().outer.as_rounded_rect().unwrap();
        assert!(close(rr.width, 0.5 * factor), "{code}: {rr:?}");
    }
}

#[test]
fn a_file_without_units_is_refused_unless_the_caller_states_them() {
    for insunits in [None, Some(0)] {
        let mut f = File::units(insunits);
        f.rect("EDGE", 0.0, 0.0, 10.0, 5.0);
        let e = read(&f).unwrap_err();
        assert_eq!(e, Error::Unitless);
        assert!(e.to_string().contains("--units"), "{e}");
        let o = read_outline(
            &f.text(),
            &Options {
                units: Some(Units::Millimetres),
                ..Options::default()
            },
        )
        .unwrap();
        assert!(!o.units_from_file);
        assert!(close(o.outer.as_rounded_rect().unwrap().width, 10.0));
    }
}

#[test]
fn stated_units_overrule_the_files_with_a_warning() {
    let mut f = File::units(Some(1));
    f.rect("EDGE", 0.0, 0.0, 10.0, 5.0);
    let o = read_outline(
        &f.text(),
        &Options {
            units: Some(Units::Millimetres),
            ..Options::default()
        },
    )
    .unwrap();
    assert!(close(o.outer.as_rounded_rect().unwrap().width, 10.0));
    assert!(
        o.warnings
            .iter()
            .any(|w| w.contains("inch") && w.contains("mm")),
        "{:?}",
        o.warnings
    );
}

#[test]
fn an_unknown_unit_code_is_named() {
    let mut f = File::units(Some(2));
    f.rect("EDGE", 0.0, 0.0, 1.0, 1.0);
    let e = read(&f).unwrap_err();
    assert_eq!(e, Error::UnknownUnits(2));
    assert!(e.to_string().contains("$INSUNITS is 2"), "{e}");
}

#[test]
fn a_gap_in_the_outline_is_reported_with_its_size_and_place() {
    let mut f = File::mm();
    f.line("EDGE", (0.0, 0.0), (20.0, 0.0))
        .line("EDGE", (20.0, 0.0), (20.0, 10.0))
        .line("EDGE", (20.0, 10.0), (0.0, 10.0))
        .line("EDGE", (0.0, 10.0), (0.0, 0.35));
    let e = read(&f).unwrap_err();
    let Error::Gaps { gaps, .. } = &e else {
        panic!("{e:?}")
    };
    assert_eq!(gaps.len(), 2);
    assert!(close(gaps[0].distance.unwrap(), 0.35));
    let text = e.to_string();
    assert!(
        text.contains("0.3500 mm") && text.contains("(0.0000, 0.3500)"),
        "{text}"
    );
}

#[test]
fn a_gap_smaller_than_the_tolerance_is_joined() {
    let mut f = File::mm();
    f.line("EDGE", (0.0, 0.0), (20.0, 0.0))
        .line("EDGE", (20.0005, 0.0), (20.0, 10.0))
        .line("EDGE", (20.0, 10.0), (0.0, 10.0))
        .line("EDGE", (0.0, 10.0), (0.0, 0.0));
    assert!(read(&f).is_ok());
}

#[test]
fn two_outer_loops_are_an_error_naming_both() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 20.0, 10.0)
        .rect("EDGE", 30.0, 0.0, 40.0, 10.0);
    let e = read(&f).unwrap_err();
    let Error::SeveralOuterLoops { loops } = &e else {
        panic!("{e:?}")
    };
    assert_eq!(loops.len(), 2);
    assert!(
        e.to_string().contains("20.000 x 10.000") && e.to_string().contains("10.000 x 10.000"),
        "{e}"
    );
}

#[test]
fn a_bow_tie_is_an_error_where_it_crosses() {
    let mut f = File::mm();
    f.polygon(
        "EDGE",
        &[(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)],
    );
    let e = read(&f).unwrap_err();
    assert!(matches!(e, Error::Crossing { .. }), "{e:?}");
    assert!(e.to_string().contains("crosses or touches itself"), "{e}");
}

#[test]
fn a_cutout_that_touches_the_edge_is_an_error() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 20.0, 10.0)
        .rect("EDGE", 15.0, 2.0, 20.0, 8.0);
    // The cutout shares the edge x = 20 with the outline: the lines overlap and the chain
    // gains a branch, or the loops touch; either way it is refused, not guessed.
    assert!(read(&f).is_err());
    let mut g = File::mm();
    g.rect("EDGE", 0.0, 0.0, 20.0, 10.0)
        .rect("EDGE", 15.0, 2.0, 25.0, 8.0);
    let e = read(&g).unwrap_err();
    assert!(matches!(e, Error::Crossing { .. }), "{e:?}");
}

#[test]
fn a_loop_inside_a_cutout_is_an_error() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 40.0, 40.0)
        .rect("EDGE", 5.0, 5.0, 35.0, 35.0)
        .rect("EDGE", 10.0, 10.0, 30.0, 30.0);
    let e = read(&f).unwrap_err();
    assert!(matches!(e, Error::Island { .. }), "{e:?}");
}

#[test]
fn a_binary_dxf_is_refused_with_a_message() {
    let e = read_outline("AutoCAD Binary DXF\r\n\u{1a}\0", &Options::default()).unwrap_err();
    assert_eq!(e, Error::Binary);
    assert!(e.to_string().contains("ASCII"), "{e}");
}

#[test]
fn text_that_is_not_dxf_is_refused_naming_the_line() {
    let e = read_outline("hello\nworld\n", &Options::default()).unwrap_err();
    let Error::Syntax { line, .. } = e else {
        panic!("{e:?}")
    };
    assert_eq!(line, 1);
}

#[test]
fn several_layers_with_no_choice_are_listed_with_their_counts() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 20.0, 10.0)
        .rect("CASE", -2.0, -2.0, 22.0, 12.0);
    f.circle("HOLES", (3.0, 3.0), 1.0);
    let e = read(&f).unwrap_err();
    let text = e.to_string();
    assert!(matches!(e, Error::AmbiguousLayer { .. }), "{e:?}");
    assert!(
        text.contains("`CASE` (4 LINE)")
            && text.contains("`EDGE` (4 LINE)")
            && text.contains("`HOLES` (1 CIRCLE)"),
        "{text}"
    );
    assert!(text.contains("--layer"), "{text}");
    // Naming the holes layer leaves two outline candidates: still ambiguous.
    // Naming the outline layer settles it, and the other layer is mentioned.
    let o = read_outline(&f.text(), &with("EDGE")).unwrap();
    assert!(
        o.warnings
            .iter()
            .any(|w| w.contains("`CASE`") && w.contains("not used")),
        "{:?}",
        o.warnings
    );
}

#[test]
fn naming_the_holes_layer_leaves_one_outline_layer_to_choose() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 20.0, 10.0)
        .circle("HOLES", (3.0, 3.0), 1.0);
    let o = read_outline(
        &f.text(),
        &Options {
            holes_layers: vec!["HOLES".into()],
            ..Options::default()
        },
    )
    .unwrap();
    assert_eq!(o.outline_layers, ["EDGE"]);
    assert_eq!(o.holes.len(), 1);
}

#[test]
fn an_unknown_layer_lists_the_ones_there_are() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 20.0, 10.0);
    let e = read_outline(&f.text(), &with("OUTLINE")).unwrap_err();
    assert!(matches!(e, Error::UnknownLayer { .. }), "{e:?}");
    assert!(
        e.to_string().contains("no layer `OUTLINE`") && e.to_string().contains("`EDGE`"),
        "{e}"
    );
}

#[test]
fn a_shape_on_the_holes_layer_that_is_not_a_circle_is_refused() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 20.0, 10.0)
        .rect("HOLES", 2.0, 2.0, 4.0, 3.0);
    let e = read_outline(
        &f.text(),
        &Options {
            holes_layers: vec!["HOLES".into()],
            ..Options::default()
        },
    )
    .unwrap_err();
    assert!(matches!(e, Error::HoleNotCircle { .. }), "{e:?}");
}

#[test]
fn a_hole_drawn_as_a_two_vertex_polyline_is_a_circle() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 20.0, 10.0);
    f.add(lwpolyline(
        "HOLES",
        true,
        &[(4.0, 5.0, 1.0), (6.0, 5.0, 1.0)],
    ));
    let o = read_outline(
        &f.text(),
        &Options {
            holes_layers: vec!["HOLES".into()],
            ..Options::default()
        },
    )
    .unwrap();
    assert!(close(o.holes[0].diameter, 2.0) && close(o.holes[0].centre.0, 5.0));
    assert!(close(o.holes[0].centre.1, 5.0));
}

#[test]
fn text_on_the_outline_layer_is_not_read_and_is_said() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 20.0, 10.0);
    f.add(
        pair(0, "TEXT")
            + &pair(8, "EDGE")
            + &pair(10, 1)
            + &pair(20, 1)
            + &pair(40, 2)
            + &pair(1, "TOP"),
    );
    let o = read(&f).unwrap();
    assert!(
        o.warnings
            .iter()
            .any(|w| w.contains("1 TEXT on layer `EDGE`")),
        "{:?}",
        o.warnings
    );
}

#[test]
fn paper_space_entities_are_ignored() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 20.0, 10.0);
    f.add(line("EDGE", (500.0, 500.0), (600.0, 600.0)) + &pair(67, 1));
    assert!(read(&f).is_ok());
}

#[test]
fn a_line_drawn_twice_is_counted_once() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 20.0, 10.0)
        .line("EDGE", (20.0, 10.0), (20.0, 0.0));
    let o = read(&f).unwrap();
    assert_eq!(o.outer.vertices.len(), 4);
    assert!(o.warnings.iter().any(|w| w.contains("drawn twice")));
}

#[test]
fn a_spline_is_approximated_within_the_tolerance() {
    // A quadratic Bezier from (10, 0) to (0, 10) with the control point (10, 10), closed by
    // the two axes: the area is that under the curve.
    let mut f = File::mm();
    f.line("EDGE", (0.0, 10.0), (0.0, 0.0))
        .line("EDGE", (0.0, 0.0), (10.0, 0.0));
    let spline = pair(0, "SPLINE")
        + &pair(8, "EDGE")
        + &pair(70, 8)
        + &pair(71, 2)
        + &pair(72, 6)
        + &pair(73, 3)
        + &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]
            .iter()
            .map(|k| pair(40, k))
            .collect::<String>()
        + &[(10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
            .iter()
            .map(|(x, y)| pair(10, x) + &pair(20, y) + &pair(30, 0))
            .collect::<String>();
    f.add(spline);
    let o = read(&f).unwrap();
    // Area: the Bezier x(t) = 10 - 10t^2 (t^2 term: 10(1-t)^2 + 20 t(1-t) + 0 = 10 - 10t^2),
    // y(t) = 20 t - 10 t^2. Area under it by Green's theorem, numerically.
    let n = 20_000;
    let mut area = 0.0;
    let pt = |t: f64| (10.0 - 10.0 * t * t, 20.0 * t - 10.0 * t * t);
    for i in 0..n {
        let (a, b) = (pt(i as f64 / n as f64), pt((i + 1) as f64 / n as f64));
        area += (a.0 * b.1 - b.0 * a.1) / 2.0;
    }
    // Close along the axes: (0, 10) -> (0, 0) -> (10, 0) adds nothing from the origin.
    assert!(
        (o.outer.area() - area.abs()).abs() < 0.02,
        "{} vs {area}",
        o.outer.area()
    );
    // The vertices lie on the curve to within the tolerance.
    for v in &o.outer.vertices {
        if v.x > 1e-9 && v.y < 10.0 - 1e-9 && v.x < 10.0 - 1e-9 && v.y > 1e-9 {
            // y is down on the board: back to the drawing's.
            let (x, y) = (v.x, 10.0 - v.y);
            let best = (0..=20_000)
                .map(|i| {
                    let p = pt(f64::from(i) / 20_000.0);
                    (p.0 - x).hypot(p.1 - y)
                })
                .fold(f64::MAX, f64::min);
            assert!(best < 1e-3, "{x} {y} {best}");
        }
    }
    assert!(o.outer.vertices.len() > 8);
}

#[test]
fn an_ellipse_cutout_is_approximated_within_the_tolerance() {
    let mut f = File::mm();
    f.rect("EDGE", 0.0, 0.0, 40.0, 30.0);
    // Centre (20, 15), major axis 5 along x, ratio 0.6: semi-axes 5 and 3.
    f.add(
        pair(0, "ELLIPSE")
            + &pair(8, "EDGE")
            + &pair(10, 20)
            + &pair(20, 15)
            + &pair(30, 0)
            + &pair(11, 5)
            + &pair(21, 0)
            + &pair(31, 0)
            + &pair(40, 0.6)
            + &pair(41, 0)
            + &pair(42, std::f64::consts::TAU),
    );
    let o = read(&f).unwrap();
    assert_eq!(o.cutouts.len(), 1);
    let cutout = &o.cutouts[0];
    assert!(cutout.as_rounded_rect().is_none());
    let exact = std::f64::consts::PI * 5.0 * 3.0;
    // A polygon inscribed with sagitta <= 0.005 loses less than that times the perimeter.
    assert!(
        cutout.area() <= exact && exact - cutout.area() < 0.005 * 25.0,
        "{}",
        cutout.area()
    );
    let (lo, hi) = cutout.bounds();
    assert!((hi.0 - lo.0 - 10.0).abs() < 0.01 && (hi.1 - lo.1 - 6.0).abs() < 0.01);
}

#[test]
fn the_edge_may_be_on_several_layers() {
    let mut f = File::mm();
    f.line("EDGE", (0.0, 0.0), (20.0, 0.0))
        .line("EDGE", (20.0, 0.0), (20.0, 10.0));
    f.line("EDGE2", (20.0, 10.0), (0.0, 10.0))
        .line("EDGE2", (0.0, 10.0), (0.0, 0.0));
    let o = read_outline(
        &f.text(),
        &Options {
            outline_layers: vec!["EDGE".into(), "EDGE2".into()],
            ..Options::default()
        },
    )
    .unwrap();
    assert!(close(o.outer.area(), 200.0));
}

#[test]
fn the_outer_loop_runs_clockwise_on_the_screen_however_it_was_drawn() {
    // The same rectangle drawn both ways round.
    let ccw = [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (0.0, 10.0)];
    let cw = [(0.0, 0.0), (0.0, 10.0), (20.0, 10.0), (20.0, 0.0)];
    for pts in [ccw, cw] {
        let mut f = File::mm();
        f.polygon("EDGE", &pts);
        let o = read(&f).unwrap();
        // Clockwise with y down: from the top left to the top right first.
        let v = &o.outer.vertices;
        let n = v.len();
        let twice: f64 = (0..n)
            .map(|i| v[i].x * v[(i + 1) % n].y - v[(i + 1) % n].x * v[i].y)
            .sum();
        assert!(twice > 0.0, "{pts:?}");
    }
}
