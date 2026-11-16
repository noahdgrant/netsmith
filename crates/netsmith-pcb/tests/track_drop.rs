//! The voltage a net's routed tracks drop (issue #652): the resistance between two pads along
//! the tracks, measured by hand.

use netsmith_pcb::{COPPER_RESISTIVITY, CopperLayer, NetPad, PadRef, Pt, Track, track_drop_of};

const OZ: f64 = 0.035;

fn pad(refdes: &str, number: &str, at: Pt) -> NetPad {
    NetPad {
        pad: PadRef {
            refdes: refdes.into(),
            number: number.into(),
        },
        min: Pt::new(at.x - 0.5, at.y - 0.5),
        max: Pt::new(at.x + 0.5, at.y + 0.5),
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
