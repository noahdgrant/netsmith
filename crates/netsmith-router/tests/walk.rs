//! The walkaround query (`netsmith_router::walk`): where a segment of a new track is free, the
//! way round copper when it is not, and why there is none. Boards are the LED example with
//! copper of other nets added in a free place of it. Nothing here is timed: the work is counted
//! in the states the search took (`Walked::tried`).

use netsmith_geom::{DrcExtras, DrcRule, drc_check};
use netsmith_pcb::{Board, CopperLayer, Pt, Track, kicad_pcb, kicad_pro};
use netsmith_router::walk::{Blocked, Ground, MOST_TRIED, Walk, Walked};

use crate::common;

const WIDTH: f64 = 0.2;

/// The LED board on an outline with room to spare, so that a part of it is empty.
fn led() -> Board {
    let mut board = common::example_board("led", &common::fixtures());
    board.outline.max = Pt::new(board.outline.max.x + 20.0, board.outline.max.y + 14.0);
    board
}

/// The first spot on a half millimetre lattice of the board where a track 6 mm wide fits on
/// both layers: a place with no copper within 3 mm.
fn empty_place(board: &Board) -> Pt {
    let ground = Ground::of(board).unwrap();
    let walker = ground.walker();
    let o = board.outline;
    let mut y = o.min.y;
    while y <= o.max.y {
        let mut x = o.min.x;
        while x <= o.max.x {
            let c = Pt::new(x, y);
            let free = [CopperLayer::Front, CopperLayer::Back]
                .iter()
                .all(|&l| walker.free("TEST", l, 6.0, c, c) == Some(true));
            if free {
                return c;
            }
            x += 0.5;
        }
        y += 0.5;
    }
    panic!("the board has no empty place: {:?}", board.outline);
}

fn at(c: Pt, x: f64, y: f64) -> Pt {
    Pt::new(c.x + x, c.y + y)
}

/// A bar of another net across the way, in the empty place.
fn with_bar(c: Pt, net: &str) -> Board {
    let mut board = led();
    board.tracks.push(Track::straight(
        net,
        CopperLayer::Front,
        at(c, 0.0, -1.5),
        at(c, 0.0, 1.5),
        0.4,
    ));
    board
}

fn walk(net: &str, layer: CopperLayer, from: Pt, to: Pt, heading: Option<Pt>) -> Walk<'_> {
    Walk {
        net,
        layer,
        width: WIDTH,
        from,
        to,
        heading,
    }
}

fn path(board: &Board, w: &Walk<'_>) -> Result<Walked, Blocked> {
    Ground::of(board).unwrap().walker().path(w)
}

fn turn(u: Pt, v: Pt) -> f64 {
    let dot = u.x * v.x + u.y * v.y;
    (dot / (u.x.hypot(u.y) * v.x.hypot(v.y)))
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

/// Whether every segment of the path is free, every bend is 45 degrees at most, and so is the
/// first turn from the heading.
fn assert_sound(board: &Board, w: &Walk<'_>, walked: &Walked) {
    let ground = Ground::of(board).unwrap();
    let walker = ground.walker();
    let p = &walked.points;
    assert_eq!(p.first(), Some(&w.from));
    assert_eq!(p.last(), Some(&w.to));
    let dir = |a: Pt, b: Pt| Pt::new(b.x - a.x, b.y - a.y);
    for pair in p.windows(2) {
        assert_eq!(
            walker.free(w.net, w.layer, w.width, pair[0], pair[1]),
            Some(true),
            "{pair:?} is not free"
        );
    }
    for three in p.windows(3) {
        let t = turn(dir(three[0], three[1]), dir(three[1], three[2]));
        assert!(t <= 45.001, "a bend of {t} degrees at {:?}", three[1]);
    }
    if let (Some(h), true) = (w.heading, p.len() >= 2) {
        let t = turn(h, dir(p[0], p[1]));
        assert!(
            t <= 45.001,
            "the first segment turns {t} degrees from the heading"
        );
    }
}

#[test]
fn a_free_segment_is_the_path_and_takes_no_search() {
    let board = led();
    let c = empty_place(&board);
    let w = walk(
        "TEST",
        CopperLayer::Front,
        at(c, -2.0, 0.3),
        at(c, 2.0, -0.7),
        None,
    );
    let walked = path(&board, &w).unwrap();
    assert_eq!(walked.points, vec![w.from, w.to]);
    assert_eq!(walked.tried, 0);
}

#[test]
fn a_track_goes_round_copper_of_another_net_with_bends_of_45_degrees_at_most() {
    let c = empty_place(&led());
    let board = with_bar(c, "GND");
    let w = walk(
        "TEST",
        CopperLayer::Front,
        at(c, -2.0, 0.0),
        at(c, 2.0, 0.0),
        None,
    );
    let ground = Ground::of(&board).unwrap();
    assert_eq!(
        ground
            .walker()
            .free(w.net, w.layer, w.width, w.from, w.to),
        Some(false),
        "the bar is in the way"
    );
    let walked = path(&board, &w).unwrap();
    assert!(
        walked.tried > 0 && walked.tried < MOST_TRIED,
        "{}",
        walked.tried
    );
    assert!(walked.points.len() >= 3);
    assert_sound(&board, &w, &walked);
    // A way round a bar, not a wander: a few corners, and not twice the straight run.
    let length: f64 = walked.points.windows(2).map(|p| p[0].distance(p[1])).sum();
    assert!(walked.points.len() <= 6, "{:?}", walked.points);
    assert!(length < 2.0 * w.from.distance(w.to), "{length}");
}

#[test]
fn a_wider_track_needs_a_longer_way_round() {
    let c = empty_place(&led());
    let board = with_bar(c, "GND");
    let narrow = walk(
        "TEST",
        CopperLayer::Front,
        at(c, -2.0, 0.0),
        at(c, 2.0, 0.0),
        None,
    );
    let wide = Walk {
        width: 0.8,
        ..narrow
    };
    let length = |w: &Walk<'_>| -> f64 {
        path(&board, w)
            .unwrap()
            .points
            .windows(2)
            .map(|p| p[0].distance(p[1]))
            .sum()
    };
    assert!(length(&wide) > length(&narrow));
}

#[test]
fn copper_of_the_walked_net_and_of_the_other_layer_is_no_obstacle() {
    let c = empty_place(&led());
    let board = with_bar(c, "GND");
    let level = |net, layer| walk(net, layer, at(c, -2.0, 0.0), at(c, 2.0, 0.0), None);
    for w in [
        level("GND", CopperLayer::Front),
        level("TEST", CopperLayer::Back),
    ] {
        let walked = path(&board, &w).unwrap();
        assert_eq!(walked.points, vec![w.from, w.to]);
        assert_eq!(walked.tried, 0);
    }
}

#[test]
fn an_end_on_copper_is_refused_with_no_search() {
    let c = empty_place(&led());
    let board = with_bar(c, "GND");
    let w = walk(
        "TEST",
        CopperLayer::Front,
        at(c, -2.0, 0.0),
        at(c, 0.1, 0.0),
        None,
    );
    assert_eq!(path(&board, &w), Err(Blocked::End));
    let w = walk(
        "TEST",
        CopperLayer::Front,
        at(c, 0.1, 0.0),
        at(c, 2.0, 0.0),
        None,
    );
    assert_eq!(path(&board, &w), Err(Blocked::Start));
}

#[test]
fn a_pocket_closed_by_copper_has_no_way_out_and_says_so_after_a_bounded_search() {
    let c = empty_place(&led());
    let mut board = led();
    // A box 3 mm wide of another net's track, with the start inside.
    let corners = [(-1.5, -1.5), (1.5, -1.5), (1.5, 1.5), (-1.5, 1.5), (-1.5, -1.5)];
    for pair in corners.windows(2) {
        board.tracks.push(Track::straight(
            "GND",
            CopperLayer::Front,
            at(c, pair[0].0, pair[0].1),
            at(c, pair[1].0, pair[1].1),
            0.4,
        ));
    }
    let w = walk("TEST", CopperLayer::Front, c, at(c, 2.9, 0.0), None);
    match path(&board, &w) {
        Err(Blocked::WalledIn { tried }) => assert!(tried > 0 && tried < MOST_TRIED, "{tried}"),
        other => panic!("{other:?}"),
    }
    // The reason is a sentence for the person drawing.
    assert!(
        Blocked::WalledIn { tried: 1 }
            .to_string()
            .contains("no way round")
    );
}

#[test]
fn the_first_segment_turns_from_the_heading_by_45_degrees_at_most() {
    let board = led();
    let c = empty_place(&board);
    // The segment arriving at `from` runs north (y grows downward): a straight run east
    // would be a right angle, so the track bends first.
    let heading = Some(Pt::new(0.0, -1.0));
    let w = walk("TEST", CopperLayer::Front, c, at(c, 2.0, 0.5), heading);
    let walked = path(&board, &w).unwrap();
    assert!(walked.points.len() >= 3);
    assert_sound(&board, &w, &walked);
}

/// How long the query takes on a routed board of a hundred parts: printed, never judged (`--ignored
/// --nocapture`).
#[test]
#[ignore = "a measurement, not a check"]
fn measured() {
    use std::time::Instant;
    let mut board = common::tight_board(0, (50.0, 42.0));
    netsmith_router::route(&mut board);
    let net = board.nets[0].name.clone();
    let started = Instant::now();
    let ground = Ground::of(&board).unwrap();
    let walker = ground.walker();
    println!("ground and room built in {:?}", started.elapsed());
    let o = board.outline;
    let (mut found, mut blocked, mut tried) = (0, 0, 0);
    let started = Instant::now();
    for k in 0..40 {
        let f = |n: f64| 3.0 + (n * 7.31) % 1.0;
        let from = Pt::new(o.min.x + f(k as f64) * 10.0, o.min.y + f(k as f64 + 0.5) * 8.0);
        let to = Pt::new(from.x + 3.0 + (k % 5) as f64, from.y + 2.0 - (k % 3) as f64);
        let w = Walk {
            net: &net,
            layer: CopperLayer::Front,
            width: WIDTH,
            from,
            to,
            heading: None,
        };
        match walker.path(&w) {
            Ok(walked) => {
                found += 1;
                tried += walked.tried;
            }
            Err(blocked_by) => {
                blocked += 1;
                tried += blocked_by.tried();
            }
        }
    }
    println!(
        "40 walks in {:?}: {found} found, {blocked} blocked, {tried} states",
        started.elapsed()
    );
}

/// The check on `board`, less the errors of copper not yet drawn (what `routes::edit_all` leaves
/// out too).
fn errors(board: &Board) -> Vec<String> {
    let extras = DrcExtras {
        mask_web: Some(board.rules.mask_bridge),
        via_in_pad: Some(board.process.via_in_pad),
        excused_courtyards: board.excused_courtyards(),
        hole_to_track: Some(board.rules.hole_clearance),
        plated_hole_clearance: Some(board.rules.plated_hole_clearance),
        pad_to_pad: Some(board.rules.pad_to_pad),
        pad_hole_to_hole: Some(board.rules.hole_to_hole),
        mask_to_track: Some(board.rules.mask_to_track),
        plated_slot: Some(board.rules.plated_slot),
        npth_slot: Some(board.rules.npth_slot),
        npth_drill: Some(board.rules.npth_drill),
        pad_ring: Some(board.rules.min_annular_ring),
        least_widths: board.least_widths(),
        neck_reach: netsmith_router::route::NECK_REACH,
        via_counts: board.via_counts(),
        track_angle: true,
    };
    drc_check(&kicad_pcb(board), Some(&kicad_pro(board)), &extras)
        .unwrap()
        .errors()
        .filter(|f| {
            !matches!(
                f.rule,
                DrcRule::UnconnectedItems | DrcRule::TrackDangling | DrcRule::ViaDangling
            )
        })
        .map(ToString::to_string)
        .collect()
}

#[test]
fn the_design_rules_check_finds_nothing_in_a_walked_track() {
    let c = empty_place(&led());
    let mut board = with_bar(c, "GND");
    let before = errors(&board);
    let w = walk(
        "VCC",
        CopperLayer::Front,
        at(c, -2.0, 0.0),
        at(c, 2.0, 0.0),
        None,
    );
    let walked = path(&board, &w).unwrap();
    for pair in walked.points.windows(2) {
        board.tracks.push(Track::straight(
            "VCC",
            CopperLayer::Front,
            pair[0],
            pair[1],
            WIDTH,
        ));
    }
    assert_eq!(errors(&board), before);
    // The same track straight through the bar is an error: the check does see the bar.
    let mut through = with_bar(c, "GND");
    through
        .tracks
        .push(Track::straight("VCC", CopperLayer::Front, w.from, w.to, WIDTH));
    assert_ne!(errors(&through), before);
}
