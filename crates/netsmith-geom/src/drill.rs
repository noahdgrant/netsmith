//! The Excellon drill files: `<board>-PTH.drl` for plated holes (vias and pad holes) and
//! `<board>-NPTH.drl` for the rest, and one more for each span of a blind or buried via
//! (issue #1015), named by its layers as KiCad names it, `<board>-front-in1.drl`,
//! `<board>-in1-in4.drl`, `<board>-in4-back.drl`, its function `Plated,1,2,Blind` or
//! `Plated,2,5,Buried`; in millimetres with decimal coordinates, as KiCad writes
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
    /// A blind or a buried via's first and last layer, as places in `Artwork::copper`; `None`
    /// for a hole through the board.
    pub span: Option<(usize, usize)>,
}

/// Every hole of the board.
pub fn holes(art: &Artwork) -> Vec<Hole> {
    let mut out = Vec::new();
    for (index, pad) in art.pads.iter().enumerate() {
        let (Some(d), Some(centre)) = (&pad.drill, pad.hole_at()) else {
            continue;
        };
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
            span: None,
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
            span: via.span,
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

/// The drill files of a board: plated through holes first, then the holes through no copper,
/// then a file for each span of a blind or buried via, from the top.
pub fn files(art: &Artwork, name: &str) -> Vec<Output> {
    let all = holes(art);
    let layers = art.copper.len().max(2);
    let mut out: Vec<Output> = [(true, "PTH"), (false, "NPTH")]
        .into_iter()
        .map(|(plated, kind)| {
            let holes = all
                .iter()
                .filter(|h| h.plated == plated && h.span.is_none())
                .collect();
            let function = if plated { "Plated" } else { "NonPlated" };
            Output {
                name: format!("{name}-{kind}.drl"),
                bytes: file(holes, art.origin, function, &format!("1,{layers},{kind}"), kind)
                    .into_bytes(),
            }
        })
        .collect();
    let mut spans: Vec<(usize, usize)> = all.iter().filter_map(|h| h.span).collect();
    spans.sort_unstable();
    spans.dedup();
    // KiCad's word for a layer in a drill file's name: `front`, `back`, `in1`...
    let word = |p: usize| match p {
        0 => "front".to_string(),
        p if p + 1 == layers => "back".to_string(),
        p => format!("in{p}"),
    };
    for (a, b) in spans {
        let kind = if a == 0 || b + 1 == layers {
            "Blind"
        } else {
            "Buried"
        };
        let holes = all.iter().filter(|h| h.span == Some((a, b))).collect();
        out.push(Output {
            name: format!("{name}-{}-{}.drl", word(a), word(b)),
            bytes: file(
                holes,
                art.origin,
                "Plated",
                &format!("{},{},{kind}", a + 1, b + 1),
                "PTH",
            )
            .into_bytes(),
        });
    }
    out
}

/// One drill file of `holes`: `function` is `Plated` or `NonPlated`, `span` the rest of its file
/// function (`1,4,PTH`, `1,2,Blind`), `kind` the word of its tools' function (`PTH`, `NPTH`).
fn file(mut holes: Vec<&Hole>, origin: Pt, function: &str, span: &str, kind: &str) -> String {
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

    let version = env!("CARGO_PKG_VERSION");
    let mut o = String::new();
    let _ = writeln!(o, "M48");
    let _ = writeln!(o, "; DRILL file {{netsmith {version}}}");
    let _ = writeln!(o, "; FORMAT={{-:-/ absolute / metric / decimal}}");
    let _ = writeln!(o, "; #@! TF.GenerationSoftware,netsmith,netsmith,{version}");
    let _ = writeln!(o, "; #@! TF.FileFunction,{function},{span}");
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

    #[test]
    fn each_span_of_a_blind_or_buried_via_has_a_drill_file_of_its_own() {
        // Issue #1015: KiCad's names and file functions for the spans.
        let art = Artwork::parse(&BOARD.replace(
            "(via (at 105 205)",
            "(via micro (at 106 205) (size 0.25) (drill 0.1) (layers \"F.Cu\" \"In1.Cu\") (net 0))\n        (via blind (at 107 205) (size 0.45) (drill 0.2) (layers \"In1.Cu\" \"In2.Cu\") (net 0))\n        (via (at 105 205)",
        ))
        .unwrap();
        let files = files(&art, "B");
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["B-PTH.drl", "B-NPTH.drl", "B-front-in1.drl", "B-in1-in2.drl"]);
        let text = |k: usize| String::from_utf8(files[k].bytes.clone()).unwrap();
        // The through via stays in the plated file, and each span's hole is in its own.
        assert!(text(0).contains("T1\nX5.0Y-5.0\n") && !text(0).contains("X6.0"), "{}", text(0));
        assert!(text(2).contains("; #@! TF.FileFunction,Plated,1,2,Blind\n"), "{}", text(2));
        assert!(text(2).contains("T1C0.100\n") && text(2).contains("X6.0Y-5.0\n"), "{}", text(2));
        assert!(text(3).contains("; #@! TF.FileFunction,Plated,2,3,Buried\n"), "{}", text(3));
        assert!(text(3).contains("X7.0Y-5.0\n"), "{}", text(3));
        // A board with through vias only has its two files, as before.
        assert_eq!(files_of_plain(), 2);
    }

    fn files_of_plain() -> usize {
        files(&Artwork::parse(BOARD).unwrap(), "B").len()
    }
}
