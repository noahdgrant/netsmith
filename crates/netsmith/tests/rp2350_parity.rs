//! The parity checker of the RP2350A Minimal port (`netsmith::parity`): small boards written as
//! KiCad text stand for what `fab` writes, and the checker must pass a copy of the reference's
//! must-match items and fail, naming the item, each way that a board can differ. One test runs
//! the checker on Raspberry Pi's own board against itself, which proves that it reads a real
//! KiCad 10 file. One more, dormant until the port has a board, builds the example with `fab`
//! and gates on the checker.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use netsmith::parity::{DesignatorMap, Item, ParityReport, Tolerances, Verdict, check_text};
use netsmith::reference_table::{Board, placements_toml, read_board};
use netsmith_kicad::testlib::TempDir;

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
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            origin: (100.0, 50.0),
            wider: 0.0,
            hole_shift: 0.0,
            j2_shift: 0.0,
            j1_shift: 0.0,
            sw1_rotation: 0.0,
            u1_shift: 0.0,
            swap_j2_numbers: false,
            swap_j1_nets: false,
            own_names: false,
        }
    }
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
    let shown = if spec.own_names && name != "GND" {
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
    s.push_str(&arc(ox + w - r, oy + r, r, 3));
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
        s.push_str(&format!(
            "\t(footprint \"Lib:Hole\"\n\t\t(layer \"F.Cu\")\n\t\t(at {} {})\n\t\t\
             (property \"Reference\" \"{name}\" (at 0 0 0) (layer \"F.SilkS\"))\n\t\t\
             (pad \"\" np_thru_hole circle (at 0 0) (size 2.75 2.75) (drill 2.75) \
             (layers \"*.Cu\"))\n\t)\n",
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

/// Dormant until the port's board exists: while `reference/designators.toml` has no entry, the
/// test is skipped with a message; with entries (the board item of the showcase fills the map)
/// it builds the example with `netsmith fab` and gates on the checker.
#[test]
fn rp2350_minimal_pcb_matches_raspberry_pi() {
    let reference = project().join("reference");
    let map =
        DesignatorMap::parse(&fs::read_to_string(reference.join("designators.toml")).unwrap())
            .unwrap_or_else(|e| panic!("{e}"));
    if map.is_empty() {
        eprintln!(
            "SKIPPED rp2350_minimal_pcb_matches_raspberry_pi: examples/rp2350_minimal/reference/\
             designators.toml has no entries, so the port has no board to compare yet"
        );
        return;
    }
    let out = TempDir::unused("rp2350-parity-fab");
    let output = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["fab", "--pins", "locked", "--out"])
        .arg(&*out)
        .current_dir(project())
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "netsmith fab failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let pcb =
        find_pcb(&out).unwrap_or_else(|| panic!("fab wrote no .kicad_pcb in {}", out.display()));
    let table = fs::read_to_string(reference.join("placements.toml")).unwrap();
    let report = check_text(
        &table,
        &fs::read_to_string(&pcb).unwrap(),
        &map,
        &Tolerances::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(report.passed(), "{}", report.text());
}

fn find_pcb(dir: &Path) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_pcb(&path) {
                return Some(found);
            }
        } else if path.extension().is_some_and(|e| e == "kicad_pcb") {
            return Some(path);
        }
    }
    None
}
