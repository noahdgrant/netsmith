//! The rules that judge a pour by its fill (issue #838): `starved_thermal`, a pad that fewer
//! than two of its thermal spokes join to the fill, and `isolated_copper`, a piece of the fill
//! that touches no copper of its net and that the zone's island setting keeps. Both are KiCad's
//! names. The fill is the one `fab` plots ([`crate::fill`]), worked out again here; a fill the
//! board file already holds is not judged.
//!
//! **Warnings.** Both are warnings, as `silk_over_copper` is (ADR 0037), until a real order shows
//! the rule is a limit: the fabricator's capabilities page names neither. A project file may say
//! otherwise in its `rule_severities` (KiCad 9 makes `starved_thermal` an error by default).
//!
//! **Two spokes.** A relief is starved when fewer than two of its spokes reach the fill: KiCad
//! 9's default for its `min_resolved_spokes` constraint (the implicit rule its DRC engine adds,
//! `drc_engine.cpp`), which is KiCad's choice and not a fabricator's number. A relief with no
//! spoke on the fill is starved too: the pad is joined to the pour by nothing, which the
//! connection check (which joins every pad in a pour's outline) does not see. A track of the
//! pad's net on the layer that runs from the pad onto the fill counts as a spoke, as it carries
//! the pad's current and heat to the pour as one does. A pad is judged when the zone joins it with
//! thermal spokes and its centre is in the zone's outline.
//!
//! **Islands.** The fill takes out every island unless the zone's `island_removal_mode` keeps
//! them (never, or above `island_area_min`), so a board netsmith writes, whose zones keep the
//! default, has none. A kept island is copper of the net joined to nothing.

use super::checks::Out;
use super::model::Kind;
use super::{Item, Rule};
use crate::fill;
use crate::plot;

pub(crate) fn run(out: &mut Out<'_>) {
    let m = out.m;
    let starved = m.rules.severity(Rule::StarvedThermal).is_some();
    let islands = m.rules.severity(Rule::IsolatedCopper).is_some();
    if !(starved || islands)
        || !m
            .art
            .zones
            .iter()
            .any(|z| z.keepout.is_none() && z.net.is_some())
    {
        return;
    }
    // An outline that is not closed is `invalid_outline`; the pours are then not judged.
    let Ok(edge) = crate::edge::outline(m.art) else {
        return;
    };
    let rules = plot::Rules {
        edge_clearance: m.rules.copper_edge,
        hole_clearance: Some(m.rules.hole_clearance),
        classes: m
            .rules
            .classes
            .iter()
            .filter(|c| c.clearance.is_some())
            .cloned()
            .collect(),
    };
    for layer in &m.layers {
        // Two zones that overlap with the same priority cannot be filled: `fab` says so.
        let Ok(judged) = fill::judge(m.art, layer, &edge, &rules) else {
            continue;
        };
        for j in judged {
            let zone = &m.art.zones[j.zone];
            let net = zone.net.clone().unwrap_or_default();
            let zone_item = |at| Item {
                what: if zone.name.is_empty() {
                    format!("Zone [{net}] on {layer}")
                } else {
                    format!("Zone {} [{net}] on {layer}", zone.name)
                },
                at,
                part: None,
                pad: None,
                net: zone.net.clone(),
            };
            if starved {
                for r in j.reliefs {
                    if r.reach + r.tracks >= MIN_SPOKES {
                        continue;
                    }
                    let Some(item) = m
                        .items
                        .iter()
                        .find(|c| c.kind == Kind::Pad(r.pad))
                        .map(|c| c.item.clone())
                    else {
                        continue;
                    };
                    let at = item.at;
                    let joined = match r.reach {
                        0 => "none".to_string(),
                        n => n.to_string(),
                    };
                    let tracks = match r.tracks {
                        0 => String::new(),
                        1 => " and 1 track".to_string(),
                        n => format!(" and {n} tracks"),
                    };
                    out.push(
                        Rule::StarvedThermal,
                        format!(
                            "thermal relief joins the pour with {joined} of its {} spokes{tracks}; the least is {MIN_SPOKES}",
                            r.spokes
                        ),
                        at,
                        Some(layer),
                        vec![item, zone_item(at)],
                    );
                }
            }
            if islands {
                for (at, area) in j.islands {
                    out.push(
                        Rule::IsolatedCopper,
                        format!(
                            "a piece of the pour of {area:.3} mm² touches no copper of its net"
                        ),
                        at,
                        Some(layer),
                        vec![zone_item(at)],
                    );
                }
            }
        }
    }
}

/// The fewest spokes that join a thermal relief to the fill: KiCad 9's `min_resolved_spokes`.
const MIN_SPOKES: usize = 2;

#[cfg(test)]
mod tests {
    use super::super::{Extras, Report, Rule, Severity, check};

    /// A 20 mm board with a GND pour on F.Cu (gap and spokes 0.5 mm, clearance 0.3 mm), a GND
    /// via in a corner so that the pour touches its net, the fill settings `fill`, and `items`.
    fn board(fill: &str, items: &str) -> Report {
        let pcb = format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "GND") (net 2 "SIG")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3)) (min_thickness 0.25)
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5) {fill})
              (polygon (pts (xy 0.2 0.2) (xy 19.8 0.2) (xy 19.8 19.8) (xy 0.2 19.8))))
            (via (at 2 2) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
            {items})"#
        );
        check(&pcb, None, &Extras::default()).unwrap()
    }

    /// A 1 mm GND pad at (10, 10), whose spokes end 1 mm from its centre.
    const PAD: &str = r#"(footprint "L:R" (layer "F.Cu") (at 10 10)
        (property "Reference" "R1")
        (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net 1 "GND")))"#;

    /// A SIG track that clears the GND pour over the end of one of the pad's spokes: `side` is
    /// right, left, down or up.
    fn cut(side: &str) -> String {
        let (a, b) = match side {
            "right" => ((11.3, 9.0), (11.3, 11.0)),
            "left" => ((8.7, 9.0), (8.7, 11.0)),
            "down" => ((9.0, 11.3), (11.0, 11.3)),
            _ => ((9.0, 8.7), (11.0, 8.7)),
        };
        format!(
            r#"(segment (start {} {}) (end {} {}) (width 0.2) (layer "F.Cu") (net 2))"#,
            a.0, a.1, b.0, b.1
        )
    }

    fn starved(items: &str) -> Vec<String> {
        board("", items)
            .findings
            .iter()
            .filter(|f| f.rule == Rule::StarvedThermal)
            .map(|f| {
                assert_eq!(f.severity, Severity::Warning);
                assert_eq!(f.layer.as_deref(), Some("F.Cu"));
                f.with_items()
            })
            .collect()
    }

    #[test]
    fn a_pad_whose_spokes_reach_the_fill_is_not_starved() {
        assert_eq!(starved(PAD), Vec::<String>::new());
        let two = format!("{PAD} {} {}", cut("right"), cut("left"));
        assert_eq!(starved(&two), Vec::<String>::new());
    }

    #[test]
    fn a_pad_with_one_spoke_on_the_fill_is_a_starved_thermal() {
        let one = format!("{PAD} {} {} {}", cut("right"), cut("left"), cut("down"));
        let found = starved(&one);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with(
                "thermal relief joins the pour with 1 of its 4 spokes; the least is 2: SMD pad 1 [GND] of R1"
            ),
            "{}",
            found[0]
        );
        assert!(found[0].contains("Zone [GND] on F.Cu"), "{}", found[0]);
    }

    #[test]
    fn a_pad_no_spoke_joins_is_a_starved_thermal() {
        let none = format!(
            "{PAD} {} {} {} {}",
            cut("right"),
            cut("left"),
            cut("down"),
            cut("up")
        );
        let found = starved(&none);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("with none of its 4 spokes"),
            "{}",
            found[0]
        );
    }

    /// A GND track from the pad's centre out past its relief's corner, onto the fill.
    const TRACK: &str =
        r#"(segment (start 10 10) (end 13 13) (width 0.25) (layer "F.Cu") (net 1))"#;

    #[test]
    fn a_track_of_the_pads_net_onto_the_fill_joins_it_as_a_spoke_does() {
        let one = format!("{PAD} {} {} {}", cut("right"), cut("left"), cut("down"));
        assert_eq!(starved(&format!("{one} {TRACK}")), Vec::<String>::new());
        let none = format!("{one} {} {TRACK}", cut("up"));
        let found = starved(&none);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("with none of its 4 spokes and 1 track; the least is 2"),
            "{}",
            found[0]
        );
    }

    #[test]
    fn a_pad_joined_solid_or_left_out_of_the_pour_has_no_relief_to_judge() {
        let one = format!("{} {} {}", cut("right"), cut("left"), cut("down"));
        for connect in ["(zone_connect 2)", "(zone_connect 0)"] {
            let pad = PAD.replace("(net 1 \"GND\")", &format!("(net 1 \"GND\") {connect}"));
            assert_eq!(
                starved(&format!("{pad} {one}")),
                Vec::<String>::new(),
                "{connect}"
            );
        }
    }

    /// A square SIG ring from (5, 5) to (15, 15) that cuts a piece of the pour off from the via.
    fn ring() -> String {
        let c = [(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)];
        (0..4)
            .map(|i| {
                let ((x0, y0), (x1, y1)) = (c[i], c[(i + 1) % 4]);
                format!(
                    r#"(segment (start {x0} {y0}) (end {x1} {y1}) (width 0.2) (layer "F.Cu") (net 2))"#
                )
            })
            .collect()
    }

    fn islands(fill: &str) -> Vec<String> {
        board(fill, &ring())
            .findings
            .iter()
            .filter(|f| f.rule == Rule::IsolatedCopper)
            .map(|f| {
                assert_eq!(f.severity, Severity::Warning);
                f.with_items()
            })
            .collect()
    }

    #[test]
    fn an_island_the_zone_keeps_is_isolated_copper() {
        // The fill removes islands unless the zone says otherwise: then there is none to report.
        assert_eq!(islands(""), Vec::<String>::new());
        let kept = islands("(island_removal_mode 1)");
        assert_eq!(kept.len(), 1, "{kept:?}");
        assert!(
            kept[0].starts_with("a piece of the pour of ")
                && kept[0].contains(" mm² touches no copper of its net: Zone [GND] on F.Cu"),
            "{}",
            kept[0]
        );
        // Kept because it is not smaller than the zone's least island (84.6 mm² inside the ring).
        assert_eq!(
            islands("(island_removal_mode 2) (island_area_min 50)").len(),
            1
        );
        assert_eq!(
            islands("(island_removal_mode 2) (island_area_min 100)"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_project_file_can_ignore_the_pour_rules() {
        let pcb_items = format!("{PAD} {} {} {}", cut("right"), cut("left"), cut("down"));
        let pcb = format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "GND") (net 2 "SIG")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3)) (min_thickness 0.25)
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
              (polygon (pts (xy 0.2 0.2) (xy 19.8 0.2) (xy 19.8 19.8) (xy 0.2 19.8))))
            {pcb_items})"#
        );
        let pro =
            r#"{"board": {"design_settings": {"rule_severities": {"starved_thermal": "error"}}}}"#;
        let r = check(&pcb, Some(pro), &Extras::default()).unwrap();
        assert!(r.errors().any(|f| f.rule == Rule::StarvedThermal), "{r}");
        let pro =
            r#"{"board": {"design_settings": {"rule_severities": {"starved_thermal": "ignore"}}}}"#;
        let r = check(&pcb, Some(pro), &Extras::default()).unwrap();
        assert_eq!(r.count(Rule::StarvedThermal), 0, "{r}");
    }
}
