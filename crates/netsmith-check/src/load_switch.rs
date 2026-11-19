//! Whether a load switch's enable can turn it on (issue 1269).
//!
//! A part with `dc load_switch .in .out .enable ...` supplies its output rail only when its
//! enable is high. The checks take it as on whenever something on the enable's net can make it
//! high, which is the worst case for what the output's loads draw from the input: a host's pin
//! that drives (`output`, `tri_state` or `bidirectional`), the `pos` of a rail (an enable strapped
//! to a supply, the input's own included), or a pin of another part that passes a level along (a
//! `passive` pin: a pull-up, a series resistor, a connector or a jumper, whatever is behind it).
//! It cannot when nothing else is on the net, when the board states the enable `unused`, when the
//! net is a ground (the `neg` of a rail is on it, and no rail's `pos`), whatever else is on it, or
//! when all that is on the net is `input` pins, `power` pins that are no rail's, and open-drain
//! outputs (which only pull low).
//!
//! An enable that is high only through a pull-up inside the part (a datasheet's "may float") is
//! taken as off: the part's table says nothing of the pull-up, so a board ties such an enable to
//! the input. Not judged here: whether the level a driver or a strap gives is above the enable's
//! threshold (that is the levels check, for an enable that is a `Logic`), and an enable that is
//! active low (a load switch with one is not modelled; issue 1269 decided an enable that turns
//! the switch on).

use netsmith_lang::ast::PinKind;

use crate::model::{DcKind, InstId, InstKind, Member, Model};

/// Why a load switch's enable cannot turn it on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnableOff {
    /// The board states the enable, or an interface that holds it, `unused`.
    Unused,
    /// Nothing else is on the enable's net.
    Floating,
    /// What is on the enable's net can only hold it low: grounds, inputs, open-drain outputs.
    HeldLow,
}

impl EnableOff {
    /// What a message says after "its enable `...`".
    pub fn text(self) -> &'static str {
        match self {
            Self::Unused => "is stated `unused`",
            Self::Floating => "is on a net with nothing else on it",
            Self::HeldLow => {
                "is held low: it is on a ground, or only inputs and open-drain outputs are on its net"
            }
        }
    }
}

impl Model {
    /// For a part that states `dc load_switch`: its enable pin, and why the enable cannot turn
    /// it on, if it cannot (`None` when it can). `None` for any other instance.
    pub fn load_switch_enable(&self, part: InstId) -> Option<(InstId, Option<EnableOff>)> {
        let dc = self.instance(part).dc.as_ref()?;
        if dc.kind != DcKind::LoadSwitch || self.instance(part).kind != InstKind::Part {
            return None;
        }
        let enable = *dc.pins.get(2)?;
        Some((enable, self.enable_off(part, enable)))
    }

    /// Whether the part, which states `dc load_switch`, supplies its output: its enable can
    /// turn it on. False for any other instance.
    pub fn load_switch_can_turn_on(&self, part: InstId) -> bool {
        matches!(self.load_switch_enable(part), Some((_, None)))
    }

    fn enable_off(&self, part: InstId, enable: InstId) -> Option<EnableOff> {
        if self.unused_by(enable).is_some() {
            return Some(EnableOff::Unused);
        }
        let Some(net) = self.net_of(enable) else {
            return Some(EnableOff::Floating);
        };
        let rail_pos = |pin: InstId| {
            let inst = self.instance(pin);
            inst.name == "pos"
                && inst.parent.is_some_and(|rail| {
                    self.rail_of(rail).is_some_and(|(_, pos, _)| pos == pin)
                        && self.unused_by(rail).is_none()
                })
        };
        // A rail's `pos` on the net, a board's own included (its pins are no part's terminals).
        if net.pins.iter().any(|&p| p != enable && rail_pos(p)) {
            return None;
        }
        // A ground: what else is on it does not lift it.
        if net.pins.iter().any(|&p| p != enable && self.is_rail_neg(p)) {
            return Some(EnableOff::HeldLow);
        }
        let mut others = net
            .terminals
            .iter()
            .copied()
            .filter(|&t| t != enable)
            .peekable();
        if others.peek().is_none() {
            return Some(EnableOff::Floating);
        }
        let drives = others.any(|t| {
            if self.unused_by(t).is_some() || self.owner_of(t) == Some(part) {
                return false;
            }
            matches!(
                self.pin_kind(t),
                Some(PinKind::Output | PinKind::TriState | PinKind::Bidirectional)
                    | Some(PinKind::Passive)
                    | None
            )
        });
        (!drives).then_some(EnableOff::HeldLow)
    }

    /// Whether a pin is the `neg` of a rail.
    fn is_rail_neg(&self, pin: InstId) -> bool {
        let inst = self.instance(pin);
        inst.name == "neg"
            && inst.parent.is_some_and(|rail| {
                matches!(self.member(rail, "neg"), Some(Member::Inst(n)) if n == pin)
                    && self.rail_of(rail).is_some()
            })
    }
}
