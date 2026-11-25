//! The DC resistance of a net's copper between two pads (issue #1703), on boards drawn by hand
//! and checked against the figures worked out by hand: two parallel ways share the current by
//! their conductances, and the heat of each piece is IPC-2221's, the rule of `track_current`.

use crate::common;

use common::*;
use netsmith_fab::resistance::{self, Ask, Copper, Kind, Resistance};
use netsmith_geom::Artwork;
use netsmith_geom::ipc2221::{Layer, TrackSizing, ipc2221_rise, ipc2221_width, via_current};

const RHO: f64 = 1.72e-5;
const COPPER: f64 = 0.035;

fn sizing() -> TrackSizing {
    TrackSizing {
        copper: COPPER,
        inner_copper: None,
        rise: 10.0,
        tolerance: 0.2,
        via_drill: 0.3,
        via_plating: 0.018,
    }
}

fn measure(b: &mut Pcb, from: &str, to: &str, current: Option<f64>) -> Result<Resistance, String> {
    let art = Artwork::parse(&b.pcb()).unwrap();
    resistance::of(
        &art,
        &Ask {
            net: "P",
            from: &[from.to_string()],
            to: &[to.to_string()],
            current,
            copper: Copper {
                outer: COPPER,
                inner: COPPER,
                plating: 0.018,
            },
            sizing: Some(sizing()),
        },
    )
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

/// Net `P` from S1 at (10, 10) to S2 at (30, 10) two ways on the front: straight, 20 mm of a
/// 0.5 mm track, and round by y = 14, 28 mm of a 0.25 mm track, each from pad centre to pad
/// centre.
fn two_ways() -> Pcb {
    let mut b = Pcb::new(2);
    b.stub("P", 10.0, 10.0);
    b.stub("P", 30.0, 10.0);
    b.track("P", "F.Cu", (10.0, 10.0), (30.0, 10.0), 0.5);
    b.track("P", "F.Cu", (10.0, 10.0), (10.0, 14.0), 0.25);
    b.track("P", "F.Cu", (10.0, 14.0), (30.0, 14.0), 0.25);
    b.track("P", "F.Cu", (30.0, 14.0), (30.0, 10.0), 0.25);
    b
}

#[test]
fn two_parallel_ways_share_the_current_by_their_conductance() {
    let r = measure(&mut two_ways(), "S1.1", "S2", Some(2.0)).unwrap();
    // By hand: each way is rho * length / (width * thickness), and the two in parallel.
    let wide = RHO * 20.0 / (0.5 * COPPER);
    let narrow = RHO * 28.0 / (0.25 * COPPER);
    let pair = wide * narrow / (wide + narrow);
    assert!(close(r.resistance, pair), "{} is not {pair}", r.resistance);
    assert!(close(r.drop.unwrap(), 2.0 * pair), "{r:?}");
    assert_eq!((r.from.as_slice(), r.to.as_slice()), (&["S1.1".to_string()][..], &["S2.1".to_string()][..]));
    // The current in each way: the wide one carries the narrow one's share of the resistance.
    let in_wide = 2.0 * narrow / (wide + narrow);
    let in_narrow = 2.0 * wide / (wide + narrow);
    let tracks: Vec<_> = r.segments.iter().filter(|s| s.kind == Kind::Track).collect();
    assert_eq!(tracks.len(), 4, "{r:#?}");
    let straight = tracks.iter().find(|s| s.width == 0.5).unwrap();
    assert!(close(straight.current.unwrap(), in_wide), "{straight:?}");
    assert_eq!((straight.from, straight.to), ((10.0, 10.0), (30.0, 10.0)));
    for s in tracks.iter().filter(|s| s.width == 0.25) {
        assert!(close(s.current.unwrap(), in_narrow), "{s:?}");
    }
    // The narrowest track the current passes is the way round, with its share.
    let n = r.narrowest.as_ref().unwrap();
    assert_eq!((n.width, n.layer.as_str()), (0.25, "F.Cu"));
    assert!(close(n.share, in_narrow / 2.0), "{n:?}");
    assert_eq!(r.vias, 0);
    // The heat is IPC-2221's at the narrowest finished track the tolerance allows, and the
    // hottest is first: here the wide track, which carries most.
    let rise = |i: f64, w: f64| ipc2221_rise(i, w * 0.8, COPPER, Layer::External);
    let hottest = &r.segments[0];
    let (rise_wide, rise_narrow) = (rise(in_wide, 0.5), rise(in_narrow, 0.25));
    assert!(rise_wide > rise_narrow, "{rise_wide} {rise_narrow}");
    assert_eq!(hottest.width, 0.5, "{r:#?}");
    assert!(close(hottest.rise.unwrap(), rise_wide), "{hottest:?}");
    // The width each needs is the check's: IPC-2221 before the tolerance.
    let needs = ipc2221_width(in_wide, COPPER, 10.0, Layer::External) / 0.8;
    assert!(close(hottest.needs.unwrap(), needs), "{hottest:?}");
    assert_eq!(hottest.over, 0.5 < needs);
    // The rise is over the allowed exactly where the width is under what it needs.
    for s in &r.segments {
        assert_eq!(s.over, s.rise.unwrap() > 10.0 + 1e-9, "{s:?}");
    }
    // At a current that heats the narrow way past 10 K, it is over and needs more.
    let hot = measure(&mut two_ways(), "S1", "S2", Some(8.0)).unwrap();
    assert!(hot.segments.iter().all(|s| s.over), "{hot:#?}");
    let text = hot.text_moved((0.0, 0.0));
    assert!(text.contains("narrowest track: 0.250 mm on F.Cu"), "{text}");
    assert!(text.contains(", over"), "{text}");
    // The work is counted, never timed.
    assert!(r.iterations > 0);
}

#[test]
fn a_via_adds_its_barrel_and_is_counted() {
    let mut b = Pcb::new(2);
    b.stub("P", 10.0, 10.0);
    b.stub("P", 30.0, 10.0);
    b.track("P", "F.Cu", (10.0, 10.0), (20.0, 10.0), 0.3);
    b.via("P", (20.0, 10.0), 0.6, 0.3);
    b.track("P", "B.Cu", (20.0, 10.0), (30.0, 10.0), 0.3);
    // A back stub of the far pad's own: the B.Cu track ends in the stub pad's copper anyway, as
    // a pad joins copper on any layer.
    let r = measure(&mut b, "S1", "S2", Some(1.0)).unwrap();
    let barrel = std::f64::consts::PI * (0.168f64.powi(2) - 0.15f64.powi(2));
    let by_hand = 2.0 * RHO * 10.0 / (0.3 * COPPER) + RHO * 1.6 / barrel;
    assert!(close(r.resistance, by_hand), "{} is not {by_hand}", r.resistance);
    assert_eq!(r.vias, 1);
    let via = r.segments.iter().find(|s| s.kind == Kind::Via).unwrap();
    assert_eq!((via.layer.as_str(), via.width), ("F.Cu-B.Cu", 0.3));
    assert!(close(via.current.unwrap(), 1.0), "{via:?}");
    assert!(close(via.carries.unwrap(), via_current(0.3, 0.018, 10.0)), "{via:?}");
}

#[test]
fn ends_with_no_copper_between_or_no_pad_are_errors() {
    let mut b = two_ways();
    b.stub("P", 50.0, 50.0);
    let e = measure(&mut b, "S1", "S3", None).unwrap_err();
    assert!(e.contains("no track or via of net P joins S1.1 to S3.1"), "{e}");
    let e = measure(&mut two_ways(), "S1", "U9", None).unwrap_err();
    assert!(e.contains("U9 is no pad of net P"), "{e}");
    // With no current, the resistance alone and the pieces by their share.
    let r = measure(&mut two_ways(), "S1", "S2", None).unwrap();
    assert!(r.current.is_none() && r.drop.is_none());
    assert!(r.segments.iter().all(|s| s.rise.is_none()));
    assert!(r.segments[0].share >= r.segments[1].share);
}
