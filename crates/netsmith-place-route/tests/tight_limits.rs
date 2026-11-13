//! Invented boards at the limit of what fits, for the placer's face split, its large parts,
//! its last pass and the parts a requirement pins (ADR 0033 and ADR 0045, sections 10 and 11,
//! PROVISIONAL).
//!
//! Every board is made here: rectangular two-pad parts of a given courtyard, a through-hole
//! "mounting hole", and nets from a small generator, so the same seed is the same board on
//! every machine. `NETSMITH_SEEDS=0..40 cargo test --release -p netsmith-pcb --test
//! main tight_limits::measured -- --ignored --nocapture` prints, for each seed, how the placer ended.

use crate::common;

use netsmith_core::{
    Design, Features, Keepout, Net, Outline, Part, PartHeight, PinRef, PlaceBound, PlaceField,
    Placement,
};
use netsmith_kicad::testlib::TempDir;
use netsmith_pcb::{Board, BoardError, FootprintLibrary, PlaceOptions};

use common::process;

/// A footprint of `w` x `h` mm of courtyard with a pad at each end of the long side, or, with
/// `hole`, one plated hole in the middle of a square courtyard.
fn footprint(name: &str, w: f64, h: f64, hole: bool) -> String {
    let (x, y) = (w / 2.0, h / 2.0);
    let (attr, pads) = if hole {
        (
            "through_hole",
            "(pad \"1\" thru_hole circle (at 0 0) (size 3 3) (drill 2) (layers \"*.Cu\" \"*.Mask\"))"
                .to_string(),
        )
    } else {
        let (px, py) = if w >= h {
            (x - 0.8, 0.0)
        } else {
            (0.0, y - 0.8)
        };
        (
            "smd",
            format!(
                "(pad \"1\" smd rect (at {} {}) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\")) \
                 (pad \"2\" smd rect (at {px} {py}) (size 1 1) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\"))",
                -px, -py
            ),
        )
    };
    format!(
        "(footprint \"{name}\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         (property \"Reference\" \"REF**\" (at 0 {ry} 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (property \"Value\" \"{name}\" (at 0 {vy} 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (attr {attr}) {pads} \
         (fp_rect (start {nx} {ny}) (end {x} {y}) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n",
        ry = -y - 1.0,
        vy = y + 1.0,
        nx = -x,
        ny = -y,
    )
}

/// A part of the invented board.
#[derive(Clone)]
pub struct Spec {
    pub refdes: String,
    /// Courtyard width and height in mm.
    pub size: (f64, f64),
    pub hole: bool,
    /// The height the design states for it, in mm.
    pub height: Option<f64>,
    /// What the design requires of its place.
    pub side: Option<&'static str>,
    pub bounds: Vec<(PlaceField, f64, f64)>,
}

impl Spec {
    pub fn new(refdes: &str, w: f64, h: f64) -> Spec {
        Spec {
            refdes: refdes.into(),
            size: (w, h),
            hole: false,
            height: None,
            side: None,
            bounds: Vec::new(),
        }
    }
    fn path(&self) -> String {
        self.refdes.to_lowercase()
    }
    fn package(&self) -> String {
        format!(
            "{}{}x{}",
            if self.hole { "H" } else { "P" },
            self.size.0,
            self.size.1
        )
    }
}

/// A small generator of numbers, the same on every machine.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn draw(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// The nets of a board: the second pad of every part is joined to the first pad of another, chosen from the
/// generator, so that the placer has wires to pull on.
pub fn nets_for(parts: &[Spec], seed: u64) -> Vec<Net> {
    let smd: Vec<&Spec> = parts.iter().filter(|p| !p.hole).collect();
    let mut rng = Lcg(seed.wrapping_mul(0x9E37_79B9) ^ 0x5151);
    let mut nets = Vec::new();
    for (i, p) in smd.iter().enumerate() {
        let q = smd[(i + 1 + rng.draw() as usize % (smd.len() - 1)) % smd.len()];
        nets.push(Net {
            name: format!("N{}", nets.len()),
            pins: vec![
                PinRef {
                    refdes: p.refdes.clone(),
                    pin: "2".into(),
                },
                PinRef {
                    refdes: q.refdes.clone(),
                    pin: "1".into(),
                },
            ],
        });
    }
    // A pad is on one net only.
    let mut seen = std::collections::BTreeSet::new();
    nets.retain(|n| {
        n.pins
            .iter()
            .all(|p| seen.insert((p.refdes.clone(), p.pin.clone())))
    });
    nets
}

/// The board of `parts` on an outline of `size`, with `keepouts`, placed as `options` say.
pub fn board(
    parts: &[Spec],
    size: (f64, f64),
    keepouts: Vec<Keepout>,
    seed: u64,
    options: &PlaceOptions,
) -> Result<Board, BoardError> {
    let folder = TempDir::new("tight-limits");
    let mut mapping = String::new();
    let mut done = std::collections::BTreeSet::new();
    for p in parts {
        if done.insert(p.package()) {
            std::fs::write(
                folder.join(format!("{}.kicad_mod", p.package())),
                footprint(&p.package(), p.size.0, p.size.1, p.hole),
            )
            .unwrap();
            mapping += &format!(
                "[[part]]\nkind = \"{}\"\npackage = \"{}\"\nsymbol = \"t:R\"\nfootprint = \"t:{}\"\n",
                if p.hole { "MountingHole" } else { "Module" },
                p.package(),
                p.package()
            );
        }
    }
    let design = Design {
        interfaces: Vec::new(),
        notes: Vec::new(),
        name: "Tight".into(),
        process: Some(process()),
        features: Features {
            outline: Some(Outline {
                width: (size.0, size.0),
                height: (size.1, size.1),
                corner_radius: 0.0,
            }),
            keepouts,
            heights: parts
                .iter()
                .filter_map(|p| {
                    Some(PartHeight {
                        path: p.path(),
                        height: p.height?,
                    })
                })
                .collect(),
            constraints: parts
                .iter()
                .filter(|p| p.side.is_some() || !p.bounds.is_empty())
                .map(|p| Placement {
                    path: p.path(),
                    module: false,
                    bounds: p
                        .bounds
                        .iter()
                        .map(|&(field, lo, hi)| PlaceBound {
                            field,
                            lo,
                            hi,
                            source: String::new(),
                        })
                        .collect(),
                    side: p.side.map(str::to_string),
                    side_source: String::new(),
                })
                .collect(),
            ..Features::default()
        },
        parts: parts
            .iter()
            .map(|p| Part {
                provides: Vec::new(),
                refdes: p.refdes.clone(),
                kind: if p.hole { "MountingHole" } else { "Module" }.into(),
                value: p.package(),
                package: Some(p.package()),
                path: p.path(),
                pins: Vec::new(),
            })
            .collect(),
        nets: nets_for(parts, seed),
    };
    let mapping = netsmith_kicad::Mapping::parse(&mapping, "tight", std::path::Path::new("."))
        .unwrap_or_else(|e| panic!("{e}"));
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
        .with_library("t", folder.path());
    // The mapping above names the library `t`.
    let options = PlaceOptions {
        seed,
        ..options.clone()
    };
    netsmith_place_route::board_from_design_placed(&design, &mapping, &library, &options)
}

/// A keepout over a box of the board that limits the height on `layers`, or forbids parts.
pub fn zone(
    name: &str,
    at: (f64, f64),
    size: (f64, f64),
    layers: &[&str],
    max_height: Option<f64>,
) -> Keepout {
    Keepout {
        name: name.into(),
        part: None,
        at: Some(at),
        size,
        layers: layers.iter().map(|l| l.to_string()).collect(),
        tracks: false,
        vias: false,
        copper: false,
        parts: max_height.is_none(),
        max_height,
    }
}

/// How a run ended, in a line: the refusal's first line, or the placement check's problems.
pub fn outcome(result: &Result<Board, BoardError>) -> String {
    match result {
        Ok(b) => {
            let problems = b.check_placement();
            if problems.is_empty() {
                "ok".to_string()
            } else {
                format!(
                    "{} problem(s): {}",
                    problems.len(),
                    problems
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                )
            }
        }
        Err(e) => e.to_string().lines().next().unwrap_or("").to_string(),
    }
}

/// Whether the run came out with no refusal and no placement problem.
pub fn is_ok(result: &Result<Board, BoardError>) -> bool {
    matches!(result, Ok(b) if b.check_placement().is_empty())
}

fn seeds() -> std::ops::Range<u64> {
    let range = std::env::var("NETSMITH_SEEDS").unwrap_or_else(|_| "0..8".into());
    let (a, b) = range.split_once("..").expect("a..b");
    a.parse().unwrap()..b.parse().unwrap()
}

// ---------------------------------------------------------------------------------------
// B1: the face split and height limits.

/// A 30 x 50 mm board with a height limit of 3 mm over the whole bottom face, a part of
/// 14 x 30 mm that is stated on the top face, one part of 4 mm that states no face, and
/// `count` small ones: more than the top face holds, so the parts are shared between the faces.
fn lid_board(seed: u64, count: usize) -> Result<Board, BoardError> {
    let mut parts = vec![
        {
            let mut p = Spec::new("J1", 14.0, 30.0);
            p.side = Some("top");
            p
        },
        {
            let mut p = Spec::new("U1", 10.0, 10.0);
            p.height = Some(4.0);
            p
        },
    ];
    for i in 1..=count {
        parts.push(Spec::new(&format!("R{i}"), 4.0, 2.5));
    }
    board(
        &parts,
        (30.0, 50.0),
        vec![zone(
            "lid",
            (0.0, 0.0),
            (30.0, 50.0),
            &["bottom"],
            Some(3.0),
        )],
        seed,
        &PlaceOptions::default(),
    )
}

#[test]
fn a_tall_part_that_states_no_face_stays_off_a_face_whose_height_limit_forbids_it() {
    for seed in 0..2 {
        let result = lid_board(seed, 60);
        let b = result
            .as_ref()
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        assert_eq!(b.check_placement(), vec![], "seed {seed}");
        assert_eq!(
            b.part("U1").unwrap().side,
            netsmith_pcb::Side::Front,
            "seed {seed}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// B2: a large part and a crowd of small ones.

/// A 35 x 60 mm board with one part of 15 x 37 mm of courtyard that must be on the bottom
/// face turned `turn` degrees, and `count` small parts on both faces, about half the area.
fn crowd(seed: u64, count: usize, turn: f64) -> Result<Board, BoardError> {
    let mut parts = vec![{
        let mut p = Spec::new("U1", 15.0, 37.0);
        p.side = Some("bottom");
        p.bounds = vec![(PlaceField::Rotation, turn, turn)];
        p
    }];
    let mut rng = Lcg(seed ^ 0xABCD);
    for i in 1..=count {
        let w = 4.0 + (rng.draw() % 3) as f64; // 4, 5 or 6
        let h = 3.0 + (rng.draw() % 2) as f64; // 3 or 4
        parts.push(Spec::new(&format!("R{i}"), w, h));
    }
    board(
        &parts,
        (35.0, 60.0),
        Vec::new(),
        seed,
        &PlaceOptions::default(),
    )
}

/// The large part of [`crowd`] with 115 small ones, on `seed`, is placed inside, on the bottom
/// face and turned as stated. Seeds 1 and 3 left the large part 4.85 and 0.55 mm outside before
/// the retry that puts it in a corner first; each is a test of its own, so that a test run deals
/// them out side by side.
fn brings_the_large_part_inside(seed: u64) {
    let result = crowd(seed, 115, 0.0);
    let b = result
        .as_ref()
        .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    assert_eq!(b.check_placement(), vec![], "seed {seed}");
    let u = b.part("U1").unwrap();
    assert_eq!(u.side, netsmith_pcb::Side::Back, "seed {seed}");
    assert_eq!(u.rotation, 0.0, "seed {seed}");
}

#[test]
fn a_large_part_with_a_stated_face_and_turn_is_brought_inside_among_a_crowd_of_small_ones_seed_1() {
    brings_the_large_part_inside(1);
}

#[test]
fn a_large_part_with_a_stated_face_and_turn_is_brought_inside_among_a_crowd_of_small_ones_seed_3() {
    brings_the_large_part_inside(3);
}

// ---------------------------------------------------------------------------------------
// B5: packing at the limit.

/// A 35 x 60 mm board with `count` small parts and nothing else, on both faces.
fn packed(seed: u64, count: usize) -> Result<Board, BoardError> {
    let mut rng = Lcg(seed ^ 0x1234);
    let parts: Vec<Spec> = (1..=count)
        .map(|i| {
            let w = 4.0 + (rng.draw() % 3) as f64;
            let h = 3.0 + (rng.draw() % 2) as f64;
            Spec::new(&format!("R{i}"), w, h)
        })
        .collect();
    board(
        &parts,
        (35.0, 60.0),
        Vec::new(),
        seed,
        &PlaceOptions::default(),
    )
}

/// The packed board placed, on `seed`, has no placement problem. Seeds 0 and 2 ended with one
/// part 0.05 mm inside its margin, and with two overlapping by 1.9 mm2, before the retries and
/// the last fine pass; each is a test of its own, so that a test run deals them out side by side.
fn packs(seed: u64, result: &Result<Board, BoardError>) {
    let b = result
        .as_ref()
        .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
    assert_eq!(b.check_placement(), vec![], "seed {seed}");
}

/// `packed(2, 160)`, placed once for the tests that check it (when they run in one process, as
/// `cargo test` runs them; nextest runs each test in a process of its own).
fn packed_seed_2() -> &'static Result<Board, BoardError> {
    static BOARD: std::sync::OnceLock<Result<Board, BoardError>> = std::sync::OnceLock::new();
    BOARD.get_or_init(|| packed(2, 160))
}

#[test]
fn a_board_packed_at_the_limit_is_placed_by_a_retry_where_the_first_run_missed_by_a_hair_seed_0() {
    packs(0, &packed(0, 160));
}

#[test]
fn a_board_packed_at_the_limit_is_placed_by_a_retry_where_the_first_run_missed_by_a_hair_seed_2() {
    packs(2, packed_seed_2());
}

#[test]
fn the_same_input_gives_the_same_board_after_a_retry() {
    // The second placing runs beside the shared one, not after it.
    let (a, b) = std::thread::scope(|s| {
        let again = s.spawn(|| packed(2, 160));
        let a = packed_seed_2().as_ref().unwrap();
        (a, again.join().unwrap().unwrap())
    });
    let at = |b: &Board| -> Vec<_> {
        b.parts
            .iter()
            .map(|p| (p.at.x.to_bits(), p.at.y.to_bits(), p.rotation.to_bits()))
            .collect()
    };
    assert_eq!(at(a), at(&b));
}

// ---------------------------------------------------------------------------------------
// B6: parts a requirement pins inside a keepout that forbids parts.

/// A 35 x 60 mm board, a keepout for parts, two holes pinned inside it by position, and a
/// few small parts.
fn pegs(seed: u64) -> Result<Board, BoardError> {
    let mut parts = Vec::new();
    for (name, x, y) in [("H1", 6.0, 8.0), ("H2", 29.0, 8.0)] {
        let mut h = Spec::new(name, 6.0, 6.0);
        h.hole = true;
        h.bounds = vec![(PlaceField::X, x, x), (PlaceField::Y, y, y)];
        parts.push(h);
    }
    for i in 1..=10 {
        parts.push(Spec::new(&format!("R{i}"), 4.0, 2.5));
    }
    board(
        &parts,
        (35.0, 60.0),
        vec![zone(
            "peg_bases",
            (0.0, 0.0),
            (35.0, 14.0),
            &["top", "bottom"],
            None,
        )],
        seed,
        &PlaceOptions::default(),
    )
}

#[test]
fn a_hole_pinned_inside_a_keepout_for_parts_is_refused_before_placing_for_every_seed() {
    // A requirement that pins a part does not override a keepout that forbids parts (the rule
    // of `crates/netsmith/tests/conflicts.rs`, ADR 0045 section 13), and the lines prove it
    // before the placer starts (section 14): the refusal is the same for every seed, because
    // no seed is tried, and it names both holes and the keepout (issue #99).
    let texts: Vec<String> = (0..8)
        .map(|seed| match pegs(seed) {
            Err(BoardError::Unplaceable(found)) => {
                assert!(found.faults.is_empty(), "{found:?}");
                found.errors().join("\n")
            }
            other => panic!("seed {seed}: the pegs must be refused before placing: {other:?}"),
        })
        .collect();
    assert!(
        texts.iter().all(|t| *t == texts[0]),
        "the seeds differ: {texts:#?}"
    );
    for want in [
        "these lines cannot both hold: ",
        "the courtyard of `h1` (H1)",
        "the courtyard of `h2` (H2)",
        "the keepout `peg_bases`",
        "which forbids parts",
    ] {
        assert!(texts[0].contains(want), "no `{want}` in {}", texts[0]);
    }
}

// ---------------------------------------------------------------------------------------

/// Prints, for each seed, how each repro ended.
#[test]
#[ignore = "a measurement, not a check"]
fn measured() {
    let which = std::env::var("NETSMITH_BOARD").unwrap_or_else(|_| "all".into());
    let mut tally = std::collections::BTreeMap::<&str, (usize, usize)>::new();
    for seed in seeds() {
        let mut run = |name: &'static str, result: Result<Board, BoardError>| {
            if which != "all" && which != name {
                return;
            }
            let t = tally.entry(name).or_default();
            t.1 += 1;
            t.0 += usize::from(is_ok(&result));
            eprintln!(
                "{name} seed {seed}: {}{}",
                outcome(&result),
                result
                    .as_ref()
                    .ok()
                    .and_then(|b| b.part("U1"))
                    .map_or(String::new(), |u| format!(" [U1 on {:?}]", u.side))
            );
        };
        if which == "all" || which == "lid" {
            run(
                "lid",
                lid_board(
                    seed,
                    std::env::var("NETSMITH_COUNT").map_or(60, |c| c.parse().unwrap()),
                ),
            );
        }
        if which == "all" || which == "crowd" {
            run(
                "crowd",
                crowd(
                    seed,
                    std::env::var("NETSMITH_COUNT").map_or(80, |c| c.parse().unwrap()),
                    0.0,
                ),
            );
        }
        if which == "packed" {
            run(
                "packed",
                packed(
                    seed,
                    std::env::var("NETSMITH_COUNT").map_or(130, |c| c.parse().unwrap()),
                ),
            );
        }
        if which == "all" || which == "pegs" {
            run("pegs", pegs(seed));
        }
    }
    for (name, (ok, all)) in tally {
        eprintln!("{name}: {ok} of {all} seeds ok");
    }
}
