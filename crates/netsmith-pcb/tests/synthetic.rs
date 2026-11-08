//! The placer and router on boards the size of a real one, made from seeds so that no
//! design's data is needed: how long they take, and that what they cannot do is said.

use crate::common;

use netsmith_kicad::testlib::TempDir;
use std::time::{Duration, Instant};

use netsmith_pcb::{PlaceOptions, Quality, Ratsnest, RouteOptions, route_with};

use common::{land_grid_board, land_grid_board_with, synthetic_board, synthetic_board_structured};

#[test]
fn a_board_of_seventy_parts_and_a_package_of_eighty_pads_is_placed_and_routed_in_bounded_time() {
    let started = Instant::now();
    let mut board = synthetic_board(7, 52, 17, 20, 72, 36);
    let placed = started.elapsed();
    assert_eq!(board.parts.len(), 70);
    assert_eq!(board.parts[0].footprint.pads.len(), 80);
    assert!(board.nets.len() >= 100, "{} nets", board.nets.len());
    assert_eq!(board.check_placement(), vec![]);
    let (w, h) = (board.outline.width(), board.outline.height());
    assert!(
        w.max(h) <= 2.5 * w.min(h) && w * h <= 20_000.0,
        "a sprawling board: {w} by {h} mm"
    );

    let routing = route_with(
        &mut board,
        RouteOptions {
            time_limit: Some(Duration::from_secs(120)),
            ..RouteOptions::default()
        },
    );
    let total = started.elapsed();
    eprintln!(
        "placed in {placed:?}, routed in {:?}, {} lines left, {} tries",
        total - placed,
        routing.left.len(),
        routing.tries
    );
    assert!(!routing.stopped, "the router ran out of time");
    assert!(
        routing.left.is_empty(),
        "unrouted: {:#?}",
        routing.left.lines
    );
    assert!(total < Duration::from_secs(60), "took {total:?}");
}

#[test]
fn a_small_board_is_placed_and_routed_completely() {
    let started = Instant::now();
    let mut board = synthetic_board(3, 10, 3, 8, 10, 4);
    assert_eq!(board.check_placement(), vec![]);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(
        routing.left.is_empty(),
        "unrouted: {:#?}",
        routing.left.lines
    );
    assert!(!routing.stopped);
    assert!(Ratsnest::of(&board).is_empty());
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn a_time_limit_stops_the_router_and_leaves_the_lines_it_did_not_reach() {
    let mut board = synthetic_board(7, 52, 17, 20, 72, 36);
    let started = Instant::now();
    let routing = route_with(
        &mut board,
        RouteOptions {
            time_limit: Some(Duration::from_millis(1)),
            ..RouteOptions::default()
        },
    );
    assert!(routing.stopped);
    assert!(!routing.left.is_empty());
    // Every line left is named, and the router stopped soon after the limit.
    assert!(routing.left.lines.iter().all(|l| !l.net.is_empty()));
    assert!(started.elapsed() < Duration::from_secs(10));
    // What was routed before the stop is kept, and joins what the ratsnest says it does.
    assert_eq!(Ratsnest::of(&board).len(), routing.left.len());
}

#[test]
fn the_same_seed_makes_the_same_board_and_the_same_routing() {
    let (mut a, mut b) = (
        synthetic_board(5, 12, 4, 8, 12, 4),
        synthetic_board(5, 12, 4, 8, 12, 4),
    );
    assert_eq!(a, b);
    route_with(&mut a, RouteOptions::default());
    route_with(&mut b, RouteOptions::default());
    assert_eq!(a, b);
    assert_ne!(a, synthetic_board(6, 12, 4, 8, 12, 4));
}

#[test]
fn pads_walled_in_by_their_neighbours_are_left_unrouted_at_once_and_named() {
    let started = Instant::now();
    // The board refuses vias in pads, which would reach them.
    let (mut board, walled) = land_grid_board_with(7, 12, &[".via_in_pad &= 0mm"]);
    assert_eq!(board.check_placement(), vec![]);
    assert!(!walled.is_empty());
    let routing = route_with(&mut board, RouteOptions::default());
    let took = started.elapsed();
    eprintln!("land grid: {:?}, left {:#?}", took, routing.left.lines);
    assert!(!routing.stopped);
    assert!(
        !routing.left.is_empty(),
        "the pads inside cannot be reached"
    );
    for line in &routing.left.lines {
        let on_package = [&line.from, &line.to]
            .into_iter()
            .find(|p| p.refdes == "U1");
        let pad = on_package.unwrap_or_else(|| panic!("{line}: not a pad of the package"));
        assert!(
            walled.contains(&pad.number),
            "{line} is not a walled in pad"
        );
    }
    // The pads on the ring are all joined, and only the walled in ones are left.
    assert_eq!(routing.left.len(), walled.len());
    assert!(took < Duration::from_secs(30), "took {took:?}");
}

#[test]
fn pads_walled_in_by_their_neighbours_are_reached_by_a_via_in_the_pad_when_the_process_has_them() {
    let started = Instant::now();
    let (mut board, walled) = land_grid_board(7, 12);
    assert!(board.rules.via_in_pad);
    let routing = route_with(&mut board, RouteOptions::default());
    eprintln!("land grid with vias in pads: {:?}", started.elapsed());
    assert!(
        routing.left.is_empty(),
        "unrouted: {:#?}",
        routing.left.lines
    );
    // Every pad that was walled in has a via in it, on its own net, and no other pad has.
    let in_pad: Vec<String> = board
        .vias
        .iter()
        .filter_map(|v| {
            let u1 = board.part("U1").unwrap();
            u1.footprint
                .pads
                .iter()
                .find(|p| {
                    u1.pad_bounds(p)
                        .contains(&netsmith_pcb::Rect::new(v.at, v.at))
                })
                .map(|p| format!("{}:{}", p.number, v.net))
        })
        .collect();
    for pad in &walled {
        assert!(
            in_pad.iter().any(|s| s.starts_with(&format!("{pad}:S"))),
            "pad {pad} has no via of its own: {in_pad:?}"
        );
    }
    assert!(Ratsnest::of(&board).is_empty());
    assert!(started.elapsed() < Duration::from_secs(30));

    // The fabrication notes say so, with the count and the pads.
    let in_pads = board.vias_in_pads();
    assert!(in_pads.len() >= walled.len(), "{in_pads:?}");
    let extras = board.process_extras();
    assert_eq!(extras.len(), 1, "{extras:?}");
    assert!(
        extras[0].starts_with(&format!("vias in pads: {} via(s)", in_pads.len()))
            && extras[0].contains("U1.")
            && extras[0].contains("epoxy filled and capped")
            && extras[0].contains("JLCPCB")
            && extras[0].contains("0.55 mm"),
        "{}",
        extras[0]
    );
}

#[test]
fn a_board_with_no_via_in_a_pad_asks_nothing_extra_of_the_fabricator() {
    let mut board = synthetic_board(3, 10, 3, 8, 10, 4);
    route_with(&mut board, RouteOptions::default());
    assert!(board.vias_in_pads().is_empty());
    assert_eq!(board.process_extras(), Vec::<String>::new());
}

#[test]
fn a_walled_in_pad_on_a_plane_net_gets_a_via_in_the_pad() {
    use netsmith_pcb::PadRef;
    // Pad 11 is inside the ring (row 1, column 3), and joins the ground plane.
    let join = |board: &mut netsmith_pcb::Board| {
        board
            .nets
            .iter_mut()
            .find(|n| n.name == "GND")
            .unwrap()
            .pads
            .push(PadRef {
                refdes: "U1".into(),
                number: "11".into(),
            });
    };
    let (mut with, _) = land_grid_board(7, 12);
    join(&mut with);
    let left = netsmith_pcb::route(&mut with);
    assert!(left.is_empty(), "{:?}", left.lines);
    let u1 = with.part("U1").unwrap();
    let pad = u1.footprint.pads_numbered("11").next().unwrap();
    let inside = |v: &netsmith_pcb::Via| {
        u1.pad_bounds(pad)
            .contains(&netsmith_pcb::Rect::new(v.at, v.at))
    };
    assert!(
        with.vias.iter().any(|v| v.net == "GND" && inside(v)),
        "no ground via in pad 11"
    );

    // Without vias in pads, the pad has no way to the plane and is named.
    let (mut without, _) = land_grid_board_with(7, 12, &[".via_in_pad &= 0mm"]);
    join(&mut without);
    let left = netsmith_pcb::route(&mut without);
    assert!(
        left.lines
            .iter()
            .any(|l| l.to_plane && l.from.refdes == "U1" && l.from.number == "11"),
        "{:?}",
        left.lines
    );
}

/// Prints the numbers ADR 0033 quotes: `cargo test --release -p netsmith-pcb --test main
/// synthetic::the_placement_of_the_big_boards_is_measured -- --nocapture`. With
/// `NETSMITH_WRITE_BOARDS` set to a folder it writes the routed boards there, for
/// `crates/netsmith-pcb/scripts/check-kicad.sh`.
#[test]
fn the_placement_of_the_big_boards_is_measured() {
    for (name, structured) in [("plain", false), ("structured", true)] {
        let started = Instant::now();
        let mut board = if structured {
            synthetic_board_structured(7, 52, 17, 20, 72, 36, &PlaceOptions::default())
        } else {
            synthetic_board(7, 52, 17, 20, 72, 36)
        };
        let placed = started.elapsed();
        let q = Quality::of(&board);
        eprintln!(
            "{name}: {} parts placed in {placed:?} on {} x {} mm; hpwl {:.1}, ratsnest {:.1}, crossings {}, decoupling {} caps mean {:.2} max {:.2}; problems {:?}",
            board.parts.len(),
            board.outline.width(),
            board.outline.height(),
            q.hpwl,
            q.ratsnest,
            q.crossings,
            q.decoupling.len(),
            q.decoupling_mean(),
            q.decoupling_max(),
            board.check_placement()
        );
        let routed = Instant::now();
        let routing = route_with(&mut board, RouteOptions::default());
        eprintln!(
            "{name}: routed in {:?}, {} left, {} tries",
            routed.elapsed(),
            routing.left.len(),
            routing.tries
        );
        // With a folder named, the routed boards are written there for KiCad's check
        // (`crates/netsmith-pcb/scripts/check-kicad.sh <folder>/<name>.kicad_pcb`).
        if let Some(dir) = std::env::var_os("NETSMITH_WRITE_BOARDS") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(format!("{name}.kicad_pcb")),
                netsmith_pcb::kicad_pcb(&board),
            )
            .unwrap();
            std::fs::write(
                dir.join(format!("{name}.kicad_pro")),
                netsmith_pcb::kicad_pro(&board),
            )
            .unwrap();
        }
    }
}

/// Writes the board, plots it with netsmith's own writers and with KiCad, and compares the
/// two (ADR 0034). `None` where KiCad is not installed.
fn crosscheck(name: &str, board: &netsmith_pcb::Board) -> Option<netsmith_fab::crosscheck::Report> {
    use netsmith_fab::crosscheck::{Limits, check};
    let cli = netsmith_fab::KicadCliExe::locate_runnable()?;
    let dir = TempDir::new(&format!("pcb-cross-{name}"));
    let pcb = dir.join(format!("{name}.kicad_pcb"));
    std::fs::write(&pcb, netsmith_pcb::kicad_pcb(board)).ok()?;
    std::fs::write(
        dir.join(format!("{name}.kicad_pro")),
        netsmith_pcb::kicad_pro(board),
    )
    .ok()?;
    let report = check(&pcb, &cli, &Limits::default()).unwrap_or_else(|e| panic!("{e}"));
    Some(report)
}

#[test]
fn a_small_routed_board_agrees_with_kicads_plot() {
    let mut board = synthetic_board(3, 10, 3, 8, 10, 4);
    let routing = route_with(&mut board, RouteOptions::default());
    assert!(routing.left.is_empty());
    let Some(report) = crosscheck("small", &board) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert!(report.ok(), "{report}");
    assert!(report.layers.len() >= 7, "{report}");
}

#[test]
fn the_board_of_seventy_parts_agrees_with_kicads_plot() {
    let mut board = synthetic_board(7, 52, 17, 20, 72, 36);
    let routing = route_with(
        &mut board,
        RouteOptions {
            time_limit: Some(Duration::from_secs(120)),
            ..RouteOptions::default()
        },
    );
    assert!(routing.left.is_empty());
    let Some(report) = crosscheck("big", &board) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    eprintln!("{report}");
    assert!(report.ok(), "{report}");
}

#[test]
fn a_land_grid_with_vias_in_its_pads_agrees_with_kicads_plot() {
    let (mut board, _) = land_grid_board(7, 12);
    assert!(
        route_with(&mut board, RouteOptions::default())
            .left
            .is_empty()
    );
    let Some(report) = crosscheck("grid", &board) else {
        eprintln!("skipped: no KiCad");
        return;
    };
    assert!(report.ok(), "{report}");
}

#[test]
fn the_boards_of_seventy_parts_the_placer_makes_pass_netsmiths_own_design_rules_check() {
    for (name, structured) in [("plain", false), ("structured", true)] {
        let mut board = if structured {
            synthetic_board_structured(7, 52, 17, 20, 72, 36, &PlaceOptions::default())
        } else {
            synthetic_board(7, 52, 17, 20, 72, 36)
        };
        let routing = route_with(&mut board, RouteOptions::default());
        assert!(routing.left.is_empty(), "{name}: {:?}", routing.left.lines);
        let report = netsmith_fab::drc::check(
            &netsmith_pcb::kicad_pcb(&board),
            Some(&netsmith_pcb::kicad_pro(&board)),
            &Default::default(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let errors: Vec<String> = report.errors().map(|f| f.to_string()).collect();
        assert_eq!(errors, Vec::<String>::new(), "{name}");
    }
}
