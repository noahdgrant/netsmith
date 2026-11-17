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
            if laser && (v.drill < want.min_laser_drill - EPS || v.drill > want.max_laser_drill + EPS)
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
            (gaps.iter().sum::<f64>(), want.max_drill_aspect, "a drilled hole's")
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
