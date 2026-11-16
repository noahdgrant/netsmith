//! Building the engine from the parts and the problem.

use super::*;

impl<'a> Engine<'a> {
    pub(super) fn new(parts: &mut [Placed], p: &'a Problem<'a>) -> Engine<'a> {
        let n = parts.len();
        let rules = p.rules;
        let gap = GAP + if p.pours { POUR_GAP } else { 0.0 };
        let hole_gap = rules.clearance.max(rules.hole_to_hole).max(gap);
        let reach_pad = (hole_gap.max(rules.silk_to_pad)).max(gap + 2.0 * MAX_ESCAPE) / 2.0 + 0.01;
        let mut models: Vec<Model> = (0..n)
            .map(|i| {
                let base = parts[i].rotation;
                let angles = [base, base + 90.0, base + 180.0, base + 270.0];
                // A part that stays has the one shape it has; any other, every turn with the
                // text on every side.
                let mut shapes: Vec<Shape> = Vec::new();
                if p.fixed[i] {
                    let side = parts[i].reference_side;
                    shapes.push(shape_of(&mut parts[i], base, side, rules, reach_pad));
                } else {
                    for &a in &angles {
                        for text in ReferenceSide::ALL {
                            shapes.push(shape_of(&mut parts[i], a, text, rules, reach_pad));
                        }
                    }
                }
                let path = parts[i].path.clone();
                let carries: Vec<Carry> = p
                    .carried
                    .iter()
                    .enumerate()
                    .filter(|(_, z)| z.part == path)
                    .map(|(zone, z)| {
                        let mut boxes = Vec::new();
                        if p.fixed[i] {
                            boxes.push(carried_box(&mut parts[i], base, z.local));
                        } else {
                            for &a in &angles {
                                let at = carried_box(&mut parts[i], a, z.local);
                                boxes.extend(ReferenceSide::ALL.iter().map(|_| at));
                            }
                        }
                        Carry { zone, boxes }
                    })
                    .collect();
                Model {
                    shapes,
                    angles,
                    side: parts[i].side,
                    hole: parts[i].has_hole(),
                    protrusion: parts[i].protrusion.as_ref().map(|t| t.length),
                    height: parts[i].height,
                    hand_fitted: parts[i].hand_fitted,
                    court_faces: parts[i].footprint.courtyard_faces,
                    role: if crate::place::is_connector(&parts[i]) {
                        Role::Connector
                    } else if netsmith_pcb::is_mechanical(parts[i].declared_kind) {
                        Role::Mechanical
                    } else {
                        Role::Plain
                    },
                    edge_pads: edge_pads(&parts[i]),
                    fixed: p.fixed[i],
                    escape: (parts[i].footprint.pads.len() as f64 * ESCAPE_PER_PAD).min(MAX_ESCAPE),
                    want: p
                        .wants
                        .get(i)
                        .and_then(Option::as_ref)
                        .filter(|_| !p.fixed[i])
                        .map(|w| Limits::of(w, base)),
                    overhangs: p
                        .wants
                        .get(i)
                        .and_then(Option::as_ref)
                        .is_some_and(|w| w.overhangs),
                    may: p.overhangs.get(i).copied().unwrap_or_default(),
                    no_go: match parts[i].height {
                        Some(h) => p
                            .zones
                            .iter()
                            .filter(|z| z.rules_out(h, parts[i].side))
                            .map(|z| z.rect)
                            .collect(),
                        None => Vec::new(),
                    },
                    barred: p
                        .face_keepouts
                        .iter()
                        .filter(|k| k.keeps_out(parts[i].side))
                        .map(|k| k.rect)
                        .collect(),
                    body_is_courtyard: parts[i].footprint.body_is_courtyard,
                    carries,
                    rims: Vec::new(),
                }
            })
            .collect();
        let poses: Vec<Pose> = parts
            .iter()
            .map(|q| Pose {
                x: (q.at.x / UNIT).round() as i64,
                y: (q.at.y / UNIT).round() as i64,
                turn: 0,
                text: q.reference_side as u8,
            })
            .collect();

        // Nets, with the pads found once.
        let index: std::collections::HashMap<String, usize> = parts
            .iter()
            .enumerate()
            .map(|(i, q)| (q.refdes.clone(), i))
            .collect();
        let mut nets = Vec::new();
        let mut nets_of: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut neighbours: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
        for net in p.nets {
            let mut pads = Vec::new();
            for pad in &net.pads {
                let Some(&i) = index.get(pad.refdes.as_str()) else {
                    continue;
                };
                for (k, q) in parts[i].footprint.pads.iter().enumerate() {
                    if q.number == pad.number {
                        pads.push((i, k));
                    }
                }
            }
            if pads.len() < 2 {
                continue;
            }
            let plane = p.planes.contains(&net.name);
            let k = pads.len() as f64;
            let mut weight = (1.0 / (1.0 + 0.04 * (k - 2.0))).max(0.35);
            if plane {
                weight *= 0.15;
            }
            let id = nets.len();
            for &(i, _) in &pads {
                if !nets_of[i].contains(&id) {
                    nets_of[i].push(id);
                }
            }
            if !plane && pads.len() <= 8 {
                for &(a, _) in &pads {
                    for &(b, pb) in &pads {
                        if a != b {
                            neighbours[a].push((b, pb));
                        }
                    }
                }
            }
            let lines = !plane && pads.len() <= MAX_LINE_PADS;
            nets.push(NetModel {
                pads,
                weight,
                lines,
                plane,
            });
        }

        // The pads of each busy part that leave it: on a net that is not a plane and that
        // reaches another part ([`escape`]).
        let mut escapers = Vec::new();
        for i in 0..n {
            if models[i].escape < BUSY_ESCAPE {
                continue;
            }
            let mut leaves = vec![false; parts[i].footprint.pads.len()];
            for model in &nets {
                if model.plane || !model.pads.iter().any(|&(j, _)| j != i) {
                    continue;
                }
                for &(j, k) in &model.pads {
                    if j == i {
                        leaves[k] = true;
                    }
                }
            }
            let angles = if p.fixed[i] {
                vec![parts[i].rotation]
            } else {
                models[i].angles.to_vec()
            };
            let rims: Vec<Option<escape::Rim>> = angles
                .iter()
                .map(|&a| escape::Rim::of(&mut parts[i], a, &leaves))
                .collect();
            if rims.iter().any(Option::is_some) {
                models[i].rims = rims;
                escapers.push(i);
            }
        }

        // Decoupling pairs, with their pads.
        let pad_ids = |part: usize, number: &str| -> Vec<usize> {
            parts[part]
                .footprint
                .pads
                .iter()
                .enumerate()
                .filter(|(_, q)| q.number == number)
                .map(|(k, _)| k)
                .collect()
        };
        let mut decs: Vec<Dec> = decoupling_pairs(parts, p.nets)
            .iter()
            .map(|pair: &DecouplingPair| {
                let side = |s: &netsmith_pcb::PairNet| {
                    (
                        pad_ids(pair.cap, &s.cap_pad),
                        s.pins
                            .iter()
                            .flat_map(|(i, number)| {
                                pad_ids(*i, number).into_iter().map(move |k| (*i, k))
                            })
                            .collect::<Vec<_>>(),
                    )
                };
                Dec {
                    cap: pair.cap,
                    nets: [side(&pair.nets[0]), side(&pair.nets[1])],
                    beyond: None,
                }
            })
            .collect();
        // The parts a datasheet's `near` guideline finds (ADR 0030, issue #829), each pulled to
        // the pads of its pin as a decoupling capacitor is to its part's; those an `away` finds,
        // pushed out to its distance (issue #994).
        let pins_of = |part: usize, numbers: &[String]| -> Vec<(usize, usize)> {
            numbers
                .iter()
                .flat_map(|number| pad_ids(part, number).into_iter().map(move |k| (part, k)))
                .collect()
        };
        let pads_of = |part: usize, numbers: &[String]| -> Vec<usize> {
            numbers
                .iter()
                .flat_map(|number| pad_ids(part, number))
                .collect()
        };
        for (pair, beyond) in near_pairs(parts, p.nets)
            .into_iter()
            .map(|pair| (pair, None))
            .chain(away_pairs(parts, p.nets).into_iter().map(|pair| {
                let beyond = match pair.rule(parts).rule {
                    netsmith_core::LayoutRule::Away { beyond, .. } => Some(beyond),
                    _ => None,
                };
                (pair, beyond)
            }))
        {
            let other = pads_of(pair.other, &pair.other_pads);
            let to = pins_of(pair.part, &pair.to_pads);
            if other.is_empty() || to.is_empty() {
                continue;
            }
            decs.push(Dec {
                cap: pair.other,
                nets: [(other, to), (Vec::new(), Vec::new())],
                beyond,
            });
        }
        // The parts a `loop` finds: each pulled to both pins, as a decoupling capacitor is to
        // the two pins of its rail (issue #994).
        for pair in loop_pairs(parts, p.nets) {
            let a = (
                pads_of(pair.other, std::slice::from_ref(&pair.a.0)),
                pins_of(pair.part, &pair.a.1),
            );
            let b = (
                pads_of(pair.other, std::slice::from_ref(&pair.b.0)),
                pins_of(pair.part, &pair.b.1),
            );
            if a.0.is_empty() || a.1.is_empty() || b.0.is_empty() || b.1.is_empty() {
                continue;
            }
            decs.push(Dec {
                cap: pair.other,
                nets: [a, b],
                beyond: None,
            });
        }
        let mut decs_of: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (d, dec) in decs.iter().enumerate() {
            let mut involved = vec![dec.cap];
            involved.extend(
                dec.nets
                    .iter()
                    .flat_map(|(_, pins)| pins.iter().map(|&(i, _)| i)),
            );
            involved.sort_unstable();
            involved.dedup();
            for i in involved {
                decs_of[i].push(d);
            }
        }
        // Neighbours of a decoupled capacitor: the pins it belongs to.
        for dec in &decs {
            for (_, pins) in &dec.nets {
                for &(i, k) in pins {
                    neighbours[dec.cap].push((i, k));
                }
            }
        }

        // Module instances: each module the board places directly whose instance path two or
        // more parts share. A module nested in it (`reg.stage.divider`) has no group of its own:
        // its parts are already in their outer module's group, and a second pull on a few of
        // them moved every part of a board when a module started using a std one inside it.
        let mut by_prefix: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
        for (i, q) in parts.iter().enumerate() {
            let Some((outer, _)) = q.path.split_once('.') else {
                continue;
            };
            by_prefix.entry(outer.to_string()).or_default().push(i);
        }
        let groups: Vec<Vec<usize>> = by_prefix
            .into_values()
            .filter(|g| g.len() >= 2 && g.len() < n)
            .collect();
        let mut groups_of: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (g, members) in groups.iter().enumerate() {
            for &i in members {
                groups_of[i].push(g);
            }
        }
        let mut near_ok = vec![false; n * n];
        for dec in &decs {
            for (_, pins) in &dec.nets {
                for &(i, _) in pins {
                    near_ok[dec.cap * n + i] = true;
                    near_ok[i * n + dec.cap] = true;
                }
            }
        }
        let in_group: Vec<Vec<bool>> = groups
            .iter()
            .map(|members| (0..n).map(|i| members.contains(&i)).collect())
            .collect();
        let free: Vec<usize> = (0..n).filter(|&i| !p.fixed[i]).collect();

        // The relations between two parts: a relation whose part is not on the board has
        // nothing to keep.
        let mut links: Vec<Link> = Vec::new();
        for o in p.offsets {
            let (Some(&a), Some(&b)) = (index.get(o.a.as_str()), index.get(o.b.as_str())) else {
                continue;
            };
            let Some(axis) = constraint_axis_of(o.fa) else {
                continue;
            };
            links.push(Link {
                a,
                b,
                fa: o.fa,
                fb: o.fb,
                axis,
                lo: o.lo,
                hi: o.hi,
                label: o.label.clone(),
            });
        }
        let mut links_of: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (k, l) in links.iter().enumerate() {
            links_of[l.a].push(k);
            links_of[l.b].push(k);
        }
        // Which part follows which, per axis (a forest: a part has one leader, and the
        // relations that would close a loop only cost). The parts that stay, and the ones the
        // requirements pin along the axis, never follow.
        let pinned = |i: usize, axis: usize| {
            models[i].want.as_ref().is_some_and(|w| {
                w.turns.iter().flatten().any(|t| {
                    if axis == 0 {
                        t.pin_x.is_some()
                    } else {
                        t.pin_y.is_some()
                    }
                })
            })
        };
        let mut leader_of: Vec<[Option<usize>; 2]> = vec![[None; 2]; n];
        let mut root: [Vec<usize>; 2] = [(0..n).collect(), (0..n).collect()];
        fn find(root: &mut [usize], mut i: usize) -> usize {
            while root[i] != i {
                root[i] = root[root[i]];
                i = root[i];
            }
            i
        }
        for (k, l) in links.iter().enumerate() {
            if !l.needs_alignment() {
                continue;
            }
            let ax = l.axis;
            let (ra, rb) = (find(&mut root[ax], l.a), find(&mut root[ax], l.b));
            if ra == rb {
                continue;
            }
            let can = |i: usize| !p.fixed[i] && !pinned(i, ax) && leader_of[i][ax].is_none();
            let child = if can(l.b) {
                l.b
            } else if can(l.a) {
                l.a
            } else {
                continue;
            };
            leader_of[child][ax] = Some(k);
            let (rc, ro) = if child == l.b { (rb, ra) } else { (ra, rb) };
            root[ax][rc] = ro;
        }
        let mut order: Vec<(usize, usize, usize)> = Vec::new();
        for (i, leaders) in leader_of.iter().enumerate() {
            for ax in 0..2 {
                if leaders[ax].is_none() {
                    continue;
                }
                let mut depth = 0;
                let mut at = i;
                while let Some(k) = leader_of[at][ax] {
                    depth += 1;
                    at = if links[k].a == at {
                        links[k].b
                    } else {
                        links[k].a
                    };
                }
                order.push((depth, i, ax));
            }
        }
        order.sort_unstable();
        let order: Vec<(usize, usize)> = order.into_iter().map(|(_, i, ax)| (i, ax)).collect();
        // A part that stays is where it is, exactly, though its pose is on the grid: the
        // check measures it there, so its courtyard and pads are measured there too, whether
        // or not it is in a relation (#457).
        let mut sub = vec![Pt::default(); n];
        for (i, q) in parts.iter().enumerate() {
            if p.fixed[i] {
                sub[i] = Pt::new(
                    q.at.x - poses[i].x as f64 * UNIT,
                    q.at.y - poses[i].y as f64 * UNIT,
                );
            }
        }
        let link_mark = vec![0; links.len()];
        let (nn, nd, ng) = (nets.len(), decs.len(), groups.len());
        let carrier = models
            .iter()
            .map(|m| !m.carries.is_empty() || m.shapes.iter().any(|s| s.ring.is_some()))
            .collect();
        let mut engine = Engine {
            p,
            models,
            poses,
            nets,
            nets_of,
            decs,
            decs_of,
            groups,
            groups_of,
            in_group,
            neighbours,
            free,
            links,
            links_of,
            link_mark,
            leader_of,
            order,
            sub,
            lines: vec![Vec::new(); nn],
            line_box: vec![None; nn],
            net_mark: vec![0; nn],
            dec_mark: vec![0; nd],
            group_mark: vec![0; ng],
            part_mark: vec![0; n],
            stamp: 0,
            gap,
            hole_gap,
            near_ok,
            reach_abs: vec![Rect::new(Pt::default(), Pt::default()); n],
            grid: grid::Grid::new(carrier),
            pairs_costed: std::cell::Cell::new(0),
            poses_tried: 0,
            text_weight: TEXT_WEIGHT_FROM,
            text_last: false,
            bins: overflow::Bins::new(&p.region, rules, p.signal_layers, nn, n),
            w_overflow: p.overflow.max(0.0),
            escape_kept: vec![(0.0, Vec::new()); escapers.len()],
            escapers,
            w_escape: p.escape_room.max(0.0),
        };
        for i in 0..n {
            let pose = engine.clamp(i, engine.poses[i]);
            engine.put(i, pose);
        }
        // Parts that stand in a relation begin where it holds, as far as it can be made to.
        if !engine.links.is_empty() {
            engine.realign();
            for i in 0..n {
                let change = engine.expand(vec![(i, engine.poses[i])]);
                for (j, pose) in change {
                    engine.put(j, pose);
                }
            }
        }
        engine.refresh_all();
        engine
    }
}

/// The pads of a part that its datasheet puts at the board's edge (`edge`, issue #994), by
/// their index in the footprint.
fn edge_pads(part: &Placed) -> Vec<usize> {
    let numbers: Vec<&String> = part
        .layout
        .iter()
        .filter(|g| matches!(g.rule.rule, netsmith_core::LayoutRule::Edge { .. }))
        .flat_map(|g| g.pads.iter().flatten())
        .collect();
    part.footprint
        .pads
        .iter()
        .enumerate()
        .filter(|(_, q)| numbers.contains(&&q.number))
        .map(|(k, _)| k)
        .collect()
}
