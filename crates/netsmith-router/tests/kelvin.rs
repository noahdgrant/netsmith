//! A Kelvin sense connection (issue #1148): a part's `kelvin` statement makes its pin a sense
//! pin, routed from the pad it senses (the shunt's pad on the pin's net) alone. The board below puts the
//! load's pad nearer the sense pin than the shunt is, so the net's shortest tree joins the sense
//! pin to the load and the shunt's current would flow through the sense pin's pad; with the
//! statement, the sense track runs from the shunt's pad and nothing else of the net touches it.
//! The tests count the copper (tracks, vias and the pads it joins), never time.

use crate::guard;

use netsmith_core::Design;
use netsmith_pcb::{Board, Pt, Rect, kelvin_findings, kelvin_senses, sense_copper};
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
    let source = SOURCE.replace("EXTRA", extra);
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
    let row = |kind: &str| {
        format!(
            "[[part]]\nkind = \"{kind}\"\npackage = \"0805\"\ndefault = true\nsymbol = \"Device:R\"\nfootprint = \"Resistor_SMD:R_0805_2012Metric\"\n"
        )
    };
    let mapping = netsmith_kicad::Mapping::parse(
        &format!("{}{}", row("Chip"), row("Amp")),
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
            (joins, c.tracks, c.reached)
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
