//! `via_span` and `via_aspect_ratio` (issue #1015): the blind and buried vias of an HDI process.
//! netsmith's rules: KiCad has none, and the cross-check counts their findings and does not
//! compare them. They are judged only when the caller says what vias the process makes
//! ([`super::Extras::vias`]; `fab` does), and each finding is an error.
//!
//! - **`via_span`.** A blind or a buried via joins two layers the process joins with one (a
//!   process that is not HDI joins none), a laser-drilled one has a hole the laser drills and a
//!   drilled buried one a hole no larger than the process's, and each has the process's least
//!   copper round its hole.
//! - **`via_aspect_ratio`.** A laser-drilled via is drilled through a dielectric no thicker than
//!   its hole times the process's ratio for a blind via (1 for JLCPCB's 1:1), and a drilled via,
//!   buried or through, through dielectric layers no thicker together than its hole times the
//!   ratio for a drilled hole (16). The depth is the dielectric between its two layers; a
//!   build-up that is not known is not judged.

use super::Rule;
use super::checks::{EPS, Out};

pub(crate) fn run(out: &mut Out<'_>) {
    let m = out.m;
    let Some(want) = &m.extras.vias else {
        return;
    };
    let art = m.art;
    let last = art.copper.len().saturating_sub(1);
    let name = |p: usize| art.copper.get(p).cloned().unwrap_or_default();
    for (vi, v) in art.vias.iter().enumerate() {
        let item = m.items[m.via_item[vi]].item.clone();
        let (a, b) = v.span.unwrap_or((0, last));
        let laser = match v.span {
            None => false,
            Some(span) => match want.spans.iter().find(|s| (s.0, s.1) == span) {
                Some(s) => s.2,
                None => {
                    out.push(
                        Rule::ViaSpan,
                        format!(
                            "a via joins {} and {}, but the process makes no blind or buried via between them",
                            name(a),
                            name(b)
                        ),
                        v.at,
                        None,
                        vec![item],
                    );
                    continue;
                }
            },
        };
        if v.span.is_some() {
            let ring = (v.diameter - v.drill) / 2.0;
            let mut wrong = Vec::new();
            if laser
                && (v.drill < want.min_laser_drill - EPS || v.drill > want.max_laser_drill + EPS)
            {
                wrong.push(format!(
                    "its {:.4} mm laser-drilled hole is not of {} to {} mm",
                    v.drill, want.min_laser_drill, want.max_laser_drill
                ));
            }
            if !laser && want.max_buried_drill > 0.0 && v.drill > want.max_buried_drill + EPS {
                wrong.push(format!(
                    "its {:.4} mm hole is larger than a buried via's {} mm",
                    v.drill, want.max_buried_drill
                ));
            }
            if ring < want.min_ring - EPS {
                wrong.push(format!(
                    "it has {ring:.4} mm of copper round its hole, the least is {} mm",
                    want.min_ring
                ));
            }
            if !wrong.is_empty() {
                out.push(
                    Rule::ViaSpan,
                    format!(
                        "the {} via from {} to {}: {}",
                        if laser { "blind" } else { "buried" },
                        name(a),
                        name(b),
                        wrong.join("; ")
                    ),
                    v.at,
                    None,
                    vec![item.clone()],
                );
            }
        }
        if want.dielectrics.len() != last || v.drill <= 0.0 {
            continue;
        }
        let gaps = &want.dielectrics[a..b];
        let (depth, ratio, what) = if laser {
            let thickest = gaps.iter().copied().fold(0.0, f64::max);
            (thickest, want.max_laser_aspect, "a blind via's")
        } else {
            (
                gaps.iter().sum::<f64>(),
                want.max_drill_aspect,
                "a drilled hole's",
            )
        };
        if ratio > 0.0 && depth > v.drill * ratio + EPS {
            out.push(
                Rule::ViaAspectRatio,
                format!(
                    "the via from {} to {} has a {:.4} mm hole through {:.4} mm of dielectric; {what} depth is at most {ratio} times its hole",
                    name(a),
                    name(b),
                    v.drill,
                    depth
                ),
                v.at,
                None,
                vec![item],
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Extras, Rule, ViaRules, check};

    /// A board of four copper layers with `vias`, each `(kind, layers, drill, diameter, x)`:
    /// KiCad's via type (`micro`, `blind` or none), its two layers, its hole and its copper.
    /// A via of [`board`]: KiCad's type, its two layers, its hole, its copper and its x.
    type TestVia<'a> = (&'a str, (&'a str, &'a str), f64, f64, f64);

    fn board(vias: &[TestVia]) -> String {
        let vias: String = vias
            .iter()
            .map(|(kind, (a, b), drill, size, x)| {
                format!(
                    "(via {kind} (at {x} 5) (size {size}) (drill {drill}) (layers \"{a}\" \"{b}\") (net 1))\n"
                )
            })
            .collect();
        format!(
            "(kicad_pcb (layers (0 \"F.Cu\" signal) (4 \"In1.Cu\" signal) (6 \"In2.Cu\" signal) (2 \"B.Cu\" signal))\n(net 0 \"\") (net 1 \"A\")\n{vias})"
        )
    }

    /// JLCPCB's first-order HDI rules on four layers: blind vias from the top and the bottom to
    /// the inner layer beside each, a buried via through the core, and a build-up of 0.086 mm
    /// prepregs round a 0.3 mm core.
    fn hdi() -> Extras {
        Extras {
            vias: Some(ViaRules {
                spans: vec![(0, 1, true), (2, 3, true), (1, 2, false)],
                dielectrics: vec![0.086, 0.3, 0.086],
                min_laser_drill: 0.075,
                max_laser_drill: 0.15,
                min_ring: 0.075,
                max_buried_drill: 0.55,
                max_laser_aspect: 1.0,
                max_drill_aspect: 16.0,
            }),
            ..Extras::default()
        }
    }

    fn found(pcb: &str, extras: &Extras) -> Vec<(Rule, String)> {
        check(pcb, None, extras)
            .unwrap()
            .findings
            .into_iter()
            .filter(|f| matches!(f.rule, Rule::ViaSpan | Rule::ViaAspectRatio))
            .map(|f| (f.rule, f.message))
            .collect()
    }

    #[test]
    fn the_vias_an_hdi_process_makes_pass() {
        let pcb = board(&[
            ("micro", ("F.Cu", "In1.Cu"), 0.1, 0.25, 5.0),
            ("micro", ("In2.Cu", "B.Cu"), 0.1, 0.25, 7.0),
            ("blind", ("In1.Cu", "In2.Cu"), 0.2, 0.45, 9.0),
            ("", ("F.Cu", "B.Cu"), 0.2, 0.45, 11.0),
        ]);
        assert_eq!(found(&pcb, &hdi()), []);
        // Unjudged without the process's vias.
        assert_eq!(found(&pcb, &Extras::default()), []);
    }

    #[test]
    fn a_span_the_process_does_not_make_and_a_wrong_hole_are_via_span_errors() {
        let pcb = board(&[
            ("blind", ("F.Cu", "In2.Cu"), 0.2, 0.45, 5.0),
            ("micro", ("F.Cu", "In1.Cu"), 0.2, 0.3, 7.0),
        ]);
        let got = found(&pcb, &hdi());
        assert_eq!(
            got,
            [
                (
                    Rule::ViaSpan,
                    "a via joins F.Cu and In2.Cu, but the process makes no blind or buried via between them".to_string()
                ),
                (
                    Rule::ViaSpan,
                    "the blind via from F.Cu to In1.Cu: its 0.2000 mm laser-drilled hole is not of 0.075 to 0.15 mm; it has 0.0500 mm of copper round its hole, the least is 0.075 mm".to_string()
                ),
            ]
        );
        // A process that makes no blind via refuses every one.
        let none = Extras {
            vias: Some(ViaRules::default()),
            ..Extras::default()
        };
        let pcb = board(&[("micro", ("F.Cu", "In1.Cu"), 0.1, 0.25, 5.0)]);
        assert_eq!(found(&pcb, &none).len(), 1);
    }

    #[test]
    fn a_via_deeper_than_its_hole_allows_is_a_via_aspect_ratio_error() {
        // 0.086 mm of prepreg over a 0.075 mm laser hole is deeper than 1:1.
        let pcb = board(&[("micro", ("F.Cu", "In1.Cu"), 0.075, 0.25, 5.0)]);
        assert_eq!(
            found(&pcb, &hdi()),
            [(
                Rule::ViaAspectRatio,
                "the via from F.Cu to In1.Cu has a 0.0750 mm hole through 0.0860 mm of dielectric; a blind via's depth is at most 1 times its hole".to_string()
            )]
        );
        // A drilled through via is held to 16:1: 0.472 mm of dielectric over a 0.025 mm hole.
        let pcb = board(&[("", ("F.Cu", "B.Cu"), 0.025, 0.25, 5.0)]);
        let got = found(&pcb, &hdi());
        assert_eq!(got.len(), 1);
        assert!(got[0].1.contains("16 times"), "{got:?}");
    }

    #[test]
    fn a_blind_via_in_a_pad_is_filled_by_the_hdi_process_and_a_drilled_one_is_not() {
        // Issue #1071: a board that takes no drilled via in a pad (`.via_in_pad &= 0mm`) on an
        // HDI process, with a micro via and a through via each in a pad of their net.
        let pad = |x: f64| {
            format!(
                r#"(footprint "L:R" (layer "F.Cu") (at {x} 5) (property "Reference" "R{x}") (pad "1" smd rect (at 0 0) (size 0.4 0.4) (layers "F.Cu") (net 1 "A")))"#
            )
        };
        let pcb = |kind: &str, drill: f64, size: f64| {
            board(&[(
                kind,
                ("F.Cu", if kind.is_empty() { "B.Cu" } else { "In1.Cu" }),
                drill,
                size,
                5.0,
            )])
            .replace("(net 1))", &format!("(net 1)) {}", pad(5.0)))
        };
        let extras = Extras {
            via_in_pad: Some(0.0),
            ..hdi()
        };
        let in_pad = |pcb: &str| check(pcb, None, &extras).unwrap().count(Rule::ViaInPad);
        assert_eq!(in_pad(&pcb("micro", 0.1, 0.25)), 0);
        assert_eq!(in_pad(&pcb("", 0.2, 0.35)), 1);
    }

    #[test]
    fn holes_of_blind_vias_on_different_layers_do_not_meet() {
        // A blind via from the top and one from the bottom at one place: no hole to hole, and the
        // bottom's via is no copper on the top.
        let pcb = board(&[
            ("micro", ("F.Cu", "In1.Cu"), 0.1, 0.25, 5.0),
            ("micro", ("In2.Cu", "B.Cu"), 0.1, 0.25, 5.1),
        ]);
        let report = check(&pcb, None, &hdi()).unwrap();
        assert_eq!(report.count(Rule::HoleToHole), 0, "{report}");
        let pcb = board(&[
            ("micro", ("F.Cu", "In1.Cu"), 0.1, 0.25, 5.0),
            ("", ("F.Cu", "B.Cu"), 0.2, 0.45, 5.1),
        ]);
        let report = check(&pcb, None, &hdi()).unwrap();
        assert!(report.count(Rule::HoleToHole) > 0, "{report}");
    }
}
