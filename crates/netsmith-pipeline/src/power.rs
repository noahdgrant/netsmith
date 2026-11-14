//! The power tree, read from the solved model (ADR 0007, ADR 0014): a node for each net
//! that a rail draws from, the currents on it, and what the part that supplies it can give.
//!
//! A node is a net that a rail draws from through `pos`, with the voltage the rails there agree
//! on; the draws on it, each with its range and the part it belongs to; their sum, the demand;
//! and the capacity, the `max_current` of the part whose rail supplies the net, where it has one.
//! An edge joins the input rail's net of a part (or a module) to its output rail's net, labelled
//! with the part, and a join in a system joins two ports' nets. A node has an issue when the
//! demand's top is above the capacity, and says the numbers. The tree is read from the solved
//! model and not from text (ADR 0019); the page lays the cards out (a rail one column right of
//! the rails that feed it, a column of many cards cut into lanes, loads heaviest first).

use netsmith_check::{Domain, InstId, InstKind, Member, Model, Sources};
use netsmith_editor::api::{
    Budget, Draw, Edge, EdgeKind, Location, PowerIssue, PowerNode, PowerTree, Range, Severity,
    Stage,
};
use netsmith_quantity::{Dimension, Quantity};

use crate::locate;

/// A location of the build as the editor's API has it.
pub fn api_location(l: crate::Location) -> Location {
    Location {
        file: l.file,
        line: l.line as u32,
        column: l.column as u32,
        end_line: l.end_line as u32,
        end_column: l.end_column as u32,
    }
}

/// The power tree of one board's model, with its issues.
pub fn board_tree(model: &Model, sources: &Sources) -> PowerTree {
    build(model, sources, &[(model.root, None)])
}

/// The power tree of a system's model: each board a scope, and the joins of their ports as
/// edges. Its issues are the whole system's, which a board alone cannot see (a rail that a
/// port of another board supplies has a source here).
pub fn system_tree(
    model: &Model,
    sources: &Sources,
    scopes: &[(InstId, Option<String>)],
) -> PowerTree {
    let mut tree = build(model, sources, scopes);
    for &(a, b) in model.joins() {
        let (from, to) = (node_of(model, scopes, a), node_of(model, scopes, b));
        if let (Some(from), Some(to)) = (from, to) {
            let label = format!("{} ~ {}", model.path_of(a), model.path_of(b));
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
    tree.issues = issues_of(&tree);
    tree
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
                at: None,
            };
            // The reference rail of a bus's levels that nothing else is on is a reference for the
            // checker and carries nothing (`Levels` states `.reference.current &= 0A`): it is no
            // rail of the board's power, and "no source" would only say that a part's levels are
            // joined to no supply.
            let only_references = !balance.draws.is_empty()
                && balance.draws.iter().all(|d| {
                    let owner = model.field(d.field).owner;
                    model
                        .instance(owner)
                        .parent
                        .is_some_and(|p| model.is_levels(p))
                });
            if only_references {
                continue;
            }
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
                        if model.rail_of(*i).is_some()
                            && model.draw_of(*i).is_some()
                            && model.unused_by(*i).is_none() =>
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
                let at = api_location(locate(sources, src, span));
                let mut stage = stage_of(model, id, &label);
                stage.at = Some(at.clone());
                stages.push(stage);
                edges.push(Edge {
                    from,
                    to,
                    label,
                    kind: EdgeKind::Converts,
                    at: Some(at),
                });
            }
        }
    }
    let mut tree = PowerTree {
        title: String::new(),
        nodes,
        edges,
        stages,
        issues: Vec::new(),
    };
    for (node, limit, from, holder) in capacity {
        if let Some(n) = tree.nodes.iter_mut().find(|n| n.id == node) {
            if n.at.is_none() {
                let (src, span) = model.instance(holder).at();
                n.at = Some(api_location(locate(sources, src, span)));
            }
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
    // Where to look when there is no supply that says what it can give: at what feeds the
    // net, else at what draws from it.
    for node in &mut tree.nodes {
        if node.at.is_none() {
            node.at = node
                .supplies
                .iter()
                .chain(&node.loads)
                .find_map(|d| d.at.clone());
        }
    }
    classify(&mut tree);
    tree.issues = issues_of(&tree);
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
        at: None,
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

/// An amount of current as the editor writes it: the biggest unit it is at least one of, three
/// significant digits, no trailing zeros (`84 mA`, `2.5 A`, `0 A`).
pub fn amps_text(x: f64) -> String {
    if !x.is_finite() {
        return "?".to_string();
    }
    let a = x.abs();
    if a <= NEGLIGIBLE {
        return "0 A".to_string();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // Rounding to three digits can carry into the next unit (999.6 mA is 1 A).
    for (unit, per) in [("A", 1.0), ("mA", 1e-3), ("uA", 1e-6), ("nA", 1e-9)] {
        let v = a / per;
        if v >= 0.9995 || unit == "nA" {
            let rounded: f64 = format!("{v:.2e}").parse().unwrap_or(v);
            return format!("{sign}{rounded} {unit}");
        }
    }
    "0 A".to_string()
}

/// The name of a converter in an edge's label (`ldo (LdoStage)` is `ldo`).
fn stage_name(label: &str) -> &str {
    match label.find(" (") {
        Some(open) if label.ends_with(')') => &label[..open],
        _ => label,
    }
}

/// The rails and converters with trouble, worst first, then in name order: a rail that can
/// draw more than its supply can give (an error), a rail nothing supplies, a rail with a load
/// whose current the solver did not settle, and a converter with an efficiency it did not
/// settle (notes). The page draws these; it does not work them out.
pub fn issues_of(tree: &PowerTree) -> Vec<PowerIssue> {
    use std::collections::{BTreeMap, BTreeSet};

    let known: BTreeSet<&str> = tree.nodes.iter().map(|n| n.id.as_str()).collect();
    let info: BTreeMap<&str, &Stage> = tree.stages.iter().map(|s| (s.label.as_str(), s)).collect();
    // One stage for each label: a converter with two outputs is one box.
    struct One<'a> {
        name: &'a str,
        from: Vec<&'a str>,
        at: Option<Location>,
        unsettled: bool,
    }
    let mut edges: Vec<&Edge> = tree
        .edges
        .iter()
        .filter(|e| known.contains(e.from.as_str()) && known.contains(e.to.as_str()))
        .collect();
    edges.sort_by(|a, b| (&a.label, &a.from, &a.to).cmp(&(&b.label, &b.from, &b.to)));
    let mut stages: BTreeMap<&str, One> = BTreeMap::new();
    let mut supplied: BTreeSet<&str> = BTreeSet::new();
    for e in edges {
        let one = stages.entry(e.label.as_str()).or_insert_with(|| {
            let stage = info.get(e.label.as_str());
            One {
                name: stage_name(&e.label),
                from: Vec::new(),
                at: stage.and_then(|s| s.at.clone()).or_else(|| e.at.clone()),
                unsettled: stage.is_some_and(|s| s.efficiency_unsettled),
            }
        });
        if !one.from.contains(&e.from.as_str()) {
            one.from.push(e.from.as_str());
        }
        supplied.insert(e.to.as_str());
    }

    let mut out: Vec<PowerIssue> = Vec::new();
    let mut nodes: Vec<&PowerNode> = tree.nodes.iter().collect();
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    for n in nodes {
        // The converters this rail feeds draw from it, and are not loads of its own.
        let feeds: BTreeSet<&str> = stages
            .values()
            .filter(|s| s.from.contains(&n.id.as_str()))
            .map(|s| s.name)
            .collect();
        let is_feed = |who: &str| {
            who.rfind('.')
                .is_some_and(|dot| dot > 0 && feeds.contains(&who[..dot]))
        };
        let loads: Vec<&Draw> = n.loads.iter().filter(|d| !is_feed(&d.who)).collect();
        let unknowns = loads.iter().filter(|d| d.range.is_none()).count();
        let sourced = !n.supplies.is_empty() || supplied.contains(n.id.as_str());
        // The node's place; a node with none has its first draw's.
        let at =
            n.at.clone()
                .or_else(|| n.supplies.iter().find_map(|d| d.at.clone()))
                .or_else(|| loads.iter().find_map(|d| d.at.clone()));
        let mut issue = |severity: Severity, text: String| {
            out.push(PowerIssue {
                id: n.id.clone(),
                severity,
                subject: n.net.clone(),
                text,
                at: at.clone(),
                board: n.board.clone(),
            });
        };
        if let (Budget::Over, Some(demand), Some(capacity)) = (n.budget, &n.demand, &n.capacity) {
            issue(
                Severity::Error,
                format!(
                    "can draw up to {}, more than the {} its supply can give",
                    amps_text(demand.hi),
                    amps_text(capacity.hi)
                ),
            );
        }
        if !sourced {
            issue(
                Severity::Note,
                "has no source: nothing supplies it".to_string(),
            );
        }
        if n.budget == Budget::Unknown || unknowns > 0 {
            let loads = match unknowns.max(1) {
                1 => "a load".to_string(),
                k => format!("{k} loads"),
            };
            issue(
                Severity::Note,
                format!("has {loads} with a current the solver has not settled"),
            );
        }
    }
    for (label, s) in &stages {
        if s.unsettled {
            let board = s
                .from
                .first()
                .and_then(|r| tree.nodes.iter().find(|n| n.id == *r))
                .and_then(|n| n.board.clone());
            out.push(PowerIssue {
                id: format!("stage:{label}"),
                severity: Severity::Note,
                subject: s.name.to_string(),
                text: "has an efficiency the solver has not settled".to_string(),
                at: s.at.clone(),
                board,
            });
        }
    }
    let rank = |s: Severity| if s == Severity::Error { 0 } else { 1 };
    out.sort_by(|a, b| {
        (rank(a.severity), &a.subject, &a.text).cmp(&(rank(b.severity), &b.subject, &b.text))
    });
    out
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
            at: None,
        }
    }

    fn budget(n: PowerNode) -> (Budget, Option<f64>) {
        let mut tree = PowerTree {
            title: String::new(),
            nodes: vec![n],
            edges: Vec::new(),
            stages: Vec::new(),
            issues: Vec::new(),
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

    #[test]
    fn currents_are_written_as_the_page_wrote_them() {
        for (x, text) in [
            (0.08401, "84 mA"),
            (0.002501, "2.5 mA"),
            (9.702e-4, "970 uA"),
            (1.121, "1.12 A"),
            (26.84e-6, "26.8 uA"),
            (-0.1401, "-140 mA"),
            (0.9996, "1 A"),
            (0.0, "0 A"),
            (5e-324, "0 A"),
        ] {
            assert_eq!(amps_text(x), text, "{x}");
        }
    }

    fn rail(id: &str, loads: Vec<Draw>, budget: Budget) -> PowerNode {
        let mut n = node(loads, None);
        n.id = id.into();
        n.net = id.into();
        n.budget = budget;
        n.supplies = vec![draw("s", -1.0, -0.1)];
        n
    }

    fn found(tree: &PowerTree) -> Vec<(Severity, String, String)> {
        issues_of(tree)
            .into_iter()
            .map(|i| (i.severity, i.subject, i.text))
            .collect()
    }

    #[test]
    fn the_four_issues_are_found_worst_first() {
        let mut over = rail("OVER", vec![draw("a.p", 0.0, 0.3)], Budget::Over);
        over.demand = Some(range(0.0, 0.3));
        over.capacity = Some(range(0.0, 0.1));
        let mut orphan = rail("ORPHAN", vec![draw("b.p", 0.0, 0.01)], Budget::Unlimited);
        orphan.supplies.clear();
        let mut unsure = rail("UNSURE", vec![draw("c.p", 0.0, 0.0)], Budget::Unknown);
        unsure.loads[0].range = None;
        let mut two = rail(
            "TWO",
            vec![draw("d.p", 0.0, 0.0), draw("e.p", 0.0, 0.0)],
            Budget::Unknown,
        );
        two.loads.iter_mut().for_each(|d| d.range = None);
        let edge = Edge {
            from: "UNSURE".into(),
            to: "ORPHAN".into(),
            label: "buck (BuckConverter)".into(),
            kind: EdgeKind::Converts,
            at: None,
        };
        let stage = Stage {
            label: "buck (BuckConverter)".into(),
            efficiency: None,
            efficiency_unsettled: true,
            at: None,
        };
        let tree = PowerTree {
            title: String::new(),
            nodes: vec![over, orphan, unsure, two],
            edges: vec![edge],
            stages: vec![stage],
            issues: Vec::new(),
        };
        let unsettled = "with a current the solver has not settled";
        // The converter makes ORPHAN, so it has a source now: only the other three, in order.
        assert_eq!(
            found(&tree),
            [
                (
                    Severity::Error,
                    "OVER".to_string(),
                    "can draw up to 300 mA, more than the 100 mA its supply can give".to_string()
                ),
                (
                    Severity::Note,
                    "TWO".to_string(),
                    format!("has 2 loads {unsettled}")
                ),
                (
                    Severity::Note,
                    "UNSURE".to_string(),
                    format!("has a load {unsettled}")
                ),
                (
                    Severity::Note,
                    "buck".to_string(),
                    "has an efficiency the solver has not settled".to_string()
                ),
            ]
        );
        // Without the converter, ORPHAN has no source.
        let mut tree = tree;
        tree.edges.clear();
        assert!(found(&tree).contains(&(
            Severity::Note,
            "ORPHAN".to_string(),
            "has no source: nothing supplies it".to_string()
        )));
    }

    #[test]
    fn a_converter_a_rail_feeds_is_not_one_of_its_loads() {
        // `buck.vin` draws a current the solver did not settle, but it is the converter's
        // input, which the unsettled efficiency says; the rail's budget is within.
        let mut feeder = rail("IN", vec![draw("buck.vin", 0.0, 0.0)], Budget::Within);
        feeder.loads[0].range = None;
        let out = rail("OUT", vec![draw("l.p", 0.0, 0.1)], Budget::Within);
        let edge = Edge {
            from: "IN".into(),
            to: "OUT".into(),
            label: "buck (BuckConverter)".into(),
            kind: EdgeKind::Converts,
            at: None,
        };
        let tree = PowerTree {
            title: String::new(),
            nodes: vec![feeder, out],
            edges: vec![edge],
            stages: Vec::new(),
            issues: Vec::new(),
        };
        assert_eq!(found(&tree), []);
    }
}
