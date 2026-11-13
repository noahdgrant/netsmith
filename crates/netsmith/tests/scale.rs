//! A design the size of the showcase boards (#561): about 1,200 parts and 5,000 pads in modules
//! and repeated circuits, generated as `.nsm` source. `measured` times every stage on it from the
//! source to the parity report. The regular tests time nothing (the speed of the machine is not
//! the code's): one runs every stage on a smaller size and checks what each makes, and one
//! compares the solver's trials, which it counts, between two sizes, so that a search that grows with the square of the board shows on every machine.
//!
//! The placer and the router are not measured (they are the router epic's, #431): the export is
//! given a board whose parts are put in rows, left to right, by their courtyards, with no placer.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::time::{Duration, Instant};

use netsmith::compare::netlist_of_design;
use netsmith::editor::Workspace;
use netsmith::parity::{DesignatorMap, Tolerances, check_text};
use netsmith::project::Target;
use netsmith::reference_table::{Board as Table, placements_toml};
use netsmith::{Built, Libraries, build_with, outcome_with};
use netsmith_check::Loader;
use netsmith_editor::Backend;
use netsmith_kicad::{Search};
use netsmith_netlist::{Identity, Netlist, Options, Outcome, compare};
use netsmith_pcb::geometry::{Pt, Rect};
use netsmith_pcb::{Board, PlaceOptions, kicad_pcb};
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

use crate::common::port::{Port, Reading};

/// The channels of the full size: 16 parts and 76 pads each, about 1,200 parts and 5,800 pads
/// in all.
const FULL: usize = 76;

/// How many channels a bank holds.
const PER_BANK: usize = 4;

/// The pins of a channel's headers that are its bus, which runs through every channel.
const BUS: std::ops::RangeInclusive<usize> = 9..=18;

/// The source of a board of `channels` channels (a multiple of [`PER_BANK`]). A channel is a
/// module of two 20-pin headers, a 10-pin socket, four indicators (a module of a resistor and an
/// LED), and decoupling; banks of channels are a module too. The bus pins of every header are
/// on one net each across the whole board, as a backplane's are; the rest are the channel's own.
pub fn source(channels: usize) -> String {
    assert!(channels > 0 && channels.is_multiple_of(PER_BANK));
    let mut s = String::from(
        "import std::parts::connector::{Header1x2, Header2x10, Socket1x10}\n\
         import std::parts::passive::{Resistor, Led, Capacitor}\n\
         import std::sources::Rail\n\
         import std::logic::Logic\n\
         import std::mfg::jlcpcb::TwoLayer\n\n",
    );
    s.push_str(
        "/// The lines of the bus that runs through every channel.\ninterface Backplane {\n",
    );
    for p in BUS {
        let _ = writeln!(s, "    .l{p}: Pin");
    }
    s.push_str("}\n\n");
    s.push_str(
        "module Indicator {\n\
         \x20   .power: Rail\n\
         \x20   .enable: Logic\n\
         \x20   .enable.reference ~ .power\n\
         \x20   .limit: Resistor {\n        .resistance &= 1kohm +/- 1%\n    }\n\
         \x20   .led: Led {\n        .color &= \"GREEN\"\n        .forward_voltage &= 1.8V..2.5V\n    }\n\
         \x20   .enable.line ~> .limit ~> .led ~> .power.neg\n\
         }\n\n",
    );
    s.push_str("module Channel {\n    .power: Rail\n    .bus_a: Backplane\n    .bus_b: Backplane\n    .bus_in: Header2x10\n    .bus_out: Header2x10\n    .probe: Socket1x10\n");
    for i in 1..=4 {
        let _ = writeln!(
            s,
            "    .ind{i}: Indicator\n    .ind{i}.power ~ .power\n    .ind{i}.enable.line ~ .bus_in.p{i}\n    .ind{i}.enable.line ~ .probe.p{i}"
        );
    }
    for i in 5..=8 {
        // The channel's own lines with no indicator, to the probe socket.
        let _ = writeln!(s, "    .bus_in.p{i} ~ .probe.p{i}");
    }
    for p in BUS {
        let _ = writeln!(
            s,
            "    .bus_in.p{p} ~ .bus_out.p{p}\n    .bus_a.l{p} ~ .bus_in.p{p}\n    .bus_b.l{p} ~ .bus_out.p{p}"
        );
    }
    for i in 1..=8 {
        let _ = writeln!(s, "    .bus_out.p{i} ~ .bus_in.p{i}");
    }
    s.push_str(
        "    .power.pos ~ .bus_in.p19\n    .power.neg ~ .bus_in.p20\n\
         \x20   .power.pos ~ .bus_out.p19\n    .power.neg ~ .bus_out.p20\n\
         \x20   .power.pos ~ .probe.p9\n    .power.neg ~ .probe.p10\n",
    );
    for i in 1..=5 {
        let value = if i == 5 { "10uF" } else { "100nF" };
        let _ = writeln!(
            s,
            "    .bypass{i}: Capacitor {{\n        .capacitance &= {value} +/- 20%\n    }}\n    .power.pos ~> .bypass{i} ~> .power.neg"
        );
    }
    s.push_str(
        "}\n\nmodule Bank {\n    .power: Rail\n    .bus_a: Backplane\n    .bus_b: Backplane\n",
    );
    for c in 0..PER_BANK {
        let _ = writeln!(s, "    .ch{c}: Channel\n    .ch{c}.power ~ .power");
    }
    for c in 1..PER_BANK {
        let _ = writeln!(s, "    .ch{}.bus_b ~ .ch{c}.bus_a", c - 1);
    }
    let _ = writeln!(
        s,
        "    .bus_a ~ .ch0.bus_a\n    .bus_b ~ .ch{}.bus_b",
        PER_BANK - 1
    );
    s.push_str("}\n\n");
    s.push_str(
        "board Scale {\n\
         \x20   .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n\
         \x20   .supply: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"VCC\"\n        .neg.net &= \"GND\"\n    }\n\
         \x20   .power_in: Header1x2\n\
         \x20   .supply.pos ~ .power_in.p1\n    .supply.neg ~ .power_in.p2\n",
    );
    let banks = channels / PER_BANK;
    for b in 0..banks {
        let _ = writeln!(s, "    .bank{b}: Bank\n    .bank{b}.power ~ .supply");
    }
    for b in 1..banks {
        let _ = writeln!(s, "    .bank{}.bus_b ~ .bank{b}.bus_a", b - 1);
    }
    s.push_str("}\n");
    s
}

/// The libraries of a machine with no KiCad: the generated footprints and std's own symbols, the
/// same on every machine (ADR 0097).
fn libraries(empty: &TempDir) -> Libraries {
    Libraries::for_project_by(None, &Search::under(empty.to_path_buf())).unwrap()
}

/// The board of `built` with its parts in rows by their courtyards, and no placer.
fn board_in_rows(built: &Built, libraries: &Libraries) -> Board {
    let mut u = Board::unplaced(
        &built.design,
        &libraries.mapping,
        &libraries.footprints,
        &PlaceOptions::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let gap = 1.0;
    let area: f64 = u
        .parts
        .iter()
        .map(|p| {
            let c = p.footprint.courtyard;
            (c.max.x - c.min.x + gap) * (c.max.y - c.min.y + gap)
        })
        .sum();
    let width = area.sqrt() * 1.2;
    let (mut x, mut y, mut row) = (gap, gap, 0.0_f64);
    let mut right = 0.0_f64;
    for p in &mut u.parts {
        let c = p.footprint.courtyard;
        let (w, h) = (c.max.x - c.min.x, c.max.y - c.min.y);
        if x + w > width {
            x = gap;
            y += row + gap;
            row = 0.0;
        }
        p.at = Pt::new(x - c.min.x, y - c.min.y);
        p.rotation = 0.0;
        x += w + gap;
        right = right.max(x);
        row = row.max(h);
    }
    let outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(right, y + row + gap));
    u.into_board(&built.design, outline, 0.0)
}

/// How long each stage took, in order, and what it made.
#[derive(Default)]
pub struct Times {
    pub stages: Vec<(&'static str, Duration, String)>,
}

impl Times {
    fn time<R>(
        &mut self,
        stage: &'static str,
        work: impl FnOnce() -> R,
        said: impl Fn(&R) -> String,
    ) -> R {
        let started = Instant::now();
        let r = work();
        let took = started.elapsed();
        self.stages.push((stage, took, said(&r)));
        r
    }

    fn text(&self) -> String {
        self.stages
            .iter()
            .map(|(stage, took, said)| {
                format!("{stage:<28} {:>9.3} s  {said}\n", took.as_secs_f64())
            })
            .collect()
    }
}

fn verdict(outcome: &Outcome) -> &'static str {
    match outcome {
        Outcome::Match(_) => "match",
        Outcome::Different(_) => "different",
        Outcome::Undecided(_) => "undecided",
    }
}

/// Every stage on a board of `channels` channels; `editor` adds the editor's analysis of the
/// design as a project on disk.
pub fn run(channels: usize, editor: bool) -> Times {
    let mut t = Times::default();
    let text = source(channels);
    let empty = TempDir::new("scale-no-kicad");
    let libraries = libraries(&empty);
    let catalogs = Catalogs::jlcpcb();
    let loader = Loader::std();

    t.time(
        "check, solve, design",
        || {
            outcome_with("scale.nsm", &text, &loader, &catalogs, None)
                .unwrap_or_else(|p| panic!("{}", netsmith::render_problems(&p)))
        },
        |o| {
            let d = &o.boards[0].design;
            let pads: usize = d.nets.iter().map(|n| n.pins.len()).sum();
            format!(
                "{} parts, {} nets, {pads} pins on nets",
                d.parts.len(),
                d.nets.len()
            )
        },
    );
    let built = t.time(
        "build (with schematic)",
        || {
            build_with("scale.nsm", &text, &loader, &catalogs, &libraries)
                .unwrap_or_else(|e| panic!("{e}"))
        },
        |b| format!("schematic {} kB", b[0].schematic.len() / 1000),
    );
    let built = &built[0];

    let board = t.time(
        "board in rows (no placer)",
        || board_in_rows(built, &libraries),
        |b| {
            let pads: usize = b.parts.iter().map(|p| p.footprint.pads.len()).sum();
            format!("{} footprints, {pads} pads", b.parts.len())
        },
    );
    let pcb = t.time(
        "kicad_pcb export",
        || kicad_pcb(&board),
        |s| format!("{} kB", s.len() / 1000),
    );

    let design = netlist_of_design(&built.design, &libraries.mapping).unwrap();
    let read = t.time(
        "read the kicad_pcb netlist",
        || Netlist::from_kicad_pcb(&pcb).unwrap(),
        |n| format!("{} components, {} nets", n.components.len(), n.nets.len()),
    );
    let by_refdes = Options {
        identity: Identity::ByRefdes,
        ..Options::default()
    };
    let o = t.time(
        "compare by designator",
        || compare(&design, &read, &by_refdes).unwrap(),
        |o| verdict(o).into(),
    );
    assert!(
        matches!(o, Outcome::Match(_)),
        "{}",
        netsmith_netlist::report(&o)
    );
    let o = t.time(
        "compare free (no designators)",
        || compare(&design, &read, &Options::default()).unwrap(),
        |o| verdict(o).into(),
    );
    assert!(
        matches!(o, Outcome::Match(_)),
        "{}",
        netsmith_netlist::report(&o)
    );

    let port = Port {
        folder: "",
        board: None,
        original: "the board",
        swapped: &[],
        renumbered: &[],
        no_nets: &[],
        own: &[],
    };
    let differences = t.time(
        "nets proof (pads' neighbours)",
        || {
            let none = BTreeMap::new();
            let ours = port.neighbours(&design, &none, Reading::AsIs);
            let theirs = port.neighbours(&read, &none, Reading::AsIs);
            port.net_differences(&ours, &theirs)
        },
        |d| format!("{} differences", d.len()),
    );
    assert!(differences.is_empty(), "{differences:?}");

    let table = t.time(
        "reference table of the board",
        || placements_toml(&pcb, "self").unwrap(),
        |s| format!("{} kB", s.len() / 1000),
    );
    let map = DesignatorMap::identity(&Table::from_table(&table).unwrap());
    let report = t.time(
        "parity against itself",
        || check_text(&table, &pcb, &map, &Tolerances::default()).unwrap(),
        |r| {
            let (ok, failed) = r.must_match_counts();
            format!("{ok} items match, {failed} fail")
        },
    );
    assert!(report.passed(), "{}", report.text());
    t.time(
        "parity report text",
        || report.text(),
        |s| format!("{} kB", s.len() / 1000),
    );
    t.time(
        "parity overlay",
        || report.overlay_svg(),
        |s| format!("{} kB", s.len() / 1000),
    );

    if editor {
        let root = TempDir::new("scale-editor");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("netsmith.toml"),
            "[project]\nname = \"scale\"\ntop_level = [\"src/main.nsm\"]\n",
        )
        .unwrap();
        fs::write(root.join("src/main.nsm"), &text).unwrap();
        let workspace = t.time(
            "editor: open and analyse",
            || {
                let w = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
                let a = w.analysis();
                (w, a)
            },
            |(_, a)| format!("{} boards, {} errors", a.boards.len(), a.errors),
        );
        let (workspace, analysis) = workspace;
        assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
        let name = analysis.boards[0].name.clone();
        t.time(
            "editor: schematic view",
            || workspace.schematic(&name).unwrap(),
            |s| format!("{} kB", s.len() / 1000),
        );
    }
    t
}

/// Prints the time of every stage at the full size (76 channels), or at the sizes of
/// `NETSMITH_SCALE=20,36,76` (channels, multiples of 4); `NETSMITH_SCALE_EDITOR=0` leaves the
/// editor out. Run with `--release`, and under `/usr/bin/time -v` for the peak memory.
#[test]
#[ignore = "a measurement, not a check"]
fn measured() {
    let sizes: Vec<usize> = std::env::var("NETSMITH_SCALE")
        .ok()
        .map(|s| s.split(',').map(|n| n.trim().parse().unwrap()).collect())
        .unwrap_or_else(|| vec![FULL]);
    let editor = std::env::var("NETSMITH_SCALE_EDITOR").map_or(true, |v| v != "0");
    for channels in sizes {
        let t = run(channels, editor);
        eprintln!("{channels} channels:\n{}", t.text());
    }
}

/// Every stage at a quarter of the full size, with what each makes checked as `run` goes (the
/// build, the board, the comparison with the board's netlist and the parity against itself pass).
/// The times are printed and not compared.
#[test]
fn a_quarter_of_a_showcase_board_goes_from_source_to_parity() {
    let t = run(FULL / 4 / PER_BANK * PER_BANK, false);
    eprintln!("{}", t.text());
}

/// The candidates the solver's search tried for a board of `channels` channels.
fn solver_trials(channels: usize) -> usize {
    let c = netsmith_check::check("scale.nsm", &source(channels), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let solved = netsmith_solve::solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(solved.diagnostics.is_empty(), "{:?}", solved.diagnostics);
    eprintln!(
        "{channels} channels: {} trials, {} work",
        solved.trials, solved.work
    );
    solved.trials
}

/// The solver's search grows with the board and not with its square: twice the channels, about
/// twice the trials (the channels share the bus's nets and not the groups of parts that are
/// searched, so each is searched on its own). A count, so that a busy runner cannot fail it. The
/// work is printed and not bounded: what a trial costs grows with the board by design.
#[test]
fn the_solvers_trials_grow_with_the_board_and_not_with_its_square() {
    let (small, big) = (solver_trials(8), solver_trials(16));
    let ratio = big as f64 / small.max(1) as f64;
    eprintln!("ratio of the trials {ratio:.2}");
    assert!(
        ratio < 2.5,
        "twice the board took {ratio:.2} times the trials ({small}, {big})"
    );
}

/// The environment variable that makes [`peak_rss_of_one_build`] build a board of that many
/// channels, in a child copy of this test binary.
const CHILD: &str = "NETSMITH_SCALE_PEAK_RSS_CHANNELS";

/// Run by [`memory_grows_about_linearly_with_the_board`] in a child process: builds a board of
/// [`CHILD`] channels and prints the process's peak resident memory (`VmHWM`, Linux). Ignored, and
/// does nothing without the variable.
#[test]
#[ignore]
fn peak_rss_of_one_build() {
    let Some(channels) = std::env::var(CHILD)
        .ok()
        .and_then(|c| c.parse::<usize>().ok())
    else {
        return;
    };
    let text = source(channels);
    let catalogs = Catalogs::jlcpcb();
    let loader = Loader::std();
    let outcome = outcome_with("scale.nsm", &text, &loader, &catalogs, None)
        .unwrap_or_else(|p| panic!("{}", netsmith::render_problems(&p)));
    let status = fs::read_to_string("/proc/self/status").expect("/proc/self/status");
    let kb = status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())
        .expect("VmHWM");
    drop(outcome);
    println!("peak_rss_kb={kb}");
}

/// The peak resident memory of a child process that builds a board of `channels` channels.
fn peak_rss_kb(channels: usize) -> u64 {
    let exe = std::env::current_exe().expect("the test binary");
    let out = std::process::Command::new(exe)
        .args([
            "--exact",
            "scale::peak_rss_of_one_build",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD, channels.to_string())
        .output()
        .expect("run the child");
    let text = String::from_utf8_lossy(&out.stdout);
    // libtest prints the test's name before its output on the same line.
    text.split("peak_rss_kb=")
        .nth(1)
        .map(|v| {
            v.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "no peak from the child: {text}{}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
}

/// The memory of a build grows about as the board does (#569): the picks kept a copy of every
/// field's domain for each group of parts, so twice the board took four times the memory (0.56 GB
/// at 36 channels, 2.2 GB at 76). Each size is built in its own process and its peak resident
/// memory read, so the binary's other tests pay nothing for the measurement (a counting global
/// allocator made the placer's pre-check twice as slow). The fixed part of a process's memory
/// cancels in the ratio of what 8 more channels cost to what 4 more cost: about 2 when memory
/// grows with the board, about 4 when it grows with its square. Measured 2026-11-13: 1.99 to 2.10
/// now, 3.06 with the picks before #569.
#[test]
fn memory_grows_about_linearly_with_the_board() {
    if !cfg!(target_os = "linux") {
        return;
    }
    let [p4, p8, p16] = [4, 8, 16].map(peak_rss_kb);
    let (first, second) = (p8.saturating_sub(p4).max(1), p16.saturating_sub(p8));
    let ratio = second as f64 / first as f64;
    eprintln!("peak RSS 4/8/16 channels: {p4}/{p8}/{p16} kB, ratio of the increments {ratio:.2}");
    assert!(
        ratio < 2.6,
        "8 more channels cost {ratio:.2} times what 4 more did ({p4}, {p8}, {p16} kB)"
    );
}
