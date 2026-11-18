//! The process's rules as KiCad custom rules (issue #1185): the `.kicad_dru` that `fab` writes
//! beside the board's `.kicad_pro`, so that KiCad's check judges the board to the numbers
//! netsmith's does, and the reader of that file, so that netsmith's check of a KiCad board
//! (`board drc --kicad-pcb`, the cross-check) judges it to them too.
//!
//! The project file holds one least clearance, one hole clearance, one hole to hole and one
//! annular width for every item. A process states more: JLCPCB's surface-mount pads keep
//! 0.15 mm from pads of other nets while copper keeps 0.1 mm, a track keeps 0.2 mm from a via's
//! hole and 0.28 mm from a plated pad's, two holes keep 0.45 mm when one is a pad's, a
//! non-plated hole and a slot have least sizes of their own, a plated pad keeps more copper
//! round its hole than a via, and a silkscreen text keeps 0.15 mm from an opening in the mask
//! (the project file's silkscreen clearance is between silkscreen and to the edge, and KiCad's
//! least from silkscreen to the mask is none; issue #1186). Each of those is a rule of the file, named
//! `netsmith <rule>` after the rule of netsmith's check that holds it ([`super::Rule::id`]), so that a finding KiCad
//! makes of it says the rule (`(rule 'netsmith pad_to_pad' clearance 0.1500 mm; actual
//! 0.1448 mm)`) and the cross-check compares it with netsmith's (`netsmith_fab::parse_kicad_drc`
//! takes the rule from it).
//!
//! A rule is written only where the process asks more than the project file already does: a
//! custom rule replaces the project's number for the items it matches, so a smaller one would
//! let KiCad pass what netsmith refuses. For the same reason a net class that keeps more than
//! the space between surface-mount pads gets a rule of its own after the pads' rule (in KiCad,
//! the last rule that matches wins, and a custom rule wins over a net class's clearance; measured
//! with KiCad 9.0.0), ordered by its clearance so that the larger of two classes wins, as KiCad
//! takes the larger clearance of two nets.
//!
//! **What a KiCad rule cannot say,** and so stays netsmith's alone: the space from a solder mask
//! opening to a track (`mask_to_track`; KiCad's `solder_mask_bridge` names a track inside an
//! opening only), the widest via a pad may hold (`via_in_pad`), the width a net's current or
//! impedance needs away from its pads (`track_current`, `track_impedance`: KiCad's `track_width`
//! constraint has no neck-down by a pad), the vias side by side a current needs (`via_current`),
//! the bends (`track_angle`, `acid_trap`), the fiducials and test points, the stackup and the
//! blind and buried vias (`via_span`, `via_aspect_ratio`). A teardrop is a zone in KiCad's file,
//! which the hole rule to tracks does not match, so `hole_to_track` of a teardrop is netsmith's
//! alone too (the router draws none that breaks it).

use super::{Extras, Rules};

/// What starts the name of each rule the file holds.
pub const PREFIX: &str = "netsmith ";

/// The condition that matches two surface-mount pads (KiCad's `connect` pads, an edge
/// connector's, among them, as netsmith's `pad_to_pad` takes them).
const SMD_PADS: &str = "A.Type == 'Pad' && B.Type == 'Pad' \
    && (A.Pad_Type == 'SMD' || A.Pad_Type == 'Edge connector') \
    && (B.Pad_Type == 'SMD' || B.Pad_Type == 'Edge connector')";

/// One rule of the file: its name after [`PREFIX`], KiCad's constraint, the least, and the
/// condition.
struct Written {
    name: String,
    constraint: &'static str,
    least: f64,
    condition: String,
}

/// The text of the `.kicad_dru` for a board whose project file holds `project` and whose
/// process states the rules of `process` that are set (`pad_to_pad`, `hole_to_track`,
/// `plated_hole_clearance`, `pad_hole_to_hole`, `npth_drill`, `npth_slot`, `plated_slot`,
/// `pad_ring`, `text_to_mask`; the other fields are not read). A file with no rule holds the
/// version alone.
pub fn kicad_dru(project: &Rules, process: &Extras) -> String {
    let mut rules: Vec<Written> = Vec::new();
    // Only where the process asks more than the project file says already.
    let more = |least: Option<f64>, than: f64| least.filter(|l| *l > than + 1e-9);
    let tracks = "(A.Type == 'Track' || A.Type == 'Arc')";
    if let Some(least) = more(
        process.pad_to_pad,
        project.clearance.max(project.default_clearance),
    ) {
        rules.push(Written {
            name: "pad_to_pad".into(),
            constraint: "clearance",
            least,
            condition: SMD_PADS.into(),
        });
        let mut classes: Vec<(&str, f64)> = project
            .classes
            .iter()
            .filter_map(|c| Some((c.name.as_str(), c.clearance?)))
            .filter(|(_, c)| *c > least + 1e-9)
            .collect();
        classes.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(b.0)));
        for (class, clearance) in classes {
            rules.push(Written {
                name: format!("clearance {class}"),
                constraint: "clearance",
                least: clearance,
                condition: format!(
                    "{SMD_PADS} && (A.hasNetclass('{class}') || B.hasNetclass('{class}'))"
                ),
            });
        }
    }
    if let Some(least) = more(process.hole_to_track, project.hole_clearance) {
        rules.push(Written {
            name: "hole_to_track".into(),
            constraint: "hole_clearance",
            least,
            condition: format!("{tracks} && (B.Type == 'Via' || B.Pad_Type == 'NPTH, mechanical')"),
        });
    }
    if let Some(least) = more(process.plated_hole_clearance, project.hole_clearance) {
        rules.push(Written {
            name: "hole_to_track plated".into(),
            constraint: "hole_clearance",
            least,
            condition: format!("{tracks} && B.Pad_Type == 'Through-hole'"),
        });
    }
    if let Some(least) = more(process.pad_hole_to_hole, project.hole_to_hole) {
        rules.push(Written {
            name: "pad_hole_to_hole".into(),
            constraint: "hole_to_hole",
            least,
            condition: "A.Type == 'Pad' || B.Type == 'Pad'".into(),
        });
    }
    let holes = [
        (process.npth_drill, "npth_drill", "NPTH, mechanical", "=="),
        (
            process.npth_slot,
            "slot_width npth",
            "NPTH, mechanical",
            "!=",
        ),
        (
            process.plated_slot,
            "slot_width plated",
            "Through-hole",
            "!=",
        ),
    ];
    for (least, name, kind, round) in holes {
        if let Some(least) = more(least, project.through_hole) {
            rules.push(Written {
                name: name.into(),
                constraint: "hole_size",
                least,
                condition: format!(
                    "A.Type == 'Pad' && A.Pad_Type == '{kind}' && A.Hole_Size_X {round} A.Hole_Size_Y"
                ),
            });
        }
    }
    if let Some(least) = more(process.pad_ring, project.annular_width) {
        rules.push(Written {
            name: "plated_ring".into(),
            constraint: "annular_width",
            least,
            condition: "A.Type == 'Pad'".into(),
        });
    }
    // The project file has no space from silkscreen to the mask (KiCad's is 0, the ink touching
    // an opening): the process's space from a text to a pad reaches KiCad's mask test alone, for
    // a text (KiCad's `Text` is a board's text, a designator and a footprint's text). KiCad
    // checks the rule on the mask layers in two tests: silkscreen against the mask's openings
    // (B is no item there, so a test of B is false), and silkscreen against the pads and vias on
    // the mask layer, which would report a pad or a via a second time, as `silk_overlap`; a pad
    // or a via as B leaves that test out, in either order (measured with KiCad 9, issue #1186).
    if let Some(least) = more(process.text_to_mask, 0.0) {
        rules.push(Written {
            name: "silk_over_copper".into(),
            constraint: "silk_clearance",
            least,
            condition: "(L == 'F.Mask' || L == 'B.Mask') && A.Type == 'Text' \
                && !(B.Type == 'Pad' || B.Type == 'Via')"
                .into(),
        });
    }
    let mut out = String::from(
        "(version 1)\n\
         # The rules of the board's manufacturing process that the project file cannot hold, written\n\
         # by netsmith (issue #1185). Each is named for the rule of netsmith's design rules check\n\
         # that holds it; netsmith writes this file again on every fab.\n",
    );
    for r in rules {
        out.push_str(&format!(
            "\n(rule \"{PREFIX}{}\"\n\t(constraint {} (min {}mm))\n\t(condition \"{}\"))\n",
            r.name,
            r.constraint,
            mm(r.least),
            r.condition
        ));
    }
    out
}

/// A length in mm as the file writes it: no trailing zeros, to the micrometre.
fn mm(v: f64) -> String {
    let s = format!("{:.4}", (v * 1e4).round() / 1e4);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

impl Extras {
    /// These extras with the rules of a `.kicad_dru` that [`kicad_dru`] wrote set where they are
    /// not set already: the rules named `netsmith <rule>`, by their least. Another rule of the
    /// file (one a person wrote, or a net class's) is not read, nor is a file that is not
    /// KiCad's rules language.
    pub fn with_rules_file(mut self, text: &str) -> Extras {
        for (name, least) in netsmith_rules(text) {
            let slot = match name.as_str() {
                "pad_to_pad" => &mut self.pad_to_pad,
                "hole_to_track" => &mut self.hole_to_track,
                "hole_to_track plated" => &mut self.plated_hole_clearance,
                "pad_hole_to_hole" => &mut self.pad_hole_to_hole,
                "npth_drill" => &mut self.npth_drill,
                "slot_width npth" => &mut self.npth_slot,
                "slot_width plated" => &mut self.plated_slot,
                "plated_ring" => &mut self.pad_ring,
                "silk_over_copper" => &mut self.text_to_mask,
                _ => continue,
            };
            slot.get_or_insert(least);
        }
        self
    }
}

/// The rules of the file named `netsmith ...`: the name after the prefix and the least in mm.
fn netsmith_rules(text: &str) -> Vec<(String, f64)> {
    // The file is several expressions, and a line that starts with `#` is a comment.
    let body: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let Ok(root) = netsmith_sexp::Sexp::parse(&format!("(file\n{body}\n)")) else {
        return Vec::new();
    };
    root.find_all("rule")
        .filter_map(|rule| {
            let name = rule.args().first()?.text()?.strip_prefix(PREFIX)?;
            let least = rule
                .find("constraint")?
                .find("min")?
                .args()
                .first()?
                .text()?;
            let mm = match least.strip_suffix("mm") {
                Some(n) => n.parse::<f64>().ok()?,
                None => least.parse::<f64>().ok()?,
            };
            Some((name.to_string(), mm))
        })
        .collect()
}

/// The `.kicad_dru` beside a board file, if there is one.
pub fn rules_file_of(pcb: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(pcb.with_extension("kicad_dru")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jlcpcb() -> Extras {
        Extras {
            pad_to_pad: Some(0.15),
            hole_to_track: Some(0.2),
            plated_hole_clearance: Some(0.28),
            pad_hole_to_hole: Some(0.45),
            npth_drill: Some(0.5),
            npth_slot: Some(1.0),
            plated_slot: Some(0.5),
            pad_ring: Some(0.18),
            text_to_mask: Some(0.15),
            ..Extras::default()
        }
    }

    fn project() -> Rules {
        Rules {
            clearance: 0.1,
            default_clearance: 0.1,
            hole_clearance: 0.1,
            hole_to_hole: 0.2,
            through_hole: 0.2,
            annular_width: 0.13,
            ..Rules::default()
        }
    }

    #[test]
    fn every_rule_the_file_writes_is_read_back_as_it_was() {
        let text = kicad_dru(&project(), &jlcpcb());
        assert_eq!(Extras::default().with_rules_file(&text), jlcpcb());
    }

    #[test]
    fn a_rule_that_asks_no_more_than_the_project_file_is_left_out() {
        let mut p = project();
        p.annular_width = 0.18;
        p.hole_to_hole = 0.45;
        let text = kicad_dru(&p, &jlcpcb());
        assert!(!text.contains("plated_ring"), "{text}");
        assert!(!text.contains("pad_hole_to_hole"), "{text}");
        assert!(text.contains("netsmith pad_to_pad"), "{text}");
        let none = kicad_dru(&project(), &Extras::default());
        assert_eq!(none.lines().next(), Some("(version 1)"));
        assert!(!none.contains("(rule"), "{none}");
    }

    #[test]
    fn a_class_that_keeps_more_than_the_pads_gets_a_rule_after_theirs_the_larger_last() {
        let mut p = project();
        for (name, clearance) in [("Clearance_1mm", 1.0), ("Clearance_0.5mm", 0.5)] {
            p.classes.push(super::super::Class {
                name: name.into(),
                clearance: Some(clearance),
                priority: 0,
                patterns: vec![],
            });
        }
        let text = kicad_dru(&p, &jlcpcb());
        let at = |s: &str| text.find(s).unwrap_or_else(|| panic!("no {s} in {text}"));
        assert!(at("netsmith pad_to_pad") < at("netsmith clearance Clearance_0.5mm"));
        assert!(at("netsmith clearance Clearance_0.5mm") < at("netsmith clearance Clearance_1mm"));
        assert!(text.contains("B.hasNetclass('Clearance_1mm')"), "{text}");
        // Read back, a class's rule is the project file's and changes nothing.
        assert_eq!(Extras::default().with_rules_file(&text), jlcpcb());
    }

    #[test]
    fn a_field_already_set_is_kept_and_a_file_that_is_not_rules_is_ignored() {
        let text = kicad_dru(&project(), &jlcpcb());
        let set = Extras {
            pad_to_pad: Some(0.2),
            ..Extras::default()
        }
        .with_rules_file(&text);
        assert_eq!(set.pad_to_pad, Some(0.2));
        assert_eq!(set.hole_to_track, Some(0.2));
        assert_eq!(Extras::default().with_rules_file("((("), Extras::default());
        assert_eq!(
            Extras::default().with_rules_file(
                "(version 1)\n(rule \"mine\" (constraint clearance (min 0.3mm)) (condition \"A.Type == 'Pad'\"))"
            ),
            Extras::default()
        );
    }
}
