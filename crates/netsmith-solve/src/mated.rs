//! The nets of a system that mated contacts join (issue #1427).
//!
//! A system's boards keep their own nets in the model, and a mate joins a net of one board to a
//! net of another at each pair of contacts (`Model::mates`, ADR 0040). The DC analysis and the
//! levels check read the nets a mate joins as one node: the analysis solves the circuit of both
//! boards at once (a pull-up on one board and the open-drain driver and the receiver on the
//! other, a supply carried on a power contact), and the passes of the levels check that look at
//! one net at a time (a pin with no levels, a level through a series part, a divider, a strap)
//! see the pins of every board the line reaches. Every contact joins, whatever its role: a
//! ground contact joins the grounds and a power contact the rails, as `~` between the two would.
//!
//! A group of joined nets is held at the place of its lowest net in `Model::nets`, so the
//! analysis and the check name one net by one index; the other nets of the group are empty
//! there. A mated contact is a wire between the two boards: it draws nothing and drives
//! nothing. Within one board, and in a system with no mate, the nets are the model's.

use std::collections::{BTreeSet, HashMap, HashSet};

use netsmith_check::{InstId, Model, Net};

/// The nets of a model with the nets that mated contacts join made one.
#[derive(Debug, Clone)]
pub(crate) struct Joined {
    /// The nets, one for each of `Model::nets`, in its order: a group's at its lowest place,
    /// with the pins and terminals of each net of the group in the order of their places, and
    /// the others empty. A net that no mate joins is the model's.
    pub nets: Vec<Net>,
    /// For each net of the model, the place of the group it is in.
    pub node: Vec<usize>,
    /// The pins of the contacts that the mates join, of both connectors.
    pub contacts: HashSet<InstId>,
    /// The places of the groups of more than one net.
    pub groups: BTreeSet<usize>,
}

impl Joined {
    /// The nets of `model` joined at its mates, or `None` when no mate joins two nets (a board,
    /// or a system whose mates join nothing), where the model's nets are the answer.
    pub(crate) fn of(model: &Model) -> Option<Self> {
        let nets = model.nets();
        let mut net_of: HashMap<InstId, usize> = HashMap::new();
        for (i, n) in nets.iter().enumerate() {
            for &p in &n.pins {
                net_of.insert(p, i);
            }
        }
        let mut parent: Vec<usize> = (0..nets.len()).collect();
        fn find(parent: &mut [usize], x: usize) -> usize {
            if parent[x] != x {
                let root = find(parent, parent[x]);
                parent[x] = root;
            }
            parent[x]
        }
        let mut contacts = HashSet::new();
        let mut any = false;
        for mate in model.mates() {
            for c in &mate.contacts {
                let (Some(&x), Some(&y)) = (net_of.get(&c.a.pin), net_of.get(&c.b.pin)) else {
                    continue;
                };
                contacts.insert(c.a.pin);
                contacts.insert(c.b.pin);
                let (rx, ry) = (find(&mut parent, x), find(&mut parent, y));
                if rx != ry {
                    parent[rx.max(ry)] = rx.min(ry);
                    any = true;
                }
            }
        }
        if !any {
            return None;
        }
        let node: Vec<usize> = (0..nets.len()).map(|i| find(&mut parent, i)).collect();
        let mut joined: Vec<Net> = nets
            .iter()
            .map(|n| Net {
                pins: Vec::new(),
                terminals: Vec::new(),
                ..n.clone()
            })
            .collect();
        let mut groups = BTreeSet::new();
        for (i, n) in nets.iter().enumerate() {
            let at = node[i];
            if at != i {
                groups.insert(at);
            }
            joined[at].pins.extend(n.pins.iter().copied());
            joined[at].terminals.extend(n.terminals.iter().copied());
        }
        Some(Self {
            nets: joined,
            node,
            contacts,
            groups,
        })
    }

    /// Name each group as the levels check's messages name a line across a mate (issue #1323):
    /// `the nets `tx` of `carrier` and `rx` of `card`, mated at `carrier.slot` pin 2 against
    /// `card.j1` pin 2`: the nets in the order of their boards in the system, and every contact
    /// that joins two of them in the order of the mates, its side on the earlier board first.
    pub(crate) fn describe(&mut self, model: &Model) {
        let nets = model.nets();
        let place = |pin: InstId| nets.iter().position(|n| n.pins.contains(&pin));
        let board = |pin: InstId| model.owner_of(pin).and_then(|o| model.board_of(o));
        let mut contacts: HashMap<usize, Vec<String>> = HashMap::new();
        for mate in model.mates() {
            for c in &mate.contacts {
                let Some(a) = place(c.a.pin).filter(|&a| self.groups.contains(&self.node[a]))
                else {
                    continue;
                };
                let (first, second) = if board(c.b.pin) < board(c.a.pin) {
                    (c.b.pin, c.a.pin)
                } else {
                    (c.a.pin, c.b.pin)
                };
                contacts.entry(self.node[a]).or_default().push(format!(
                    "{} against {}",
                    contact(model, first),
                    contact(model, second)
                ));
            }
        }
        for &g in &self.groups {
            let mut members: Vec<usize> = (0..nets.len()).filter(|&i| self.node[i] == g).collect();
            members.sort_by_key(|&i| {
                let on = nets[i]
                    .terminals
                    .iter()
                    .find_map(|&t| model.owner_of(t).and_then(|o| model.board_of(o)));
                (on, i)
            });
            let named: Vec<String> = members
                .iter()
                .map(|&i| format!("`{}`{}", nets[i].name, board_of_net(model, &nets[i])))
                .collect();
            let list = match named.as_slice() {
                [a, b] => format!("{a} and {b}"),
                [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
                [] => String::new(),
            };
            let at = contacts.remove(&g).unwrap_or_default();
            self.nets[g].name = format!("the nets {list}, mated at {}", at.join(" and "));
        }
    }
}

/// ` of `board``, the board a net is on, for a message (or nothing, off any board).
pub(crate) fn board_of_net(model: &Model, net: &Net) -> String {
    net.terminals
        .iter()
        .find_map(|&t| model.owner_of(t).and_then(|o| model.board_of(o)))
        .map_or_else(String::new, |b| format!(" of `{}`", model.path_of(b)))
}

/// A contact of a connector, for a message: `` `carrier.slot` pin 3 ``.
pub(crate) fn contact(model: &Model, pin: InstId) -> String {
    let connector = model
        .owner_of(pin)
        .map_or_else(String::new, |o| model.path_of(o));
    let name = model.path_of(pin);
    let number = name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .trim_start_matches('p');
    format!("`{connector}` pin {number}")
}

/// The nets of a model as the DC analysis and the levels check read them: joined at the mates
/// where a mate joins any (see [`Joined`]), else the model's.
pub(crate) fn nets<'a>(model: &'a Model, joined: Option<&'a Joined>) -> &'a [Net] {
    joined.map_or(model.nets(), |j| &j.nets)
}
