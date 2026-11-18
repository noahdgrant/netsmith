//! `via_dangling` (issue #1185): a via that the copper of its net joins on fewer than two layers
//! leads nowhere. netsmith counts the layers the touching copper is on; KiCad counts a via or a
//! through-hole pad that the via touches on its top layer alone, so it calls a via of a cluster
//! side by side dangling, and the cross-check leaves that finding of KiCad's out. The test that
//! runs KiCad skips itself where it is not installed.

use crate::common;

use common::*;
use netsmith_fab::{compare_kicad_drc, drop_dangling_vias_of_clusters};
use netsmith_geom::{Artwork, DrcExtras, DrcReport, DrcRule, drc_check};

/// A via of net V at (10, 10) with a track to a pad on the front; with `back`, a track to a pad
/// on the back too. With `beside`, a second via of the net beside it, their copper overlapping,
/// joined to nothing else.
fn board(back: bool, beside: bool) -> Pcb {
    let mut b = Pcb::new(2);
    b.stub("V", 6.0, 10.0);
    b.track("V", "F.Cu", (6.0, 10.0), (10.0, 10.0), 0.2);
    if back {
        b.back_stub("V", 14.0, 10.0);
        b.track("V", "B.Cu", (10.0, 10.0), (14.0, 10.0), 0.2);
    }
    b.via("V", (10.0, 10.0), 0.5, 0.2);
    if beside {
        b.via("V", (10.0, 10.45), 0.5, 0.2);
    }
    b
}

fn check(b: &mut Pcb) -> DrcReport {
    let (pcb, pro) = (b.pcb(), b.pro());
    drc_check(&pcb, Some(&pro), &DrcExtras::default()).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_via_joined_on_one_layer_dangles_and_one_joined_on_two_does_not() {
    let r = check(&mut board(false, false));
    assert_eq!(r.count(DrcRule::ViaDangling), 1, "{r}");
    let f = r.findings.iter().find(|f| f.rule == DrcRule::ViaDangling).unwrap();
    assert!(f.message.contains("on 1 layer(s)"), "{}", f.message);
    assert_eq!(check(&mut board(true, false)).count(DrcRule::ViaDangling), 0);
}

#[test]
fn a_via_beside_a_via_of_its_net_is_joined_on_every_layer_its_neighbours_copper_is() {
    // Their copper runs together on both layers: the second carries its share of the current.
    let r = check(&mut board(true, true));
    assert_eq!(r.count(DrcRule::ViaDangling), 0, "{r}");
    // Beside a via that dangles itself, it is joined on one layer too.
    assert_eq!(check(&mut board(false, true)).count(DrcRule::ViaDangling), 2);
}

#[test]
fn kicad_calls_a_via_of_a_cluster_dangling_and_the_cross_check_leaves_that_out() {
    let mut b = board(true, true);
    let (pcb, pro) = (b.pcb(), b.pro());
    let Some(mut theirs) = kicad_findings("cluster", &pcb, &pro) else {
        eprintln!("skipped: KiCad is not installed");
        return;
    };
    let ours = check(&mut b);
    let dangling = |k: &[netsmith_fab::KicadDrcReported]| {
        k.iter().filter(|r| r.kind == "via_dangling").count()
    };
    assert_eq!(dangling(&theirs), 1, "{theirs:#?}");
    let art = Artwork::parse(&pcb).unwrap();
    assert_eq!(drop_dangling_vias_of_clusters(&mut theirs, &art, &ours), 1);
    let c = compare_kicad_drc(&ours, &theirs, 0.1);
    assert!(c.agrees(), "{c}");
    // A via that dangles to both is kept and agrees.
    let mut lone = board(false, true);
    let (pcb, pro) = (lone.pcb(), lone.pro());
    let mut theirs = kicad_findings("lone", &pcb, &pro).expect("KiCad ran once already");
    let ours = check(&mut lone);
    let art = Artwork::parse(&pcb).unwrap();
    assert_eq!(drop_dangling_vias_of_clusters(&mut theirs, &art, &ours), 0);
    assert_eq!(dangling(&theirs), 2, "{theirs:#?}");
    let c = compare_kicad_drc(&ours, &theirs, 0.1);
    assert!(c.agrees(), "{c}");
}
