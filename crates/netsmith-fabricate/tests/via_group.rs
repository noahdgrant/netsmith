//! A group of vias side by side is checked together against the via-current check (issue
//! #1114): the three vias of a net whose current needs three are accepted as a group, and one
//! of them alone is refused, as the router's own group is checked.

use netsmith_core::NetCurrent;
use netsmith_fabricate::routes::{Edit, Refusal, edit_all, via_row};
use netsmith_pipeline::Libraries;
use netsmith_pcb::Routes;
use netsmith_solve::Catalogs;

const HDI: &str = include_str!("../../netsmith-tools/tests/fixtures/hdi_dense.nsm");

/// The HDI fixture's board, 10 mm square, whose net GND carries 1.2 A in three vias.
fn board() -> netsmith_pcb::Board {
    let libraries = Libraries::sample();
    let built = netsmith_pipeline::build_with(
        "hdi.nsm",
        HDI,
        &netsmith_check::Loader::std(),
        &Catalogs::jlcpcb(),
        &libraries,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut board = netsmith_placer::board_from_design(
        &built[0].design,
        &libraries.mapping,
        &libraries.footprints,
    )
    .unwrap();
    board.outline.max = board.outline.min + netsmith_pcb::Pt::new(10.0, 10.0);
    board.net_currents = vec![NetCurrent {
        net: "GND".into(),
        current: 1.2,
        least_width: 0.5,
        inner_least_width: None,
        vias: 3,
        source: String::new(),
        ..Default::default()
    }];
    board
}

fn via(at: [f64; 2]) -> Edit {
    Edit::AddVia {
        net: "GND".into(),
        at,
        diameter: None,
        drill: None,
        layers: None,
        lock: false,
    }
}

#[test]
fn three_vias_of_a_net_that_needs_three_are_accepted_as_a_group_and_one_alone_is_refused() {
    let board = board();
    let pitch = board.rules.via_diameter;
    let places = via_row([5.0, 5.0], 3, pitch).unwrap();
    assert_eq!(places.len(), 3);
    assert_eq!(places[1], [5.0, 5.0], "the row is centred on the place");
    assert!((places[2][0] - places[0][0] - 2.0 * pitch).abs() < 1e-5);
    // One alone: the check finds its cluster smaller than the net's current needs.
    let mut routes = Routes::default();
    match edit_all(&board, &mut routes, &[via(places[0])]) {
        Err(Refusal::Refused(why)) => assert!(why.contains("via_current"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(routes, Routes::default(), "a refusal leaves the routes as they were");
    // The three together, as one edit.
    let group: Vec<Edit> = places.iter().map(|&p| via(p)).collect();
    edit_all(&board, &mut routes, &group).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(routes.boards[&board.name].nets["GND"].vias.len(), 3);
    // Two of three are still short, and a pitch that leaves them apart is no group.
    let mut routes = Routes::default();
    assert!(matches!(
        edit_all(&board, &mut routes, &group[..2]),
        Err(Refusal::Refused(_))
    ));
    let apart: Vec<Edit> = via_row([5.0, 5.0], 3, pitch * 3.0)
        .unwrap()
        .into_iter()
        .map(via)
        .collect();
    assert!(matches!(
        edit_all(&board, &mut routes, &apart),
        Err(Refusal::Refused(_))
    ));
    assert_eq!(routes, Routes::default());
}

#[test]
fn a_row_of_vias_has_a_count_and_a_pitch() {
    assert_eq!(via_row([1.0, 2.0], 1, 0.5).unwrap(), vec![[1.0, 2.0]]);
    assert_eq!(
        via_row([1.0, 2.0], 2, 0.5).unwrap(),
        vec![[0.75, 2.0], [1.25, 2.0]]
    );
    for (count, pitch) in [(0, 0.5), (65, 0.5), (3, 0.0), (3, -1.0), (3, f64::NAN)] {
        assert!(via_row([0.0, 0.0], count, pitch).is_err(), "{count} {pitch}");
    }
}
