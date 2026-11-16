//! `stackup` (issue #993): a board file whose layers the process does not make. netsmith's rule:
//! KiCad has none, and the cross-check counts its findings and does not compare them. It is
//! judged only when the caller says what the process makes ([`super::Extras::stackup`]; `fab`
//! does), and each finding is an error, since the board house cannot make the board as the file
//! states it. Three things are judged:
//!
//! - **The copper layers.** The file's copper layers (its `layers` list) are as many as the
//!   process's.
//! - **The thickness.** The board's thickness (`(general (thickness ...))`) is one the process
//!   makes.
//! - **The stackup adds up.** The layers of the setup's stackup (copper, dielectric and mask)
//!   add up to the board's thickness within the fabricator's tolerance of the thickness
//!   (`netsmith_pcb::thickness_tolerance`, from JLCPCB's capabilities page), and its copper
//!   layers are the file's. A file with no stackup is not judged on it.
//!
//! The numbers are the process's, so a board netsmith writes, whose file is written from its
//! process, meets them: the rule catches a board file that was changed after it was written,
//! or one checked against a process it was not made for.

use super::Rule;
use super::checks::Out;
use super::model::Model;
use crate::geom::Pt;

/// How near two thicknesses are to be one, in mm: a file's numbers are rounded to the nanometre.
const SAME: f64 = 1e-6;

pub(crate) fn run(out: &mut Out<'_>) {
    let m = out.m;
    let Some(want) = &m.extras.stackup else {
        return;
    };
    if m.rules.severity(Rule::Stackup).is_none() {
        return;
    }
    let at = if m.edge.is_empty() {
        Pt::new(0.0, 0.0)
    } else {
        let b = Model::bounds_of(&m.edge.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>());
        Pt::new(b.min.x, b.min.y)
    };
    let art = m.art;
    let mut push = |message: String| out.push(Rule::Stackup, message, at, None, Vec::new());
    let copper = art.copper.len();
    if copper != want.layers {
        push(format!(
            "the board file has {copper} copper layer(s); the process makes boards of {}",
            want.layers
        ));
    }
    let (lo, hi) = want.thickness;
    let range = if (hi - lo).abs() <= SAME {
        format!("{} mm", mm(lo))
    } else {
        format!("{} mm to {} mm", mm(lo), mm(hi))
    };
    let Some(thickness) = art.thickness else {
        push(format!(
            "the board file states no thickness; the process makes {range}"
        ));
        return;
    };
    if thickness < lo - SAME || thickness > hi + SAME {
        push(format!(
            "the board is {} mm thick; the process makes {range}",
            mm(thickness)
        ));
    }
    if art.stackup.is_empty() {
        return;
    }
    let stacked = art.stackup.iter().filter(|l| l.kind == "copper").count();
    if stacked != copper {
        push(format!(
            "the board file's stackup has {stacked} copper layer(s) and its layer list {copper}"
        ));
    }
    let total: f64 = art.stackup.iter().map(|l| l.thickness).sum();
    if (total - thickness).abs() > want.tolerance + SAME {
        push(format!(
            "the layers of the board file's stackup add up to {} mm, {} mm from the board's {} mm; the fabricator makes a board within {} mm of its thickness",
            mm(total),
            mm((total - thickness).abs()),
            mm(thickness),
            mm(want.tolerance)
        ));
    }
}

/// A length in mm as a person writes it: at most four decimals, no trailing zeros.
fn mm(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::super::{Extras, Rule, Severity, Stackup, check};

    /// A two-layer board 1.6 mm thick whose stackup is `stack` (layers as `(name type thickness)`).
    fn board(general: &str, layers: &str, stack: &[(&str, &str, f64)]) -> String {
        let stack: String = stack
            .iter()
            .map(|(name, kind, t)| format!(r#"(layer "{name}" (type "{kind}") (thickness {t}))"#))
            .collect();
        format!(
            r#"(kicad_pcb (general {general}) (layers {layers})
            (setup (stackup {stack}))
            (net 0 "")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts")))"#
        )
    }

    const TWO: &str = r#"(0 "F.Cu" signal) (2 "B.Cu" signal)"#;

    fn two_layer(core: f64) -> Vec<(&'static str, &'static str, f64)> {
        vec![
            ("F.Mask", "Top Solder Mask", 0.01),
            ("F.Cu", "copper", 0.035),
            ("dielectric 1", "core", core),
            ("B.Cu", "copper", 0.035),
            ("B.Mask", "Bottom Solder Mask", 0.01),
        ]
    }

    fn process(layers: usize, lo: f64, hi: f64) -> Extras {
        Extras {
            stackup: Some(Stackup {
                layers,
                thickness: (lo, hi),
                tolerance: 0.16,
            }),
            ..Extras::default()
        }
    }

    fn found(pcb: &str, extras: &Extras) -> Vec<String> {
        check(pcb, None, extras)
            .unwrap()
            .findings
            .iter()
            .filter(|f| f.rule == Rule::Stackup)
            .map(|f| {
                assert_eq!(f.severity, Severity::Error);
                f.message.clone()
            })
            .collect()
    }

    #[test]
    fn a_board_the_process_makes_has_no_stackup_finding() {
        let pcb = board("(thickness 1.6)", TWO, &two_layer(1.51));
        assert_eq!(found(&pcb, &process(2, 0.4, 2.0)), Vec::<String>::new());
        assert_eq!(found(&pcb, &process(2, 1.6, 1.6)), Vec::<String>::new());
    }

    #[test]
    fn without_a_process_the_stackup_is_not_judged() {
        let pcb = board("(thickness 7)", TWO, &two_layer(0.1));
        assert_eq!(found(&pcb, &Extras::default()), Vec::<String>::new());
    }

    #[test]
    fn copper_layers_the_process_does_not_make_are_a_stackup_error() {
        let pcb = board("(thickness 1.6)", TWO, &two_layer(1.51));
        assert_eq!(
            found(&pcb, &process(4, 1.6, 1.6)),
            ["the board file has 2 copper layer(s); the process makes boards of 4"]
        );
    }

    #[test]
    fn a_thickness_the_process_does_not_make_is_a_stackup_error() {
        let pcb = board("(thickness 2.4)", TWO, &two_layer(2.31));
        assert_eq!(
            found(&pcb, &process(2, 0.4, 2.0)),
            ["the board is 2.4 mm thick; the process makes 0.4 mm to 2 mm"]
        );
        let pcb = board("", TWO, &two_layer(1.51));
        assert_eq!(
            found(&pcb, &process(2, 1.6, 1.6)),
            ["the board file states no thickness; the process makes 1.6 mm"]
        );
    }

    #[test]
    fn a_stackup_that_does_not_add_up_is_a_stackup_error() {
        // 0.09 mm of copper and mask round a 1.2 mm core: 1.29 mm, 0.31 mm from 1.6 mm.
        let pcb = board("(thickness 1.6)", TWO, &two_layer(1.2));
        assert_eq!(
            found(&pcb, &process(2, 0.4, 2.0)),
            [
                "the layers of the board file's stackup add up to 1.29 mm, 0.31 mm from the board's 1.6 mm; the fabricator makes a board within 0.16 mm of its thickness"
            ]
        );
        let mut one_copper = two_layer(1.51);
        one_copper.remove(3);
        one_copper[2].2 = 1.545;
        let pcb = board("(thickness 1.6)", TWO, &one_copper);
        assert_eq!(
            found(&pcb, &process(2, 0.4, 2.0)),
            ["the board file's stackup has 1 copper layer(s) and its layer list 2"]
        );
    }

    #[test]
    fn a_project_file_may_ignore_the_stackup_rule() {
        let pcb = board("(thickness 2.4)", TWO, &two_layer(2.31));
        let pro = r#"{"board": {"design_settings": {"rule_severities": {"stackup": "ignore"}}}}"#;
        let report = check(&pcb, Some(pro), &process(2, 0.4, 2.0)).unwrap();
        assert_eq!(report.count(Rule::Stackup), 0);
    }
}
