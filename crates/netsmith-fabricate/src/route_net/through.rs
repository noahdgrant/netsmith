//! `routes route NET --via X,Y[,LAYER,LAYER]` (issue #1642): the net routed through vias placed
//! where they are asked for.
//!
//! **The via.** Each via asked for is the one of the net's record at that place (within
//! [`SAME_PLACE`]), when the record has one, else a new via of the record, unlocked, of the
//! process's size (the two layers a blind via ends on, when given), added as `routes add-via`
//! adds it: refused, with nothing written, when the design rules check finds an error with it (a
//! clearance to another net's copper, a short), and the refusal names what blocks it. A buried
//! via (on no outer layer) is refused: a way to it starts on an outer layer.
//!
//! **Through it first.** Each pad of the net (of the connection, for `NET:PAD-PAD`) goes to the
//! nearest of the vias asked for, and each pad, the nearest to its via first, is joined to its
//! via before anything else is routed: the router joins the pad, and the copper that joins it
//! already, to the via, every other pad of the net counted as no net's for that run, as a
//! connection is routed alone ([`Ask::connection`]). It reaches the via as it reaches a pad: a
//! plated pad of the via's size and drill on the via's layers stands in for the via in that run
//! (a part of the board for the router only, never written). A pad its via joins already costs
//! no run. A pad with no way to its via is a refusal that says what walls it in. Then the rest of
//! the net is routed as without `--via`, which joins the vias' groups to one another.
//!
//! **Bounds.** One run of the router for each pad not joined to its via and one for the rest, at
//! most [`MOST_RUNS`]: a net with more pads than that is refused before the first run. The
//! result is one edit of the file, checked as every result of `routes route` is
//! ([`super::finish`]). `--rip` and `--reroute` are not taken with `--via`.

use super::*;

use crate::routes::via_offer;
use netsmith_pcb::{CopperLayer, Footprint, Mount, Pt, Side};

/// How near a via of the record must stand to the place asked for to be the one asked for, mm.
pub const SAME_PLACE: f64 = 0.001;

/// A via asked for (`--via X,Y[,LAYER,LAYER]`), in the routes file's places.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViaAsked {
    pub at: [f64; 2],
    /// The two layers a blind via ends on; `None` is a through via.
    pub layers: Option<[CopperLayer; 2]>,
}

/// What became of a via asked for ([`Routed::through`]), in the routes file's places.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Through {
    pub at: [f64; 2],
    /// The layers it ends on (`top`, `bottom`, `inner1`, ...).
    pub layers: Vec<String>,
    /// It was added to the record; `false` when the record had it there already.
    pub added: bool,
    /// The pads joined through it, nearest first.
    pub pads: Vec<String>,
}

impl Runs<'_> {
    /// The net routed through the vias of the ask: the module's header.
    #[allow(clippy::type_complexity)]
    pub(super) fn through(
        &mut self,
        routes: &Routes,
        out: &mut Routed,
    ) -> Result<Result<Option<Routes>, String>, Refusal> {
        let board = self.board;
        let net = self.ask.net.as_str();
        if !self.ask.rip.is_empty() || self.ask.reroute {
            return Ok(Err(
                "`--via` routes the net round the copper there: it takes no `--rip` or `--reroute`"
                    .to_string(),
            ));
        }
        let mut state = routes.clone();
        let mut vias: Vec<netsmith_pcb::Via> = Vec::new();
        for asked in &self.ask.through {
            let said = at_said(asked.at);
            let record = state.boards.get(&board.name).and_then(|t| t.nets.get(net));
            let found = record.and_then(|r| {
                r.vias
                    .iter()
                    .find(|v| (v.at[0] - asked.at[0]).hypot(v.at[1] - asked.at[1]) < SAME_PLACE)
            });
            let via = match found {
                Some(v) => {
                    out.through.push(Through {
                        at: v.at,
                        layers: Vec::new(),
                        added: false,
                        pads: Vec::new(),
                    });
                    v.on(board, net)
                }
                None => {
                    let offer = match via_offer(board, asked.layers.as_ref()) {
                        Ok(o) => o,
                        Err(Refusal::Refused(e)) => {
                            return Ok(Err(format!("the via {said} of net {net}: {e}")));
                        }
                        Err(e) => return Err(e),
                    };
                    if offer
                        .span
                        .is_some_and(|s| s.from != CopperLayer::Front && s.to != CopperLayer::Back)
                    {
                        return Ok(Err(format!(
                            "the via {said} of net {net} is a buried via: a way to a via starts on an outer layer, so `--via` takes a through or a blind via"
                        )));
                    }
                    let add = Edit::AddVia {
                        net: net.to_string(),
                        at: asked.at,
                        diameter: None,
                        drill: None,
                        layers: asked.layers,
                        lock: false,
                    };
                    let mut next = state.clone();
                    match edit_all(board, &mut next, &[add]) {
                        Ok(_) => {}
                        Err(Refusal::Refused(e)) => {
                            return Ok(Err(format!("the via {said} of net {net}: {e}")));
                        }
                        Err(e) => return Err(e),
                    }
                    state = next;
                    let r = &state.boards[&board.name].nets[net];
                    let v = r.vias.last().expect("the via was added");
                    out.through.push(Through {
                        at: v.at,
                        layers: Vec::new(),
                        added: true,
                        pads: Vec::new(),
                    });
                    v.on(board, net)
                }
            };
            let (real, _) = with_routes(board, &state)?;
            out.through.last_mut().expect("pushed").layers = real
                .via_layers(&via)
                .iter()
                .map(|l| l.word().to_string())
                .collect();
            if !vias.iter().any(|v| v.at.distance(via.at) < SAME_PLACE) {
                vias.push(via);
            } else {
                out.through.pop();
            }
        }
        // Each pad and the via it goes to, the nearest to its via first.
        let pads = self.pads();
        if pads.len() + 1 > self.limit {
            return Ok(Err(format!(
                "net {net} has {} pads: `--via` routes each to its via in a run of its own, and a try runs the router at most {} times",
                pads.len(),
                self.limit
            )));
        }
        let mut goes: Vec<(usize, f64, PadRef)> = pads
            .into_iter()
            .map(|(pad, at)| {
                let (k, d) = vias
                    .iter()
                    .enumerate()
                    .map(|(k, v)| (k, v.at.distance(at)))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .expect("one via at least");
                (k, d, pad)
            })
            .collect();
        goes.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        for (k, _, pad) in goes {
            let (mut b, _) = with_routes(board, &state)?;
            let stand_in = stand_in(&b, &vias[k], k);
            let stand_pad = PadRef {
                refdes: stand_in.refdes.clone(),
                number: "1".to_string(),
            };
            b.parts.push(stand_in);
            if let Some(n) = b.nets.iter_mut().find(|n| n.name == net) {
                n.pads = vec![pad.clone(), stand_pad.clone()];
            }
            let to = out
                .through
                .iter()
                .position(|t| {
                    let at = board.outline.min + Pt::new(t.at[0], t.at[1]);
                    at.distance(vias[k].at) < SAME_PLACE
                })
                .expect("each via is said");
            out.through[to].pads.push(pad.to_string());
            if !Ratsnest::of(&b).lines.iter().any(|l| l.net == net) {
                continue;
            }
            self.runs += 1;
            let way =
                one::route_nets_weighed(&b, &BTreeSet::from([net.to_string()]), self.ask.weights);
            if !way.complete() {
                let via_said = at_said(out.through[to].at);
                let stand = stand_pad.to_string();
                out.walls = walls(board, &way.why)
                    .into_iter()
                    .map(|mut w| {
                        w.line = w.line.replace(&stand, &format!("the via {via_said}"));
                        w.pads.retain(|p| *p != stand);
                        w
                    })
                    .collect();
                return Ok(Err(format!(
                    "pad {pad} of net {net} has no way to the via {via_said}, so nothing is written"
                )));
            }
            add(board, &mut state, net, &way);
        }
        let (after, _) = with_routes(board, &state)?;
        if !self.open(&after, net, true).is_empty() {
            let rest = self.route(&state, net, true)?;
            if !rest.complete() {
                out.walls = walls(board, &rest.why);
                return Ok(Err(format!(
                    "net {net} was joined through the via(s) asked for, but the rest of it has no way round the copper there, so nothing is written"
                )));
            }
            add(board, &mut state, net, &rest);
        }
        if state == *routes {
            out.joined = true;
            return Ok(Ok(None));
        }
        Ok(Ok(Some(state)))
    }

    /// The pads of the net asked for (of its connection alone, when one is asked) and the centre
    /// of each.
    fn pads(&self) -> Vec<(PadRef, Pt)> {
        let net = self.board.nets.iter().find(|n| n.name == self.ask.net);
        net.map(|n| n.pads.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| self.ask.connection.as_ref().is_none_or(|c| c.contains(p)))
            .filter_map(|p| {
                let part = self.board.part(&p.refdes)?;
                let pad = part.footprint.pads_numbered(&p.number).next()?;
                let at = part.pad_centre(pad);
                Some((p, at))
            })
            .collect()
    }
}

/// `at (x, y)`, a place of the routes file.
fn at_said(at: [f64; 2]) -> String {
    format!("at ({}, {})", um(at[0]) + 0.0, um(at[1]) + 0.0)
}

/// The part that stands in for `via` (the `k`th asked for) in a run of the router: one plated
/// pad of the via's size and drill, on its layers, where it is (the module's header).
fn stand_in(board: &Board, via: &netsmith_pcb::Via, k: usize) -> netsmith_pcb::Placed {
    let layers: Vec<String> = board
        .via_layers(via)
        .iter()
        .map(|l| format!("\"{}\"", l.name()))
        .collect();
    let source = format!(
        "(footprint \"Via\" (version 20241229) (generator \"netsmith\") (layer \"F.Cu\") \
         (attr exclude_from_pos_files exclude_from_bom) \
         (pad \"1\" thru_hole circle (at 0 0) (size {d} {d}) (drill {h}) (layers {layers})))",
        d = via.diameter,
        h = via.drill,
        layers = layers.join(" "),
    );
    let footprint = Footprint::parse("Via", &source).expect("a via's footprint is well formed");
    let mut part = board.parts[0].clone();
    part.refdes = format!("~via{}", k + 1);
    part.path = part.refdes.clone();
    part.kind = "Via".to_string();
    part.provides = Vec::new();
    part.declared_kind = None;
    part.value = String::new();
    part.library_id = "netsmith:Via".to_string();
    part.footprint = Footprint {
        mount: Mount::ThroughHole,
        ..footprint
    };
    part.power_pads = Vec::new();
    part.at = via.at;
    part.rotation = 0.0;
    part.side = Side::Front;
    part.designator = None;
    part.apart = Vec::new();
    part.layout = Vec::new();
    part.contacts = Vec::new();
    part.symbol = None;
    part
}
