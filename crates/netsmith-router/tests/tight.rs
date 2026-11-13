//! A tight board that places but did not route to the end (issue #3): about a hundred parts on
//! both faces in a small area, round a module of 80 pads of 0.6 mm on a 1.1 mm pitch.

use crate::common;
use crate::guard;

use std::time::{Duration, Instant};

use netsmith_router::{RouteOptions, route_with};

use common::tight_board;

/// The outline of the measured boards.
const AREA: (f64, f64) = (27.0, 25.0);
/// The outline of the boards the checks use, 675 mm2.
const AREA_TIGHT: (f64, f64) = (27.0, 25.0);

/// The design rules check's errors on a routed board.
fn drc_errors(board: &netsmith_pcb::Board) -> Vec<String> {
    let report = netsmith_fab::drc::check(
        &netsmith_pcb::kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &Default::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    report.errors().map(|f| f.to_string()).collect()
}

#[test]
fn the_tight_board_has_a_module_of_eighty_fine_pitch_pads_and_parts_on_both_faces() {
    let board = tight_board(0, AREA_TIGHT);
    assert_eq!(board.parts.len(), 99);
    assert_eq!(board.check_placement(), vec![]);
    let u1 = board.part("U1").unwrap();
    assert_eq!(u1.footprint.pads.len(), 80);
    let pads = &u1.footprint.pads;
    assert!(pads.iter().all(|p| (p.width, p.height) == (0.6, 0.6)));
    // 1.1 mm from each pad to the nearest other.
    for p in pads {
        let near = pads
            .iter()
            .filter(|q| q.number != p.number)
            .map(|q| p.at.distance(q.at))
            .fold(f64::MAX, f64::min);
        assert!((near - 1.1).abs() < 1e-3, "pad {}: {near}", p.number);
    }
    let back = board
        .parts
        .iter()
        .filter(|p| p.side == netsmith_pcb::Side::Back)
        .count();
    assert!((40..=60).contains(&back), "{back} parts on the back");
    let (w, h) = (board.outline.width(), board.outline.height());
    assert!(w * h <= AREA_TIGHT.0 * AREA_TIGHT.1 + 1.0, "{w} x {h} mm");
}

#[test]
fn a_tight_board_that_the_router_finishes_passes_the_design_rules_check() {
    // Seed 10 routes completely; most seeds of this size leave lines (see `measured`).
    let mut board = tight_board(10, AREA_TIGHT);
    let routing = route_with(
        &mut board,
        RouteOptions {
            time_limit: Some(guard::GUARD),
            ..RouteOptions::default()
        },
    );
    guard::not_hung(&routing);
    assert!(
        routing.left.is_empty(),
        "unrouted: {:#?}",
        routing.left.lines
    );
    assert_eq!(drc_errors(&board), Vec::<String>::new());
}

#[test]
fn a_tight_board_the_router_cannot_finish_leaves_named_lines_and_only_copper_that_keeps_the_rules()
{
    // Seed 2 leaves lines on the module when the router tries no other order and does no rip-up or negotiation
    // (a fixed amount of work, so that what is left is the same on every machine: a time limit
    // would leave what a machine got done in it). What was routed must still pass the rules.
    let mut board = tight_board(2, AREA_TIGHT);
    let routing = route_with(
        &mut board,
        RouteOptions {
            retry_orders: false,
            rip_up: false,
            negotiate: false,
            time_limit: Some(guard::GUARD),
            ..RouteOptions::default()
        },
    );
    guard::not_hung(&routing);
    assert!(!routing.left.is_empty());
    // Every order is one more time through the board: never more than four.
    assert!(routing.tries <= 4, "{} tries", routing.tries);
    assert!(routing.left.lines.iter().all(|l| !l.net.is_empty()));
    let errors: Vec<String> = drc_errors(&board)
        .into_iter()
        .filter(|e| !e.to_lowercase().contains("unconnected"))
        .collect();
    assert_eq!(errors, Vec::<String>::new());
}

#[test]
fn the_lines_that_end_deep_in_the_fine_pitch_module_are_routed_first_so_an_easy_seed_needs_one_try()
{
    // Seed 0 completes, but only on its third try when the lines are routed shortest first:
    // the short lines of the parts round the module take the gaps between its pads before
    // the lines of its inner pads, which have no other way out, are reached. With those
    // lines first it completes on the first.
    let mut board = tight_board(0, AREA_TIGHT);
    let routing = route_with(
        &mut board,
        RouteOptions {
            time_limit: Some(guard::GUARD),
            ..RouteOptions::default()
        },
    );
    guard::not_hung(&routing);
    assert!(
        routing.left.is_empty(),
        "unrouted: {:#?}",
        routing.left.lines
    );
    assert_eq!(routing.tries, 1);
}

/// Prints, for each seed in `NETSMITH_SEEDS` (`0..8` by default; `a..b`), the lines left
/// unrouted and the time: `NETSMITH_SEEDS=0..20 cargo test --release -p netsmith-router --test
/// main tight::measured -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn measured() {
    let range = std::env::var("NETSMITH_SEEDS").unwrap_or_else(|_| "0..8".into());
    let (a, b) = range.split_once("..").expect("a..b");
    let (a, b): (u64, u64) = (a.parse().unwrap(), b.parse().unwrap());
    let area = std::env::var("NETSMITH_AREA")
        .ok()
        .and_then(|a| {
            let (w, h) = a.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        })
        .unwrap_or(AREA);
    let mut done = 0;
    let mut lines = 0;
    let mut total = Duration::ZERO;
    for seed in a..b {
        let Ok(mut board) = std::panic::catch_unwind(|| tight_board(seed, area)) else {
            eprintln!("seed {seed}: does not place in {area:?}");
            continue;
        };
        let problems = board.check_placement();
        let started = Instant::now();
        let routing = route_with(
            &mut board,
            RouteOptions {
                time_limit: Some(guard::GUARD),
                ..RouteOptions::default()
            },
        );
        let took = started.elapsed();
        common::write_measured(&board, &format!("tight-seed{seed}"));
        let on_u1 = routing
            .left
            .lines
            .iter()
            .filter(|l| l.from.refdes == "U1" || l.to.refdes == "U1")
            .count();
        eprintln!(
            "seed {seed}: {} parts, placement problems {}, left {} ({on_u1} on U1), tries {}, vias {}, {took:.1?}",
            board.parts.len(),
            problems.len(),
            routing.left.len(),
            routing.tries,
            board.vias.len(),
        );
        for l in &routing.left.lines {
            eprintln!("    {l}");
        }
        if std::env::var_os("NETSMITH_PICTURE").is_some() {
            picture(&board, &routing.left, "U1");
        }
        done += usize::from(routing.left.is_empty());
        lines += routing.left.len();
        total += took;
    }
    eprintln!(
        "{done} of {} seeds complete, {lines} lines left, {total:.1?}",
        b - a
    );
}

/// A picture of the board round part `refdes` in letters, for the two faces, at 0.15 mm a character:
/// pads `#` (`@` for a pad with a line left), a track the letter of its net (by index), a
/// via `o`, other parts' pads `+`.
pub fn picture(board: &netsmith_pcb::Board, left: &netsmith_pcb::Ratsnest, refdes: &str) {
    picture_step(board, left, refdes, None, 0.15);
}

/// [`picture`] at `step` mm a character; with `round` (a pad's centre) the picture is the 5 mm square about it.
pub fn picture_step(
    board: &netsmith_pcb::Board,
    left: &netsmith_pcb::Ratsnest,
    refdes: &str,
    round: Option<netsmith_pcb::Pt>,
    step: f64,
) {
    use netsmith_pcb::{CopperLayer, Pt, Rect};
    let u1 = board.part(refdes).unwrap();
    let mut area = Rect::new(u1.at, u1.at);
    for p in &u1.footprint.pads {
        let b = u1.pad_bounds(p);
        area = Rect::new(
            Pt::new(area.min.x.min(b.min.x), area.min.y.min(b.min.y)),
            Pt::new(area.max.x.max(b.max.x), area.max.y.max(b.max.y)),
        );
    }
    let area = match round {
        Some(c) => netsmith_pcb::Rect::new(c, c).expanded(2.5),
        None => area.expanded(2.0),
    };
    let (w, h) = (
        (area.width() / step) as usize + 1,
        (area.height() / step) as usize + 1,
    );
    let letter = |net: &str| {
        let k = board.nets.iter().position(|n| n.name == net).unwrap_or(0);
        (b'a' + (k % 26) as u8) as char
    };
    for (face, layer) in [("front", CopperLayer::Front), ("back", CopperLayer::Back)] {
        let mut grid = vec![vec!['.'; w]; h];
        let mut put = |p: Pt, c: char| {
            let (x, y) = (
                ((p.x - area.min.x) / step).round() as isize,
                ((p.y - area.min.y) / step).round() as isize,
            );
            if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
                grid[y as usize][x as usize] = c;
            }
        };
        for part in &board.parts {
            for pad in &part.footprint.pads {
                let (f, b) = part.pad_layers(pad);
                if !(if layer == CopperLayer::Front { f } else { b }) {
                    continue;
                }
                let b = part.pad_bounds(pad);
                let wanted = left.lines.iter().any(|l| {
                    [&l.from, &l.to]
                        .iter()
                        .any(|r| r.refdes == part.refdes && r.number == pad.number)
                });
                let c = if wanted {
                    '@'
                } else if part.refdes == refdes {
                    '#'
                } else {
                    '+'
                };
                let mut y = b.min.y;
                while y <= b.max.y {
                    let mut x = b.min.x;
                    while x <= b.max.x {
                        put(Pt::new(x, y), c);
                        x += step / 2.0;
                    }
                    y += step / 2.0;
                }
            }
        }
        for t in board.tracks.iter().filter(|t| t.layer == layer) {
            let n = ((t.from.x - t.to.x).abs().max((t.from.y - t.to.y).abs()) / (step / 2.0))
                .ceil()
                .max(1.0) as usize;
            for k in 0..=n {
                let f = k as f64 / n as f64;
                put(
                    Pt::new(
                        t.from.x + (t.to.x - t.from.x) * f,
                        t.from.y + (t.to.y - t.from.y) * f,
                    ),
                    letter(&t.net),
                );
            }
        }
        for v in &board.vias {
            put(v.at, 'o');
        }
        eprintln!("PIC {face}, from {:.1},{:.1} mm", area.min.x, area.min.y);
        for row in grid {
            eprintln!("PIC {}", row.into_iter().collect::<String>());
        }
    }
}
