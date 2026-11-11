//! An invented board the size of a real 12 V automotive one, 35 x 60 mm (2100 mm2) with parts
//! up to 4 mm high, made of two QFN packages, SOIC and SOT-23 parts, 0603 and 1206 passives, and
//! four layers (issue #3): the placer places it, and the router has to finish it.
//!
//! Nothing here is any design's data: the footprints are written below (no KiCad library is
//! read), and the nets come from a small generator, so the same seed is the same board on every
//! machine. `NETSMITH_SEEDS=0..12 cargo test --release -p netsmith-pcb --test main
//! mixed::measured -- --ignored --nocapture` prints, for each seed, how the router ended.

use crate::common;

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use netsmith_core::{Design, Features, Keepout, Net, Outline, Part, PartHeight, PinRef};
use netsmith_kicad::testlib::TempDir;
use netsmith_pcb::{Board, BoardError, FootprintLibrary, PlaceOptions, Side};
use netsmith_pcb::{RouteOptions, route_with};

use common::four_layer_process;

/// The outline of the board, width and height in mm.
pub const AREA: (f64, f64) = (35.0, 60.0);
/// The tallest a part may stand, in mm.
pub const MAX_HEIGHT: f64 = 4.0;

/// One footprint file: `pads` are `(number, x, y, width, height)` and `court` is the courtyard's
/// half width and half height.
fn footprint(name: &str, pads: &[(String, f64, f64, f64, f64)], court: (f64, f64)) -> String {
    let mut text = String::new();
    for (n, x, y, w, h) in pads {
        let _ = write!(
            text,
            "(pad \"{n}\" smd rect (at {x:.4} {y:.4}) (size {w} {h}) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) "
        );
    }
    format!(
        "(footprint \"{name}\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 {ry} 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (property \"Value\" \"{name}\" (at 0 {vy} 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (attr smd) {text}\
         (fp_rect (start {nx} {ny}) (end {x} {y}) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n",
        ry = -court.1 - 1.0,
        vy = court.1 + 1.0,
        nx = -court.0,
        ny = -court.1,
        x = court.0,
        y = court.1,
    )
}

/// A QFN of `per_side` pads on each side at `pitch` and a square body of `body` mm, with an exposed
/// pad of `ep` mm numbered after the others. The pads are numbered from the top left, round.
fn qfn(name: &str, per_side: usize, pitch: f64, body: f64, ep: f64) -> String {
    let (length, width) = (0.875, 0.25);
    let centre = body / 2.0 - 0.0625;
    let mut pads = Vec::new();
    for side in 0..4 {
        for k in 0..per_side {
            let along = (k as f64 - (per_side as f64 - 1.0) / 2.0) * pitch;
            let (x, y, w, h) = match side {
                0 => (-centre, along, length, width),
                1 => (along, centre, width, length),
                2 => (centre, -along, length, width),
                _ => (-along, -centre, width, length),
            };
            pads.push(((side * per_side + k + 1).to_string(), x, y, w, h));
        }
    }
    pads.push(((4 * per_side + 1).to_string(), 0.0, 0.0, ep, ep));
    // As KiCad's generator draws it: 0.25 mm past the ends of the pads.
    let court = centre + length / 2.0 + 0.25;
    footprint(name, &pads, (court, court))
}

/// A dual in-line package of `pins` pads at 1.27 mm, the pads 1.975 mm long on a 4.95 mm span.
fn soic(name: &str, pins: usize) -> String {
    let rows = pins / 2;
    let mut pads = Vec::new();
    for k in 0..rows {
        let y = (k as f64 - (rows as f64 - 1.0) / 2.0) * 1.27;
        pads.push(((k + 1).to_string(), -2.475, y, 1.975, 0.6));
        pads.push(((pins - k).to_string(), 2.475, y, 1.975, 0.6));
    }
    let high = (rows as f64 - 1.0) * 1.27 / 2.0 + 0.6 + 0.25;
    footprint(name, &pads, (3.7, high))
}

/// A two-pad chip: pads `w` x `h` with their centres `span` apart.
fn chip(name: &str, span: f64, w: f64, h: f64) -> String {
    let pads = vec![
        ("1".to_string(), -span / 2.0, 0.0, w, h),
        ("2".to_string(), span / 2.0, 0.0, w, h),
    ];
    footprint(name, &pads, (span / 2.0 + w / 2.0 + 0.25, h / 2.0 + 0.25))
}

/// What the generator makes of each pin: a net name, or a signal to join to another pin.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    Gnd,
    Rail,
    Signal,
}

struct Spec {
    refdes: String,
    kind: &'static str,
    package: &'static str,
    pins: usize,
    height: f64,
    side: Side,
}

/// A small generator of numbers, the same on every machine.
struct Lcg(u64);

impl Lcg {
    fn draw(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as usize % bound
    }
}

/// How many of each part the board has.
#[derive(Clone, Copy)]
pub struct Mix {
    /// 0603 decoupling capacitors between the supply and ground.
    pub decoupling: usize,
    /// 0603 pull-ups from the supply to a signal.
    pub pull_ups: usize,
    /// 1206 bulk capacitors on the 12 V and 5 V rails.
    pub bulk: usize,
}

impl Mix {
    /// 5 ICs and 4 regulators, with 52 resistors, 32 capacitors and 8 bulk: 101 parts.
    pub const DEFAULT: Mix = Mix {
        decoupling: 32,
        pull_ups: 8,
        bulk: 8,
    };
}

/// The board made from `seed` placed in `area` (`None`: sized to the parts) by the placer.
pub fn mixed_board(seed: u64, area: (f64, f64), mix: Mix) -> Result<Board, BoardError> {
    let mut specs: Vec<Spec> = Vec::new();
    let mut add = |refdes: String,
                   kind: &'static str,
                   package: &'static str,
                   pins: usize,
                   height: f64,
                   side: Side| {
        specs.push(Spec {
            refdes,
            kind,
            package,
            pins,
            height,
            side,
        });
    };
    add("U1".into(), "Mcu", "QFN48", 49, 1.0, Side::Front);
    add("U2".into(), "Driver", "QFN32", 33, 1.0, Side::Front);
    add("U3".into(), "Ic", "SOIC8", 8, 1.75, Side::Front);
    add("U4".into(), "Ic", "SOIC8", 8, 1.75, Side::Back);
    add("U5".into(), "Ic", "SOIC16", 16, 1.75, Side::Back);
    for k in 1..=4 {
        let side = if k % 2 == 0 { Side::Back } else { Side::Front };
        add(format!("Q{k}"), "LinearRegulator", "SOT-23", 3, 1.2, side);
    }
    let signal_resistors = 44;
    for k in 1..=signal_resistors + mix.pull_ups {
        let side = if k % 3 == 0 { Side::Back } else { Side::Front };
        add(format!("R{k}"), "Resistor", "0603", 2, 0.55, side);
    }
    for k in 1..=mix.decoupling {
        let side = if k % 2 == 0 { Side::Back } else { Side::Front };
        add(format!("C{k}"), "Capacitor", "0603", 2, 0.9, side);
    }
    for k in 1..=mix.bulk {
        let side = if k % 2 == 0 { Side::Back } else { Side::Front };
        add(format!("Cb{k}"), "Bulk", "1206", 2, 1.8, side);
    }

    // The role of every pin of the ICs: ground and the 3.3 V plane at fixed places, the
    // regulators' pins 12 V to 5 V, and the rest are signals.
    let role = |s: &Spec, p: usize| -> Role {
        match s.kind {
            "Mcu" | "Driver" | "Ic" => {
                // The exposed pad of a QFN, and every eighth pad, are ground.
                let exposed = p > s.pins - usize::from(s.package.starts_with("QFN"));
                if exposed || p % 8 == 4 {
                    Role::Gnd
                } else if p.is_multiple_of(8) {
                    Role::Rail
                } else {
                    Role::Signal
                }
            }
            "LinearRegulator" => [Role::Gnd, Role::Rail, Role::Signal][p - 1],
            _ => Role::Signal,
        }
    };
    let mut rng = Lcg(seed ^ 0x6A09_E667);
    let pin = |s: &Spec, p: usize| PinRef {
        refdes: s.refdes.clone(),
        pin: p.to_string(),
    };
    let mut gnd: Vec<PinRef> = Vec::new();
    let mut rail: Vec<PinRef> = Vec::new();
    let mut v12: Vec<PinRef> = Vec::new();
    let mut v5: Vec<PinRef> = Vec::new();
    let mut signals: Vec<(usize, PinRef)> = Vec::new();
    for (i, s) in specs.iter().enumerate() {
        match s.kind {
            "Mcu" | "Driver" | "Ic" | "LinearRegulator" => {
                for p in 1..=s.pins {
                    match role(s, p) {
                        Role::Gnd => gnd.push(pin(s, p)),
                        Role::Rail => rail.push(pin(s, p)),
                        Role::Signal => signals.push((i, pin(s, p))),
                    }
                }
            }
            "Capacitor" => {
                rail.push(pin(s, 1));
                gnd.push(pin(s, 2));
            }
            "Bulk" => {
                let k: usize = s.refdes[2..].parse().unwrap();
                if k.is_multiple_of(2) {
                    &mut v5
                } else {
                    &mut v12
                }
                .push(pin(s, 1));
                gnd.push(pin(s, 2));
            }
            _ => {}
        }
    }
    // The regulators' signal pin is the 5 V output, an input from 12 V on the first ICs.
    signals.retain(|(i, p)| {
        let s = &specs[*i];
        if s.kind == "LinearRegulator" {
            v5.push(p.clone());
            false
        } else if s.refdes == "U3" && (p.pin == "1" || p.pin == "2") {
            v12.push(p.clone());
            false
        } else {
            true
        }
    });
    // Shuffle the signals, deterministic.
    for k in (1..signals.len()).rev() {
        let j = rng.draw(k + 1);
        signals.swap(k, j);
    }
    // Each series resistor joins two signal pins of different parts, taken in the shuffled
    // order.
    let mut nets: Vec<Net> = Vec::new();
    let mut resistor = 0;
    let mut used = vec![false; signals.len()];
    let take = |from: Option<usize>, signals: &[(usize, PinRef)], used: &mut [bool]| {
        let k = (0..signals.len()).find(|&k| !used[k] && from.is_none_or(|f| signals[k].0 != f))?;
        used[k] = true;
        Some(k)
    };
    while resistor < signal_resistors {
        let Some(a) = take(None, &signals, &mut used) else {
            break;
        };
        let Some(b) = take(Some(signals[a].0), &signals, &mut used) else {
            break;
        };
        resistor += 1;
        let r = format!("R{resistor}");
        nets.push(Net {
            name: format!("A{resistor}"),
            pins: vec![
                signals[a].1.clone(),
                PinRef {
                    refdes: r.clone(),
                    pin: "1".into(),
                },
            ],
        });
        nets.push(Net {
            name: format!("B{resistor}"),
            pins: vec![
                signals[b].1.clone(),
                PinRef {
                    refdes: r,
                    pin: "2".into(),
                },
            ],
        });
    }
    // Signals left over join the pull-ups' second pad.
    for (k, (_, p)) in signals.iter().enumerate() {
        if used[k] {
            continue;
        }
        let r = signal_resistors + 1 + (k % mix.pull_ups.max(1));
        let name = format!("P{r}");
        if let Some(net) = nets.iter_mut().find(|n| n.name == name) {
            net.pins.push(p.clone());
        } else {
            nets.push(Net {
                name,
                pins: vec![
                    p.clone(),
                    PinRef {
                        refdes: format!("R{r}"),
                        pin: "2".into(),
                    },
                ],
            });
        }
    }
    for k in 1..=mix.pull_ups {
        rail.push(PinRef {
            refdes: format!("R{}", signal_resistors + k),
            pin: "1".into(),
        });
    }
    nets.retain(|n| n.pins.len() >= 2);
    // A pull-up with no signal left joins nothing: take its pins out of the planes.
    let named: std::collections::BTreeSet<String> = nets
        .iter()
        .flat_map(|n| n.pins.iter().map(|p| p.refdes.clone()))
        .collect();
    rail.retain(|p| !p.refdes.starts_with('R') || named.contains(&p.refdes));
    for (name, pins) in [("GND", gnd), ("3V3", rail), ("V12", v12), ("V5", v5)] {
        nets.push(Net {
            name: name.into(),
            pins,
        });
    }

    // The parts, and the folder of footprints.
    let folder = TempDir::new("mixed");
    for (name, text) in [
        ("QFN48", qfn("QFN48", 12, 0.5, 7.0, 5.15)),
        ("QFN32", qfn("QFN32", 8, 0.5, 5.0, 3.45)),
        ("SOIC8", soic("SOIC8", 8)),
        ("SOIC16", soic("SOIC16", 16)),
        ("C1206", chip("C1206", 2.95, 1.25, 1.75)),
    ] {
        std::fs::write(folder.join(format!("{name}.kicad_mod")), text).unwrap();
    }
    let mut mapping = String::new();
    for (kind, package, fp) in [
        ("Mcu", "QFN48", "QFN48"),
        ("Driver", "QFN32", "QFN32"),
        ("Ic", "SOIC8", "SOIC8"),
        ("Ic", "SOIC16", "SOIC16"),
        ("Bulk", "1206", "C1206"),
    ] {
        let _ = write!(
            mapping,
            "[[part]]\nkind = \"{kind}\"\npackage = \"{package}\"\nsymbol = \"t:X\"\nfootprint = \"t:{fp}\"\n"
        );
    }
    let mapping = netsmith_kicad::Mapping::parse(&mapping, "mixed", std::path::Path::new("."))
        .unwrap_or_else(|e| panic!("{e}"));
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
        .with_library("t", folder.path());

    let design = Design {
        interfaces: Vec::new(),
        notes: Vec::new(),
        name: "Mixed".into(),
        process: Some(four_layer_process()),
        features: Features {
            outline: Some(Outline {
                width: (area.0, area.0),
                height: (area.1, area.1),
                corner_radius: 0.0,
            }),
            keepouts: vec![Keepout {
                name: "lid".into(),
                part: None,
                at: Some((0.0, 0.0)),
                size: area,
                layers: vec!["top".into(), "bottom".into()],
                tracks: false,
                vias: false,
                copper: false,
                parts: false,
                max_height: Some(MAX_HEIGHT),
            }],
            heights: specs
                .iter()
                .map(|s| PartHeight {
                    path: s.refdes.to_lowercase(),
                    height: s.height,
                })
                .collect(),
            ..Features::default()
        },
        parts: specs
            .iter()
            .map(|s| Part {
                provides: Vec::new(),
                refdes: s.refdes.clone(),
                kind: s.kind.into(),
                value: s.package.into(),
                package: Some(s.package.into()),
                path: s.refdes.to_lowercase(),
                pins: Vec::new(),
            })
            .collect(),
        nets,
    };
    let options = PlaceOptions {
        sides: specs
            .iter()
            .filter(|s| s.side == Side::Back)
            .map(|s| (s.refdes.clone(), Side::Back))
            .collect(),
        fit: Some(area),
        seed,
        ..PlaceOptions::default()
    };
    Board::from_design_placed(&design, &mapping, &library, &options)
}

/// The design rules check's errors on a routed board.
pub fn drc_errors(board: &Board) -> Vec<String> {
    let report = netsmith_fab::drc::check(
        &netsmith_pcb::kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &Default::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    report.errors().map(|f| f.to_string()).collect()
}

#[test]
fn the_mixed_board_is_placed_in_35_by_60_mm_with_parts_on_both_faces() {
    let board = mixed_board(0, AREA, Mix::DEFAULT).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(board.check_placement(), vec![]);
    assert!(board.parts.len() >= 100, "{} parts", board.parts.len());
    assert!(
        board
            .parts
            .iter()
            .any(|p| p.footprint.name.starts_with("QFN"))
    );
    assert!(board.parts.iter().any(|p| p.side == Side::Back));
    let (w, h) = (board.outline.width(), board.outline.height());
    assert!(
        (w - AREA.0).abs() < 1e-6 && (h - AREA.1).abs() < 1e-6,
        "{w} x {h}"
    );
}

/// The least distance between two parts' courtyards (0 if they touch), in mm.
fn courtyard_gap(board: &Board, a: &str, b: &str) -> f64 {
    let (a, b) = (
        board.part(a).unwrap().courtyard(),
        board.part(b).unwrap().courtyard(),
    );
    let dx = (a.min.x - b.max.x).max(b.min.x - a.max.x).max(0.0);
    let dy = (a.min.y - b.max.y).max(b.min.y - a.max.y).max(0.0);
    dx.hypot(dy)
}

#[test]
fn two_busy_packages_that_a_dozen_nets_join_are_not_pulled_together_past_the_room_their_pads_need()
{
    // The two QFN packages are joined by many nets, and the pull of the ratsnest used to leave
    // 0.5 mm between their courtyards (1 mm between the ends of their pads): the pads that
    // face each other could not leave. Their escape spaces (1.47 and 0.99 mm) add up now,
    // and the placer keeps them (3.5 mm on seeds 0 to 3).
    for seed in 0..4 {
        let board = mixed_board(seed, AREA, Mix::DEFAULT).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(board.check_placement(), vec![], "seed {seed}");
        let gap = courtyard_gap(&board, "U1", "U2");
        eprintln!("seed {seed}: {gap:.2} mm between U1 and U2");
        assert!(gap >= 2.0, "seed {seed}: {gap:.2} mm between U1 and U2");
    }
}

#[test]
fn the_mixed_board_the_router_cannot_finish_leaves_named_lines_and_only_copper_that_keeps_the_rules()
 {
    // Seed 3 leaves lines at QFN pads (see `measured`). Stopped early, what was routed must
    // still pass the design rules check, and every line left is named.
    let mut board = mixed_board(3, AREA, Mix::DEFAULT).unwrap_or_else(|e| panic!("{e}"));
    let routing = route_with(
        &mut board,
        RouteOptions {
            time_limit: Some(Duration::from_secs(10)),
            ..RouteOptions::default()
        },
    );
    assert!(!routing.left.is_empty());
    assert!(routing.left.lines.iter().all(|l| !l.net.is_empty()));
    assert!(!board.tracks.is_empty() && !board.vias.is_empty());
    let errors: Vec<String> = drc_errors(&board)
        .into_iter()
        .filter(|e| !e.to_lowercase().contains("unconnected"))
        .collect();
    assert_eq!(errors, Vec::<String>::new());
}

/// The aim of issue #3 for this board: every connection routed, on every seed (the first four
/// are run here; `measured` prints the others).
#[test]
fn the_mixed_board_is_routed_completely() {
    for seed in 0..4 {
        let mut board = mixed_board(seed, AREA, Mix::DEFAULT).unwrap_or_else(|e| panic!("{e}"));
        let routing = route_with(
            &mut board,
            RouteOptions {
                time_limit: Some(Duration::from_secs(120)),
                ..RouteOptions::default()
            },
        );
        assert!(!routing.stopped);
        assert!(
            routing.left.is_empty(),
            "seed {seed}, unrouted: {:#?}",
            routing.left.lines
        );
        assert_eq!(drc_errors(&board), Vec::<String>::new(), "seed {seed}");
    }
}

/// Seed 7 is one that ripping up the nets in a line's way leaves three lines of; the negotiation
/// and the lines ripped up one for another finish it. (It was seed 2 until the via became
/// JLCPCB's standard-price 0.2 in 0.45 mm, issue #431; seed 2 now needs the other orders of
/// `the_mixed_board_is_routed_completely`.) Nothing in that is random: the same board is routed
/// to the same copper.
#[test]
fn lines_ripped_up_for_lines_ripped_up_finish_a_board_and_the_same_board_gives_the_same_copper() {
    let route = |negotiate: bool| {
        let mut board = mixed_board(7, AREA, Mix::DEFAULT).unwrap_or_else(|e| panic!("{e}"));
        let routing = route_with(
            &mut board,
            RouteOptions {
                retry_orders: false,
                negotiate,
                time_limit: Some(Duration::from_secs(120)),
                ..RouteOptions::default()
            },
        );
        assert!(!routing.stopped);
        (board, routing.left)
    };
    let (_, without) = route(false);
    assert!(!without.is_empty(), "the rip-up alone leaves lines");
    let route = || {
        let (board, left) = route(true);
        assert!(left.is_empty(), "{:#?}", left.lines);
        board
    };
    let (a, b) = (route(), route());
    assert_eq!(a.tracks, b.tracks);
    assert_eq!(a.vias, b.vias);
    assert_eq!(drc_errors(&a), Vec::<String>::new());
}

/// Prints, for each seed in `NETSMITH_SEEDS` (`0..8` by default), how the placer and the router
/// ended; `NETSMITH_AREA=30x55` changes the outline. `NETSMITH_MIX=decoupling,pull_ups,bulk`.
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
    let mix = std::env::var("NETSMITH_MIX")
        .ok()
        .and_then(|m| {
            let v: Vec<usize> = m.split(',').filter_map(|x| x.parse().ok()).collect();
            Some(Mix {
                decoupling: *v.first()?,
                pull_ups: *v.get(1)?,
                bulk: *v.get(2)?,
            })
        })
        .unwrap_or(Mix::DEFAULT);
    let (mut done, mut lines, mut total) = (0, 0, Duration::ZERO);
    for seed in a..b {
        let started = Instant::now();
        let mut board = match mixed_board(seed, area, mix) {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "seed {seed}: does not place in {area:?}: {}",
                    e.to_string().lines().next().unwrap_or("")
                );
                continue;
            }
        };
        let placed = started.elapsed();
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
        common::write_measured(&board, &format!("mixed-seed{seed}"));
        // What the routed copper breaks in the design rules check (an unrouted line is not one).
        let broken = drc_errors(&board)
            .iter()
            .filter(|e| !e.to_lowercase().contains("unconnected"))
            .count();
        eprintln!(
            "seed {seed}: {} parts, placed in {placed:.1?} (problems {}), left {} lines, tries {}, vias {}, rules broken {broken}, routed in {took:.1?}",
            board.parts.len(),
            problems.len(),
            routing.left.len(),
            routing.tries,
            board.vias.len(),
        );
        for l in &routing.left.lines {
            eprintln!("    {l}");
            for (pad, at) in [(&l.from, l.from_at), (&l.to, l.to_at)] {
                if pad.refdes.starts_with('U') {
                    let part = board.part(&pad.refdes).unwrap();
                    eprintln!(
                        "        {}.{} at ({:.2}, {:.2}) = part {:?} turned {} on {:?}, offset ({:.3}, {:.3})",
                        pad.refdes,
                        pad.number,
                        at.x,
                        at.y,
                        (part.at.x, part.at.y),
                        part.rotation,
                        part.side,
                        at.x - part.at.x,
                        at.y - part.at.y
                    );
                }
            }
        }
        if let Ok(part) = std::env::var("NETSMITH_PICTURE") {
            crate::tight::picture(&board, &routing.left, &part);
        }
        if std::env::var_os("NETSMITH_PICTURE_LEFT").is_some() {
            // A close picture about the first pad of every line left, on the part it is of.
            for l in &routing.left.lines {
                let (pad, at) = if l.from.refdes.starts_with('U') {
                    (&l.from, l.from_at)
                } else {
                    (&l.to, l.to_at)
                };
                eprintln!("PICTURE of {} pad {}", pad.refdes, pad.number);
                crate::tight::picture_step(&board, &routing.left, &pad.refdes, Some(at), 0.05);
            }
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
