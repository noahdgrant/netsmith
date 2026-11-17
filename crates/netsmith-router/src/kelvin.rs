//! Kelvin sense connections (issue #1148): a sense pin is routed from the pad it senses, alone.
//!
//! A part's `kelvin` statement names a sense pin and finds the pad it senses, the pad of another
//! part on the pin's net (`netsmith_pcb::kelvin_senses`). The router keeps the sense track apart from the
//! rest of the net by routing the two as two nets: while the board is routed, the sensed pad
//! is cut in two halves across its longer side, a gap between them, and the half nearer the sense
//! pin, with the sense pin's pads, is a net of its own (`NET~sense~U2.4`), while the other half
//! stays the net's. Every pass of the router then keeps the rules' space between the sense
//! track and the net's other copper, as it does between two nets, and the sense pin is the
//! only pad of its net besides the sensed pad, so no other line ends on it. When the router is done,
//! the pad is whole again and the sense copper is the net's: it touches the rest of the net in
//! the sensed pad only.
//!
//! The gap is the larger of the net's clearance and what lets a track of either net end at the
//! centre of its half and keep the space from the other half: with `c` the clearance, `w` the
//! wider of the two nets' tracks, `W` the pad's longer side and `e` the router's own margin
//! (0.15 mm), the gap `g` keeps `(W - g) / 4 + g >= c + w / 2 + e`.
//!
//! A net with a pour, a plane or a zone is cut the same way (issue #1174). While the board is
//! routed the sense net is another net to the pours and planes too: the pours the stitching and
//! the pour pads are judged by keep their clearance from the sense copper, and the sense pin's
//! pads get no via to the net's plane. When the board is put back, the board file carves the
//! pours round the sense copper (`netsmith_pcb::sense_carves`), so they keep that clearance in
//! the file, the Gerbers and KiCad's refill, and touch the sense copper nowhere.
//!
//! A sensed pad is not cut, and the sense pin is routed as any other line of its net (the check of
//! `netsmith_pcb::kelvin_findings` then says whether it is alone), when the statement finds no
//! part or several, when the net has copper already (a routes file's, or copper the router keeps)
//! or is locked, when the sensed pad has a hole or a custom shape, when a half would be narrower
//! than the process's least track, or when another sense pin senses the same pad.

use netsmith_pcb::{Board, BoardNet, CopperLayer, Footprint, PadRef, PadShape, Pt};

/// The router's own margin round a track's end, in mm: its margin and half a cell of the grid,
/// with room over.
const END_ROOM: f64 = 0.15;

/// What [`split`] changed on the board, to put back.
#[derive(Debug, Default)]
pub(crate) struct Split {
    /// The footprints of the sensed parts before, by part index.
    footprints: Vec<(usize, Footprint)>,
    /// The nets before.
    nets: Vec<BoardNet>,
    /// Each sense net's name and the net it is of.
    names: Vec<(String, String)>,
}

impl Split {
    pub(crate) fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// Cut the sensed pad of each sense pin the board has in two and make the sense pin and its half a
/// net of their own (see the module header). [`join`] puts the board back.
pub(crate) fn split(board: &mut Board) -> Split {
    let senses = netsmith_pcb::kelvin_senses(&board.parts, &board.nets);
    let mut out = Split::default();
    if senses.is_empty() {
        return out;
    }
    let mut cut: Vec<(usize, usize)> = Vec::new();
    let mut new_nets: Vec<BoardNet> = Vec::new();
    let mut leave: Vec<(usize, Vec<PadRef>)> = Vec::new();
    for kelvin in &senses {
        let (Some(n), Some(t)) = (kelvin.net, kelvin.sensed) else {
            continue;
        };
        let net = board.nets[n].name.clone();
        let has_copper =
            board.tracks.iter().any(|x| x.net == net) || board.vias.iter().any(|x| x.net == net);
        if has_copper || board.fixed.is_locked(&net) || cut.contains(&(t.part, t.pad)) {
            continue;
        }
        let sensing = &board.parts[kelvin.part];
        let sense_at: Vec<Pt> = kelvin
            .sense
            .iter()
            .flat_map(|x| sensing.footprint.pads_numbered(x))
            .map(|p| sensing.pad_centre(p))
            .collect();
        let sense_refdes = sensing.refdes.clone();
        let wide = board
            .track_width_on(&net, CopperLayer::Front)
            .max(board.rules.track_width);
        let clearance = board.clearance(&net);
        let least = board.rules.min_track_width;
        let part = &mut board.parts[t.part];
        let pad = part.footprint.pads[t.pad].clone();
        if pad.drill.is_some() || pad.extent.is_some() {
            continue;
        }
        let long = pad.width.max(pad.height);
        let gap = clearance.max(4.0 / 3.0 * (clearance + wide / 2.0 + END_ROOM - long / 4.0));
        let half = (long - gap) / 2.0;
        if half < least {
            continue;
        }
        // The two halves in the pad's own frame, along its longer side, turned with it.
        let along = if pad.width >= pad.height {
            Pt::new((half + gap) / 2.0, 0.0)
        } else {
            Pt::new(0.0, (half + gap) / 2.0)
        }
        .rotate(pad.angle);
        let halve = |sign: f64, number: String| {
            let mut p = pad.clone();
            p.number = number;
            p.at = Pt::new(pad.at.x + sign * along.x, pad.at.y + sign * along.y);
            if pad.width >= pad.height {
                p.width = half;
            } else {
                p.height = half;
            }
            if matches!(p.shape, PadShape::Circle | PadShape::Oval) {
                p.shape = PadShape::Rect;
            }
            p
        };
        let sense_number = format!("{}~sense~{sense_refdes}", pad.number);
        let (a, b) = (
            halve(1.0, pad.number.clone()),
            halve(-1.0, pad.number.clone()),
        );
        let nearer = |p: &netsmith_pcb::Pad| {
            let c = part.pad_centre(p);
            sense_at
                .iter()
                .map(|s| s.distance(c))
                .fold(f64::MAX, f64::min)
        };
        let (mut sense_half, net_half) = if nearer(&a) <= nearer(&b) {
            (a, b)
        } else {
            (b, a)
        };
        sense_half.number = sense_number.clone();
        if !out.footprints.iter().any(|(i, _)| *i == t.part) {
            out.footprints.push((t.part, part.footprint.clone()));
        }
        part.footprint.pads[t.pad] = net_half;
        part.footprint.pads.push(sense_half);
        cut.push((t.part, t.pad));
        let sense_pads: Vec<PadRef> = kelvin
            .sense
            .iter()
            .map(|number| PadRef {
                refdes: sense_refdes.clone(),
                number: number.clone(),
            })
            .collect();
        let name = format!("{net}~sense~{sense_refdes}.{}", kelvin.sense.join("."));
        let mut pads = sense_pads.clone();
        pads.push(PadRef {
            refdes: part.refdes.clone(),
            number: sense_number,
        });
        new_nets.push(BoardNet {
            name: name.clone(),
            pads,
        });
        leave.push((n, sense_pads));
        out.names.push((name, net));
    }
    if out.names.is_empty() {
        return out;
    }
    out.nets = board.nets.clone();
    for (n, pads) in leave {
        board.nets[n].pads.retain(|p| !pads.contains(p));
    }
    board.nets.extend(new_nets);
    out
}

/// Put back what [`split`] changed: the whole pads, the nets, and the sense copper on its net.
pub(crate) fn join(board: &mut Board, split: Split) {
    if split.is_empty() {
        return;
    }
    for (i, footprint) in split.footprints {
        board.parts[i].footprint = footprint;
    }
    board.nets = split.nets;
    let rename = |net: &mut String| {
        if let Some((_, of)) = split.names.iter().find(|(sense, _)| sense == net) {
            *net = of.clone();
        }
    };
    for t in &mut board.tracks {
        rename(&mut t.net);
    }
    for v in &mut board.vias {
        rename(&mut v.net);
    }
    for t in &mut board.teardrops {
        rename(&mut t.net);
    }
}
