//! Shoving unlocked copper out of the way of a track being drawn (`netsmith_router::shove`, the
//! third slice of issue 726): which pieces move and where, which stay, and the piece that cannot
//! get out of the way. Boards are the LED example with copper in a free place of it. Nothing
//! here is timed: the work is counted in the questions asked of the room (`Shoved::checked`).

use netsmith_geom::{DrcExtras, DrcRule, drc_check};
use netsmith_pcb::{Board, CopperLayer, Pt, Track, Via, kicad_pcb, kicad_pro};
use netsmith_router::shove::{Drawn, MOST_CHECKED, Moved, Piece, Shoved, Stuck, What};
use netsmith_router::walk::Ground;

use crate::common;

const WIDTH: f64 = 0.2;
const BAR: f64 = 0.4;

fn led() -> Board {
    let mut board = common::example_board("led", &common::fixtures());
    board.outline.max = Pt::new(board.outline.max.x + 20.0, board.outline.max.y + 14.0);
    board
}

/// The first spot on a half millimetre lattice where a track 6 mm wide fits on both layers.
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

fn bar(id: usize, net: &str, a: Pt, b: Pt) -> Piece {
    Piece {
        id,
        what: What::Track(Track::straight(net, CopperLayer::Front, a, b, BAR)),
    }
}

fn track_of(p: &Piece) -> &Track {
    match &p.what {
        What::Track(t) => t,
        What::Via(_) => panic!("a via"),
    }
}

fn drawn<'a>(net: &'a str, corners: &'a [Pt]) -> Drawn<'a> {
    Drawn {
        net,
        layer: CopperLayer::Front,
        width: WIDTH,
        corners,
    }
}

fn shove(board: &Board, d: &Drawn<'_>, pieces: &[Piece]) -> Result<Shoved, Stuck> {
    Ground::of(board).unwrap().shove(d, pieces)
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

/// The board with the pieces where the shove put them and the drawn track on it.
fn after(base: &Board, pieces: &[Piece], d: &Drawn<'_>, shoved: &Shoved) -> Board {
    let mut board = base.clone();
    for p in pieces {
        let moved = shoved.moved.iter().find(|m| m.id() == p.id);
        match (&p.what, moved) {
            (What::Track(t), Some(Moved::Track { from, to, .. })) => {
                board.tracks.push(Track {
                    from: *from,
                    to: *to,
                    ..t.clone()
                });
            }
            (What::Via(v), Some(Moved::Via { at, .. })) => board.vias.push(Via {
                at: *at,
                ..v.clone()
            }),
            (What::Track(t), None) => board.tracks.push(t.clone()),
            (What::Via(v), None) => board.vias.push(v.clone()),
            _ => panic!("a piece moved as another kind"),
        }
    }
    for w in d.corners.windows(2) {
        board
            .tracks
            .push(Track::straight(d.net, d.layer, w[0], w[1], d.width));
    }
    board
}

fn moved_track(shoved: &Shoved, id: usize) -> (Pt, Pt) {
    match shoved.moved.iter().find(|m| m.id() == id) {
        Some(Moved::Track { from, to, .. }) => (*from, *to),
        other => panic!("piece {id} did not move as a track: {other:?}"),
    }
}

#[test]
fn a_track_across_an_unlocked_track_pushes_it_clear_and_the_check_is_quiet() {
    let base = led();
    let c = empty_place(&base);
    let pieces = [bar(7, "GND", at(c, 0.0, -1.5), at(c, 0.0, 1.5))];
    // The track starts at the left and ends past the bar: the bar is pushed on to the right.
    let corners = [at(c, -2.0, 0.0), at(c, 0.3, 0.0)];
    let d = drawn("VCC", &corners);
    let shoved = shove(&base, &d, &pieces).unwrap();
    assert_eq!(shoved.pushed, 1);
    assert_eq!(shoved.moved.len(), 1);
    let (from, to) = moved_track(&shoved, 7);
    // Still the same bar, straight along y, as long as it was, and moved to the right.
    assert!((from.x - to.x).abs() < 1e-9, "{from:?} {to:?}");
    assert!(((from.y - to.y).abs() - 3.0).abs() < 1e-9 && from.y == at(c, 0.0, -1.5).y);
    assert!(from.x > at(c, 0.3, 0.0).x + WIDTH / 2.0 + BAR / 2.0, "{from:?}");
    assert!(shoved.checked > 0 && shoved.checked < MOST_CHECKED);
    assert_eq!(errors(&after(&base, &pieces, &d, &shoved)), errors(&base));
    // Without the push the same copper is an error: the check does see the bar.
    let mut crossed = base.clone();
    crossed.tracks.push(track_of(&pieces[0]).clone());
    crossed
        .tracks
        .push(Track::straight("VCC", CopperLayer::Front, corners[0], corners[1], WIDTH));
    assert_ne!(errors(&crossed), errors(&base));
}

#[test]
fn copper_that_keeps_its_spaces_stays_where_it_is() {
    let base = led();
    let c = empty_place(&base);
    let pieces = [
        bar(1, "GND", at(c, 0.0, -1.5), at(c, 0.0, 1.5)),
        bar(2, "GND", at(c, 2.2, -1.5), at(c, 2.2, 1.5)),
    ];
    // The track stops well short of the first bar.
    let corners = [at(c, -2.0, 0.0), at(c, -1.0, 0.0)];
    let shoved = shove(&base, &drawn("VCC", &corners), &pieces).unwrap();
    assert!(shoved.moved.is_empty(), "{shoved:?}");
    assert_eq!(shoved.pushed, 0);
}

#[test]
fn a_cascade_of_two_pushes_the_next_track_too() {
    let base = led();
    let c = empty_place(&base);
    // The second bar is where the first would go: it is pushed in its turn.
    let pieces = [
        bar(1, "GND", at(c, 0.0, -1.5), at(c, 0.0, 1.5)),
        bar(2, "VCC", at(c, 0.9, -1.5), at(c, 0.9, 1.5)),
    ];
    let corners = [at(c, -2.0, 0.0), at(c, 0.3, 0.0)];
    // A third net of the board, so that the bars keep the space from each other too.
    let third = base
        .nets
        .iter()
        .map(|n| n.name.clone())
        .find(|n| n != "VCC" && n != "GND")
        .unwrap();
    let d = drawn(&third, &corners);
    let shoved = shove(&base, &d, &pieces).unwrap();
    assert_eq!(shoved.pushed, 2, "{shoved:?}");
    let (a, _) = moved_track(&shoved, 1);
    let (b, _) = moved_track(&shoved, 2);
    assert!(b.x > a.x && a.x > 0.0 + c.x, "{a:?} {b:?}");
    assert_eq!(errors(&after(&base, &pieces, &d, &shoved)), errors(&base));
}

#[test]
fn a_track_that_is_pinched_between_the_drawn_track_and_copper_that_stays_is_stuck_and_named() {
    let mut base = led();
    let c = empty_place(&base);
    // A wall of another net that stays (it is in the ground): too near for the bar to go between.
    base.tracks.push(Track::straight(
        "VCC",
        CopperLayer::Front,
        at(c, 0.8, -2.5),
        at(c, 0.8, 2.5),
        BAR,
    ));
    let pieces = [bar(5, "GND", at(c, 0.0, -1.5), at(c, 0.0, 1.5))];
    let corners = [at(c, -2.0, 0.0), at(c, 0.3, 0.0)];
    let stuck = shove(&base, &drawn("TEST", &corners), &pieces).unwrap_err();
    assert_eq!(stuck.by(), Some(5), "{stuck:?}");
    assert!(stuck.checked() > 0 && stuck.checked() <= MOST_CHECKED);
}

#[test]
fn a_track_with_both_ends_held_is_stuck_and_a_track_with_one_end_held_turns_about_it() {
    let mut base = led();
    let c = empty_place(&base);
    // Copper of the bar's net that stays, at each end: the ends are held.
    for y in [-1.5, 1.5] {
        base.tracks.push(Track::straight(
            "GND",
            CopperLayer::Front,
            at(c, 0.0, y),
            at(c, 0.0, y * 1.6),
            BAR,
        ));
    }
    let pieces = [bar(3, "GND", at(c, 0.0, -1.5), at(c, 0.0, 1.5))];
    let corners = [at(c, -2.0, 0.0), at(c, 0.3, 0.0)];
    let stuck = shove(&base, &drawn("VCC", &corners), &pieces).unwrap_err();
    assert_eq!(stuck.by(), Some(3), "{stuck:?}");

    // With one end free it is pushed on its free end alone: it turns about the other.
    let mut one = led();
    one.tracks.push(Track::straight(
        "GND",
        CopperLayer::Front,
        at(c, 0.0, -1.5),
        at(c, 0.0, -2.4),
        BAR,
    ));
    let corners = [at(c, -2.0, 1.0), at(c, 0.3, 1.0)];
    let d = drawn("VCC", &corners);
    let shoved = shove(&one, &d, &pieces).unwrap();
    let (from, to) = moved_track(&shoved, 3);
    assert_eq!(from, at(c, 0.0, -1.5), "the held end stays");
    assert!(to.x > c.x + 0.3, "{to:?}");
}

#[test]
fn the_tracks_that_meet_a_pushed_one_follow_and_keep_their_angles() {
    let base = led();
    let c = empty_place(&base);
    // A horizontal track with a track at 45 degrees at each end, all free.
    let a = at(c, -1.5, -1.0);
    let b = at(c, 1.5, -1.0);
    let pieces = [
        bar(1, "GND", a, b),
        bar(2, "GND", at(c, -2.2, -1.7), a),
        bar(3, "GND", b, at(c, 2.2, -1.7)),
    ];
    // The track comes up from below and ends just under the bar.
    let corners = [at(c, 0.0, 2.0), at(c, 0.0, -0.8)];
    let d = drawn("VCC", &corners);
    let shoved = shove(&base, &d, &pieces).unwrap();
    assert_eq!(shoved.pushed, 1, "{shoved:?}");
    assert_eq!(shoved.moved.len(), 3, "{shoved:?}");
    let (top_a, top_b) = moved_track(&shoved, 1);
    let (left_far, left_joint) = moved_track(&shoved, 2);
    let (right_joint, right_far) = moved_track(&shoved, 3);
    // The bar is still level and moved up; the others still meet it at its ends.
    assert!((top_a.y - top_b.y).abs() < 1e-9 && top_a.y < a.y - 0.1, "{top_a:?} {top_b:?}");
    assert_eq!(left_joint, top_a);
    assert_eq!(right_joint, top_b);
    // Their far ends stayed and their directions did not turn: the corners are still 135 degrees.
    assert_eq!((left_far, right_far), (at(c, -2.2, -1.7), at(c, 2.2, -1.7)));
    let dir = |p: Pt, q: Pt| Pt::new(q.x - p.x, q.y - p.y);
    let (l, r) = (dir(left_far, left_joint), dir(right_joint, right_far));
    assert!((l.x.abs() - l.y.abs()).abs() < 1e-5 && (r.x.abs() - r.y.abs()).abs() < 1e-5);
    assert_eq!(errors(&after(&base, &pieces, &d, &shoved)), errors(&base));
}

#[test]
fn a_via_is_pushed_and_the_track_that_meets_it_follows() {
    let base = led();
    let c = empty_place(&base);
    let via = Via {
        net: "GND".into(),
        at: at(c, 0.0, 0.0),
        diameter: 0.6,
        drill: 0.3,
    };
    let pieces = [
        Piece {
            id: 1,
            what: What::Via(via),
        },
        bar(2, "GND", at(c, 0.0, 0.0), at(c, 1.5, 1.5)),
    ];
    let corners = [at(c, -2.0, 0.0), at(c, -0.2, 0.0)];
    let d = drawn("VCC", &corners);
    let shoved = shove(&base, &d, &pieces).unwrap();
    let Some(Moved::Via { at: now, .. }) = shoved.moved.iter().find(|m| m.id() == 1) else {
        panic!("the via did not move: {shoved:?}");
    };
    assert!(now.x > c.x, "{now:?}");
    // The track kept its far end and its end on the via.
    let (from, to) = moved_track(&shoved, 2);
    assert_eq!((from, to), (*now, at(c, 1.5, 1.5)));
    assert_eq!(errors(&after(&base, &pieces, &d, &shoved)), errors(&base));
}

#[test]
fn the_work_is_counted_and_far_copper_adds_none_to_it() {
    let base = led();
    let c = empty_place(&base);
    let near = bar(1, "GND", at(c, 0.0, -1.5), at(c, 0.0, 1.5));
    let corners = [at(c, -2.0, 0.0), at(c, 0.3, 0.0)];
    let few = shove(&base, &drawn("VCC", &corners), std::slice::from_ref(&near)).unwrap();
    // Twenty tracks of the same net 2.5 mm and more from the track change nothing it does.
    let mut many = vec![near];
    for k in 0..20 {
        many.push(bar(
            10 + k,
            "GND",
            at(c, -2.8, -2.8 + k as f64 * 0.001),
            at(c, -2.8, -2.2 + k as f64 * 0.001),
        ));
    }
    let more = shove(&base, &drawn("VCC", &corners), &many).unwrap();
    assert_eq!(more.moved.len(), few.moved.len());
    assert_eq!(more.pushed, few.pushed);
}
