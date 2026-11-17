//! The 2D picture of a board's copper (`netsmith view pcb`, issue #1086): a small board placed
//! and routed by hand, so that neither the placer nor the router moves its pictures, drawn whole,
//! as a region, and as its bottom layer seen from below, with one connection left open.
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
fn board() -> Board {
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
    [
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
    .collect()
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
