//! The power tree, read from the solved model (ADR 0007, ADR 0014): a node for each net
//! that a rail draws from, the currents on it, and what the part that supplies it can give.

use netsmith_check::{Domain, InstId, InstKind, Member, Model, Sources};
use netsmith_editor::{
    ApiError,
    api::{Budget, Draw, Edge, EdgeKind, PowerNode, PowerTree, Range, Stage},
};
use netsmith_quantity::{Dimension, Quantity};

use super::analysis::{Snapshot, api_location};
use crate::locate;

/// The power tree of a board by its name, or of a system by its name, which shows every
/// board in it and the joins between them.
pub fn tree(snapshot: &Snapshot, name: &str) -> Result<PowerTree, ApiError> {
    if let Some(system) = snapshot.system(name) {
        let solved = &system.solved;
        let scopes: Vec<(InstId, Option<String>)> = solved
            .boards
            .iter()
            .map(|(member, _, inst)| (*inst, Some(member.clone())))
            .collect();
        let mut tree = build(&solved.model, &solved.sources, &scopes);
        for &(a, b) in solved.model.joins() {
            let (from, to) = (
                node_of(&solved.model, &scopes, a),
                node_of(&solved.model, &scopes, b),
            );
            if let (Some(from), Some(to)) = (from, to) {
                let label = format!("{} ~ {}", solved.model.path_of(a), solved.model.path_of(b));
                tree.edges.push(Edge {
                    from,
                    to,
                    label,
                    kind: EdgeKind::Join,
                    at: None,
                });
            }
        }
        classify(&mut tree);
        tree.title = name.to_string();
        return Ok(tree);
    }
    let data = snapshot.board(name)?;
    let trace = &data.built.trace;
    let scopes = [(trace.model.root, None)];
    let mut tree = build(&trace.model, &trace.sources, &scopes);
    tree.title = name.to_string();
    Ok(tree)
}

/// The id of the node on the net a port's `pos` pin is on, if the net has a node.
fn node_of(model: &Model, scopes: &[(InstId, Option<String>)], port: InstId) -> Option<String> {
    let (_, pos, _) = model.rail_of(port)?;
    let net = model.nets().iter().find(|n| n.pins.contains(&pos))?;
    let board = model.board_of(port)?;
    let (_, prefix) = scopes.iter().find(|(inst, _)| *inst == board)?;
    Some(node_id(prefix.as_deref(), &net.name))
}

fn node_id(prefix: Option<&str>, net: &str) -> String {
    match prefix {
        Some(prefix) => format!("{prefix}:{net}"),
        None => net.to_string(),
    }
}

/// Amperes below which an end of a current range is residue of the arithmetic, not a current:
/// a supply whose load can be nothing has a range that ends at zero, and the solver's end of it
/// can be a hair above (`5e-324`), which must not turn the supply into a load.
const NEGLIGIBLE: f64 = 1e-12;

/// Whether a current range is a supply's: it ends at zero or below and starts below it.
fn supplies(q: &Quantity) -> bool {
    q.hi() <= NEGLIGIBLE && q.lo() < -NEGLIGIBLE
}

/// Whether a current range is a draw's: it starts at zero or above and ends above it.
fn draws(q: &Quantity) -> bool {
    q.lo() >= -NEGLIGIBLE && q.hi() > NEGLIGIBLE
}

fn range_of(q: &Quantity) -> Range {
    Range {
        lo: q.lo(),
        hi: q.hi(),
        text: q.to_string(),
    }
}

/// A field's quantity, if the model settled it.
fn quantity(model: &Model, owner: InstId, field: &str) -> Option<Quantity> {
    match model.member(owner, field)? {
        Member::Field(f) => match &model.field(f).domain {
            Domain::Quantity(q) => Some(*q),
            _ => None,
        },
        _ => None,
    }
}

fn sum(dim: Dimension, ranges: impl Iterator<Item = (f64, f64)>) -> Option<Quantity> {
    let (lo, hi) = ranges.fold((0.0, 0.0), |(a, b), (l, h)| (a + l, b + h));
    Quantity::interval(dim, lo, hi).ok()
}

fn build(model: &Model, sources: &Sources, scopes: &[(InstId, Option<String>)]) -> PowerTree {
    let mut nodes: Vec<PowerNode> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut stages: Vec<Stage> = Vec::new();
    // For each node, the capacity edges give it.
    let mut capacity: Vec<(String, f64, String, InstId)> = Vec::new();

    for (scope, prefix) in scopes {
        let prefix = prefix.as_deref();
        for balance in model
            .balances()
            .iter()
            .filter(|b| b.scope == *scope && !b.returns)
        {
            let mut node = PowerNode {
                id: node_id(prefix, &balance.net),
                net: balance.net.clone(),
                board: prefix.map(str::to_string),
                voltage: None,
                loads: Vec::new(),
                supplies: Vec::new(),
                demand: None,
                capacity: None,
                capacity_from: None,
                budget: Budget::Unknown,
            };
            for draw in &balance.draws {
                let field = model.field(draw.field);
                let owner = field.owner;
                let is_rail = model.instance(owner).kind == InstKind::Interface;
                let range = match &field.domain {
                    Domain::Quantity(q) if draw.flipped => {
                        Quantity::interval(q.dim(), -q.hi(), -q.lo()).ok()
                    }
                    Domain::Quantity(q) => Some(*q),
                    _ => None,
                };
                if node.voltage.is_none() && is_rail {
                    node.voltage = quantity(model, owner, "voltage").map(|q| range_of(&q));
                }
                let (src, span) = model.instance(owner).at();
                let item = Draw {
                    who: model.path_below(owner, *scope),
                    kind: if is_rail { "rail" } else { "chain" }.to_string(),
                    range: range.as_ref().map(range_of),
                    at: Some(api_location(locate(sources, src, span))),
                };
                match &range {
                    Some(q) if supplies(q) => node.supplies.push(item),
                    _ => node.loads.push(item),
                }
            }
            nodes.push(node);
        }

        // A module or part that takes one rail and makes another is an edge.
        for (id, inst) in model.instances() {
            if !model.is_inside(id, *scope)
                || id == *scope
                || !matches!(inst.kind, InstKind::Module | InstKind::Part)
            {
                continue;
            }
            let rails: Vec<InstId> = inst
                .children
                .iter()
                .filter_map(|(_, m)| match m {
                    Member::Inst(i)
                        if model.rail_of(*i).is_some() && model.draw_of(*i).is_some() =>
                    {
                        Some(*i)
                    }
                    _ => None,
                })
                .collect();
            let current = |rail: InstId| quantity(model, rail, "current");
            let inputs: Vec<InstId> = rails
                .iter()
                .copied()
                .filter(|r| current(*r).is_some_and(|q| draws(&q)))
                .collect();
            let outputs: Vec<InstId> = rails
                .iter()
                .copied()
                .filter(|r| current(*r).is_some_and(|q| supplies(&q)))
                .collect();
            // Every rail it takes feeds every rail it makes.
            for (&input, &output) in inputs
                .iter()
                .flat_map(|i| outputs.iter().map(move |o| (i, o)))
            {
                let net_of = |rail: InstId| {
                    let (_, pos, _) = model.rail_of(rail)?;
                    model.nets().iter().find(|n| n.pins.contains(&pos))
                };
                let (Some(from), Some(to)) = (net_of(input), net_of(output)) else {
                    continue;
                };
                let (from, to) = (node_id(prefix, &from.name), node_id(prefix, &to.name));
                let label = format!("{} ({})", model.path_below(id, *scope), inst.type_name);
                // The shallowest instance names the edge: a stage, not the regulator in it.
                let depth = model.path_below(id, *scope).matches('.').count();
                if let Some(i) = edges.iter().position(|e| e.from == from && e.to == to) {
                    let kept = edges[i].label.matches('.').count();
                    if depth >= kept {
                        maybe_capacity(model, *scope, id, &mut capacity, &to);
                        continue;
                    }
                    edges.remove(i);
                }
                maybe_capacity(model, *scope, id, &mut capacity, &to);
                let (src, span) = inst.at();
                stages.retain(|s| s.label != label);
                stages.push(stage_of(model, id, &label));
                edges.push(Edge {
                    from,
                    to,
                    label,
                    kind: EdgeKind::Converts,
                    at: Some(api_location(locate(sources, src, span))),
                });
            }
        }
    }
    let mut tree = PowerTree {
        title: String::new(),
        nodes,
        edges,
        stages,
    };
    for (node, limit, from, _) in capacity {
        if let Some(n) = tree.nodes.iter_mut().find(|n| n.id == node) {
            let total = n.capacity.as_ref().map_or(0.0, |c| c.hi) + limit;
            n.capacity = Quantity::interval(Dimension::CURRENT, 0.0, total)
                .ok()
                .map(|q| range_of(&q));
            n.capacity_from = Some(match &n.capacity_from {
                Some(old) => format!("{old}, {from}"),
                None => from,
            });
        }
    }
    classify(&mut tree);
    tree
}

/// What the solver settled for the efficiency of the converter `inst`: the first field called
/// `efficiency` on it or on something inside it.
fn stage_of(model: &Model, inst: InstId, label: &str) -> Stage {
    let holder = std::iter::once(inst)
        .chain(
            model
                .instances()
                .filter(|(i, _)| model.is_inside(*i, inst))
                .map(|(i, _)| i),
        )
        .find_map(|i| match model.member(i, "efficiency") {
            Some(Member::Field(f)) => Some(f),
            _ => None,
        });
    let settled = holder.and_then(|f| match &model.field(f).domain {
        Domain::Quantity(q) => Some(range_of(q)),
        _ => None,
    });
    Stage {
        label: label.to_string(),
        efficiency_unsettled: holder.is_some() && settled.is_none(),
        efficiency: settled,
    }
}

/// Record what `inst`, or a part inside it, can give: its `max_current`.
fn maybe_capacity(
    model: &Model,
    scope: InstId,
    inst: InstId,
    capacity: &mut Vec<(String, f64, String, InstId)>,
    node: &str,
) {
    let holder = std::iter::once(inst)
        .chain(
            model
                .instances()
                .filter(|(i, _)| model.is_inside(*i, inst))
                .map(|(i, _)| i),
        )
        .find_map(|i| quantity(model, i, "max_current").map(|q| (i, q)));
    // A stage and the regulator in it are one supply, so it is counted once.
    if let Some((holder, q)) = holder
        && !capacity
            .iter()
            .any(|(n, _, _, h)| n == node && *h == holder)
    {
        let label = format!(
            "{} ({})",
            model.path_below(holder, scope),
            model.instance(holder).type_name
        );
        capacity.push((node.to_string(), q.hi(), label, holder));
    }
}

/// Fill each node's demand and say whether the supply can give it.
fn classify(tree: &mut PowerTree) {
    for node in &mut tree.nodes {
        let known = node.loads.iter().all(|d| d.range.is_some());
        node.demand = known
            .then(|| {
                sum(
                    Dimension::CURRENT,
                    node.loads
                        .iter()
                        .filter_map(|d| d.range.as_ref())
                        .map(|r| (r.lo, r.hi)),
                )
            })
            .flatten()
            .map(|q| range_of(&q));
        node.budget = match (&node.demand, &node.capacity) {
            (None, _) => Budget::Unknown,
            (Some(_), None) => Budget::Unlimited,
            (Some(d), Some(c)) if d.hi <= c.hi * (1.0 + 1e-9) => Budget::Within,
            (Some(_), Some(_)) => Budget::Over,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(lo: f64, hi: f64) -> Range {
        Range {
            lo,
            hi,
            text: format!("{lo}..{hi}"),
        }
    }

    fn draw(who: &str, lo: f64, hi: f64) -> Draw {
        Draw {
            who: who.into(),
            kind: "rail".into(),
            range: Some(range(lo, hi)),
            at: None,
        }
    }

    fn node(loads: Vec<Draw>, capacity: Option<Range>) -> PowerNode {
        PowerNode {
            id: "n".into(),
            net: "n".into(),
            board: None,
            voltage: None,
            loads,
            supplies: Vec::new(),
            demand: None,
            capacity,
            capacity_from: None,
            budget: Budget::Unknown,
        }
    }

    fn budget(n: PowerNode) -> (Budget, Option<f64>) {
        let mut tree = PowerTree {
            title: String::new(),
            nodes: vec![n],
            edges: Vec::new(),
            stages: Vec::new(),
        };
        classify(&mut tree);
        (
            tree.nodes[0].budget,
            tree.nodes[0].demand.as_ref().map(|d| d.hi),
        )
    }

    #[test]
    fn demand_is_the_sum_of_the_loads_and_is_weighed_against_the_supply() {
        let loads = || vec![draw("a", 0.0, 0.06), draw("b", 0.01, 0.05)];
        // 110 mA at the most, from a supply of 100 mA.
        assert_eq!(
            budget(node(loads(), Some(range(0.0, 0.1)))),
            (Budget::Over, Some(0.11))
        );
        assert_eq!(
            budget(node(loads(), Some(range(0.0, 0.11)))).0,
            Budget::Within
        );
        assert_eq!(budget(node(loads(), None)).0, Budget::Unlimited);
    }

    #[test]
    fn a_supply_whose_range_ends_a_hair_above_zero_is_still_a_supply() {
        let tiny = Quantity::interval(Dimension::CURRENT, -0.19, 5e-324).unwrap();
        assert!(supplies(&tiny) && !draws(&tiny));
        let load = Quantity::interval(Dimension::CURRENT, 0.0, 0.04).unwrap();
        assert!(draws(&load) && !supplies(&load));
        let nothing = Quantity::interval(Dimension::CURRENT, 0.0, 0.0).unwrap();
        assert!(!draws(&nothing) && !supplies(&nothing));
        let mixed = Quantity::interval(Dimension::CURRENT, -0.01, 0.01).unwrap();
        assert!(!draws(&mixed) && !supplies(&mixed));
    }

    #[test]
    fn a_current_the_solver_has_not_settled_leaves_the_budget_unknown() {
        let mut unsettled = draw("c", 0.0, 0.0);
        unsettled.range = None;
        let (budget, demand) = budget(node(
            vec![draw("a", 0.0, 0.01), unsettled],
            Some(range(0.0, 1.0)),
        ));
        assert_eq!((budget, demand), (Budget::Unknown, None));
    }
}
