//! The 2D picture of a board's copper (`netsmith view pcb`, issue #1086): a small board placed
//! and routed by hand, so that neither the placer nor the router moves its pictures, drawn whole,
//! as a region, and as its bottom layer seen from below, with one connection left open; and the
//! same board routed so that the fill of its bottom pour starves a pad and takes islands out.
//!
//! The pictures' bytes are held by their hashes in `fixtures/pcb_picture/golden.txt`.
//! `NETSMITH_PCB_PICTURE_OUT=<folder>` writes each picture there to look at, and
//! `NETSMITH_PCB_PICTURE_BLESS=1 cargo test -p netsmith-fabricate --test main pcb_picture::`
//! rewrites the file from what the code draws now: look at the pictures before committing it.

use std::path::Path;

use netsmith_fabricate::pcb_picture::{Drawn, Format, Request, draw};
use netsmith_pcb::{Board, CopperLayer, PictureLayers, Pt, Rect, Track, Via};
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;

const SOURCE: &str = r#"import std::parts::connector::Header1x2
import std::parts::passive::{Resistor, Led}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Picture {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .bottom.net &= "GND"
    }
    .supply: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VCC"
    }
    .led: Led { .current &= 5mA..15mA }
    .r1: Resistor {}
    .power_in: Header1x2
    .supply.pos ~ .power_in.p1
    .supply.neg ~ .power_in.p2
    .supply.pos ~> .r1 ~> .led ~> .supply.neg
}
"#;

/// The centre of the pad `number` of the part `refdes`.
fn pad(board: &Board, refdes: &str, number: &str) -> Pt {
    let part = board.part(refdes).unwrap_or_else(|| panic!("no {refdes}"));
    let pad = part
        .footprint
        .pads_numbered(number)
        .next()
        .unwrap_or_else(|| panic!("no pad {refdes}.{number}"));
    part.pad_centre(pad)
}

/// The board 20 by 12 mm with the header on the left, the resistor in the middle and the LED on
/// the right, all on the top; VCC routed from the header on the bottom to a via and on the top
/// to the resistor, the anode on the top, and GND left open.
pub(crate) fn board() -> Board {
    let libraries = Libraries::sample();
    let built = netsmith_pipeline::build_with(
        "picture.nsm",
        SOURCE,
        &netsmith_check::Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut board = netsmith_placer::board_from_design(
        &built[0].design,
        &libraries.mapping,
        &libraries.footprints,
    )
    .unwrap();
    let min = board.outline.min;
    board.outline = Rect::new(min, min + Pt::new(20.0, 12.0));
    let at = |x: f64, y: f64| min + Pt::new(x, y);
    // The designators the design stage gives: the header, the resistor and the LED.
    let names: Vec<String> = board.parts.iter().map(|p| p.refdes.clone()).collect();
    assert_eq!(names.len(), 3, "{names:?}");
    for p in &mut board.parts {
        p.at = match p.refdes.chars().next() {
            Some('J') => at(3.5, 6.0),
            Some('R') => at(10.0, 3.5),
            _ => at(15.5, 7.0),
        };
        p.rotation = 0.0;
    }
    let (j, r, d) = (refdes(&board, 'J'), refdes(&board, 'R'), refdes(&board, 'D'));
    let net_of = |board: &Board, refdes: &str, number: &str| -> String {
        board
            .nets
            .iter()
            .find(|n| {
                n.pads
                    .iter()
                    .any(|p| p.refdes == refdes && p.number == number)
            })
            .map(|n| n.name.clone())
            .unwrap_or_else(|| panic!("{refdes}.{number} is on no net"))
    };
    let vcc = net_of(&board, &j, "1");
    assert_eq!(vcc, "VCC");
    let r_vcc = if net_of(&board, &r, "1") == vcc { "1" } else { "2" };
    let r_led = if r_vcc == "1" { "2" } else { "1" };
    let anode = net_of(&board, &r, r_led);
    let d_anode = if net_of(&board, &d, "1") == anode { "1" } else { "2" };
    let via = at(7.0, 9.5);
    let corner = Pt::new(pad(&board, &r, r_vcc).x, via.y);
    board.tracks = vec![
        Track::straight(&vcc, CopperLayer::Back, pad(&board, &j, "1"), via, 0.3),
        Track::straight(&vcc, CopperLayer::Front, via, corner, 0.3),
        Track::straight(&vcc, CopperLayer::Front, corner, pad(&board, &r, r_vcc), 0.3),
        Track::straight(
            &anode,
            CopperLayer::Front,
            pad(&board, &r, r_led),
            pad(&board, &d, d_anode),
            0.25,
        ),
    ];
    board.vias = vec![Via {
        net: vcc,
        at: via,
        diameter: 0.6,
        drill: 0.3,
        span: None,
    }];
    board
}

/// The designator of the one part whose designator starts with `letter`.
fn refdes(board: &Board, letter: char) -> String {
    board
        .parts
        .iter()
        .find(|p| p.refdes.starts_with(letter))
        .map(|p| p.refdes.clone())
        .unwrap_or_else(|| panic!("no part {letter}"))
}

fn request(format: Format) -> Request {
    Request {
        layers: PictureLayers::All,
        from_below: None,
        region: None,
        scale: 10.0,
        courtyards: false,
        designators: false,
        highlight: Vec::new(),
        only_open: false,
        drc: false,
        islands: false,
        stitching: false,
        rule_areas: false,
        format,
    }
}

/// The golden pictures: a name, the request, and the picture.
fn goldens() -> Vec<(&'static str, Request, Drawn)> {
    let board = board();
    let whole = Request {
        courtyards: true,
        designators: true,
        ..request(Format::Png)
    };
    let region = Request {
        region: Some([5.0, 1.0, 9.0, 9.0]),
        scale: 30.0,
        highlight: vec!["net:VCC".to_string()],
        ..request(Format::Png)
    };
    let bottom = Request {
        layers: PictureLayers::One(CopperLayer::Back),
        designators: true,
        ..request(Format::Png)
    };
    let open = Request {
        only_open: true,
        ..request(Format::Png)
    };
    let mut pictures: Vec<(&'static str, Request, Drawn)> = [
        ("whole", whole),
        ("region", region),
        ("bottom", bottom),
        ("only-open", open),
    ]
    .into_iter()
    .map(|(name, request)| {
        let drawn = draw(&board, &request, None).unwrap_or_else(|e| panic!("{name}: {e}"));
        (name, request, drawn)
    })
    .collect();
    // The bottom pour with its thermal reliefs, the islands its fill took out and the pad it
    // starves (issue #1188).
    let fill = fill_request();
    let drawn = draw(&fill_board(), &fill, None).unwrap_or_else(|e| panic!("fill: {e}"));
    pictures.push(("fill", fill, drawn));
    pictures
}

/// FNV-1a, 64 bits.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn the_golden_pictures_have_not_changed() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pcb_picture/golden.txt");
    let pictures = goldens();
    if let Some(dir) = std::env::var_os("NETSMITH_PCB_PICTURE_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        for (name, _, drawn) in &pictures {
            std::fs::write(Path::new(&dir).join(format!("{name}.png")), &drawn.bytes).unwrap();
        }
    }
    let now: Vec<String> = pictures
        .iter()
        .map(|(name, _, drawn)| format!("{name} {:016x}", fnv(&drawn.bytes)))
        .collect();
    if std::env::var_os("NETSMITH_PCB_PICTURE_BLESS").is_some() {
        let mut text = String::from(
            "# FNV-1a 64 of the PNG bytes of each golden picture of tests/pcb_picture.rs.\n\
             # Rewrite with NETSMITH_PCB_PICTURE_BLESS=1 cargo test -p netsmith-fabricate --test main pcb_picture::\n\
             # and look at the pictures first (NETSMITH_PCB_PICTURE_OUT=<folder>).\n",
        );
        for line in &now {
            text.push_str(line);
            text.push('\n');
        }
        std::fs::write(&path, text).unwrap();
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap();
    let want: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(
        now,
        want,
        "a golden picture changed: look at them (NETSMITH_PCB_PICTURE_OUT=<folder>), then rewrite \
         {} with NETSMITH_PCB_PICTURE_BLESS=1",
        path.display()
    );
}

#[test]
fn the_one_open_connection_is_listed_and_drawn_where_its_pads_are_mirrored_from_below() {
    let board = board();
    let (j, d) = (refdes(&board, 'J'), refdes(&board, 'D'));
    let drawn = draw(&board, &request(Format::Svg), None).unwrap();
    assert_eq!(drawn.open.len(), 1, "{:?}", drawn.open);
    assert!(drawn.open[0].starts_with("net GND: "), "{:?}", drawn.open);
    assert!(
        drawn.open[0].contains(&format!("{j}.2")) && drawn.open[0].contains(&d),
        "{:?}",
        drawn.open
    );
    // The middle of the open connection, in the picture's pixels, from above and from below.
    let middle = |below: bool| -> (f64, f64) {
        let svg = String::from_utf8(
            draw(
                &board,
                &Request {
                    from_below: Some(below),
                    ..request(Format::Svg)
                },
                None,
            )
            .unwrap()
            .bytes,
        )
        .unwrap();
        let picture = netsmith_draw::svg::read(&svg).unwrap();
        let open = svg
            .split("<g class=\"open\">")
            .nth(1)
            .and_then(|s| s.split("<line").nth(1))
            .expect("an open connection is drawn");
        let attr = |name: &str| -> f64 {
            let at = open.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
            open[at..].split('"').next().unwrap().parse().unwrap()
        };
        let (x, y) = (
            (attr("x1") + attr("x2")) / 2.0,
            (attr("y1") + attr("y2")) / 2.0,
        );
        let image =
            netsmith_draw::raster::render(&picture, picture.view, 10.0, netsmith_draw::Rgba::BLACK);
        let (px, py) = (
            ((x - picture.view.min.x) * 10.0) as usize,
            ((y - picture.view.min.y) * 10.0) as usize,
        );
        let at = (py * image.width + px) * 4;
        assert_eq!(
            &image.rgba[at..at + 3],
            &[0xff, 0xd8, 0x4d],
            "the open connection is bright where it is drawn"
        );
        (x, y)
    };
    let (above, below) = (middle(false), middle(true));
    let o = board.outline;
    assert!(
        (above.0 + below.0 - (o.min.x + o.max.x)).abs() < 1e-6 && above.1 == below.1,
        "from below the board is mirrored about its middle: {above:?} {below:?}"
    );
}

#[test]
fn a_region_is_cut_at_its_scale_and_the_bottom_layer_draws_no_top_copper() {
    let board = board();
    let region = draw(
        &board,
        &Request {
            region: Some([5.0, 1.0, 9.0, 9.0]),
            scale: 30.0,
            ..request(Format::Png)
        },
        None,
    )
    .unwrap();
    assert_eq!((region.width, region.height), (270, 270));
    let bottom = String::from_utf8(
        draw(
            &board,
            &Request {
                layers: PictureLayers::One(CopperLayer::Back),
                ..request(Format::Svg)
            },
            None,
        )
        .unwrap()
        .bytes,
    )
    .unwrap();
    assert!(bottom.contains(r#"data-layer="B.Cu""#), "{bottom}");
    assert!(!bottom.contains(r#"data-layer="F.Cu""#), "{bottom}");
    assert!(bottom.contains("seen from below"), "{bottom}");
    // The ground pour of the bottom is drawn as filled copper.
    assert!(
        bottom.contains(r#"class="pour" data-net="GND""#),
        "{bottom}"
    );
}

#[test]
fn a_highlight_of_an_id_the_board_has_not_is_refused_with_the_ids_it_has() {
    let board = board();
    let err = draw(
        &board,
        &Request {
            highlight: vec!["net:NOPE".into(), "Q9".into()],
            ..request(Format::Svg)
        },
        None,
    )
    .unwrap_err();
    assert!(err.contains("`net:NOPE`, `Q9`"), "{err}");
    assert!(err.contains("net:VCC"), "{err}");
    let err = draw(
        &board,
        &Request {
            layers: PictureLayers::One(CopperLayer::Inner(1)),
            ..request(Format::Svg)
        },
        None,
    )
    .unwrap_err();
    assert!(err.contains("no copper layer inner1"), "{err}");
}

#[test]
fn a_finding_of_the_design_rules_check_is_a_numbered_marker() {
    let mut board = board();
    // A VCC track straight across the LED's pads: copper of two nets that touch.
    let d = refdes(&board, 'D');
    let (a, b) = (pad(&board, &d, "1"), pad(&board, &d, "2"));
    board.tracks.push(Track::straight(
        "VCC",
        CopperLayer::Front,
        a + Pt::new(0.0, -2.0),
        b + Pt::new(0.0, 2.0),
        0.3,
    ));
    let drawn = draw(
        &board,
        &Request {
            drc: true,
            ..request(Format::Svg)
        },
        None,
    )
    .unwrap();
    assert!(!drawn.markers.is_empty());
    assert!(drawn.markers[0].starts_with("1: error ["), "{:?}", drawn.markers);
    assert!(
        drawn.markers.iter().all(|m| !m.contains("unconnected_items")),
        "a missing connection is an open connection, not a marker: {:?}",
        drawn.markers
    );
    let svg = String::from_utf8(drawn.bytes).unwrap();
    assert_eq!(
        svg.matches(r#"<g class="marker">"#).count(),
        drawn.markers.len()
    );
}

#[test]
fn a_module_lights_its_parts_each_module_in_its_own_colour() {
    // The resistor and the LED put in a module `ind`, the header in `io` (issue #1129).
    let mut board = board();
    for p in &mut board.parts {
        p.path = match p.refdes.chars().next() {
            Some('J') => format!("io.{}", p.path),
            _ => format!("ind.{}", p.path),
        };
    }
    let svg = |highlight: &[&str]| -> Result<String, String> {
        let request = Request {
            highlight: highlight.iter().map(|s| s.to_string()).collect(),
            ..request(Format::Svg)
        };
        draw(&board, &request, None).map(|d| String::from_utf8(d.bytes).unwrap())
    };
    let one = svg(&["module:ind"]).unwrap();
    let (first, second) = (
        netsmith_pcb::module_colour(0, true),
        netsmith_pcb::module_colour(1, true),
    );
    assert!(one.contains(&format!(r#"<g data-module="ind" stroke="{first}""#)), "{one}");
    // Two boxes of its parts and the dashed box round them; the header is not in it.
    let group = one.split(r#"data-module="ind""#).nth(1).unwrap();
    let group = &group[..group.find("</g>").unwrap()];
    assert_eq!(group.matches("<rect").count(), 3, "{group}");
    assert_eq!(group.matches("stroke-dasharray").count(), 1, "{group}");

    // Two modules, each in its colour, in the order named.
    let two = svg(&["module:io", "module:ind"]).unwrap();
    assert!(two.contains(&format!(r#"data-module="io" stroke="{first}""#)), "{two}");
    assert!(two.contains(&format!(r#"data-module="ind" stroke="{second}""#)), "{two}");

    // A module the board lacks, and a part's path, are refused with the modules it has.
    let e = svg(&["module:nope"]).unwrap_err();
    assert!(e.contains("`module:nope`") && e.contains("the modules: module:ind, module:io"), "{e}");
    assert!(svg(&["module:io.power_in"]).is_err());
}

/// The board of [`board`] with VCC routed on the bottom round the header's GND pad instead of
/// straight to the via (issue #1188): the track so near the pad on its left and below it that
/// the GND pour of the bottom has no room there, so of the pad's four thermal spokes only the one
/// to the right reaches the pour (a starved pad), and the slivers of pour the track leaves
/// between itself and the pad touch no GND copper (copper islands the fill takes out).
fn fill_board() -> Board {
    let mut board = board();
    let min = board.outline.min;
    let at = |x: f64, y: f64| min + Pt::new(x, y);
    let j = refdes(&board, 'J');
    let start = pad(&board, &j, "1");
    assert!(
        board.tracks[0].layer == CopperLayer::Back,
        "the first track is VCC on the bottom"
    );
    board.tracks.remove(0);
    let path = [
        start,
        at(1.8, 7.7),
        at(1.8, 9.7),
        at(2.3, 10.2),
        at(6.3, 10.2),
        board.vias[0].at,
    ];
    for w in path.windows(2) {
        board
            .tracks
            .push(Track::straight("VCC", CopperLayer::Back, w[0], w[1], 0.3));
    }
    board
}

/// The region of [`fill_board`] round the header, at a scale its reliefs show at.
fn fill_request() -> Request {
    Request {
        region: Some([0.5, 4.5, 8.0, 7.0]),
        scale: 60.0,
        islands: true,
        drc: true,
        ..request(Format::Png)
    }
}

#[test]
fn the_islands_the_fill_took_out_are_hatched_and_the_starved_pad_is_a_marker() {
    let board = fill_board();
    let report = netsmith_pcb::FillReport::of(&board).unwrap();
    let removed: usize = report.fills.iter().map(|f| f.removed_islands.len()).sum();
    assert!(removed >= 1, "{}", report.text());
    assert!(
        report
            .fills
            .iter()
            .flat_map(|f| &f.removed_islands)
            .all(|i| !i.outline.is_empty() && i.outline[0].len() >= 3),
        "a removed island has its outline"
    );
    assert_eq!(report.starved.len(), 1, "{}", report.text());
    let j = refdes(&board, 'J');
    assert_eq!(
        (report.starved[0].part.as_str(), report.starved[0].pad.as_str()),
        (j.as_str(), "2")
    );

    let drawn = draw(
        &board,
        &Request {
            format: Format::Svg,
            ..fill_request()
        },
        None,
    )
    .unwrap();
    assert_eq!(drawn.islands.len(), removed, "{:?}", drawn.islands);
    assert!(
        drawn.islands[0].starts_with("I1: bottom GND at ("),
        "{:?}",
        drawn.islands
    );
    let svg = String::from_utf8(drawn.bytes).unwrap();
    assert_eq!(
        svg.matches(r#"<g class="island" data-layer="B.Cu">"#).count(),
        removed
    );
    // The starved pad is the check's `starved_thermal` finding, marked where the pad is.
    let starved = drawn
        .markers
        .iter()
        .find(|m| m.contains("[starved_thermal]"))
        .unwrap_or_else(|| panic!("no marker of the starved pad: {:?}", drawn.markers));
    assert!(starved.contains(&format!("of {j}")), "{starved}");
    // The pour is drawn as its fill: the clearances and the thermal relief round the GND pad are
    // a hole in it.
    let pour = svg
        .split(r#"<path class="pour" data-net="GND" d=""#)
        .nth(1)
        .expect("the GND pour is drawn");
    let d = pour.split('"').next().unwrap();
    assert!(d.matches('M').count() >= 2, "the fill has a hole: {d}");

    // Without the flag, nothing of it is drawn; the top layer alone has none of the bottom's.
    for request in [
        Request {
            islands: false,
            format: Format::Svg,
            ..fill_request()
        },
        Request {
            layers: PictureLayers::One(CopperLayer::Front),
            format: Format::Svg,
            ..fill_request()
        },
    ] {
        let drawn = draw(&board, &request, None).unwrap();
        let svg = String::from_utf8(drawn.bytes).unwrap();
        assert!(!svg.contains(r#"class="island""#));
        assert!(drawn.islands.is_empty());
    }
}

/// The board of [`board`] with a ground pour on the top stitched every 3 mm to the ground pour
/// of the bottom (issue #1489).
fn stitched_board() -> Board {
    let mut board = board();
    board.pours.push(netsmith_pcb::BoardPour {
        layer: CopperLayer::Front,
        net: "GND".into(),
        stitch_pitch: Some(3.0),
        thermal_gap: None,
        spoke_width: None,
        clearance: None,
        min_width: None,
        pads: None,
    });
    assert!(
        board
            .pours
            .iter()
            .any(|p| p.net == "GND" && p.layer == CopperLayer::Back),
        "the bottom is poured with GND"
    );
    board
}

/// The stitching vias a picture draws: those of its SVG in the stitching tone.
fn stitching_drawn(board: &Board) -> (usize, usize) {
    let drawn = draw(
        board,
        &Request {
            stitching: true,
            ..request(Format::Svg)
        },
        None,
    )
    .unwrap();
    let svg = String::from_utf8(drawn.bytes).unwrap();
    let toned = svg
        .matches(r#"<circle class="stitching" data-net="GND""#)
        .count();
    assert!(
        svg.contains(netsmith_pcb::PICTURE_STITCHING),
        "the tone is the stitching's"
    );
    (drawn.stitching, toned)
}

#[test]
fn the_stitching_drawn_is_the_stitching_the_router_lays_as_fab_routes() {
    // Issue #1489: `view pcb --stitching` draws the grid `fab` will lay, counted the same before
    // the board is routed and after, and none without the flag.
    let board = stitched_board();
    let (said, toned) = stitching_drawn(&board);
    assert!(said > 0, "the top pour is stitched");
    assert_eq!(toned, said, "every via said is drawn in the stitching tone");
    let plain = String::from_utf8(draw(&board, &request(Format::Svg), None).unwrap().bytes).unwrap();
    assert!(!plain.contains(r#"class="stitching""#));

    // The board as `fab` routes it: the same stitching on it, and the picture of it names the
    // same vias, now the router's.
    let mut routed = board.clone();
    netsmith_router::route(&mut routed);
    let laid = netsmith_router::stitching_vias(&routed);
    let before: Vec<Pt> = netsmith_router::stitching_vias(&board)
        .iter()
        .map(|v| v.at)
        .collect();
    let after: Vec<Pt> = laid.iter().map(|v| v.at).collect();
    assert_eq!(after.len(), said, "{before:?}\n{after:?}");
    assert!(
        laid.iter().all(|v| routed.vias.contains(v)),
        "they are the router's vias, not laid again"
    );
    assert_eq!(stitching_drawn(&routed), (said, said));
}

/// A board whose tracks are mitered (its `.corner`, issue #1508) is drawn with each arc as its
/// chamfer, a straight track from the arc's start to its end; a rounded board draws the arc.
#[test]
fn a_mitered_board_draws_an_arc_as_its_chamfer() {
    let mut board = board();
    let (from, to) = (board.tracks[1].from, board.tracks[1].to);
    let half = Pt::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    let length = dx.hypot(dy);
    let chord = board.clone();
    board.tracks[1].mid = Some(Pt::new(
        half.x - dy / length * 0.4,
        half.y + dx / length * 0.4,
    ));
    let svg = |b: &Board| draw(b, &request(Format::Svg), None).unwrap().bytes;
    assert_ne!(svg(&board), svg(&chord), "a rounded board draws its arc");
    board.corner = netsmith_core::TrackCorner::Mitered;
    let mut chord = chord;
    chord.corner = netsmith_core::TrackCorner::Mitered;
    assert_eq!(svg(&board), svg(&chord), "a mitered board draws the chamfer");
}

/// The board of [`board`] with a keep-out zone in the footprint of its resistor, as a library
/// has it (forbidding vias and pours on the top, in the footprint's frame), and a keepout of the
/// physical file on the top (issue #1543).
fn rule_area_board() -> (Board, String) {
    let mut board = board();
    let r = refdes(&board, 'R');
    let part = board.parts.iter_mut().find(|p| p.refdes == r).unwrap();
    let zone = r#"(zone (net 0) (net_name "") (layers "F.Cu") (name "antenna") (hatch full 0.508)
        (connect_pads (clearance 0)) (min_thickness 0.254)
        (keepout (tracks allowed) (vias not_allowed) (pads allowed)
            (copperpour not_allowed) (footprints allowed))
        (fill (thermal_gap 0.5) (thermal_bridge_width 0.5))
        (polygon (pts (xy -1 -2) (xy 3 -2) (xy 3 1) (xy -1 1))))"#;
    let netsmith_kicad::Sexp::List(fields) = &mut part.footprint.source else {
        panic!("a footprint is a list");
    };
    fields.push(netsmith_kicad::Sexp::parse(zone).unwrap());
    board.keepouts.push(netsmith_pcb::BoardKeepout {
        name: "lid".into(),
        part: None,
        at: Some(Pt::new(1.0, 1.0)),
        size: (2.0, 2.0),
        layers: vec!["top".into()],
        tracks: true,
        vias: false,
        copper: false,
        parts: false,
        max_height: None,
        on_carrier_face: false,
        polygon: None,
    });
    (board, r)
}

#[test]
fn the_rule_areas_of_the_footprints_are_hatched_with_a_tip_naming_what_they_forbid() {
    // Issue #1543: the rule areas of `board rules` are drawn on the layers they are on, the
    // footprint's at its part, with a tip and a line naming the kinds; the physical file's
    // keepout is drawn as a keepout, not again as a rule area; with no rule areas asked, none.
    let (board, r) = rule_area_board();
    let rules = netsmith_fabricate::drc::rules_of_board(&board).unwrap();
    let area = rules
        .rule_areas
        .iter()
        .find(|a| a.part.as_deref() == Some(r.as_str()))
        .expect("the footprint's area is a rule area");
    let drawn = draw(
        &board,
        &Request {
            rule_areas: true,
            ..request(Format::Svg)
        },
        None,
    )
    .unwrap();
    let svg = String::from_utf8(drawn.bytes).unwrap();
    assert_eq!(
        svg.matches(r#"<g class="rule-area""#).count(),
        1,
        "only the footprint's: {svg}"
    );
    assert!(
        svg.contains(&format!(
            r#"data-rule-area="antenna" data-part="{r}" data-layers="F.Cu" data-forbids="vias copperpour""#
        )),
        "{svg}"
    );
    let said = format!("antenna of {r} forbids vias, copperpour on F.Cu");
    assert!(svg.contains(&format!("<title>{said}</title>")), "{svg}");
    assert_eq!(drawn.rule_areas, [said]);
    assert!(svg.contains(r#"data-keepout="lid""#), "the keepout is drawn as one");
    // The outline is where the export writes it: the first corner of the area's outline.
    let [x, y] = area.outlines[0][0];
    let at = netsmith_kicad::fmt_num;
    assert!(
        svg.contains(&format!("M{} {}", at(x), at(y))),
        "the outline starts at ({x}, {y})"
    );

    // The bottom alone has none of it.
    let bottom = draw(
        &board,
        &Request {
            rule_areas: true,
            layers: PictureLayers::One(CopperLayer::Back),
            ..request(Format::Svg)
        },
        None,
    )
    .unwrap();
    assert!(bottom.rule_areas.is_empty());
    assert!(!String::from_utf8(bottom.bytes).unwrap().contains(r#"class="rule-area""#));
    let plain = String::from_utf8(draw(&board, &request(Format::Svg), None).unwrap().bytes).unwrap();
    assert!(!plain.contains(r#"class="rule-area""#));
}
