//! The parity checker of the RP2350A Minimal port (`netsmith::parity`): small boards written as
//! KiCad text stand for what `fab` writes, and the checker must pass a copy of the reference's
//! must-match items and fail, naming the item, each way that a board can differ. One test runs
//! the checker on Raspberry Pi's own board against itself, which proves that it reads a real
//! KiCad 10 file. One more builds the example with `fab` and gates on the checker.

use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

use netsmith::parity::{DesignatorMap, Item, ParityReport, Tolerances, Verdict, check_text};
use netsmith::reference_table::{Board, placements_toml, read_board};

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/rp2350_minimal")
}

/// The ways a synthetic board differs from the reference one. `Default` is the reference itself,
/// with its outline's corner at (100, 50).
#[derive(Clone)]
struct Spec {
    /// The board's own origin (where the upper-left corner of the outline is).
    origin: (f64, f64),
    /// Added to the outline's width.
    wider: f64,
    /// Added to the second mounting hole's x.
    hole_shift: f64,
    /// The second mounting hole is written as KiCad's plated `MountingHole_*_Pad` family writes
    /// it (a `thru_hole` pad), not as a non-plated `np_thru_hole` one.
    hole_plated: bool,
    /// The second mounting hole's pad size (its drill is 2.75).
    hole_pad: f64,
    /// The net of the second mounting hole's pad ("" for none).
    hole_net: &'static str,
    /// Added to J2's x.
    j2_shift: f64,
    /// Added to J1's x.
    j1_shift: f64,
    /// SW1's rotation.
    sw1_rotation: f64,
    /// Added to U1's x.
    u1_shift: f64,
    /// The numbers of J2's first two pads are swapped.
    swap_j2_numbers: bool,
    /// J1's pads 2 and 3 carry each other's nets.
    swap_j1_nets: bool,
    /// Every net of the board spelled the way netsmith names its own (`net-gpio0`).
    own_names: bool,
    /// The corner at the top right is cut by a line instead of an arc: the same box, one rounded
    /// corner fewer.
    chamfer: bool,
    /// Replacements `(from, to)` in the board's text, each `from` found once.
    edits: Vec<(String, String)>,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            origin: (100.0, 50.0),
            wider: 0.0,
            hole_shift: 0.0,
            hole_plated: false,
            hole_pad: 2.75,
            hole_net: "",
            j2_shift: 0.0,
            j1_shift: 0.0,
            sw1_rotation: 0.0,
            u1_shift: 0.0,
            swap_j2_numbers: false,
            swap_j1_nets: false,
            own_names: false,
            chamfer: false,
            edits: Vec::new(),
        }
    }
}

/// The default board with these replacements in its text.
fn edited(edits: &[(&str, &str)]) -> Spec {
    Spec {
        edits: edits
            .iter()
            .map(|(from, to)| (from.to_string(), to.to_string()))
            .collect(),
        ..Spec::default()
    }
}

/// J2's pads as the board text writes them (thru-hole, rect, 1.6 x 1.2, drill 1).
const J2_PAD_1: &str = r#"(pad "1" thru_hole rect (at 0 0 0) (size 1.6 1.2)"#;
const J2_PAD_3: &str = r#"(pad "3" thru_hole rect (at 2.54 0 0) (size 1.6 1.2) (drill 1)"#;
const J2_PAD_4: &str =
    r#"(pad "4" thru_hole rect (at 2.54 2.54 0) (size 1.6 1.2) (drill 1) (layers "*.Cu")"#;
const J2_PAD_5A: &str = r#"(pad "5" thru_hole rect (at 5.08 0 0) (size 1.6 1.2) (drill 1) (layers "*.Cu") (net "GND"))"#;
const J2_PAD_5B: &str = r#"(pad "5" thru_hole rect (at 5.08 2.54 0) (size 1.6 1.2) (drill 1) (layers "*.Cu") (net "GND"))"#;
const J2_PAD_7: &str = r#"(pad "7" thru_hole rect (at 7.62 0 0) (size 1.6 1.2) (drill 1) (layers "*.Cu") (net "/ONLY"))"#;
const J2_PAD_7_BARE: &str =
    r#"(pad "7" thru_hole rect (at 7.62 0 0) (size 1.6 1.2) (drill 1) (layers "*.Cu"))"#;

fn replaced(text: &str, from: &str, to: &str) -> String {
    assert_eq!(text.matches(from).count(), 1, "{from:?} is not found once");
    text.replacen(from, to, 1)
}

fn arc(cx: f64, cy: f64, r: f64, corner: usize) -> String {
    // The arc of a rounded corner: the start and end on the two sides, the middle on the curve.
    let (sx, sy) = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)][corner];
    let (ex, ey) = [(0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (1.0, 0.0)][corner];
    let (mx, my) = (
        (sx + ex) * std::f64::consts::FRAC_1_SQRT_2,
        (sy + ey) * std::f64::consts::FRAC_1_SQRT_2,
    );
    format!(
        "\t(gr_arc (start {} {}) (mid {} {}) (end {} {}) (layer \"Edge.Cuts\"))\n",
        cx + sx * r,
        cy + sy * r,
        cx + mx * r,
        cy + my * r,
        cx + ex * r,
        cy + ey * r
    )
}

fn net(spec: &Spec, name: &str) -> String {
    // A net that only one pad is on has nothing to vouch for a renamed one: it keeps its name.
    let shown = if spec.own_names && name != "GND" && name != "/ONLY" {
        format!("net-{}", name.trim_start_matches('/').to_lowercase())
    } else {
        name.to_string()
    };
    format!(" (net \"{shown}\")")
}

/// A footprint of thru-hole pads `(number, x, y, net)`, written as KiCad 10 writes it.
fn footprint(
    spec: &Spec,
    reference: &str,
    at: (f64, f64),
    rotation: f64,
    kind: &str,
    pads: &[(&str, f64, f64, &str)],
) -> String {
    let (ox, oy) = spec.origin;
    let mut s = format!(
        "\t(footprint \"Lib:{reference}\"\n\t\t(layer \"F.Cu\")\n\t\t(at {} {} {})\n\t\t\
         (property \"Reference\" \"{reference}\" (at 0 0 0) (layer \"F.SilkS\"))\n",
        ox + at.0,
        oy + at.1,
        rotation
    );
    for (number, x, y, name) in pads {
        let _ = writeln!(
            s,
            "\t\t(pad \"{number}\" {kind} rect (at {x} {y} {rotation}) (size 1.6 1.2) (drill 1) \
             (layers \"*.Cu\"){})",
            if name.is_empty() {
                String::new()
            } else {
                net(spec, name)
            }
        );
    }
    s.push_str("\t)\n");
    s
}

/// A board of 40 x 30 mm with 2 mm corners, two mounting holes, two headers, a button and an
/// MCU stand-in (U1) whose pads share the headers' nets.
fn pcb(spec: &Spec) -> String {
    let (ox, oy) = spec.origin;
    let (w, h, r) = (40.0 + spec.wider, 30.0, 2.0);
    let mut s = String::from("(kicad_pcb\n\t(version 20260206)\n\t(generator \"pcbnew\")\n");
    let _ = writeln!(
        s,
        "\t(gr_line (start {} {}) (end {} {}) (layer \"Edge.Cuts\"))",
        ox + r,
        oy,
        ox + w - r,
        oy
    );
    if spec.chamfer {
        let _ = writeln!(
            s,
            "\t(gr_line (start {} {}) (end {} {}) (layer \"Edge.Cuts\"))",
            ox + w - r,
            oy,
            ox + w,
            oy + r
        );
    } else {
        s.push_str(&arc(ox + w - r, oy + r, r, 3));
    }
    let _ = writeln!(
        s,
        "\t(gr_line (start {} {}) (end {} {}) (layer \"Edge.Cuts\"))",
        ox + w,
        oy + r,
        ox + w,
        oy + h - r
    );
    s.push_str(&arc(ox + w - r, oy + h - r, r, 0));
    let _ = writeln!(
        s,
        "\t(gr_line (start {} {}) (end {} {}) (layer \"Edge.Cuts\"))",
        ox + w - r,
        oy + h,
        ox + r,
        oy + h
    );
    s.push_str(&arc(ox + r, oy + h - r, r, 1));
    let _ = writeln!(
        s,
        "\t(gr_line (start {} {}) (end {} {}) (layer \"Edge.Cuts\"))",
        ox,
        oy + h - r,
        ox,
        oy + r
    );
    s.push_str(&arc(ox + r, oy + r, r, 2));

    for (name, x) in [("MH1", 3.0), ("MH2", 37.0 + spec.hole_shift)] {
        let second = name == "MH2";
        let (library, kind) = if second && spec.hole_plated {
            ("MountingHole_2.75mm_Pad", "thru_hole")
        } else {
            ("Hole", "np_thru_hole")
        };
        let pad = if second { spec.hole_pad } else { 2.75 };
        let net = if second && !spec.hole_net.is_empty() {
            net(spec, spec.hole_net)
        } else {
            String::new()
        };
        s.push_str(&format!(
            "\t(footprint \"Lib:{library}\"\n\t\t(layer \"F.Cu\")\n\t\t(at {} {})\n\t\t\
             (property \"Reference\" \"{name}\" (at 0 0 0) (layer \"F.SilkS\"))\n\t\t\
             (pad \"\" {kind} circle (at 0 0) (size {pad} {pad}) (drill 2.75) \
             (layers \"*.Cu\"){net})\n\t)\n",
            ox + x,
            oy + 3.0
        ));
    }
    s.push_str(&footprint(
        spec,
        "J1",
        (10.0 + spec.j1_shift, 15.0),
        270.0,
        "thru_hole",
        &[
            ("1", 0.0, 0.0, "GND"),
            (
                "2",
                0.0,
                2.54,
                if spec.swap_j1_nets {
                    "/GPIO1"
                } else {
                    "/GPIO0"
                },
            ),
            (
                "3",
                2.54,
                0.0,
                if spec.swap_j1_nets {
                    "/GPIO0"
                } else {
                    "/GPIO1"
                },
            ),
            ("4", 2.54, 2.54, "GND"),
        ],
    ));
    let (a, b) = if spec.swap_j2_numbers {
        ("2", "1")
    } else {
        ("1", "2")
    };
    s.push_str(&footprint(
        spec,
        "J2",
        (28.0 + spec.j2_shift, 15.0),
        0.0,
        "thru_hole",
        &[
            (a, 0.0, 0.0, "GND"),
            (b, 0.0, 2.54, "/GPIO2"),
            ("3", 2.54, 0.0, "/GPIO3"),
            ("4", 2.54, 2.54, "GND"),
            // Two pads of one number (like J1's shell: a surface pad and a plated slot), and a
            // net that only this part is on.
            ("5", 5.08, 0.0, "GND"),
            ("5", 5.08, 2.54, "GND"),
            ("7", 7.62, 0.0, "/ONLY"),
        ],
    ));
    s.push_str(&footprint(
        spec,
        "SW1",
        (20.0, 25.0),
        spec.sw1_rotation,
        "smd",
        &[("1", -2.0, 0.0, "RUN"), ("2", 2.0, 0.0, "GND")],
    ));
    s.push_str(&footprint(
        spec,
        "U1",
        (20.0 + spec.u1_shift, 8.0),
        0.0,
        "smd",
        &[
            ("1", -3.0, 0.0, "/GPIO0"),
            ("2", -1.0, 0.0, "/GPIO1"),
            ("3", 1.0, 0.0, "/GPIO2"),
            ("4", 3.0, 0.0, "/GPIO3"),
            ("5", 0.0, 2.0, "GND"),
            ("6", 0.0, -2.0, "RUN"),
        ],
    ));
    s.push_str(")\n");
    for (from, to) in &spec.edits {
        s = replaced(&s, from, to);
    }
    s
}

fn table() -> String {
    placements_toml(&pcb(&Spec::default()), "synthetic.kicad_pcb").unwrap()
}

fn identity() -> DesignatorMap {
    DesignatorMap::identity(&Board::from_table(&table()).unwrap())
}

fn run_with(spec: &Spec, map: &DesignatorMap) -> ParityReport {
    check_text(&table(), &pcb(spec), map, &Tolerances::default()).unwrap_or_else(|e| panic!("{e}"))
}

fn run(spec: &Spec) -> ParityReport {
    run_with(spec, &identity())
}

/// The table is made from the `reference` board, the checked board is `board`.
fn run_pair(reference: &Spec, board: &Spec) -> ParityReport {
    let table = placements_toml(&pcb(reference), "synthetic.kicad_pcb").unwrap();
    let map = DesignatorMap::identity(&Board::from_table(&table).unwrap());
    check_text(&table, &pcb(board), &map, &Tolerances::default()).unwrap_or_else(|e| panic!("{e}"))
}

fn item<'a>(report: &'a ParityReport, id: &str) -> &'a Item {
    report
        .items
        .iter()
        .find(|i| i.id == id)
        .unwrap_or_else(|| panic!("no item {id:?} in\n{}", report.text()))
}

/// The report fails, and the item of that id is the one that says so.
fn fails(report: &ParityReport, id: &str) {
    assert!(!report.passed(), "{id} should fail:\n{}", report.text());
    assert_eq!(
        item(report, id).verdict,
        Verdict::Fail,
        "{id}:\n{}",
        report.text()
    );
    let line = report
        .text()
        .lines()
        .find(|l| l.starts_with("FAIL") && l.contains(id))
        .map(str::to_string);
    assert!(line.is_some(), "the text names {id}:\n{}", report.text());
}

#[test]
fn a_copy_of_the_reference_passes() {
    let report = run(&Spec::default());
    assert!(report.passed(), "{}", report.text());
    assert_eq!(report.failures().count(), 0);
    let (ok, failed) = report.must_match_counts();
    assert!(ok > 30 && failed == 0, "{}", report.text());
    assert!(
        report
            .text()
            .contains(&format!("must-match: {ok} ok, 0 failed")),
        "{}",
        report.text()
    );
}

#[test]
fn a_board_moved_as_a_whole_passes_because_the_corner_is_the_origin() {
    // fab gives a board an origin of its own: here the corner is at (0, 0), not (100, 50).
    for origin in [(0.0, 0.0), (-17.5, 230.25)] {
        let report = run(&Spec {
            origin,
            ..Spec::default()
        });
        assert!(report.passed(), "{origin:?}\n{}", report.text());
        let origin = item(&report, "outline origin");
        assert_eq!(origin.verdict, Verdict::Info);
        assert!(origin.dx.unwrap().abs() > 1.0, "the corners are reported");
    }
    let text = run(&Spec::default()).text();
    assert!(text.contains("upper-left corner"), "{text}");
}

#[test]
fn an_outline_that_is_too_wide_fails() {
    let report = run(&Spec {
        wider: 0.2,
        ..Spec::default()
    });
    fails(&report, "outline size");
    let size = item(&report, "outline size").dsize.unwrap();
    assert!(
        (size.0 - 0.2).abs() < 1e-6 && size.1.abs() < 1e-6,
        "{size:?}"
    );
}

#[test]
fn a_mounting_hole_that_moved_by_a_tenth_fails() {
    let report = run(&Spec {
        hole_shift: 0.1,
        ..Spec::default()
    });
    fails(&report, "hole MH2");
    assert_eq!(item(&report, "hole MH1").verdict, Verdict::Ok);
    assert!((item(&report, "hole MH2").dx.unwrap() - 0.1).abs() < 1e-6);
}

/// A hole is judged by more than its place and drill: whether it is plated, its pad's size and
/// its net. The reference's holes are non-plated 2.75 mm pads on no net.
#[test]
fn a_hole_that_is_plated_or_has_another_pad_or_net_fails() {
    // The board's second hole is a plated pad of the same drill and size: not the same hole.
    let report = run(&Spec {
        hole_plated: true,
        ..Spec::default()
    });
    fails(&report, "hole MH2");
    assert!(
        item(&report, "hole MH2").note.contains("plated"),
        "{}",
        report.text()
    );
    assert_eq!(item(&report, "hole MH1").verdict, Verdict::Ok);

    // The same drill with a pad of 2.9 (the reference's is 2.75): a size difference.
    let report = run(&Spec {
        hole_pad: 2.9,
        ..Spec::default()
    });
    fails(&report, "hole MH2");
    assert!(
        item(&report, "hole MH2").note.contains("pad"),
        "{}",
        report.text()
    );
    // Within the tolerance of a pad's size (0.05) it passes.
    let report = run(&Spec {
        hole_pad: 2.79,
        ..Spec::default()
    });
    assert!(report.passed(), "{}", report.text());

    // A plated hole on GND in the reference: the board's must be on GND too.
    let plated_gnd = Spec {
        hole_plated: true,
        hole_net: "GND",
        ..Spec::default()
    };
    let plated = Spec {
        hole_plated: true,
        ..Spec::default()
    };
    let report = run_pair(&plated_gnd, &plated_gnd);
    assert!(report.passed(), "{}", report.text());
    let report = run_pair(&plated_gnd, &plated);
    fails(&report, "hole MH2");
    assert!(
        item(&report, "hole MH2").note.contains("net"),
        "{}",
        report.text()
    );
    let report = run_pair(&plated, &plated_gnd);
    fails(&report, "hole MH2");
    // Plated and unplated against each other, in both directions.
    let report = run_pair(&plated, &Spec::default());
    fails(&report, "hole MH2");
}

#[test]
fn a_header_that_is_two_tenths_off_fails() {
    let report = run(&Spec {
        j2_shift: 0.2,
        ..Spec::default()
    });
    fails(&report, "part J2 position");
    assert_eq!(item(&report, "part J1 position").verdict, Verdict::Ok);
    // The pads moved with the part: their places in its own frame are the same.
    assert_eq!(item(&report, "part J2 pad 1").verdict, Verdict::Ok);
}

#[test]
fn a_button_turned_by_ninety_degrees_fails() {
    let report = run(&Spec {
        sw1_rotation: 90.0,
        ..Spec::default()
    });
    fails(&report, "part SW1 rotation");
    assert!((item(&report, "part SW1 rotation").drotation.unwrap() - 90.0).abs() < 1e-9);
    // 360 degrees is no turn at all.
    let report = run(&Spec {
        sw1_rotation: 360.0,
        ..Spec::default()
    });
    assert!(report.passed(), "{}", report.text());
}

#[test]
fn a_header_pad_with_the_wrong_number_fails() {
    let report = run(&Spec {
        swap_j2_numbers: true,
        ..Spec::default()
    });
    fails(&report, "part J2 pad 1");
    fails(&report, "part J2 pad 2");
}

#[test]
fn two_header_pads_on_the_wrong_nets_fail() {
    let report = run(&Spec {
        swap_j1_nets: true,
        ..Spec::default()
    });
    fails(&report, "part J1 pad 2 net");
    fails(&report, "part J1 pad 3 net");
    assert_eq!(item(&report, "part J1 pad 1 net").verdict, Verdict::Ok);
    let note = &item(&report, "part J1 pad 2 net").note;
    assert!(note.contains("U1.2") && note.contains("U1.1"), "{note}");
}

#[test]
fn a_move_of_three_hundredths_passes() {
    let report = run(&Spec {
        wider: 0.03,
        hole_shift: 0.03,
        j1_shift: 0.03,
        j2_shift: -0.03,
        ..Spec::default()
    });
    assert!(report.passed(), "{}", report.text());
}

#[test]
fn a_part_that_need_not_match_moved_by_five_millimetres_is_info() {
    let report = run(&Spec {
        u1_shift: 5.0,
        ..Spec::default()
    });
    assert!(report.passed(), "{}", report.text());
    let u1 = item(&report, "part U1 position");
    assert_eq!(u1.verdict, Verdict::Info);
    assert!(!u1.must_match);
    assert!((u1.dx.unwrap() - 5.0).abs() < 1e-6);
    assert!(report.text().contains("info  "), "{}", report.text());
}

#[test]
fn a_must_match_part_without_a_map_entry_fails_and_another_is_a_note() {
    let table = Board::from_table(&table()).unwrap();
    let lines = |skip: &[&str]| {
        table
            .parts
            .iter()
            .filter(|p| !skip.contains(&p.reference.as_str()))
            .map(|p| format!("{0} = \"{0}\"\n", p.reference))
            .collect::<String>()
    };
    let map = DesignatorMap::parse(&lines(&["J2", "U1"])).unwrap();
    let report = run_with(&Spec::default(), &map);
    fails(&report, "part J2 unmapped");
    assert_eq!(item(&report, "part U1 unmapped").verdict, Verdict::Info);

    // A line that names a part the board does not have.
    let map = DesignatorMap::parse(&format!("{}J2 = \"J9\"\n", lines(&["J2"]))).unwrap();
    fails(&run_with(&Spec::default(), &map), "part J2 missing");
}

#[test]
fn nets_named_in_netsmiths_way_pass_where_the_connectivity_vouches_for_them() {
    let report = run(&Spec {
        own_names: true,
        ..Spec::default()
    });
    assert!(report.passed(), "{}", report.text());
    assert!(
        report.warnings.iter().any(|w| w.contains("part J1 pad 2")),
        "{:?}",
        report.warnings
    );
    // With U1 left out of the map nothing says which pads share the net: it cannot be proven.
    let table = Board::from_table(&table()).unwrap();
    let map = DesignatorMap::parse(
        &table
            .parts
            .iter()
            .filter(|p| p.reference != "U1")
            .map(|p| format!("{0} = \"{0}\"\n", p.reference))
            .collect::<String>(),
    )
    .unwrap();
    let report = run_with(
        &Spec {
            own_names: true,
            ..Spec::default()
        },
        &map,
    );
    fails(&report, "part J1 pad 2 net");
    assert!(item(&report, "part J1 pad 2 net").note.contains("vouch"));
}

#[test]
fn the_text_has_a_line_for_each_item_sorted_and_a_summary() {
    let report = run(&Spec {
        j2_shift: 0.2,
        ..Spec::default()
    });
    let text = report.text();
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.last().unwrap().starts_with("must-match: "), "{text}");
    let item_lines = lines
        .iter()
        .filter(|l| l.starts_with("ok ") || l.starts_with("FAIL") || l.starts_with("info"))
        .count();
    assert_eq!(item_lines, report.items.len(), "{text}");
    let at = |needle: &str| {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle}\n{text}"))
    };
    assert!(at("outline size") < at("hole MH1"));
    assert!(at("hole MH2") < at("part J1 position"));
    assert!(at("part J1 position") < at("part J2 position"));
    assert!(at("part J2 position") < at("part SW1 position"));
    assert!(at("part SW1 position") < at("part U1 position"));
}

/// A check that the text is XML: every tag closes in order, attributes are quoted, a name that
/// is an attribute appears once in a tag.
fn well_formed(svg: &str) {
    let mut stack: Vec<String> = Vec::new();
    let mut rest = svg;
    let mut roots = 0;
    while let Some(start) = rest.find('<') {
        let end = rest[start..].find('>').expect("a tag that never ends") + start;
        let tag = &rest[start + 1..end];
        rest = &rest[end + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            assert_eq!(
                stack.pop().as_deref(),
                Some(name.trim()),
                "unbalanced {tag}"
            );
            continue;
        }
        let body = tag.trim_end_matches('/');
        let name = body.split_whitespace().next().unwrap();
        // Attributes: name="value" pairs, each name once.
        let mut names = Vec::new();
        let mut attrs = &body[name.len()..];
        while let Some(eq) = attrs.find('=') {
            let attr = attrs[..eq].trim();
            names.push(attr);
            assert!(
                attrs[eq + 1..].starts_with('"'),
                "unquoted attribute in {tag}"
            );
            let close = attrs[eq + 2..]
                .find('"')
                .expect("an attribute that never ends")
                + eq
                + 2;
            attrs = &attrs[close + 1..];
        }
        assert!(attrs.trim().is_empty(), "stray text in {tag}");
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "an attribute twice in {tag}");
        if !tag.ends_with('/') {
            if stack.is_empty() {
                roots += 1;
            }
            stack.push(name.to_string());
        } else if stack.is_empty() {
            roots += 1;
        }
    }
    assert!(stack.is_empty(), "unclosed {stack:?}");
    assert_eq!(roots, 1, "one root element");
}

#[test]
fn the_overlay_is_svg_and_marks_the_failing_item() {
    let failing = run(&Spec {
        j2_shift: 0.2,
        ..Spec::default()
    });
    let svg = failing.overlay_svg();
    well_formed(&svg);
    assert!(svg.starts_with("<svg "), "{svg}");
    assert!(
        svg.contains("<g class=\"part fail\" data-ref=\"J2\""),
        "the failing part is red:\n{svg}"
    );
    assert!(!svg.contains("data-ref=\"J1\" stroke"), "J1 is not marked");
    assert!(
        svg.contains(">J2 position: dx +0.200 dy +0.000</text>"),
        "the label has the offset:\n{svg}"
    );
    assert!(
        svg.contains("#2b6cb0") && svg.contains("#dd6b20"),
        "two colours"
    );

    // A failing outline is marked too.
    let wide = run(&Spec {
        wider: 0.2,
        ..Spec::default()
    })
    .overlay_svg();
    well_formed(&wide);
    assert!(wide.contains("class=\"outline fail\""), "{wide}");

    // A passing board has no mark.
    let good = run(&Spec::default()).overlay_svg();
    well_formed(&good);
    assert!(!good.contains(" fail\""), "{good}");
    assert!(
        good.contains(">J2 position: dx +0.000 dy +0.000</text>"),
        "{good}"
    );
}

// ---- review of the checker (#372): each way a board that is not interchangeable passed -------

/// The pad's own item names what differs.
fn differs_in(report: &ParityReport, id: &str, what: &str) {
    fails(report, id);
    let note = &item(report, id).note;
    assert!(note.contains(what), "{id}: {note}\n{}", report.text());
}

#[test]
fn every_pad_of_a_number_is_judged_not_the_first_one() {
    // J1's shell pads 6, 7, 9 and 10 have two pads each. Here J2's pad 5 has two, and the
    // second one is on another net: the first pad's net must not speak for both.
    let report = run(&edited(&[(
        J2_PAD_5B,
        &J2_PAD_5B.replace("(net \"GND\")", "(net \"/GPIO2\")"),
    )]));
    fails(&report, "part J2 pad 5 (2 of 2) net");
    assert_eq!(
        item(&report, "part J2 pad 5 (1 of 2) net").verdict,
        Verdict::Ok
    );
    // Both pads have an item of their own when all is well.
    let report = run(&Spec::default());
    for id in ["part J2 pad 5 (1 of 2) net", "part J2 pad 5 (2 of 2) net"] {
        assert_eq!(item(&report, id).verdict, Verdict::Ok, "{id}");
    }
}

#[test]
fn pads_of_one_number_are_paired_by_their_place_in_the_footprint() {
    // The board lists the two pads of number 5 the other way round, and the pad at (5.08, 0)
    // is on /GPIO2 there: the pairing is by place, not by the order of the file.
    let board = edited(&[(
        &format!("{J2_PAD_5A}\n\t\t{J2_PAD_5B}"),
        &format!(
            "{J2_PAD_5B}\n\t\t{}",
            J2_PAD_5A.replace("(net \"GND\")", "(net \"/GPIO2\")")
        ),
    )]);
    let report = run(&board);
    // The pad at (5.08, 0) is on /GPIO2 in the board and on GND in the reference.
    fails(&report, "part J2 pad 5 (1 of 2) net");
    assert_eq!(
        item(&report, "part J2 pad 5 (2 of 2) net").verdict,
        Verdict::Ok
    );
}

#[test]
fn the_same_copper_written_turned_is_not_a_size_difference() {
    // 1.2 x 1.6 turned by 90 degrees is 1.6 x 1.2.
    let turned = r#"(pad "1" thru_hole rect (at 0 0 90) (size 1.2 1.6)"#;
    let report = run(&edited(&[(J2_PAD_1, turned)]));
    assert!(report.passed(), "{}", report.text());
    assert_eq!(item(&report, "part J2 pad 1").verdict, Verdict::Ok);
}

#[test]
fn a_pad_turned_by_a_quarter_or_with_swapped_sides_fails() {
    let turned = r#"(pad "1" thru_hole rect (at 0 0 90) (size 1.6 1.2)"#;
    differs_in(
        &run(&edited(&[(J2_PAD_1, turned)])),
        "part J2 pad 1",
        "size",
    );
    let swapped = r#"(pad "1" thru_hole rect (at 0 0 0) (size 1.2 1.6)"#;
    differs_in(
        &run(&edited(&[(J2_PAD_1, swapped)])),
        "part J2 pad 1",
        "size",
    );
    // A reference pad that is 1.2 x 1.6 turned by 90 and a board pad that is 1.2 x 1.6 not
    // turned are two shapes of copper, though the figures of size are the same.
    let reference = edited(&[(
        J2_PAD_1,
        r#"(pad "1" thru_hole rect (at 0 0 90) (size 1.2 1.6)"#,
    )]);
    let board = edited(&[(
        J2_PAD_1,
        r#"(pad "1" thru_hole rect (at 0 0 0) (size 1.2 1.6)"#,
    )]);
    differs_in(&run_pair(&reference, &board), "part J2 pad 1", "size");
    // An angle that is no quarter turn is compared too.
    let reference = edited(&[(
        J2_PAD_1,
        r#"(pad "1" thru_hole rect (at 0 0 30) (size 1.6 1.2)"#,
    )]);
    let board = edited(&[(
        J2_PAD_1,
        r#"(pad "1" thru_hole rect (at 0 0 40) (size 1.6 1.2)"#,
    )]);
    differs_in(&run_pair(&reference, &board), "part J2 pad 1", "angle");
    assert!(run_pair(&reference, &reference).passed());
}

#[test]
fn a_pad_of_another_shape_fails() {
    for shape in ["oval", "roundrect", "circle", "custom"] {
        let other = format!(r#"(pad "1" thru_hole {shape} (at 0 0 0) (size 1.6 1.2)"#);
        differs_in(
            &run(&edited(&[(J2_PAD_1, &other)])),
            "part J2 pad 1",
            "shape",
        );
    }
}

fn with_polygon(points: &str) -> Spec {
    let custom = format!(
        r#"{} (primitives (gr_poly (pts {points}) (width 0) (fill yes)))"#,
        J2_PAD_4.replace(" rect ", " custom ")
    );
    edited(&[(J2_PAD_4, &custom)])
}

#[test]
fn a_custom_pad_is_compared_by_the_extent_of_its_polygon() {
    let small = "(xy -1 -1) (xy 1 -1) (xy 1 1)";
    let big = "(xy -1 -1) (xy 1.4 -1) (xy 1 1)";
    let same = run_pair(&with_polygon(small), &with_polygon(small));
    assert!(same.passed(), "{}", same.text());
    differs_in(
        &run_pair(&with_polygon(small), &with_polygon(big)),
        "part J2 pad 4",
        "extent",
    );
    // Custom against a plain rectangle of the same anchor is a shape difference.
    differs_in(
        &run_pair(&with_polygon(small), &Spec::default()),
        "part J2 pad 4",
        "shape",
    );
}

#[test]
fn both_sides_of_a_slot_are_compared() {
    let slot = |length: &str| {
        let pad = format!(
            r#"(pad "3" thru_hole oval (at 2.54 0 0) (size 1.6 1.2) (drill oval 0.65 {length})"#
        );
        edited(&[(J2_PAD_3, &pad)])
    };
    let same = run_pair(&slot("1.05"), &slot("1.05"));
    assert!(same.passed(), "{}", same.text());
    // The short side is the same, the length is half a millimetre more.
    differs_in(
        &run_pair(&slot("1.05"), &slot("1.55")),
        "part J2 pad 3",
        "drill",
    );
    // A slot against a round hole.
    let round = r#"(pad "3" thru_hole oval (at 2.54 0 0) (size 1.6 1.2) (drill 0.65)"#;
    differs_in(
        &run_pair(&slot("1.05"), &edited(&[(J2_PAD_3, round)])),
        "part J2 pad 3",
        "drill",
    );
    // A slot written turned by a quarter, with its sides swapped, is the same slot.
    let turned = r#"(pad "3" thru_hole oval (at 2.54 0 90) (size 1.2 1.6) (drill oval 1.05 0.65)"#;
    let report = run_pair(&slot("1.05"), &edited(&[(J2_PAD_3, turned)]));
    assert!(report.passed(), "{}", report.text());
}

#[test]
fn an_outline_with_a_cut_corner_or_a_cut_out_fails_though_its_box_is_the_same() {
    let report = run(&Spec {
        chamfer: true,
        ..Spec::default()
    });
    fails(&report, "outline elements");
    // The box and the radius of the arcs that are left are the same.
    assert_eq!(item(&report, "outline size").verdict, Verdict::Ok);
    assert_eq!(item(&report, "outline corner radius").verdict, Verdict::Ok);
    let note = &item(&report, "outline elements").note;
    assert!(note.contains("lines") && note.contains("arcs"), "{note}");

    // A rectangular cut-out inside the board.
    let cut = r#"	(gr_rect (start 110 55) (end 115 60) (layer "Edge.Cuts"))
	(footprint "Lib:J1""#;
    let report = run(&edited(&[("\t(footprint \"Lib:J1\"", cut)]));
    fails(&report, "outline elements");
    assert_eq!(item(&report, "outline size").verdict, Verdict::Ok);

    // And the board itself is fine.
    assert_eq!(
        item(&run(&Spec::default()), "outline elements").verdict,
        Verdict::Ok
    );
}

/// A peg (an `np_thru_hole` pad) on J2, between the centre of its first pads.
const PEG: &str =
    "(pad \"\" np_thru_hole circle (at 1 1 0) (size 1 1) (drill 1) (layers \"*.Cu\"))\n\t\t";

fn with_peg() -> Vec<(String, String)> {
    vec![(format!("\t\t{J2_PAD_1}"), format!("\t\t{PEG}{J2_PAD_1}"))]
}

#[test]
fn a_connector_with_a_peg_is_still_judged_as_a_part() {
    let reference = Spec {
        edits: with_peg(),
        ..Spec::default()
    };
    // Moved by 0.2 mm: the part's own position fails, the peg does not hide it.
    let moved = Spec {
        j2_shift: 0.2,
        edits: with_peg(),
        ..Spec::default()
    };
    let report = run_pair(&reference, &moved);
    fails(&report, "part J2 position");
    // A copy of it passes, and its pads (the peg too) are items.
    let report = run_pair(&reference, &reference);
    assert!(report.passed(), "{}", report.text());
    assert_eq!(
        item(&report, "part J2 pad (no number)").verdict,
        Verdict::Ok
    );
    // The board's J2 without the peg fails as a pad that has no partner.
    let report = run_pair(&reference, &Spec::default());
    fails(&report, "part J2 pad (no number)");
    // A mounting hole (a part of nothing but NPTH pads) is still not a part.
    assert!(
        report.items.iter().all(|i| i.id != "part MH1 position"),
        "{}",
        report.text()
    );
}

#[test]
fn one_plain_name_and_one_made_up_pass_only_where_other_pads_vouch_for_the_net() {
    // J2 pad 7 is on /ONLY, a net that no other mapped pad is on. In the board it has no net,
    // or a made-up name: nothing says it is the same net.
    for board_pad in [
        J2_PAD_7_BARE.to_string(),
        J2_PAD_7.replace("/ONLY", "Net-(J2-Pad7)"),
        J2_PAD_7.replace("/ONLY", "unconnected-(J2-Pad7)"),
    ] {
        let report = run(&edited(&[(J2_PAD_7, &board_pad)]));
        fails(&report, "part J2 pad 7 net");
        assert!(
            item(&report, "part J2 pad 7 net").note.contains("vouch"),
            "{}",
            report.text()
        );
    }
    // Both made up, and nothing else on either net: the same (a pad on no net).
    let reference = edited(&[(J2_PAD_7, J2_PAD_7_BARE)]);
    let board = edited(&[(
        J2_PAD_7,
        &J2_PAD_7.replace("/ONLY", "unconnected-(J2-Pad7)"),
    )]);
    let report = run_pair(&reference, &board);
    assert!(report.passed(), "{}", report.text());

    // With pads that vouch (the net of J2's pad 2 has U1's pad 3 on it) it passes, with a warning.
    let board = edited(&[
        (
            r#"(pad "2" thru_hole rect (at 0 2.54 0) (size 1.6 1.2) (drill 1) (layers "*.Cu") (net "/GPIO2"))"#,
            r#"(pad "2" thru_hole rect (at 0 2.54 0) (size 1.6 1.2) (drill 1) (layers "*.Cu") (net "Net-(U1-Pad3)"))"#,
        ),
        (
            r#"(pad "3" smd rect (at 1 0 0) (size 1.6 1.2) (drill 1) (layers "*.Cu") (net "/GPIO2"))"#,
            r#"(pad "3" smd rect (at 1 0 0) (size 1.6 1.2) (drill 1) (layers "*.Cu") (net "Net-(U1-Pad3)"))"#,
        ),
    ]);
    let report = run(&board);
    assert!(report.passed(), "{}", report.text());
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("part J2 pad 2") && w.contains("made-up")),
        "{:?}",
        report.warnings
    );
}

/// A footprint of the reference that the map does not name: `R9`, one pad on `net`.
fn r9(net: &str) -> String {
    format!(
        "\t(footprint \"Lib:R9\"\n\t\t(layer \"F.Cu\")\n\t\t(at 105 52 0)\n\t\t\
         (property \"Reference\" \"R9\" (at 0 0 0) (layer \"F.SilkS\"))\n\t\t\
         (pad \"1\" smd rect (at 0 0 0) (size 1 1) (layers \"F.Cu\") (net \"{net}\"))\n\t)\n"
    )
}

const BEFORE_U1: &str = "\t(footprint \"Lib:U1\"";

/// The reference's table with R9 in it, and the map of every part but R9.
fn run_without_r9(reference: &Spec, board: &Spec) -> ParityReport {
    let table = placements_toml(&pcb(reference), "synthetic.kicad_pcb").unwrap();
    let parts = Board::from_table(&table).unwrap();
    let map = DesignatorMap::parse(
        &parts
            .parts
            .iter()
            .filter(|p| p.reference != "R9")
            .map(|p| format!("{0} = \"{0}\"\n", p.reference))
            .collect::<String>(),
    )
    .unwrap();
    check_text(&table, &pcb(board), &map, &Tolerances::default()).unwrap_or_else(|e| panic!("{e}"))
}

/// A pad with no net is not the same as a made-up name: `Net-(...)` says other pads are on the
/// net, and a pad on a net with other pads (mapped or not) does not pass against a pad on none.
/// Both unconnected passes.
#[test]
fn a_pad_that_the_reference_connects_does_not_pass_against_a_pad_with_no_net() {
    let made_up = J2_PAD_7.replace("/ONLY", "Net-(J2-Pad7)");
    // The reference's J2 pad 7 is on a made-up net that R9 (which the map does not name) is on.
    let connected = edited(&[
        (J2_PAD_7, &made_up),
        (BEFORE_U1, &format!("{}{BEFORE_U1}", r9("Net-(J2-Pad7)"))),
    ]);
    let bare = edited(&[(J2_PAD_7, J2_PAD_7_BARE)]);
    let report = run_without_r9(&connected, &bare);
    fails(&report, "part J2 pad 7 net");
    assert!(
        item(&report, "part J2 pad 7 net").note.contains("connects"),
        "{}",
        report.text()
    );

    // And the other way: the board connects a pad that the reference leaves alone.
    let report = run_pair(&bare, &connected);
    fails(&report, "part J2 pad 7 net");

    // Alone on its made-up net, the reference's pad is connected to nothing: it passes against
    // a pad with no net, and so does a pad with none against another with none.
    let alone = edited(&[(J2_PAD_7, &made_up)]);
    let report = run_pair(&alone, &bare);
    assert!(report.passed(), "{}", report.text());
    let report = run_pair(&bare, &bare);
    assert!(report.passed(), "{}", report.text());
    // The same net with its other pad on both sides passes (a made-up name against a made-up one).
    let report = run_without_r9(&connected, &connected);
    assert!(report.passed(), "{}", report.text());
}

#[test]
fn a_table_that_is_truncated_or_has_no_must_match_is_an_error() {
    let text = table();
    // A part with no `must_match` (or one misspelled) is an error, not `false`.
    let at = text.find("\n[[part]]\n").unwrap();
    let missing = format!(
        "{}{}",
        &text[..at],
        text[at..].replacen("must_match = ", "must_matc = ", 1)
    );
    let err = Board::from_table(&missing).unwrap_err();
    assert!(err.contains("must_match"), "{err}");
    let err = check_text(
        &missing,
        &pcb(&Spec::default()),
        &identity(),
        &Tolerances::default(),
    )
    .unwrap_err();
    assert!(err.contains("must_match"), "{err}");
    // Not a bool.
    let text_value = format!(
        "{}{}",
        &text[..at],
        text[at..].replacen("must_match = true", "must_match = \"true\"", 1)
    );
    assert!(
        Board::from_table(&text_value)
            .unwrap_err()
            .contains("must_match")
    );

    // The outline alone is no table.
    let outline_only = &text[..at];
    let outline_only = outline_only.split("\n[[hole]]\n").next().unwrap();
    let err = Board::from_table(outline_only).unwrap_err();
    assert!(err.contains("part"), "{err}");
    let err = check_text(
        outline_only,
        &pcb(&Spec::default()),
        &DesignatorMap::default(),
        &Tolerances::default(),
    )
    .unwrap_err();
    assert!(err.contains("part"), "{err}");

    // No outline, or an outline that lost the count of its lines or arcs.
    assert!(Board::from_table("").unwrap_err().contains("outline"));
    let no_lines = text.replacen("lines = ", "linez = ", 1);
    assert!(Board::from_table(&no_lines).unwrap_err().contains("lines"));
    let no_arcs = text.replacen("arcs = ", "arcz = ", 1);
    assert!(Board::from_table(&no_arcs).unwrap_err().contains("arcs"));
}

#[test]
fn the_report_says_what_each_verdict_compared() {
    let text = run(&Spec::default()).text();
    for phrase in [
        "compared  outline: the bounding box, the corner radius, the number of Edge.Cuts lines and arcs and each arc's radius",
        "compared  hole: place, drill, plated or not, the pad's size, the pad's net",
        "compared  part: position, rotation, layer",
        "compared  pad: number, centre, size and angle in the footprint's frame, shape, the extent of a custom pad's polygon, drill (both sides of a slot), type",
        "compared  net: each pad's own net, by its name or, where a name is made up, by the pads on it",
        "not compared",
    ] {
        assert!(text.contains(phrase), "{phrase}\n{text}");
    }
    // The summary line is the last one and has its old form.
    assert!(
        text.lines().last().unwrap().starts_with("must-match: "),
        "{text}"
    );
    let svg = run(&Spec::default()).overlay_svg();
    assert!(svg.contains("size, shape, drill and net"), "{svg}");
}

/// Raspberry Pi's own board file, against the table made from it, with every part mapped to
/// itself: this reads a real KiCad 10 file with the checker.
#[test]
fn raspberry_pis_board_passes_against_itself() {
    let dir = project().join("reference");
    let pcb = fs::read_to_string(dir.join("RPI-RP2350A-MINIMAL_R4-S1.kicad_pcb")).unwrap();
    let table = fs::read_to_string(dir.join("placements.toml")).unwrap();

    // The table read back is what the file says.
    let from_table = Board::from_table(&table).unwrap();
    let from_file = read_board(&pcb, "reference").unwrap();
    assert_eq!(from_table.parts.len(), 43);
    assert_eq!(from_table.holes.len(), from_file.holes.len());
    assert_eq!(from_table.outline.radii, from_file.outline.radii);
    for (a, b) in from_table.parts.iter().zip(&from_file.parts) {
        assert_eq!(a.reference, b.reference);
        assert_eq!(a.pads.len(), b.pads.len(), "{}", a.reference);
    }

    let map = DesignatorMap::identity(&from_table);
    assert_eq!(map.len(), 43);
    let report =
        check_text(&table, &pcb, &map, &Tolerances::default()).unwrap_or_else(|e| panic!("{e}"));
    assert!(report.passed(), "{}", report.text());
    assert_eq!(report.failures().count(), 0);
    assert_eq!(
        report
            .items
            .iter()
            .filter(|i| i.verdict == Verdict::Info)
            .count(),
        1,
        "only the origin line is info:\n{}",
        report.text()
    );
    let (ok, failed) = report.must_match_counts();
    assert_eq!(failed, 0);
    // The outline (2), four holes, and J1 to J4, SW1 and SW2 with their pads and nets.
    assert!(ok > 100, "{ok} items\n{}", report.text());
    for id in [
        "part J2 position",
        "part SW1 rotation",
        "hole MH4",
        "part J2 pad 1 net",
    ] {
        assert_eq!(item(&report, id).verdict, Verdict::Ok, "{id}");
    }
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    well_formed(&report.overlay_svg());
}

#[test]
fn the_designator_map_says_what_it_cannot_read() {
    assert!(DesignatorMap::parse("").unwrap().is_empty());
    assert_eq!(
        DesignatorMap::parse("J1 = \"J7\"").unwrap().get("J1"),
        Some("J7")
    );
    assert!(DesignatorMap::parse("J1 = 3").unwrap_err().contains("J1"));
    assert!(DesignatorMap::parse("J1 =").is_err());
    // The committed map reads.
    let committed = fs::read_to_string(project().join("reference/designators.toml")).unwrap();
    DesignatorMap::parse(&committed).unwrap_or_else(|e| panic!("{e}"));
}
