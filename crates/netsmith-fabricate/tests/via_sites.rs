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
    // The track holds it, with its place, the rule and the figures of the check's finding
    // (issue #1669), as `routes check-via` gives them.
    // It breaks several rules with the via there; each holder is the track, the largest
    // shortfall first.
    assert!(!site.held_by.is_empty());
    for h in &site.held_by {
        assert_eq!((h.net.as_str(), h.what.as_str()), ("N_A1", "track"), "{h:?}");
        assert!(h.item.starts_with("Track [N_A1]"), "{h:?}");
        assert!(
            (h.at[0] - a1.x).abs() < 1e-3 && (h.at[1] - y).abs() < 1e-3,
            "{h:?}"
        );
        let (required, measured, short) = (
            h.required.unwrap(),
            h.measured.unwrap(),
            h.shortfall.unwrap(),
        );
        assert!(
            measured < required && (short - (required - measured)).abs() < 1e-3,
            "{h:?}"
        );
    }
    assert!(
        site.held_by
            .windows(2)
            .all(|w| w[0].shortfall >= w[1].shortfall)
    );
    let h = &site.held_by[0];
    let (required, measured, short) = (
        h.required.unwrap(),
        h.measured.unwrap(),
        h.shortfall.unwrap(),
    );
    let checked = netsmith_fabricate::via_check::check_via(
        &board, &routes, centre, None, None, None, None,
    )
    .expect("checked");
    for h in &site.held_by {
        let c = checked
            .breaks
            .iter()
            .find(|c| c.rule == h.rule && c.item == "track")
            .unwrap_or_else(|| panic!("check-via names the track for {}: {:?}", h.rule, checked.breaks));
        assert_eq!(
            (c.required, c.measured, c.shortfall),
            (h.required, h.measured, h.shortfall)
        );
    }
    let text = sites.text();
    let line = text
        .lines()
        .find(|l| l.contains(&format!("held {:.3},{:.3}", centre[0], centre[1])))
        .expect("the held site's line");
    assert!(
        line.contains(&format!("a track of N_A1 at {:.3},{:.3} ({}: needs {required:.4} mm, has {measured:.4} mm, short by {short:.4} mm)", h.at[0], h.at[1], h.rule)),
        "{line}"
    );
    let json = serde_json::to_value(&sites).unwrap();
    let held = json["between"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["free"] == false)
        .expect("a held site in the JSON");
    for key in ["net", "what", "item", "at", "rule", "required", "measured", "shortfall"] {
        assert!(!held["held_by"][0][key].is_null(), "{key}: {held}");
    }
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

/// Square centre `(row, column)` of U1's grid of balls, from 0: the site between balls
/// `(row, column)` and `(row + 1, column + 1)`.
fn square(board: &Board, row: usize, column: usize) -> [f64; 2] {
    let name = |r: usize, c: usize| format!("{}{}", (b'A' + r as u8) as char, c + 1);
    let (a, b) = (ball(board, &name(row, column)), ball(board, &name(row + 1, column + 1)));
    [(a.x + b.x) / 2.0, (a.y + b.y) / 2.0]
}

/// The BGA board with a GND zone on the bottom under U1, a GND via at square (2, 2) and a ring
/// of N_D4 vias round it at the 16 squares two away, but for the one at (4, 2): a via at (4, 2)
/// would seal the pocket of the zone round the GND via (issue #1668).
fn pocket_board() -> (Board, Routes) {
    let board = bga_board();
    let (a1, g7) = (ball(&board, "A1"), ball(&board, "G7"));
    let (min, max) = (
        Pt::new(a1.x.min(g7.x), a1.y.min(g7.y)),
        Pt::new(a1.x.max(g7.x), a1.y.max(g7.y)),
    );
    let mut changes = vec![Edit::AddZone {
        net: "GND".to_string(),
        layers: vec!["bottom".to_string()],
        outline: vec![
            [min.x - 2.0, min.y - 2.0],
            [max.x + 2.0, min.y - 2.0],
            [max.x + 2.0, max.y + 2.0],
            [min.x - 2.0, max.y + 2.0],
        ],
        clearance: Some(0.2),
        min_width: Some(0.3),
        thermal_gap: None,
        spoke_width: None,
        priority: 0,
        pads: netsmith_pcb::ZonePads::default(),
        patch: false,
        lock: false,
    }];
    let via = |net: &str, at: [f64; 2]| Edit::AddVia {
        net: net.to_string(),
        at,
        diameter: None,
        drill: None,
        layers: None,
        lock: false,
    };
    changes.push(via("GND", square(&board, 2, 2)));
    for row in 0..5 {
        for column in 0..5 {
            let ring = row == 0 || row == 4 || column == 0 || column == 4;
            if ring && (row, column) != (4, 2) {
                changes.push(via("N_D4", square(&board, row, column)));
            }
        }
    }
    let mut routes = Routes::default();
    routes::edit_all(&board, &mut routes, &changes).expect("the zone and the vias keep the rules");
    (board, routes)
}

fn site_at(sites: &[via_sites::Site], at: [f64; 2]) -> Option<&via_sites::Site> {
    sites
        .iter()
        .find(|s| (s.at[0] - at[0]).abs() < 1e-6 && (s.at[1] - at[1]).abs() < 1e-6)
}

#[test]
fn a_site_that_would_seal_a_pocket_round_another_via_is_marked_and_left_out_by_zone_safe() {
    let (board, routes) = pocket_board();
    let sites = via_sites::sites(&board, &routes, "U1", None).expect("sites");
    eprintln!("{}", sites.text());
    let gap = square(&board, 4, 2);
    let inside = square(&board, 2, 2);
    let site = site_at(&sites.between, gap).expect("the gap is a legal site");
    let [pocket] = site.pockets.as_slice() else {
        panic!("one pocket: {site:?}");
    };
    assert_eq!(
        (pocket.layer.as_str(), pocket.fill.as_str(), pocket.net.as_str()),
        ("bottom", "zone", "GND")
    );
    assert!(
        pocket
            .vias
            .iter()
            .any(|v| (v.at[0] - inside[0]).abs() < 1e-3 && (v.at[1] - inside[1]).abs() < 1e-3),
        "{pocket:?}"
    );
    assert!(pocket.area > 0.0);
    // A site that cuts nothing off is unmarked.
    let far = site_at(&sites.between, square(&board, 0, 5)).expect("a corner site");
    assert!(far.pockets.is_empty(), "{far:?}");
    assert!(sites.text().contains("cuts the bottom zone of GND into a new island"));
    // The work is two fills of a small box for each site listed, whatever the board's size.
    assert!(sites.fills <= 2 * (sites.between.len() + sites.in_pad.len()));
    // `--zone-safe` lists the site apart.
    let safe = sites.clone().zone_safe();
    assert!(site_at(&safe.between, gap).is_none());
    assert!(site_at(&safe.cuts_zone, gap).is_some());
    assert!(site_at(&safe.between, square(&board, 0, 5)).is_some());
    assert!(safe.between.iter().all(|s| s.pockets.is_empty()));
    assert!(safe.text().contains("set apart by `--zone-safe`"), "{}", safe.text());

    // `routes check-via` says the same of the place, and nothing of the far one.
    let checked = netsmith_fabricate::via_check::check_via(&board, &routes, gap, None, None, None, None)
        .expect("checked");
    assert_eq!(checked.pockets, site.pockets);
    assert!(checked.text().contains("note: the via cuts the bottom zone of GND"), "{}", checked.text());
    let far = netsmith_fabricate::via_check::check_via(&board, &routes, square(&board, 0, 5), None, None, None, None)
        .expect("checked");
    assert!(far.pockets.is_empty(), "{:?}", far.pockets);

    // `routes move-via` notes it of a via moved into the gap.
    let mut routes = routes;
    routes::edit(
        &board,
        &mut routes,
        &Edit::AddVia {
            net: "N_D4".to_string(),
            at: square(&board, 5, 5),
            diameter: None,
            drill: None,
            layers: None,
            lock: false,
        },
    )
    .expect("a via in the corner keeps the rules");
    let n = routes.boards[&board.name].nets["N_D4"].vias.len();
    let moved = netsmith_fabricate::move_via::move_via(&board, &routes, "N_D4", n, gap).expect("moved");
    assert!(moved.refused.is_none(), "{moved:?}");
    assert_eq!(moved.pockets.len(), 1, "{moved:?}");
    assert!(moved.text().contains("note: the via cuts the bottom zone of GND"));
}
