//! A summary of a board's copper areas (issue #1085): which layers carry a plane or a pour, on
//! which net and with which settings, how many zones the routes file holds by hand, and how
//! many teardrop fills the routed board has. `netsmith board copper` prints it and the editor's
//! PCB summary tab shows it, both from [`CopperAreas::of`], so the two cannot differ.
//!
//! It is read from the source: the process's layers (the design's [`netsmith_core::Process`]),
//! the outer pours (`process.top.net`, `process.bottom.net` and their fields) and the zones of
//! the routes file, with no placing and no routing. The KiCad board `fab` writes holds a zone for
//! each teardrop fill as well, hundreds of them on a routed board, so a count of its zones by
//! net and layer looks like a pour and is not. The four kinds of copper area are told apart here:
//!
//! - a **plane**: an inner layer that is one sheet on a net (`InnerUse::Plane`);
//! - a **pour**: an outer layer poured on a net by the process (`process.top.net`);
//! - a **hand zone**: a zone of the routes file (`netsmith routes add-zone`, or imported from a
//!   KiCad board), counted apart from the process's pours. One of the net of the layer's plane
//!   or pour restates it, and the process's copper there is not drawn
//!   ([`AreaLayer::replaced_by_zones`]); one of another net has the area it covers, and the
//!   plane or pour is drawn round it (issue #1314);
//! - a **teardrop fill**: the copper that widens a track where it meets a pad or a via. The
//!   router makes it after it has routed the board, so the source holds none and
//!   [`AreaTeardrops::counted`] is false until a routed board is given to
//!   [`CopperAreas::with_teardrops`] (the editor does, when the board has been routed).
//!
//! # Decisions (issue #1085)
//!
//! - **Every copper layer, top to bottom, with one of three uses**: `plane` (an inner layer that
//!   is a plane), `pour` (an outer layer with a pour) and `signal` (a layer for tracks: an inner
//!   signal layer, or an outer layer with no pour). No layer is `none`: each is one of these.
//! - **The fields as stated.** A pour field the design does not state is `None` here and is said
//!   to be the default (`netsmith_pcb::BoardPour`: the process's plane clearance, a minimum
//!   width of 0.25 mm, a thermal relief), so the summary never claims a number the process did
//!   not state.
//! - **The routes file's zones as written.** A zone of a net whose pads moved since the record
//!   was written is dropped when the board is built ([`crate::apply_routes`], which needs the
//!   placed pads); that cannot be told from the source, so [`AreaZone::locked`] is shown: a
//!   locked zone is kept whatever the pads do.

use netsmith_core::{Design, InnerUse};
use serde::{Deserialize, Serialize};

use crate::{Board, RouteZone, Routes, ZoneIslands, ZonePads};

/// What a copper layer is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaKind {
    /// An inner layer that is one sheet of copper on a net.
    Plane,
    /// An outer layer poured on a net.
    Pour,
    /// A layer for tracks: an inner signal layer, or an outer layer with no pour.
    Signal,
}

impl AreaKind {
    /// The word of the text and of JSON: `plane`, `pour` or `signal`.
    pub fn word(self) -> &'static str {
        match self {
            AreaKind::Plane => "plane",
            AreaKind::Pour => "pour",
            AreaKind::Signal => "signal",
        }
    }
}

/// The fields of a pour of the process, as the design states them (mm); `None` for one it
/// does not state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaPour {
    /// The most space between the vias that stitch it to the other layers of its net.
    pub stitch_pitch: Option<f64>,
    /// The gap of a thermal relief.
    pub thermal_gap: Option<f64>,
    /// The width of the spokes of a thermal relief.
    pub spoke_width: Option<f64>,
    /// The space from copper of other nets (the process's plane clearance when not stated).
    pub clearance: Option<f64>,
    /// The narrowest copper the fill keeps (0.25 mm when not stated).
    pub min_width: Option<f64>,
    /// How it joins its net's pads: `solid`, `thermal`, `none` or `thru_hole_only` (a thermal
    /// relief when not stated).
    pub pads: Option<ZonePads>,
}

/// One copper layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaLayer {
    /// The routes file's word for it: `top`, `inner1`, `bottom`.
    pub layer: String,
    pub kind: AreaKind,
    /// The net of the plane or the pour; none for a signal layer.
    pub net: Option<String>,
    /// The fields of the pour; none for a plane and a signal layer.
    pub pour: Option<AreaPour>,
    /// How many zones of the routes file are on the layer.
    pub zones: u32,
    /// Whether a zone of the plane's or the pour's own net is among them: the routes file then
    /// restates that copper, and the process's plane or pour on the layer is not drawn. Zones of
    /// other nets cut the plane or the pour where they are and leave it drawn round them (issue
    /// #1314).
    pub replaced_by_zones: bool,
}

/// A zone of the routes file, counted apart from the process's pours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaZone {
    pub net: String,
    /// The layers it fills: `top`, `inner1`, `bottom`.
    pub layers: Vec<String>,
    pub priority: u32,
    pub clearance: f64,
    pub min_width: f64,
    pub pads: ZonePads,
    pub thermal_gap: f64,
    pub spoke_width: f64,
    pub islands: ZoneIslands,
    /// The area of its outline less its holes, in mm2, on one layer.
    pub area: f64,
    /// Whether the record keeps it when the net's pads have moved.
    pub locked: bool,
}

/// The teardrop fills of the routed board.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AreaTeardrops {
    /// Whether they were counted: false for the source alone, which holds none (the router makes
    /// them, `fab` writes them as zones of the KiCad board).
    pub counted: bool,
    pub total: u32,
    /// How many are on each layer that has any, top to bottom.
    pub layers: Vec<AreaTeardropLayer>,
}

/// The teardrop fills on one layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaTeardropLayer {
    pub layer: String,
    pub count: u32,
}

/// The copper areas of a board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CopperAreas {
    pub board: String,
    /// Every copper layer, from the top.
    pub layers: Vec<AreaLayer>,
    /// The zones of the routes file, in the file's order of nets.
    pub zones: Vec<AreaZone>,
    pub teardrops: AreaTeardrops,
}

impl CopperAreas {
    /// The copper areas of `design` and of the zones `routes` holds for it, with no teardrops
    /// counted. A design with no process has no layers: an error saying so.
    pub fn of(design: &Design, routes: Option<&Routes>) -> Result<CopperAreas, String> {
        let process = design.process.as_ref().ok_or_else(|| {
            format!(
                "`{}` states no process, so it has no copper layers to summarise",
                design.name
            )
        })?;
        let inner = process.stackup.layers.saturating_sub(2) as usize;
        let mut zones = Vec::new();
        if let Some(table) = routes.and_then(|r| r.boards.get(&design.name)) {
            for (net, record) in &table.nets {
                zones.extend(record.zones.iter().map(|z| zone(net, z, record.lock)));
            }
        }
        let on = |word: &str, net: Option<&str>| -> u32 {
            zones
                .iter()
                .filter(|z| z.layers.iter().any(|l| l == word))
                .filter(|z| net.is_none_or(|n| z.net == n))
                .count() as u32
        };
        let mut layers = Vec::with_capacity(inner + 2);
        let mut push = |layer: String, kind: AreaKind, net: Option<String>, pour| {
            let zones = on(&layer, None);
            // A zone of the layer's own net restates its pour or plane; one of another net cuts
            // it (issue #1314).
            let own = net.as_deref().map_or(0, |n| on(&layer, Some(n)));
            layers.push(AreaLayer {
                replaced_by_zones: own > 0 && kind != AreaKind::Signal,
                layer,
                kind,
                net,
                pour,
                zones,
            });
        };
        let outer = |word: &str| design.features.pours.iter().find(|p| p.layer == word);
        let outer_layer = |word: &str| match outer(word) {
            Some(p) => (
                AreaKind::Pour,
                Some(p.net.clone()),
                Some(AreaPour {
                    stitch_pitch: p.stitch_pitch,
                    thermal_gap: p.thermal_gap,
                    spoke_width: p.spoke_width,
                    clearance: p.clearance,
                    min_width: p.min_width,
                    pads: p.pads,
                }),
            ),
            None => (AreaKind::Signal, None, None),
        };
        let (kind, net, pour) = outer_layer("top");
        push("top".into(), kind, net, pour);
        for i in 0..inner {
            let used = process
                .stackup
                .inner
                .as_ref()
                .and_then(|inner| inner.layers.get(i));
            let (kind, net) = match used {
                Some(InnerUse::Plane(net)) => (AreaKind::Plane, Some(net.clone())),
                _ => (AreaKind::Signal, None),
            };
            push(format!("inner{}", i + 1), kind, net, None);
        }
        let (kind, net, pour) = outer_layer("bottom");
        push("bottom".into(), kind, net, pour);
        Ok(CopperAreas {
            board: design.name.clone(),
            layers,
            zones,
            teardrops: AreaTeardrops::default(),
        })
    }

    /// The same, with the teardrop fills of `routed` counted: the board after the router has
    /// drawn its teardrops.
    pub fn with_teardrops(mut self, routed: &Board) -> CopperAreas {
        let mut layers: Vec<AreaTeardropLayer> = Vec::new();
        for drop in &routed.teardrops {
            let word = drop.layer.word();
            match layers.iter_mut().find(|l| l.layer == word) {
                Some(l) => l.count += 1,
                None => layers.push(AreaTeardropLayer {
                    layer: word.to_string(),
                    count: 1,
                }),
            }
        }
        let rank = |word: &str| self.layers.iter().position(|l| l.layer == word);
        layers.sort_by_key(|l| rank(&l.layer).unwrap_or(usize::MAX));
        self.teardrops = AreaTeardrops {
            counted: true,
            total: layers.iter().map(|l| l.count).sum(),
            layers,
        };
        self
    }

    /// The summary as short text for a person.
    pub fn text(&self) -> String {
        let mut out = format!("{}: {} copper layers\n", self.board, self.layers.len());
        for l in &self.layers {
            let net = l
                .net
                .as_deref()
                .map_or(String::new(), |n| format!(" on {n}"));
            out.push_str(&format!("  {:<7} {}{net}", l.layer, l.kind.word()));
            if let Some(p) = &l.pour {
                out.push_str(&format!(": {}", pour_fields(p)));
            }
            if l.replaced_by_zones {
                out.push_str(&format!(
                    " (not drawn: the routes file's zones of its net replace it; {} zone(s) on the layer)",
                    l.zones
                ));
            } else if l.zones > 0 && l.kind != AreaKind::Signal {
                out.push_str(&format!(
                    " (cut by {} hand zone(s) of other nets, drawn round them)",
                    l.zones
                ));
            } else if l.zones > 0 {
                out.push_str(&format!(" ({} hand zone(s))", l.zones));
            }
            out.push('\n');
        }
        out.push_str(&format!("hand zones (routes file): {}\n", self.zones.len()));
        for z in &self.zones {
            out.push_str(&format!(
                "  {} on {}: {} mm2, priority {}, clearance {} mm, min width {} mm, pads {}, thermal gap {} mm, spoke {} mm{}{}\n",
                z.net,
                z.layers.join(", "),
                round(z.area),
                z.priority,
                z.clearance,
                z.min_width,
                z.pads.word(),
                z.thermal_gap,
                z.spoke_width,
                match z.islands {
                    ZoneIslands::Remove => "",
                    ZoneIslands::Keep => ", islands kept",
                    ZoneIslands::RemoveBelow => ", small islands removed",
                },
                if z.locked { ", locked" } else { "" },
            ));
        }
        if self.teardrops.counted {
            out.push_str(&format!("teardrop fills: {}", self.teardrops.total));
            if !self.teardrops.layers.is_empty() {
                let by: Vec<String> = self
                    .teardrops
                    .layers
                    .iter()
                    .map(|l| format!("{} on {}", l.count, l.layer))
                    .collect();
                out.push_str(&format!(" ({})", by.join(", ")));
            }
            out.push_str(" (zones of the KiCad board that are no pour)\n");
        } else {
            out.push_str(
                "teardrop fills: not counted, the router makes them when it routes the board (`fab` writes each as a zone of the KiCad board that is no pour)\n",
            );
        }
        out
    }
}

/// The stated fields of a pour in words, `stitch pitch 7 mm, ...`, and the default for the rest.
fn pour_fields(p: &AreaPour) -> String {
    let mm = |name: &str, v: Option<f64>| match v {
        Some(v) => format!("{name} {v} mm"),
        None => format!("{name} default"),
    };
    format!(
        "{}, {}, {}, {}, {}, pads {}",
        mm("stitch pitch", p.stitch_pitch),
        mm("thermal gap", p.thermal_gap),
        mm("spoke width", p.spoke_width),
        mm("clearance", p.clearance),
        mm("min width", p.min_width),
        p.pads
            .map_or("default".to_string(), |c| c.word().to_string()),
    )
}

fn zone(net: &str, z: &RouteZone, record_lock: bool) -> AreaZone {
    AreaZone {
        net: net.to_string(),
        layers: z.layers.clone(),
        priority: z.priority,
        clearance: z.clearance,
        min_width: z.min_width,
        pads: z.pads,
        thermal_gap: z.thermal_gap,
        spoke_width: z.spoke_width,
        islands: z.islands,
        area: ring_area(&z.outline) - z.holes.iter().map(|h| ring_area(h)).sum::<f64>(),
        locked: z.lock || record_lock,
    }
}

/// The area of a polygon, in mm2 (the shoelace formula).
fn ring_area(points: &[[f64; 2]]) -> f64 {
    let n = points.len();
    let twice: f64 = (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    (twice / 2.0).abs()
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::tests::{four_layer_design, led_board, led_design};
    use crate::{BoardRoutes, CopperLayer, NetRoutes, Pt, Teardrop};

    fn square(side: f64) -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [side, 0.0], [side, side], [0.0, side]]
    }

    fn pour(layer: &str, net: &str) -> netsmith_core::Pour {
        netsmith_core::Pour {
            layer: layer.into(),
            net: net.into(),
            stitch_pitch: Some(7.0),
            thermal_gap: None,
            spoke_width: None,
            clearance: Some(0.3),
            min_width: None,
            pads: Some(ZonePads::Solid),
        }
    }

    fn routes_with_zone(board: &str, net: &str, layer: &str, lock: bool) -> Routes {
        let zone = RouteZone {
            layers: vec![layer.into()],
            priority: 2,
            clearance: 0.2,
            min_width: 0.25,
            pads: ZonePads::Thermal,
            thermal_gap: 0.5,
            spoke_width: 0.5,
            islands: ZoneIslands::Remove,
            island_min_area: None,
            outline: square(4.0),
            holes: vec![square(1.0)],
            lock,
        };
        let record = NetRoutes {
            zones: vec![zone],
            ..NetRoutes::default()
        };
        Routes {
            boards: [(
                board.to_string(),
                BoardRoutes {
                    nets: [(net.to_string(), record)].into(),
                },
            )]
            .into(),
        }
    }

    #[test]
    fn the_area_of_a_ring_less_its_holes() {
        assert_eq!(ring_area(&square(4.0)), 16.0);
        let reversed: Vec<[f64; 2]> = square(4.0).into_iter().rev().collect();
        assert_eq!(ring_area(&reversed), 16.0);
        assert_eq!(ring_area(&[]), 0.0);
    }

    #[test]
    fn a_pour_field_the_design_does_not_state_is_said_to_be_the_default() {
        let p = AreaPour {
            stitch_pitch: Some(7.0),
            thermal_gap: None,
            spoke_width: None,
            clearance: Some(0.3),
            min_width: None,
            pads: Some(ZonePads::Solid),
        };
        assert_eq!(
            pour_fields(&p),
            "stitch pitch 7 mm, thermal gap default, spoke width default, clearance 0.3 mm, min width default, pads solid"
        );
    }

    #[test]
    fn a_design_with_no_process_has_no_layers() {
        let mut design = led_design();
        design.process = None;
        let e = CopperAreas::of(&design, None).unwrap_err();
        assert!(e.contains("states no process"), "{e}");
    }

    #[test]
    fn planes_pours_and_signal_layers_are_told_apart() {
        let mut design = four_layer_design();
        design.features.pours.push(pour("top", "GND"));
        let areas = CopperAreas::of(&design, None).unwrap();
        let rows: Vec<(&str, AreaKind, Option<&str>)> = areas
            .layers
            .iter()
            .map(|l| (l.layer.as_str(), l.kind, l.net.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                ("top", AreaKind::Pour, Some("GND")),
                ("inner1", AreaKind::Plane, Some("GND")),
                ("inner2", AreaKind::Plane, Some("3V3")),
                ("bottom", AreaKind::Signal, None),
            ]
        );
        let top = areas.layers[0].pour.as_ref().unwrap();
        assert_eq!(
            (top.stitch_pitch, top.thermal_gap, top.clearance, top.pads),
            (Some(7.0), None, Some(0.3), Some(ZonePads::Solid))
        );
        assert!(areas.layers[1].pour.is_none() && areas.layers[3].pour.is_none());
        assert!(areas.zones.is_empty());
        assert!(!areas.teardrops.counted);
        // A board with no pour at all: the outer layers are signal layers.
        let areas = CopperAreas::of(&four_layer_design(), None).unwrap();
        assert_eq!(areas.layers[0].kind, AreaKind::Signal);
        assert_eq!(areas.layers[3].kind, AreaKind::Signal);
    }

    #[test]
    fn a_hand_zone_is_counted_apart_from_the_pours_and_replaces_what_its_layer_has() {
        let mut design = four_layer_design();
        design.features.pours.push(pour("top", "GND"));
        let routes = routes_with_zone(&design.name, "GND", "top", true);
        let areas = CopperAreas::of(&design, Some(&routes)).unwrap();
        assert_eq!(areas.zones.len(), 1);
        let z = &areas.zones[0];
        assert_eq!(
            (z.net.as_str(), z.layers.as_slice(), z.priority),
            ("GND", &["top".to_string()][..], 2)
        );
        assert_eq!(z.area, 15.0, "the outline less its hole");
        assert!(z.locked);
        // The layer's pour is kept in the summary and marked as not drawn; no other layer is.
        let top = &areas.layers[0];
        assert_eq!(
            (top.kind, top.zones, top.replaced_by_zones),
            (AreaKind::Pour, 1, true)
        );
        assert!(top.pour.is_some());
        assert!(
            areas.layers[1..]
                .iter()
                .all(|l| l.zones == 0 && !l.replaced_by_zones)
        );
        // A zone of another net cuts the pour and replaces nothing (issue #1314).
        let routes = routes_with_zone(&design.name, "VCC", "top", false);
        let areas = CopperAreas::of(&design, Some(&routes)).unwrap();
        let top = &areas.layers[0];
        assert_eq!(
            (top.kind, top.zones, top.replaced_by_zones),
            (AreaKind::Pour, 1, false)
        );
        assert!(
            areas.text().contains("cut by 1 hand zone(s) of other nets"),
            "{}",
            areas.text()
        );
        // On a signal layer the zones replace nothing.
        let routes = routes_with_zone(&design.name, "GND", "bottom", false);
        let areas = CopperAreas::of(&design, Some(&routes)).unwrap();
        let bottom = &areas.layers[3];
        assert_eq!(
            (bottom.kind, bottom.zones, bottom.replaced_by_zones),
            (AreaKind::Signal, 1, false)
        );
        assert!(!areas.zones[0].locked);
        // A record of another board is not this board's.
        let routes = routes_with_zone("Other", "GND", "top", false);
        assert!(
            CopperAreas::of(&design, Some(&routes))
                .unwrap()
                .zones
                .is_empty()
        );
    }

    #[test]
    fn teardrop_fills_are_counted_by_layer_from_a_routed_board() {
        let mut board = led_board();
        let drop = |layer| Teardrop {
            net: "GND".into(),
            layer,
            points: vec![Pt::new(0.0, 0.0), Pt::new(1.0, 0.0), Pt::new(0.0, 1.0)],
        };
        board.teardrops = vec![
            drop(CopperLayer::Back),
            drop(CopperLayer::Front),
            drop(CopperLayer::Back),
        ];
        let areas = CopperAreas::of(&led_design(), None)
            .unwrap()
            .with_teardrops(&board);
        assert!(areas.teardrops.counted);
        assert_eq!(areas.teardrops.total, 3);
        let by: Vec<(&str, u32)> = areas
            .teardrops
            .layers
            .iter()
            .map(|l| (l.layer.as_str(), l.count))
            .collect();
        assert_eq!(by, [("top", 1), ("bottom", 2)], "top to bottom");
        // They are no layer's pour and no hand zone.
        assert!(areas.zones.is_empty() && areas.layers.iter().all(|l| l.kind == AreaKind::Signal));
        assert!(
            areas
                .text()
                .contains("teardrop fills: 3 (1 on top, 2 on bottom)")
        );
    }

    #[test]
    fn the_text_says_the_four_apart() {
        let mut design = four_layer_design();
        design.features.pours.push(pour("top", "GND"));
        let routes = routes_with_zone(&design.name, "GND", "bottom", false);
        let text = CopperAreas::of(&design, Some(&routes)).unwrap().text();
        assert!(
            text.contains("top     pour on GND: stitch pitch 7 mm"),
            "{text}"
        );
        assert!(text.contains("inner1  plane on GND\n"), "{text}");
        assert!(text.contains("bottom  signal (1 hand zone(s))"), "{text}");
        assert!(
            text.contains("hand zones (routes file): 1\n  GND on bottom: 15 mm2, priority 2"),
            "{text}"
        );
        assert!(text.contains("teardrop fills: not counted"), "{text}");
    }
}
