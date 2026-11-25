//! `routes check-via` (issue #1643): a via 0.05 mm too near a track of another net is refused
//! with the track, its net, the space required and measured and the shortfall; a via with room
//! is legal and has its smallest margins.

use netsmith_fabricate::routes::{self, Edit};
use netsmith_fabricate::via_check::{self, Clash};
use netsmith_pcb::{Board, CopperLayer, Pt, Rect, Routes};

/// The LED board, grown, with a track of GND across its empty part at y = 15 (the routes
/// file's frame), 0.2 mm wide.
fn board_with_track() -> (Board, Routes) {
    let mut board: Board = serde_json::from_str(include_str!(
        "../../netsmith-pcb/tests/fixtures/led.board.json"
    ))
    .expect("a fixture is the JSON of a Board");
    board.outline = Rect::new(Pt::new(20.0, 20.0), Pt::new(52.0, 40.0));
    let mut routes = Routes::default();
    routes::edit(
        &board,
        &mut routes,
        &Edit::AddTrack {
            net: "GND".to_string(),
            layer: CopperLayer::Front,
            width: Some(0.2),
            from: [4.0, 15.0],
            mid: None,
            to: [14.0, 15.0],
            lock: false,
        },
    )
    .expect("the track keeps the rules' space");
    (board, routes)
}

/// The y of a via whose copper is `gap` mm from the track's edge.
fn y_at(board: &Board, gap: f64) -> f64 {
    let via = routes::via_offer(board, None).expect("a through via");
    15.0 + 0.1 + via.diameter / 2.0 + gap
}

fn track_clearance(found: &[Clash]) -> &Clash {
    found
        .iter()
        .find(|c| c.rule == "clearance" && c.item == "track")
        .unwrap_or_else(|| panic!("no clearance to the track in {found:?}"))
}

#[test]
fn a_via_too_near_a_track_names_the_track_its_net_and_the_shortfall() {
    let (board, routes) = board_with_track();
    let least = board.rules.clearance;
    let at = [9.0, y_at(&board, least - 0.05)];
    let checked =
        via_check::check_via(&board, &routes, at, Some("VCC"), None, None, None).expect("checked");
    eprintln!("{}", checked.text());
    assert!(!checked.legal);
    let c = track_clearance(&checked.breaks);
    assert_eq!(c.net.as_deref(), Some("GND"));
    assert!((c.required.unwrap() - least).abs() < 1e-4, "{c:?}");
    assert!((c.measured.unwrap() - (least - 0.05)).abs() < 1e-4, "{c:?}");
    assert!((c.shortfall.unwrap() - 0.05).abs() < 1e-4, "{c:?}");
    let text = checked.text();
    assert!(text.contains("refused"), "{text}");
    assert!(text.contains("short by 0.0500 mm"), "{text}");
}

#[test]
fn a_legal_via_lists_its_smallest_margins() {
    let (board, routes) = board_with_track();
    let least = board.rules.clearance;
    let at = [9.0, y_at(&board, least + 0.1)];
    let checked =
        via_check::check_via(&board, &routes, at, Some("VCC"), None, None, None).expect("checked");
    eprintln!("{}", checked.text());
    assert!(checked.legal, "{}", checked.text());
    assert!(checked.breaks.is_empty());
    let c = track_clearance(&checked.margins);
    assert_eq!(c.net.as_deref(), Some("GND"));
    assert!((c.measured.unwrap() - (least + 0.1)).abs() < 1e-4, "{c:?}");
    assert!((c.shortfall.unwrap() + 0.1).abs() < 1e-4, "{c:?}");
    // The least room first.
    let rooms: Vec<f64> = checked
        .margins
        .iter()
        .filter_map(|m| m.shortfall)
        .collect();
    assert!(rooms.windows(2).all(|w| w[0] >= w[1]), "{rooms:?}");
    assert!(checked.text().contains("to spare"), "{}", checked.text());
    // A via of the track's own net is not judged against it.
    let own =
        via_check::check_via(&board, &routes, at, Some("GND"), None, None, None).expect("checked");
    assert!(
        !own.margins
            .iter()
            .any(|m| m.item == "track" && m.net.as_deref() == Some("GND")),
        "{:?}",
        own.margins
    );
}

#[test]
fn a_net_the_board_lacks_is_refused() {
    let (board, routes) = board_with_track();
    assert!(matches!(
        via_check::check_via(&board, &routes, [9.0, 10.0], Some("NOPE"), None, None, None),
        Err(routes::Refusal::Refused(_))
    ));
}
