//! `routes fanout` (issue #1640): a generated 7 by 7 BGA of 0.8 mm pitch fanned out as a group,
//! the pads a track of another net walls in listed with what blocks them, and a plan followed or
//! refused.

use std::collections::BTreeSet;

use netsmith_fabricate::fanout::{self, FanoutAsk};
use netsmith_fabricate::routes::{self, Edit, Refusal, drc_errors, with_routes};
use netsmith_pcb::{Board, BoardNet, CopperLayer, PadRef, Pt, Rect, Routes};

/// The LED board, grown, with two 7 by 7 BGAs of 0.8 mm pitch, `U1` and `U2`, and a net
/// `N_<ball>` from each ball of U1 to the same ball of U2: every net leaves U1.
fn board() -> Board {
    let mut board: Board = serde_json::from_str(include_str!(
        "../../netsmith-pcb/tests/fixtures/led.board.json"
    ))
    .expect("a fixture is the JSON of a Board");
    board.outline = Rect::new(Pt::new(20.0, 20.0), Pt::new(64.0, 40.0));
    let footprint =
        netsmith_pcb::generated_footprint("Package_BGA:Analog_BGA-49_6.25x6.25mm_Layout7x7_P0.8mm")
            .expect("a generated BGA");
    for (refdes, x) in [("U1", 44.0), ("U2", 56.0)] {
        let mut part = board.parts[0].clone();
        part.refdes = refdes.to_string();
        part.path = refdes.to_lowercase();
        part.footprint = footprint.clone();
        part.at = Pt::new(x, 31.0);
        part.rotation = 0.0;
        board.parts.push(part);
    }
    for pad in &footprint.pads {
        board.nets.push(BoardNet {
            name: format!("N_{}", pad.number),
            pads: ["U1", "U2"]
                .iter()
                .map(|r| PadRef {
                    refdes: r.to_string(),
                    number: pad.number.clone(),
                })
                .collect(),
        });
    }
    board
}

/// The place of ball `number` of U1, in the routes file's frame.
fn ball(board: &Board, number: &str) -> Pt {
    let part = board.part("U1").unwrap();
    let pad = part.footprint.pads_numbered(number).next().unwrap();
    part.pad_centre(pad) - board.outline.min
}

/// The errors of the check with `routes` that it does not find with `before`.
fn new_errors(board: &Board, before: &Routes, after: &Routes) -> Vec<String> {
    let mut old = drc_errors(&with_routes(board, before).unwrap().0).unwrap();
    let mut new = Vec::new();
    for e in drc_errors(&with_routes(board, after).unwrap().0).unwrap() {
        match old.iter().position(|o| *o == e) {
            Some(k) => {
                old.swap_remove(k);
            }
            None => new.push(e),
        }
    }
    new
}

/// Whether the tracks of `net`'s record join the pad at `pad` to the via at `via` (the routes
/// file's places).
fn joined(routes: &Routes, board: &Board, net: &str, pad: Pt, via: [f64; 2]) -> bool {
    let record = &routes.boards[&board.name].nets[net];
    assert!(
        record
            .vias
            .iter()
            .any(|v| (v.at[0] - via[0]).hypot(v.at[1] - via[1]) < 1e-6),
        "the via of {net} is in its record"
    );
    let ends: Vec<(Pt, Pt)> = record
        .tracks
        .iter()
        .map(|t| (Pt::new(t.from[0], t.from[1]), Pt::new(t.to[0], t.to[1])))
        .collect();
    let via = Pt::new(via[0], via[1]);
    let mut reached = vec![pad];
    let mut used = BTreeSet::new();
    loop {
        if reached.iter().any(|p| p.distance(via) < 0.01) {
            return true;
        }
        let before = used.len();
        for (k, (a, b)) in ends.iter().enumerate() {
            if used.contains(&k) {
                continue;
            }
            let near = |p: &Pt| reached.iter().any(|r| r.distance(*p) < 0.01);
            if near(a) || near(b) {
                used.insert(k);
                reached.push(*a);
                reached.push(*b);
            }
        }
        if used.len() == before {
            return false;
        }
    }
}

fn ask() -> FanoutAsk {
    FanoutAsk {
        part: "U1".to_string(),
        ..FanoutAsk::default()
    }
}

#[test]
fn a_bga_is_fanned_out_as_a_group_to_legal_sites_with_no_new_errors() {
    let board = board();
    let routes = Routes::default();
    let done = fanout::fanout(&board, &routes, &ask()).expect("a fanout");
    eprintln!("{}", done.text());
    // The 25 balls inside the outer ring need to escape.
    assert_eq!(done.needed, 25, "{}", done.text());
    eprintln!(
        "escaped {} of {} pads ({:.0} %), {} run(s), {} check(s)",
        done.escaped.len(),
        done.needed,
        100.0 * done.escaped.len() as f64 / done.needed as f64,
        done.runs,
        done.checks
    );
    assert_eq!(done.escaped.len(), 25, "{}", done.text());
    assert!(done.left.is_empty(), "{}", done.text());
    assert!(done.runs <= fanout::ROUNDS);
    let after = done.routes.clone().expect("the change");
    let errors = new_errors(&board, &routes, &after);
    assert!(errors.is_empty(), "{errors:#?}");
    // Each via is at a site of `routes sites`, and each pad is joined to its via.
    let sites = netsmith_fabricate::via_sites::sites(&board, &routes, "U1", None).unwrap();
    let mut taken = BTreeSet::new();
    for e in &done.escaped {
        assert!(
            sites
                .between
                .iter()
                .any(|s| (s.at[0] - e.at[0]).hypot(s.at[1] - e.at[1]) < 1e-6),
            "{e:?} is at a legal site"
        );
        assert!(taken.insert(((e.at[0] * 1e4) as i64, (e.at[1] * 1e4) as i64)));
        let number = e.pad.strip_prefix("U1.").unwrap();
        assert!(
            joined(&after, &board, &e.net, ball(&board, number), e.at),
            "{} is joined to its via: {:#?}",
            e.pad,
            after.boards[&board.name].nets[&e.net]
        );
    }
    // Asked again, every pad is joined already and nothing changes.
    let again = fanout::fanout(&board, &after, &ask()).unwrap();
    assert_eq!(again.joined.len(), 25, "{}", again.text());
    assert_eq!(again.needed, 0);
    assert!(again.routes.is_none());
    assert_eq!(again.runs, 0);
}

#[test]
fn pads_whose_sites_another_net_holds_are_listed_with_the_blocker() {
    let board = board();
    let (c1, d1, e1, c7) = (
        ball(&board, "C1"),
        ball(&board, "D1"),
        ball(&board, "E1"),
        ball(&board, "C7"),
    );
    // Two tracks of N_A1 along the channels on both sides of row D: every site of a ball of row
    // D is held.
    let mut routes = Routes::default();
    for y in [(c1.y + d1.y) / 2.0, (d1.y + e1.y) / 2.0] {
        routes::edit(
            &board,
            &mut routes,
            &Edit::AddTrack {
                net: "N_A1".to_string(),
                layer: CopperLayer::Front,
                width: Some(0.1),
                from: [c1.x, y],
                mid: None,
                to: [c7.x, y],
                lock: true,
            },
        )
        .expect("the track keeps the rules' space");
    }
    let done = fanout::fanout(&board, &routes, &ask()).expect("a fanout");
    eprintln!("{}", done.text());
    assert_eq!(done.needed, 25);
    assert_eq!(done.escaped.len(), 20, "{}", done.text());
    let left: Vec<&str> = done.left.iter().map(|l| l.pad.as_str()).collect();
    assert_eq!(left, ["U1.D2", "U1.D3", "U1.D4", "U1.D5", "U1.D6"]);
    for l in &done.left {
        assert!(l.blocker.contains("N_A1"), "{l:?}");
    }
    let text = done.text();
    assert!(text.contains("5 pad(s) not fanned out"), "{text}");
    let after = done.routes.clone().expect("the others are written");
    assert!(new_errors(&board, &routes, &after).is_empty());
}

#[test]
fn a_plan_is_followed_and_an_illegal_planned_site_is_refused_naming_the_blocker() {
    let board = board();
    let routes = Routes::default();
    let (b2, c3, c4) = (ball(&board, "B2"), ball(&board, "C3"), ball(&board, "C4"));
    // B2 to the centre of its square with C3, below it to the right.
    let site = [(b2.x + c3.x) / 2.0, (b2.y + c3.y) / 2.0];
    let plan = fanout::parse_plan(&format!(
        "# a plan\nU1.B2 = {},{}\n\n",
        site[0], site[1]
    ))
    .unwrap();
    let done = fanout::fanout(
        &board,
        &routes,
        &FanoutAsk {
            plan: Some(plan),
            ..ask()
        },
    )
    .expect("a fanout");
    assert_eq!(done.needed, 1, "{}", done.text());
    assert_eq!(done.escaped.len(), 1, "{}", done.text());
    assert!((done.escaped[0].at[0] - site[0]).abs() < 1e-6);
    let after = done.routes.clone().unwrap();
    assert!(joined(&after, &board, "N_B2", b2, done.escaped[0].at));
    assert!(new_errors(&board, &routes, &after).is_empty());

    // Between C3 and C4 a via touches C4's pad: refused, naming its net.
    let plan = fanout::parse_plan(&format!("C3 = {},{}", (c3.x + c4.x) / 2.0, c3.y)).unwrap();
    let refused = fanout::fanout(
        &board,
        &routes,
        &FanoutAsk {
            plan: Some(plan),
            ..ask()
        },
    );
    match refused {
        Err(Refusal::Refused(why)) => {
            assert!(why.contains("U1.C3") && why.contains("N_C4"), "{why}");
            assert!(why.contains("nothing is written"), "{why}");
        }
        other => panic!("refused, not {other:?}"),
    }
    // A line that is not `PAD = X,Y` is said with its number.
    let bad = fanout::parse_plan("B2 = 1\n").unwrap_err();
    assert!(bad.contains("line 1"), "{bad}");
}

#[test]
fn the_matching_gives_every_pad_a_site_at_the_least_total() {
    // Two pads, two sites: the greedy choice of pad 0 (site 0, cost 1) leaves pad 1 with none.
    let cost = vec![vec![Some(1.0), Some(2.0)], vec![Some(1.5), None]];
    assert_eq!(fanout::assign(&cost), [Some(1), Some(0)]);
    // More pads than sites: one is left with none.
    let cost = vec![vec![Some(1.0)], vec![Some(0.5)]];
    assert_eq!(fanout::assign(&cost), [None, Some(0)]);
}
