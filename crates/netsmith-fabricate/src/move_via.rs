//! `netsmith routes move-via` (issue #1644): a via of the routes file moved, with every layer's
//! tracks of its net that end in it carried along, their ends laid again in legs at multiples of
//! 45 degrees, written to the routes file as the other `routes` commands write it.
//!
//! **Which tracks follow.** The straight tracks of the via's own record whose end lies in the
//! via's copper (no further from its centre than half its diameter), on a layer the via is on
//! (every layer for a through via, the layers of its span for a blind or a buried one). That end
//! moves with the via, keeping where it was in the via; a track with both ends in the via moves
//! whole. An arc that ends in it is refused (it cannot be laid again in straight legs: delete it,
//! or draw it straight, first), and so are a locked via and a locked track that would follow.
//!
//! **How a track is laid again.** From its end that stays (`A`) to its end in the via's new place
//! (`B`): one straight leg when `B - A` runs at a multiple of 45 degrees (within [`SKEW`] mm of
//! one), else two legs, along the two neighbouring multiples of 45 degrees between which `B - A`
//! runs, with a bend of 45 degrees between them: the router's corner, mitered (no arc is drawn;
//! `board fab` draws the routes file's copper as written). Of the two orders of the legs the one
//! that bends least where it leaves `A` is taken: where exactly one other straight track of the
//! net ends at `A` on its layer, the bend from it; else (a pad, a via, a joint of several tracks)
//! the turn from the track's old way. A bend at `A` of more than 45 degrees is refused, unless the
//! track bent as much there before (it is then no sharper than it was), so no move makes an acute
//! bend. The first leg keeps the track's number in the record; a second is added at the record's
//! end, with the track's width and unlocked.
//!
//! **Checked.** The via's move and the tracks' new legs are one edit of the routes file
//! ([`crate::routes::edit_all`]), refused as `routes add-track` refuses a track when the design
//! rules check finds a new error with it (a clearance to another net's copper, a short): the
//! refusal names the findings. It is refused too when the net has more connections left to route
//! after it than before ([`netsmith_pcb::Ratsnest`]): a move that loses a connection names it.
//! Nothing is written on a refusal, nor with `dry_run`.

use netsmith_pcb::{Board, CopperLayer, Ratsnest, RouteTrack, Routes};

use crate::routes::batch::Found;
use crate::routes::{Checking, Edit, EditCounts, Refusal, edit_all_with, summary, with_routes};

/// How far, in mm, the end of a track may be from a way at a multiple of 45 degrees and still be
/// laid as one leg (a second leg shorter than this is left out).
pub const SKEW: f64 = 0.001;

/// The most a new bend may turn where a track leaves its end that stays, in degrees, unless it
/// turned more there before.
pub const MOST_BEND: f64 = 45.0;

/// One track that follows the via, laid again.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Followed {
    /// Its number in the net's record, from 1, as `routes show` lists it.
    pub track: usize,
    pub layer: String,
    pub width: f64,
    /// Its way before, from its end that stays to its end in the via.
    pub way_before: Vec<[f64; 2]>,
    /// Its way after: two places for one leg, three for two (the bend between them).
    pub way_after: Vec<[f64; 2]>,
    /// How far the new way turns where it leaves its end that stays, in degrees (0 where nothing
    /// else of the net's tracks ends there and it keeps its old way).
    pub bend: f64,
}

/// What `routes move-via` did ([module](self)).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MovedVia {
    pub board: String,
    pub net: String,
    /// The via's number in the net's record, from 1.
    pub via: usize,
    pub from: [f64; 2],
    pub to: [f64; 2],
    /// The tracks that follow it, laid again.
    pub tracks: Vec<Followed>,
    /// Why it was refused, when it was: the findings of the check, a connection lost, a bend.
    pub refused: Option<String>,
    /// The new errors of the design rules check that refused it, in the routes file's frame.
    pub findings: Vec<Found>,
    /// What the edit said, when it was made.
    pub said: Option<String>,
    /// The new islands the via in its new place cuts a pour, a plane or a zone into, on a layer
    /// it passes ([`crate::via_pockets`]): a note, which does not refuse the move.
    pub pockets: Vec<crate::via_pockets::Pocket>,
    /// The routes file with the move, when it was not refused.
    #[serde(skip)]
    pub routes: Option<Routes>,
    /// The work of the check: the design rules checks made and the items they looked at.
    #[serde(skip)]
    pub counts: EditCounts,
}

impl MovedVia {
    /// The text for a person.
    pub fn text(&self) -> String {
        self.text_moved([0.0, 0.0])
    }

    /// [`MovedVia::text`] with the places moved by `by`, mm (into another frame than the routes
    /// file's).
    pub fn text_moved(&self, by: [f64; 2]) -> String {
        let at = |p: [f64; 2]| format!("{:.3},{:.3}", p[0] + by[0], p[1] + by[1]);
        let mut out = format!(
            "via {} of net {}: from {} to {}\n",
            self.via,
            self.net,
            at(self.from),
            at(self.to)
        );
        if self.tracks.is_empty() {
            out.push_str("no track ends in it\n");
        }
        for p in &self.pockets {
            out.push_str(&format!("note: the via {}\n", p.text_moved(by)));
        }
        for t in &self.tracks {
            let way = |w: &[[f64; 2]]| w.iter().map(|p| at(*p)).collect::<Vec<_>>().join(" -> ");
            out.push_str(&format!(
                "track {} on {} ({:.3} mm): {} now {}",
                t.track,
                t.layer,
                t.width,
                way(&t.way_before),
                way(&t.way_after)
            ));
            if t.bend > 1e-6 {
                out.push_str(&format!(" (bends {:.1} degrees at its end)", t.bend));
            }
            out.push('\n');
        }
        out
    }
}

/// Via `via` (numbered from 1) of `net`'s record in `routes`, moved to `to` (the routes file's
/// frame) on `board` (placed, with no routes on it), with the tracks that end in it, as the
/// [module](self) says. `Err` when it cannot be tried: no such net, record or via, a locked via
/// or track, an arc, or the board cannot be read; a move the check refuses is `Ok` with
/// [`MovedVia::refused`].
pub fn move_via(
    board: &Board,
    routes: &Routes,
    net: &str,
    via: usize,
    to: [f64; 2],
) -> Result<MovedVia, Refusal> {
    let refuse = |e: String| Refusal::Refused(e);
    if !board.nets.iter().any(|n| n.name == net) {
        return Err(refuse(format!(
            "the board {} has no net `{net}`",
            board.name
        )));
    }
    let record = routes
        .boards
        .get(&board.name)
        .and_then(|t| t.nets.get(net))
        .ok_or_else(|| {
            refuse(format!(
                "net {net} has no record in the routes file, so it has no via to move (`netsmith routes show` lists the records)"
            ))
        })?;
    let v = record.vias.get(via.wrapping_sub(1)).ok_or_else(|| {
        refuse(format!(
            "net {net} has no via {via}: its record has {} (`netsmith routes show` numbers them)",
            record.vias.len()
        ))
    })?;
    if v.lock {
        return Err(refuse(format!(
            "via {via} of net {net} is locked: `netsmith routes unlock --net {net} --via {via}` first"
        )));
    }
    let to = [to_um(to[0]), to_um(to[1])];
    let d = [to[0] - v.at[0], to[1] - v.at[1]];
    let layers = board.copper_count();
    let span = match v.layers.as_slice() {
        [a, b] => match (CopperLayer::of_word(a), CopperLayer::of_word(b)) {
            (Some(a), Some(b)) => Some(netsmith_pcb::ViaSpan::between(a, b, layers)),
            _ => None,
        },
        _ => None,
    };
    let on = |l: CopperLayer| span.is_none_or(|s| s.holds(l, layers));
    let inside = |p: [f64; 2]| dist(p, v.at) <= v.diameter / 2.0 + 1e-9;

    let mut changes = vec![Edit::MoveVia {
        net: net.to_string(),
        via,
        at: to,
    }];
    let mut added: Vec<Edit> = Vec::new();
    let mut followed = Vec::new();
    for (i, t) in record.tracks.iter().enumerate() {
        let n = i + 1;
        let (in_from, in_to) = (inside(t.from), inside(t.to));
        if !on(t.layer) || !(in_from || in_to) {
            continue;
        }
        if t.lock {
            return Err(refuse(format!(
                "track {n} of net {net}, which ends in via {via}, is locked: `netsmith routes unlock --net {net} --track {n}` first"
            )));
        }
        if t.mid.is_some() {
            return Err(refuse(format!(
                "track {n} of net {net}, which ends in via {via}, is an arc, which a move cannot lay again in straight legs: delete it (`netsmith routes delete --net {net} --track {n}`) or draw it straight first"
            )));
        }
        let shift = |p: [f64; 2]| [to_um(p[0] + d[0]), to_um(p[1] + d[1])];
        if in_from && in_to {
            changes.push(Edit::MoveTrack {
                net: net.to_string(),
                track: n,
                from: shift(t.from),
                mid: None,
                to: shift(t.to),
            });
            followed.push(Followed {
                track: n,
                layer: t.layer.word().to_string(),
                width: t.width,
                way_before: vec![t.from, t.to],
                way_after: vec![shift(t.from), shift(t.to)],
                bend: 0.0,
            });
            continue;
        }
        // From the end that stays to the end in the via.
        let (a, b) = if in_to {
            (t.from, t.to)
        } else {
            (t.to, t.from)
        };
        let b2 = shift(b);
        if dist(a, b2) < SKEW {
            return Err(refuse(format!(
                "track {n} of net {net} on {} would have no length with via {via} at {},{}: move the via elsewhere, or delete the track",
                t.layer.word(),
                to[0],
                to[1]
            )));
        }
        let incoming = incoming(&record.tracks, i, t.layer, a);
        let (way, bend) = lay(a, b, b2, incoming).map_err(|turn| {
            refuse(format!(
                "track {n} of net {net} on {} would bend by {turn:.1} degrees where it leaves {:.3},{:.3}, more than {MOST_BEND} degrees and more than it bent there before: move the via elsewhere, or lay the track by hand",
                t.layer.word(),
                a[0],
                a[1]
            ))
        })?;
        // The track keeps its number on its first leg from the end that stays; the end in the
        // via stays in the field it was in.
        let first = |p: [f64; 2], q: [f64; 2]| if in_to { (p, q) } else { (q, p) };
        let (from, to_) = first(way[0], way[1]);
        changes.push(Edit::MoveTrack {
            net: net.to_string(),
            track: n,
            from,
            mid: None,
            to: to_,
        });
        if way.len() == 3 {
            let (from, to_) = first(way[1], way[2]);
            added.push(Edit::AddTrack {
                net: net.to_string(),
                layer: t.layer,
                width: Some(t.width),
                from,
                mid: None,
                to: to_,
                lock: false,
            });
        }
        followed.push(Followed {
            track: n,
            layer: t.layer.word().to_string(),
            width: t.width,
            way_before: vec![a, b],
            way_after: way,
            bend,
        });
    }
    changes.extend(added);

    let mut moved = MovedVia {
        board: board.name.clone(),
        net: net.to_string(),
        via,
        from: v.at,
        to,
        tracks: followed,
        refused: None,
        findings: Vec::new(),
        said: None,
        pockets: Vec::new(),
        routes: None,
        counts: EditCounts::default(),
    };
    let mut next = routes.clone();
    let mut counts = EditCounts::default();
    let mut checking = Checking {
        counts: &mut counts,
        before: None,
        found: Vec::new(),
    };
    let made = edit_all_with(board, &mut next, &changes, &mut checking);
    let found: Vec<Found> = checking.found.iter().map(Found::of).collect();
    let checked = checking.before.take();
    moved.counts = counts;
    match made {
        Ok(said) => {
            // The connections of the net, before and after: none may be lost.
            let open = |b: &Board| -> Vec<String> {
                let mut l: Vec<String> = Ratsnest::of(b)
                    .lines
                    .iter()
                    .filter(|l| l.net == net)
                    .map(ToString::to_string)
                    .collect();
                l.sort();
                l
            };
            let after = match checked {
                Some(b) => b,
                None => with_routes(board, &next)?.0,
            };
            let before = with_routes(board, routes)?.0;
            let was = open(&before);
            let now = open(&after);
            if now.len() > was.len() {
                let lost = now.iter().find(|l| !was.contains(l)).unwrap_or(&now[0]);
                moved.refused = Some(format!(
                    "refused: it leaves a connection of net {net} open ({lost}), and the routes file is not changed"
                ));
            } else {
                moved.said = Some(summary(&said));
                moved.pockets = crate::via_pockets::Pockets::new().of_change_at(
                    &before,
                    &after,
                    &[netsmith_pcb::Pt::new(to[0], to[1]) + board.outline.min],
                    v.diameter,
                )?;
                moved.routes = Some(next);
            }
        }
        Err(Refusal::Refused(e)) => {
            moved.refused = Some(e);
            moved.findings = found;
        }
        Err(e) => return Err(e),
    }
    Ok(moved)
}

/// The way from `a` to `b2`, the end in the via moved from `b`, and the bend where it leaves
/// `a`: one leg or two ([module](self)); `Err` with the least bend when every way bends more than
/// it may.
fn lay(
    a: [f64; 2],
    b: [f64; 2],
    b2: [f64; 2],
    incoming: Option<[f64; 2]>,
) -> Result<(Vec<[f64; 2]>, f64), f64> {
    let d = [b2[0] - a[0], b2[1] - a[1]];
    let old = unit([b[0] - a[0], b[1] - a[1]]);
    // How far a way leaving `a` along `u` turns: from the track that comes in, else from the old
    // way, which is allowed whatever it was.
    let bend = |u: [f64; 2]| match incoming {
        Some(i) => turn(i, u),
        None => turn(old, u),
    };
    let allowed = match incoming {
        Some(i) => MOST_BEND.max(turn(i, old)),
        None => 180.0,
    } + 1e-6;
    let angle = d[1].atan2(d[0]).to_degrees().rem_euclid(360.0);
    let k = (angle / 45.0).floor();
    let u1 = way(k * 45.0);
    let u2 = way((k + 1.0) * 45.0);
    // d = p u1 + q u2.
    let det = u1[0] * u2[1] - u1[1] * u2[0];
    let p = (d[0] * u2[1] - d[1] * u2[0]) / det;
    let q = (u1[0] * d[1] - u1[1] * d[0]) / det;
    let mut ways: Vec<(Vec<[f64; 2]>, f64)> = Vec::new();
    if p < SKEW || q < SKEW {
        ways.push((vec![a, b2], bend(unit(d))));
    } else {
        let by = |u: [f64; 2], s: f64| [to_um(a[0] + u[0] * s), to_um(a[1] + u[1] * s)];
        ways.push((vec![a, by(u1, p), b2], bend(u1)));
        ways.push((vec![a, by(u2, q), b2], bend(u2)));
    }
    // The least bend at `a`, then the way nearest the track's old way.
    ways.sort_by(|x, y| {
        (x.1, turn(old, unit(sub(x.0[1], a))))
            .partial_cmp(&(y.1, turn(old, unit(sub(y.0[1], a)))))
            .expect("finite")
    });
    let (w, least) = ways.swap_remove(0);
    if least > allowed {
        return Err(least);
    }
    Ok((w, least))
}

/// The way of the one other straight track of the net on `layer` (not track `me`) that ends at
/// `a`, towards `a`: `None` when none or several do.
fn incoming(tracks: &[RouteTrack], me: usize, layer: CopperLayer, a: [f64; 2]) -> Option<[f64; 2]> {
    let mut found = tracks
        .iter()
        .enumerate()
        .filter(|(i, t)| *i != me && t.layer == layer && t.mid.is_none())
        .filter_map(|(_, t)| {
            if dist(t.to, a) < 1e-6 {
                Some(unit(sub(a, t.from)))
            } else if dist(t.from, a) < 1e-6 {
                Some(unit(sub(a, t.to)))
            } else {
                None
            }
        });
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// The unit way at `degrees` counterclockwise from +x in these coordinates (y down: clockwise on
/// screen), exact at the multiples of 90.
fn way(degrees: f64) -> [f64; 2] {
    let r = degrees.rem_euclid(360.0);
    let h = std::f64::consts::FRAC_1_SQRT_2;
    match (r / 45.0).round() as i64 % 8 {
        0 => [1.0, 0.0],
        1 => [h, h],
        2 => [0.0, 1.0],
        3 => [-h, h],
        4 => [-1.0, 0.0],
        5 => [-h, -h],
        6 => [0.0, -1.0],
        _ => [h, -h],
    }
}

/// How far a way along `u` turns into one along `v`, in degrees: 0 straight on.
fn turn(u: [f64; 2], v: [f64; 2]) -> f64 {
    (u[0] * v[0] + u[1] * v[1])
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees()
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn unit(p: [f64; 2]) -> [f64; 2] {
    let l = p[0].hypot(p[1]).max(1e-18);
    [p[0] / l, p[1] / l]
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn to_um(v: f64) -> f64 {
    (v * 1e6).round() / 1e6 + 0.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes_tidy::tests::{W, path, routes_of, track, via};
    use netsmith_pcb::NetRoutes;

    const TOP: CopperLayer = CopperLayer::Front;
    const BOTTOM: CopperLayer = CopperLayer::Back;
    const NET: &str = "net-led.anode";

    fn led() -> Board {
        crate::routes_tidy::tests::led()
    }

    /// The anode net from D1.2 up on the top to a via, along the bottom to a second via, and down
    /// on the top to R1.2.
    fn two_layers(board: &Board) -> Routes {
        let mut tracks = path(TOP, &[[13.3875, 4.2], [13.3875, 1.5]]);
        tracks.push(track(BOTTOM, [13.3875, 1.5], [9.61, 1.5]));
        tracks.extend(path(TOP, &[[9.61, 1.5], [9.61, 4.4]]));
        routes_of(
            board,
            vec![(
                NET,
                false,
                tracks,
                vec![via([13.3875, 1.5]), via([9.61, 1.5])],
            )],
        )
    }

    fn record<'a>(board: &Board, routes: &'a Routes, net: &str) -> &'a NetRoutes {
        &routes.boards[&board.name].nets[net]
    }

    fn open(board: &Board, routes: &Routes, net: &str) -> usize {
        Ratsnest::of(&with_routes(board, routes).unwrap().0)
            .lines
            .iter()
            .filter(|l| l.net == net)
            .count()
    }

    /// Every track runs at a multiple of 45 degrees and every two tracks of a layer that meet
    /// outside a via bend by 45 degrees at most.
    fn octilinear(board: &Board, r: &NetRoutes) {
        let vias: Vec<[f64; 2]> = r.vias.iter().map(|v| v.at).collect();
        for t in &r.tracks {
            let a = (t.to[1] - t.from[1])
                .atan2(t.to[0] - t.from[0])
                .to_degrees();
            let off = (a / 45.0 - (a / 45.0).round()).abs() * 45.0;
            assert!(off < 0.05, "{t:?} runs at {a} degrees on {}", board.name);
        }
        for (i, t) in r.tracks.iter().enumerate() {
            for u in &r.tracks[i + 1..] {
                if t.layer != u.layer {
                    continue;
                }
                for (p, tp) in [(t.from, t.to), (t.to, t.from)] {
                    for (q, uq) in [(u.from, u.to), (u.to, u.from)] {
                        if dist(p, q) > 1e-6 || vias.iter().any(|v| dist(*v, p) < 1e-6) {
                            continue;
                        }
                        let bend = turn(unit(sub(p, tp)), unit(sub(uq, q)));
                        assert!(bend <= MOST_BEND + 0.05, "{t:?} and {u:?} bend {bend}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_via_that_ends_tracks_on_two_layers_moves_with_both_in_45_degree_legs() {
        let board = led();
        let routes = two_layers(&board);
        assert_eq!(open(&board, &routes, NET), 0);
        let moved = move_via(&board, &routes, NET, 1, [12.8, 1.0]).unwrap();
        assert_eq!(moved.refused, None, "{}", moved.text());
        assert_eq!(moved.tracks.len(), 2, "{}", moved.text());
        let layers: Vec<&str> = moved.tracks.iter().map(|t| t.layer.as_str()).collect();
        assert_eq!(layers, ["top", "bottom"]);
        // Each follows in two legs that leave the end that stays the way it left it.
        for t in &moved.tracks {
            assert_eq!(t.way_after.len(), 3, "{}", moved.text());
            assert_eq!(t.bend, 0.0, "{}", moved.text());
            assert_eq!(*t.way_after.last().unwrap(), [12.8, 1.0]);
        }
        let after = moved.routes.as_ref().unwrap();
        let r = record(&board, after, NET);
        assert_eq!(r.vias[0].at, [12.8, 1.0]);
        assert_eq!(r.vias[1].at, [9.61, 1.5]);
        assert_eq!(r.tracks.len(), 5, "{r:#?}");
        assert!(r.tracks.iter().all(|t| t.width == W && !t.lock));
        octilinear(&board, r);
        // The net stays connected, and the check finds nothing new.
        assert_eq!(open(&board, after, NET), 0);
        let errors = crate::routes::new_errors(
            &with_routes(&board, &routes).unwrap().0,
            &with_routes(&board, after).unwrap().0,
        )
        .unwrap();
        assert!(errors.is_empty(), "{errors:#?}");
        // A move straight along a track keeps it one leg.
        let along = move_via(&board, &routes, NET, 1, [13.3875, 2.0]).unwrap();
        assert_eq!(along.refused, None, "{}", along.text());
        assert_eq!(
            along.tracks[0].way_after,
            vec![[13.3875, 4.2], [13.3875, 2.0]]
        );
    }

    #[test]
    fn a_move_into_another_nets_copper_is_refused_naming_it() {
        let board = led();
        let mut routes = two_layers(&board);
        let vcc = routes_of(
            &board,
            vec![(
                "VCC",
                false,
                vec![track(BOTTOM, [11.0, 0.6], [14.5, 0.6])],
                Vec::new(),
            )],
        );
        let r = vcc.boards[&board.name].nets["VCC"].clone();
        routes
            .boards
            .get_mut(&board.name)
            .unwrap()
            .nets
            .insert("VCC".to_string(), r);
        let moved = move_via(&board, &routes, NET, 1, [13.3875, 0.7]).unwrap();
        let refused = moved.refused.clone().expect("refused");
        assert!(refused.contains("VCC"), "{refused}");
        assert!(!moved.findings.is_empty(), "{refused}");
        assert!(
            moved
                .findings
                .iter()
                .any(|f| f.shapes.iter().any(|s| s.contains("VCC"))),
            "{:#?}",
            moved.findings
        );
        assert_eq!(moved.routes, None);
    }

    #[test]
    fn a_locked_via_or_track_is_refused() {
        let board = led();
        let mut routes = two_layers(&board);
        let table = routes.boards.get_mut(&board.name).unwrap();
        table.nets.get_mut(NET).unwrap().tracks[1].lock = true;
        let e = move_via(&board, &routes, NET, 1, [12.8, 1.0]).unwrap_err();
        assert!(
            e.to_string()
                .contains("track 2 of net net-led.anode, which ends in via 1, is locked"),
            "{e}"
        );
        assert!(
            e.to_string()
                .contains("routes unlock --net net-led.anode --track 2")
        );
        let mut routes = two_layers(&board);
        let table = routes.boards.get_mut(&board.name).unwrap();
        table.nets.get_mut(NET).unwrap().vias[0].lock = true;
        let e = move_via(&board, &routes, NET, 1, [12.8, 1.0]).unwrap_err();
        assert!(
            e.to_string()
                .contains("via 1 of net net-led.anode is locked"),
            "{e}"
        );
        // No such via.
        let e = move_via(&board, &two_layers(&board), NET, 3, [1.0, 1.0]).unwrap_err();
        assert!(e.to_string().contains("has no via 3"), "{e}");
    }

    #[test]
    fn a_way_bends_45_degrees_at_most_where_it_leaves_a_track() {
        // A track that comes in heading east at (0, 0); the via was 2 mm on, now 1 mm south of
        // there and a little east: straight south first would turn 90, so the diagonal first.
        let (w, bend) = lay([0.0, 0.0], [2.0, 0.0], [0.3, 1.0], Some([1.0, 0.0])).unwrap();
        assert!((bend - 45.0).abs() < 1e-6, "{w:?} {bend}");
        assert_eq!(w.len(), 3);
        // A move behind the joint would double back: refused.
        assert!(lay([0.0, 0.0], [2.0, 0.0], [-1.0, 0.1], Some([1.0, 0.0])).is_err());
    }
}
