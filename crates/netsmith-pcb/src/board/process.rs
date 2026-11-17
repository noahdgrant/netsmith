//! What the process of a board says: track widths, layers, vias and the notes for the order.

use std::cmp::Ordering;

use super::*;

impl Board {
    /// The width the router draws a net's tracks at on the outer layers: the front's
    /// [`Board::track_width_on`]. For a net with no impedance target it is the widest its rails
    /// ask for, and never less than the rules' track width.
    pub fn track_width(&self, net: &str) -> f64 {
        self.track_width_on(net, CopperLayer::Front)
    }

    /// The widest a net's rails ask for, and never less than the rules' track width.
    fn rail_width(&self, net: &str) -> f64 {
        self.net_widths
            .iter()
            .find(|(n, _)| n == net)
            .map_or(self.rules.track_width, |(_, w)| {
                w.max(self.rules.track_width)
            })
    }

    /// The width the router draws a net's tracks at on `layer`.
    ///
    /// A net with an impedance target (issue #1025) is drawn on each layer at the width that
    /// gives it there ([`Board::target_width`]), and never narrower than the least its current
    /// needs or the process makes ([`Board::least_width_on`]; the board's
    /// `impedance_findings` say where that floor moves it off the target). Its rails' widths and
    /// the rules' track width do not size it.
    ///
    /// Any other net: on an outer layer the widest its rails ask for, and never less than the
    /// rules' track width; on an inner signal layer, a net whose current is known is drawn at
    /// least as wide as that current needs in the inner copper (IPC-2221's inner constant,
    /// `NetCurrent::inner_least_width`), rounded up to the 0.01 mm a width is written in (issue
    /// #833).
    pub fn track_width_on(&self, net: &str, layer: CopperLayer) -> f64 {
        if let Some(w) = self.target_width(net, layer) {
            return w.max(self.least_width_on(net, layer));
        }
        let width = self.rail_width(net);
        if layer.is_outer() {
            return width;
        }
        match self.inner_least_width(net) {
            Some(least) => width.max((least * 100.0 - 1e-9).ceil() / 100.0),
            None => width,
        }
    }

    /// The width that gives a net its impedance target on `layer` (issue #1025), to the nearest
    /// 0.01 mm; `None` when the net has no target, or no width gives it there.
    pub fn target_width(&self, net: &str, layer: CopperLayer) -> Option<f64> {
        let name = layer.name();
        self.impedance_widths
            .iter()
            .find(|w| w.net == net && w.layer == name)
            .map(|w| w.width)
    }

    /// Give the board the impedance targets of its design and solve the width each net of
    /// them is drawn at on each layer (`impedance_widths`, issue #1025).
    pub fn set_impedance_targets(&mut self, targets: Vec<netsmith_core::ImpedanceTarget>) {
        self.impedance_widths = crate::impedance::target_widths(&self.process.stackup, &targets);
        self.impedance_targets = targets;
    }

    /// The pair a net is one track of by a `PairImpedance` (issue #1025): its partner and the
    /// spacing the target states between the two tracks, edge to edge, in mm. The first target
    /// that names the net, as for its width.
    pub fn pair_spacing(&self, net: &str) -> Option<(&str, f64)> {
        let t = self
            .impedance_targets
            .iter()
            .find(|t| t.nets.iter().any(|n| n == net))?;
        let spacing = t.spacing?;
        let partner = t.nets.iter().find(|n| *n != net)?;
        Some((partner.as_str(), spacing))
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
    ///
    /// A net of a pair with an impedance target keeps at least the pair's spacing (issue
    /// #1025), from its partner and so from any copper: copper of another net nearer than the
    /// partner would couple to the track more strongly than the partner does.
    pub fn clearance(&self, net: &str) -> f64 {
        let spacing = self.pair_spacing(net).map_or(0.0, |(_, s)| s);
        self.net_clearances
            .iter()
            .find(|(n, _)| n == net)
            .map_or(self.rules.clearance, |(_, c)| c.max(self.rules.clearance))
            .max(spacing)
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
            .chain(self.impedance_targets.iter().filter_map(|t| t.spacing))
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

    /// The nets that are routed at another width than the rules' track width (wider for their
    /// current or rails, or at the width an impedance target gives), keep more than the rules'
    /// clearance or are a pair with an impedance target, grouped by width, clearance and
    /// pair gap, from the narrowest and then the least clearance: the net classes of the
    /// project file (issues #758, #1025). The width is the outer layers'.
    pub fn net_classes(&self) -> Vec<NetClass> {
        let mut classes: Vec<NetClass> = Vec::new();
        for net in &self.nets {
            let width = self.track_width(&net.name);
            let clearance = self.clearance(&net.name);
            // A pair's gap is never less than the clearance its tracks keep.
            let pair_gap = self
                .pair_spacing(&net.name)
                .map(|(_, s)| s.max(self.rules.clearance));
            if (width - self.rules.track_width).abs() < 1e-9
                && clearance <= self.rules.clearance
                && pair_gap.is_none()
            {
                continue;
            }
            match classes.iter_mut().find(|c| {
                c.width == width && c.clearance == clearance && c.pair_gap == pair_gap
            }) {
                Some(c) => c.nets.push(net.name.clone()),
                None => classes.push(NetClass {
                    width,
                    clearance,
                    pair_gap,
                    nets: vec![net.name.clone()],
                }),
            }
        }
        classes.sort_by(|a, b| {
            a.width
                .total_cmp(&b.width)
                .then(a.clearance.total_cmp(&b.clearance))
                .then(a.pair_gap.partial_cmp(&b.pair_gap).unwrap_or(Ordering::Equal))
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
        // How much of each routed pair keeps a gap that gives its impedance (issue #1025).
        extras.extend(self.pair_couplings().iter().map(|c| {
            let gaps = match c.gaps {
                Some((lo, hi)) => format!(", at gaps of {lo:.3} to {hi:.3} mm edge to edge"),
                None => String::new(),
            };
            format!(
                "track impedance pair: nets `{}` and `{}` ({}): {:.1} mm of the {:.1} mm of `{}`'s track runs beside `{}` at a gap whose differential impedance is within JLCPCB's test tolerance ({:.0} %){gaps}; the rest is where the pair parts at its ends, or the router could not keep it together",
                c.p,
                c.n,
                c.path,
                c.within,
                c.length,
                c.p,
                c.n,
                c.share() * 100.0
            )
        }));
        extras
    }
}

/// The impedance `fab-notes.txt` gives a track's width for, in ohms: the usual impedance of a
/// single-ended line (an RF receptacle's `.impedance`, a coaxial cable). A board states no
/// target here: a design that states one (a `TrackImpedance` or a `PairImpedance`, issue
/// #1007) gets its own lines.
const IMPEDANCE_TARGET: f64 = 50.0;
