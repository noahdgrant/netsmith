//! The layout file beside a source file (ADR 0020 item 7, ADR 0032): `<stem>.pcb.toml`.
//!
//! It is optional. `fab` and the editor's PCB view read it when it exists. It tells the placer
//! which face each part is on and what to keep clear of
//! ([`netsmith_pcb::place_options`]), and then puts what it holds on the placed board with
//! [`netsmith_pcb::apply_layout`].
//!
//! Why a file beside the source, with the geometry of a placement in it (ADR 0020 item 7,
//! decided by the owner on 2026-11-02, and amended by ADR 0048: a requirement on where a part
//! goes is in the physical file, and the solutions stay here). A generated `.kicad_pcb` is
//! never read back (ADR 0006), so a hand layout survives regeneration only if netsmith owns
//! where it is stored. The file is keyed by instance path (`stage.output_cap`), which does not
//! change when reference designators are renumbered. A part with no entry is placed by the
//! placer, a `locked` entry is fixed and the placer places the rest round it, and an entry for
//! a path that is gone is an error naming it. The source stays free of geometry. What the
//! circuit implies is derived by the placer (a capacitor between a rail's two pins on the net
//! of a supply pin of a part is a decoupling capacitor, and parts of one module are grouped
//! by instance path). Rejected: constraints in the source (`.c1 near .u.vdd`): a hint that
//! travels with the module is an advantage, but `near` is an unordered relation over geometry
//! that needs a distance and a checker, it cannot say a fixed position (so a file is needed
//! anyway), and a module can name only its own parts. A `near` or `group` statement waits for
//! a case the derived placement gets wrong, and is then a decision with a real example.

use std::path::{Path, PathBuf};

use netsmith_pcb::Layout;

/// The layout file of a source file: `led.nsm` has `led.pcb.toml`.
pub fn layout_path(source: &Path) -> PathBuf {
    source.with_extension("pcb.toml")
}

/// The layout in the file at `file`, with the name to quote it by, or `None` if there is no
/// such file. `boards` are the names of all the boards of the source file, so that a table
/// for a board that is gone is found. A file that cannot be read or has a wrong table is an
/// error naming it.
pub fn read(file: &Path, boards: &[String]) -> Result<Option<(Layout, String)>, String> {
    let name = file.display().to_string();
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let layout = Layout::parse(&text, &name).map_err(|e| e.to_string())?;
    let known: Vec<&str> = boards.iter().map(String::as_str).collect();
    layout
        .check_boards(&name, &known)
        .map_err(|e| e.to_string())?;
    Ok(Some((layout, name)))
}

/// How many seeds the placer tries for a board before it gives up, when `netsmith.toml` does
/// not say (`place_seeds`, issue #458): the layout file's seed (0 when it states none) and
/// the ones after it.
pub const DEFAULT_PLACE_SEEDS: u32 = 8;

/// How many seeds the placer tries for a board of `project`: its `place_seeds`, or
/// [`DEFAULT_PLACE_SEEDS`].
pub fn place_seeds(project: Option<&crate::project::Project>) -> u32 {
    project
        .and_then(|p| p.manifest.place_seeds)
        .unwrap_or(DEFAULT_PLACE_SEEDS)
}

/// Why [`place`] made no board.
pub enum PlaceFailure {
    /// The design cannot be made into a placed board (a footprint, a process, the placer).
    Board(netsmith_pcb::BoardError),
    /// The layout file does not fit the placed board.
    Layout(String),
    /// No seed tried fits the parts in the outline: the refusal of the closest try, and what
    /// was tried.
    Unplaced {
        error: Box<netsmith_pcb::DoesNotFit>,
        seeds: Seeds,
    },
}

impl PlaceFailure {
    /// The message, as a person reads it.
    pub fn message(&self) -> String {
        match self {
            PlaceFailure::Board(e) => e.to_string(),
            PlaceFailure::Layout(e) => e.clone(),
            PlaceFailure::Unplaced { error, seeds } => format!("{}\n{error}", seeds.report()),
        }
    }
}

/// A board placed by [`place`]: the board, the options the placer was given (for what
/// measures the requirements afterwards) and the seeds it took.
pub struct Placing {
    pub board: netsmith_pcb::Board,
    pub options: netsmith_pcb::PlaceOptions,
    pub seeds: Seeds,
}

/// The seeds [`place`] tried for a board, and which one the board it gives is of.
#[derive(Debug, Clone, PartialEq)]
pub struct Seeds {
    /// The seed tried first: the layout file's, or 0.
    pub first: u64,
    /// Whether the layout file states it.
    pub stated: bool,
    /// How many seeds may be tried (`place_seeds`).
    pub limit: u32,
    /// The seed of the board given.
    pub used: u64,
    /// Whether it places the board; if not, the board given is the closest try, and
    /// `blockers` says what is in its way.
    pub placed: bool,
    /// Each seed tried that does not place the board, in order, with the first thing that
    /// blocks it there.
    pub missed: Vec<(u64, String)>,
    /// What blocks the closest try, when no seed places the board: one sentence each, the
    /// members by path with the designator beside, and by how much.
    pub blockers: Vec<String>,
}

impl Seeds {
    /// Whether a seed other than the first placed the board: the one to write to the layout
    /// file.
    pub fn changed(&self) -> bool {
        self.placed && self.used != self.first
    }

    /// The seeds that missed, as a list: `seed 3 (...); seed 4 (...)`.
    fn missed_list(&self) -> String {
        self.missed
            .iter()
            .map(|(seed, why)| format!("seed {seed} ({why})"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// The sentence for a board that a seed after the first placed (`None` if the first did,
    /// or none did): the first seed, stale if the layout file states it, what blocked each
    /// seed tried, and the seed that places it. `file` is the layout file to name.
    pub fn note(&self, file: &str) -> Option<String> {
        if !self.changed() {
            return None;
        }
        let head = if self.stated {
            format!(
                "the seed {} that {file} states no longer places the board, so it is stale",
                self.first
            )
        } else {
            format!(
                "seed {} (the default: {file} states no seed) does not place the board",
                self.first
            )
        };
        Some(format!(
            "{head}: the placer tried {}; seed {} places it",
            self.missed_list(),
            self.used
        ))
    }

    /// The report for a board that no seed places: how many were tried, which, how to try
    /// more, the closest, and what blocks it there.
    pub fn report(&self) -> String {
        let tried = self.missed.len();
        let mut out = if tried <= 1 {
            format!(
                "seed {} does not place the board, and no other was tried (`place_seeds = {}` in netsmith.toml; a larger number tries the seeds after it)",
                self.first, self.limit
            )
        } else {
            let last = self.missed.last().map_or(self.first, |(s, _)| *s);
            format!(
                "no seed places the board: the placer tried {tried} seeds, {} to {last} (`place_seeds = {}` in netsmith.toml sets how many), and the closest is seed {}",
                self.first, self.limit, self.used
            )
        };
        if self.blockers.is_empty() {
            out.push('.');
        } else {
            out.push_str(", where this blocks it:");
            for b in &self.blockers {
                out.push_str(&format!("\n  {b}"));
            }
        }
        out
    }
}

/// One seed's try: a board (here, the placed board and its options), or a refusal.
enum Try<B> {
    /// A board, with what blocks it (empty: the seed places it) and how badly it misses.
    Board {
        board: B,
        blockers: Vec<String>,
        badness: (usize, f64),
    },
    /// The parts do not fit the outline.
    Short(Box<netsmith_pcb::DoesNotFit>),
}

impl<B> Try<B> {
    fn badness(&self) -> (usize, f64) {
        match self {
            Try::Board { badness, .. } => *badness,
            Try::Short(e) => e.badness(),
        }
    }

    fn blockers(&self) -> Vec<String> {
        match self {
            Try::Board { blockers, .. } => blockers.clone(),
            Try::Short(e) => e.blockers(),
        }
    }
}

/// Try the seeds from `first`, up to `limit` of them, with `run`, and give the board of the
/// first that places it, or the closest try (the fewest faults, then the least area and
/// distance; the earlier seed on a tie) with what blocks it. An error of `run` is given at
/// once: no seed changes it.
fn search<B>(
    first: u64,
    stated: bool,
    limit: u32,
    mut run: impl FnMut(u64) -> Result<Try<B>, PlaceFailure>,
) -> Result<(B, Seeds), PlaceFailure> {
    let limit = limit.max(1);
    let mut missed = Vec::new();
    let mut best: Option<(u64, Try<B>)> = None;
    for k in 0..limit {
        // The layout file's seed is a u32; the next ones wrap round within it.
        let seed = u64::from(u32::try_from(first).unwrap_or(0).wrapping_add(k));
        let attempt = run(seed)?;
        if let Try::Board { blockers, .. } = &attempt
            && blockers.is_empty()
        {
            let Try::Board { board, .. } = attempt else {
                unreachable!("matched above")
            };
            let seeds = Seeds {
                first,
                stated,
                limit,
                used: seed,
                placed: true,
                missed,
                blockers: Vec::new(),
            };
            return Ok((board, seeds));
        }
        let why = attempt
            .blockers()
            .into_iter()
            .next()
            .unwrap_or_else(|| "the parts do not fit the outline".to_string());
        missed.push((seed, why));
        if best
            .as_ref()
            .is_none_or(|(_, b)| attempt.badness() < b.badness())
        {
            best = Some((seed, attempt));
        }
    }
    let (used, closest) = best.expect("at least one seed is tried");
    let seeds = Seeds {
        first,
        stated,
        limit,
        used,
        placed: false,
        missed,
        blockers: closest.blockers(),
    };
    match closest {
        Try::Board { board, .. } => Ok((board, seeds)),
        Try::Short(error) => Err(PlaceFailure::Unplaced { error, seeds }),
    }
}

/// What blocks a placed board that another seed could change, with how badly it misses:
/// every fault of the placement check when one of them is about no part the layout file puts
/// and about at least one part the placer moved (one no requirement pins), and, with no layout file, the requirements of the
/// physical file it does not meet (unless they are warnings for the run), as `fab` refuses
/// them. A fault about a part the layout file puts is the file's (a part moved by hand is put
/// where the file says after the placer, whatever the seed), and one about parts the
/// requirements pin is theirs: no seed moves it, and the board
/// is then placed as far as the placer goes, and `fab` reports the fault.
fn blockers_of(
    board: &netsmith_pcb::Board,
    design: &netsmith_core::Design,
    options: &netsmith_pcb::PlaceOptions,
    layout: Option<&Layout>,
    lines_warn: bool,
) -> (Vec<String>, (usize, f64)) {
    let entries = layout.and_then(|l| l.boards.get(&design.name));
    // Whether the layout file puts the part (a hand's place, whatever the seed).
    let in_file = |refdes: &str| {
        board
            .part(refdes)
            .is_none_or(|p| entries.is_some_and(|e| e.parts.contains_key(&p.path)))
    };
    let problems = board.check_placement();
    let mut blockers = Vec::new();
    let mut amount = 0.0;
    let mut count = 0;
    // A fault another seed may move: no part of it is the layout file's, and one of them is
    // the placer's to move (not pinned by a requirement).
    let the_placers = |p: &netsmith_pcb::PlacementProblem| {
        let parts = p.parts();
        !parts.iter().any(|r| in_file(r)) && parts.iter().any(|r| !board.pinned.contains(*r))
    };
    if problems.iter().any(the_placers) {
        for p in &problems {
            blockers.push(p.to_string());
            amount += p.amount();
        }
        count += problems.len();
    }
    if layout.is_none()
        && !lines_warn
        && let Some(report) = board.explain_requirements(design, options)
    {
        count += report.unmet.len();
        blockers.extend(report.unmet);
    }
    (blockers, (count, amount))
}

/// The board of `design` placed as `fab`, the editor's PCB view and `board3d` place it: the
/// layout file's faces and locks tell the placer what to keep, and its positions are then put on
/// the board. `lines_warn` makes an unmet requirement of the physical file a warning (ADR 0045).
///
/// When the layout file's seed (0 if it states none) does not place the board, the seeds after
/// it are tried, up to `seeds` in all (`place_seeds` in `netsmith.toml`, issue #458): a seed
/// places the board when the placer fits the parts, the placement check finds no fault the
/// placer could move and, with no layout file, every requirement of the physical file is met.
/// When none does, the closest try is given (the fewest faults, then the least area and
/// distance), with what blocks it: as the board, or as the refusal for parts that do not fit.
/// A failure no seed changes (a footprint, a process, a requirement proved not to hold) is
/// given at once.
pub fn place(
    design: &netsmith_core::Design,
    layout: Option<&(Layout, String)>,
    libraries: &netsmith_pipeline::Libraries,
    lines_warn: bool,
    seeds: u32,
) -> Result<Placing, PlaceFailure> {
    let mut base = netsmith_pcb::place_options(design, layout.map(|l| &l.0));
    base.lines_warn = lines_warn;
    let stated = layout
        .and_then(|l| l.0.boards.get(&design.name))
        .and_then(|b| b.seed)
        .is_some();
    let ((board, options), seeds) = search(base.seed, stated, seeds, |seed| {
        let mut options = base.clone();
        options.seed = seed;
        match netsmith_placer::board_from_design_placed(
            design,
            &libraries.mapping,
            &libraries.footprints,
            &options,
        ) {
            Ok(mut board) => {
                if let Some((layout, file)) = layout {
                    netsmith_pcb::apply_layout(&mut board, layout, file)
                        .map_err(|e| PlaceFailure::Layout(e.to_string()))?;
                }
                let (blockers, badness) =
                    blockers_of(&board, design, &options, layout.map(|l| &l.0), lines_warn);
                Ok(Try::Board {
                    board: (board, options),
                    blockers,
                    badness,
                })
            }
            Err(netsmith_pcb::BoardError::DoesNotFit(e)) if e.seed_may_help() => {
                Ok(Try::Short(Box::new(e)))
            }
            Err(e) => Err(PlaceFailure::Board(e)),
        }
    })?;
    Ok(Placing {
        board,
        options,
        seeds,
    })
}

/// Write `seed` as the seed of `board` in the layout file at `file`, keeping the rest of what
/// it holds; the file is made, with that alone, if it does not exist.
pub fn write_seed(file: &Path, board: &str, seed: u64) -> Result<(), String> {
    let name = file.display().to_string();
    let mut layout = match std::fs::read_to_string(file) {
        Ok(text) => Layout::parse(&text, &name).map_err(|e| e.to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Layout::default(),
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let seed = u32::try_from(seed).map_err(|_| format!("{name}: seed {seed} is too big"))?;
    layout.boards.entry(board.to_string()).or_default().seed = Some(seed);
    std::fs::write(file, layout.to_text()).map_err(|e| format!("{name}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A try of a board that `bad` faults block, each of `area` mm2.
    fn board(bad: usize, area: f64) -> Try<u64> {
        Try::Board {
            board: 0,
            blockers: (0..bad)
                .map(|k| {
                    format!("the courtyards of `a{k}` (C{k}) and `b` (R1) overlap by {area:.2} mm2")
                })
                .collect(),
            badness: (bad, area * bad as f64),
        }
    }

    #[test]
    fn the_seeds_after_the_first_are_tried_until_one_places_the_board() {
        let mut tried = Vec::new();
        let (got, seeds) = search(3, true, 8, |seed| {
            tried.push(seed);
            Ok(if seed == 5 {
                Try::Board {
                    board: seed,
                    blockers: Vec::new(),
                    badness: (0, 0.0),
                }
            } else {
                board(1, 0.5)
            })
        })
        .ok()
        .unwrap();
        assert_eq!(tried, [3, 4, 5]);
        assert_eq!(
            (got, seeds.used, seeds.placed, seeds.changed()),
            (5, 5, true, true)
        );
        assert_eq!(
            seeds.note("main.pcb.toml").unwrap(),
            "the seed 3 that main.pcb.toml states no longer places the board, so it is stale: the placer tried seed 3 (the courtyards of `a0` (C0) and `b` (R1) overlap by 0.50 mm2); seed 4 (the courtyards of `a0` (C0) and `b` (R1) overlap by 0.50 mm2); seed 5 places it"
        );
    }

    #[test]
    fn the_first_seed_that_places_the_board_says_nothing() {
        let (_, seeds) = search(0, false, 8, |seed| {
            Ok(Try::Board {
                board: seed,
                blockers: Vec::new(),
                badness: (0, 0.0),
            })
        })
        .ok()
        .unwrap();
        assert_eq!((seeds.used, seeds.changed()), (0, false));
        assert_eq!(seeds.note("main.pcb.toml"), None);
    }

    #[test]
    fn a_default_seed_that_does_not_place_the_board_is_said_as_the_default() {
        let (_, seeds) = search(0, false, 8, |seed| {
            Ok(if seed == 0 {
                board(1, 1.0)
            } else {
                Try::Board {
                    board: seed,
                    blockers: Vec::new(),
                    badness: (0, 0.0),
                }
            })
        })
        .ok()
        .unwrap();
        assert_eq!(
            seeds.note("main.pcb.toml").unwrap(),
            "seed 0 (the default: main.pcb.toml states no seed) does not place the board: the placer tried seed 0 (the courtyards of `a0` (C0) and `b` (R1) overlap by 1.00 mm2); seed 1 places it"
        );
    }

    #[test]
    fn when_no_seed_places_the_board_the_closest_is_given_with_what_blocks_it() {
        let mut tried = 0;
        let (got, seeds) = search(u64::from(u32::MAX), false, 3, |seed| {
            tried += 1;
            // The second seed (the first wraps to 0) misses by least.
            Ok(match seed {
                0 => Try::Board {
                    board: seed,
                    blockers: vec![
                        "the courtyards of `usb` (J2) and `c37` (C37) overlap by 0.42 mm2".into(),
                    ],
                    badness: (1, 0.42),
                },
                _ => board(2, 3.0),
            })
        })
        .ok()
        .unwrap();
        assert_eq!(tried, 3);
        assert_eq!((got, seeds.used, seeds.placed), (0, 0, false));
        assert_eq!(seeds.note("f"), None);
        assert_eq!(
            seeds.report(),
            "no seed places the board: the placer tried 3 seeds, 4294967295 to 1 (`place_seeds = 3` in netsmith.toml sets how many), and the closest is seed 0, where this blocks it:\n  the courtyards of `usb` (J2) and `c37` (C37) overlap by 0.42 mm2"
        );
    }

    #[test]
    fn one_seed_only_says_that_no_other_was_tried() {
        let (_, seeds) = search(7, true, 1, |_| Ok(board(1, 2.0))).ok().unwrap();
        assert_eq!(
            seeds.report(),
            "seed 7 does not place the board, and no other was tried (`place_seeds = 1` in netsmith.toml; a larger number tries the seeds after it), where this blocks it:\n  the courtyards of `a0` (C0) and `b` (R1) overlap by 2.00 mm2"
        );
    }

    /// A refusal where `R3` (`stage.r3`) is `by` mm outside the left edge.
    fn short(by: f64) -> Try<u64> {
        Try::Short(Box::new(netsmith_pcb::DoesNotFit {
            needs: (30.0, 20.0),
            outline: (25.0, 20.0),
            ended: Some(Box::new(netsmith_pcb::Ended {
                outside: 1,
                outside_parts: vec![netsmith_pcb::OutsideAt {
                    part: "R3".into(),
                    why: vec![format!("{by:.2} mm outside the left edge")],
                    worst: by,
                }],
                ..netsmith_pcb::Ended::default()
            })),
            wall: 0.5,
            oversize: Vec::new(),
            area: (100.0, 900.0),
            parts: vec![netsmith_pcb::PartState {
                part: "R3".into(),
                path: "stage.r3".into(),
                kind: "Resistor".into(),
                side: netsmith_pcb::Side::Front,
                at: (0.0, 0.0),
                rotation: 0.0,
                size: (1.0, 1.0),
                fixed: false,
                state: Vec::new(),
            }]
            .into_boxed_slice(),
            requirements: None,
        }))
    }

    #[test]
    fn when_no_seed_fits_the_parts_the_closest_refusal_is_given_with_the_seeds() {
        let failure = search::<u64>(0, false, 2, |seed| {
            Ok(short(if seed == 0 { 0.8 } else { 0.3 }))
        })
        .err()
        .unwrap();
        let PlaceFailure::Unplaced { error, seeds } = &failure else {
            panic!("not a refusal");
        };
        assert_eq!(seeds.used, 1);
        assert_eq!(
            seeds.missed,
            [
                (
                    0,
                    "`stage.r3` (R3): 0.80 mm outside the left edge".to_string()
                ),
                (
                    1,
                    "`stage.r3` (R3): 0.30 mm outside the left edge".to_string()
                ),
            ]
        );
        assert_eq!(
            failure.message(),
            format!(
                "no seed places the board: the placer tried 2 seeds, 0 to 1 (`place_seeds = 2` in netsmith.toml sets how many), and the closest is seed 1, where this blocks it:\n  `stage.r3` (R3): 0.30 mm outside the left edge\n{error}"
            )
        );
    }

    #[test]
    fn a_failure_no_seed_changes_is_given_at_once() {
        let mut tried = 0;
        let e = search::<u64>(0, false, 8, |_| {
            tried += 1;
            Err(PlaceFailure::Layout("the layout is wrong".into()))
        })
        .err()
        .unwrap();
        assert_eq!((tried, e.message()), (1, "the layout is wrong".to_string()));
    }

    #[test]
    fn the_seed_is_written_to_the_layout_file_and_the_rest_is_kept() {
        let dir = netsmith_testkit::TempDir::new("write-seed");
        let file = dir.path().join("main.pcb.toml");
        // No file: one is made with the seed alone.
        write_seed(&file, "Board", 4).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "version = 1\n\n[Board]\nseed = 4\n"
        );
        // A file with entries keeps them.
        std::fs::write(
            &file,
            "version = 1\n[Board]\nseed = 1\n[Board.parts.r1]\nat = [1.0, 2.0]\nlock = true\n[Other]\nseed = 9\n",
        )
        .unwrap();
        write_seed(&file, "Board", 6).unwrap();
        let layout = Layout::parse(&std::fs::read_to_string(&file).unwrap(), "f").unwrap();
        assert_eq!(layout.boards["Board"].seed, Some(6));
        assert!(layout.boards["Board"].parts["r1"].lock);
        assert_eq!(layout.boards["Other"].seed, Some(9));
    }
}
