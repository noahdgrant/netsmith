//! `pour_replaced` (issue #1612): a pour of the process that a zone of the routes file replaces
//! on its layer is a note, which fails nothing, and a `via_dangling` finding of a via of its net
//! says the pour is replaced there. The board file holds the zone and no pour, so the caller
//! tells the check ([`Options::replaced_pours`]).

use crate::common;

use common::*;
use netsmith_fab::replaced_pour::ReplacedPour;
use netsmith_fab::{Checked, Options};
use netsmith_geom::{DrcRule, DrcSeverity, Pt};

/// A via of net V at (10, 10) with a track to a pad on the front, and a zone of V on the back
/// away from it (the routes file's zone, as the board file holds it, with no pour of the
/// process).
fn board() -> Pcb {
    let mut b = Pcb::new(2);
    b.stub("V", 6.0, 10.0);
    b.track("V", "F.Cu", (6.0, 10.0), (10.0, 10.0), 0.2);
    b.via("V", (10.0, 10.0), 0.5, 0.2);
    b.zone("V", "B.Cu", (20.0, 20.0, 30.0, 25.0));
    b
}

fn replaced() -> ReplacedPour {
    ReplacedPour {
        layer: "B.Cu".into(),
        net: "V".into(),
        kind: "pour",
        zones: vec!["20,20,10,5".into()],
        at: Pt::new(20.0, 20.0),
    }
}

fn check(b: &mut Pcb, replaced_pours: Vec<ReplacedPour>) -> Checked {
    let (pcb, pro) = (b.pcb(), b.pro());
    let options = Options {
        replaced_pours,
        ..Options::default()
    };
    netsmith_fab::check(&pcb, Some(&pro), &options).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_replaced_pour_is_a_note_naming_the_layer_the_net_and_the_zone() {
    let c = check(&mut board(), vec![replaced()]);
    let notes: Vec<_> = c
        .report
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::PourReplaced)
        .collect();
    assert_eq!(notes.len(), 1, "{}", c.report);
    let n = notes[0];
    assert_eq!(n.severity, DrcSeverity::Note);
    assert_eq!(n.layer.as_deref(), Some("B.Cu"));
    for word in ["pour of net V on B.Cu", "is replaced there", "zone at 20,20,10,5"] {
        assert!(n.message.contains(word), "{}", n.message);
    }
    assert!(n.to_string().starts_with("note [pour_replaced] "), "{n}");
    // A note fails nothing and no warning counts it.
    assert_eq!(c.report.notes().count(), 1);
    assert!(c.report.warnings().all(|w| w.rule != DrcRule::PourReplaced));
}

#[test]
fn a_via_that_dangles_because_of_it_names_the_replaced_pour() {
    let c = check(&mut board(), vec![replaced()]);
    let dangling: Vec<_> = c
        .report
        .findings
        .iter()
        .filter(|f| f.rule == DrcRule::ViaDangling)
        .collect();
    assert_eq!(dangling.len(), 1, "{}", c.report);
    assert!(
        dangling[0]
            .message
            .ends_with("the process's pour of net V on B.Cu is replaced by the routes file's zone at 20,20,10,5 (X,Y,W,H in mm from the outline's top left corner, as `routes add-zone --rect` takes it), which does not join it"),
        "{}",
        dangling[0].message
    );
}

#[test]
fn a_board_with_no_replaced_pour_has_neither() {
    let c = check(&mut board(), Vec::new());
    assert_eq!(c.report.count(DrcRule::PourReplaced), 0, "{}", c.report);
    let dangling = c
        .report
        .findings
        .iter()
        .find(|f| f.rule == DrcRule::ViaDangling)
        .expect("the via dangles");
    assert!(!dangling.message.contains("replaced"), "{}", dangling.message);
}

#[test]
fn a_via_of_another_net_does_not_name_the_pour() {
    let mut other = replaced();
    other.net = "W".into();
    let c = check(&mut board(), vec![other]);
    let dangling = c
        .report
        .findings
        .iter()
        .find(|f| f.rule == DrcRule::ViaDangling)
        .unwrap();
    assert!(!dangling.message.contains("replaced"), "{}", dangling.message);
}
