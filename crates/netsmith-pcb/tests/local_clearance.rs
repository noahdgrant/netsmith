//! A pad's own `(clearance ...)` from its footprint (a stock fiducial has one) is kept by the
//! router as the design rules check applies it: the larger of it and the board's. The part is
//! an invented one, so no KiCad library is needed.

use crate::common;

use netsmith_core::{Design, Features, Net, Outline, Part, PinRef};
use netsmith_fab::drc::{self, Extras};
use netsmith_kicad::testlib::TempDir;
use netsmith_pcb::{Board, FootprintLibrary, Pt, Rect, kicad_pcb, kicad_pro, route};

use common::process;

/// A two-pad part that asks for a clearance from copper of other nets (on its pads, or, said
/// of the footprint as a whole, on each). `thru` makes its pads copper on both faces, so a
/// route cannot go under them.
fn footprint(name: &str, pad_clearance: &str, footprint_clearance: &str, thru: bool) -> String {
    let (attr, kind, layers) = if thru {
        (
            "through_hole",
            "thru_hole rect",
            "(drill 0.3) (layers \"*.Cu\" \"*.Mask\")",
        )
    } else {
        (
            "smd",
            "smd rect",
            "(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")",
        )
    };
    format!(
        "(footprint \"{name}\" (version 20241229) (generator \"netsmith-test\") (layer \"F.Cu\") \
         {footprint_clearance} \
         (property \"Reference\" \"REF**\" (at 0 -1.6 0) (layer \"F.SilkS\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (property \"Value\" \"{name}\" (at 0 1.6 0) (layer \"F.Fab\") (effects (font (size 0.8 0.8) (thickness 0.12)))) \
         (attr {attr}) \
         (pad \"1\" {kind} (at -0.9 0) (size 0.8 1) {layers} {pad_clearance}) \
         (pad \"2\" {kind} (at 0.9 0) (size 0.8 1) {layers} {pad_clearance}) \
         (fp_rect (start -1 -0.5) (end 1 0.5) (stroke (width 0.05) (type solid)) (fill none) (layer \"F.CrtYd\")))\n"
    )
}

/// Three parts in a row: `R1` and `R3` joined by a net that must go round `R2`, whose two
/// pads are on nets of their own (to `R4`). The middle part is the invented one.
fn board(pad: &str, whole: &str) -> (Board, f64) {
    let folder = TempDir::new("local-clearance");
    std::fs::write(
        folder.join("MIDDLE.kicad_mod"),
        footprint("MIDDLE", pad, whole, true),
    )
    .unwrap();
    std::fs::write(
        folder.join("PLAIN.kicad_mod"),
        footprint("PLAIN", "", "", false),
    )
    .unwrap();
    let part = |refdes: &str, package: &str| Part {
        provides: Vec::new(),
        refdes: refdes.into(),
        kind: "Resistor".into(),
        value: "10k".into(),
        package: Some(package.into()),
        path: String::new(),
        pins: Vec::new(),
    };
    let net = |name: &str, a: (&str, &str), b: (&str, &str)| Net {
        name: name.into(),
        pins: vec![
            PinRef {
                refdes: a.0.into(),
                pin: a.1.into(),
            },
            PinRef {
                refdes: b.0.into(),
                pin: b.1.into(),
            },
        ],
    };
    let design = Design {
        interfaces: Vec::new(),
        notes: Vec::new(),
        name: "Local".into(),
        process: Some(process()),
        features: Features {
            outline: Some(Outline {
                width: (20.0, 20.0),
                height: (12.0, 12.0),
                corner_radius: 0.0,
            }),
            ..Features::default()
        },
        parts: vec![
            part("R1", "PLAIN"),
            part("R2", "MIDDLE"),
            part("R3", "PLAIN"),
            part("R4", "PLAIN"),
        ],
        nets: vec![
            net("A", ("R1", "2"), ("R3", "1")),
            net("B", ("R2", "1"), ("R4", "1")),
            net("C", ("R2", "2"), ("R4", "2")),
        ],
    };
    let mapping = netsmith_kicad::Mapping::parse(
        "[[part]]\nkind = \"Resistor\"\npackage = \"MIDDLE\"\nsymbol = \"t:R\"\nfootprint = \"t:MIDDLE\"\n\
         [[part]]\nkind = \"Resistor\"\npackage = \"PLAIN\"\nsymbol = \"t:R\"\nfootprint = \"t:PLAIN\"\n",
        "local",
        std::path::Path::new("."),
    )
    .unwrap();
    let mapping = netsmith_kicad::Mapping::standard().merged(mapping).unwrap();
    let library = FootprintLibrary::at(netsmith_kicad::testlib::sample_footprints())
        .with_library("t", folder.path());
    let mut board = Board::from_design(&design, &mapping, &library).unwrap();
    let at = [(4.0, 6.0), (10.0, 6.0), (16.0, 6.0), (10.0, 10.0)];
    for (p, (x, y)) in board.parts.iter_mut().zip(at) {
        p.at = Pt::new(x, y);
        p.rotation = 0.0;
    }
    board.outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(20.0, 12.0));
    let clearance = board.rules.clearance;
    (board, clearance)
}

/// The least gap between a track of net `A` and the pads of `R2`.
fn least_gap(board: &Board) -> f64 {
    let middle = board.parts.iter().find(|p| p.refdes == "R2").unwrap();
    let pads: Vec<Rect> = middle
        .footprint
        .pads
        .iter()
        .map(|p| middle.pad_bounds(p))
        .collect();
    let a = board.nets.iter().position(|n| n.name == "A").unwrap();
    board
        .tracks
        .iter()
        .filter(|t| t.net == board.nets[a].name)
        .flat_map(|t| {
            let pads = &pads;
            (0..=200).map(move |i| {
                let f = f64::from(i) / 200.0;
                let q = Pt::new(
                    t.from.x + (t.to.x - t.from.x) * f,
                    t.from.y + (t.to.y - t.from.y) * f,
                );
                pads.iter()
                    .map(|r| {
                        let dx = (r.min.x - q.x).max(q.x - r.max.x).max(0.0);
                        let dy = (r.min.y - q.y).max(q.y - r.max.y).max(0.0);
                        dx.hypot(dy) - t.width / 2.0
                    })
                    .fold(f64::MAX, f64::min)
            })
        })
        .fold(f64::MAX, f64::min)
}

fn check(board: &Board) -> drc::Report {
    let extras = Extras {
        mask_web: Some(board.rules.mask_bridge),
        via_in_pad: Some(board.process.via_in_pad),
        excused_courtyards: board.excused_courtyards(),
        hole_to_track: Some(board.rules.hole_clearance),
        plated_hole_clearance: Some(board.rules.plated_hole_clearance),
        pad_to_pad: Some(board.rules.pad_to_pad),
        pad_hole_to_hole: Some(board.rules.hole_to_hole),
        mask_to_track: Some(board.rules.mask_to_track),
        plated_slot: Some(board.rules.plated_slot),
        npth_slot: Some(board.rules.npth_slot),
        npth_drill: Some(board.rules.npth_drill),
        pad_ring: Some(board.rules.min_annular_ring),
        least_widths: board.least_widths(),
        neck_reach: netsmith_pcb::route::NECK_REACH,
        via_counts: board.via_counts(),
        track_angle: true,
    };
    drc::check(&kicad_pcb(board), Some(&kicad_pro(board)), &extras).unwrap()
}

#[test]
fn a_pad_with_a_clearance_of_its_own_keeps_it_from_the_route_and_the_check_passes() {
    // The pads ask for it themselves, or the footprint asks for it of every pad.
    for (pad, whole) in [("(clearance 0.5)", ""), ("", "(clearance 0.5)")] {
        let (mut board, base) = board(pad, whole);
        assert!(base < 0.3, "the board's clearance is {base}");
        let left = route(&mut board);
        assert!(left.is_empty(), "{left:?}");
        let gap = least_gap(&board);
        assert!(gap >= 0.5 - 1e-6, "the route is {gap} mm from the pad");
        let report = check(&board);
        let errors: Vec<_> = report.errors().map(ToString::to_string).collect();
        assert_eq!(errors, Vec::<String>::new());
    }
}
