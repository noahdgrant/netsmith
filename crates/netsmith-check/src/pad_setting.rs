//! A pad's own connection stated off a part (issue #1674).
//!
//! `.pour_connection` and `.spoke_width` say how a pour joins the pads of one pin of a part
//! (issue #1276). A port of a module or a pin of a rail (`.reg.vin.neg`, `.input.neg`) has no
//! pads: it stands for the pins of parts that the module or board joins to it. A setting on it
//! would reach none, so it is refused, and the message names the pins of parts on the port's net
//! inside the module (or the board, for a rail of its own) that the person may have meant. The
//! statement is read before the nets are known, so it is recorded here and reported once they
//! are.

use crate::checker::Checker;
use crate::diag::SourceId;
use crate::model::{FieldId, InstId, InstKind};
use netsmith_lang::Span;

/// The pins a message lists before it says how many more there are.
const LISTED: usize = 8;

/// A `.pour_connection` or `.spoke_width` stated on a pin that is not of a part.
pub(crate) struct PadSettingOffPart {
    /// The pin the statement names.
    pin: InstId,
    /// The field's name, `pour_connection` or `spoke_width`.
    field: String,
    /// The instance whose body holds the statement: paths in the message are from it.
    scope: InstId,
    /// The pin's path as written, `.reg.vin.neg`.
    written: String,
    source: SourceId,
    span: Span,
}

impl Checker<'_> {
    /// Whether `field` is a pad setting of a pin that is not a part's (a port of a module, or a
    /// pin of a rail or other interface of a module or board).
    pub(crate) fn is_pad_setting_off_part(&self, field: FieldId) -> bool {
        let f = self.model.field(field);
        matches!(f.name.as_str(), "pour_connection" | "spoke_width")
            && self.model.instance(f.owner).kind == InstKind::Pin
            && self
                .model
                .owner_of(f.owner)
                .is_none_or(|o| self.model.instance(o).kind != InstKind::Part)
    }

    /// Record the refused statement `c` on `field`, written as `path`, to report when the nets
    /// are known.
    pub(crate) fn refuse_pad_setting_off_part(
        &mut self,
        field: FieldId,
        path: &netsmith_lang::ast::Path,
        span: Span,
    ) {
        let pin = self.model.field(field).owner;
        let field = self.model.field(field).name.clone();
        // A statement in an interface's block (`.supply: Rail { ... }`) is told from the
        // module or board that holds the interface, where the pins of parts can be named.
        let mut scope = self.current();
        let written = if self.model.instance(scope).kind == InstKind::Interface {
            scope = self.model.owner_of(scope).unwrap_or(self.model.root);
            format!(".{}", self.model.path_below(pin, scope))
        } else {
            let mut owner = path.clone();
            owner.segments.pop();
            owner.to_source()
        };
        self.pad_settings_off_part.push(PadSettingOffPart {
            pin,
            field,
            scope,
            written,
            source: self.cur_source(),
            span,
        });
    }

    /// Report each recorded statement, now that the nets are known, with the pins of parts on
    /// the pin's net inside the module or board that holds the pin.
    pub(crate) fn report_pad_settings_off_part(&mut self) {
        let refused = std::mem::take(&mut self.pad_settings_off_part);
        for r in refused {
            let owner = self.model.owner_of(r.pin).unwrap_or(self.model.root);
            let owner_kind = self.model.instance(owner).kind;
            let inside = |model: &crate::model::Model, mut inst: InstId| loop {
                if inst == owner {
                    return true;
                }
                match model.instance(inst).parent {
                    Some(p) => inst = p,
                    None => return false,
                }
            };
            let mut pins: Vec<String> = self
                .model
                .nets()
                .iter()
                .find(|n| n.pins.contains(&r.pin))
                .map(|n| {
                    n.pins
                        .iter()
                        .copied()
                        .filter(|&p| {
                            self.model
                                .owner_of(p)
                                .is_some_and(|o| self.model.instance(o).kind == InstKind::Part)
                                && inside(&self.model, p)
                        })
                        .map(|p| format!(".{}", self.model.path_below(p, r.scope)))
                        .collect()
                })
                .unwrap_or_default();
            pins.sort();
            pins.dedup();
            // What the pin is: a pin of an interface (a rail, a port of a module), or a port
            // of a module that is one pin.
            let parent = self.model.instance(r.pin).parent.unwrap_or(owner);
            let kind_word = owner_kind.word();
            let owner_name = &self.model.instance(owner).type_name;
            let owner_at = self.model.path_below(owner, r.scope);
            let of_owner = if owner_kind == InstKind::Module {
                format!("module `{owner_name}`")
            } else {
                format!("the {kind_word}")
            };
            let word = if self.model.instance(parent).kind == InstKind::Interface {
                let iface = self.model.path_below(parent, r.scope);
                let ty = &self.model.instance(parent).type_name;
                let role = if owner_kind == InstKind::Module {
                    "port"
                } else {
                    "interface"
                };
                format!("a pin of `.{iface}`, a `{ty}` {role} of {of_owner}")
            } else {
                format!("a port of {of_owner}")
            };
            let holder = if owner_at.is_empty() {
                String::new()
            } else {
                format!(" inside `.{owner_at}`")
            };
            let field = &r.field;
            let written = &r.written;
            let mut message = format!(
                "`{written}.{field}` reaches no pad: `{written}` is {word}, not a pin of a part, and has no pads; a pour joins the pads of a part's pin as that pin states"
            );
            if pins.is_empty() {
                message.push_str(&format!(
                    ", and no pin of a part{holder} is on its net: state `.{field}` on the pin of the part whose pads it is for"
                ));
            } else {
                let more = pins.len().saturating_sub(LISTED);
                let listed: Vec<String> =
                    pins.iter().take(LISTED).map(|p| format!("`{p}`")).collect();
                let mut list = listed.join(", ");
                if more > 0 {
                    list.push_str(&format!(" and {more} more"));
                }
                let example = &pins[0];
                message.push_str(&format!(
                    ": state it on the pin of the part you mean, such as `{example}.{field}`; the pins of parts on its net{holder} are {list}"
                ));
            }
            self.report(r.source, r.span, message);
        }
    }
}
