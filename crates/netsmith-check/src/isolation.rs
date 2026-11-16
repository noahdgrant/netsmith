//! Isolated grounds (issue #793, the direction ADR 0040 recommends): `.isolated` of a rail.
//!
//! A board with a barrier (an isolated DC-DC converter, a digital isolator) has two grounds on
//! purpose, and copper must not join them. `std::sources::Rail` has `.isolated: String`:
//! `"none"` (the same as unset), or what keeps the rail's ground apart from the board's other
//! grounds, in words. It marks the ground net the rail's `neg` is on as isolated. The text is
//! said back in the messages and checks nothing of the barrier part: a rating of the barrier
//! (a `.mating.isolation` voltage) was weighed in `docs/research/hierarchy-and-connectors-options.md`
//! and is not built.
//!
//! Each ground is a net of its own, so nothing else changes: the voltages are measured from
//! each rail's own ground, the balance of currents sums each net apart, and the current through
//! mated contacts is read for each ground's contacts apart (`require_current` keys the ground
//! contacts by their net). Two checks are added, both errors:
//!
//! - **On a board**: an isolated rail of the board (a member of the board itself) whose ground
//!   is on the ground net of another rail of the board that is not isolated and is not joined
//!   to it. This is the trap ADR 0121 names: a rail with no ground statement is on `GND`, so an
//!   isolated rail that forgets `.neg.net &= "ISO_GND"` joins the two sides by name. Rails inside
//!   a module or a part are not read: they are on the net they are joined to, and only a board
//!   says which domain a net is in.
//! - **In a system**: the mates must not join an isolated ground to another ground of the same
//!   board, through the grounds of other boards ([`Model::ground_groups`], ADR 0040). A card
//!   whose one `GND` meets the carrier's `GND` on one connector and the carrier's `ISO_GND` on
//!   another bridges the barrier. Reported once for each such group, at the last mate of it.
//!
//! Rails joined with `~` share `.isolated` as they share their voltage, so a module's rail
//! joined to an isolated rail is isolated too, and a mate's derived port join carries it to the
//! other board's rail, which is on the same copper.

use std::collections::{BTreeMap, BTreeSet};

use crate::checker::Checker;
use crate::model::{Domain, InstId, InstKind, Member};

/// One isolated rail: the rail, what isolates it, and its ground net and board.
struct Isolated {
    rail: InstId,
    why: String,
    board: InstId,
    net: String,
}

impl Checker<'_> {
    /// What a rail's `.isolated` says, when it says the rail is isolated.
    fn isolated_by(&self, rail: InstId) -> Option<String> {
        let Some(Member::Field(f)) = self.model.member(rail, "isolated") else {
            return None;
        };
        match &self.model.field(f).domain {
            Domain::Text(t) if t != "none" => Some(t.clone()),
            _ => None,
        }
    }

    /// The board and the ground net of a rail, when its `neg` is on a net of a board.
    fn ground_of(&self, rail: InstId) -> Option<(InstId, String, InstId)> {
        let (_, _, neg) = self.model.rail_of(rail)?;
        let net = self.model.net_of(neg)?;
        Some((self.model.board_of(rail)?, net.name.clone(), neg))
    }

    /// The checks of `.isolated`: run once the nets and the ground groups are known.
    pub(crate) fn check_isolated_grounds(&mut self) {
        let rails: Vec<InstId> = self
            .model
            .instances()
            .filter(|(id, i)| i.kind == InstKind::Interface && self.model.rail_of(*id).is_some())
            .map(|(id, _)| id)
            .collect();
        let mut isolated = Vec::new();
        for &rail in &rails {
            if let Some(why) = self.isolated_by(rail)
                && let Some((board, net, _)) = self.ground_of(rail)
            {
                isolated.push(Isolated {
                    rail,
                    why,
                    board,
                    net,
                });
            }
        }
        if isolated.is_empty() {
            return;
        }
        self.isolated_on_a_board(&rails, &isolated);
        self.isolated_through_mates(&isolated);
    }

    /// An isolated rail of a board on the ground of a rail of the board that is not isolated.
    fn isolated_on_a_board(&mut self, rails: &[InstId], isolated: &[Isolated]) {
        // A board in a system was checked alone too, with the same rails: it is read there.
        let on_board = |this: &Self, r: InstId| {
            this.model.instance(r).parent.is_some_and(|p| {
                let board = this.model.instance(p);
                board.kind == InstKind::Board && board.parent.is_none()
            })
        };
        for iso in isolated {
            if !on_board(self, iso.rail) {
                continue;
            }
            let (_, pos, _) = self.model.rail_of(iso.rail).expect("a rail");
            let own_pos = self.find(pos.0);
            let mut others: Vec<String> = Vec::new();
            for &other in rails {
                if other == iso.rail || !on_board(self, other) || self.isolated_by(other).is_some()
                {
                    continue;
                }
                let Some((board, net, _)) = self.ground_of(other) else {
                    continue;
                };
                let (_, opos, _) = self.model.rail_of(other).expect("a rail");
                if board != iso.board || net != iso.net || self.find(opos.0) == own_pos {
                    continue;
                }
                others.push(format!("`.{}`", self.model.path_of(other)));
            }
            if others.is_empty() {
                continue;
            }
            others.sort();
            let (source, span) = self.model.instance(iso.rail).declared.unwrap_or((
                self.model.instance(iso.rail).source,
                self.model.instance(iso.rail).span,
            ));
            let message = format!(
                "`.{}` is isolated ({}) but its ground is net `{}`, the ground of {} too, which is not isolated; an isolated rail has a ground of its own: state its net (`.neg.net &= \"ISO_{}\"`)",
                self.model.path_of(iso.rail),
                iso.why,
                iso.net,
                others.join(", "),
                iso.net
            );
            self.report(source, span, message);
        }
    }

    /// The mates join an isolated ground to another ground of its board.
    fn isolated_through_mates(&mut self, isolated: &[Isolated]) {
        let groups = self.model.ground_groups.clone();
        let mates = self.model.mates.clone();
        for group in &groups {
            let mut by_board: BTreeMap<InstId, BTreeSet<String>> = BTreeMap::new();
            for n in &group.nets {
                by_board.entry(n.board).or_default().insert(n.net.clone());
            }
            let Some(iso) = isolated.iter().find(|i| {
                by_board
                    .get(&i.board)
                    .is_some_and(|nets| nets.len() > 1 && nets.contains(&i.net))
            }) else {
                continue;
            };
            let board = self.model.path_of(iso.board);
            let others: Vec<String> = by_board[&iso.board]
                .iter()
                .filter(|n| **n != iso.net)
                .map(|n| format!("`{board}.{n}`"))
                .collect();
            let all: Vec<String> = group
                .nets
                .iter()
                .map(|n| format!("{}.{}", self.model.path_of(n.board), n.net))
                .collect();
            let shown = |p: InstId| format!("`.{}`", self.model.path_of(p));
            let through: Vec<String> = group
                .contacts
                .iter()
                .map(|&(m, _)| format!("{} and {}", shown(mates[m].a), shown(mates[m].b)))
                .collect();
            let Some(&(last, _)) = group.contacts.iter().max() else {
                continue;
            };
            let message = format!(
                "the mates of {} join `{board}.{}`, the ground of the isolated rail `.{}` ({}), to {}, another ground of `{board}`, through the grounds of other boards (the ground group {}); an isolated ground meets no other ground of its board, so the contacts on one side of the barrier need a ground of their own on the other board",
                through.join(", "),
                iso.net,
                self.model.path_of(iso.rail),
                iso.why,
                others.join(", "),
                all.join(" + ")
            );
            self.report(mates[last].source, mates[last].span, message);
        }
    }
}
