//! The footprints drawn from a maker's drawing (`std/libs/netsmith.pretty/`, ADR 0087 and
//! ADR 0097 step 3) against KiCad's own footprint of the same part, where KiCad is installed.
//!
//! A maker's drawing and KiCad's footprint are two readings of one part, so they may differ,
//! and the difference is reported, not hidden. For each footprint [`PAIRS`] says which KiCad
//! footprint it is compared with, the most that its pads may differ (the centre of a pad and
//! each side of it, in mm) and why. The two origins are not the same, so a footprint is moved
//! before it is compared: both are centred on the middle of their `S1` pads (the shell holes of
//! a receptacle), or on the middle of all their pads if there is no shell. Pad numbers, kinds,
//! shapes' boxes and holes must be the same pad for pad. The courtyard is a drawing convention
//! (ours is the copper and the body and 0.25 mm; KiCad's is its own), so its difference is
//! printed and bounded, and is not a finding about the land pattern. The test is skipped when
//! KiCad's footprint libraries are not installed. Each entry's bounds must still be reached by
//! some pad, so the table cannot go stale.
//!
//! `cargo test -p netsmith-pcb --test main maker_vs_kicad:: -- --nocapture` prints the table.

use netsmith_pcb::{Footprint, FootprintLibrary, Pad, Pt};

/// Where two footprints that agree on a pad are said to agree: the float noise of a value
/// written 0.01 apart.
const AGREE: f64 = 0.0100001;

struct Pair {
    ours: &'static str,
    kicad: &'static str,
    /// The most any pad's centre may differ, in mm, after the two are centred.
    centre: f64,
    /// The most any side of a pad or of its hole may differ, in mm.
    size: f64,
    /// The most any edge of the courtyard may differ, in mm.
    courtyard: f64,
    why: &'static str,
}

const PAIRS: &[Pair] = &[
    Pair {
        ours: "netsmith:USB_C_Receptacle_GCT_USB4125_6P_TopMnt_Horizontal",
        kicad: "Connector_USB:USB_C_Receptacle_GCT_USB4125-xx-x_6P_TopMnt_Horizontal",
        centre: 0.0,
        size: 0.0,
        courtyard: 0.30,
        why: "pads, shell holes and their sizes agree with KiCad's; only the courtyard differs \
              (KiCad's has its own outline of the body, ours the copper and the drawing's body, \
              and 0.25)",
    },
    Pair {
        ours: "netsmith:USB_C_Receptacle_GCT_USB4105_16P_TopMnt_Horizontal",
        kicad: "Connector_USB:USB_C_Receptacle_GCT_USB4105-xx-A_16P_TopMnt_Horizontal",
        centre: 0.0,
        size: 0.0,
        courtyard: 0.30,
        why: "pads, shell holes, positioning holes and their sizes agree with KiCad's; only the \
              courtyard differs",
    },
    Pair {
        ours: "netsmith:USB_C_Receptacle_HRO_TYPE-C-31-M-12",
        kicad: "Connector_USB:USB_C_Receptacle_HRO_TYPE-C-31-M-12",
        centre: 0.35,
        size: 0.35,
        courtyard: 0.80,
        why: "KiCad's footprint cites an earlier drawing of the part (krhro.com, in a folder dated 2018-03-20), \
              not the Rev A of 2020-12-08 read here. The shell holes are at the same 4.18. KiCad's \
              signal pads are 1.45 long where the drawing's are 1.14 (1.64 less 0.50): they reach \
              0.50 farther back and 0.19 farther forward; its corner and inner pads are 0.05 \
              farther from the centre (3.25 and 2.45; the drawing's 6.40 and 4.80 give 3.20 and \
              2.40); its shell pads are 1.0 wide (ours 0.90) and its lower ones 1.6 long with a \
              1.2 drill (ours 1.70 and 1.40); its positioning holes are 0.65 (ours 0.60)",
    },
    Pair {
        ours: "netsmith:SW_SPST_Omron_B3U-1000P",
        kicad: "Button_Switch_SMD:SW_SPST_B3U-1000P",
        centre: 3.4,
        size: 0.10,
        courtyard: 0.20,
        why: "the numbers are mirrored: the drawing's terminal arrangement has terminal 2 on \
              the left and 1 on the right, KiCad's pad 1 is on the left (the switch has no \
              polarity; compared by number the centres are 3.4 apart, by place they agree). \
              KiCad's pads are 0.9 wide, the drawing's 4.2 over the outer edges and 2.6 between \
              the inner ones make them 0.8",
    },
    Pair {
        ours: "netsmith:SW_SPST_CK_RS-282G05A3-SM",
        kicad: "Button_Switch_SMD:SW_SPST_CK_RS282G05A3",
        centre: 0.0,
        size: 0.0,
        courtyard: 0.0,
        why: "the lands (1.5 x 1.5 at 7.8 between centres) and the courtyard agree",
    },
    Pair {
        ours: "netsmith:SMA_Amphenol_132134_Vertical",
        kicad: "Connector_Coaxial:SMA_Amphenol_132134_Vertical",
        centre: 0.0,
        size: 0.05,
        courtyard: 0.25,
        why: "the holes (one 1.5, four 1.7 on a 5.08 square) and their numbers agree with \
              KiCad's; the pads differ because the drawing gives none and ours are derived \
              (hole + 2 x 0.05 + 0.50, IPC-2221 land rule, density level B): 2.10 and 2.30 \
              against KiCad's 2.05 and 2.25, 0.05 larger each, and the courtyard is the copper \
              and 0.25 (KiCad's is its own, 0.23 farther out)",
    },
    Pair {
        ours: "netsmith:Tag-Connect_TC2030-IDC-NL_2x03_P1.27mm_Vertical",
        kicad: "Connector:Tag-Connect_TC2030-IDC-NL_2x03_P1.27mm_Vertical",
        centre: 0.0,
        size: 0.0,
        courtyard: 0.25,
        why: "pads, holes, numbers and positions agree with KiCad's; the drawing's 0.787 and \
              0.991 are the inch labels .0310 and .0390 rounded, which KiCad has as 0.7874 and \
              0.9906 (0.0004 apart); only the courtyard differs (ours is the copper and 0.25)",
    },
];

struct Report {
    pads: usize,
    centre: f64,
    size: f64,
    courtyard: f64,
    structural: Option<String>,
}

/// The middle of the `S1` pads, or of every pad if there are none.
fn middle(fp: &Footprint) -> Pt {
    let shell: Vec<&Pad> = fp.pads.iter().filter(|p| p.number == "S1").collect();
    let pads: Vec<&Pad> = if shell.is_empty() {
        fp.pads.iter().collect()
    } else {
        shell
    };
    let n = pads.len() as f64;
    Pt::new(
        pads.iter().map(|p| p.at.x).sum::<f64>() / n,
        pads.iter().map(|p| p.at.y).sum::<f64>() / n,
    )
}

fn compare(ours: &Footprint, kicad: &Footprint) -> Report {
    let mut report = Report {
        pads: ours.pads.len(),
        centre: 0.0,
        size: 0.0,
        courtyard: 0.0,
        structural: None,
    };
    if ours.pads.len() != kicad.pads.len() {
        report.structural = Some(format!(
            "{} pads, KiCad has {}",
            ours.pads.len(),
            kicad.pads.len()
        ));
        return report;
    }
    let (mine, theirs) = (middle(ours), middle(kicad));
    let at = |p: &Pad, m: Pt| Pt::new(p.at.x - m.x, p.at.y - m.y);
    let mut unmatched: Vec<&Pad> = kicad.pads.iter().collect();
    for pad in &ours.pads {
        let here = at(pad, mine);
        let candidate = unmatched
            .iter()
            .enumerate()
            .filter(|(_, k)| k.number == pad.number)
            .min_by(|(_, a), (_, b)| {
                at(a, theirs)
                    .distance(here)
                    .partial_cmp(&at(b, theirs).distance(here))
                    .unwrap()
            });
        let Some((index, _)) = candidate else {
            report.structural = Some(format!("pad `{}` has no counterpart", pad.number));
            return report;
        };
        let k = unmatched.remove(index);
        if pad.kind != k.kind || pad.drill.is_some() != k.drill.is_some() {
            report.structural = Some(format!(
                "pad `{}` is {:?} with {:?}, KiCad's {:?} with {:?}",
                pad.number, pad.kind, pad.drill, k.kind, k.drill
            ));
            return report;
        }
        let there = at(k, theirs);
        report.centre = report
            .centre
            .max((here.x - there.x).abs())
            .max((here.y - there.y).abs());
        let mut sides = vec![(pad.width - k.width).abs(), (pad.height - k.height).abs()];
        if let (Some(a), Some(b)) = (pad.drill, k.drill) {
            sides.push((a.width - b.width).abs());
            sides.push((a.height - b.height).abs());
        }
        for d in sides {
            report.size = report.size.max(d);
        }
    }
    let (a, b) = (ours.courtyard, kicad.courtyard);
    // The courtyards are compared in their own frames, moved the same way.
    for d in [
        ((a.min.x - mine.x) - (b.min.x - theirs.x)).abs(),
        ((a.min.y - mine.y) - (b.min.y - theirs.y)).abs(),
        ((a.max.x - mine.x) - (b.max.x - theirs.x)).abs(),
        ((a.max.y - mine.y) - (b.max.y - theirs.y)).abs(),
    ] {
        report.courtyard = report.courtyard.max(d);
    }
    report
}

#[test]
fn every_footprint_drawn_from_a_makers_drawing_is_compared_with_kicads_and_each_difference_is_recorded()
 {
    // The table names every maker-drawn footprint of the library, so that a new one is compared.
    let named: Vec<&str> = PAIRS
        .iter()
        .map(|p| p.ours.strip_prefix("netsmith:").unwrap())
        .collect();
    for name in netsmith_kicad::stdlib::footprint_names() {
        // The LT8641's has no KiCad footprint to compare with (ADR 0087).
        if name.starts_with("UDC-QFN-18") {
            continue;
        }
        assert!(
            named.contains(&name),
            "{name} is in std/libs/netsmith.pretty but not in PAIRS of maker_vs_kicad.rs"
        );
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
    let std = FootprintLibrary::at("/nonexistent");

    println!("| footprint | pads | centre (mm) | size (mm) | courtyard (mm) | |");
    println!("|---|---|---|---|---|---|");
    let mut problems = Vec::new();
    for pair in PAIRS {
        let ours = std.load(pair.ours).unwrap();
        let theirs = kicad
            .load(pair.kicad)
            .unwrap_or_else(|e| panic!("KiCad has no {}: {e}", pair.kicad));
        let r = compare(&ours, &theirs);
        println!(
            "| {} | {} | {:.3} | {:.3} | {:.3} | {} |",
            pair.ours,
            r.pads,
            r.centre,
            r.size,
            r.courtyard,
            r.structural.clone().unwrap_or_else(|| pair.why.to_string())
        );
        if let Some(s) = r.structural {
            problems.push(format!("{}: {s}", pair.ours));
            continue;
        }
        // At most what is recorded; a recorded difference that no pad reaches is stale, and
        // one that is recorded as zero must stay within the agreement asked for everywhere.
        for (what, got, bound) in [
            ("centre", r.centre, pair.centre),
            ("size", r.size, pair.size),
            ("courtyard", r.courtyard, pair.courtyard),
        ] {
            if got > bound.max(0.0) + AGREE {
                problems.push(format!(
                    "{}: {what} differs by {got:.3} mm, more than the {bound} mm recorded ({})",
                    pair.ours, pair.why
                ));
            }
            if bound > 0.0 && got + 0.1 < bound {
                problems.push(format!(
                    "{}: {what} differs by only {got:.3} mm, the {bound} mm recorded is too loose",
                    pair.ours
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
