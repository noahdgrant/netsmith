//! The reproductions of the ported boards (issue #599): the RP2350A Minimal, ORBIT and Easyduino
//! ESP32-S3 ports, each drawn by `netsmith fab` from the routes file and the layout file that
//! `netsmith routes import` writes from the original's KiCad board, must have every track, arc,
//! via and zone of the original, on the same layer and net (a zone with the same outline, holes
//! and settings), and nothing else; it must leave no connection but those named below with why,
//! and the router must add no copper (every net is locked, a net the original joins by a zone
//! alone too).
//!
//! Each port keeps two boards to compare as the placer and the router improve: its own build,
//! the automatic board (the example as it is, with no routes file, placed and routed by
//! netsmith), and the reproduction, from the files of its `reference/agent/` (the imported
//! routes file and layout file, which `scripts/reproductions --update` writes, and any other
//! file the reproduction needs, such as a physical file, written by hand). A test copies the
//! example to a temporary folder, imports the original's board there and requires the committed
//! files to be what the import writes now, then puts the files of `reference/agent/` into `src/`,
//! runs `fab` with no KiCad and compares the copper and the zones of the board it writes with the
//! original's (`netsmith_tools::copper`). `measured_copper_of_the_three_boards` prints the copper of the
//! automatic board, the reproduction and the original side by side; it routes the automatic
//! boards, so it is `#[ignore]`d.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use netsmith_testkit::TempDir;
use netsmith_geom::DrcRule;
use netsmith_tools::copper::{self, Diff, Kind, Shape, ZoneDiff};
use netsmith_tools::footprint_diff;
use netsmith_tools::parity::DesignatorMap;
use netsmith_tools::port::PortParts;
use netsmith_tools::reference_table::natural_key;

/// How far apart two ends of the same copper may be, in mm: the original's numbers and the
/// routes file's are the same to 1 nm, and the board file writes them to 0.1 µm.
const TOLERANCE: f64 = 1e-4;

/// A port and what its reproduction is known to lack, each with why (its README says the same).
struct Port {
    /// The folder under `examples/`.
    dir: &'static str,
    /// The original's board file in its `reference/` folder.
    original: &'static str,
    /// The board's name, the folder `fab` writes it into.
    board: &'static str,
    /// The nets on which `fab` leaves connections, each with why: the copper of the original
    /// that the reproduction does not have. No other net may have one, and each must.
    left: &'static [(&'static str, &'static str)],
    /// The nets on which the reproduction has copper that the original does not, each with why.
    extra: &'static [(&'static str, &'static str)],
    /// The parts the import places otherwise than the original's footprint (turned), so that
    /// their pads stand where the original has them, by path: the original numbers a part's two
    /// pads the other way round, or the port's footprint is drawn in another frame.
    refitted: &'static [&'static str],
    /// The original's parts whose footprint on the reproduction differs from the original's in
    /// anything [`footprint_diff`] compares (place, turn, face, pads and their nets, courtyard),
    /// by the original's designator, each with why. Every other part is the original's.
    footprints: &'static [(&'static str, &'static str)],
    /// The errors of netsmith's design rules check of the reproduction that are not copper left
    /// unjoined (a connection or a dangling end, which `left` and the zones account for): the
    /// rule, the original's designators of the parts it names, and why. A courtyard overlap must
    /// be one the original's own courtyards have ([`footprint_diff::courtyard_overlaps`]).
    findings: &'static [(&'static str, [&'static str; 2], &'static str)],
    /// The courtyard overlaps `fab`'s placement check warns of (two parts that both stand where
    /// the import put them may share courtyard area), by the parts' paths, each with why.
    overlaps: &'static [([&'static str; 2], &'static str)],
}

const RP2350: Port = Port {
    dir: "rp2350_minimal",
    original: "RPI-RP2350A-MINIMAL_R4-S1.kicad_pcb",
    board: "Rp2350Minimal",
    left: &[],
    extra: &[],
    // Every part is drawn with Raspberry Pi's own footprint (`libs/rp2350_original.pretty`), so
    // each stands as theirs does.
    refitted: &[],
    footprints: &[(
        "U2",
        "Raspberry Pi numbers the SOT-223's tab 4; the standard LinearRegulator's pin table (and \
         KiCad's SOT-223-3_TabPin2) has it as a second pad 2, the output, so the project's copy \
         numbers it 2: the same copper on the same net",
    )],
    findings: &[],
    overlaps: &[],
};

const ORBIT: Port = Port {
    dir: "orbit",
    original: "ORBIT.kicad_pcb",
    board: "Orbit",
    left: &[],
    extra: &[],
    refitted: &["i2c_pullups.scl_pullup", "i2c_pullups.sda_pullup"],
    footprints: &[
        ("R9", CROSSED),
        ("R10", CROSSED),
        (
            "U16",
            "its pads 42 and 43 (SCL and SDA) share a net with pad 1 of R9 and R10 in ORBIT and \
             with pad 2 in netsmith's design: R9 and R10 above",
        ),
        (
            "U17",
            "its pads 5 and 6 (SDA and SCL) share a net with pad 1 of R10 and R9 in ORBIT and \
             with pad 2 in netsmith's design: R9 and R10 above",
        ),
    ],
    findings: &[
        (
            "courtyards_overlap",
            ["U16", "C5"],
            "ORBIT's own: the courtyard of its LQFP-48 covers 1.65 mm2 of C5's",
        ),
        (
            "courtyards_overlap",
            ["U16", "C6"],
            "ORBIT's own: the courtyard of its LQFP-48 covers 1.03 mm2 of C6's",
        ),
    ],
    overlaps: &[
        (["mcu", "v3v3_100n[2]"], "ORBIT's own (U16 and C5)"),
        (["mcu", "v3v3_100n[3]"], "ORBIT's own (U16 and C6)"),
        (
            ["usb_uart.ic", "usb_uart.vdd_bulk"],
            "ORBIT's QFN-28 (U22) draws no courtyard, and netsmith takes its pads and 0.25 mm \
             for one, which reaches 0.03 mm2 into C11's; KiCad's check passes a footprint with \
             no courtyard over",
        ),
    ],
};

/// Why a two-pad part stands turned by half a turn from the original's: its pads are numbered
/// the other way round, which is the design's net on each pad and not its footprint.
const CROSSED: &str = "the original joins its pad 1 to the line and pad 2 to the supply, and \
     the standard pull-up module the design uses joins pad 1 to the supply (as RP2350's own \
     pull-up R1 is): the import turns it by half a turn so that each pad stands where the \
     original has the pad of its net, so its rotation differs by 180 degrees and its pads' nets \
     are crossed";

const EASYDUINO: Port = Port {
    dir: "easyduino_esp32s3",
    original: "Easyduino_ESP32S3.kicad_pcb",
    board: "EasyduinoEsp32s3",
    left: &[],
    extra: &[],
    refitted: &["bridge_reset_pullup.resistor"],
    footprints: &[
        ("R11", CROSSED),
        (
            "U3",
            "its pad 9 (RSTb) shares a net with pad 1 of R11 in Easyduino and with pad 2 in \
             netsmith's design: R11 above",
        ),
        (
            "U2",
            "Easyduino's footprint of the headers draws no courtyard (and draws the board's \
             outline); netsmith would take its pads and 0.25 mm for one, the whole board on both \
             faces, so the project's footprint has Easyduino's pads and its courtyard on B.CrtYd \
             only",
        ),
    ],
    findings: &[],
    overlaps: &[],
};

fn example(port: &Port) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(port.dir)
}

/// `netsmith <args>` in `dir`, with KiCad switched off: the reproduction needs none (ADR 0097).
fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap()
}

fn text(run: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    )
}

/// Copy the files under `from` into `to`, but not a `fab` or `out` folder of an earlier run.
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let name = entry.file_name();
        if path.is_dir() {
            if name != "fab" && name != "out" {
                copy_tree(&path, &to.join(&name));
            }
        } else {
            fs::copy(&path, to.join(&name)).unwrap();
        }
    }
}

/// The copper of the KiCad board at `path`.
fn shapes(path: &Path) -> Vec<Shape> {
    let pcb = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let (shapes, unread) = copper::read(&pcb, &path.display().to_string()).unwrap();
    assert!(unread.is_empty(), "{}: {unread:?}", path.display());
    shapes
}

/// What a reproduction made: the copper diff, the zone diff, the nets `fab` left connections
/// on, and the folder with its board.
struct Made {
    diff: Diff,
    zones: ZoneDiff,
    left: BTreeSet<String>,
    folder: TempDir,
    /// What `fab` printed.
    said: String,
}

impl Made {
    fn pcb(&self, port: &Port) -> PathBuf {
        self.folder
            .join("fab")
            .join(port.board)
            .join(format!("{}.kicad_pcb", port.board))
    }
}

/// A copy of the example in which a source file of `reference/agent/` that is also in `src/` is
/// the example's with one run of lines added, and no other line changed: the reproduction
/// changes what it needs and follows the example in the rest.
fn assert_adds_one_block(port: &Port, name: &str) {
    let agent = fs::read_to_string(example(port).join("reference/agent").join(name)).unwrap();
    let own = fs::read_to_string(example(port).join("src").join(name)).unwrap();
    let (a, o): (Vec<&str>, Vec<&str>) = (agent.lines().collect(), own.lines().collect());
    let head = a.iter().zip(&o).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(o[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    assert_eq!(
        head + tail,
        o.len(),
        "examples/{}/reference/agent/{name} is not src/{name} with one run of lines added: its \
         line {} differs from the example's; make the change in both, or only the addition in \
         the reproduction",
        port.dir,
        head + 1
    );
}

/// The reproduction of `port`: the committed files of `reference/agent/` are what the import
/// writes now, and `fab` draws the board from them.
fn reproduce(port: &Port, tag: &str) -> Made {
    let folder = TempDir::new(tag);
    copy_tree(&example(port), &folder);
    let _ = fs::remove_file(folder.join("src/main.routes.toml"));
    let run = netsmith(
        &folder,
        &["routes", "import", &format!("reference/{}", port.original)],
    );
    assert!(run.status.success(), "routes import:\n{}", text(&run));
    let mut refitted: Vec<String> = text(&run)
        .lines()
        .filter_map(|l| {
            let (path, rest) = l.strip_prefix("part ")?.split_once(' ')?;
            rest.starts_with("is placed").then(|| path.to_string())
        })
        .collect();
    refitted.sort();
    assert_eq!(
        refitted,
        port.refitted,
        "the parts the import places otherwise than the original's footprint:\n{}",
        text(&run)
    );
    let agent = folder.join("reference/agent");
    for name in ["main.routes.toml", "main.pcb.toml"] {
        let fresh = fs::read_to_string(folder.join("src").join(name)).unwrap();
        let kept = fs::read_to_string(agent.join(name)).unwrap_or_default();
        assert!(
            fresh == kept,
            "examples/{}/reference/agent/{name} is not what `netsmith routes import` writes now \
             (a footprint, the placer or the importer changed): run `scripts/reproductions \
             --update` and review the diff\n{}",
            port.dir,
            text(&run)
        );
    }
    for entry in fs::read_dir(&agent).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if name.ends_with(".nsm") {
            assert_adds_one_block(port, &name);
        }
        fs::copy(entry.path(), folder.join("src").join(&name)).unwrap();
    }
    // `--locked`: the pick lock and the layout file's seed are the example's, and nothing is
    // written into them. The original's copper breaks some of netsmith's design rules (its own
    // were KiCad's), so the board is packaged with them; each net it leaves is named in `left`.
    let run = netsmith(
        &folder,
        &[
            "fab",
            "--locked",
            "--allow-drc-violations",
            "--no-glb",
            "--no-step",
            "--out",
            "fab",
        ],
    );
    let said = text(&run);
    assert!(
        run.status.success() || said.contains("INCOMPLETE:"),
        "fab failed, and not for connections left:\n{said}"
    );
    // A record whose pads moved, or of a net the board lacks, is a warning naming the file.
    assert!(
        !said
            .lines()
            .any(|l| l.contains("warning") && l.contains("routes.toml")),
        "fab did not draw the routes file as written:\n{said}"
    );
    let out = folder.join("fab").join(port.board);
    let left: BTreeSet<String> = fs::read_to_string(out.join("routing-failure.txt"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            l.strip_prefix("net ")?
                .split_once(':')
                .map(|(n, _)| n.to_string())
        })
        .collect();
    let original = example(port).join("reference").join(port.original);
    let ours = out.join(format!("{}.kicad_pcb", port.board));
    Made {
        diff: copper::compare(&shapes(&original), &shapes(&ours), TOLERANCE),
        zones: copper::compare_zones(&zones(&original), &zones(&ours), TOLERANCE),
        left,
        folder,
        said,
    }
}

/// The zones of the KiCad board at `path`.
fn zones(path: &Path) -> Vec<copper::Zone> {
    let pcb = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let (zones, unread) = copper::read_zones(&pcb, &path.display().to_string()).unwrap();
    assert!(unread.is_empty(), "{}: {unread:?}", path.display());
    zones
}

/// Every shape and zone of the original is drawn, on one net each way, a zone with the
/// original's settings; the extra copper is on the nets `port.extra` names and the connections
/// left on those `port.left` names, each of them.
fn assert_reproduces(port: &Port, made: &Made) {
    let diff = &made.diff;
    assert!(
        diff.missing.is_empty() && diff.crossed.is_empty(),
        "the reproduction of {} lacks copper of the original:\n{diff}",
        port.dir
    );
    let zones = &made.zones;
    assert!(
        zones.is_same(),
        "the zones of the reproduction of {} are not the original's:\n{zones}",
        port.dir
    );
    // A net that has both tracks and zones is the same net of the design for both.
    for (theirs, ours) in &zones.nets {
        if let Some(by_tracks) = diff.nets.get(theirs) {
            assert_eq!(
                by_tracks, ours,
                "the original's net `{theirs}` has its tracks on `{by_tracks}` and its zones on `{ours}`"
            );
        }
    }
    let named: BTreeSet<&str> = port.extra.iter().map(|(n, _)| *n).collect();
    let extra: BTreeSet<&str> = diff.extra.iter().map(|s| s.net.as_str()).collect();
    assert_eq!(
        extra, named,
        "the nets with copper the original does not have:\n{diff}"
    );
    let named: BTreeSet<String> = port.left.iter().map(|(n, _)| n.to_string()).collect();
    assert_eq!(
        made.left, named,
        "the nets with connections left (the reasons the test knows: {:?})",
        port.left
    );
}

/// Every part of the reproduction is drawn, placed and turned as the original's, but those
/// `port.footprints` names; netsmith's design rules check finds nothing but copper left unjoined
/// and `port.findings`, each a courtyard overlap of the original's own; and `fab` warns of the
/// courtyard overlaps `port.overlaps` names and no other.
fn assert_footprints(port: &Port, made: &Made) {
    let original = fs::read_to_string(example(port).join("reference").join(port.original)).unwrap();
    let pcb = fs::read_to_string(made.pcb(port)).unwrap();
    let parts = PortParts::read(&example(port).join("reference/parts.toml")).unwrap();
    let diff = footprint_diff::compare(&original, &pcb, &parts).unwrap();
    let named: Vec<&str> = port.footprints.iter().map(|(r, _)| *r).collect();
    let mut differ = diff.references();
    differ.sort_by_key(|r| natural_key(r));
    let mut named_sorted = named.clone();
    named_sorted.sort_by_key(|r| natural_key(r));
    assert_eq!(
        differ, named_sorted,
        "the parts of the reproduction of {} drawn otherwise than the original's:\n{diff}",
        port.dir
    );

    // The design rules check: each error that is not copper left unjoined, by rule and the
    // original's designators of its parts.
    let ours_to_theirs: BTreeMap<String, String> = {
        let map = DesignatorMap::from_parts(&parts, &pcb).unwrap();
        parts
            .iter()
            .filter_map(|(_, theirs)| Some((map.get(theirs)?.to_string(), theirs.to_string())))
            .collect()
    };
    let project = fs::read_to_string(made.pcb(port).with_extension("kicad_pro")).ok();
    let report =
        netsmith_geom::drc_check(&pcb, project.as_deref(), &netsmith_geom::DrcExtras::default())
            .unwrap();
    let mut found: Vec<(String, Vec<String>)> = report
        .errors()
        .filter(|f| {
            !matches!(
                f.rule,
                DrcRule::UnconnectedItems | DrcRule::TrackDangling | DrcRule::ViaDangling
            )
        })
        .map(|f| {
            let mut theirs: Vec<String> = f
                .message
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter_map(|w| ours_to_theirs.get(w).cloned())
                .collect();
            theirs.sort();
            (f.rule.id().to_string(), theirs)
        })
        .collect();
    found.sort();
    let mut expected: Vec<(String, Vec<String>)> = port
        .findings
        .iter()
        .map(|(rule, pair, _)| {
            let mut pair: Vec<String> = pair.iter().map(|r| r.to_string()).collect();
            pair.sort();
            (rule.to_string(), pair)
        })
        .collect();
    expected.sort();
    assert_eq!(
        found, expected,
        "the errors of netsmith's design rules check of the reproduction of {} that are not copper \
         left unjoined, by rule and the original's designators",
        port.dir
    );
    // A courtyard overlap it finds is one the original's own courtyards have.
    let own = footprint_diff::courtyard_overlaps(
        &netsmith_tools::reference_table::read_board(&original, "the original").unwrap(),
    );
    for (rule, pair, why) in port.findings {
        if *rule == "courtyards_overlap" {
            assert!(
                own.iter().any(|(a, b, _)| {
                    (a == pair[0] && b == pair[1]) || (a == pair[1] && b == pair[0])
                }),
                "{pair:?} is said to be the original's own overlap ({why}), but the original's \
                 courtyards that overlap are {own:?}"
            );
        }
    }

    // The placement check's warnings of courtyards that overlap, by the parts' paths.
    let mut warned: Vec<[String; 2]> = made
        .said
        .lines()
        .filter_map(|l| {
            let rest = l.split_once("the courtyards of `")?.1;
            let (a, rest) = rest.split_once('`')?;
            let b = rest.split_once(" and `")?.1.split_once('`')?.0;
            let mut pair = [a.to_string(), b.to_string()];
            pair.sort();
            Some(pair)
        })
        .collect();
    warned.sort();
    let mut expected: Vec<[String; 2]> = port
        .overlaps
        .iter()
        .map(|(pair, _)| {
            let mut pair = pair.map(str::to_string);
            pair.sort();
            pair
        })
        .collect();
    expected.sort();
    assert_eq!(
        warned, expected,
        "the courtyard overlaps `fab` warns of on the reproduction of {}:\n{}",
        port.dir, made.said
    );
}

#[test]
fn the_rp2350_reproduction_has_the_originals_copper() {
    let made = reproduce(&RP2350, "reproduce-rp2350");
    assert_reproduces(&RP2350, &made);
    assert_footprints(&RP2350, &made);
    assert_eq!(
        (
            made.diff.matched.get(&Kind::Segment),
            made.diff.matched.get(&Kind::Via)
        ),
        (Some(&352), Some(&40))
    );
    assert_eq!(made.zones.matched, 10);
}

#[test]
fn the_orbit_reproduction_has_the_originals_copper() {
    let made = reproduce(&ORBIT, "reproduce-orbit");
    assert_reproduces(&ORBIT, &made);
    assert_footprints(&ORBIT, &made);
    assert_eq!(
        (
            made.diff.matched.get(&Kind::Segment),
            made.diff.matched.get(&Kind::Via)
        ),
        (Some(&299), Some(&49))
    );
    assert_eq!(made.zones.matched, 1);
}

#[test]
fn the_easyduino_reproduction_has_the_originals_copper() {
    let made = reproduce(&EASYDUINO, "reproduce-easyduino");
    assert_reproduces(&EASYDUINO, &made);
    assert_footprints(&EASYDUINO, &made);
    assert_eq!(
        (
            made.diff.matched.get(&Kind::Segment),
            made.diff.matched.get(&Kind::Via)
        ),
        (Some(&703), Some(&137))
    );
    assert_eq!(made.zones.matched, 5);
}

/// Segments, arcs, vias and the length of the tracks of a board's copper.
fn summary(shapes: &[Shape]) -> String {
    let n = |k: Kind| shapes.iter().filter(|s| s.kind == k).count();
    let length: f64 = shapes.iter().map(Shape::length).sum();
    format!(
        "{:>5} segments {:>4} arcs {:>4} vias {:>9.1} mm",
        n(Kind::Segment),
        n(Kind::Arc),
        n(Kind::Via),
        length
    )
}

/// The errors of netsmith's design rules check of the board file at `pcb` (with its `.kicad_pro`
/// beside it), counted by rule (`clearance 12, unconnected_items 34`), then each that is not of
/// copper left unjoined (a connection, a dangling end) on a line of its own.
fn drc_errors(pcb: &Path) -> String {
    let board = fs::read_to_string(pcb).unwrap();
    let project = fs::read_to_string(pcb.with_extension("kicad_pro")).ok();
    let report = netsmith_geom::drc_check(
        &board,
        project.as_deref(),
        &netsmith_geom::DrcExtras::default(),
    )
    .unwrap_or_else(|e| panic!("{}: {e}", pcb.display()));
    let mut by_rule: std::collections::BTreeMap<&str, usize> = Default::default();
    let mut lines = String::new();
    for f in report.errors() {
        *by_rule.entry(f.rule.id()).or_default() += 1;
        let unjoined = matches!(
            f.rule,
            DrcRule::UnconnectedItems | DrcRule::TrackDangling | DrcRule::ViaDangling
        );
        if !unjoined {
            lines.push_str(&format!("\n                  {}: {}", f.rule.id(), f.message));
        }
    }
    by_rule
        .iter()
        .map(|(rule, n)| format!("{rule} {n}"))
        .collect::<Vec<_>>()
        .join(", ")
        + &lines
}


/// The copper of each port's automatic board (the example as it is, placed and routed by
/// netsmith), its reproduction and its original, printed, to follow as the placer and the router
/// change; nothing is asserted of the automatic board. It routes the three examples, about two
/// minutes in a debug build, so it is `#[ignore]`d: run it with `cargo test -p netsmith-cli
/// --test main reproduce::measured -- --ignored --nocapture`.
#[test]
#[ignore]
fn measured_copper_of_the_three_boards() {
    for port in [&RP2350, &ORBIT, &EASYDUINO] {
        let made = reproduce(port, &format!("measured-{}", port.dir));
        let auto = TempDir::new(&format!("measured-auto-{}", port.dir));
        copy_tree(&example(port), &auto);
        let run = netsmith(
            &auto,
            &["fab", "--locked", "--no-glb", "--no-step", "--out", "fab"],
        );
        let automatic = auto
            .join("fab")
            .join(port.board)
            .join(format!("{}.kicad_pcb", port.board));
        let notes = |dir: &Path| {
            fs::read_to_string(dir.join("fab").join(port.board).join("fab-notes.txt"))
                .unwrap_or_default()
                .lines()
                .find(|l| l.starts_with("design rules check:"))
                .unwrap_or("design rules check: (no notes)")
                .to_string()
        };
        println!("{}:", port.dir);
        println!(
            "  automatic     {}  ({})",
            summary(&shapes(&automatic)),
            if run.status.success() {
                "complete".to_string()
            } else {
                "INCOMPLETE".to_string()
            }
        );
        println!("                {}", notes(&auto));
        println!(
            "  reproduction  {}  ({} net(s) with connections left)",
            summary(&shapes(&made.pcb(port))),
            made.left.len()
        );
        println!("                {}", notes(&made.folder));
        println!(
            "                errors of netsmith's design rules check: {}",
            drc_errors(&made.pcb(port))
        );
        println!(
            "  original      {}",
            summary(&shapes(
                &example(port).join("reference").join(port.original)
            ))
        );
        print!("  {}", made.diff);
        print!("  {}", made.zones);
    }
}
