//! The KiCad board `fab` writes holds netsmith's own fill (issue #1184): each zone that pours
//! copper has a `filled_polygon` for each piece of its fill on each layer, and the copper of
//! those polygons is the copper of the fill in the Gerbers. Checked on the example with outer
//! pours and planes, with no KiCad: the areas are compared, layer by layer and zone by zone.
//! KiCad's check of the same kind of file, with no refill, is in netsmith-router's
//! `kicad_drc.rs` (`kicad_checks_the_fill_fab_writes_as_it_is...`).

use std::collections::BTreeMap;
use std::fs;
use std::io::Read as _;

use netsmith_fabricate::fab::{FabOptions, fab_with_options};
use netsmith_kicad::Sexp;
use netsmith_pipeline::Libraries;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const SOURCE: &str = include_str!("../../../examples/outline_and_pours.nsm");
const PHYSICAL: &str = include_str!("../../../examples/outline_and_pours.physical.nsm");
const LAYOUT: &str = include_str!("../../../examples/outline_and_pours.pcb.toml");
const BOARD: &str = "OutlineAndPours";

/// Twice the signed area of a ring.
fn shoelace(ring: &[(f64, f64)]) -> f64 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            a.0 * b.1 - b.0 * a.1
        })
        .sum()
}

fn perimeter(ring: &[(f64, f64)]) -> f64 {
    (0..ring.len())
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
            (b.0 - a.0).hypot(b.1 - a.1)
        })
        .sum()
}

/// What the board file holds of one zone on one layer.
#[derive(Debug, Default)]
struct Held {
    zones: usize,
    pieces: usize,
    area: f64,
    perimeter: f64,
}

/// The fill of each zone of the board file that pours copper (no rule area, no teardrop), by
/// its net and layer.
fn held(pcb: &str) -> BTreeMap<(String, String), Held> {
    let root = Sexp::parse(pcb).unwrap();
    let text = |node: &Sexp, key: &str| -> String {
        node.find(key)
            .and_then(|n| n.items().get(1)?.text())
            .unwrap_or_default()
            .to_string()
    };
    let mut out: BTreeMap<(String, String), Held> = BTreeMap::new();
    for zone in root.find_all("zone") {
        let teardrop = zone.find("attr").is_some_and(|a| a.find("teardrop").is_some());
        if zone.find("keepout").is_some() || teardrop {
            continue;
        }
        let fill = zone.find("fill").unwrap();
        assert_eq!(
            fill.items().get(1).and_then(Sexp::text),
            Some("yes"),
            "a zone KiCad reads as filled"
        );
        let net = text(zone, "net_name");
        let layer = text(zone, "layer");
        out.entry((net.clone(), layer.clone())).or_default().zones += 1;
        for f in zone.find_all("filled_polygon") {
            assert_eq!(text(f, "layer"), layer);
            let ring: Vec<(f64, f64)> = f
                .find("pts")
                .unwrap()
                .find_all("xy")
                .map(|xy| {
                    let n = |i: usize| xy.items()[i].text().unwrap().parse::<f64>().unwrap();
                    (n(1), n(2))
                })
                .collect();
            let h = out.entry((net.clone(), layer.clone())).or_default();
            h.pieces += 1;
            h.area += shoelace(&ring).abs() / 2.0;
            h.perimeter += perimeter(&ring);
        }
    }
    out
}

/// The fill of a copper Gerber as `fill::pour` writes it, first in the file: each piece one dark
/// region with the net's attribute, then its holes as clear regions. By net: the number of
/// pieces and the copper's area (the dark regions less the clear ones). Read up to the first
/// object that is no region (the pads, tracks and vias come after the fill).
fn gerber_fill(gbr: &str) -> BTreeMap<String, (usize, f64)> {
    let mut out: BTreeMap<String, (usize, f64)> = BTreeMap::new();
    let (mut dark, mut net, mut ring) = (true, String::new(), None::<Vec<(f64, f64)>>);
    let coordinate = |line: &str| -> (f64, f64) {
        let body = &line[1..line.find('D').unwrap()];
        let (x, y) = body.split_once('Y').unwrap();
        // `%FSLAX46Y46*%`: six decimals.
        (
            x.parse::<f64>().unwrap() / 1e6,
            y.parse::<f64>().unwrap() / 1e6,
        )
    };
    assert!(gbr.contains("%FSLAX46Y46*%") && gbr.contains("%MOMM*%"));
    for line in gbr.lines() {
        match line {
            "%LPD*%" => dark = true,
            "%LPC*%" => dark = false,
            "G36*" => ring = Some(Vec::new()),
            "G37*" => {
                let r = ring.take().unwrap();
                let a = shoelace(&r).abs() / 2.0;
                let e = out.entry(net.clone()).or_default();
                if dark {
                    e.0 += 1;
                    e.1 += a;
                } else {
                    e.1 -= a;
                }
            }
            l if l.starts_with("%TO.N,") => {
                net = l["%TO.N,".len()..l.len() - 2].to_string();
            }
            l if l.starts_with('X') && ring.is_some() => {
                ring.as_mut().unwrap().push(coordinate(l))
            }
            l if (l.ends_with("D03*") || l.ends_with("D01*")) && !out.is_empty() => break,
            _ => {}
        }
    }
    out
}

/// `fab` of the example in `dir`, with `routes` as the routes file beside its layout file when
/// given: the folder of the board's package, and its notes.
fn fab_example(dir: &TempDir, routes: Option<&str>) -> (std::path::PathBuf, String) {
    let layout = dir.join("source").join("outline_and_pours.pcb.toml");
    fs::create_dir_all(layout.parent().unwrap()).unwrap();
    fs::write(&layout, LAYOUT).unwrap();
    if let Some(routes) = routes {
        fs::write(layout.with_file_name("outline_and_pours.routes.toml"), routes).unwrap();
    }
    let made = fab_with_options(
        "outline_and_pours.nsm",
        SOURCE,
        &netsmith_check::Loader::std().with_physical(
            "outline_and_pours.nsm",
            "outline_and_pours.physical.nsm",
            PHYSICAL,
        ),
        &dir.join("fab"),
        &Catalogs::jlcpcb(),
        &Libraries::standard(),
        None,
        &FabOptions {
            layout: Some(layout),
            glb: false,
            step: false,
            pdf: false,
            ..FabOptions::default()
        },
    )
    .unwrap_or_else(|e| panic!("{e}"));
    (made[0].dir.clone(), made[0].notes.clone())
}

/// Each zone's fill the board file in `out` holds is the fill of the Gerbers, by net and layer.
fn assert_held_is_the_gerbers(out: &std::path::Path, held: &BTreeMap<(String, String), Held>) {
    let zip = fs::File::open(out.join(format!("{BOARD}-gerbers.zip"))).unwrap();
    let mut zip = zip::ZipArchive::new(zip).unwrap();
    for ((net, layer), h) in held {
        assert_eq!(h.zones, 1, "{net} on {layer}");
        let mut gbr = String::new();
        zip.by_name(&format!("gerber/{BOARD}-{}.gbr", layer.replace('.', "_")))
            .unwrap()
            .read_to_string(&mut gbr)
            .unwrap();
        let fill = gerber_fill(&gbr);
        let (pieces, area) = fill[net.as_str()];
        // The points are written to 0.1 µm, so each ring is at most that far off the Gerber's
        // (which has them to 1 nm), and the area at most its perimeter times that.
        let tolerance = h.perimeter * 1e-4 + 1e-6;
        eprintln!(
            "{net} on {layer}: {} pieces, {:.4} mm² held, {area:.4} mm² in the Gerber, tolerance {tolerance:.4} mm²",
            h.pieces, h.area
        );
        assert_eq!(h.pieces, pieces, "{net} on {layer}: the pieces");
        assert!(
            (h.area - area).abs() <= tolerance,
            "{net} on {layer}: {} mm² held, {area} mm² in the Gerber",
            h.area
        );
    }
}

#[test]
fn the_kicad_board_holds_the_fill_of_the_gerbers_in_each_zone() {
    let dir = TempDir::new("poured-kicad-board");
    let (out, notes) = fab_example(&dir, None);
    let pcb = fs::read_to_string(out.join(format!("{BOARD}.kicad_pcb"))).unwrap();
    let held = held(&pcb);
    // The two inner planes and the two outer pours, each one zone of its layer.
    let keys: Vec<(&str, &str)> = held
        .keys()
        .map(|(n, l)| (n.as_str(), l.as_str()))
        .collect();
    assert_eq!(
        keys,
        [
            ("3V3", "In2.Cu"),
            ("GND", "B.Cu"),
            ("GND", "F.Cu"),
            ("GND", "In1.Cu")
        ]
    );
    for ((net, layer), h) in &held {
        assert!(h.area > 100.0, "{net} on {layer}: {} mm²", h.area);
    }
    assert_held_is_the_gerbers(&out, &held);
    // netsmith reads the fill of the file it wrote as its own and makes it again, so the
    // fill notes still say it filled every zone itself.
    assert!(
        notes.contains("zone fill: netsmith filled GND on In1.Cu, 3V3 on In2.Cu, GND on F.Cu, GND on B.Cu itself"),
        "{notes}"
    );
}

/// A hand zone of 3V3 on the bottom, over the pad J2.1 of its net, in the bottom's GND pour.
const ROUTES_3V3_ZONE: &str = r#"version = 1

[OutlineAndPours.nets.3V3]
pads = "x"

[[OutlineAndPours.nets.3V3.zones]]
layers = ["bottom"]
priority = 0
clearance = 0.3
min_width = 0.25
pads = "thermal"
thermal_gap = 0.5
spoke_width = 0.5
islands = "remove"
outline = [[25.0, 29.0], [30.0, 29.0], [30.0, 32.5], [25.0, 32.5]]
lock = true
"#;

/// The priority a zone of the board file is written with (none is 0).
fn priorities(pcb: &str) -> BTreeMap<(String, String), u32> {
    let root = Sexp::parse(pcb).unwrap();
    let text = |node: &Sexp, key: &str| -> String {
        node.find(key)
            .and_then(|n| n.items().get(1)?.text())
            .unwrap_or_default()
            .to_string()
    };
    root.find_all("zone")
        .filter(|z| z.find("keepout").is_none() && z.find("attr").is_none())
        .map(|z| {
            let p = text(z, "priority").parse().unwrap_or(0);
            ((text(z, "net_name"), text(z, "layer")), p)
        })
        .collect()
}

#[test]
fn a_hand_zone_of_another_net_cuts_the_pour_and_kicads_refill_agrees() {
    // Issue #1314: a hand zone of 3V3 on the bottom left the bottom's GND pour out of the
    // board. The pour stays, written under the zone, and is filled round it.
    let dir = TempDir::new("poured-kicad-board-zone");
    let (out, notes) = fab_example(&dir, Some(ROUTES_3V3_ZONE));
    let pcb = fs::read_to_string(out.join(format!("{BOARD}.kicad_pcb"))).unwrap();
    let held = held(&pcb);
    let keys: Vec<(&str, &str)> = held
        .keys()
        .map(|(n, l)| (n.as_str(), l.as_str()))
        .collect();
    assert_eq!(
        keys,
        [
            ("3V3", "B.Cu"),
            ("3V3", "In2.Cu"),
            ("GND", "B.Cu"),
            ("GND", "F.Cu"),
            ("GND", "In1.Cu")
        ]
    );
    let priorities = priorities(&pcb);
    let key = |n: &str, l: &str| (n.to_string(), l.to_string());
    assert_eq!(priorities[&key("GND", "B.Cu")], 0, "{priorities:?}");
    assert_eq!(priorities[&key("3V3", "B.Cu")], 1, "{priorities:?}");
    // The pour has all of the bottom but the zone and its clearance (the bottom is 1663 mm² of
    // pour with no zone); the zone has its own 17.5 mm² less its pad's relief.
    let pour = held[&key("GND", "B.Cu")].area;
    let zone = held[&key("3V3", "B.Cu")].area;
    assert!(pour > 1600.0 && pour < 1660.0, "the GND pour: {pour} mm²");
    assert!(zone > 10.0 && zone < 17.5, "the 3V3 zone: {zone} mm²");
    assert_held_is_the_gerbers(&out, &held);
    assert!(
        notes.contains("B.Cu is a pour on GND, cut by the routes file's zones"),
        "{notes}"
    );
    // KiCad's own filler, where KiCad is installed, fills the board file's zones alike.
    let Some(cli) = netsmith_fab::KicadCliExe::locate_runnable() else {
        eprintln!("poured_kicad_board: KiCad is not installed; netsmith's fill alone is checked");
        return;
    };
    let path = out.join(format!("{BOARD}.kicad_pcb"));
    let refilled = self::held(&cli.refilled(&path).unwrap_or_else(|e| panic!("{e}")));
    for (k, theirs) in &refilled {
        let ours = &held[k];
        eprintln!("{k:?}: {:.3} mm² netsmith's, {:.3} mm² KiCad's", ours.area, theirs.area);
        // KiCad cuts curves into other straight pieces, and keeps 2.5 µm less: 1 % covers it.
        // Inside the thermal relief of a pad of the zone's own net, under the pad's copper, the
        // two polygons differ by up to 1 mm² (the pour's two pads as much as the zone's one), so
        // a small zone is held to that.
        assert!(
            (ours.area - theirs.area).abs() <= (ours.area * 0.01).max(1.0),
            "{k:?}: {} mm² netsmith's, {} mm² KiCad's",
            ours.area,
            theirs.area
        );
    }
    assert_eq!(
        refilled.keys().collect::<Vec<_>>(),
        held.keys().collect::<Vec<_>>()
    );
}
