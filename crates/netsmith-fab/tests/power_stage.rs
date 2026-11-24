//! The power stage of a switching regulator (issue #1585), measured on a board drawn by hand: a
//! buck IC `U1` with its input capacitor's loop, a boot capacitor routed long through two vias,
//! its feedback route, and a via of a signal under the IC beside its switch-node pad.

use crate::common;

use common::*;
use netsmith_core::{RegulatorPin, SupportCap, SwitchingRegulator};
use netsmith_fab::power_stage::{self, DEFAULT_SWITCH_NODE_KEEPOUT, PowerStage};
use netsmith_geom::Artwork;

fn pin(net: &str, pads: &[&str]) -> RegulatorPin {
    RegulatorPin {
        net: net.to_string(),
        pads: pads.iter().map(|p| p.to_string()).collect(),
    }
}

/// `U1` as the design names it: VIN on pads 3 and 5 (EN tied to VIN), ground 1, the switch
/// node 2, feedback 4 and the boot pin 6, with `C1` across the input and `C2` from BOOT to SW.
fn regulator() -> SwitchingRegulator {
    SwitchingRegulator {
        refdes: "U1".into(),
        path: "reg.ic".into(),
        vin: pin("VIN", &["3", "5"]),
        gnd: pin("GND", &["1"]),
        sw: pin("SW", &["2"]),
        fb: Some(pin("FB", &["4"])),
        input_caps: vec!["C1".into()],
        support: vec![SupportCap {
            pin: "BOOT".into(),
            pads: vec!["6".into()],
            net: "BST".into(),
            cap: "C2".into(),
            boot: true,
        }],
        switch_parts: vec!["L1".into(), "C2".into()],
    }
}

fn bare() -> Draw {
    Draw {
        courtyard: None,
        silk: None,
        back: false,
    }
}

/// The IC at (20, 20) with pads 0.6 by 0.4 mm at x = 18.9 (1 GND, 2 SW, 3 VIN, from the top) and
/// x = 21.1 (6 BOOT, 5 EN on VIN, 4 FB); `C1` at (16, 23), `C2` at (30, 20), the inductor at
/// (13, 17) and the divider's top at (24, 23). The input loop: VIN on the front 3.4 + 2.05 mm,
/// ground on the back through two vias, 1.4 + 3.95 + 1 mm. The boot route: 2.9 mm on the front,
/// 5.5 on the back, 0.95 on the front. The feedback route: 2.4 + 2.05 mm.
fn board() -> Pcb {
    let mut b = Pcb::new(2);
    b.part(
        "U1",
        (20.0, 20.0),
        0.0,
        &[
            smd("1", -1.1, -0.95, 0.6, 0.4, "GND"),
            smd("2", -1.1, 0.0, 0.6, 0.4, "SW"),
            smd("3", -1.1, 0.95, 0.6, 0.4, "VIN"),
            smd("4", 1.1, 0.95, 0.6, 0.4, "FB"),
            smd("5", 1.1, 0.0, 0.6, 0.4, "VIN"),
            smd("6", 1.1, -0.95, 0.6, 0.4, "BST"),
        ],
        bare(),
    );
    let two = |a: &'static str, b: &'static str| {
        [
            smd("1", -0.5, 0.0, 0.5, 0.5, a),
            smd("2", 0.5, 0.0, 0.5, 0.5, b),
        ]
    };
    b.part("C1", (16.0, 23.0), 0.0, &two("VIN", "GND"), bare());
    b.part("C2", (30.0, 20.0), 0.0, &two("BST", "SW"), bare());
    b.part("L1", (13.0, 17.0), 0.0, &two("VOUT", "SW"), bare());
    b.part("R1", (24.0, 23.0), 0.0, &two("FB", "VOUT"), bare());
    // The input loop.
    b.track("VIN", "F.Cu", (18.9, 20.95), (15.5, 20.95), 0.3);
    b.track("VIN", "F.Cu", (15.5, 20.95), (15.5, 23.0), 0.3);
    b.track("GND", "F.Cu", (18.9, 19.05), (17.5, 19.05), 0.3);
    b.via("GND", (17.5, 19.05), 0.5, 0.3);
    b.track("GND", "B.Cu", (17.5, 19.05), (17.5, 23.0), 0.3);
    b.via("GND", (17.5, 23.0), 0.5, 0.3);
    b.track("GND", "F.Cu", (17.5, 23.0), (16.5, 23.0), 0.3);
    // The boot capacitor, routed long.
    b.track("BST", "F.Cu", (21.1, 19.05), (24.0, 19.05), 0.2);
    b.via("BST", (24.0, 19.05), 0.5, 0.3);
    b.track("BST", "B.Cu", (24.0, 19.05), (29.5, 19.05), 0.2);
    b.via("BST", (29.5, 19.05), 0.5, 0.3);
    b.track("BST", "F.Cu", (29.5, 19.05), (29.5, 20.0), 0.2);
    // The feedback route.
    b.track("FB", "F.Cu", (21.1, 20.95), (23.5, 20.95), 0.2);
    b.track("FB", "F.Cu", (23.5, 20.95), (23.5, 23.0), 0.2);
    // A signal far from the stage.
    b.stub("FAR", 5.0, 28.0);
    b.stub("FAR", 9.0, 28.0);
    b.track("FAR", "F.Cu", (5.0, 28.0), (9.0, 28.0), 0.2);
    b
}

fn stages(b: &mut Pcb) -> Vec<PowerStage> {
    let art = Artwork::parse(&b.pcb()).unwrap();
    power_stage::of(&art, &[regulator()], DEFAULT_SWITCH_NODE_KEEPOUT)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn the_stage_has_its_input_loop_boot_route_and_feedback() {
    let s = stages(&mut board());
    assert_eq!(s.len(), 1);
    let s = &s[0];
    assert_eq!((s.vin.as_str(), s.gnd.as_str(), s.sw.as_str()), ("VIN", "GND", "SW"));
    // The input loop: centre to centre, and along the tracks of each net.
    let c = &s.input_caps[0];
    assert_eq!((c.vin.from.as_str(), c.vin.to.as_str()), ("U1.3", "C1.1"));
    assert!(close(c.vin.distance, 3.4f64.hypot(2.05)), "{c:?}");
    assert!(close(c.vin.routed.unwrap(), 3.4 + 2.05), "{c:?}");
    assert_eq!(c.vin.vias, Some(0));
    assert_eq!((c.gnd.from.as_str(), c.gnd.to.as_str()), ("U1.1", "C1.2"));
    assert!(close(c.gnd.distance, 2.4f64.hypot(3.95)), "{c:?}");
    assert!(close(c.gnd.routed.unwrap(), 1.4 + 3.95 + 1.0), "{c:?}");
    assert_eq!(c.gnd.vias, Some(2));
    // The boot capacitor's long route, through two vias.
    let boot = &s.support[0];
    assert!(boot.boot && boot.pin == "BOOT" && boot.cap == "C2", "{boot:?}");
    assert!(close(boot.leg.routed.unwrap(), 2.9 + 5.5 + 0.95), "{boot:?}");
    assert_eq!(boot.leg.vias, Some(2));
    assert!(close(boot.leg.distance, 8.4f64.hypot(0.95)), "{boot:?}");
    // The feedback route.
    let fb = s.feedback.as_ref().unwrap();
    assert!(close(fb.length, 2.4 + 2.05) && fb.vias == 0, "{fb:?}");
    // No copper of another net near the switch node: the far signal is not, and the nets of
    // the IC, the inductor and the boot capacitor are the stage's own.
    assert!(s.foreign.is_empty(), "{:?}", s.foreign);
    assert!(power_stage::notes(std::slice::from_ref(s)).is_empty());
    let text = power_stage::text(std::slice::from_ref(s));
    println!("{text}");
    assert!(text.contains("input C1: VIN U1.3 to C1.1 3.97 mm apart, routed 5.45 mm with 0 via(s)"), "{text}");
    assert!(text.contains("boot C2 on BOOT: U1.6 to C2.1 8.45 mm apart, routed 9.35 mm with 2 via(s)"), "{text}");
    assert!(text.contains("feedback FB: routed 4.45 mm with 0 via(s)"), "{text}");
    assert!(text.contains("foreign copper: none within 1 mm"), "{text}");
}

#[test]
fn a_signal_via_under_the_ic_beside_the_switch_node_is_noted() {
    let mut b = board();
    // 0.27 mm from the SW pad's edge (x = 19.2), edge to edge, between VIN and ground.
    b.stub("SIG", 35.0, 5.0);
    b.via("SIG", (19.72, 20.0), 0.5, 0.3);
    let s = stages(&mut b);
    let f = &s[0].foreign;
    assert_eq!(f.len(), 1, "{f:?}");
    assert_eq!((f[0].kind, f[0].net.as_str()), ("via", "SIG"));
    assert!(close(f[0].gap.unwrap(), 0.27), "{f:?}");
    assert!(f[0].under_body, "{f:?}");
    let notes = power_stage::notes(&s);
    assert_eq!(notes.len(), 1);
    println!("{}", notes[0].message);
    assert!(
        notes[0]
            .message
            .starts_with("via of SIG on F.Cu-B.Cu at (19.72, 20) is 0.27 mm from the switch node SW of U1, under U1's body (keepout 1 mm)"),
        "{}",
        notes[0].message
    );
    // A track of a signal 0.8 mm from the inductor's switch-node pad, on the same layer, is
    // within the keepout too; at 1.5 mm it is not.
    let mut b = board();
    b.stub("NEAR", 30.0, 26.0);
    b.track("NEAR", "F.Cu", (12.0, 17.25 + 0.8 + 0.1), (14.0, 17.25 + 0.8 + 0.1), 0.2);
    b.track("NEAR", "F.Cu", (12.0, 17.25 + 1.5 + 0.1), (14.0, 17.25 + 1.5 + 0.1), 0.2);
    let f = &stages(&mut b)[0].foreign;
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(close(f[0].gap.unwrap(), 0.8) && !f[0].under_body, "{f:?}");
}

#[test]
fn a_plane_between_shields_a_track_under_the_ic() {
    // On a four-layer board, a signal on the back under the IC is facing the switch node with
    // nothing between; a pour on an inner layer over the place shields it.
    let mut b = board();
    b.layers = 4;
    b.stub("SIG", 35.0, 5.0);
    b.track("SIG", "B.Cu", (17.0, 20.5), (23.0, 20.5), 0.2);
    let f = &stages(&mut b)[0].foreign;
    assert_eq!(f.len(), 1, "{f:?}");
    assert!(f[0].under_body && f[0].layer == "B.Cu", "{f:?}");
    b.zone("GND", "In1.Cu", (10.0, 10.0, 30.0, 30.0));
    assert!(stages(&mut b)[0].foreign.is_empty());
}
