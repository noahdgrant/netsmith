//! What the process of a board says: track widths, layers, vias and the notes for the order.

use super::*;

impl Board {
    /// The width the router draws a net's tracks at: the widest its rails ask for, and never
    /// less than the rules' track width.
    pub fn track_width(&self, net: &str) -> f64 {
        self.net_widths
            .iter()
            .find(|(n, _)| n == net)
            .map_or(self.rules.track_width, |(_, w)| {
                w.max(self.rules.track_width)
            })
    }

    /// The width the router draws a net's tracks at on `layer`. On an outer layer it is
    /// [`Board::track_width`]; on an inner signal layer, a net whose current is known is drawn at
    /// least as wide as that current needs in the inner copper (IPC-2221's inner constant,
    /// `NetCurrent::inner_least_width`), rounded up to the 0.01 mm a width is written in (issue
    /// #833).
    pub fn track_width_on(&self, net: &str, layer: CopperLayer) -> f64 {
        let width = self.track_width(net);
        if layer.is_outer() {
            return width;
        }
        match self.inner_least_width(net) {
            Some(least) => width.max((least * 100.0 - 1e-9).ceil() / 100.0),
            None => width,
        }
    }

    /// The narrowest a track of `net` on `layer` may be away from its pads: on an inner layer,
    /// the least width its current needs in the inner copper (issue #833), and never less than
    /// the process's least track.
    pub fn least_width_on(&self, net: &str, layer: CopperLayer) -> f64 {
        match (layer.is_outer(), self.inner_least_width(net)) {
            (false, Some(least)) => least.max(self.rules.min_track_width),
            _ => self.least_width(net),
        }
    }

    /// Whether a net keeps its tracks to the outer layers: its current is known and its width
    /// was sized for the outer copper only, because the board has no inner signal layer or its
    /// design was written before a width for one was sized (ADR 0133, issue #833).
    pub fn keeps_outside(&self, net: &str) -> bool {
        self.net_currents
            .iter()
            .any(|c| c.net == net && c.inner_least_width.is_none())
    }

    /// The least width a net's current needs on an inner signal layer, if it is known.
    fn inner_least_width(&self, net: &str) -> Option<f64> {
        self.net_currents
            .iter()
            .find(|c| c.net == net)
            .and_then(|c| c.inner_least_width)
    }

    /// The least space between copper of `net` and copper of any other net: the largest its
    /// rails ask for, and never less than the rules' clearance (issue #758).
    pub fn clearance(&self, net: &str) -> f64 {
        self.net_clearances
            .iter()
            .find(|(n, _)| n == net)
            .map_or(self.rules.clearance, |(_, c)| c.max(self.rules.clearance))
    }

    /// The space copper of `a` keeps from copper of `b`: the larger of their clearances, as
    /// KiCad holds two net classes (issue #758).
    pub fn clearance_between(&self, a: &str, b: &str) -> f64 {
        self.clearance(a).max(self.clearance(b))
    }

    /// The largest clearance of any net: what keeps the farthest of any copper.
    pub fn widest_clearance(&self) -> f64 {
        self.net_clearances
            .iter()
            .map(|(_, c)| *c)
            .fold(self.rules.clearance, f64::max)
    }

    /// The narrowest a track of `net` may be away from its pads: the least width its current
    /// needs, and never less than the process's least track (issue #431).
    pub fn least_width(&self, net: &str) -> f64 {
        self.net_currents
            .iter()
            .find(|c| c.net == net)
            .map_or(0.0, |c| c.least_width)
            .max(self.rules.min_track_width)
    }

    /// The box round the centres of a part's pads, which a track of a net that carries current
    /// may be necked down near (issue #431).
    pub fn neck_box(&self, refdes: &str) -> Option<Rect> {
        let part = self.part(refdes)?;
        let mut pts = part.footprint.pads.iter().map(|p| part.pad_centre(p));
        let first = pts.next()?;
        Some(pts.fold(Rect::new(first, first), |r, p| {
            Rect::new(
                Pt::new(r.min.x.min(p.x), r.min.y.min(p.y)),
                Pt::new(r.max.x.max(p.x), r.max.y.max(p.y)),
            )
        }))
    }

    /// The nets that need more than one via side by side where they change layer, and how
    /// many (issue #431).
    pub fn via_counts(&self) -> Vec<(String, usize)> {
        self.net_currents
            .iter()
            .filter(|c| c.vias > 1)
            .map(|c| (c.net.clone(), c.vias))
            .collect()
    }

    /// The nets whose current needs more than the process's least track, with the least width
    /// it needs: what the design rules check holds their routed copper to (issue #431).
    pub fn least_widths(&self) -> Vec<(String, f64)> {
        self.net_currents
            .iter()
            .filter(|c| c.least_width > self.rules.min_track_width + 1e-9)
            .map(|c| (c.net.clone(), c.least_width))
            .collect()
    }

    /// The nets whose current needs more than the process's least track on an inner signal
    /// layer, with the least width it needs there: what the design rules check holds their
    /// inner copper to (issue #833).
    pub fn inner_least_widths(&self) -> Vec<(String, f64)> {
        self.net_currents
            .iter()
            .filter_map(|c| Some((c.net.clone(), c.inner_least_width?)))
            .filter(|(_, w)| *w > self.rules.min_track_width + 1e-9)
            .collect()
    }

    /// The nets that are routed wider than the rules' track width or keep more than the
    /// rules' clearance, grouped by width and clearance, from the narrowest and then the
    /// least clearance: the net classes of the project file (issue #758).
    pub fn net_classes(&self) -> Vec<NetClass> {
        let mut classes: Vec<NetClass> = Vec::new();
        for net in &self.nets {
            let width = self.track_width(&net.name);
            let clearance = self.clearance(&net.name);
            if width <= self.rules.track_width && clearance <= self.rules.clearance {
                continue;
            }
            match classes
                .iter_mut()
                .find(|c| c.width == width && c.clearance == clearance)
            {
                Some(c) => c.nets.push(net.name.clone()),
                None => classes.push(NetClass {
                    width,
                    clearance,
                    nets: vec![net.name.clone()],
                }),
            }
        }
        classes.sort_by(|a, b| {
            a.width
                .total_cmp(&b.width)
                .then(a.clearance.total_cmp(&b.clearance))
        });
        classes
    }

    /// The process the board was routed and checked to, on one line for the fabrication
    /// notes: who makes it, where its numbers are published, and the rules and stackup the
    /// board ended up with (the library's, after the board narrowed them).
    pub fn process_note(&self) -> String {
        let (p, r) = (&self.process, &self.rules);
        let thickness = p
            .stackup
            .chosen_thickness()
            .map_or_else(|| "unchosen".to_string(), |t| t.to_string());
        let signals: Vec<String> = self
            .inner_layers()
            .into_iter()
            .filter(|l| !self.planes.iter().any(|pl| pl.layer == *l))
            .collect();
        let planes = if self.planes.is_empty() && self.pours.is_empty() && signals.is_empty() {
            String::new()
        } else {
            let mut inner: Vec<(String, String)> = self
                .planes
                .iter()
                .map(|pl| {
                    let text = if self.zoned(&pl.layer) {
                        format!(
                            "{} has the routes file's zones instead of a plane on {}",
                            pl.layer, pl.net
                        )
                    } else {
                        format!("{} is a plane on {}", pl.layer, pl.net)
                    };
                    (pl.layer.clone(), text)
                })
                .chain(signals.iter().map(|l| {
                    (
                        l.clone(),
                        format!("{l} is a signal layer, with no tracks yet"),
                    )
                }))
                .collect();
            inner.sort_by_key(|(l, _)| inner_number(l));
            let list: Vec<String> = inner
                .into_iter()
                .map(|(_, text)| text)
                .chain(self.pours.iter().map(|po| {
                    let layer = po.layer.name();
                    if self.zoned(layer) {
                        format!(
                            "{layer} has the routes file's zones instead of a pour on {}",
                            po.net
                        )
                    } else {
                        format!("{layer} is a pour on {}", po.net)
                    }
                }))
                .collect();
            format!(" ({})", list.join(", "))
        };
        format!(
            "{} {} (numbers from {}); track {} mm, space {} mm, drill {} mm, via {} mm, annular ring {} mm, hole to hole {} mm, copper to edge {} mm, silkscreen {} mm line and {} mm text, mask bridge {} mm; {} layers{}, {} mm thick, {} oz copper{}, finish {}; {}",
            p.fabricator,
            p.name,
            p.source,
            r.min_track_width,
            r.clearance,
            r.via_drill,
            r.via_diameter,
            r.min_annular_ring,
            r.hole_to_hole,
            r.copper_to_edge,
            r.min_text_thickness,
            r.min_text_height,
            p.min_mask_bridge,
            p.stackup.layers,
            planes,
            thickness,
            p.stackup.copper_weight,
            p.stackup
                .inner
                .as_ref()
                .map_or_else(String::new, |i| format!(", build-up {}", i.build)),
            p.finish,
            if p.is_fictional() {
                "a fictional process: this package is not for ordering"
            } else {
                "order it with these"
            },
        )
    }

    /// KiCad's names of the inner copper layers, from the top: `In1.Cu`, `In2.Cu`... Each is a
    /// plane (in [`Board::planes`]) or a signal layer (ADR 0111).
    pub fn inner_layers(&self) -> Vec<String> {
        (1..=self.process.stackup.layers.saturating_sub(2))
            .map(|n| format!("In{n}.Cu"))
            .collect()
    }

    /// The inner layers that are signal layers (ADR 0111), from the top: each inner layer that
    /// is not a plane. The router draws tracks on them as on the outer layers (issue #833).
    pub fn signal_layers(&self) -> Vec<CopperLayer> {
        self.inner_layers()
            .iter()
            .filter(|l| !self.planes.iter().any(|pl| pl.layer == **l))
            .filter_map(|l| CopperLayer::of_name(l))
            .collect()
    }

    /// Each via that stands in a surface-mount pad (its centre is on the pad's copper): the
    /// via's place in [`Board::vias`] and the pad. A via in a pad has to be filled and capped
    /// by the fabricator, or the solder runs down the hole.
    pub fn vias_in_pads(&self) -> Vec<(usize, PadRef)> {
        let mut found = Vec::new();
        for (k, via) in self.vias.iter().enumerate() {
            let at = Rect::new(via.at, via.at);
            for part in &self.parts {
                for pad in &part.footprint.pads {
                    let surface = pad.drill.is_none() && part.pad_layers(pad) != (false, false);
                    if surface && part.pad_bounds(pad).contains(&at) {
                        found.push((
                            k,
                            PadRef {
                                refdes: part.refdes.clone(),
                                number: pad.number.clone(),
                            },
                        ));
                    }
                }
            }
        }
        found
    }

    /// What the board needs of the fabricator beyond its standard process, one line each for
    /// the fabrication notes; empty when the standard process makes it. A board of a fictional
    /// fabricator (ADR 0112) says first that it cannot be ordered. A board with a build-up
    /// adds its track impedance last ([`crate::impedance_notes`]): what a track is on each layer,
    /// which a board that is to be ordered with impedance control is checked against.
    pub fn process_extras(&self) -> Vec<String> {
        let mut extras = Vec::new();
        if self.process.is_fictional() {
            extras.push(format!(
                "fictional fabricator: {} does not exist; its numbers were chosen, not sourced, and stand for no real fabricator's capability, so this package cannot be ordered (`fab --order` refuses it, ADR 0112)",
                self.process.fabricator
            ));
        }
        let in_pad = self.vias_in_pads();
        if !in_pad.is_empty() {
            let mut pads: Vec<String> = in_pad.iter().map(|(_, p)| p.to_string()).collect();
            pads.dedup();
            extras.push(format!(
                "vias in pads: {} via(s) stand in pads ({}); order them as epoxy filled and capped (plated over) vias, which {} makes up to {} mm wide at extra cost, because an open via in a pad draws the solder away; this is on the order form and the Gerbers do not say it",
                in_pad.len(),
                pads.join(", "),
                self.process.fabricator,
                self.process.via_in_pad
            ));
        }
        // What a track's impedance is on each layer, from the recorded build-up (issue #759).
        extras.extend(crate::impedance::impedance_notes(
            &self.process.stackup,
            self.rules.track_width,
            IMPEDANCE_TARGET,
        ));
        // What the design states for some nets (issue #1007), at the widths the router draws.
        extras.extend(crate::impedance::target_notes(
            &self.process.stackup,
            &self.impedance_targets,
            &|net, layer| match CopperLayer::of_name(layer) {
                Some(l) => self.track_width_on(net, l),
                None => self.track_width(net),
            },
        ));
        extras
    }
}

/// The impedance `fab-notes.txt` gives a track's width for, in ohms: the usual impedance of a
/// single-ended line (an RF receptacle's `.impedance`, a coaxial cable). A board states no
/// target here: a design that states one (a `TrackImpedance` or a `PairImpedance`, issue
/// #1007) gets its own lines.
const IMPEDANCE_TARGET: f64 = 50.0;
