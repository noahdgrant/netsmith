//! The branch of a net whose pin states its current (issue #1113) is routed for that current:
//! the line to its pad is drawn at the branch's width with the branch's vias, the rest of the
//! net at the net's, and netsmith's design rules check, told the branches, passes both (the
//! check's own test, `netsmith-fab`'s `fab_rules`, shows it naming the same copper when it is
//! not told).

use crate::common;

use netsmith_core::{BranchCurrent, NetCurrent, PinRef};
use netsmith_geom::ipc2221::TrackSizing;
use netsmith_geom::{DrcBranch, DrcExtras, DrcRule, drc_check};
use netsmith_pcb::{Board, Ratsnest};
use netsmith_router::{RouteOptions, route_with};

use common::synthetic_board;

/// JLCPCB's two-layer copper and via, at 10 K and its 20 % width tolerance.
const SIZING: TrackSizing = TrackSizing {
    copper: 0.035,
    inner_copper: None,
    rise: 10.0,
    tolerance: 0.2,
    via_drill: 0.3,
    via_plating: 0.018,
};

/// The board of `seed`, with its first signal net of three pads made a rail of 2 A (0.98 mm and
/// four vias side by side), and the pad of that net farthest from the others a branch of 1 mA:
/// the net and the branch's pad. `None` when the board has no such net.
fn with_a_branch(seed: u64) -> Option<(Board, String, PinRef)> {
    let mut board = synthetic_board(seed, 40, 8, 8, 20, 20);
    let net = board
        .nets
        .iter()
        .find(|n| n.name.starts_with('S') && n.pads.len() >= 3)?
        .clone();
    let least = SIZING.least_width(2.0);
    board.net_widths.push((net.name.clone(), (least * 100.0).ceil() / 100.0));
    board.net_currents.push(NetCurrent {
        net: net.name.clone(),
        current: 2.0,
        least_width: least,
        inner_least_width: None,
        vias: SIZING.vias(2.0),
        source: String::new(),
    });
    let centre = |p: &netsmith_pcb::PadRef| {
        let part = board.part(&p.refdes).unwrap();
        part.pad_centre(part.footprint.pads_numbered(&p.number).next().unwrap())
    };
    let far = net
        .pads
        .iter()
        .max_by(|a, b| {
            let sum = |p| net.pads.iter().map(|q| centre(p).distance(centre(q))).sum::<f64>();
            sum(a).total_cmp(&sum(b))
        })
        .unwrap()
        .clone();
    let pin = PinRef {
        refdes: far.refdes.clone(),
        pin: far.number.clone(),
    };
    board.branch_currents.push(BranchCurrent {
        net: net.name.clone(),
        path: "probe.p1".into(),
        pins: vec![pin.clone()],
        current: 0.001,
        width: board.rules.track_width,
        least_width: SIZING.least_width(0.001),
        inner_least_width: None,
        vias: 1,
    });
    Some((board, net.name, pin))
}

fn extras(board: &Board) -> DrcExtras {
    DrcExtras {
        least_widths: board.least_widths(),
        via_counts: board.via_counts(),
        neck_reach: netsmith_router::route::NECK_REACH,
        branches: board
            .branch_currents
            .iter()
            .map(|b| DrcBranch {
                net: b.net.clone(),
                path: b.path.clone(),
                part: b.pins[0].refdes.clone(),
                pads: vec![b.pins[0].pin.clone()],
                current: b.current,
            })
            .collect(),
        sizing: Some(SIZING),
        ..DrcExtras::default()
    }
}

#[test]
fn the_line_to_a_branch_s_pad_is_drawn_for_its_current_and_the_check_holds_it_to_it() {
    // The first two seeds whose board has a signal net of three pads.
    let boards: Vec<(u64, (Board, String, PinRef))> = (1..40)
        .filter_map(|seed| with_a_branch(seed).map(|b| (seed, b)))
        .take(2)
        .collect();
    assert_eq!(boards.len(), 2);
    for (seed, (mut board, net, pin)) in boards {
        let name = format!("seed {seed}, net {net}");
        // Its pad is a leaf of the net: one line ends at it, and that line is the branch's.
        let rats = Ratsnest::of(&board);
        let at_pad = |p: &netsmith_pcb::PadRef| p.refdes == pin.refdes && p.number == pin.pin;
        let lines: Vec<_> = rats
            .lines
            .iter()
            .filter(|l| l.net == net && (at_pad(&l.from) || at_pad(&l.to)))
            .collect();
        assert_eq!(lines.len(), 1, "{name}: {lines:?}");
        assert_eq!(
            board.line_width_on(lines[0], netsmith_pcb::CopperLayer::Front),
            board.rules.track_width
        );
        assert_eq!(board.line_vias(lines[0]), 1);
        let trunk = board.track_width(&net);
        assert!(trunk >= 0.98, "{name}: {trunk}");

        let routing = route_with(&mut board, RouteOptions::default());
        assert!(routing.left.is_empty(), "{name}: {:?}", routing.left.lines);
        let narrow = board
            .tracks
            .iter()
            .filter(|t| t.net == net && t.width < trunk - 1e-9)
            .count();
        let wide = board
            .tracks
            .iter()
            .filter(|t| t.net == net && t.width >= trunk - 1e-9)
            .count();
        println!("{name}: {narrow} tracks at the branch's width, {wide} at the net's");
        assert!(narrow >= 1 && wide >= 1, "{name}: {narrow} narrow, {wide} wide");

        let told = drc_check(
            &netsmith_pcb::kicad_pcb(&board),
            Some(&netsmith_pcb::kicad_pro(&board)),
            &extras(&board),
        )
        .unwrap();
        assert_eq!(
            (
                told.count(DrcRule::TrackCurrent),
                told.count(DrcRule::ViaCurrent)
            ),
            (0, 0),
            "{name}\n{told}"
        );
        // Only the branch's copper reaches its pad: a narrow track ends on it, and no wide one.
        let part = board.part(&pin.refdes).unwrap();
        let pad = part.pad_bounds(part.footprint.pads_numbered(&pin.pin).next().unwrap());
        let ends_on = |t: &&netsmith_pcb::Track| {
            t.net == net
                && [t.from, t.to]
                    .iter()
                    .any(|&p| pad.contains(&netsmith_pcb::Rect::new(p, p)))
        };
        let at_pad: Vec<f64> = board.tracks.iter().filter(ends_on).map(|t| t.width).collect();
        assert!(!at_pad.is_empty(), "{name}: a track reaches the branch's pad");
        assert!(
            at_pad.iter().all(|&w| w < trunk - 1e-9),
            "{name}: the tracks at the branch's pad are {at_pad:?} mm wide"
        );
    }
}
