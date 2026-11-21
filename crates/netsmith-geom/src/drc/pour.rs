//! The rules that judge a pour by its fill (issues #838, #993, #1222): `starved_thermal`, a pad
//! that one or more but fewer than two of its thermal spokes join to the fill,
//! `unjoinable_thermal`, a pad that none of them and no track joins to it, `isolated_copper`, a
//! piece of the fill that touches no copper of its net and that the zone's island setting keeps,
//! and `copper_sliver`, copper of the fill narrower than the least track. All but
//! `unjoinable_thermal` are KiCad's names. The fill is the one `fab` plots: the fill the board file holds where a zone holds
//! one (`filled_polygon`, as a board ported from KiCad does), else the fill [`crate::fill`]
//! works out, again here, which makes a spoke only where KiCad's refill makes one (its module
//! header has the rule, issue #1115).
//!
//! **Warnings.** Both are warnings, as `silk_over_copper` is (ADR 0037), until a real order shows
//! the rule is a limit: the fabricator's capabilities page names neither. A project file may say
//! otherwise in its `rule_severities` (KiCad 9 makes `starved_thermal` an error by default).
//!
//! **Two spokes.** A relief is starved when fewer than two of its spokes reach the fill
//! ([`fill::MIN_SPOKES`]: KiCad 9's default for its `min_resolved_spokes` constraint, the implicit
//! rule its DRC engine adds, `drc_engine.cpp`), which is KiCad's choice and not a fabricator's
//! number. A spoke reaches the fill where the fill has copper halfway across the thermal gap on
//! the spoke's axis, as KiCad's check (`drc_test_provider_zone_connections.cpp`) counts the
//! places where the fill crosses the pad grown by half the gap. A track of the pad's net on the
//! layer that runs from the pad onto the fill counts as a spoke, as KiCad counts it, since it
//! carries the pad's current and heat to the pour as one does. A pad is judged when the zone
//! joins it with thermal spokes and its centre is in the zone's outline.
//!
//! **A pad no spoke joins (issue #1222).** A relief with no spoke on the fill and no track is not
//! starved, as KiCad's check skips it too (and reports the pad as unconnected instead, which
//! netsmith's connection check, which joins every pad in a pour's outline, does not see). It is
//! `unjoinable_thermal`, netsmith's own warning, which says why the fill makes none of its
//! spokes (`fill::Unjoinable`: the spoke under the minimum thickness, or what keeps the fill off
//! the end of each, another net's clearance by name, the edge, a keepout, a strip of fill
//! narrower than the minimum thickness): nothing the fill can do, so the pad is joined by its own
//! via or track, which the router keeps for it. With a track and no spoke, neither is reported,
//! nor where another fill of the pad's net joins it (issue #1292: a zone of the net of a higher
//! priority over the pad, which the pour gives its area, or a fill on another layer of a pad with
//! a hole): every layer is judged at once (`fill::judge_board`), by the one function `board fill`
//! reads too. KiCad does not count a spoke onto a piece of the fill that joins
//! nothing but the pad; here every spoke on the fill counts. Of a fill the file holds, a piece of
//! copper across the gap off the spokes' axes is not counted, which KiCad counts.
//!
//! **Islands.** The fill takes out every island unless the zone's `island_removal_mode` keeps
//! them (never, or above `island_area_min`), so a board netsmith writes, whose zones keep the
//! default, has none. A kept island is copper of the net joined to nothing. Of a fill the file
//! holds, every island is reported, whatever the zone's setting: the board house makes that
//! copper as the file draws it.
//!
//! **Copper slivers.** A copper sliver is copper of the fill narrower than the least track the
//! process makes (the project file's `min_track_width`; with none, not judged) that runs further
//! than that width from what trimming the fill to it keeps (`fill::slivers`): a strip between
//! two clearances or a spike. A corner, which trimming only rounds, stays within the width and
//! is not one; neither is a part of the fill that lies against a pad, a track or another zone of
//! the net, since the fill is trimmed together with that copper. The width is the fabricator's
//! (JLCPCB's least track, a recorded number); KiCad's `copper_sliver` measures with an angle and
//! a width of its own (its advanced settings), which no fabricator publishes, so the two are not
//! compared. A fill worked out here is already trimmed to the zone's `min_thickness`, so only a
//! zone with a thinner one is measured; a fill the file holds always is. A warning, as the other
//! two are.

use super::checks::Out;
use super::model::Kind;
use super::{Item, Rule};
use crate::fill::{self, MIN_SPOKES};
use crate::plot;

pub(crate) fn run(out: &mut Out<'_>) {
    let m = out.m;
    let starved = m.rules.severity(Rule::StarvedThermal).is_some();
    let unjoinable = m.rules.severity(Rule::UnjoinableThermal).is_some();
    let islands = m.rules.severity(Rule::IsolatedCopper).is_some();
    let width = m.rules.track_width;
    let slivers = m.rules.severity(Rule::CopperSliver).is_some();
    let cut = m.rules.severity(Rule::PadIslandCut).is_some();
    if !(starved || unjoinable || islands || slivers || cut)
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
    // Every layer at once, so that a pad another fill of its net joins is not unjoinable
    // (`fill::judge_board`, issue #1292).
    for (layer, judged) in
        fill::judge_board(m.art, &m.layers, &edge, &rules, slivers.then_some(width))
    {
        // Two zones that overlap with the same priority cannot be filled: `fab` says so.
        let Ok(judged) = judged else {
            continue;
        };
        if cut {
            cut_pieces(out, layer, &judged, &rules);
        }
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
            for r in j.reliefs {
                let unjoined = r.unjoined();
                if !(starved && fill::starved(r.reach, r.lone, r.tracks) || unjoinable && unjoined)
                {
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
                if unjoined {
                    let why = r
                        .unjoinable
                        .as_ref()
                        .map_or_else(String::new, |w| format!(": {w}"));
                    out.push(
                        Rule::UnjoinableThermal,
                        format!(
                            "thermal relief joins the pour by none of its {} spokes and no track{why}; nothing the fill can do; the pad's own via or track joins it to its net",
                            r.spokes
                        ),
                        at,
                        Some(layer),
                        vec![item, zone_item(at)],
                    );
                    continue;
                }
                let tracks = match r.tracks {
                    0 => String::new(),
                    1 => " and 1 track".to_string(),
                    n => format!(" and {n} tracks"),
                };
                let lone = match r.lone {
                    0 => String::new(),
                    n => format!(" ({n} onto copper of the pour that joins nothing but the pad)"),
                };
                out.push(
                    Rule::StarvedThermal,
                    format!(
                        "thermal relief joins the pour with {} of its {} spokes{lone}{tracks}; the least is {MIN_SPOKES}",
                        r.reach, r.spokes
                    ),
                    at,
                    Some(layer),
                    vec![item, zone_item(at)],
                );
            }
            let whose = if j.held {
                "the fill the board file holds"
            } else {
                "the pour"
            };
            if islands {
                for (at, area) in j.islands {
                    out.push(
                        Rule::IsolatedCopper,
                        format!("a piece of {whose} of {area:.3} mm² touches no copper of its net"),
                        at,
                        Some(layer),
                        vec![zone_item(at)],
                    );
                }
            }
            if slivers {
                for at in j.spikes {
                    out.push(
                        Rule::CopperSliver,
                        format!(
                            "{whose} comes to a point sharper than 20 degrees, a copper sliver as KiCad's check measures it; a fabricator may not etch it or it may lift"
                        ),
                        at,
                        Some(layer),
                        vec![zone_item(at)],
                    );
                }
                for (at, area) in j.slivers {
                    out.push(
                        Rule::CopperSliver,
                        format!(
                            "a copper sliver of {whose} of {area:.4} mm² is narrower than the least track, {width} mm, and runs further than that from the rest of the fill; a fabricator may not etch it or it may lift: widen the gap it is in, or raise the zone's minimum thickness"
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

/// The `pad_island_cut` findings of the fills `judged` of `layer` ([`fill::cuts`], issue
/// #1509): each piece a track of another net cut off from the rest of its fill, with pads of its
/// net on it and no other way to them. A warning, an error when guidelines are strict.
fn cut_pieces(out: &mut Out<'_>, layer: &str, judged: &[fill::Judged], rules: &plot::Rules) {
    let m = out.m;
    for c in fill::cuts(m.art, layer, judged, rules, &m.extras.open) {
        let zone = &m.art.zones[c.zone];
        let net = zone.net.clone().unwrap_or_default();
        let item_of = |kind: Kind| {
            m.items
                .iter()
                .find(|i| i.kind == kind)
                .map(|i| i.item.clone())
        };
        let pads: Vec<Item> = c
            .pads
            .iter()
            .filter_map(|&k| item_of(Kind::Pad(k)))
            .collect();
        let Some(track) = item_of(Kind::Track(c.track)) else {
            continue;
        };
        let names: Vec<String> = c
            .pads
            .iter()
            .map(|&k| {
                let p = &m.art.pads[k];
                format!("{}.{}", p.part, p.number)
            })
            .collect();
        let whose = if zone.filled.iter().any(|(l, _)| l == layer) {
            "the fill the board file holds"
        } else {
            "the pour"
        };
        let other = m.art.tracks[c.track]
            .net
            .clone()
            .unwrap_or_else(|| "no net".to_string());
        let message = format!(
            "a track of {other} cuts a piece of {whose} of {net} ({:.3} mm² at ({:.3}, {:.3})) off from the rest of its fill, and nothing else joins {} on it to the net: no via, plated hole, track or other fill of {net}; move the track, or give the pad a via or a track of its own",
            c.area,
            c.at.x,
            c.at.y,
            names.join(", "),
        );
        let mut items = pads;
        items.push(track);
        items.push(Item {
            what: if zone.name.is_empty() {
                format!("Zone [{net}] on {layer}")
            } else {
                format!("Zone {} [{net}] on {layer}", zone.name)
            },
            at: c.at,
            part: None,
            pad: None,
            net: zone.net.clone(),
        });
        let before = out.findings.len();
        out.push(Rule::PadIslandCut, message, c.near, Some(layer), items);
        if m.extras.strict_guidelines {
            for f in &mut out.findings[before..] {
                f.severity = super::Severity::Error;
            }
        }
    }
}

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
    fn a_pad_no_spoke_joins_is_unjoinable_with_the_reason_and_not_starved() {
        let none = format!(
            "{PAD} {} {} {} {}",
            cut("right"),
            cut("left"),
            cut("down"),
            cut("up")
        );
        assert_eq!(starved(&none), Vec::<String>::new());
        let found: Vec<String> = board("", &none)
            .findings
            .iter()
            .filter(|f| f.rule == Rule::UnjoinableThermal)
            .map(|f| {
                assert_eq!(f.severity, Severity::Warning);
                f.with_items()
            })
            .collect();
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with(
                "thermal relief joins the pour by none of its 4 spokes and no track: the end of each spoke is off the fill: the clearance of track [SIG] (0.3 mm) (4 spokes)"
            ),
            "{}",
            found[0]
        );
        // With a track of the pad's net onto the fill, the pour joins it: neither.
        let joined = board("", &format!("{none} {TRACK}"));
        assert_eq!(joined.count(Rule::UnjoinableThermal), 0, "{joined}");
    }

    /// A solid GND zone of a higher priority over the pad, as a zone of the routes file over a
    /// board's pour is.
    const SOLID_ZONE: &str = r#"(zone (net 1) (net_name "GND") (layer "F.Cu") (priority 1) (connect_pads yes (clearance 0.3)) (min_thickness 0.25)
        (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
        (polygon (pts (xy 7 7) (xy 13 7) (xy 13 13) (xy 7 13))))"#;

    #[test]
    fn a_pad_a_solid_zone_of_its_net_joins_is_not_unjoinable_in_the_pour_under_it() {
        // Issue #1292: the pour gives the higher zone its area, so none of the pour's spokes is
        // on the pour's fill, but the zone's fill joins the pad solid.
        let found = board("", &format!("{PAD} {SOLID_ZONE}"));
        assert_eq!(found.count(Rule::UnjoinableThermal), 0, "{found}");
        assert_eq!(found.count(Rule::StarvedThermal), 0, "{found}");
    }

    /// A GND track from the pad's centre out past its relief's corner, onto the fill.
    const TRACK: &str =
        r#"(segment (start 10 10) (end 13 13) (width 0.25) (layer "F.Cu") (net 1))"#;

    #[test]
    fn a_track_of_the_pads_net_onto_the_fill_joins_it_as_a_spoke_does() {
        let one = format!("{PAD} {} {} {}", cut("right"), cut("left"), cut("down"));
        assert_eq!(starved(&format!("{one} {TRACK}")), Vec::<String>::new());
        // With no spoke, KiCad's check leaves the pad to its connection check, which the track
        // satisfies: no starved thermal relief either.
        let none = format!("{one} {} {TRACK}", cut("up"));
        assert_eq!(starved(&none), Vec::<String>::new());
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

    /// A 20 mm board with a GND zone on F.Cu that holds the fill `fill` (`filled_polygon`s), a
    /// GND via at (2, 2) on it, `items`, and the project file `project`.
    fn held(fill: &[&[(f64, f64)]], items: &str, project: Option<&str>) -> Report {
        let fill: String = fill
            .iter()
            .map(|poly| {
                let pts: String = poly.iter().map(|(x, y)| format!("(xy {x} {y}) ")).collect();
                format!(r#"(filled_polygon (layer "F.Cu") (pts {pts}))"#)
            })
            .collect();
        let pcb = format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "GND") (net 2 "SIG")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3)) (min_thickness 0.25)
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
              (polygon (pts (xy 0.2 0.2) (xy 19.8 0.2) (xy 19.8 19.8) (xy 0.2 19.8)))
              {fill})
            (via (at 2 2) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
            {items})"#
        );
        check(&pcb, project, &Extras::default()).unwrap()
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> [(f64, f64); 4] {
        [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    }

    fn of(r: &Report, rule: Rule) -> Vec<String> {
        r.findings
            .iter()
            .filter(|f| f.rule == rule)
            .map(|f| {
                assert_eq!(f.severity, Severity::Warning);
                f.with_items()
            })
            .collect()
    }

    #[test]
    fn a_fill_the_file_holds_is_judged_for_starved_thermals() {
        // The fill round the pad at (10, 10) leaves a 2 mm square hole (a 0.5 mm gap round the
        // 1 mm pad), crossed by the spokes given.
        let frame = [
            rect(0.5, 0.5, 19.5, 9.0),
            rect(0.5, 11.0, 19.5, 19.5),
            rect(0.5, 9.0, 9.0, 11.0),
            rect(11.0, 9.0, 19.5, 11.0),
        ];
        let right = rect(10.5, 9.75, 11.0, 10.25);
        let left = rect(9.0, 9.75, 9.5, 10.25);
        let with = |spokes: &[[(f64, f64); 4]]| {
            let polys: Vec<&[(f64, f64)]> =
                frame.iter().chain(spokes).map(|r| r.as_slice()).collect();
            of(&held(&polys, PAD, None), Rule::StarvedThermal)
        };
        let one = with(&[right]);
        assert_eq!(one.len(), 1, "{one:?}");
        assert!(
            one[0].starts_with(
                "thermal relief joins the pour with 1 of its 4 spokes; the least is 2: SMD pad 1 [GND] of R1"
            ),
            "{}",
            one[0]
        );
        assert_eq!(with(&[right, left]), Vec::<String>::new());
    }

    #[test]
    fn every_island_of_a_fill_the_file_holds_is_isolated_copper() {
        // The zone removes islands, but the file holds one: it is made as the file says.
        let main = rect(0.5, 0.5, 8.0, 8.0);
        let island = rect(12.0, 12.0, 14.0, 14.0);
        let found = of(&held(&[&main, &island], "", None), Rule::IsolatedCopper);
        assert_eq!(
            found,
            [
                "a piece of the fill the board file holds of 4.000 mm² touches no copper of its net: Zone [GND] on F.Cu"
            ]
        );
        assert_eq!(
            of(&held(&[&main], "", None), Rule::IsolatedCopper),
            Vec::<String>::new()
        );
    }

    /// A project file whose least track is 0.2 mm.
    const LEAST_TRACK: &str =
        r#"{"board": {"design_settings": {"rules": {"min_track_width": 0.2}}}}"#;

    #[test]
    fn a_strip_of_fill_narrower_than_the_least_track_is_a_copper_sliver() {
        // An 8 mm square with a 0.1 mm wide strip 3 mm long off its right side.
        let strip = [
            (1.0, 1.0),
            (9.0, 1.0),
            (9.0, 5.0),
            (12.0, 5.0),
            (12.0, 5.1),
            (9.0, 5.1),
            (9.0, 9.0),
            (1.0, 9.0),
        ];
        let found = of(&held(&[&strip], "", Some(LEAST_TRACK)), Rule::CopperSliver);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("a copper sliver of the fill the board file holds of 0.2")
                && found[0].contains("is narrower than the least track, 0.2 mm")
                && found[0].ends_with(": Zone [GND] on F.Cu"),
            "{}",
            found[0]
        );
        // A strip that lies along other copper of the net is as wide as the two together.
        let along = r#"(segment (start 9 5.05) (end 12 5.05) (width 0.3) (layer "F.Cu") (net 1))"#;
        assert_eq!(
            of(
                &held(&[&strip], along, Some(LEAST_TRACK)),
                Rule::CopperSliver
            ),
            Vec::<String>::new()
        );
        // The square's corners are no sliver, and without a least track nothing is judged.
        let square = rect(1.0, 1.0, 9.0, 9.0);
        assert_eq!(
            of(&held(&[&square], "", Some(LEAST_TRACK)), Rule::CopperSliver),
            Vec::<String>::new()
        );
        assert_eq!(
            of(&held(&[&strip], "", None), Rule::CopperSliver),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_fill_trimmed_to_less_than_the_least_track_can_leave_a_copper_sliver() {
        // Two SIG tracks 0.9 mm apart across the pour: with the 0.3 mm clearance and 0.075 mm
        // of track each side, 0.15 mm of fill runs between them, which a minimum thickness of
        // 0.1 mm keeps.
        let tracks = r#"(segment (start 4 8) (end 16 8) (width 0.15) (layer "F.Cu") (net 2))
            (segment (start 4 8.9) (end 16 8.9) (width 0.15) (layer "F.Cu") (net 2))"#;
        let pcb = |min: f64| {
            format!(
                r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
                (net 0 "") (net 1 "GND") (net 2 "SIG")
                (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
                (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads (clearance 0.3)) (min_thickness {min})
                  (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5) (island_removal_mode 1))
                  (polygon (pts (xy 0.2 0.2) (xy 19.8 0.2) (xy 19.8 19.8) (xy 0.2 19.8))))
                (via (at 2 2) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
                {tracks})"#
            )
        };
        let thin = check(&pcb(0.1), Some(LEAST_TRACK), &Extras::default()).unwrap();
        let found = of(&thin, Rule::CopperSliver);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].starts_with("a copper sliver of the pour of "),
            "{}",
            found[0]
        );
        let thick = check(&pcb(0.25), Some(LEAST_TRACK), &Extras::default()).unwrap();
        assert_eq!(of(&thick, Rule::CopperSliver), Vec::<String>::new());
    }

    /// The board of [`board`] with the SIG ring of [`ring`] round a GND pad of R1 at (10, 10),
    /// a second GND pad (R2) outside the ring, and `items`, checked with `extras`.
    fn cut_board(items: &str, extras: &Extras) -> Report {
        let pcb = format!(
            r#"(kicad_pcb (layers (0 "F.Cu" signal) (2 "B.Cu" signal))
            (net 0 "") (net 1 "GND") (net 2 "SIG")
            (gr_rect (start 0 0) (end 20 20) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
            (zone (net 1) (net_name "GND") (layer "F.Cu") (connect_pads yes (clearance 0.3)) (min_thickness 0.25)
              (fill yes (thermal_gap 0.5) (thermal_bridge_width 0.5))
              (polygon (pts (xy 0.2 0.2) (xy 19.8 0.2) (xy 19.8 19.8) (xy 0.2 19.8))))
            (via (at 2 2) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))
            {PAD}
            (footprint "L:R" (layer "F.Cu") (at 17 17)
              (property "Reference" "R2")
              (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu") (net 1 "GND")))
            {} {items})"#,
            ring()
        );
        check(&pcb, None, extras).unwrap()
    }

    fn cuts(report: &Report) -> Vec<String> {
        report
            .findings
            .iter()
            .filter(|f| f.rule == Rule::PadIslandCut)
            .map(|f| {
                assert_eq!(f.layer.as_deref(), Some("F.Cu"));
                f.with_items()
            })
            .collect()
    }

    #[test]
    fn a_pad_a_track_of_another_net_cut_off_from_its_fill_is_a_pad_island_cut() {
        // Issue #1509: the SIG ring cuts the piece of the pour round R1's pad off from the via,
        // which the check's connectivity (a pad joined by the pour's outline) does not see.
        let report = cut_board("", &Extras::default());
        let found = cuts(&report);
        assert_eq!(found.len(), 1, "{report}");
        assert!(
            found[0].starts_with("a track of SIG cuts a piece of the pour of GND (")
                && found[0].contains(
                    "off from the rest of its fill, and nothing else joins R1.1 on it to the net"
                )
                && found[0].contains("SMD pad 1 [GND] of R1")
                && found[0].contains("Track [SIG]")
                && found[0].contains("Zone [GND] on F.Cu"),
            "{}",
            found[0]
        );
        let f = report
            .findings
            .iter()
            .find(|f| f.rule == Rule::PadIslandCut)
            .unwrap();
        assert_eq!(f.severity, Severity::Warning);
        // The connectivity still joins the pad: no missing connection says it.
        assert_eq!(report.count(Rule::UnconnectedItems), 0, "{report}");
    }

    #[test]
    fn a_cut_piece_with_its_own_via_or_strict_guidelines() {
        // A GND via on the cut piece joins it to the net on the back: nothing is cut.
        let via = r#"(via (at 12 12) (size 0.6) (drill 0.3) (layers "F.Cu" "B.Cu") (net 1))"#;
        let joined = cut_board(via, &Extras::default());
        assert_eq!(cuts(&joined), Vec::<String>::new(), "{joined}");
        // With strict guidelines it is an error.
        let strict = Extras {
            strict_guidelines: true,
            ..Extras::default()
        };
        let report = cut_board("", &strict);
        let f: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.rule == Rule::PadIslandCut)
            .collect();
        assert_eq!(f.len(), 1, "{report}");
        assert_eq!(f[0].severity, Severity::Error);
    }

    #[test]
    fn a_piece_that_reaches_the_edge_of_a_neighbourhood_is_not_judged() {
        let open = |min: (f64, f64), max: (f64, f64)| Extras {
            open: vec![crate::geom::Bounds {
                min: crate::geom::Pt::new(min.0, min.1),
                max: crate::geom::Pt::new(max.0, max.1),
            }],
            ..Extras::default()
        };
        // A box round the ring holds the cut piece: judged as on the whole board.
        assert_eq!(
            cuts(&cut_board("", &open((3.0, 3.0), (17.0, 17.0)))).len(),
            1
        );
        // A box that cuts through the piece: the piece may go on outside it.
        assert_eq!(
            cuts(&cut_board("", &open((3.0, 3.0), (10.0, 17.0)))),
            Vec::<String>::new()
        );
    }
}
