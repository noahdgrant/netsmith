//! The voltage a net's routed tracks drop (issue #652): the resistance between two pads along
//! the tracks, measured by hand.

use netsmith_pcb::{
    COPPER_RESISTIVITY, CopperLayer, NetPad, PadRef, PinDraw, Pt, Track, in_outer_copper,
    track_drop_of,
};

const OZ: f64 = 0.035;

fn pad(refdes: &str, number: &str, at: Pt) -> NetPad {
    NetPad {
        pad: PadRef {
            refdes: refdes.into(),
            number: number.into(),
        },
        min: Pt::new(at.x - 0.5, at.y - 0.5),
        max: Pt::new(at.x + 0.5, at.y + 0.5),
        draws: None,
    }
}

/// A pad of a pin whose current is known: `current` amperes (issue #1541).
fn drawing(refdes: &str, number: &str, at: Pt, current: f64) -> NetPad {
    NetPad {
        draws: Some(PinDraw {
            pin: format!("{refdes}.{number}"),
            current,
        }),
        ..pad(refdes, number, at)
    }
}

fn track(layer: CopperLayer, from: Pt, to: Pt, width: f64) -> Track {
    Track::straight("P", layer, from, to, width)
}

fn ohms(length: f64, width: f64) -> f64 {
    COPPER_RESISTIVITY * length / (width * OZ)
}

#[test]
fn a_track_between_two_pads_drops_its_resistance_times_the_current() {
    // 50 mm at 0.98 mm then 10 mm at 0.5 mm, with a bend between.
    let a = track(
        CopperLayer::Front,
        Pt::new(0.0, 0.0),
        Pt::new(50.0, 0.0),
        0.98,
    );
    let b = track(
        CopperLayer::Front,
        Pt::new(50.0, 0.0),
        Pt::new(50.0, 10.0),
        0.5,
    );
    let pads = [
        pad("J1", "1", Pt::new(0.0, 0.0)),
        pad("J2", "1", Pt::new(50.0, 10.0)),
    ];
    let d = track_drop_of("P", 2.0, &[&a, &b], &[], &pads, OZ).unwrap();
    let r = ohms(50.0, 0.98) + ohms(10.0, 0.5);
    assert!((d.resistance - r).abs() < 1e-12, "{} vs {r}", d.resistance);
    assert!((d.drop() - 2.0 * r).abs() < 1e-12);
    assert!((d.length - 60.0).abs() < 1e-9);
    assert_eq!(d.narrowest, 0.5);
    assert_eq!((d.from.refdes.as_str(), d.to.refdes.as_str()), ("J1", "J2"));
    // 2 A through 50 mm of 0.98 mm in 1 oz is about 0.05 V, the figure of the guide.
    assert!((2.0 * ohms(50.0, 0.98) - 0.0501).abs() < 1e-4);
}

#[test]
fn a_branch_that_leaves_a_track_part_way_joins_it_there() {
    // A trunk from J1 to J2 and a branch to J3 from its middle: the worst pair is J1 to J2
    // only when the trunk is longer than the branch's way round.
    let trunk = track(
        CopperLayer::Front,
        Pt::new(0.0, 0.0),
        Pt::new(40.0, 0.0),
        1.0,
    );
    let branch = track(
        CopperLayer::Front,
        Pt::new(20.0, 0.0),
        Pt::new(20.0, 30.0),
        0.25,
    );
    let pads = [
        pad("J1", "1", Pt::new(0.0, 0.0)),
        pad("J2", "1", Pt::new(40.0, 0.0)),
        pad("J3", "1", Pt::new(20.0, 30.0)),
    ];
    let d = track_drop_of("P", 1.0, &[&trunk, &branch], &[], &pads, OZ).unwrap();
    // J3 to either end: 30 mm of 0.25 mm and 20 mm of 1 mm.
    let r = ohms(30.0, 0.25) + ohms(20.0, 1.0);
    assert!((d.resistance - r).abs() < 1e-12, "{} vs {r}", d.resistance);
    assert_eq!(d.to.refdes, "J3");
    assert!((d.length - 50.0).abs() < 1e-9);
}

#[test]
fn a_via_joins_the_layers_and_pads_no_track_joins_are_no_pair() {
    let front = track(
        CopperLayer::Front,
        Pt::new(0.0, 0.0),
        Pt::new(10.0, 0.0),
        0.3,
    );
    let back = track(
        CopperLayer::Back,
        Pt::new(10.0, 0.0),
        Pt::new(10.0, 10.0),
        0.3,
    );
    let pads = [
        pad("U1", "1", Pt::new(0.0, 0.0)),
        pad("U2", "1", Pt::new(10.0, 10.0)),
        // On the net but joined to nothing.
        pad("U3", "1", Pt::new(80.0, 80.0)),
    ];
    let d = track_drop_of(
        "P",
        0.5,
        &[&front, &back],
        &[Pt::new(10.0, 0.0)],
        &pads,
        OZ,
    )
    .unwrap();
    assert!((d.resistance - ohms(20.0, 0.3)).abs() < 1e-12);
    assert_eq!((d.from.refdes.as_str(), d.to.refdes.as_str()), ("U1", "U2"));

    // Two pads of one part with nothing between them but each other's copper are no pair.
    let pads = [
        pad("U1", "1", Pt::new(0.0, 0.0)),
        pad("U1", "2", Pt::new(0.5, 0.0)),
    ];
    assert_eq!(track_drop_of("P", 0.5, &[], &[], &pads, OZ), None);
}

#[test]
fn of_two_ways_between_two_pads_the_one_of_least_resistance_counts() {
    let short = track(
        CopperLayer::Front,
        Pt::new(0.0, 0.0),
        Pt::new(10.0, 0.0),
        0.2,
    );
    let round = [
        track(CopperLayer::Back, Pt::new(0.0, 0.0), Pt::new(0.0, 5.0), 2.0),
        track(CopperLayer::Back, Pt::new(0.0, 5.0), Pt::new(10.0, 5.0), 2.0),
        track(CopperLayer::Back, Pt::new(10.0, 5.0), Pt::new(10.0, 0.0), 2.0),
    ];
    let pads = [
        pad("J1", "1", Pt::new(0.0, 0.0)),
        pad("J2", "1", Pt::new(10.0, 0.0)),
    ];
    let tracks: Vec<&Track> = std::iter::once(&short).chain(&round).collect();
    let d = track_drop_of("P", 1.0, &tracks, &[], &pads, OZ).unwrap();
    assert!((d.resistance - ohms(20.0, 2.0)).abs() < 1e-12);
    assert_eq!(d.narrowest, 2.0);
}

#[test]
fn a_track_on_an_inner_layer_drops_by_its_own_thinner_copper() {
    // 20 mm at 1 mm on the front, then 20 mm at 1 mm on In2.Cu in 0.0152 mm copper (issue
    // #833): the inner run is the outer track of the same resistance, 0.0152 / 0.035 as wide.
    const INNER: f64 = 0.0152;
    let a = track(CopperLayer::Front, Pt::new(0.0, 0.0), Pt::new(20.0, 0.0), 1.0);
    let b = track(
        CopperLayer::Inner(2),
        Pt::new(20.0, 0.0),
        Pt::new(40.0, 0.0),
        1.0,
    );
    let pads = [
        pad("J1", "1", Pt::new(0.0, 0.0)),
        pad("J2", "1", Pt::new(40.0, 0.0)),
    ];
    let owned = in_outer_copper(&[&a, &b], OZ, INNER);
    assert_eq!(owned[0].width, 1.0);
    let tracks: Vec<&Track> = owned.iter().collect();
    let d = track_drop_of("P", 1.0, &tracks, &[Pt::new(20.0, 0.0)], &pads, OZ).unwrap();
    let inner = COPPER_RESISTIVITY * 20.0 / (1.0 * INNER);
    assert!((d.resistance - (ohms(20.0, 1.0) + inner)).abs() < 1e-12, "{d:?}");
}

/// A trunk of 1 mm from the supply's header J1 (0, 0) to the load's header J2 (40, 0), and a
/// branch of 0.2 mm from the trunk's middle 30 mm up to a divider's resistor R1.
fn trunk_and_divider() -> (Track, Track) {
    (
        track(
            CopperLayer::Front,
            Pt::new(0.0, 0.0),
            Pt::new(40.0, 0.0),
            1.0,
        ),
        track(
            CopperLayer::Front,
            Pt::new(20.0, 0.0),
            Pt::new(20.0, 30.0),
            0.2,
        ),
    )
}

#[test]
fn a_divider_draws_its_own_current_and_not_the_nets() {
    // Issue #1541: 2 A from J1 to J2, and R1 of a feedback divider takes 50 uA. The divider's
    // branch carries its 50 uA, so the worst drop is J1 to J2 at the net's 2 A and not the
    // net's 2 A through R1's 30 mm of 0.2 mm.
    let (trunk, branch) = trunk_and_divider();
    let pads = [
        pad("J1", "1", Pt::new(0.0, 0.0)),
        pad("J2", "1", Pt::new(40.0, 0.0)),
        drawing("R1", "1", Pt::new(20.0, 30.0), 50e-6),
    ];
    let d = track_drop_of("P", 2.0, &[&trunk, &branch], &[], &pads, OZ).unwrap();
    assert_eq!((d.from.refdes.as_str(), d.to.refdes.as_str()), ("J1", "J2"));
    assert!((d.drop() - 2.0 * ohms(40.0, 1.0)).abs() < 1e-12, "{d:?}");
    assert!(d.whole_current());

    // Without the divider's current it would be 2 A through the 0.2 mm branch.
    let unknown = [
        pad("J1", "1", Pt::new(0.0, 0.0)),
        pad("J2", "1", Pt::new(40.0, 0.0)),
        pad("R1", "1", Pt::new(20.0, 30.0)),
    ];
    let d = track_drop_of("P", 2.0, &[&trunk, &branch], &[], &unknown, OZ).unwrap();
    assert_eq!(d.to.refdes, "R1");
    let whole = 2.0 * (ohms(20.0, 1.0) + ohms(30.0, 0.2));
    assert!((d.drop() - whole).abs() < 1e-12, "{d:?}");
}

#[test]
fn the_drop_to_a_divider_sums_its_own_current_and_the_trunks_along_the_path() {
    // Only the supply's header is of no known current: J2 draws 1.5 A, R1 50 uA. From J1 the
    // trunk's first 20 mm carry both, its last 20 mm J2's 1.5 A, and the branch R1's 50 uA;
    // the worst drop is to J2.
    let (trunk, branch) = trunk_and_divider();
    let pads = [
        pad("J1", "1", Pt::new(0.0, 0.0)),
        drawing("J2", "1", Pt::new(40.0, 0.0), 1.5),
        drawing("R1", "1", Pt::new(20.0, 30.0), 50e-6),
    ];
    let d = track_drop_of("P", 2.0, &[&trunk, &branch], &[], &pads, OZ).unwrap();
    let first = (1.5 + 50e-6) * ohms(20.0, 1.0);
    assert_eq!(d.to.refdes, "J2");
    assert!(
        (d.drop() - (first + 1.5 * ohms(20.0, 1.0))).abs() < 1e-12,
        "{d:?}"
    );
    assert!((d.current - (1.5 + 50e-6)).abs() < 1e-12);
    assert!(!d.whole_current());
    assert_eq!(d.runs.len(), 2);
    assert!((d.runs[1].current - 1.5).abs() < 1e-12, "{:?}", d.runs);
    // R1's own drop is the trunk's first run and its branch at 50 uA, less than J2's.
    let to_r1 = first + 50e-6 * ohms(30.0, 0.2);
    assert!(to_r1 < d.drop());
    // The width that brings it within a budget widens the runs at the current each carries.
    let w = d.width_for(0.5 * d.drop()).unwrap();
    assert!((1.9..=2.1).contains(&w), "{w}");
}

#[test]
fn a_pin_of_two_pads_draws_its_current_once() {
    // R1 is a pin of two pads, each on the branch's end; it draws 1 A once, not twice.
    let (trunk, branch) = trunk_and_divider();
    let mut second = drawing("R1", "1", Pt::new(20.0, 30.0), 1.0);
    second.pad.number = "2".into();
    let pads = [
        pad("J1", "1", Pt::new(0.0, 0.0)),
        drawing("R1", "1", Pt::new(20.0, 30.0), 1.0),
        second,
    ];
    let d = track_drop_of("P", 2.0, &[&trunk, &branch], &[], &pads, OZ).unwrap();
    let r = ohms(20.0, 1.0) + ohms(30.0, 0.2);
    assert!((d.drop() - r).abs() < 1e-12, "{d:?}");
}
