//! A tight board that places but did not route to the end (issue #3): about a hundred parts on
//! both faces in a small area, round a module of 80 pads of 0.6 mm on a 1.1 mm pitch.

mod common;

use std::time::{Duration, Instant};

use netsmith_pcb::{RouteOptions, route_with};

use common::tight_board;

/// The outline of the measured boards, 2100 mm2.
const AREA: (f64, f64) = (50.0, 42.0);

/// Prints, for each seed in `NETSMITH_SEEDS` (`0..8` by default; `a..b`), the lines left
/// unrouted and the time: `NETSMITH_SEEDS=0..20 cargo test --release -p netsmith-pcb --test
/// tight measured -- --ignored --nocapture`.
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
                time_limit: Some(Duration::from_secs(300)),
                ..RouteOptions::default()
            },
        );
        let took = started.elapsed();
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
            picture(&board, &routing.left);
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

/// A picture of the board round U1 in letters, for the two faces, at 0.15 mm a character:
/// pads `#` (`@` for a pad with a line left), a track the letter of its net (by index), a
/// via `o`, other parts' pads `+`.
fn picture(board: &netsmith_pcb::Board, left: &netsmith_pcb::Ratsnest) {
    use netsmith_pcb::{CopperLayer, Pt, Rect};
    let u1 = board.part("U1").unwrap();
    let mut area = Rect::new(u1.at, u1.at);
    for p in &u1.footprint.pads {
        let b = u1.pad_bounds(p);
        area = Rect::new(
            Pt::new(area.min.x.min(b.min.x), area.min.y.min(b.min.y)),
            Pt::new(area.max.x.max(b.max.x), area.max.y.max(b.max.y)),
        );
    }
    let area = area.expanded(2.0);
    let step = 0.15;
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
                } else if part.refdes == "U1" {
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
