//! Every generated footprint against KiCad's own of the same name (ADR 0043).
//!
//! The generators are not trusted, they are checked: for each name they make, the footprint
//! in the installed KiCad libraries must have the same pads (count, numbers, kind, shape,
//! layers) with centres and sizes within 0.01 mm, and the same courtyard within 0.01 mm.
//! Where KiCad's footprint differs by design the name is in [`DIFFERENCES`] with the reason,
//! and the test checks that it still differs, so the table cannot go stale. The test is
//! skipped when KiCad's libraries are not installed.
//!
//! `cargo test -p netsmith-pcb --test main generated_vs_kicad:: -- --nocapture` prints the table.

use netsmith_pcb::{generated_footprint, generated_footprint_names, Footprint, FootprintLibrary, Pad, PadShape};

/// The agreement asked for, in mm. A tolerance of a hair over 0.01 absorbs the float noise of
/// a value written as 0.01 apart.
const TOLERANCE: f64 = 0.0100001;

/// Where KiCad's own footprint differs by design from the IPC-7351B pattern the generator
/// makes, and why: every name that starts with `prefix` may differ by at most `pad` mm in
/// its pads and `courtyard` mm in its courtyard. A difference outside these bounds fails the
/// test, and an entry that no name needs any more fails it too, so the table cannot go stale.
struct Difference {
    prefix: &'static str,
    pad: f64,
    courtyard: f64,
    why: &'static str,
}

const DIFFERENCES: &[Difference] = &[Difference {
    prefix: "Connector_PinSocket_2.54mm:",
    pad: 0.0,
    courtyard: 0.05,
    why: "KiCad draws the courtyard of a 2.54 mm socket 0.04 to 0.05 mm inside the body plus \
          0.5 mm that its header and its 2.0 mm socket have; the pads are the same",
}];

/// Names that KiCad's 9.0.0 library has and its later 9.0 releases do not (the VQFP footprints
/// were dropped in favour of LQFP and TQFP; the library of 9.0.7 and the tag 9.0.9.1 have none).
/// The generators still make them, and they were checked against 9.0.0's. A name here may be
/// missing from the installed library; where it is there it is compared like any other. Any
/// other name without a counterpart fails the test.
const ABSENT_FROM_LATER_LIBRARIES: &[&str] = &[
    "Package_QFP:VQFP-80_14x14mm_P0.65mm",
    "Package_QFP:VQFP-100_14x14mm_P0.5mm",
    "Package_QFP:VQFP-128_14x14mm_P0.4mm",
    "Package_QFP:VQFP-176_20x20mm_P0.4mm",
];

struct Report {
    name: String,
    pads: usize,
    /// The largest deviation of a pad's centre or size, in mm.
    pad_deviation: f64,
    /// The largest deviation of a courtyard edge, in mm.
    courtyard_deviation: f64,
    /// A difference that is not a number: count, numbers, shape, layers.
    structural: Option<String>,
    /// Differences that are not counted.
    notes: Vec<String>,
}

impl Report {
    fn agrees(&self) -> bool {
        self.structural.is_none()
            && self.pad_deviation <= TOLERANCE
            && self.courtyard_deviation <= TOLERANCE
    }
}

fn layer_set(pad: &Pad) -> Vec<String> {
    let mut l = pad.layers.clone();
    l.sort();
    l
}

fn compare(name: &str, ours: &Footprint, kicad: &Footprint) -> Report {
    let mut report = Report {
        name: name.to_string(),
        pads: ours.pads.len(),
        pad_deviation: 0.0,
        courtyard_deviation: 0.0,
        structural: None,
        notes: Vec::new(),
    };
    if ours.pads.len() != kicad.pads.len() {
        report.structural = Some(format!(
            "{} pads, KiCad has {}",
            ours.pads.len(),
            kicad.pads.len()
        ));
        return report;
    }
    let mut unmatched: Vec<&Pad> = kicad.pads.iter().collect();
    for pad in &ours.pads {
        let candidate = unmatched
            .iter()
            .enumerate()
            .filter(|(_, k)| k.number == pad.number && layer_set(k) == layer_set(pad))
            .min_by(|(_, a), (_, b)| {
                a.at.distance(pad.at)
                    .partial_cmp(&b.at.distance(pad.at))
                    .unwrap()
            });
        let Some((index, _)) = candidate else {
            report.structural = Some(format!(
                "pad `{}` on {:?} has no counterpart in KiCad's",
                pad.number, pad.layers
            ));
            return report;
        };
        let k = unmatched.remove(index);
        // Square corners against rounded ones is a matter of style, noted but not counted.
        if pad.shape != k.shape
            && !matches!((pad.shape, k.shape), (PadShape::RoundRect, PadShape::Rect))
        {
            report.structural = Some(format!(
                "pad `{}` is {:?}, KiCad's is {:?}",
                pad.number, pad.shape, k.shape
            ));
            return report;
        }
        if pad.shape != k.shape && !report.notes.iter().any(|n| n.contains("square corners")) {
            report
                .notes
                .push("KiCad's pads have square corners".to_string());
        }
        if pad.kind != k.kind {
            report.structural = Some(format!(
                "pad `{}` is {:?}, KiCad's is {:?}",
                pad.number, pad.kind, k.kind
            ));
            return report;
        }
        if let (Some(a), Some(b)) = (pad.drill, k.drill) {
            for d in [(a.width - b.width).abs(), (a.height - b.height).abs()] {
                report.pad_deviation = report.pad_deviation.max(d);
            }
        } else if pad.drill.is_some() != k.drill.is_some() {
            report.structural = Some(format!("pad `{}` differs in having a hole", pad.number));
            return report;
        }
        // A pad turned a quarter with its sides swapped is the same pad: compare the boxes.
        let (ours_box, theirs_box) = (pad.bounds(), k.bounds());
        for d in [
            (ours_box.min.x - theirs_box.min.x).abs(),
            (ours_box.min.y - theirs_box.min.y).abs(),
            (ours_box.max.x - theirs_box.max.x).abs(),
            (ours_box.max.y - theirs_box.max.y).abs(),
        ] {
            report.pad_deviation = report.pad_deviation.max(d);
        }
    }
    for d in [
        (ours.courtyard.min.x - kicad.courtyard.min.x).abs(),
        (ours.courtyard.min.y - kicad.courtyard.min.y).abs(),
        (ours.courtyard.max.x - kicad.courtyard.max.x).abs(),
        (ours.courtyard.max.y - kicad.courtyard.max.y).abs(),
    ] {
        report.courtyard_deviation = report.courtyard_deviation.max(d);
    }
    report
}

#[test]
fn every_generated_footprint_agrees_with_kicads_own_to_ten_microns() {
    let installed = FootprintLibrary::installed();
    if !installed.root().is_dir() {
        eprintln!(
            "skipped: no KiCad footprint libraries at {}",
            installed.root().display()
        );
        return;
    }
    let kicad = FootprintLibrary::at(installed.root());
    let mut reports = Vec::new();
    let mut without_counterpart = Vec::new();
    for id in generated_footprint_names() {
        let ours = generated_footprint(id).expect("a listed name is generated");
        match kicad.load(id) {
            Ok(theirs) => reports.push(compare(id, &ours, &theirs)),
            Err(_) => without_counterpart.push(id.clone()),
        }
    }

    println!("| footprint | pads | pad deviation (mm) | courtyard deviation (mm) | |");
    println!("|---|---|---|---|---|");
    for r in &reports {
        println!(
            "| {} | {} | {:.3} | {:.3} | {} |",
            r.name,
            r.pads,
            r.pad_deviation,
            r.courtyard_deviation,
            r.structural.clone().unwrap_or_else(|| {
                let status = if r.agrees() { "agrees" } else { "differs" };
                match r.notes.as_slice() {
                    [] => status.to_string(),
                    notes => format!("{status}; {}", notes.join("; ")),
                }
            })
        );
    }
    let agreeing = reports.iter().filter(|r| r.agrees()).count();
    println!(
        "{agreeing} of {} generated footprints agree with KiCad's; {} names have no counterpart: {:?}",
        reports.len(),
        without_counterpart.len(),
        without_counterpart
    );

    let mut problems = Vec::new();
    let mut needed = vec![false; DIFFERENCES.len()];
    for r in &reports {
        if r.agrees() {
            continue;
        }
        let known = DIFFERENCES
            .iter()
            .position(|d| r.name.starts_with(d.prefix) && r.structural.is_none());
        match known {
            Some(i) => {
                needed[i] = true;
                let d = &DIFFERENCES[i];
                if r.pad_deviation > d.pad + TOLERANCE || r.courtyard_deviation > d.courtyard + 1e-9
                {
                    problems.push(format!(
                        "{}: pads differ by {:.3} mm and courtyard by {:.3} mm, more than the {} mm and {} mm recorded ({})",
                        r.name, r.pad_deviation, r.courtyard_deviation, d.pad, d.courtyard, d.why
                    ));
                }
            }
            None => problems.push(format!(
                "{}: pads differ by {:.3} mm, courtyard by {:.3} mm{}",
                r.name,
                r.pad_deviation,
                r.courtyard_deviation,
                r.structural
                    .as_ref()
                    .map(|s| format!(" ({s})"))
                    .unwrap_or_default()
            )),
        }
    }
    for (i, d) in DIFFERENCES.iter().enumerate() {
        if !needed[i] {
            problems.push(format!(
                "no footprint under `{}` differs from KiCad's any more; drop it from DIFFERENCES",
                d.prefix
            ));
        }
    }
    without_counterpart.retain(|name| !ABSENT_FROM_LATER_LIBRARIES.contains(&name.as_str()));
    assert!(
        without_counterpart.is_empty(),
        "no KiCad footprint named {without_counterpart:?}"
    );
    assert!(problems.is_empty(), "{problems:#?}");
}

/// The footprints that are one circle round the origin (ADR 0097, step 3): the mounting
/// holes, the test points and the fiducial.
const CIRCULAR: &[&str] = &[
    "MountingHole:MountingHole_3.2mm_M3",
    "MountingHole:MountingHole_3.2mm_M3_Pad",
    "MountingHole:MountingHole_2.2mm_M2",
    "MountingHole:MountingHole_2.2mm_M2_Pad",
    "TestPoint:TestPoint_Pad_D1.0mm",
    "TestPoint:TestPoint_Pad_D1.5mm",
    "TestPoint:TestPoint_Pad_D2.0mm",
    "Fiducial:Fiducial_1mm_Mask2mm",
];

/// The radii of the circles a footprint draws on `layer`, sorted.
fn radii(fp: &Footprint, layer: &str) -> Vec<f64> {
    use netsmith_pcb::{Layer, Shape};
    let layer = match layer {
        "silk" => Layer::Silk,
        "fab" => Layer::Fab,
        _ => Layer::Courtyard,
    };
    let mut all: Vec<f64> = fp
        .graphics
        .iter()
        .filter(|g| g.layer == layer)
        .filter_map(|g| match g.shape {
            Shape::Circle { radius, .. } => Some(radius),
            _ => None,
        })
        .collect();
    all.sort_by(|a, b| a.partial_cmp(b).unwrap());
    all
}

/// What a pad's comparison does not see: the mask margin, the clearance and whether the
/// plating stays on the inner layers, as the file says them (`None`: it says nothing).
fn pad_extras(fp: &Footprint) -> Vec<(String, Option<String>, Option<String>, bool)> {
    let value = |pad: &netsmith_kicad::Sexp, key: &str| {
        pad.find(key)
            .and_then(|n| n.args().first())
            .and_then(|a| a.text())
            .map(str::to_string)
    };
    fp.source
        .find_all("pad")
        .map(|pad| {
            (
                pad.args()
                    .first()
                    .and_then(|a| a.text())
                    .unwrap_or_default()
                    .to_string(),
                value(pad, "solder_mask_margin"),
                value(pad, "clearance"),
                value(pad, "remove_unused_layers").as_deref() == Some("no"),
            )
        })
        .collect()
}

/// The words of the `(attr ...)`, sorted.
fn attr_words(fp: &Footprint) -> Vec<String> {
    let mut words: Vec<String> = fp
        .source
        .find("attr")
        .map(|a| {
            a.args()
                .iter()
                .filter_map(|w| w.text().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    words.sort();
    words
}

#[test]
fn the_holes_test_points_and_fiducial_are_generated_and_agree_with_kicads_in_every_circle() {
    for id in CIRCULAR {
        assert!(generated_footprint_names().contains(&id.to_string()), "{id} is not generated");
    }
    let installed = FootprintLibrary::installed();
    if !installed.root().is_dir() {
        eprintln!(
            "skipped: no KiCad footprint libraries at {}",
            installed.root().display()
        );
        return;
    }
    let kicad = FootprintLibrary::at(installed.root());
    let mut problems = Vec::new();
    for id in CIRCULAR {
        let ours = generated_footprint(id).unwrap();
        let theirs = kicad
            .load(id)
            .unwrap_or_else(|e| panic!("KiCad has no {id}: {e}"));
        let report = compare(id, &ours, &theirs);
        if !report.agrees() {
            problems.push(format!(
                "{id}: pads differ by {:.3} mm, courtyard by {:.3} mm{}",
                report.pad_deviation,
                report.courtyard_deviation,
                report
                    .structural
                    .map(|s| format!(" ({s})"))
                    .unwrap_or_default()
            ));
        }
        for layer in ["court", "silk", "fab"] {
            let (a, b) = (radii(&ours, layer), radii(&theirs, layer));
            let same = a.len() == b.len() && a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-6);
            if !same {
                problems.push(format!("{id}: circles on {layer} are {a:?}, KiCad's {b:?}"));
            }
        }
        if pad_extras(&ours) != pad_extras(&theirs) {
            problems.push(format!(
                "{id}: pad extras {:?}, KiCad's {:?}",
                pad_extras(&ours),
                pad_extras(&theirs)
            ));
        }
        if attr_words(&ours) != attr_words(&theirs) {
            problems.push(format!(
                "{id}: attr {:?}, KiCad's {:?}",
                attr_words(&ours),
                attr_words(&theirs)
            ));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
