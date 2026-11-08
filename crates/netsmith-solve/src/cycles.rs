//! Values that wait on each other. Propagation sets a field from an equation or a balance of
//! currents only when everything the equation reads has a value. If a group of fields depend
//! on each other in a cycle and none has a value, nothing in the group can be set first: every
//! relation in and downstream of the cycle stays unevaluated, and the checks they hold are not
//! made. A range stated on one field of the cycle (`&=`) gives propagation a place to start.
//!
//! This module only finds such groups, so that the build can name them. It changes no value.
//! A group that also waits on a value nothing states is found too (`circles`), with the values
//! it waits on, because a range on the group alone would not start it and the finding must say
//! what would.

use std::collections::{BTreeMap, BTreeSet};

use netsmith_check::{Domain, FieldId, InstKind, Model, RExpr};
use netsmith_lang::ast::CmpOp;

use crate::eval::fields_in;

/// One way a value can be worked out: the class of the field it sets and the classes of the
/// unset fields it reads.
struct Producer {
    out: usize,
    reads: Vec<usize>,
    /// An equation, as opposed to a balance of currents. Two unknown currents on one net are
    /// no cycle, so a group is reported only when an equation is part of it.
    equation: bool,
    /// The balance this stands for, when the solver does not use it: a part's current that
    /// the other currents of a net of more than two terms would determine (ADR 0026), if the
    /// solver worked it out from them. Only `balance_circles` looks at these.
    through: Option<usize>,
}

/// A group of fields that depend on each other in a cycle while none has a value.
pub(crate) struct Circle {
    /// In field order.
    pub fields: Vec<FieldId>,
    /// Everything that could set one of the fields reads only fields of the group or fields
    /// that have values: a range stated on one of the fields starts it.
    pub closed: bool,
    /// For a group that is not closed, the fields outside it, without a value, that something
    /// in it reads, directly or through the equations that set what it reads, and that nothing
    /// sets (or that wait in a circle of their own): what the board has to state before a
    /// range on the group could start it. In field order.
    pub waiting: Vec<FieldId>,
    /// For a group found by `balance_circles`: the nets (indices into `Model::balances`) whose
    /// balance would close it, and the currents on each that the solver does not work out.
    pub through: Vec<Through>,
}

/// A net with more than two currents, all of whose parts' currents the solver leaves alone, and
/// that a group depends on: what the group's values would need from it.
pub(crate) struct Through {
    pub balance: usize,
    /// The part currents of the net that no value gives: what the solver does not work out.
    pub unsolved: Vec<FieldId>,
    /// Every current on the net that has no value, in field order.
    pub unknown: Vec<FieldId>,
}

/// The groups of fields that depend on each other, none of which has a value, and are closed.
/// Each group is in field order and the groups are in the order of their first field.
///
/// A group is reported only when it is closed: everything that could set one of its fields
/// reads only fields of the group or fields that have values. A field that also waits on
/// something nothing states is a different fault, and a range stated on the group would not
/// start it, so such a group is left out (no note rather than a wrong one). `circles` has
/// those too.
pub(crate) fn find(model: &Model) -> Vec<Vec<FieldId>> {
    circles(model)
        .into_iter()
        .filter(|c| c.closed)
        .map(|c| c.fields)
        .collect()
}

/// Every group of fields that depend on each other in a cycle while none has a value, closed
/// or not, in the order of their first field.
pub(crate) fn circles(model: &Model) -> Vec<Circle> {
    groups(model, false)
}

/// The groups that would be circles if the solver worked a part's current out from a balance
/// of more than two terms, and that it does not (ADR 0026: such a current is verified and
/// never set, because the residual of several ranges says little about one part). A divider
/// behind a series part is one: the divider's current needs its voltage, which needs the drop
/// across the part, which needs the part's current, which is what the net carries to the
/// divider and to whatever else is on it. Nothing in it is set, no equation of it is a cycle
/// in the solver's sense (`circles` has none), and every check on the divider waits (issue
/// #18). A group is returned only when a balance of that kind is part of the cycle.
pub(crate) fn balance_circles(model: &Model) -> Vec<Circle> {
    groups(model, true)
        .into_iter()
        .filter(|c| !c.through.is_empty())
        .collect()
}

fn groups(model: &Model, balances_too: bool) -> Vec<Circle> {
    let size = model.fields().map(|(f, _)| f.0 + 1).max().unwrap_or(0);
    // Fields joined by `~` hold one value, so they are one node.
    let mut parent: Vec<usize> = (0..size).collect();
    fn root(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &(a, b) in model.equalities() {
        let (ra, rb) = (root(&mut parent, a.0), root(&mut parent, b.0));
        parent[ra] = rb;
    }
    let unset = |f: FieldId| matches!(model.field(f).domain, Domain::Unset);
    let mut class_of = |f: FieldId| root(&mut parent, f.0);

    let mut producers: Vec<Producer> = Vec::new();
    for r in model.relations() {
        if !matches!(r.op, CmpOp::EqEq | CmpOp::Narrow) {
            continue;
        }
        let RExpr::Field(out) = &r.lhs else { continue };
        if !unset(*out) {
            continue;
        }
        let mut reads = Vec::new();
        fields_in(&r.rhs, &mut reads);
        producers.push(Producer {
            out: class_of(*out),
            reads: reads
                .into_iter()
                .filter(|&f| unset(f))
                .map(&mut class_of)
                .collect(),
            equation: true,
            through: None,
        });
    }
    for (index, b) in model.balances().iter().enumerate() {
        if b.returns {
            continue;
        }
        for d in &b.draws {
            let owner = model.field(d.field).owner;
            let part = model.instance(owner).kind == InstKind::Part;
            if !unset(d.field) {
                continue;
            }
            // What a balance may set (see `Solver::solve_balance_but`).
            let live = b.draws.iter().filter(|d| !model.draw_is_zero(d)).count();
            let refused = part && (!b.is_complete() || live > 2);
            // A complete balance of more than two terms would determine the part's current
            // if the solver used it. An incomplete one has pins that draw what the model
            // cannot say, so nothing is determined by it, in the solver or in fact.
            let would = refused && b.is_complete();
            if refused && !(balances_too && would) {
                continue;
            }
            producers.push(Producer {
                out: class_of(d.field),
                reads: b
                    .draws
                    .iter()
                    .filter(|o| o.field != d.field && unset(o.field))
                    .map(|o| class_of(o.field))
                    .collect(),
                equation: false,
                through: refused.then_some(index),
            });
        }
    }
    if !producers.iter().any(|p| p.equation) {
        return Vec::new();
    }

    // The graph: an edge from each field a value is read from to the field it sets.
    let mut edges: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for p in &producers {
        edges.entry(p.out).or_default();
        for &r in &p.reads {
            edges.entry(r).or_default().insert(p.out);
        }
    }
    let components = strongly_connected(&edges);
    let mut component_of: BTreeMap<usize, usize> = BTreeMap::new();
    for (i, c) in components.iter().enumerate() {
        for &n in c {
            component_of.insert(n, i);
        }
    }

    // The classes something sets: a read that is none of them is waiting on a value nobody
    // gives.
    let settled: BTreeSet<usize> = producers.iter().map(|p| p.out).collect();
    let mut groups: Vec<Circle> = Vec::new();
    for (i, c) in components.iter().enumerate() {
        let inside = |n: &usize| component_of.get(n) == Some(&i);
        let mine = || producers.iter().filter(|p| inside(&p.out));
        // A cycle: an equation of the group that reads a field of the group.
        let cycle = mine().any(|p| p.equation && p.reads.iter().any(inside));
        if !cycle {
            continue;
        }
        // Closed: nothing that sets a field of the group reads an unset field outside it.
        let closed = mine().all(|p| p.reads.iter().all(inside));
        let mut fields: Vec<FieldId> = model
            .fields()
            .map(|(f, _)| f)
            .filter(|&f| unset(f) && c.contains(&class_of(f)))
            .collect();
        fields.sort();
        // What the group waits on: the classes it reads from outside, and, through whatever
        // sets those, the classes that nothing sets (a value the board has to state). A class
        // that something sets is only as good as what it waits on, so it is followed back; one
        // in another group that depends on itself is named as it is, because nothing in it can
        // be set first either.
        let mut outside: BTreeSet<usize> = BTreeSet::new();
        let mut visited: BTreeSet<usize> = BTreeSet::new();
        let mut todo: Vec<usize> = mine()
            .flat_map(|p| p.reads.iter().copied())
            .filter(|n| !inside(n))
            .collect();
        while let Some(n) = todo.pop() {
            if !visited.insert(n) {
                continue;
            }
            if !settled.contains(&n) || component_of.contains_key(&n) {
                outside.insert(n);
                continue;
            }
            todo.extend(
                producers
                    .iter()
                    .filter(|p| p.out == n)
                    .flat_map(|p| p.reads.iter().copied())
                    .filter(|r| !inside(r)),
            );
        }
        let mut waiting: Vec<FieldId> = Vec::new();
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        for (f, _) in model.fields() {
            let class = class_of(f);
            if unset(f) && outside.contains(&class) && seen.insert(class) {
                waiting.push(f);
            }
        }
        // The balances the solver does not use that the cycle runs through.
        let mut through: Vec<Through> = Vec::new();
        for p in mine().filter(|p| p.reads.iter().any(inside)) {
            let Some(balance) = p.through else { continue };
            if through.iter().any(|t| t.balance == balance) {
                continue;
            }
            let draws = &model.balances()[balance].draws;
            let mut unknown: Vec<FieldId> = draws
                .iter()
                .map(|d| d.field)
                .filter(|&f| unset(f))
                .collect();
            unknown.sort();
            let unsolved: Vec<FieldId> = unknown
                .iter()
                .copied()
                .filter(|&f| model.instance(model.field(f).owner).kind == InstKind::Part)
                .collect();
            through.push(Through {
                balance,
                unsolved,
                unknown,
            });
        }
        groups.push(Circle {
            fields,
            closed,
            waiting,
            through,
        });
    }
    groups.sort_by(|a, b| a.fields.cmp(&b.fields));
    groups
}

/// Tarjan's algorithm, without recursion. A component is returned only if it holds a cycle:
/// more than one node, or a node with an edge to itself.
fn strongly_connected(edges: &BTreeMap<usize, BTreeSet<usize>>) -> Vec<BTreeSet<usize>> {
    let mut index: BTreeMap<usize, usize> = BTreeMap::new();
    let mut low: BTreeMap<usize, usize> = BTreeMap::new();
    let mut on_stack: BTreeSet<usize> = BTreeSet::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut found: Vec<BTreeSet<usize>> = Vec::new();
    let mut next = 0;
    for &start in edges.keys() {
        if index.contains_key(&start) {
            continue;
        }
        // Each node on the path, with the successors it has still to visit.
        let mut work: Vec<(usize, Vec<usize>)> = Vec::new();
        index.insert(start, next);
        low.insert(start, next);
        next += 1;
        stack.push(start);
        on_stack.insert(start);
        work.push((start, edges[&start].iter().rev().copied().collect()));
        while let Some((node, todo)) = work.last_mut() {
            let node = *node;
            if let Some(succ) = todo.pop() {
                match index.get(&succ).copied() {
                    None => {
                        index.insert(succ, next);
                        low.insert(succ, next);
                        next += 1;
                        stack.push(succ);
                        on_stack.insert(succ);
                        let more = edges[&succ].iter().rev().copied().collect();
                        work.push((succ, more));
                    }
                    Some(i) if on_stack.contains(&succ) => {
                        let l = low[&node].min(i);
                        low.insert(node, l);
                    }
                    Some(_) => {}
                }
                continue;
            }
            work.pop();
            if let Some((parent, _)) = work.last() {
                let l = low[parent].min(low[&node]);
                low.insert(*parent, l);
            }
            if low[&node] == index[&node] {
                let mut component = BTreeSet::new();
                while let Some(n) = stack.pop() {
                    on_stack.remove(&n);
                    component.insert(n);
                    if n == node {
                        break;
                    }
                }
                if component.len() > 1 || edges[&node].contains(&node) {
                    found.push(component);
                }
            }
        }
    }
    found
}
