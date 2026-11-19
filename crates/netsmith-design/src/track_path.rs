//! The lines of one impedance from a pin to a pin, through the parts in series on them (issue
//! #1275): each `std::sources::TrackPath` of the design.
//!
//! **Decided by the owner** (issue #1275): "A path interface joined with `~`:
//! `std::sources::TrackPath { .impedance; .budget_loss; .budget_length }` joined to the first
//! and last pin; each net between series parts takes the target; the budget is checked over the
//! chain; a corner style and a ground-via fencing distance are fields."
//!
//! **The chain** (the implementer's, within the owner's words). The nets of a path are found
//! from the net of `.first` to the net of `.last` over the parts in series: a part with exactly
//! two pins on nets, one on each of two nets. A rail's net (`Features::rail_nets`, a supply or a
//! ground) is never a link of the chain, so a shunt part of a matching network, from the line to
//! ground, is not taken for a series part and the chain never runs through the ground. The chain
//! is the shortest such way; when there is none, or two of the same length, the build says so
//! and names them, since a path the designer did not mean must not be held to the target.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use netsmith_check::{Domain, InstId, InstKind, Member, Model};
use netsmith_core::{ImpedanceTarget, TrackCorner, TrackPath};

use crate::features::{domain, exact, mm, point};

/// The part a pin is of (a pin of an interface of a part counts).
fn part_of(model: &Model, mut id: InstId) -> Option<InstId> {
    loop {
        let i = model.instance(id);
        if i.kind == InstKind::Part {
            return Some(id);
        }
        if !matches!(i.kind, InstKind::Pin | InstKind::Interface) {
            return None;
        }
        id = i.parent?;
    }
}

/// The net a pin member of `inst` is on, by index into `model.nets()`.
fn net_of(model: &Model, inst: InstId, pin: &str) -> Option<usize> {
    match model.member(inst, pin)? {
        Member::Inst(p) => model.nets().iter().position(|n| n.pins.contains(&p)),
        _ => None,
    }
}

/// The parts in series of the design: each part whose terminals on nets are two, on two nets,
/// with those two nets, by the part's path.
fn series_parts(model: &Model) -> BTreeMap<String, (usize, usize)> {
    let mut on: BTreeMap<InstId, Vec<usize>> = BTreeMap::new();
    for (k, net) in model.nets().iter().enumerate() {
        for &t in &net.terminals {
            if let Some(part) = part_of(model, t) {
                on.entry(part).or_default().push(k);
            }
        }
    }
    on.into_iter()
        .filter_map(|(part, nets)| match nets[..] {
            [a, b] if a != b => Some((model.path_of(part), (a, b))),
            _ => None,
        })
        .collect()
}

/// The top of a budget, in its base unit: `None` when it is unset; an error when it has no top
/// or a top not above 0.
fn top(
    model: &Model,
    inst: InstId,
    path: &str,
    name: &str,
    errors: &mut Vec<String>,
) -> Option<f64> {
    let Some(Domain::Quantity(q)) = domain(model, inst, name) else {
        return None;
    };
    let top = q.hi();
    if !top.is_finite() || top <= 0.0 {
        errors.push(format!(
            "`{path}.{name}` is {q}, and a budget is the most the path may have: give it a top above 0, `&= 0mm..25mm` for a length or `&= 0..0.5` dB for a loss"
        ));
        return None;
    }
    Some(top)
}

/// Each `TrackPath` of the design, in the order of the paths, with its chain of nets.
pub(crate) fn track_paths(
    model: &Model,
    rail_nets: &[String],
    errors: &mut Vec<String>,
) -> Vec<TrackPath> {
    let mut members: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Interface && i.type_name == "TrackPath")
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    members.sort();
    if members.is_empty() {
        return Vec::new();
    }
    let nets = model.nets();
    let rail: BTreeSet<usize> = nets
        .iter()
        .enumerate()
        .filter(|(_, n)| rail_nets.contains(&n.name))
        .map(|(k, _)| k)
        .collect();
    let series = series_parts(model);
    let mut found = Vec::new();
    for (path, id) in members {
        let ends: Vec<Option<usize>> = ["first", "last"]
            .iter()
            .map(|pin| {
                let on = net_of(model, id, pin);
                if on.is_none() {
                    errors.push(format!(
                        "`{path}.{pin}` is on no net: join it with `~` to the pin the line {} at",
                        if *pin == "first" { "starts" } else { "ends" }
                    ));
                }
                on
            })
            .collect();
        let ohms = point(model, id, "impedance", errors);
        if ohms.is_none()
            && domain(model, id, "impedance").is_none_or(|d| matches!(d, Domain::Unset))
        {
            errors.push(format!(
                "`{path}.impedance` is not stated: say it with `&= 50ohm`"
            ));
        }
        let budget_length = top(model, id, &path, "budget_length", errors).map(mm);
        let budget_loss = top(model, id, &path, "budget_loss", errors);
        let corner = match domain(model, id, "corner") {
            Some(Domain::Text(v)) => TrackCorner::of_variant(&v).unwrap_or_default(),
            _ => TrackCorner::Rounded,
        };
        let fence = match domain(model, id, "fence") {
            Some(Domain::Quantity(_)) => match exact(model, id, "fence", errors) {
                Some(f) if f > 0.0 => Some(f),
                Some(f) => {
                    errors.push(format!(
                        "`{path}.fence` is {f} mm: the vias of a fence are apart, say it with `&= 1.5mm`"
                    ));
                    None
                }
                None => None,
            },
            _ => None,
        };
        let (Some(ohms), Some(first), Some(last)) = (ohms, ends[0], ends[1]) else {
            continue;
        };
        if ohms <= 0.0 {
            errors.push(format!(
                "`{path}.impedance` is {ohms} ohm: a target is above 0"
            ));
            continue;
        }
        if let Some((pin, k)) = [("first", first), ("last", last)]
            .into_iter()
            .find(|(_, k)| rail.contains(k))
        {
            errors.push(format!(
                "`{path}.{pin}` is on `{}`, a rail's net (a supply or a ground), and a path is a signal line: join it to the pin the line {} at",
                nets[k].name,
                if pin == "first" { "starts" } else { "ends" }
            ));
            continue;
        }
        let Some((chain, parts)) = chain(&path, first, last, &series, &rail, model, errors) else {
            continue;
        };
        found.push(TrackPath {
            path,
            nets: chain.iter().map(|&k| nets[k].name.clone()).collect(),
            parts,
            ohms,
            budget_length,
            budget_loss,
            corner,
            fence,
        });
    }
    found
}

/// The shortest chain of nets from `first` to `last` over the parts in series, with the parts:
/// `None` after saying why when there is none or two of the same length.
fn chain(
    path: &str,
    first: usize,
    last: usize,
    series: &BTreeMap<String, (usize, usize)>,
    rail: &BTreeSet<usize>,
    model: &Model,
    errors: &mut Vec<String>,
) -> Option<(Vec<usize>, Vec<String>)> {
    let nets = model.nets();
    // Breadth first, counting the ways of the least length to each net.
    let mut depth: BTreeMap<usize, usize> = BTreeMap::from([(first, 0)]);
    let mut ways: BTreeMap<usize, usize> = BTreeMap::from([(first, 1)]);
    let mut back: BTreeMap<usize, Vec<(usize, &str)>> = BTreeMap::new();
    let mut queue = VecDeque::from([first]);
    while let Some(at) = queue.pop_front() {
        if at == last {
            continue;
        }
        let d = depth[&at];
        for (part, &(a, b)) in series {
            let next = match (a == at, b == at) {
                (true, _) => b,
                (_, true) => a,
                _ => continue,
            };
            if rail.contains(&next) {
                continue;
            }
            match depth.get(&next) {
                None => {
                    depth.insert(next, d + 1);
                    ways.insert(next, ways[&at]);
                    back.entry(next).or_default().push((at, part));
                    queue.push_back(next);
                }
                Some(&nd) if nd == d + 1 => {
                    *ways.get_mut(&next).unwrap() += ways[&at];
                    back.entry(next).or_default().push((at, part));
                }
                _ => {}
            }
        }
    }
    let Some(&n) = ways.get(&last) else {
        errors.push(format!(
            "`{path}` runs from the net `{}` to the net `{}`, and no chain of parts in series (two pins, one on each of two nets, none of them a rail's) joins the two: join `.first` and `.last` to the two ends of one line",
            nets[first].name, nets[last].name
        ));
        return None;
    };
    if n > 1 {
        let parts: Vec<&str> = back[&last].iter().map(|(_, p)| *p).collect();
        errors.push(format!(
            "`{path}` runs from the net `{}` to the net `{}` by {n} chains of parts in series of the same length (the last part is one of {}): a path is one line, so join `.first` and `.last` to the two ends of one",
            nets[first].name,
            nets[last].name,
            parts
                .iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        return None;
    }
    let mut chain = vec![last];
    let mut parts = Vec::new();
    let mut at = last;
    while at != first {
        let (prev, part) = back[&at][0];
        parts.push(part.to_string());
        chain.push(prev);
        at = prev;
    }
    chain.reverse();
    parts.reverse();
    Some((chain, parts))
}

/// The impedance target of each net of the paths (issue #1275), as a `TrackImpedance` of the net
/// would state it, for the nets no other target names; a net another target names at another
/// impedance is an error.
pub(crate) fn path_targets(
    paths: &[TrackPath],
    targets: &mut Vec<ImpedanceTarget>,
    errors: &mut Vec<String>,
) {
    for p in paths {
        for net in &p.nets {
            if let Some(t) = targets.iter().find(|t| t.nets.contains(net)) {
                if t.nets.len() > 1 || (t.ohms - p.ohms).abs() > 1e-9 {
                    errors.push(format!(
                        "the net `{net}` is on the path `{}`, of {} ohm, and `{}` states {} ohm for it: one net has one target",
                        p.path, p.ohms, t.path, t.ohms
                    ));
                }
                continue;
            }
            targets.push(ImpedanceTarget {
                path: p.path.clone(),
                nets: vec![net.clone()],
                ohms: p.ohms,
                spacing: None,
                skew: None,
            });
        }
    }
}
