//! The system scene (ADR 0110, `netsmith_3d::SystemScene`): two copies of the invented board of
//! `scene3d.rs`, mated by their two-pin rows, with mated heights given by hand. A copy is the
//! carrier, the base; the other, the card, stands on it face down. The mates are keyed, so that
//! pin 1 is looked for on the pads only by the tests of pin 1 at the end.

use netsmith_3d::{SystemCode, MateInput, MemberInput, SystemPlace, SystemScene};
use netsmith_pcb::Board;

use crate::scene3d::{board, scene};

fn mate(a: &str, b: &str, height: Option<(f64, f64)>) -> MateInput {
    let member = |p: &str| p.split_once('.').map(|(m, _)| m.to_string());
    MateInput {
        a: a.into(),
        b: b.into(),
        a_member: member(a),
        b_member: member(b),
        a_seat: height.map(|(lo, hi)| (lo / 2.0, hi / 2.0)),
        b_seat: height.map(|(lo, hi)| (lo / 2.0, hi / 2.0)),
        a_body: None,
        b_body: None,
        mated_height: height,
        unknown: height
            .is_none()
            .then(|| format!("the mate of `{a}` and `{b}` has no mated height")),
        window: None,
        keyed: true,
    }
}

fn system(boards: &[(&str, &Board)], mates: Vec<MateInput>) -> SystemScene {
    let members = boards
        .iter()
        .map(|(name, board)| MemberInput {
            name: name.to_string(),
            board,
            scene: scene(board),
        })
        .collect();
    SystemScene::of("Kit", members, mates)
}

fn codes(s: &SystemScene) -> Vec<(SystemCode, Vec<String>)> {
    s.findings
        .iter()
        .map(|f| (f.code, f.parts.clone()))
        .collect()
}

#[test]
fn the_card_is_flipped_turned_and_lifted_so_that_its_pads_land_on_the_carriers() {
    let (carrier, card) = (board(), board());
    let s = system(
        &[("carrier", &carrier), ("card", &card)],
        vec![mate("carrier.row", "card.row", Some((5.0, 5.4)))],
    );
    let base = s.member("carrier").unwrap();
    assert_eq!(base.place, SystemPlace::Base);
    let m = s.member("card").unwrap();
    assert_eq!(m.place, SystemPlace::Mated { mate: 0 });
    // Both rows stand on the top face, so the card is turned over; its row runs the other way
    // once flipped, so it is turned half round, and its top face is the least mated height up.
    assert!(m.motion.flip);
    assert_eq!(m.motion.turn, 180.0);
    assert_eq!(m.motion.offset, [50.0, 0.0, 5.0]);
    assert_eq!(s.mates[0].residual, Some(0.0));
    assert_eq!(s.mates[0].gap, Some(5.0));
    assert_eq!(s.mates[0].contacts, 2);
    // Nothing collides at 5 mm; the rows have no height, and each faces the other board.
    assert!(
        s.findings.iter().all(|f| f.code == SystemCode::SystemUnchecked),
        "{:?}",
        codes(&s)
    );
    let unchecked: Vec<&str> = s.findings.iter().map(|f| f.parts[0].as_str()).collect();
    assert_eq!(unchecked, ["carrier/row", "card/row"]);
}

#[test]
fn a_body_that_reaches_the_other_board_collides_with_it() {
    let (carrier, card) = (board(), board());
    // At 2.5 mm the card's 3 mm `b` (catalog) hangs 0.5 mm into the carrier, and the carrier's
    // 3 mm `b` reaches 0.5 mm into the card.
    let s = system(
        &[("carrier", &carrier), ("card", &card)],
        vec![mate("carrier.row", "card.row", Some((2.5, 2.9)))],
    );
    let collisions: Vec<&Vec<String>> = s
        .findings
        .iter()
        .filter(|f| f.code == SystemCode::SystemCollision)
        .map(|f| &f.parts)
        .collect();
    assert!(
        collisions.contains(&&vec!["carrier/".to_string(), "card/b".to_string()]),
        "{collisions:?}"
    );
    assert!(
        collisions.contains(&&vec!["carrier/b".to_string(), "card/".to_string()]),
        "{collisions:?}"
    );
    let f = s
        .findings
        .iter()
        .find(|f| f.parts == ["carrier/", "card/b"])
        .unwrap();
    assert_eq!(f.z, Some((-0.5, 0.0)));
    assert!(
        f.code.error() && f.message.contains("the board `carrier`"),
        "{}",
        f.message
    );
}

#[test]
fn a_second_mate_whose_pads_do_not_meet_is_misaligned() {
    let (carrier, card) = (board(), board());
    // The rows place the card; the boxes `a` are then 30 mm apart, and cannot mate.
    let s = system(
        &[("carrier", &carrier), ("card", &card)],
        vec![
            mate("carrier.row", "card.row", Some((5.0, 5.4))),
            mate("carrier.a", "card.a", Some((5.0, 5.4))),
        ],
    );
    let bad: Vec<_> = s
        .findings
        .iter()
        .filter(|f| f.code == SystemCode::MateMisaligned)
        .collect();
    assert_eq!(bad.len(), 1, "{:?}", codes(&s));
    assert_eq!(
        bad[0].mate,
        Some(("carrier.a".to_string(), "card.a".to_string()))
    );
    assert!(bad[0].distance.unwrap() > 25.0);
    assert!(bad[0].code.error());
    assert_eq!(s.mates[0].residual, Some(0.0));
}

#[test]
fn a_member_no_mate_places_is_drawn_apart_with_its_reason() {
    let (carrier, card, spare) = (board(), board(), board());
    let s = system(
        &[("carrier", &carrier), ("card", &card), ("spare", &spare)],
        vec![mate("carrier.row", "card.row", None)],
    );
    let card = s.member("card").unwrap();
    let SystemPlace::Apart { reason } = &card.place else {
        panic!("{:?}", card.place)
    };
    assert!(reason.contains("no mated height"), "{reason}");
    // Beside the carrier along +x, 10 mm past its bounds (30 mm), unturned.
    assert_eq!(card.motion.offset, [40.0, 0.0, 0.0]);
    assert!(!card.motion.flip && card.motion.turn == 0.0);
    let spare = s.member("spare").unwrap();
    assert_eq!(spare.motion.offset, [80.0, 0.0, 0.0]);
    assert!(matches!(&spare.place, SystemPlace::Apart { reason } if reason.contains("no mate joins it")));
    let apart: Vec<&str> = s
        .findings
        .iter()
        .filter(|f| f.code == SystemCode::SystemApart)
        .map(|f| f.members[0].as_str())
        .collect();
    assert_eq!(apart, ["card", "spare"]);
    // Nothing across boards is checked for a member apart: no other finding.
    assert!(s.findings.iter().all(|f| f.code == SystemCode::SystemApart));
    assert!(!SystemCode::SystemApart.error() && SystemCode::SystemApart.is_error(true));
}

#[test]
fn a_partial_mate_pairs_the_pads_of_its_window() {
    let (carrier, card) = (board(), board());
    // A window of both pads from pad 1 pairs them as a mate of every contact does.
    let mut whole = mate("carrier.row", "card.row", Some((5.0, 5.4)));
    whole.window = Some((1, 1, 2));
    let s = system(&[("carrier", &carrier), ("card", &card)], vec![whole]);
    assert_eq!(s.mates[0].contacts, 2);
    assert_eq!(s.member("card").unwrap().place, SystemPlace::Mated { mate: 0 });
    // Pad 2 of the carrier's row on pad 1 of the card's: one pair, too few to line them up.
    let mut shifted = mate("carrier.row", "card.row", Some((5.0, 5.4)));
    shifted.window = Some((2, 1, 1));
    let s = system(&[("carrier", &carrier), ("card", &card)], vec![shifted]);
    assert_eq!(s.mates[0].contacts, 1);
    match &s.member("card").unwrap().place {
        SystemPlace::Apart { reason } => {
            assert!(reason.contains("have 1 pad of one number"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
}

/// A mate of the two rows that neither connector keys.
fn unkeyed() -> MateInput {
    MateInput {
        keyed: false,
        ..mate("carrier.row", "card.row", Some((5.0, 5.4)))
    }
}

fn unmarked(s: &SystemScene) -> Vec<&netsmith_3d::SystemFinding> {
    s.findings
        .iter()
        .filter(|f| f.code == SystemCode::Pin1Unmarked)
        .collect()
}

#[test]
fn pin_1_of_an_unkeyed_mate_that_looks_like_its_other_end_is_a_guideline() {
    // Both pads of each row are round, 1.7 mm across, with a 1 mm hole: turned end for end,
    // pad 1 lands where pad 2 is and looks the same.
    let (carrier, card) = (board(), board());
    let s = system(&[("carrier", &carrier), ("card", &card)], vec![unkeyed()]);
    let found = unmarked(&s);
    let parts: Vec<&str> = found.iter().map(|f| f.parts[0].as_str()).collect();
    assert_eq!(parts, ["carrier/row", "card/row"]);
    let f = found[0];
    assert!(
        f.message.contains("pin 1 of `carrier.row` cannot be told from its pads")
            && f.message.contains("lands where pad 2 is")
            && f.message.contains("round pads 1.70 x 1.70 mm")
            && f.message.contains("`.mating.keyed`"),
        "{}",
        f.message
    );
    assert_eq!(
        f.mate,
        Some(("carrier.row".to_string(), "card.row".to_string()))
    );
    assert_eq!(f.code.word(), "pin1_unmarked");
    assert!(!f.code.error() && f.code.guideline() && f.code.severity() == "warning");
    // A keyed mate goes together one way, so its pads are not looked at.
    let s = system(
        &[("carrier", &carrier), ("card", &card)],
        vec![mate("carrier.row", "card.row", Some((5.0, 5.4)))],
    );
    assert!(unmarked(&s).is_empty(), "{:?}", codes(&s));
}

/// Pad 1 of the row of `b`.
fn pad_1(b: &mut Board) -> &mut netsmith_pcb::Pad {
    b.parts
        .iter_mut()
        .find(|p| p.path == "row")
        .unwrap()
        .footprint
        .pads
        .iter_mut()
        .find(|p| p.number == "1")
        .unwrap()
}

#[test]
fn a_pad_1_of_another_shape_or_size_marks_pin_1() {
    let (carrier, mut card) = (board(), board());
    // The card's pad 1 square, as a std header's is: only the carrier's row is reported.
    pad_1(&mut card).shape = netsmith_pcb::PadShape::Rect;
    let s = system(&[("carrier", &carrier), ("card", &card)], vec![unkeyed()]);
    let parts: Vec<&str> = unmarked(&s).iter().map(|f| f.parts[0].as_str()).collect();
    assert_eq!(parts, ["carrier/row"]);
    // A round pad 1 that is larger marks it too.
    let mut card = board();
    let one = pad_1(&mut card);
    (one.width, one.height) = (2.0, 2.0);
    let s = system(&[("carrier", &carrier), ("card", &card)], vec![unkeyed()]);
    let parts: Vec<&str> = unmarked(&s).iter().map(|f| f.parts[0].as_str()).collect();
    assert_eq!(parts, ["carrier/row"]);
}

#[test]
fn pads_that_do_not_turn_onto_pads_key_the_connector() {
    // A third pad beside pad 1: turned half round, the pads land where none is, so the
    // connector goes in one way and its pin 1 needs no mark.
    let (carrier, mut card) = (board(), board());
    let pads = &mut card
        .parts
        .iter_mut()
        .find(|p| p.path == "row")
        .unwrap()
        .footprint
        .pads;
    let mut third = pads[0].clone();
    third.number = "3".into();
    third.at.x += 2.54;
    pads.push(third);
    let s = system(&[("carrier", &carrier), ("card", &card)], vec![unkeyed()]);
    let parts: Vec<&str> = unmarked(&s).iter().map(|f| f.parts[0].as_str()).collect();
    assert_eq!(parts, ["carrier/row"]);
}

/// The carrier's row, 5.4 mm tall, seated 5.4 +/- 0.4 mm, with `body` as its
/// `.mating.body_height`, mated with the card's row, 0.2 mm tall and seated at 0.2 mm: the
/// boards' faces are 5.2 mm apart at the least.
fn seated(body: Option<(f64, f64)>) -> SystemScene {
    let (mut carrier, mut card) = (board(), board());
    for (b, h) in [(&mut carrier, 5.4), (&mut card, 0.2)] {
        b.parts.iter_mut().find(|p| p.path == "row").unwrap().height = Some(h);
    }
    let input = MateInput {
        a_seat: Some((5.0, 5.8)),
        b_seat: Some((0.2, 0.2)),
        a_body: body,
        mated_height: Some((5.2, 6.0)),
        ..mate("carrier.row", "card.row", Some((5.2, 6.0)))
    };
    system(&[("carrier", &carrier), ("card", &card)], vec![input])
}

fn row_collisions(s: &SystemScene) -> Vec<&netsmith_3d::SystemFinding> {
    s.findings
        .iter()
        .filter(|f| f.code == SystemCode::SystemCollision && f.parts.contains(&"carrier/row".to_string()))
        .collect()
}

#[test]
fn a_connectors_body_is_its_seat_and_keeps_out_of_the_other_board_for_the_whole_tolerance() {
    // The row's 5.4 mm height is its seat's middle, and the scene places the card at the least
    // mated height, 5.2 mm: the body is the seat, one surface with one tolerance, so it moves
    // with the seat and the card, resting on its partner, never meets it (issue 1324).
    let s = seated(None);
    assert_eq!(s.member("card").unwrap().motion.offset[2], 5.2);
    assert!(row_collisions(&s).is_empty(), "{:?}", codes(&s));
}

#[test]
fn a_stated_body_height_is_kept_out_of_the_other_board_at_its_most() {
    // A body stated apart from the seat reaches to its most, 5.4 mm, 0.2 mm into the card at
    // the least mated height.
    let s = seated(Some((5.4, 5.4)));
    let [f] = &row_collisions(&s)[..] else {
        panic!("{:?}", codes(&s))
    };
    assert_eq!(f.parts, ["carrier/row", "card/"]);
    assert_eq!(f.z, Some((5.2, 5.4)));
    assert!(
        f.code.error()
            && f.message.contains("`carrier.row.mating.body_height` reaches 5.400 mm at its most")
            && f.message.contains("5.200 mm apart at the least mated height"),
        "{}",
        f.message
    );
    // A body whose most is below the least mated height stays out.
    let s = seated(Some((4.9, 5.1)));
    assert!(row_collisions(&s).is_empty(), "{:?}", codes(&s));
}
