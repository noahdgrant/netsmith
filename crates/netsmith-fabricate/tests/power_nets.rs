//! The power nets of a routed board (issue #1532): a net with a known current that needs vias
//! side by side and that no pour fills is routed on one layer where it can, and changes layer
//! through a group of vias sized for its current. Measured on the examples and on generated
//! boards whose supply and ground carry amps, by what `board nets` reports of each power net
//! (`netsmith_fabricate::net_quality::PowerNet`). The tests count; `measured` prints the
//! numbers.

use netsmith_fabricate::net_quality::{Counts, PowerNet, Quality};
use netsmith_pcb::Board;
use netsmith_router::{RouteOptions, Routing, route_with};

use crate::router_quality::placer_common;

/// A generated board whose [`POWERED`] nets of the most pads (its supply and ground are planes)
/// carry 2 A: three vias side by side each.
fn powered(seed: u64) -> Board {
    let mut board = placer_common::synthetic_board(seed, 10, 3, 8, 10, 4);
    let mut by_pads: Vec<(usize, String)> = board
        .nets
        .iter()
        .filter(|n| n.name.starts_with('S'))
        .map(|n| (n.pads.len(), n.name.clone()))
        .collect();
    by_pads.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (_, net) in by_pads.into_iter().take(POWERED) {
        board.net_currents.retain(|c| c.net != net);
        board.net_currents.push(netsmith_core::NetCurrent {
            net,
            current: 2.0,
            least_width: board.rules.track_width,
            inner_least_width: None,
            vias: 3,
            source: String::new(),
            ..Default::default()
        });
    }
    board
}

/// How many nets of a generated board are power nets.
const POWERED: usize = 10;

/// The boards: the routed examples and three generated boards with power nets.
fn boards() -> Vec<(String, Board)> {
    let mut boards: Vec<(String, Board)> = [
        "led",
        "status_indicator",
        "linear_regulator",
        "four_layer",
        "six_layer",
    ]
    .into_iter()
    .map(|name| {
        let board = placer_common::example_board(name, &placer_common::fixtures());
        (name.to_string(), board)
    })
    .collect();
    boards.extend((1..=3).map(|seed| (format!("generated {seed}"), powered(seed))));
    boards
}

fn routed(name: &str, mut board: Board, options: RouteOptions) -> (Routing, Board) {
    let routing = route_with(&mut board, options);
    assert!(routing.left.is_empty(), "{name}");
    (routing, board)
}

/// The totals over a board's power nets: (layer changes, vias, groups, short groups).
fn totals(power: &[PowerNet]) -> (usize, usize, usize, usize) {
    power.iter().fold((0, 0, 0, 0), |t, p| {
        (
            t.0 + p.layer_changes,
            t.1 + p.vias,
            t.2 + p.groups,
            t.3 + p.short_groups,
        )
    })
}

/// On the generated boards the power nets change layer no more often than before, each change
/// through one group of as many vias as the current needs, every board routes completely, and
/// the copper has no defect the quality report counts (the vias of a group beside the one its
/// tracks meet are no dead copper).
#[test]
fn power_nets_change_layer_less_and_through_one_whole_group() {
    let (mut before, mut after) = (0, 0);
    for (name, board) in boards().into_iter().skip(5) {
        let old = RouteOptions {
            power_groups: false,
            ..RouteOptions::default()
        };
        let (_, old) = routed(&name, board.clone(), old);
        let (_, new) = routed(&name, board, RouteOptions::default());
        before += totals(&PowerNet::of_board(&old)).0;
        let power = PowerNet::of_board(&new);
        assert_eq!(power.len(), POWERED, "{name}");
        for p in &power {
            assert_eq!(p.short_groups, 0, "{name}: {p:?}");
            assert_eq!(p.vias, p.groups * p.vias_needed, "{name}: {p:?}");
            assert!(p.layer_changes <= p.groups, "{name}: {p:?}");
        }
        after += totals(&power).0;
        let quality = Quality::of_board(&new);
        assert_eq!(quality.totals, Counts::LIMITS, "{name}: {:?}", quality.nets);
    }
    assert!(
        after < before,
        "power nets' layer changes: {after} against {before}"
    );
}

/// The numbers: for each board, each power net's current, layers, layer changes, vias and
/// groups, and the board's totals.
#[test]
#[ignore = "measured: prints the power nets of each routed board"]
fn measured() {
    println!(
        "{:<18} {:<8} {:>6} {:>6} {:>7} {:>5} {:>6} {:>5}",
        "board", "net", "amps", "layers", "changes", "vias", "groups", "short"
    );
    for (name, board) in boards() {
        for (label, power_groups) in [("before", false), ("after", true)] {
            let options = RouteOptions {
                power_groups,
                ..RouteOptions::default()
            };
            let (_, board) = routed(&name, board.clone(), options);
            let power = PowerNet::of_board(&board);
            for p in &power {
                println!(
                    "{:<18} {:<8} {:>6.2} {:>6} {:>7} {:>5} {:>6} {:>5}",
                    format!("{name}, {label}"),
                    p.net,
                    p.current,
                    p.layers,
                    p.layer_changes,
                    p.vias,
                    p.groups,
                    p.short_groups
                );
            }
            let (c, v, g, s) = totals(&power);
            let o = netsmith_fabricate::net_quality::Objective::of_table(
                &netsmith_fabricate::net_table::of_board(&board).expect("a table"),
            );
            println!(
                "{:<18} {:<8} {:>6} {:>6} {c:>7} {v:>5} {g:>6} {s:>5}  (all nets: {} vias, detour {:.3})",
                format!("{name}, {label}"),
                "total",
                "",
                "",
                o.vias,
                o.detour
            );
            let quality = Quality::of_board(&board);
            if quality.totals != Counts::LIMITS {
                println!("{name}, {label}: defects {:?}", quality.nets);
            }
        }
    }
}
