//! Tests of the clean-up of a line's copper (issue #1486).

use crate::testlib::{fixtures, led_design};

use super::*;

fn led_board() -> Board {
    crate::board_from_design(
        &led_design(),
        &netsmith_kicad::Mapping::standard(),
        &fixtures(),
    )
    .unwrap()
}

fn piece(from: (f64, f64), to: (f64, f64)) -> Track {
    Track {
        net: "VCC".into(),
        layer: CopperLayer::Front,
        from: Pt::new(from.0, from.1),
        to: Pt::new(to.0, to.1),
        width: 0.2,
        mid: None,
    }
}

/// The line's tracks after [`Router::smooth`], with nothing in the way.
fn smoothed(tracks: Vec<Track>) -> Vec<Track> {
    let board = led_board();
    let router = Router::new(&board, &RouteOptions::default());
    let mut routed = Routed {
        line: 0,
        tracks,
        vias: Vec::new(),
    };
    router.smooth(&mut routed, &|_, _, _, _| true);
    routed.tracks
}

/// The bends between pieces in a row, in degrees.
fn bends(tracks: &[Track]) -> Vec<f64> {
    let mut out = Vec::new();
    for a in tracks {
        for b in tracks {
            if a.to.distance(b.from) < 1e-6 {
                let (u, v) = (a.to - a.from, b.to - b.from);
                let cos = (u.x * v.x + u.y * v.y) / (u.x.hypot(u.y) * v.x.hypot(v.y));
                out.push(cos.clamp(-1.0, 1.0).acos().to_degrees());
            }
        }
    }
    out
}

#[test]
fn a_jog_of_one_cell_becomes_one_bend_of_45_degrees_each_way() {
    let tracks = smoothed(vec![
        piece((50.0, 50.0), (52.0, 50.0)),
        piece((52.0, 50.0), (52.0, 50.1)),
        piece((52.0, 50.1), (54.0, 50.1)),
    ]);
    assert_eq!(tracks.len(), 3, "{tracks:?}");
    assert!(
        tracks.iter().all(|t| t.length() >= SHORT - 1e-9),
        "{tracks:?}"
    );
    assert!(
        bends(&tracks).iter().all(|b| (b - 45.0).abs() < 1e-6),
        "{tracks:?}"
    );
}

#[test]
fn a_stair_of_short_pieces_becomes_one_bend() {
    let tracks = smoothed(vec![
        piece((50.0, 50.0), (52.0, 50.0)),
        piece((52.0, 50.0), (52.0, 50.1)),
        piece((52.0, 50.1), (52.1, 50.1)),
        piece((52.1, 50.1), (52.1, 50.2)),
        piece((52.1, 50.2), (52.2, 50.2)),
        piece((52.2, 50.2), (52.2, 52.0)),
    ]);
    assert!(
        tracks.iter().all(|t| t.length() >= SHORT - 1e-9),
        "no short piece is left: {tracks:?}"
    );
    assert!(
        bends(&tracks).iter().all(|b| *b <= 45.0 + 1e-6),
        "{tracks:?}"
    );
    // The two long runs still start and end where they did.
    let ends: Vec<Pt> = tracks.iter().flat_map(|t| [t.from, t.to]).collect();
    assert!(ends.iter().any(|p| p.distance(Pt::new(50.0, 50.0)) < 1e-9));
    assert!(ends.iter().any(|p| p.distance(Pt::new(52.2, 52.0)) < 1e-9));
}

#[test]
fn a_step_whose_change_comes_too_near_other_copper_stays() {
    let board = led_board();
    let router = Router::new(&board, &RouteOptions::default());
    let tracks = vec![
        piece((50.0, 50.0), (52.0, 50.0)),
        piece((52.0, 50.0), (52.0, 50.1)),
        piece((52.0, 50.1), (54.0, 50.1)),
    ];
    let mut routed = Routed {
        line: 0,
        tracks: tracks.clone(),
        vias: Vec::new(),
    };
    router.smooth(&mut routed, &|_, _, _, _| false);
    assert_eq!(routed.tracks, tracks);
}

#[test]
fn a_stub_in_a_pad_is_folded_into_its_run_with_the_bend_moved_to_meet_it() {
    let board = led_board();
    let router = Router::new(&board, &RouteOptions::default());
    let line = &router.lines[0];
    let p = line.from_at;
    let net = router.nets[0];
    let layer = board
        .tracks
        .first()
        .map(|t| t.layer)
        .unwrap_or(CopperLayer::Front);
    let at = |dx: f64, dy: f64| Pt::new(p.x + dx, p.y + dy);
    // The stub to a cell 0.03 and 0.04 mm off the centre, a run along x, a bend, a run along y.
    let mut routed = Routed {
        line: 0,
        tracks: [
            (at(0.0, 0.0), at(0.03, 0.04)),
            (at(0.03, 0.04), at(3.0, 0.04)),
            (at(3.0, 0.04), at(3.0, 3.0)),
        ]
        .into_iter()
        .map(|(from, to)| Track {
            net: line.net.clone(),
            layer,
            from,
            to,
            width: 0.2,
            mid: None,
        })
        .collect(),
        vias: Vec::new(),
    };
    router.snap_ends(&mut routed, net, &|_, _, _, _| true);
    let ends: Vec<(Pt, Pt)> = routed.tracks.iter().map(|t| (t.from, t.to)).collect();
    assert_eq!(ends.len(), 2, "{ends:?}");
    assert!(
        ends[0].0.distance(p) < 1e-9 && ends[0].1.distance(at(3.0, 0.0)) < 1e-9,
        "{ends:?}"
    );
    assert!(
        ends[1].0.distance(at(3.0, 0.0)) < 1e-9 && ends[1].1.distance(at(3.0, 3.0)) < 1e-9,
        "{ends:?}"
    );
}
