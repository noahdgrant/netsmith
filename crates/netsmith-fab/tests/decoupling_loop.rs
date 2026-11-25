//! The decoupling loop of a capacitor (issue #1730), measured on a board drawn by hand: an IC
//! `U1` with its supply pin and its ground pin, and its capacitor `C1` close by, routed straight
//! on the front, or moved 7 mm farther off and its ground taken through two vias, over a ground
//! pour on the back.

use crate::common;

use common::*;
use netsmith_fab::decoupling_loop::{self, Decoupled, DecouplingLoop, Side};
use netsmith_geom::Artwork;

fn bare() -> Draw {
    Draw {
        courtyard: None,
        silk: None,
        back: false,
    }
}

/// `C1` between VCC and GND, decoupling `U1`'s pads 1 (VCC) and 2 (GND).
fn cap() -> Decoupled {
    let side = |net: &str, cap_pad: &str, pin: &str| Side {
        net: net.into(),
        cap_pad: cap_pad.into(),
        pins: vec![("U1".into(), pin.into())],
    };
    Decoupled {
        cap: "C1".into(),
        path: "c1".into(),
        supply: side("VCC", "1", "1"),
        ret: side("GND", "2", "2"),
    }
}

/// `U1` at (20, 15), its VCC pad at (19, 15) and its GND pad at (21, 15); `C1` at (20, `y`), its
/// VCC pad at (19.5, y) and its GND pad at (20.5, y). Near (`y` = 18): each leg one track on the
/// front. Far (`y` = 25): VCC down the front (10 + 0.5 mm), GND 5 mm on the front, 5 on the back
/// and 0.5 on the front again, through two vias, 0.2 mm wide. GND is poured on the back.
fn board(far: bool) -> Pcb {
    let y = if far { 25.0 } else { 18.0 };
    let mut b = Pcb::new(2);
    b.part(
        "U1",
        (20.0, 15.0),
        0.0,
        &[
            smd("1", -1.0, 0.0, 0.6, 0.6, "VCC"),
            smd("2", 1.0, 0.0, 0.6, 0.6, "GND"),
        ],
        bare(),
    );
    b.part(
        "C1",
        (20.0, y),
        0.0,
        &[
            smd("1", -0.5, 0.0, 0.5, 0.5, "VCC"),
            smd("2", 0.5, 0.0, 0.5, 0.5, "GND"),
        ],
        bare(),
    );
    if far {
        b.track("VCC", "F.Cu", (19.0, 15.0), (19.0, 25.0), 0.3);
        b.track("VCC", "F.Cu", (19.0, 25.0), (19.5, 25.0), 0.3);
        b.track("GND", "F.Cu", (21.0, 15.0), (21.0, 20.0), 0.25);
        b.via("GND", (21.0, 20.0), 0.5, 0.3);
        b.track("GND", "B.Cu", (21.0, 20.0), (21.0, 25.0), 0.2);
        b.via("GND", (21.0, 25.0), 0.5, 0.3);
        b.track("GND", "F.Cu", (21.0, 25.0), (20.5, 25.0), 0.25);
    } else {
        b.track("VCC", "F.Cu", (19.0, 15.0), (19.5, 18.0), 0.3);
        b.track("GND", "F.Cu", (21.0, 15.0), (20.5, 18.0), 0.25);
    }
    b.zone("GND", "B.Cu", (1.0, 1.0, 39.0, 29.0));
    b
}

fn loops(far: bool) -> Vec<DecouplingLoop> {
    let art = Artwork::parse(&board(far).pcb()).unwrap();
    decoupling_loop::of(&art, &[cap()])
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn a_capacitor_close_by_has_a_short_loop_of_two_legs_over_the_pour() {
    let l = loops(false);
    assert_eq!(l.len(), 1);
    let l = &l[0];
    assert_eq!((l.cap.as_str(), l.part.as_str()), ("C1", "U1"));
    assert!(close(l.at.0, 20.0) && close(l.at.1, 18.0), "{l:?}");
    // The supply leg from the capacitor to the pin it serves, the return leg back.
    assert_eq!((l.supply.from.as_str(), l.supply.to.as_str()), ("C1.1", "U1.1"));
    let r = l.ret.as_ref().unwrap();
    assert_eq!((r.from.as_str(), r.to.as_str()), ("U1.2", "C1.2"));
    let leg = 0.5f64.hypot(3.0);
    assert!(close(l.supply.distance, leg) && close(l.supply.routed.unwrap(), leg), "{l:?}");
    assert!(close(r.routed.unwrap(), leg), "{l:?}");
    assert_eq!((l.supply.vias, r.vias), (Some(0), Some(0)));
    assert!(close(l.total.length, 2.0 * leg) && l.total.complete, "{l:?}");
    assert_eq!(l.total.narrowest, Some(0.25));
    assert_eq!(l.total.vias, 0);
    // The pour on the back is the return copper, the board's thickness below.
    let p = l.return_copper.as_ref().unwrap();
    assert_eq!((p.net.as_str(), p.layer.as_str()), ("GND", "B.Cu"));
    assert!(close(p.distance, 1.6), "{p:?}");
}

/// The test of the issue: the capacitor moved farther from its pin has the longer loop, more
/// vias and a narrower track, and the comparison says so.
#[test]
fn a_capacitor_moved_farther_from_its_pin_has_the_longer_loop_in_the_compare() {
    let (near, far) = (&loops(false)[0], &loops(true)[0]);
    assert!(close(far.supply.routed.unwrap(), 10.5), "{far:?}");
    let r = far.ret.as_ref().unwrap();
    assert!(close(r.routed.unwrap(), 10.5), "{far:?}");
    assert_eq!(r.vias, Some(2));
    assert_eq!(r.narrowest, Some(0.2));
    assert!(close(far.total.length, 21.0) && far.total.vias == 2, "{far:?}");
    let rows = decoupling_loop::compare(&[("C1".into(), near.figures())], std::slice::from_ref(far));
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert!(row.length_change.unwrap() > 10.0, "{row:?}");
    assert_eq!(row.vias_change, Some(2));
    let text = decoupling_loop::compare_text("before.json", &rows);
    assert!(text.contains("C1: loop 6.08 -> 21 mm (+14.92), vias 0 -> 2 (+2), narrowest 0.25 -> 0.2 mm"), "{text}");
    let text = decoupling_loop::text(std::slice::from_ref(far), (0.0, 0.0));
    assert!(text.contains("return: U1.2 to C1.2 on GND, 10.01 mm apart, routed 10.5 mm, narrowest 0.2 mm, 2 via(s)"), "{text}");
}

/// A leg that only the pour joins has no track: the loop says it is not complete, and counts the
/// leg that has one.
#[test]
fn a_return_that_only_the_pour_carries_has_no_track() {
    let mut b = Pcb::new(2);
    b.part(
        "U1",
        (20.0, 15.0),
        0.0,
        &[
            smd("1", -1.0, 0.0, 0.6, 0.6, "VCC"),
            smd("2", 1.0, 0.0, 0.6, 0.6, "GND"),
        ],
        bare(),
    );
    b.part(
        "C1",
        (20.0, 18.0),
        0.0,
        &[
            smd("1", -0.5, 0.0, 0.5, 0.5, "VCC"),
            smd("2", 0.5, 0.0, 0.5, 0.5, "GND"),
        ],
        bare(),
    );
    b.track("VCC", "F.Cu", (19.0, 15.0), (19.5, 18.0), 0.3);
    b.zone("GND", "F.Cu", (1.0, 1.0, 39.0, 29.0));
    let art = Artwork::parse(&b.pcb()).unwrap();
    let l = &decoupling_loop::of(&art, &[cap()])[0];
    let r = l.ret.as_ref().unwrap();
    assert_eq!((r.routed, r.vias), (None, None));
    assert!(!l.total.complete && close(l.total.length, 0.5f64.hypot(3.0)), "{l:?}");
    // A pour on the capacitor's own layer is 0 mm from it.
    let p = l.return_copper.as_ref().unwrap();
    assert_eq!((p.layer.as_str(), p.distance), ("F.Cu", 0.0));
    let text = decoupling_loop::text(std::slice::from_ref(l), (0.0, 0.0));
    assert!(text.contains("no track (a pour or a plane, or open)"), "{text}");
    assert!(text.contains("(not complete: a leg has no track)"), "{text}");
}
