//! The Excellon drill files: `<board>-PTH.drl` for plated holes (vias and pad holes) and
//! `<board>-NPTH.drl` for the rest, in millimetres with decimal coordinates, as KiCad writes
//! them and JLCPCB took them in earlier orders. Slots are routed (`G00`, `M15`, `G01`,
//! `M16`) with a tool as wide as the slot. The origin is the board's drill/place origin and
//! y points up, the same as the Gerbers.

use crate::Output;
use crate::artwork::{Artwork, PadKind};
use crate::geom::Pt;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub struct Hole {
    pub at: Pt,
    pub diameter: f64,
    /// For a slot, where the other end of the routed path is.
    pub slot_to: Option<Pt>,
    pub plated: bool,
    /// A via's hole rather than a pad's.
    pub via: bool,
    /// The index of the pad or via (in `Artwork::pads` or `Artwork::vias`) it is the hole of.
    pub index: usize,
}

/// Every hole of the board.
pub fn holes(art: &Artwork) -> Vec<Hole> {
    let mut out = Vec::new();
    for (index, pad) in art.pads.iter().enumerate() {
        let Some(d) = &pad.drill else { continue };
        let centre = pad.at + d.offset.rotate(pad.angle);
        let (diameter, slot_to, at) = if d.is_slot() {
            // The slot lies along the longer side of its drill, in the pad's frame.
            let (long, short) = (d.width.max(d.length), d.width.min(d.length));
            let half = (long - short) / 2.0;
            let axis = if d.width >= d.length {
                Pt::new(half, 0.0)
            } else {
                Pt::new(0.0, half)
            }
            .rotate(pad.angle);
            (short, Some(centre + axis), centre - axis)
        } else {
            (d.width, None, centre)
        };
        out.push(Hole {
            at,
            diameter,
            slot_to,
            plated: pad.kind != PadKind::Npth,
            via: false,
            index,
        });
    }
    for (index, via) in art.vias.iter().enumerate() {
        out.push(Hole {
            at: via.at,
            diameter: via.drill,
            slot_to: None,
            plated: true,
            via: true,
            index,
        });
    }
    out
}

/// A coordinate in millimetres with up to six decimals, at least one, as the file writes it.
fn coord(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0');
    let s = if s.ends_with('.') {
        format!("{s}0")
    } else {
        s.to_string()
    };
    if s == "-0.0" { "0.0".to_string() } else { s }
}

/// The two drill files of a board: plated first.
pub fn files(art: &Artwork, name: &str) -> Vec<Output> {
    let all = holes(art);
    let layers = art.copper.len().max(2);
    [(true, "PTH"), (false, "NPTH")]
        .into_iter()
        .map(|(plated, kind)| Output {
            name: format!("{name}-{kind}.drl"),
            bytes: file(&all, art.origin, plated, layers).into_bytes(),
        })
        .collect()
}

fn file(all: &[Hole], origin: Pt, plated: bool, layers: usize) -> String {
    let mut holes: Vec<&Hole> = all.iter().filter(|h| h.plated == plated).collect();
    // Tools: by diameter (to a micrometre), vias before pad holes of the same size.
    let tool_key = |h: &Hole| ((h.diameter * 1000.0).round() as i64, !h.via);
    let mut tools: Vec<(i64, bool)> = holes.iter().map(|h| tool_key(h)).collect();
    tools.sort_unstable();
    tools.dedup();
    holes.sort_by(|a, b| {
        let at = |h: &Hole| (h.at.x - origin.x, h.at.y - origin.y);
        at(a)
            .0
            .total_cmp(&at(b).0)
            .then(at(a).1.total_cmp(&at(b).1))
    });

    let (function, kind) = if plated {
        ("Plated", "PTH")
    } else {
        ("NonPlated", "NPTH")
    };
    let version = env!("CARGO_PKG_VERSION");
    let mut o = String::new();
    let _ = writeln!(o, "M48");
    let _ = writeln!(o, "; DRILL file {{netsmith {version}}}");
    let _ = writeln!(o, "; FORMAT={{-:-/ absolute / metric / decimal}}");
    let _ = writeln!(o, "; #@! TF.GenerationSoftware,netsmith,netsmith,{version}");
    let _ = writeln!(o, "; #@! TF.FileFunction,{function},1,{layers},{kind}");
    let _ = writeln!(o, "FMAT,2");
    let _ = writeln!(o, "METRIC");
    for (i, (um, not_via)) in tools.iter().enumerate() {
        let what = if *not_via {
            "ComponentDrill"
        } else {
            "ViaDrill"
        };
        let _ = writeln!(o, "; #@! TA.AperFunction,{function},{kind},{what}");
        let _ = writeln!(o, "T{}C{:.3}", i + 1, *um as f64 / 1000.0);
    }
    let _ = writeln!(o, "%");
    let _ = writeln!(o, "G90");
    let _ = writeln!(o, "G05");
    let at = |p: Pt| format!("X{}Y{}", coord(p.x - origin.x), coord(-(p.y - origin.y)));
    for (i, key) in tools.iter().enumerate() {
        let _ = writeln!(o, "T{}", i + 1);
        for h in holes.iter().filter(|h| tool_key(h) == *key) {
            match h.slot_to {
                None => {
                    let _ = writeln!(o, "{}", at(h.at));
                }
                Some(to) => {
                    let _ = writeln!(o, "G00{}", at(h.at));
                    let _ = writeln!(o, "M15");
                    let _ = writeln!(o, "G01{}", at(to));
                    let _ = writeln!(o, "M16");
                    let _ = writeln!(o, "G05");
                }
            }
        }
    }
    let _ = writeln!(o, "M30");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = r#"(kicad_pcb
        (layers (0 "F.Cu" signal) (4 "In1.Cu" signal) (6 "In2.Cu" signal) (2 "B.Cu" signal))
        (setup (aux_axis_origin 100 200))
        (footprint "L:X" (layer "F.Cu") (at 110 210)
          (property "Reference" "J1")
          (pad "1" thru_hole rect (at 0 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask"))
          (pad "2" thru_hole oval (at 0 2.54 90) (size 3 1.5) (drill oval 2 1) (layers "*.Cu" "*.Mask"))
          (pad "" np_thru_hole circle (at 5 0) (size 3.2 3.2) (drill 3.2) (layers "*.Cu" "*.Mask")))
        (via (at 105 205) (size 0.45) (drill 0.15) (layers "F.Cu" "B.Cu") (net 0))
      )"#;

    #[test]
    fn plated_and_unplated_holes_go_to_separate_files_in_the_files_frame() {
        let art = Artwork::parse(BOARD).unwrap();
        let files = files(&art, "B");
        assert_eq!(files[0].name, "B-PTH.drl");
        assert_eq!(files[1].name, "B-NPTH.drl");
        let pth = String::from_utf8(files[0].bytes.clone()).unwrap();
        // Tools by size: the via's 0.15 first, then the pad hole 1.0 and the slot 1.0 wide.
        assert!(
            pth.contains("; #@! TF.FileFunction,Plated,1,4,PTH\n"),
            "{pth}"
        );
        assert!(
            pth.contains("; #@! TA.AperFunction,Plated,PTH,ViaDrill\nT1C0.150\n"),
            "{pth}"
        );
        assert!(
            pth.contains("; #@! TA.AperFunction,Plated,PTH,ComponentDrill\nT2C1.000\n"),
            "{pth}"
        );
        // The via at (105, 205) is at (5, -5) from the origin, y up.
        assert!(pth.contains("T1\nX5.0Y-5.0\n"), "{pth}");
        // The round hole at (110, 210) and the slot, whose long axis is turned to vertical by
        // the pad's 90 degrees: from (110, 212.54 + 0.5) to (110, 212.54 - 0.5).
        assert!(pth.contains("T2\nX10.0Y-10.0\n"), "{pth}");
        assert!(
            pth.contains("G00X10.0Y-13.04\nM15\nG01X10.0Y-12.04\nM16\nG05\n"),
            "{pth}"
        );
        assert!(pth.ends_with("M30\n"));
        let npth = String::from_utf8(files[1].bytes.clone()).unwrap();
        assert!(
            npth.contains("TF.FileFunction,NonPlated,1,4,NPTH"),
            "{npth}"
        );
        assert!(
            npth.contains("T1C3.200\n%\nG90\nG05\nT1\nX15.0Y-10.0\nM30\n"),
            "{npth}"
        );
    }

    #[test]
    fn a_board_without_unplated_holes_still_gets_the_file() {
        let art = Artwork::parse(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (via (at 1 1) (size 0.45) (drill 0.15) (layers "F.Cu" "B.Cu")))"#,
        )
        .unwrap();
        let npth = String::from_utf8(files(&art, "B")[1].bytes.clone()).unwrap();
        assert!(
            npth.starts_with("M48\n") && npth.contains("METRIC\n%\nG90\nG05\nM30\n"),
            "{npth}"
        );
    }

    #[test]
    fn coordinates_are_short_and_never_negative_zero() {
        assert_eq!(coord(24.29), "24.29");
        assert_eq!(coord(-0.0), "0.0");
        assert_eq!(coord(-25.5), "-25.5");
        assert_eq!(coord(30.0), "30.0");
        assert_eq!(coord(1.234_567_8), "1.234568");
    }
}
