//! A logo's vector file as outlines (`read_logo`): small SVG and DXF files written here by hand.

use netsmith_dxf::read_logo;

fn svg(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<!-- a comment with <path d=\"M0 0\"/> in it -->\n<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 100 100\">\n{body}\n</svg>\n"
    )
}

/// The box of the outlines: its width and height.
fn extent(outlines: &[Vec<(f64, f64)>]) -> (f64, f64) {
    let pts: Vec<(f64, f64)> = outlines.iter().flatten().copied().collect();
    let (min_x, max_x) = pts
        .iter()
        .fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
    let (min_y, max_y) = pts
        .iter()
        .fold((f64::MAX, f64::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
    (max_x - min_x, max_y - min_y)
}

fn area(ring: &[(f64, f64)]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            (a.0 * b.1 - b.0 * a.1) / 2.0
        })
        .sum::<f64>()
        .abs()
}

#[test]
fn a_rectangle_is_scaled_so_that_its_longer_side_is_the_size_and_is_centred() {
    let text = svg("<rect x=\"10\" y=\"20\" width=\"40\" height=\"20\"/>");
    let outlines = read_logo("mark.svg", &text, 8.0).unwrap();
    assert_eq!(outlines.len(), 1);
    let (w, h) = extent(&outlines);
    assert!((w - 8.0).abs() < 1e-9 && (h - 4.0).abs() < 1e-9, "{w} x {h}");
    let xs: Vec<f64> = outlines[0].iter().map(|p| p.0).collect();
    assert!(xs.iter().all(|x| x.abs() <= 4.0 + 1e-9), "{xs:?}");
    assert!(
        (xs.iter().sum::<f64>()).abs() < 1e-9,
        "centred on the origin"
    );
}

#[test]
fn a_path_with_curves_and_an_arc_is_cut_into_straight_pieces_on_the_curve() {
    // A circle of radius 10 as two arcs, and a quadratic and a cubic bump on a square.
    let text = svg("<path d=\"M 10 0 A 10 10 0 1 1 -10 0 A 10 10 0 1 1 10 0 Z\"/>");
    let outlines = read_logo("round.svg", &text, 20.0).unwrap();
    assert_eq!(outlines.len(), 1);
    assert!(outlines[0].len() > 30, "{}", outlines[0].len());
    for (x, y) in &outlines[0] {
        assert!((x.hypot(*y) - 10.0).abs() < 1e-6, "({x}, {y})");
    }
    let bumps = svg("<path d=\"M0 0 Q 5 -5 10 0 C 12 4, 12 6, 10 10 S 2 14 0 10 z\"/>");
    let outlines = read_logo("bumps.svg", &bumps, 10.0).unwrap();
    assert!(outlines[0].len() > 90, "{}", outlines[0].len());
}

#[test]
fn relative_commands_and_compact_numbers_are_read() {
    let text = svg("<path d=\"m10,10h20v20h-20z\"/><polygon points=\"0,0 5,0 5,5\"/>");
    let outlines = read_logo("rel.svg", &text, 1.0).unwrap();
    assert_eq!(outlines.len(), 2);
    assert_eq!(outlines[0].len(), 4);
    assert_eq!(outlines[1].len(), 3);
}

#[test]
fn a_shape_inside_another_is_a_hole_joined_by_a_cut_of_no_width() {
    let text = svg(
        "<path d=\"M0 0H100V100H0Z M25 25H75V75H25Z\"/><circle cx=\"50\" cy=\"50\" r=\"10\"/>",
    );
    let outlines = read_logo("ring.svg", &text, 100.0).unwrap();
    // The square with its hole is one polygon, and the circle in the hole is another.
    assert_eq!(outlines.len(), 2, "{outlines:?}");
    let keyhole = outlines.iter().find(|o| o.len() < 20).unwrap();
    // The 4 + 4 corners of the two squares, and the cut's two ends again.
    assert_eq!(keyhole.len(), 4 + 4 + 2, "{keyhole:?}");
    let want = 100.0 * 100.0 - 50.0 * 50.0;
    assert!((area(keyhole) - want).abs() < 1e-6, "{}", area(keyhole));
}

#[test]
fn an_inkscape_file_with_its_metadata_and_a_defs_block_is_read() {
    let text = svg(
        "<sodipodi:namedview id=\"base\" inkscape:zoom=\"2\"/><defs><path id=\"x\" d=\"M0 0H1V1Z\" transform=\"scale(2)\"/></defs><g id=\"layer1\"><path d=\"M0 0H10V10H0Z\" style=\"fill:#000000;stroke:none\"/><rect width=\"5\" height=\"5\" style=\"display:none\"/></g>",
    );
    let outlines = read_logo("ink.svg", &text, 2.0).unwrap();
    assert_eq!(outlines.len(), 1);
}

fn refused(name: &str, text: &str) -> String {
    read_logo(name, text, 5.0).unwrap_err()
}

#[test]
fn what_cannot_be_drawn_is_refused_naming_the_file_the_element_and_the_line() {
    let e = refused(
        "mark.svg",
        &svg("<g transform=\"translate(5 5)\"><rect width=\"4\" height=\"4\"/></g>"),
    );
    assert!(e.starts_with("`mark.svg`: `<g>` at line 4 has a `transform`"), "{e}");

    let e = refused(
        "mark.svg",
        &svg("<path d=\"M0 0H5V5Z\" fill=\"none\" stroke=\"black\"/>"),
    );
    assert!(e.contains("`<path>` at line 4 has no fill"), "{e}");
    assert!(e.contains("Stroke to Path"), "{e}");

    let e = refused("mark.svg", &svg("<text x=\"1\">hi</text>"));
    assert!(e.contains("`<text>` at line 4") && e.contains("convert the text to paths"), "{e}");

    let e = refused("mark.svg", &svg("<image href=\"a.png\"/>"));
    assert!(e.contains("raster image"), "{e}");

    let e = refused("mark.svg", &svg("<path d=\"M0 0 L 5\"/>"));
    assert!(e.contains("`<path>` at line 4"), "{e}");

    let e = refused("mark.svg", &svg("<path d=\"L0 0\"/>"));
    assert!(e.contains("must start with `M`"), "{e}");

    let e = refused("mark.svg", "<html></html>");
    assert!(e.contains("no `<svg>` element"), "{e}");

    let e = refused("mark.svg", &svg("<rect width=\"0\" height=\"0\"/>"));
    assert!(e.contains("no closed shape"), "{e}");
}

#[test]
fn a_raster_image_or_an_unknown_extension_is_refused() {
    let e = refused("logo.png", "\u{89}PNG");
    assert!(e.contains("`logo.png` is neither an SVG nor a DXF file"), "{e}");
    assert!(e.contains("raster image is not read"), "{e}");
}

type Line = ((f64, f64), (f64, f64));

fn dxf_lines(lines: &[Line]) -> String {
    let mut s = String::from("0\nSECTION\n2\nENTITIES\n");
    for ((x1, y1), (x2, y2)) in lines {
        s += &format!("0\nLINE\n8\n0\n10\n{x1}\n20\n{y1}\n30\n0\n11\n{x2}\n21\n{y2}\n31\n0\n");
    }
    s + "0\nENDSEC\n0\nEOF\n"
}

#[test]
fn a_dxf_square_of_lines_is_one_outline_with_y_turned_over() {
    let square = dxf_lines(&[
        ((0.0, 0.0), (20.0, 0.0)),
        ((20.0, 0.0), (20.0, 10.0)),
        ((20.0, 10.0), (0.0, 10.0)),
        ((0.0, 10.0), (0.0, 0.0)),
    ]);
    let outlines = read_logo("mark.dxf", &square, 4.0).unwrap();
    assert_eq!(outlines.len(), 1);
    let (w, h) = extent(&outlines);
    assert!((w - 4.0).abs() < 1e-9 && (h - 2.0).abs() < 1e-9, "{w} x {h}");
    assert!((area(&outlines[0]) - 8.0).abs() < 1e-9);
}

#[test]
fn a_dxf_with_a_gap_in_a_loop_is_refused_naming_the_file() {
    let open = dxf_lines(&[
        ((0.0, 0.0), (20.0, 0.0)),
        ((20.0, 0.0), (20.0, 10.0)),
        ((20.0, 10.0), (0.0, 10.0)),
    ]);
    let e = refused("mark.dxf", &open);
    assert!(e.starts_with("`mark.dxf`: "), "{e}");
}
