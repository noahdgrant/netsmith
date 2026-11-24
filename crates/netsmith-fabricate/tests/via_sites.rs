//! `routes sites` (issue #1628): the legal via sites between the pads of a 0.8 mm pitch BGA,
//! generated here on the LED board, each with the nets of its neighbours, and a site a track of
//! another net holds.

use netsmith_fabricate::routes::{self, Edit};
use netsmith_fabricate::via_sites;
use netsmith_pcb::{Board, BoardNet, CopperLayer, PadRef, Pt, Rect, Routes};

/// The LED board, grown, with a 7 by 7 BGA of 0.8 mm pitch (`U1`) whose every ball is on a net
/// of its own, `N_<ball>`.
fn bga_board() -> Board {
    let mut board: Board = serde_json::from_str(include_str!(
        "../../netsmith-pcb/tests/fixtures/led.board.json"
    ))
    .expect("a fixture is the JSON of a Board");
    board.outline = Rect::new(Pt::new(20.0, 20.0), Pt::new(52.0, 40.0));
    let mut part = board.parts[0].clone();
    part.refdes = "U1".to_string();
    part.path = "u1".to_string();
    part.footprint =
        netsmith_pcb::generated_footprint("Package_BGA:Analog_BGA-49_6.25x6.25mm_Layout7x7_P0.8mm")
            .expect("a generated BGA");
    part.at = Pt::new(44.0, 31.0);
    part.rotation = 0.0;
    for pad in &part.footprint.pads {
        board.nets.push(BoardNet {
            name: format!("N_{}", pad.number),
            pads: vec![PadRef {
                refdes: "U1".to_string(),
                number: pad.number.clone(),
            }],
        });
    }
    board.parts.push(part);
    board
}

/// The place of ball `number` of U1, in the routes file's frame.
fn ball(board: &Board, number: &str) -> Pt {
    let part = board.part("U1").unwrap();
    let pad = part.footprint.pads_numbered(number).next().unwrap();
    part.pad_centre(pad) - board.outline.min
}

#[test]
fn a_bga_lists_the_sites_between_its_balls_with_the_net_of_each_neighbour() {
    let board = bga_board();
    let sites = via_sites::sites(&board, &Routes::default(), "U1", None).expect("sites");
    eprintln!("{}", sites.text());
    // A via fits at the centre of each square of four balls, not between two of a row.
    assert_eq!(sites.between.len(), 36, "{}", sites.text());
    assert_eq!(sites.tried, 36 + 2 * 7 * 6);
    assert!(sites.between.iter().all(|s| s.free && s.pads.len() == 4));
    let (a1, b2) = (ball(&board, "A1"), ball(&board, "B2"));
    let first = &sites.between[0];
    assert!(
        (first.at[0] - (a1.x + b2.x) / 2.0).abs() < 1e-6
            && (first.at[1] - (a1.y + b2.y) / 2.0).abs() < 1e-6,
        "{first:?}"
    );
    let mut nets: Vec<&str> = first
        .pads
        .iter()
        .map(|p| p.net.as_deref().unwrap())
        .collect();
    nets.sort();
    assert_eq!(nets, ["N_A1", "N_A2", "N_B1", "N_B2"]);
    assert!(
        first
            .pads
            .iter()
            .all(|p| (p.distance - 0.8 * std::f64::consts::SQRT_2 / 2.0).abs() < 1e-3)
    );
    // The row midpoints are refused by a rule of the check, counted.
    assert_eq!(
        sites.refused.iter().map(|r| r.sites).sum::<usize>(),
        2 * 7 * 6,
        "{:?}",
        sites.refused
    );
    // Each ball is a site in a pad, of its own net, listed apart.
    assert_eq!(sites.in_pad.len(), 49, "{}", sites.text());
    let in_a1 = sites.in_pad.iter().find(|s| s.pad == "U1.A1").unwrap();
    assert_eq!(in_a1.net.as_deref(), Some("N_A1"));
    // The text says the frame, and moves with it.
    assert!(sites.text().contains("from the outline's top left corner"));
    let at = format!("{:.3},{:.3}", first.at[0] + 20.0, first.at[1] + 20.0);
    assert!(sites.text_moved([20.0, 20.0], "the board file's").contains(&at));
}

#[test]
fn a_site_a_track_of_another_net_crosses_is_held_by_that_net() {
    let board = bga_board();
    let (a1, a2, b1) = (ball(&board, "A1"), ball(&board, "A2"), ball(&board, "B1"));
    // A track of N_A1 along the channel between rows A and B, from A1's column to beyond A2's:
    // it crosses the site at the centre of A1, A2, B1 and B2.
    let y = (a1.y + b1.y) / 2.0;
    let mut routes = Routes::default();
    routes::edit(
        &board,
        &mut routes,
        &Edit::AddTrack {
            net: "N_A1".to_string(),
            layer: CopperLayer::Front,
            width: Some(0.1),
            from: [a1.x, y],
            mid: None,
            to: [a2.x + 0.4, y],
            lock: false,
        },
    )
    .expect("the track keeps the rules' space");
    let sites = via_sites::sites(&board, &routes, "U1", None).expect("sites");
    let centre = [(a1.x + a2.x) / 2.0, y];
    let site = sites
        .between
        .iter()
        .find(|s| (s.at[0] - centre[0]).abs() < 1e-6 && (s.at[1] - centre[1]).abs() < 1e-6)
        .expect("the site is legal");
    assert!(!site.free, "{site:?}");
    assert_eq!(
        site.held_by,
        [via_sites::Holder {
            net: "N_A1".to_string(),
            what: "track".to_string()
        }]
    );
    // For a via of the track's own net it is free: its own copper is no other net's.
    let own = via_sites::sites(&board, &routes, "U1", Some("N_A1")).expect("sites");
    let site = own
        .between
        .iter()
        .find(|s| (s.at[0] - centre[0]).abs() < 1e-6 && (s.at[1] - centre[1]).abs() < 1e-6)
        .expect("the site is legal");
    assert!(site.free, "{site:?}");
    // Only the pads of the asked net are sites in a pad.
    assert_eq!(own.in_pad.len(), 1);
    // A site far from the track is free.
    assert!(sites.between.iter().filter(|s| s.free).count() >= 30);
}
