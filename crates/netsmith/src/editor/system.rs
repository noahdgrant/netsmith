//! The system view: the boards of a system and the joins of their ports, or the members of
//! one board and the nets they share.

use std::collections::HashSet;

use netsmith_check::{Domain, InstId, InstKind, Member, Model, Sources};
use netsmith_editor::{
    ApiError,
    api::{Edge, EdgeKind, GroundGroupView, MateContact, MateView, Port, SystemNode, SystemView},
};

use super::analysis::{Snapshot, api_location};
use crate::locate;

/// More members than this on one net is a supply or a ground, not a connection.
const MOST_ON_A_NET: usize = 4;

pub fn view(snapshot: &Snapshot, name: &str) -> Result<SystemView, ApiError> {
    if let Some(system) = snapshot.system(name) {
        let solved = &system.solved;
        let model = &solved.model;
        let nodes = solved
            .boards
            .iter()
            .map(|(member, type_name, inst)| SystemNode {
                id: member.clone(),
                name: member.clone(),
                type_name: type_name.clone(),
                kind: "board".to_string(),
                ports: ports(model, *inst),
                at: at(model, &solved.sources, *inst),
            })
            .collect();
        let edges = model
            .joins()
            .iter()
            .filter_map(|&(a, b)| {
                let board = |port: InstId| {
                    let owner = model.board_of(port)?;
                    solved
                        .boards
                        .iter()
                        .find(|(_, _, inst)| *inst == owner)
                        .map(|(member, _, _)| member.clone())
                };
                Some(Edge {
                    from: board(a)?,
                    to: board(b)?,
                    label: format!("{} ~ {}", model.path_of(a), model.path_of(b)),
                    kind: EdgeKind::Join,
                    at: None,
                })
            })
            .collect();
        let member_of = |inst: InstId| {
            let owner = model.board_of(inst)?;
            solved
                .boards
                .iter()
                .find(|(_, _, board)| *board == owner)
                .map(|(member, _, _)| member.clone())
        };
        let mates = model
            .mates()
            .iter()
            .map(|m| MateView {
                a: model.path_of(m.a),
                b: model.path_of(m.b),
                a_board: member_of(m.a).unwrap_or_default(),
                b_board: member_of(m.b).unwrap_or_default(),
                contacts: m
                    .contacts
                    .iter()
                    .map(|c| MateContact {
                        number: c.number as u32,
                        a_role: role(c.a.role),
                        b_role: role(c.b.role),
                        a_net: c.a.net.clone(),
                        b_net: c.b.net.clone(),
                    })
                    .collect(),
                ports: m
                    .ports
                    .iter()
                    .map(|&(a, b)| format!("{} ~ {}", model.path_of(a), model.path_of(b)))
                    .collect(),
                reversible: m.reversible.clone(),
                at: Some(api_location(locate(&solved.sources, m.source, m.span))),
            })
            .collect();
        let ground_groups = model
            .ground_groups()
            .iter()
            .map(|g| GroundGroupView {
                nets: g
                    .nets
                    .iter()
                    .map(|n| format!("{}.{}", model.path_of(n.board), n.net))
                    .collect(),
                contacts: g
                    .contacts
                    .iter()
                    .map(|&(mate, n)| {
                        let m = &model.mates()[mate];
                        format!("{} / {}: {n}", model.path_of(m.a), model.path_of(m.b))
                    })
                    .collect(),
            })
            .collect();
        return Ok(SystemView {
            name: name.to_string(),
            kind: "system".to_string(),
            nodes,
            edges,
            mates,
            ground_groups,
        });
    }

    let data = snapshot.board(name)?;
    let trace = &data.built.trace;
    let model = &trace.model;
    let root = model.root;
    // The members of the board: its ports, modules and parts.
    let members: Vec<(String, InstId)> = model
        .instance(root)
        .children
        .iter()
        .filter_map(|(n, m)| match m {
            Member::Inst(i)
                if matches!(model.instance(*i).kind, InstKind::Module | InstKind::Part)
                    || is_port(model, *i) =>
            {
                Some((n.clone(), *i))
            }
            _ => None,
        })
        .collect();
    let nodes = members
        .iter()
        .map(|(n, inst)| {
            let i = model.instance(*inst);
            SystemNode {
                id: n.clone(),
                name: n.clone(),
                type_name: i.type_name.clone(),
                kind: match i.kind {
                    InstKind::Interface => "port",
                    InstKind::Module => "module",
                    _ => "part",
                }
                .to_string(),
                ports: if i.kind == InstKind::Interface {
                    Vec::new()
                } else {
                    ports(model, *inst)
                },
                at: at(model, &trace.sources, *inst),
            }
        })
        .collect();

    // A net that a return (ground) is, is left out: it would join everything.
    let returns: HashSet<&str> = model
        .balances()
        .iter()
        .filter(|b| b.returns)
        .map(|b| b.net.as_str())
        .collect();
    let mut edges = Vec::new();
    for net in model.nets() {
        if returns.contains(net.name.as_str()) {
            continue;
        }
        let mut touched: Vec<&str> = Vec::new();
        for pin in &net.pins {
            if let Some((n, _)) = members
                .iter()
                .find(|(_, inst)| *inst == *pin || model.is_inside(*pin, *inst))
                && !touched.contains(&n.as_str())
            {
                touched.push(n);
            }
        }
        if touched.len() < 2 || touched.len() > MOST_ON_A_NET {
            continue;
        }
        for other in &touched[1..] {
            edges.push(Edge {
                from: touched[0].to_string(),
                to: (*other).to_string(),
                label: net.name.clone(),
                kind: EdgeKind::Net,
                at: None,
            });
        }
    }
    Ok(SystemView {
        name: name.to_string(),
        kind: "board".to_string(),
        nodes,
        edges,
        mates: Vec::new(),
        ground_groups: Vec::new(),
    })
}

/// The word for a contact's role in the API.
fn role(role: netsmith_check::Role) -> String {
    match role {
        netsmith_check::Role::Ground => "ground",
        netsmith_check::Role::Power => "power",
        netsmith_check::Role::Signal => "signal",
        netsmith_check::Role::Open => "open",
    }
    .to_string()
}

fn at(model: &Model, sources: &Sources, inst: InstId) -> Option<netsmith_editor::api::Location> {
    let (src, span) = model.instance(inst).at();
    Some(api_location(locate(sources, src, span)))
}

/// The interfaces `inst` declares: its ports, with the voltage and current of a rail.
fn ports(model: &Model, inst: InstId) -> Vec<Port> {
    model
        .instance(inst)
        .children
        .iter()
        .filter_map(|(name, m)| match m {
            Member::Inst(i) if is_port(model, *i) => {
                let text = |field: &str| match model.member(*i, field) {
                    Some(Member::Field(f)) => match &model.field(f).domain {
                        Domain::Quantity(q) => Some(q.to_string()),
                        _ => None,
                    },
                    _ => None,
                };
                Some(Port {
                    name: name.clone(),
                    voltage: text("voltage"),
                    current: text("current"),
                })
            }
            _ => None,
        })
        .collect()
}

/// An interface with pins: a port a board can be joined at. A manufacturing process is an
/// interface too, but has no pins.
fn is_port(model: &Model, inst: InstId) -> bool {
    model.instance(inst).kind == InstKind::Interface
        && model
            .instance(inst)
            .children
            .iter()
            .any(|(_, m)| matches!(m, Member::Inst(i) if model.instance(*i).kind == InstKind::Pin))
}
