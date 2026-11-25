//! `routes route NET --layer L` (issue #1656): the net routed on one inner signal layer, with a
//! via at each end.
//!
//! **Which layers.** The router draws on the outer layers and the inner layers the process
//! declares signal layers (`.process.inner2.use &= "signal"`, ADR 0111); an inner layer the
//! process gives a net is a plane, one sheet of copper that holds no track. A four-layer board
//! whose two inner layers are planes (the usual stack-up: ground and a supply) has no inner
//! signal layer, so the router routes it on the outer layers only. `--layer` names an inner
//! signal layer ([`signal_layer`]); an outer layer, a plane or a layer the board does not have
//! is refused, and the refusal lists the board's layers from the top, what each is, and how to
//! make an inner layer a signal layer. A net whose current sets a width for the outer copper
//! alone (ADR 0133) is refused too: its width was not sized for the inner copper.
//!
//! **The vias at the ends.** A pad of the net (of the connection, for `NET:PAD-PAD`) on the
//! layer, a plated hole's, is an end as it is. Each other pad gets a through via of the
//! process near it, as `routes route --via` places one (issue #1642, [`super::through`]): the
//! first legal place, as `routes check-via` judges a via of the net ([`crate::via_check`],
//! which counts keep-outs), on rings round the pad's centre, the first just clear of the pad's
//! copper (half its diagonal, the via's radius and the net's clearance) and each next
//! [`END_STEP`] mm further out, at most [`END_RINGS`] rings, in [`END_TURNS`] directions on each,
//! the one towards the net's other pads first. A pad with no legal place is a refusal that says
//! how many places were checked; `--via X,Y` names the vias instead (each pad joined to the
//! nearest, as with `--via` alone), and each must reach the layer.
//!
//! **On the layer alone.** Each pad is joined to its via first, as `--via` joins it. Then the
//! vias (and the pads on the layer) are joined to one another by one run of the router on the
//! layer alone, with no via: the net is routed as a net on one layer is (issue #1274,
//! [`netsmith_router::one::route_nets_on`]), every other net held as it is, so it goes round
//! their copper, the pads, the keep-outs and the edge. A plated pad of each via's size and drill
//! stands in for the via in that run (never written). No cost is added for the copper of the
//! layers beside it. Where the layer has no way, the refusal says what walls it in, and nothing
//! is written.
//!
//! **Checked as an edit**, as every result of `routes route` is ([`super::finish`]): one that
//! adds an error of the design rules check is refused. `fab` is unchanged: it routes what the
//! routes file leaves open on every layer, as before.
//!
//! **Bounds, counts.** At most `END_RINGS * END_TURNS` places checked for each pad
//! ([`Routed::end_checks`] counts them), one run of the router for each pad not joined to its
//! via and one for the layer, at most [`MOST_RUNS`] in all.

use super::*;

use netsmith_pcb::{CopperLayer, PadKind, Pt};

/// How far apart the rings of places for a via at an end are, mm.
pub const END_STEP: f64 = 0.15;
/// How many rings of places for a via at an end are checked.
pub const END_RINGS: usize = 4;
/// How many directions on each ring.
pub const END_TURNS: usize = 12;

/// The inner signal layer `word` names on `board` (`inner2`, or KiCad's `In2.Cu`): refused, with
/// the board's layers listed, for an outer layer, a plane or a layer it does not have.
pub fn signal_layer(board: &Board, word: &str) -> Result<CopperLayer, Refusal> {
    let layers = layers_said(board);
    let fix = "an inner layer is a signal layer when the process says so, `.process.inner2.use &= \"signal\"`, and a plane when it names a net";
    let Some(layer) = CopperLayer::of_word(word).or_else(|| CopperLayer::of_name(word)) else {
        return Err(Refusal::Refused(format!(
            "`{word}` is not a copper layer: `--layer` names an inner signal layer, `inner2` (or KiCad's `In2.Cu`); the board {} has {layers}",
            board.name
        )));
    };
    let name = layer.word();
    if layer.is_outer() {
        return Err(Refusal::Refused(format!(
            "`{name}` is an outer layer: `--layer` names an inner signal layer of the board {} (the router uses the outer layers without it); it has {layers}",
            board.name
        )));
    }
    if !board.inner_layers().iter().any(|l| l == layer.name()) {
        return Err(Refusal::Refused(format!(
            "the board {} has no layer `{name}`: it has {layers}",
            board.name
        )));
    }
    if let Some(plane) = board.planes.iter().find(|p| p.layer == layer.name()) {
        return Err(Refusal::Refused(format!(
            "`{name}` is a plane on net {} of the board {}, not a signal layer: a plane is one sheet of copper and holds no track. The board has {layers}; {fix}",
            plane.net, board.name
        )));
    }
    Ok(layer)
}

/// The board's copper layers from the top, what each is, and its signal layers:
/// `4 layers (top: signal, inner1: plane on GND, inner2: plane on 3V3, bottom: signal) and no
/// inner signal layer`.
fn layers_said(board: &Board) -> String {
    let mut each = vec!["top: signal".to_string()];
    for name in board.inner_layers() {
        let what = match board.planes.iter().find(|p| p.layer == name) {
            Some(p) => format!("plane on {}", p.net),
            None => "signal".to_string(),
        };
        let word = CopperLayer::of_name(&name).map_or(name.clone(), |l| l.word().to_string());
        each.push(format!("{word}: {what}"));
    }
    each.push("bottom: signal".to_string());
    let signal: Vec<&str> = board.signal_layers().iter().map(|l| l.word()).collect();
    format!(
        "{} layers ({}) and {}",
        board.copper_count(),
        each.join(", "),
        match signal.as_slice() {
            [] => "no inner signal layer".to_string(),
            [one] => format!("one inner signal layer, {one}"),
            more => format!("the inner signal layers {}", more.join(", ")),
        }
    )
}

/// Whether pad `pad` has copper on `layer`: a plated hole's pad is on every layer.
pub(crate) fn pad_on(board: &Board, pad: &PadRef, layer: CopperLayer) -> bool {
    let Some(part) = board.part(&pad.refdes) else {
        return false;
    };
    part.footprint.pads_numbered(&pad.number).any(|p| {
        p.kind == PadKind::ThruHole && p.layers.iter().any(|l| l == "*.Cu" || l == layer.name())
    })
}

impl Runs<'_> {
    /// `--layer`: the module's header.
    #[allow(clippy::type_complexity)]
    pub(super) fn layered(
        &mut self,
        real: &Board,
        routes: &Routes,
        out: &mut Routed,
    ) -> Result<Result<Option<Routes>, String>, Refusal> {
        let net = self.ask.net.as_str();
        let layer = self.ask.layer.expect("asked with a layer");
        if !self.ask.rip.is_empty() || self.ask.reroute {
            return Ok(Err(
                "`--layer` routes the net round the copper there: it takes no `--rip` or `--reroute`"
                    .to_string(),
            ));
        }
        if self.board.keeps_outside(net) {
            return Ok(Err(format!(
                "net {net} keeps to the outer layers: its current sets its width for the outer copper alone, so it is not routed on {}",
                layer.word()
            )));
        }
        if self.open(real, net, true).is_empty() {
            out.joined = true;
            return Ok(Ok(None));
        }
        let asked = if self.ask.through.is_empty() {
            match self.end_vias(routes, layer, out)? {
                Ok(asked) => asked,
                Err(why) => return Ok(Err(why)),
            }
        } else {
            self.ask.through.clone()
        };
        self.through_vias(routes, out, &asked)
    }

    /// A place for a through via near each pad of the net that is not on `layer`, each checked
    /// with the vias placed before it (the module's header).
    fn end_vias(
        &mut self,
        routes: &Routes,
        layer: CopperLayer,
        out: &mut Routed,
    ) -> Result<Result<Vec<ViaAsked>, String>, Refusal> {
        let board = self.board;
        let net = self.ask.net.as_str();
        let pads = self.pads();
        let mut trial = routes.clone();
        let mut asked: Vec<ViaAsked> = Vec::new();
        let corner = board.outline.min;
        for (pad, at) in &pads {
            if pad_on(board, pad, layer) {
                continue;
            }
            // Towards the middle of the net's other pads.
            let others: Vec<Pt> = pads
                .iter()
                .filter(|(p, _)| p != pad)
                .map(|(_, a)| *a)
                .collect();
            let towards = if others.is_empty() {
                0.0
            } else {
                let n = others.len() as f64;
                let mid = Pt::new(
                    others.iter().map(|p| p.x).sum::<f64>() / n,
                    others.iter().map(|p| p.y).sum::<f64>() / n,
                );
                (mid.y - at.y).atan2(mid.x - at.x)
            };
            let half = board
                .part(&pad.refdes)
                .and_then(|part| {
                    let p = part.footprint.pads_numbered(&pad.number).next()?;
                    let b = part.pad_bounds(p);
                    Some((b.max.x - b.min.x).hypot(b.max.y - b.min.y) / 2.0)
                })
                .unwrap_or(0.0);
            let first = half + board.rules.via_diameter / 2.0 + board.clearance(net);
            let mut found = None;
            let mut checked = 0;
            'rings: for ring in 0..END_RINGS {
                let d = first + ring as f64 * END_STEP;
                for k in 0..END_TURNS {
                    // 0, +1, -1, +2, -2, ... turns of the circle's twelfth from `towards`.
                    let step = k.div_ceil(2) as f64 * if k % 2 == 1 { 1.0 } else { -1.0 };
                    let angle = towards + step * std::f64::consts::TAU / END_TURNS as f64;
                    let place = *at + Pt::new(d * angle.cos(), d * angle.sin()) - corner;
                    let place = [um(place.x), um(place.y)];
                    checked += 1;
                    out.end_checks += 1;
                    let judged = crate::via_check::check_via(
                        board,
                        &trial,
                        place,
                        Some(net),
                        None,
                        None,
                        None,
                    )?;
                    if judged.legal {
                        found = Some((place, judged.via));
                        break 'rings;
                    }
                }
            }
            let Some((place, via)) = found else {
                return Ok(Err(format!(
                    "pad {pad} of net {net} has no place for a via to {} within {:.2} mm of its edge ({checked} place(s) checked, `routes check-via` says why one is refused): `--via X,Y` names one",
                    layer.word(),
                    board.rules.via_diameter / 2.0
                        + board.clearance(net)
                        + (END_RINGS - 1) as f64 * END_STEP
                )));
            };
            trial
                .boards
                .entry(board.name.clone())
                .or_default()
                .nets
                .entry(net.to_string())
                .or_insert_with(|| NetRoutes {
                    pads: netsmith_pcb::pad_hash(board, net).unwrap_or_default(),
                    ..NetRoutes::default()
                })
                .vias
                .push(RouteVia {
                    at: place,
                    diameter: via.diameter,
                    drill: via.drill,
                    layers: Vec::new(),
                    lock: false,
                });
            asked.push(ViaAsked {
                at: place,
                layers: None,
            });
        }
        Ok(Ok(asked))
    }

    /// The net's vias `vias` (and its pads on `layer`) joined to one another on `layer` alone,
    /// the copper added to `state` (the module's header); a refusal says what walls it in.
    pub(super) fn on_layer(
        &mut self,
        state: &mut Routes,
        vias: &[netsmith_pcb::Via],
        layer: CopperLayer,
        out: &mut Routed,
    ) -> Result<Result<(), String>, Refusal> {
        let board = self.board;
        let net = self.ask.net.as_str();
        let (mut b, _) = with_routes(board, state)?;
        self.cut(&mut b, net);
        let mut ends: Vec<PadRef> = b
            .nets
            .iter()
            .find(|n| n.name == net)
            .map(|n| n.pads.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| pad_on(board, p, layer))
            .collect();
        let mut stands: Vec<(String, [f64; 2])> = Vec::new();
        for (k, via) in vias.iter().enumerate() {
            // A through pad, so that the router's ends are on every layer, the one asked too.
            let through = netsmith_pcb::Via {
                span: None,
                ..via.clone()
            };
            let part = through::stand_in(&b, &through, k);
            let at = via.at - board.outline.min;
            stands.push((format!("{}.1", part.refdes), [um(at.x), um(at.y)]));
            ends.push(PadRef {
                refdes: part.refdes.clone(),
                number: "1".to_string(),
            });
            b.parts.push(part);
        }
        if let Some(n) = b.nets.iter_mut().find(|n| n.name == net) {
            n.pads = ends;
        }
        if !Ratsnest::of(&b).lines.iter().any(|l| l.net == net) {
            return Ok(Ok(()));
        }
        self.runs += 1;
        let way = one::route_nets_on(
            &b,
            &BTreeSet::from([net.to_string()]),
            self.ask.weights,
            Some(layer),
        );
        if !way.complete() {
            let said = |text: &str| {
                stands.iter().fold(text.to_string(), |t, (stand, at)| {
                    t.replace(
                        stand,
                        &format!("the via at ({}, {})", at[0] + 0.0, at[1] + 0.0),
                    )
                })
            };
            out.walls = walls(board, &way.why)
                .into_iter()
                .map(|mut w| {
                    w.line = said(&w.line);
                    w.pads.retain(|p| !stands.iter().any(|(s, _)| s == p));
                    w
                })
                .collect();
            return Ok(Err(format!(
                "net {net} has no way on {} between its vias at the ends, round the copper there, so nothing is written",
                layer.word()
            )));
        }
        add(board, state, net, &way);
        Ok(Ok(()))
    }
}
