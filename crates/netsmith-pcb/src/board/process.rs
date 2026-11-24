//! What the process of a board says: track widths, layers, vias and the notes for the order.

use std::cmp::Ordering;

use super::*;
use crate::ratsnest::Ratline;

/// Vias of a board that stand in pads: each via's place in [`Board::vias`] and the pad.
type ViasAt = Vec<(usize, PadRef)>;

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

    /// The branch whose pin has `pad`, on `net` (issue #1113): a pin of a part that states the
    /// current of its branch.
    pub fn branch_of(&self, net: &str, pad: &PadRef) -> Option<&netsmith_core::BranchCurrent> {
        self.branch_currents.iter().find(|b| {
            b.net == net
                && b.pins
                    .iter()
                    .any(|p| p.refdes == pad.refdes && p.pin == pad.number)
        })
    }

    /// How a pour joins the pad `number` of the part `refdes`, as its pin states it (issue
    /// #1276), if it does.
    pub fn pad_pour(&self, refdes: &str, number: &str) -> Option<&netsmith_core::PadPour> {
        self.pad_pours.iter().find(|p| {
            p.pins
                .iter()
                .any(|pin| pin.refdes == refdes && pin.pin == number)
        })
    }

    /// The branch a line carries the current of (issue #1113): a line with one end at a pad of a
    /// branch and the other at a pad of no branch, or a pad's line to its plane. The ratsnest
    /// joins a branch's pad to the rest of its net by one such line
    /// (`crate::ratsnest`), so the line carries what the branch does and no more. A line
    /// between the pads of two branches carries what the net does, and so does any line of a
    /// net with an impedance target, which is drawn at the width that gives it.
    pub fn line_branch(&self, line: &Ratline) -> Option<&netsmith_core::BranchCurrent> {
        if self.branch_currents.is_empty()
            || self.impedance_widths.iter().any(|w| w.net == line.net)
        {
            return None;
        }
        let from = self.branch_of(&line.net, &line.from);
        if line.to_plane {
            return from;
        }
        match (from, self.branch_of(&line.net, &line.to)) {
            (Some(b), None) | (None, Some(b)) => Some(b),
            _ => None,
        }
    }

    /// The width the router draws a line's tracks at on `layer`: its branch's, when it carries
    /// one's current ([`Board::line_branch`]), and otherwise its net's
    /// ([`Board::track_width_on`]). A branch is drawn at its own width on an outer layer, and
    /// on an inner signal layer at least as wide as its current needs in the inner copper.
    pub fn line_width_on(&self, line: &Ratline, layer: CopperLayer) -> f64 {
        let Some(b) = self.line_branch(line) else {
            return self.track_width_on(&line.net, layer);
        };
        let width = b.width.max(self.rules.track_width);
        match b.inner_least_width.filter(|_| !layer.is_outer()) {
            Some(least) => width.max((least * 100.0 - 1e-9).ceil() / 100.0),
            None => width,
        }
    }

    /// The narrowest a line's track may be on `layer` away from its pads: its branch's least
    /// width when it carries one's current, and otherwise its net's
    /// ([`Board::least_width_on`]).
    pub fn line_least_width_on(&self, line: &Ratline, layer: CopperLayer) -> f64 {
        let Some(b) = self.line_branch(line) else {
            return self.least_width_on(&line.net, layer);
        };
        let least = match b.inner_least_width.filter(|_| !layer.is_outer()) {
            Some(inner) => inner,
            None => b.least_width,
        };
        least.max(self.rules.min_track_width)
    }

    /// How many vias side by side a line needs where it changes layer: its branch's count when
    /// it carries one's current, and otherwise its net's.
    pub fn line_vias(&self, line: &Ratline) -> usize {
        match self.line_branch(line) {
            Some(b) => b.vias,
            None => self
                .net_currents
                .iter()
                .find(|c| c.net == line.net)
                .map_or(1, |c| c.vias),
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

    /// Whether a net's corners are mitered: the board's `.corner` is `Mitered` (issue #1508), or
    /// the net is on a `TrackPath` whose corners are (issue #1275). The router draws no arc at
    /// its bends, and `view pcb` draws an arc of it as the chamfer ([`Board::chamfered`]).
    pub fn mitered(&self, net: &str) -> bool {
        self.corner == netsmith_core::TrackCorner::Mitered
            || self.track_paths.iter().any(|p| {
                p.corner == netsmith_core::TrackCorner::Mitered && p.nets.iter().any(|n| n == net)
            })
    }

    /// The board with each arc of a mitered net ([`Board::mitered`]) drawn as its chamfer, a
    /// straight track from its start to its end (issue #1508): what `view pcb` draws, so a board
    /// whose style is mitered reads as mitered even where its copper (an import, a hand-drawn
    /// arc) has arcs. `None` when it has no such arc.
    pub fn chamfered(&self) -> Option<Board> {
        if !self
            .tracks
            .iter()
            .any(|t| t.mid.is_some() && self.mitered(&t.net))
        {
            return None;
        }
        let mut board = self.clone();
        for i in 0..board.tracks.len() {
            if board.tracks[i].mid.is_some() && self.mitered(&board.tracks[i].net) {
                board.tracks[i].mid = None;
            }
        }
        Some(board)
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

    /// The RF nets (issue #1274): the nets of a stated impedance of one track
    /// ([`netsmith_core::rf_nets`]). The placer and the router take them first and keep them
    /// short.
    pub fn rf_nets(&self) -> Vec<String> {
        netsmith_core::rf_nets(&self.impedance_targets)
    }

    /// The nets routed on one copper layer with no via (issue #1274): the RF nets, unless the
    /// physical file says `route any_layer`, and the nets it says `route one_layer` of
    /// ([`netsmith_core::one_layer_nets`]).
    pub fn one_layer_nets(&self) -> Vec<String> {
        netsmith_core::one_layer_nets(&self.impedance_targets, &self.net_routes)
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
            match classes
                .iter_mut()
                .find(|c| c.width == width && c.clearance == clearance && c.pair_gap == pair_gap)
            {
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
                .then(
                    a.pair_gap
                        .partial_cmp(&b.pair_gap)
                        .unwrap_or(Ordering::Equal),
                )
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
                    let text = if self.replaced(&pl.layer, &pl.net) {
                        format!(
                            "{} has the routes file's zones instead of a plane on {}",
                            pl.layer, pl.net
                        )
                    } else if self.zoned(&pl.layer) {
                        format!(
                            "{} is a plane on {}, {} the routes file's zones",
                            pl.layer,
                            pl.net,
                            self.cut_by(&pl.layer, &pl.net)
                        )
                    } else {
                        format!("{} is a plane on {}", pl.layer, pl.net)
                    };
                    (pl.layer.clone(), text)
                })
                .chain(signals.iter().map(|l| {
                    let count = self
                        .track_widths_on(l)
                        .iter()
                        .map(|(_, n)| n)
                        .sum::<usize>();
                    let tracks = match count {
                        // A board with no copper at all is not routed yet.
                        0 if self.tracks.is_empty() => "with no tracks yet".to_string(),
                        0 => "with no tracks".to_string(),
                        1 => "with 1 track".to_string(),
                        n => format!("with {n} tracks"),
                    };
                    (l.clone(), format!("{l} is a signal layer, {tracks}"))
                }))
                .collect();
            inner.sort_by_key(|(l, _)| inner_number(l));
            let list: Vec<String> = inner
                .into_iter()
                .map(|(_, text)| text)
                .chain(self.pours.iter().map(|po| {
                    let layer = po.layer.name();
                    if self.replaced(layer, &po.net) {
                        format!(
                            "{layer} has the routes file's zones instead of a pour on {}",
                            po.net
                        )
                    } else if self.zoned(layer) {
                        format!(
                            "{layer} is a pour on {}, {} the routes file's zones",
                            po.net,
                            self.cut_by(layer, &po.net)
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

    /// The track widths in use on the layer named `layer` (`F.Cu`, `In2.Cu`...), narrowest
    /// first, each with how many tracks have it (an arc is a track), widths taken to the
    /// micrometre. The fabrication notes quote the impedance of each (issue #1589).
    pub fn track_widths_on(&self, layer: &str) -> Vec<(f64, usize)> {
        let mut counts: std::collections::BTreeMap<i64, usize> = std::collections::BTreeMap::new();
        for t in self.tracks.iter().filter(|t| t.layer.name() == layer) {
            *counts.entry((t.width * 1000.0).round() as i64).or_default() += 1;
        }
        counts
            .into_iter()
            .map(|(w, n)| (w as f64 / 1000.0, n))
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

    /// Each via that stands in a surface-mount pad: its centre is on the pad's copper (its
    /// shape, not its box: [`Placed::pad_copper_distance`], issue #1578), the edge included,
    /// and the via is on the pad's face. The via's place in [`Board::vias`] and the pad. A via in
    /// a pad has to be filled and capped by the fabricator, or the solder runs down the hole.
    pub fn vias_in_pads(&self) -> Vec<(usize, PadRef)> {
        self.vias_by_pads(|_, distance| distance <= IN_PAD_TOLERANCE)
    }

    /// Each via whose hole reaches into a surface-mount pad on its face while its centre is on
    /// no such pad (issue #1578): the drill's circle crosses the pad's edge, so part of the hole
    /// opens in the pad. It is not a via in a pad ([`Board::vias_in_pads`]) and is not filled;
    /// `fab-notes.txt` lists them so that a person can decide. A hole that only meets the edge
    /// at a point is not one. The via's place in [`Board::vias`] and each pad it reaches.
    pub fn vias_touching_pads(&self) -> Vec<(usize, PadRef)> {
        let inside: std::collections::HashSet<usize> =
            self.vias_in_pads().into_iter().map(|(k, _)| k).collect();
        self.vias_by_pads(|via, distance| {
            distance > IN_PAD_TOLERANCE && distance < via.drill / 2.0 - IN_PAD_TOLERANCE
        })
        .into_iter()
        .filter(|(k, _)| !inside.contains(k))
        .collect()
    }

    /// Each via and surface-mount pad on its face, the via's centre `distance` mm from the
    /// pad's copper, that `takes` says yes to.
    fn vias_by_pads(&self, takes: impl Fn(&Via, f64) -> bool) -> Vec<(usize, PadRef)> {
        let n = self.copper_count();
        let mut found = Vec::new();
        for (k, via) in self.vias.iter().enumerate() {
            for part in &self.parts {
                for pad in &part.footprint.pads {
                    let (front, back) = part.pad_layers(pad);
                    let surface = pad.drill.is_none() && (front || back);
                    let met = (front && via.on(CopperLayer::Front, n))
                        || (back && via.on(CopperLayer::Back, n));
                    // The box first: a via far from the pad is not measured.
                    if !surface
                        || !met
                        || !part
                            .pad_bounds(pad)
                            .expanded(via.drill)
                            .contains(&Rect::new(via.at, via.at))
                    {
                        continue;
                    }
                    if takes(via, part.pad_copper_distance(pad, via.at)) {
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

    /// Whether the `k`th via is a laser-drilled blind via of an HDI process, which the process
    /// fills and planarizes by plating as it makes every one (issue #1071).
    fn laser_via(&self, offers: &[ViaOffer], k: usize) -> bool {
        let span = self.vias[k].span;
        span.is_some() && offers.iter().any(|o| o.laser && o.span == span)
    }

    /// [`Board::vias_in_pads`] split in two: the laser-drilled blind vias of an HDI process, which
    /// the process fills and planarizes by plating as it makes every one (issue #1071), and the
    /// drilled vias, which the board house fills with epoxy and caps at extra cost.
    fn vias_in_pads_split(&self) -> (ViasAt, ViasAt) {
        let offers = self.via_offers();
        self.vias_in_pads()
            .into_iter()
            .partition(|(k, _)| self.laser_via(&offers, *k))
    }

    /// The drilled vias that stand in pads, which the board house has to fill and cap (issue
    /// #1118): how many, and the parts and pads that carry them. A laser-drilled blind via of an
    /// HDI process is not one of them.
    pub fn filled_vias_in_pads(&self) -> ViasInPads {
        let (_, in_pad) = self.vias_in_pads_split();
        ViasInPads::of(in_pad.len(), &in_pad)
    }

    /// The drilled vias whose hole touches a pad's edge, their centre outside every pad
    /// ([`Board::vias_touching_pads`], issue #1578): how many vias, and the parts and pads. A
    /// laser-drilled blind via of an HDI process, filled by plating, is not one of them.
    pub fn drilled_vias_touching_pads(&self) -> ViasInPads {
        let offers = self.via_offers();
        let touching: ViasAt = self
            .vias_touching_pads()
            .into_iter()
            .filter(|(k, _)| !self.laser_via(&offers, *k))
            .collect();
        let mut vias: Vec<usize> = touching.iter().map(|(k, _)| *k).collect();
        vias.dedup();
        ViasInPads::of(vias.len(), &touching)
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
                "fictional fabricator: {} does not exist; its numbers were chosen, not sourced, and stand for no real fabricator's capability, so this package cannot be ordered (`project order` refuses it, ADR 0112)",
                self.process.fabricator
            ));
        }
        let in_pad = self.filled_vias_in_pads();
        if in_pad.count > 0 {
            extras.push(in_pad.line(&self.process.fabricator, self.process.via_in_pad));
        }
        let touching = self.drilled_vias_touching_pads();
        if touching.count > 0 {
            extras.push(touching.touching_line(&self.process.fabricator));
        }
        let blind_in_pad = self.vias_in_pads_split().0;
        if !blind_in_pad.is_empty() {
            let mut pads: Vec<String> = blind_in_pad.iter().map(|(_, p)| p.to_string()).collect();
            pads.dedup();
            extras.push(format!(
                "blind vias in pads: {} blind via(s) stand in pads ({}); {}'s HDI service fills and planarizes its blind vias by plating, so they are ordered with the HDI board and need no epoxy fill",
                blind_in_pad.len(),
                pads.join(", "),
                self.process.fabricator
            ));
        }
        // An HDI board is ordered from the fabricator's HDI service, with its own build-up
        // (issue #1015).
        if let Some(hdi) = &self.process.hdi {
            let (mut blind, mut buried) = (0, 0);
            for v in &self.vias {
                match v.span {
                    Some(s) if s.is_blind() => blind += 1,
                    Some(_) => buried += 1,
                    None => {}
                }
            }
            let spans: Vec<String> = self
                .via_offers()
                .iter()
                .filter_map(|o| o.span)
                .map(|s| format!("{}-{}", s.from.name(), s.to.name()))
                .collect();
            extras.push(format!(
                "HDI: order it from {}'s HDI service as a {}-layer board of order {} with the build-up `{}` the board states, which {} confirms before it makes the board; {blind} blind via(s), laser drilled and filled and planarized by plating, and {buried} buried via(s), resin plugged and capped, of the spans {}; each span has its own drill file",
                self.process.fabricator,
                self.process.stackup.layers,
                hdi.order,
                self.process.stackup.inner.as_ref().map_or("", |i| i.build.as_str()),
                self.process.fabricator,
                spans.join(", ")
            ));
        }
        // What a track's impedance is on each layer, from the recorded build-up (issue #759).
        extras.extend(crate::impedance::impedance_notes(
            &self.process.stackup,
            &|layer| self.track_widths_on(layer),
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
        extras.extend(self.track_path_lines());
        extras
    }

    /// The length of the tracks of all the nets of a `TrackPath` together, in mm (issue #1275).
    pub fn track_path_length(&self, path: &netsmith_core::TrackPath) -> f64 {
        self.tracks
            .iter()
            .filter(|t| path.nets.contains(&t.net))
            .map(Track::length)
            .sum()
    }

    /// The loss of the tracks of all the nets of a `TrackPath` together, in dB, at its
    /// `.frequency` (issue #1405): each track's length times the loss per mm of its width on its
    /// layer ([`crate::loss_per_mm`]). `None` when the path states no frequency; an error that
    /// says why when a track has no figure (no build-up, no loss tangent recorded for it, a
    /// width outside a formula's domain). The parts in series are not counted.
    pub fn track_path_loss(&self, path: &netsmith_core::TrackPath) -> Option<Result<f64, String>> {
        let f = path.frequency?;
        let mut total = 0.0;
        for t in self.tracks.iter().filter(|t| path.nets.contains(&t.net)) {
            match crate::loss_per_mm(&self.process.stackup, t.layer.name(), t.width, f) {
                Ok(l) => total += l.total() * t.length(),
                Err(e) => return Some(Err(e.to_string())),
            }
        }
        Some(Ok(total))
    }

    /// One line of `fab-notes.txt` for each `TrackPath` of the design (issue #1275): its chain,
    /// its routed length against its budget, its loss at its frequency against its loss budget
    /// (issue #1405), its corners and its fence.
    pub fn track_path_lines(&self) -> Vec<String> {
        self.track_paths
            .iter()
            .map(|p| {
                let mut chain = p.nets[0].clone();
                for (part, net) in p.parts.iter().zip(&p.nets[1..]) {
                    chain.push_str(&format!(" -{part}- {net}"));
                }
                let length = self.track_path_length(p);
                let budget = match p.budget_length {
                    Some(b) if length > b + 1e-9 => format!(
                        "{length:.2} mm of track, OVER its length budget of {b} mm: move the parts of the path nearer each other"
                    ),
                    Some(b) => format!("{length:.2} mm of track, within its length budget of {b} mm"),
                    None => format!("{length:.2} mm of track"),
                };
                let loss = match (self.track_path_loss(p), p.budget_loss) {
                    (None, Some(l)) => format!(
                        "; its loss budget of {l} dB is not judged: the path states no `.frequency` to work the loss out at"
                    ),
                    (None, None) => String::new(),
                    (Some(Err(e)), Some(l)) => format!(
                        "; its loss budget of {l} dB is not judged: no loss at {}: {e}",
                        crate::hertz(p.frequency.unwrap_or(0.0))
                    ),
                    (Some(Err(e)), None) => format!(
                        "; no loss at {}: {e}",
                        crate::hertz(p.frequency.unwrap_or(0.0))
                    ),
                    (Some(Ok(got)), budget) => {
                        let at = crate::hertz(p.frequency.unwrap_or(0.0));
                        match budget {
                            Some(l) if got > l + 1e-9 => format!(
                                "; {got:.3} dB of track loss at {at}, OVER its loss budget of {l} dB: shorten the path or widen its tracks on a thicker dielectric"
                            ),
                            Some(l) => format!(
                                "; {got:.3} dB of track loss at {at}, within its loss budget of {l} dB"
                            ),
                            None => format!("; {got:.3} dB of track loss at {at}"),
                        }
                    }
                };
                let corner = match p.corner {
                    netsmith_core::TrackCorner::Rounded => "",
                    netsmith_core::TrackCorner::Mitered => "; corners mitered, no arcs",
                };
                let fence = match p.fence {
                    None => String::new(),
                    Some(f) => {
                        let layers: Vec<CopperLayer> = self
                            .tracks
                            .iter()
                            .filter(|t| p.nets.contains(&t.net))
                            .map(|t| t.layer)
                            .collect();
                        let grounds: Vec<String> = layers
                            .iter()
                            .filter_map(|&l| self.fence_ground(l))
                            .collect::<std::collections::BTreeSet<_>>()
                            .into_iter()
                            .collect();
                        if grounds.is_empty() && !layers.is_empty() {
                            "; no fence: no pour on its layer and no inner plane of a rail's net to join ground vias to".to_string()
                        } else {
                            format!(
                                "; a fence of ground vias every {f} mm along both sides{}",
                                if grounds.is_empty() {
                                    String::new()
                                } else {
                                    format!(", of `{}`", grounds.join("`, `"))
                                }
                            )
                        }
                    }
                };
                format!(
                    "track path: `{}` ({chain}), {} ohm: {budget}{loss}{corner}{fence}",
                    p.path, p.ohms
                )
            })
            .collect()
    }

    /// The ground a fence of a `TrackPath`'s tracks on `layer` is joined to (issue #1275): the
    /// net of the pour on that layer, else the first rail net, by name, that has an inner plane.
    pub fn fence_ground(&self, layer: CopperLayer) -> Option<String> {
        if let Some(p) = self.pours.iter().find(|p| p.layer == layer) {
            return Some(p.net.clone());
        }
        let mut planes: Vec<&str> = self
            .planes
            .iter()
            .map(|p| p.net.as_str())
            .filter(|n| self.rail_nets.iter().any(|r| r == n))
            .collect();
        planes.sort();
        planes.first().map(|n| n.to_string())
    }
}

/// How near a via's centre must be to a pad's copper to stand in it, in mm: on its edge counts.
const IN_PAD_TOLERANCE: f64 = 1e-6;

/// The impedance `fab-notes.txt` gives a track's width for, in ohms: the usual impedance of a
/// single-ended line (an RF receptacle's `.impedance`, a coaxial cable). A board states no
/// target here: a design that states one (a `TrackImpedance` or a `PairImpedance`, issue
/// #1007) gets its own lines.
const IMPEDANCE_TARGET: f64 = 50.0;

/// The drilled vias of a board that stand in pads ([`Board::filled_vias_in_pads`]): the board
/// house fills them with epoxy and caps them with plating, an option of its order form that costs
/// extra (issue #1118). `fab-notes.txt` says so in a line of its own, an order's summary says it
/// for each board, and `project order` stops until the cost is accepted.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ViasInPads {
    /// How many vias stand in pads.
    pub count: usize,
    /// The parts whose pads carry them, `U1`, sorted, each once.
    pub parts: Vec<String>,
    /// The pads, `U1.5`, each once, in the order of the board's vias.
    pub pads: Vec<String>,
}

impl ViasInPads {
    /// `count` vias at `at` (each via with a pad it is in or touches): the parts sorted, each
    /// once, and the pads each once in the order of the board's vias.
    fn of(count: usize, at: &[(usize, PadRef)]) -> ViasInPads {
        let mut parts: Vec<String> = at.iter().map(|(_, p)| p.refdes.clone()).collect();
        parts.sort();
        parts.dedup();
        let mut pads: Vec<String> = Vec::new();
        for (_, p) in at {
            let pad = p.to_string();
            if !pads.contains(&pad) {
                pads.push(pad);
            }
        }
        ViasInPads { count, parts, pads }
    }

    /// `3 vias whose hole touches a pad's edge` (`1 via whose ...`): the first words of the line
    /// of the vias whose hole touches a pad (issue #1578), which an order's summary repeats.
    pub fn touching_headline(&self) -> String {
        format!(
            "{} {} hole touches a pad's edge",
            self.count,
            if self.count == 1 {
                "via whose"
            } else {
                "vias whose"
            }
        )
    }

    /// The line of `fab-notes.txt` for the vias whose hole touches a pad's edge (issue #1578):
    /// the headline, the parts and pads, and why a person may want to look. It stops nothing.
    pub fn touching_line(&self, fabricator: &str) -> String {
        format!(
            "{}: on {} (pads {}); each centre is off the pad's copper, so it is not counted as a via in a pad and is not filled, but part of its hole opens in the pad and may draw solder down it; whether {fabricator} takes it for a via in a pad is not known (the guide's chapter 21, `.via_in_pad`, quotes what JLCPCB's pages say): move it off the pad, or ask the board house",
            self.touching_headline(),
            self.parts.join(", "),
            self.pads.join(", "),
        )
    }

    /// `21 vias in pads: the board house's filled and capped option is needed (extra cost)` (`1 via
    /// in a pad: ...`): the first words of the line, which an order's summary repeats.
    pub fn headline(&self) -> String {
        format!(
            "{} {}: the board house's filled and capped option is needed (extra cost)",
            self.count,
            if self.count == 1 {
                "via in a pad"
            } else {
                "vias in pads"
            }
        )
    }

    /// The line of `fab-notes.txt`: the headline, the parts and pads, and why.
    pub fn line(&self, fabricator: &str, widest: f64) -> String {
        format!(
            "{}; on {} (pads {}); {fabricator} fills them with epoxy and caps them with plating up to {widest} mm wide, because an open via in a pad draws the solder away; choose it on the order form, the Gerbers do not say it; `.process.via_in_pad &= 0mm` forbids vias in pads",
            self.headline(),
            self.parts.join(", "),
            self.pads.join(", "),
        )
    }
}
