//! The crumbs of the router's copper (issue #1539), counted one kind at a time on routed boards:
//! a straight track shorter than a third of its width (a tiny piece, such as a nudge of 0.05 mm
//! from one pad to the next), a track of no length, a track equal to another of its net and
//! layer (two copies of one track), and a track from a pad's centre that ends inside the pad (a
//! piece that joins nothing). The router must leave none of them on the examples and on generated
//! boards, with its own weights and with the polish pass of stated weights. (The larger projects
//! of `examples/` showed each kind; their pieces came from the clean-up of a line's ends, in
//! `route/clean.rs`, and from the loops' pass, in `loops.rs`.)

use netsmith_pcb::{Board, Pt, Track};
use netsmith_router::{RouteOptions, Weights, route_with};

use crate::common;

/// Ends nearer than this are one place.
const SAME: f64 = 1e-4;

/// Each kind of crumb, counted over a board.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Crumbs {
    pub tiny: usize,
    pub zero_length: usize,
    pub duplicates: usize,
    pub in_pad_from_centre: usize,
}

fn same(p: Pt, q: Pt) -> bool {
    p.distance(q) < SAME
}

fn equal(a: &Track, b: &Track) -> bool {
    a.net == b.net
        && a.layer == b.layer
        && ((same(a.from, b.from) && same(a.to, b.to)) || (same(a.from, b.to) && same(a.to, b.from)))
        && match (a.mid, b.mid) {
            (None, None) => true,
            (Some(p), Some(q)) => same(p, q),
            _ => false,
        }
}

/// Counts the crumbs of `board`'s copper, with where each is in `places`.
pub fn count(board: &Board, places: &mut Vec<String>) -> Crumbs {
    let mut c = Crumbs::default();
    // Each pad of a net: its net, centre, box and copper layers.
    let mut pads = Vec::new();
    for part in &board.parts {
        for pad in &part.footprint.pads {
            let Some(net) = board.net_of(&part.refdes, &pad.number) else {
                continue;
            };
            pads.push((
                net.name.clone(),
                part.pad_centre(pad),
                part.pad_bounds(pad),
                part.pad_layers(pad),
                format!("{}.{}", part.refdes, pad.number),
            ));
        }
    }
    for (i, t) in board.tracks.iter().enumerate() {
        let length = t.from.distance(t.to);
        if t.mid.is_none() {
            if length < 1e-6 {
                c.zero_length += 1;
                places.push(format!("zero length {} {:?} at {:?}", t.net, t.layer, t.from));
                continue;
            }
            if length < t.width / 3.0 {
                c.tiny += 1;
                places.push(format!(
                    "tiny {} {:?} {:?}->{:?} ({length:.4} mm)",
                    t.net, t.layer, t.from, t.to
                ));
            }
        }
        if board.tracks[..i].iter().any(|o| equal(o, t)) {
            c.duplicates += 1;
            places.push(format!("duplicate {} {:?} {:?}->{:?}", t.net, t.layer, t.from, t.to));
        }
        if t.mid.is_none() {
            for (net, centre, bounds, (front, back), name) in &pads {
                let on = match t.layer {
                    netsmith_pcb::CopperLayer::Front => *front,
                    netsmith_pcb::CopperLayer::Back => *back,
                    netsmith_pcb::CopperLayer::Inner(_) => *front && *back,
                };
                if !on || *net != t.net {
                    continue;
                }
                let inside = |p: Pt| {
                    p.x > bounds.min.x + SAME
                        && p.x < bounds.max.x - SAME
                        && p.y > bounds.min.y + SAME
                        && p.y < bounds.max.y - SAME
                };
                let (a, b) = (t.from, t.to);
                if (same(a, *centre) && inside(b)) || (same(b, *centre) && inside(a)) {
                    c.in_pad_from_centre += 1;
                    places.push(format!(
                        "in pad {name} {} {:?} {:?}->{:?}",
                        t.net, t.layer, t.from, t.to
                    ));
                }
            }
        }
    }
    c
}

fn boards() -> Vec<(String, Board)> {
    let mut boards: Vec<(String, Board)> = [
        "led",
        "status_indicator",
        "linear_regulator",
        "four_layer",
        "six_layer",
    ]
    .into_iter()
    .map(|name| (name.to_string(), common::example_board(name, &common::fixtures())))
    .collect();
    boards.extend((1..=3).map(|seed| {
        (
            format!("generated board {seed}"),
            common::synthetic_board(seed, 10, 3, 8, 10, 4),
        )
    }));
    // The seeded boards of crossed nets, whose pads sit close together.
    boards.extend(
        common::hard_boards(4)
            .into_iter()
            .enumerate()
            .map(|(k, b)| (format!("crossed nets {k}"), b)),
    );
    boards
}

fn check(name: &str, board: &Board) {
    let mut places = Vec::new();
    let c = count(board, &mut places);
    assert_eq!(c, Crumbs::default(), "{name}: {places:#?}");
}

/// The examples and generated boards, routed with the router's own weights: no crumb.
#[test]
fn the_router_leaves_no_crumb_on_the_examples_and_generated_boards() {
    for (name, mut board) in boards() {
        route_with(&mut board, RouteOptions::default());
        check(&name, &board);
    }
}

/// The same boards routed with stated weights, so with the polish pass (issue #1535): no crumb.
#[test]
fn the_polish_pass_leaves_no_crumb() {
    for (name, mut board) in boards() {
        route_with(
            &mut board,
            RouteOptions {
                weights: Some(Weights::DEFAULT),
                ..RouteOptions::default()
            },
        );
        check(&format!("{name}, polished"), &board);
    }
}
