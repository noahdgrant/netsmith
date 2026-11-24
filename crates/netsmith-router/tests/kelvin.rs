//! A Kelvin sense connection (issue #1148): a part's `kelvin` statement makes its pin a sense
//! pin, routed from the pad it senses (the shunt's pad on the pin's net) alone. The board below puts the
//! load's pad nearer the sense pin than the shunt is, so the net's shortest tree joins the sense
//! pin to the load and the shunt's current would flow through the sense pin's pad; with the
//! statement, the sense track runs from the shunt's pad and nothing else of the net touches it.
//! The tests count the copper (tracks, vias and the pads it joins) and measure the areas the pours
//! fill, never time.
//!
//! On a net with a pour, a plane or a zone (issue #1174, low-side sensing: the shunt's ground side)
//! the board file carves the pour round the sense copper, so the fill the Gerbers hold covers none
//! of the sense pin's pad or track, while the rest of the net is poured as before.

use crate::guard;

use netsmith_core::Design;
use netsmith_geom::{Contour, DrcExtras, Pen, Polygons, Pt as G};
use netsmith_pcb::{
    Board, BoardZone, CopperLayer, Pt, Rect, SenseCopper, ZoneIslands, ZonePads, kelvin_findings,
    kelvin_senses, kicad_pcb, kicad_pro, sense_carves, sense_copper,
};
use netsmith_router::{RouteOptions, route_with};

const SOURCE: &str = r#"
import std::mfg::jlcpcb::TwoLayer

part Chip {
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
}

/// A current-sense amplifier with only its two inputs, invented for the test.
part Amp {
    pins {
        .in_p input "IN+" "1"
        .in_n input "IN-" "2"
    }
    layout {
        source "Invented datasheet, 9 Layout"
        /// "Connect IN+ and IN- to the shunt's pads with Kelvin connections."
        kelvin sense_p bridging .in_p .in_n to .in_p
        kelvin sense_n bridging .in_p .in_n to .in_n
    }
}

board Sensed {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .amp: Amp
    .shunt: Chip
    .load: Chip
    .feed: Chip
    .shunt.a ~ .amp.in_p
    .shunt.a ~ .load.a
    .shunt.b ~ .amp.in_n
    .shunt.b ~ .feed.a
    .load.b ~ .feed.b
    EXTRA
}
"#;

/// The design of the board, with `extra` statements in it.
fn design(extra: &str) -> Design {
    design_on("TwoLayer", extra)
}

/// The design of the board made with `process` (`TwoLayer`, `FourLayer`), with `extra`
/// statements in it.
fn design_on(process: &str, extra: &str) -> Design {
    let source = SOURCE
        .replace("TwoLayer", process)
        .replace("EXTRA", extra);
    let checked = netsmith_check::check("t.nsm", &source, &netsmith_check::Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = &checked.boards[0];
    let solution = netsmith_solve::solve(board, &netsmith_solve::Catalogs::jlcpcb());
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution.diagnostics
    );
    netsmith_design::design_board(board, &solution)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .design
}

/// The board with its parts where the test puts them: the amplifier in the middle, the shunt
/// 5 mm below it, the load's pad on the sense net 2.2 mm left of `IN+`, the feed to the right.
fn placed(design: &Design) -> Board {
    placed_with(design, "Resistor_SMD:R_0805_2012Metric")
}

/// [`placed`], with the amplifier's pins on the pads of `amp` (a footprint of two pads 1 and 2
/// side by side).
fn placed_with(design: &Design, amp: &str) -> Board {
    let row = |kind: &str, footprint: &str| {
        format!(
            "[[part]]\nkind = \"{kind}\"\npackage = \"0805\"\ndefault = true\nsymbol = \"Device:R\"\nfootprint = \"{footprint}\"\n"
        )
    };
    let mapping = netsmith_kicad::Mapping::parse(
        &format!(
            "{}{}",
            row("Chip", "Resistor_SMD:R_0805_2012Metric"),
            row("Amp", amp)
        ),
        "kelvin",
        std::path::Path::new("."),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let mut board =
        netsmith_placer::board_from_design(design, &mapping, &crate::common::fixtures())
            .unwrap_or_else(|e| panic!("{e}"));
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 20.0));
    board.corner_radius = 0.0;
    board.polygon = None;
    for (path, at, rotation) in [
        ("amp", (10.0, 10.0), 0.0),
        ("shunt", (10.0, 15.0), 0.0),
        ("load", (6.0, 10.0), 180.0),
        ("feed", (14.0, 10.0), 0.0),
    ] {
        let part = board
            .parts
            .iter_mut()
            .find(|p| p.path == path)
            .unwrap_or_else(|| panic!("no part `{path}`"));
        part.at = Pt::new(at.0, at.1);
        part.rotation = rotation;
    }
    board
}

fn route(board: &mut Board) {
    let routing = route_with(
        board,
        RouteOptions {
            time_limit: Some(guard::GUARD),
            ..RouteOptions::default()
        },
    );
    guard::not_hung(&routing);
    assert!(routing.left.is_empty(), "{:?}", routing.left.lines);
}

/// The pads each sense pin's copper joins besides the pad it senses, and whether it reaches it.
fn sense(board: &Board) -> Vec<(Vec<String>, usize, bool)> {
    kelvin_senses(&board.parts, &board.nets)
        .iter()
        .map(|t| {
            let c = sense_copper(board, t).expect("the sensed pad is found");
            let joins = c.joins.iter().map(ToString::to_string).collect();
            (joins, c.tracks.len(), c.reached)
        })
        .collect()
}

#[test]
fn each_sense_pin_is_routed_from_the_shunt_pad_and_touches_no_other_copper_of_its_net() {
    let mut board = placed(&design(""));
    let senses = kelvin_senses(&board.parts, &board.nets);
    assert_eq!(senses.len(), 2, "{senses:?}");
    let shunt = board.parts.iter().position(|p| p.path == "shunt").unwrap();
    assert!(
        senses.iter().all(|t| t.found == vec![shunt]),
        "each statement finds the shunt: {senses:?}"
    );
    route(&mut board);
    for (joins, tracks, reached) in sense(&board) {
        assert!(joins.is_empty(), "the sense copper joins {joins:?}");
        assert!(tracks > 0, "the sense pin has a track");
        assert!(reached, "the sense track reaches the shunt's pad");
    }
    assert!(
        kelvin_findings(&board).is_empty(),
        "{:?}",
        kelvin_findings(&board)
    );
    // The board is put back as it was: the shunt's pads whole, the nets the design's.
    let shunt_pads: Vec<&str> = board.parts[shunt]
        .footprint
        .pads
        .iter()
        .map(|p| p.number.as_str())
        .collect();
    assert_eq!(shunt_pads, ["1", "2"]);
    assert!(
        board.tracks.iter().all(|t| !t.net.contains("~sense~")),
        "every track is on a net of the design"
    );
}

#[test]
fn without_the_statement_the_sense_pin_is_joined_to_the_load_and_the_finding_says_so() {
    let mut board = placed(&design(""));
    let amp = board.parts.iter().position(|p| p.path == "amp").unwrap();
    let layout = std::mem::take(&mut board.parts[amp].layout);
    route(&mut board);
    board.parts[amp].layout = layout;
    let joined: Vec<Vec<String>> = sense(&board).into_iter().map(|s| s.0).collect();
    let load = &board.parts.iter().find(|p| p.path == "load").unwrap().refdes;
    assert!(
        joined.iter().flatten().any(|p| p.starts_with(&format!("{load}."))),
        "the shortest tree runs the load's current through IN+: {joined:?}"
    );
    let findings = kelvin_findings(&board);
    assert!(
        findings
            .iter()
            .any(|f| f.name == "sense_p" && f.message.contains("besides the pad it senses")),
        "{findings:?}"
    );
}

#[test]
fn a_statement_that_finds_two_parts_senses_no_pad_and_says_which() {
    let mut board = placed(&design(
        ".spare: Chip\n    .spare.a ~ .amp.in_p\n    .spare.b ~ .amp.in_n",
    ));
    let spare = board.parts.iter().position(|p| p.path == "spare").unwrap();
    board.parts[spare].at = Pt::new(10.0, 4.0);
    let senses = kelvin_senses(&board.parts, &board.nets);
    assert!(senses.iter().all(|t| t.found.len() == 2 && t.sensed.is_none()));
    let findings = kelvin_findings(&board);
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        findings[0].message.contains("finds 2 parts"),
        "{}",
        findings[0].message
    );
}

/// The ground side of the shunt, which every case below pours: low-side sensing.
const GROUND: &str = ".shunt.b.net &= \"GND\"";

/// The statement for the sense pin of the ground side, `sense_n`, and the copper of its pin.
fn ground_sense(board: &Board) -> SenseCopper {
    let senses = kelvin_senses(&board.parts, &board.nets);
    let amp = board.parts.iter().position(|p| p.path == "amp").unwrap();
    let kelvin = senses
        .iter()
        .find(|t| t.part == amp && t.rule(&board.parts).name == "sense_n")
        .expect("the statement `sense_n`");
    assert_eq!(board.nets[kelvin.net.unwrap()].name, "GND");
    sense_copper(board, kelvin).expect("the sensed pad is found")
}

/// The sense copper on `layer` (its tracks, vias and pin pads), grown by `grow`, as polygons.
fn drawn(board: &Board, copper: &SenseCopper, layer: CopperLayer, grow: f64) -> Polygons {
    let g = |p: Pt| G { x: p.x, y: p.y };
    let mut pen = Polygons::new();
    pen.polarity(true);
    for &i in &copper.tracks {
        let t = &board.tracks[i];
        if t.layer == layer {
            pen.line(t.width + 2.0 * grow, None, g(t.from), g(t.to), &[]);
        }
    }
    for &i in &copper.vias {
        let v = &board.vias[i];
        if v.on(layer, board.copper_count()) {
            pen.region(Contour::circle(g(v.at), v.diameter / 2.0 + grow), None, &[]);
        }
    }
    // The teardrops where the sense tracks meet a pad.
    for drop in board.teardrops.iter().filter(|d| d.layer == layer) {
        let on_track = copper.tracks.iter().any(|&i| {
            let t = &board.tracks[i];
            t.layer == layer && drop.points.iter().any(|&p| t.distance_to(p) <= t.width / 2.0 + 1e-6)
        });
        if on_track {
            let ring: Vec<G> = drop.points.iter().map(|&p| g(p)).collect();
            pen.region(Contour::polygon(&ring), None, &[]);
            if grow > 0.0 {
                for k in 0..ring.len() {
                    pen.line(2.0 * grow, None, ring[k], ring[(k + 1) % ring.len()], &[]);
                }
            }
        }
    }
    for p in &copper.pads {
        let part = board.part(&p.refdes).unwrap();
        for pad in part.footprint.pads_numbered(&p.number) {
            let (front, back) = part.pad_layers(pad);
            let on = match layer {
                CopperLayer::Front => front,
                CopperLayer::Back => back,
                CopperLayer::Inner(_) => front && back,
            };
            if !on {
                continue;
            }
            // The pad's box (its pads are rectangles on the board's axes), and a round pen
            // `grow` wide on each side round it.
            let corners: Vec<G> = part.pad_bounds(pad).corners().iter().map(|&c| g(c)).collect();
            pen.region(Contour::polygon(&corners), None, &[]);
            if grow > 0.0 {
                for k in 0..4 {
                    pen.line(2.0 * grow, None, corners[k], corners[(k + 1) % 4], &[]);
                }
            }
        }
    }
    pen
}

/// The area in mm² that the fill of net `GND` on `layer` (the Gerbers' fill:
/// `netsmith_pcb::pours_of`) shares with the sense copper on it grown by `grow`.
fn poured_over(board: &Board, copper: &SenseCopper, layer: CopperLayer, grow: f64) -> f64 {
    let g = |p: Pt| G { x: p.x, y: p.y };
    let pours = netsmith_pcb::pours_of(board).unwrap_or_else(|e| panic!("{e:?}"));
    let fill: Vec<_> = pours
        .iter()
        .filter(|l| l.layer == layer.name())
        .flat_map(|l| &l.pours)
        .filter(|p| p.net.as_deref() == Some("GND"))
        .collect();
    assert!(!fill.is_empty(), "net GND is poured on {}", layer.name());
    let mut poured = Polygons::new();
    poured.polarity(true);
    for p in &fill {
        let outer: Vec<G> = p.area.outer.iter().map(|&q| g(q)).collect();
        poured.region(Contour::polygon(&outer), None, &[]);
        poured.polarity(false);
        for h in &p.area.holes {
            let hole: Vec<G> = h.iter().map(|&q| g(q)).collect();
            poured.region(Contour::polygon(&hole), None, &[]);
        }
        poured.polarity(true);
    }
    shared(&poured, &drawn(board, copper, layer, grow))
}

/// The area in mm² that the fill of net `GND` on `layer` as the KiCad board `fab` writes holds
/// it (its `filled_polygon`s, issue #1184) shares with the sense copper on it grown by `grow`.
fn held_over(board: &Board, copper: &SenseCopper, layer: CopperLayer, grow: f64) -> f64 {
    let text = netsmith_pcb::kicad_pcb_filled(board)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .text;
    let root = netsmith_kicad::Sexp::parse(&text).unwrap();
    let word = |node: &netsmith_kicad::Sexp, key: &str| {
        node.find(key)
            .and_then(|n| n.items().get(1)?.text())
            .map(str::to_string)
    };
    let mut held = Polygons::new();
    held.polarity(true);
    let mut pieces = 0;
    for z in root.find_all("zone") {
        if word(z, "net_name").as_deref() != Some("GND") || z.find("attr").is_some() {
            continue;
        }
        for f in z
            .find_all("filled_polygon")
            .filter(|f| word(f, "layer").as_deref() == Some(layer.name()))
        {
            let ring: Vec<G> = f
                .find("pts")
                .unwrap()
                .find_all("xy")
                .map(|xy| {
                    let n = |i: usize| xy.items()[i].text().unwrap().parse::<f64>().unwrap();
                    G { x: n(1), y: n(2) }
                })
                .collect();
            // One ring with its holes joined to it: the even-odd rule reads it.
            held.region(Contour::polygon(&ring), None, &[]);
            pieces += 1;
        }
    }
    assert!(pieces > 0, "the board file holds GND's fill on {}", layer.name());
    shared(&held, &drawn(board, copper, layer, grow))
}

/// The area in mm² that `a` and `b` share.
fn shared(poured: &Polygons, sense: &Polygons) -> f64 {
    let size = |pen: &Polygons| pen.areas().iter().map(|a| a.size()).sum::<f64>();
    // |A and B| = |A| + |B| - |A or B|.
    let mut both = Polygons::new();
    both.polarity(true);
    for a in poured.areas().iter().chain(&sense.areas()) {
        both.region(Contour::polygon(&a.outer), None, &[]);
        both.polarity(false);
        for h in &a.holes {
            both.region(Contour::polygon(h), None, &[]);
        }
        both.polarity(true);
    }
    size(poured) + size(sense) - size(&both)
}

/// The errors of netsmith's design rules check of the board file.
fn drc_errors(board: &Board) -> Vec<String> {
    let report = netsmith_geom::drc_check(
        &kicad_pcb(board),
        Some(&kicad_pro(board)),
        &DrcExtras::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    report.errors().map(|f| f.to_string()).collect()
}

/// What every poured case checks once the board is routed: the sense pin's copper reaches the
/// shunt's pad and no other pad, the pour on `layer` covers none of it and keeps `space` from it,
/// the finding is gone, nothing is left unrouted and the design rules check finds no error. And
/// with the statement taken away (the copper left as it is) the carve goes and the pour covers
/// the sense copper: the carve is what keeps it off.
fn sense_copper_is_alone_under_the_pour(board: &mut Board, layer: CopperLayer, space: f64) {
    let copper = ground_sense(board);
    assert!(copper.joins.is_empty(), "the sense copper joins {:?}", copper.joins);
    assert!(!copper.tracks.is_empty(), "the sense pin has a track");
    assert!(copper.reached, "the sense track reaches the shunt's pad");
    let carves = sense_carves(board);
    assert!(
        carves
            .iter()
            .any(|c| c.net == "GND" && c.layers.iter().any(|(l, _)| *l == layer)),
        "the pour on {} is carved: {carves:?}",
        layer.name()
    );
    let drops = board.teardrops.len();
    let over = poured_over(board, &copper, layer, 0.0);
    assert!(over < 1e-6, "the pour covers {over} mm² of the sense copper ({drops} teardrops)");
    // A micrometre of the polygons' own error less than the space.
    let near = poured_over(board, &copper, layer, space - 0.001);
    assert!(near < 1e-6, "the pour comes {near} mm² nearer the sense copper than {space} mm");
    // The KiCad board holds the same fill (issue #1184), carved the same way.
    let held = held_over(board, &copper, layer, space - 0.001);
    assert!(held < 1e-6, "the board file's fill comes {held} mm² nearer the sense copper than {space} mm");
    let findings = kelvin_findings(board);
    assert!(findings.is_empty(), "{findings:?}");
    assert!(netsmith_pcb::Ratsnest::of(board).is_empty());
    let errors = drc_errors(board);
    assert!(errors.is_empty(), "{errors:#?}");

    let amp = board.parts.iter().position(|p| p.path == "amp").unwrap();
    let layout = std::mem::take(&mut board.parts[amp].layout);
    let over = poured_over(board, &copper, layer, 0.0);
    board.parts[amp].layout = layout;
    assert!(
        over > 0.01,
        "with no statement the pour covers the sense copper: {over} mm²"
    );
}

#[test]
fn on_a_net_with_an_outer_pour_the_pour_is_carved_round_the_sense_track() {
    let mut board = placed(&design(&format!(
        "{GROUND}\n    .process.top.net &= \"GND\""
    )));
    assert!(netsmith_pcb::net_is_poured(&board, "GND"));
    route(&mut board);
    let space = board
        .pours
        .iter()
        .map(|p| p.clearance.unwrap_or(board.rules.plane_clearance))
        .fold(board.clearance("GND"), f64::max);
    sense_copper_is_alone_under_the_pour(&mut board, CopperLayer::Front, space);
}

#[test]
fn on_a_net_with_a_plane_the_plane_is_carved_round_the_sense_pins_through_hole_pad() {
    // The amplifier's pins are through-hole pads, so the plane on `inner1` reaches the sense pin's
    // pad wherever its track runs: the plane is carved round it.
    let design = design_on(
        "FourLayer",
        &format!("{GROUND}\n    .process.inner1.net &= \"GND\"\n    .process.inner2.use &= \"signal\""),
    );
    let mut board = placed_with(&design, "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical");
    // The header's pads are 2.54 mm apart, from `IN+` at the amplifier's place to the right:
    // the feed moves right by as much.
    let amp = board.parts.iter().position(|p| p.path == "amp").unwrap();
    board.parts[amp].rotation = 90.0;
    let pads: Vec<Pt> = board.parts[amp]
        .footprint
        .pads
        .iter()
        .map(|p| board.parts[amp].pad_centre(p))
        .collect();
    assert!(
        (pads[1].x - pads[0].x - 2.54).abs() < 1e-6 && (pads[1].y - pads[0].y).abs() < 1e-6,
        "{pads:?}"
    );
    let feed = board.parts.iter().position(|p| p.path == "feed").unwrap();
    board.parts[feed].at = Pt::new(17.0, 10.0);
    assert_eq!(board.planes.len(), 1, "{:?}", board.planes);
    route(&mut board);
    let inner = CopperLayer::of_name("In1.Cu").unwrap();
    assert!(
        board.tracks.iter().all(|t| t.layer != inner),
        "a plane holds no track"
    );
    let space = board.rules.plane_clearance.max(board.clearance("GND"));
    sense_copper_is_alone_under_the_pour(&mut board, inner, space);
}

#[test]
fn on_a_net_with_a_zone_of_the_routes_file_the_zone_is_carved_round_the_sense_track() {
    let mut board = placed(&design(GROUND));
    let o = board.outline;
    let clearance = 0.3;
    board.zones.push(BoardZone {
        net: "GND".into(),
        layers: vec!["F.Cu".into()],
        outline: vec![
            Pt::new(o.min.x + 0.5, o.min.y + 0.5),
            Pt::new(o.max.x - 0.5, o.min.y + 0.5),
            Pt::new(o.max.x - 0.5, o.max.y - 0.5),
            Pt::new(o.min.x + 0.5, o.max.y - 0.5),
        ],
        holes: Vec::new(),
        priority: 0,
        clearance,
        min_width: 0.25,
        pads: ZonePads::Thermal,
        thermal_gap: 0.5,
        spoke_width: 0.5,
        islands: ZoneIslands::Remove,
        island_min_area: None,
        patch: false,
    });
    route(&mut board);
    let space = clearance.max(board.clearance("GND"));
    sense_copper_is_alone_under_the_pour(&mut board, CopperLayer::Front, space);
}
