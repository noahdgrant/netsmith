//! The pin budget of a host processor (issue 1284): how many of its pins and serial blocks each
//! module of the board uses, and how many are left. `netsmith board explain <host>` prints it,
//! the solved file has it as `pin_budgets`, and the editor's board summary shows it.
//!
//! - **A host** is a part whose pin table, in the package the part is in, lists a function on a
//!   pin (`fn GPIO`). Its **pins** are those rows: a supply pin or a pin with no function is
//!   not a pin a board can give a job, so it is not counted.
//! - **A pin is used** when its net reaches a pin of another part, or the board chose a function
//!   on it (a chosen function is a use, also on a pin that nothing is connected to, as the rules
//!   of `shared` read it, ADR 0071).
//! - **The module of a use** is the innermost module that holds the other part on the net (`""`,
//!   the board, for a part declared in the board). A pin whose net reaches parts of several
//!   modules is used by each, and counted once in the host's total. A pin that reaches no other
//!   part (a function chosen and nothing joined) is the use of the host's own module.
//! - **A serial block** is a `peripheral` of the part, with the bus it is an end of when it says
//!   (`peripheral USART1 as Uart`). It is used when one of its functions is chosen on a pin, by
//!   the modules of those pins; it is **left** when it is not used and every `shared` resource
//!   that lists it or one of its functions still allows one more member.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use netsmith_check::{InstId, InstKind, Model, PinTable};
use netsmith_editor::api::{PinBudget, PinBudgetModule, SerialBlock};

use crate::Built;

/// The pin budget of each host of the board, in the order of the design's parts.
pub fn of(built: &Built) -> Vec<PinBudget> {
    let model = &built.trace.model;
    let design = &built.design;
    let origins = &built.trace.origins;
    let mut net_of: HashMap<(&str, &str), usize> = HashMap::new();
    for (i, net) in design.nets.iter().enumerate() {
        for pin in &net.pins {
            net_of.insert((pin.refdes.as_str(), pin.pin.as_str()), i);
        }
    }
    let inst_of: HashMap<&str, InstId> = design
        .parts
        .iter()
        .zip(origins)
        .map(|(p, &inst)| (p.refdes.as_str(), inst))
        .collect();
    let mut out = Vec::new();
    for (part, &inst) in design.parts.iter().zip(origins) {
        let Some(table) = table_of(model, inst, part.package.as_deref()) else {
            continue;
        };
        if !table.rows.iter().any(|r| !r.functions.is_empty()) {
            continue;
        }
        // The board's own path of a part: the model's path less what comes before the design's
        // path (the board's name in a system).
        let prefix = model
            .path_of(inst)
            .strip_suffix(part.path.as_str())
            .unwrap_or("")
            .to_string();
        let module_of = |id: InstId| -> String {
            let mut cur = model.instance(id).parent;
            while let Some(p) = cur {
                let i = model.instance(p);
                if i.kind == InstKind::Module {
                    let path = model.path_of(p);
                    return path.strip_prefix(&prefix).unwrap_or(&path).to_string();
                }
                if matches!(i.kind, InstKind::Board | InstKind::System) {
                    break;
                }
                cur = i.parent;
            }
            String::new()
        };
        let own = module_of(inst);
        let rules = &model.instance(inst).rules;
        // Each row's use: the function chosen on it and the modules its net reaches.
        struct Use {
            chosen: Option<String>,
            modules: BTreeSet<String>,
            used: bool,
        }
        let uses: Vec<Use> = table
            .rows
            .iter()
            .map(|row| {
                let chosen = model.function_of(row.pin);
                let mut modules = BTreeSet::new();
                for pad in &row.pads {
                    let Some(&net) = net_of.get(&(part.refdes.as_str(), pad.as_str())) else {
                        continue;
                    };
                    for other in &design.nets[net].pins {
                        if other.refdes == part.refdes {
                            continue;
                        }
                        if let Some(&o) = inst_of.get(other.refdes.as_str()) {
                            modules.insert(module_of(o));
                        }
                    }
                }
                let used = !modules.is_empty() || chosen.is_some();
                if used && modules.is_empty() {
                    modules.insert(own.clone());
                }
                Use {
                    chosen,
                    modules,
                    used,
                }
            })
            .collect();
        let mut by_module: BTreeMap<String, PinBudgetModule> = BTreeMap::new();
        let (mut pins, mut used) = (0u32, 0u32);
        for (row, u) in table.rows.iter().zip(&uses) {
            if row.functions.is_empty() {
                continue;
            }
            pins += 1;
            if u.used {
                used += 1;
                for m in &u.modules {
                    entry(&mut by_module, m).pins.push(row.name.clone());
                }
            }
        }
        let chosen_anywhere = |f: &str| uses.iter().any(|u| u.chosen.as_deref() == Some(f));
        let peripheral_used = |name: &str| {
            rules
                .peripherals
                .iter()
                .find(|p| p.name == name)
                .is_some_and(|p| p.functions.iter().any(|f| chosen_anywhere(&f.name)))
        };
        let mut blocks = Vec::new();
        for p in &rules.peripherals {
            let mut modules = BTreeSet::new();
            for u in &uses {
                if u.chosen
                    .as_deref()
                    .is_some_and(|c| p.functions.iter().any(|f| f.name == c))
                {
                    modules.extend(u.modules.iter().cloned());
                }
            }
            let is_used = peripheral_used(&p.name);
            let room = rules
                .shares
                .iter()
                .filter(|s| {
                    s.members.iter().any(|m| {
                        m.pin.is_none()
                            && (m.name == p.name || p.functions.iter().any(|f| f.name == m.name))
                    })
                })
                .all(|s| {
                    let in_use = s
                        .members
                        .iter()
                        .filter(|m| match m.pin {
                            Some(pin) => table
                                .rows
                                .iter()
                                .zip(&uses)
                                .any(|(r, u)| r.pin == pin && u.used),
                            None if rules.peripherals.iter().any(|q| q.name == m.name) => {
                                peripheral_used(&m.name)
                            }
                            None => chosen_anywhere(&m.name),
                        })
                        .count();
                    in_use < s.limit as usize
                });
            for m in &modules {
                entry(&mut by_module, m).blocks.push(p.name.clone());
            }
            blocks.push(SerialBlock {
                name: p.name.clone(),
                bus: p.bus.as_ref().map(|b| b.interface.clone()),
                used: is_used,
                left: !is_used && room,
                modules: modules.into_iter().collect(),
            });
        }
        out.push(PinBudget {
            host: part.path.clone(),
            kind: part.kind.clone(),
            refdes: part.refdes.clone(),
            pins,
            used,
            left: pins - used,
            modules: by_module.into_values().collect(),
            blocks,
        });
    }
    out
}

/// The record of the module `m` in `by_module`, made when it is not there.
fn entry<'a>(
    by_module: &'a mut BTreeMap<String, PinBudgetModule>,
    m: &str,
) -> &'a mut PinBudgetModule {
    by_module
        .entry(m.to_string())
        .or_insert_with(|| PinBudgetModule {
            module: m.to_string(),
            pins: Vec::new(),
            blocks: Vec::new(),
        })
}

/// The pin table of a part in its package: the one for the package, or the only one.
fn table_of<'a>(model: &'a Model, inst: InstId, package: Option<&str>) -> Option<&'a PinTable> {
    let tables = &model.instance(inst).tables;
    match package {
        Some(p) => model.table_for(inst, p),
        None if tables.len() == 1 => tables.first(),
        None => tables.iter().find(|t| t.package.is_none()),
    }
}

/// The pin budgets as text, for `netsmith board explain`: for each host, its pins used and
/// left, a line for each module with the pins and serial blocks it uses, and the serial blocks
/// used and left by bus.
pub fn text(budgets: &[PinBudget]) -> String {
    let mut out = String::new();
    for b in budgets {
        out.push_str(&format!(
            "  pin budget of `{}` (`{}`, {}): {} pins, {} used, {} left\n",
            b.host, b.kind, b.refdes, b.pins, b.used, b.left
        ));
        let name = |m: &str| {
            if m.is_empty() {
                "(board)".to_string()
            } else {
                m.to_string()
            }
        };
        let width = b
            .modules
            .iter()
            .map(|m| name(&m.module).len())
            .max()
            .unwrap_or(0);
        for m in &b.modules {
            let blocks = if m.blocks.is_empty() {
                String::new()
            } else {
                format!("; serial blocks {}", m.blocks.join(", "))
            };
            out.push_str(&format!(
                "    {:width$}  {} pin{}{}{blocks}\n",
                name(&m.module),
                m.pins.len(),
                if m.pins.len() == 1 { "" } else { "s" },
                if m.pins.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", m.pins.join(", "))
                },
            ));
        }
        if !b.blocks.is_empty() {
            let mut buses: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
            for s in &b.blocks {
                let e = buses
                    .entry(s.bus.as_deref().unwrap_or("other"))
                    .or_default();
                e.0 += 1;
                e.1 += usize::from(s.used);
                e.2 += usize::from(s.left);
            }
            let said: Vec<String> = buses
                .iter()
                .map(|(bus, (all, used, left))| {
                    let free: Vec<&str> = b
                        .blocks
                        .iter()
                        .filter(|s| s.left && s.bus.as_deref().unwrap_or("other") == *bus)
                        .map(|s| s.name.as_str())
                        .collect();
                    let free = if free.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", free.join(", "))
                    };
                    format!("{bus} {used} of {all} used, {left} left{free}")
                })
                .collect();
            out.push_str(&format!("    serial blocks: {}\n", said.join("; ")));
        }
    }
    out
}
