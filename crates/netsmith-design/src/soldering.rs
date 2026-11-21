//! How a part may be soldered against who puts it on the board (issue #1261).
//!
//! A part with a member `.soldering` of the enum `Soldering` (the standard `Supercap`, or a
//! project's own part with the same member) says how its datasheet allows it to be soldered:
//! `Reflow` (and so by hand too), `Hand` (a soldering iron only, never a reflow oven) or
//! `None` (not soldered at all: a clip, a holder, a press fit). Its `.assembly` says who puts it
//! on the board. The board house (`Fab`, `Consigned`) solders what it places in its reflow
//! oven, so:
//!
//! - a `Hand` part assembled by the board house is an error: its datasheet forbids reflow;
//! - a `None` part that is soldered at all (`Fab`, `Consigned` or `Hand`) is an error;
//! - a `Reflow` part, and a part whose `.soldering` nothing states, says nothing.
//!
//! The error names the part, where the method comes from (the design or the catalog row) and
//! the line that fixes it. A board house that solders a through-hole part by hand or by wave is
//! not modelled: such a part is `.assembly &= AssemblyMode::Hand` on the board, and the fab
//! notes list it for the person who orders.

use netsmith_check::{Domain, Member, Model};

use crate::Assembly;
use crate::environment::Fitted;

/// Judge every fitted part that states `.soldering` against its assembly, pushing an error for
/// each that breaks it.
pub(crate) fn judge(model: &Model, parts: &[Fitted<'_>], errors: &mut Vec<String>) {
    for fitted in parts {
        let Some(Member::Field(f)) = model.member(fitted.part, "soldering") else {
            continue;
        };
        let field = model.field(f);
        let Domain::Text(method) = &field.domain else {
            continue;
        };
        let soldered_by_house = matches!(fitted.assembly, Assembly::Fab | Assembly::Consigned);
        let refused = match method.as_str() {
            "Hand" => soldered_by_house,
            "None" => fitted.assembly != Assembly::None,
            _ => false,
        };
        if !refused {
            continue;
        }
        let name = model.path_of(fitted.part);
        let from = match fitted.pick {
            Some(pick) if field.narrowed_at.is_empty() => format!(
                "its catalog row {}{} states `soldering` {method}",
                pick.candidate,
                pick.sku
                    .as_deref()
                    .map_or(String::new(), |lcsc| format!(" ({lcsc})"))
            ),
            _ => format!("`.{name}.soldering` is `Soldering::{method}`"),
        };
        let assembly = format!("`AssemblyMode::{}`", fitted.assembly.variant());
        errors.push(match method.as_str() {
            "Hand" => format!(
                "`{name}` may be soldered by hand only ({from}), and its assembly is {assembly}: the board house solders the parts it places in a reflow oven, which the part's datasheet forbids; state `.{name}.assembly &= AssemblyMode::Hand`, or choose a part that allows reflow"
            ),
            _ => format!(
                "`{name}` is not soldered at all ({from}), and its assembly is {assembly}, which solders it: state `.{name}.assembly &= AssemblyMode::None`, and hold the part in its clip or holder"
            ),
        });
    }
}
