//! Orders (ADR 0035): which boards go into one fabrication order, and how many of each.
//!
//! An order is a named table of `netsmith.toml`:
//!
//! ```toml
//! [order.devkit.boards]
//! "DevKit/carrier" = 2
//! "DevKit/power_card" = 1
//! Unit = 10
//! ```
//!
//! Each key is a board as its output folder names it (`Board`, or `System/member`) or a whole
//! system (`System`, all its members), and the value is how many to order. `netsmith fab
//! --order devkit` builds those boards only, writes them under `<fab>/orders/devkit/`, and
//! adds `order.txt` (the summary and the notes to read before ordering) and `order-bom.csv`
//! (the bill of materials of the whole order).
//!
//! This module holds the manifest part (types, reading, rendering), the choice of boards,
//! the summary, and the run that `main` and the editor share.
//!
//! # What an order does, in detail (ADR 0035; the owner reviewed it on 2026-11-02)
//!
//! A project holds many boards (ADR 0010), and `netsmith fab` made a package for every one, with
//! nothing saying which go into one order to the board house or how many of each. There is one
//! way to choose boards: a named order. A plain `netsmith fab` still makes every board of the
//! project, and `netsmith fab <file.nsm>` one file's boards.
//!
//! - **A key** is a board as its output folder names it, or a system's name for all its
//!   members, each with that quantity. A name that is neither is an error naming the manifest line
//!   and listing the boards that exist; naming one board twice (alone, and through its system) is
//!   an error naming both lines. **The value** is a whole number of at least 1; 0 is an error that
//!   says to leave the board out. There is no default quantity, so a reader of the order sees every
//!   number. The name is letters, digits, `-` and `_` (it is a folder name) and is unique, because
//!   TOML does not allow a table twice. An order with no boards is an error. `--order` with no name
//!   lists the orders and takes no file. The tables are read by the `toml` crate through serde with
//!   `deny_unknown_fields` on the file and on each order, so any other key refuses with the line and
//!   column; mistakes the types cannot say are `netsmith.toml:LINE: order `name`: ...`.
//! - **The spelling** was chosen from three. (1) `boards = [...]` and `quantity = { ... }` with a
//!   default of 1: rejected, a board is named in two places, which allows a quantity for a board
//!   that is not in the order and a duplicate in the list, and a default of 1 is a second way to
//!   say "1". (2) An array of tables `[[order.devkit.board]] name = "..." quantity = 2`: rejected,
//!   a block of three lines for each board, and the name still needs the uniqueness check. (3) A
//!   table from board to quantity: each board is named once, TOML itself refuses a duplicate key
//!   with its line, and a line reads "this board, this many". The cost is that an order has no room
//!   for other per-board fields. None is wanted; if one is, that order's table becomes a table of
//!   tables and this form is replaced, not extended (ADR 0001).
//! - **The run** builds every top-level file once (the checker and solver, no KiCad) to learn the
//!   project's boards, resolves the order against them, then runs the usual `fab` for the boards
//!   named only (`FabOptions::only`). Each package goes under
//!   `<fab>/orders/<name>/<board folder>/`. `orders/<name>/` was chosen over `fab/<name>/` so an
//!   order's name can never be the folder of a board; a board or system called `orders` is refused
//!   when an order runs. A folder of the same order from an earlier run is removed first, only if
//!   it holds an `order.txt`, so nothing else is deleted and a board dropped from the order does
//!   not linger. If any board fails, nothing of the summary is written and all the problems are
//!   reported. Every board of an order is routed and checked again when the order runs, even if a
//!   plain `fab` made it; a cache of finished packages is a later optimisation (not built, issue #62).
//! - **`order.txt`**, for a person: the number of board designs and of boards; for each board its
//!   copies, copper layers, size in mm, thickness (or "thickness open"), the finish (`finish not
//!   stated`: the process definitions do not carry a surface finish, and netsmith will not say one it
//!   does not know; a sourced surface-finish field is not built, issue #62), the parts the board house assembles and on how many BOM lines, the
//!   hand-solder parts; the combined bill of materials; "before you order", each board's
//!   `fab-notes.txt` lines; and the total of unverified items, counted once for each board and not
//!   once for each copy because the check is of the design (`Package::unverified`).
//! - **`order-bom.csv`**: `Comment,Footprint,LCSC Part #,Quantity,Used on`, one row for each LCSC
//!   number. The quantity is the copies of each board times the parts of that number on one board,
//!   summed over the boards, with **no spares or overage**: how many spares to buy is a decision
//!   about the assembler's minimums and the user's risk, not a fact of the design. `Used on` reads
//!   `2 x 4 DevKit/carrier; 10 x 1 Unit` (copies x parts on one board) so a number can be checked
//!   by hand. It is named apart from each board's `bom.csv`, which is the upload JLCPCB reads for
//!   one board and has other columns.
//! - **The editor's Build panel** has an Order section: a select of the manifest's orders, the
//!   picked order's boards with quantities, and a "Make order" button that runs the same function
//!   ([`fab_order`]). The server has `GET /api/orders`, `POST /api/order?name=` and `GET
//!   /api/order/download?name=&file=` (`summary` or `bom`); orders are read when the editor
//!   starts, like the rest of the manifest.
//! - **The pick lock** (ADR 0100) is read for each source file that builds a board of the
//!   order, and only for those: the boards of that file are solved held to `<stem>.lock.toml`,
//!   so an order gets the rows `fab` of the same file gets. What `fab` does with the file, an
//!   order does: an absent lock is written (the picks of a file are the picks `fab` would
//!   write, whatever boards the order takes from it), an existing lock is never rewritten, and
//!   a difference is a note. With `--locked` ([`OrderLocks::Locked`]) an order refuses, before
//!   it removes or writes anything, a lock that does not exist or that the picks of any of
//!   those files differ from, and says all of them.
//! - **A key may be a panel** (ADR 0040), and its quantity counts panels. The questions that
//!   ADR 0035 left open for panels (how a panel references boards, the quantity inside a panel,
//!   where it lives, what `fab` writes) were decided by ADR 0040.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use netsmith_check::Loader;
use netsmith_fab::{BomEntry, KicadCli, Package};
use netsmith_pcb::Board;
use netsmith_solve::Catalogs;
use serde::{Deserialize, Serialize};
use toml::Spanned;

use crate::fab::{FabOptions, Fabricated, fab_with_options};
use crate::pick_lock::{self, Use};
use crate::project::{MANIFEST, Target, is_project_name};
use crate::{Built, Libraries, outcome_with, render_problems};

/// The directory of `fab` that holds the orders: `fab/orders/<name>/`.
pub const ORDERS_DIR: &str = "orders";
/// The summary an order writes, for a person to read before ordering.
pub const SUMMARY_FILE: &str = "order.txt";
/// The bill of materials of the whole order.
pub const BOM_FILE: &str = "order-bom.csv";

/// One `[order.<name>.boards]` table of the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    pub name: String,
    /// The line of the manifest it starts on.
    pub line: usize,
    /// In the order they are written in the manifest.
    pub boards: Vec<OrderBoard>,
}

/// One line of an order: a board or a system, and how many.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderBoard {
    /// `Unit`, `DevKit/carrier`, or a system's name for all its members.
    pub name: String,
    pub quantity: u32,
    /// The line of the manifest it is written on.
    pub line: usize,
}

impl Order {
    /// `DevKit/carrier x2, Unit x10`, for listings.
    pub fn describe(&self) -> String {
        self.boards
            .iter()
            .map(|b| match b.quantity {
                1 => b.name.clone(),
                n => format!("{} x{n}", b.name),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// What the `order` tables hold as the TOML crate reads them, with the position of every
/// name and number so a mistake can name its line.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawOrder {
    boards: BTreeMap<Spanned<String>, Spanned<u32>>,
}

pub(crate) type RawOrders = BTreeMap<Spanned<String>, RawOrder>;

/// What is written back: the same tables, without positions.
#[derive(Serialize)]
pub(crate) struct RenderedOrder {
    boards: BTreeMap<String, u32>,
}

pub(crate) fn rendered(orders: &[Order]) -> BTreeMap<String, RenderedOrder> {
    orders
        .iter()
        .map(|o| {
            let boards = o.boards.iter().map(|b| (b.name.clone(), b.quantity));
            (
                o.name.clone(),
                RenderedOrder {
                    boards: boards.collect(),
                },
            )
        })
        .collect()
}

/// The 1-based line of byte `offset` of `text`.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

/// Check the orders the TOML crate read: names, quantities and the form of each board name.
/// What needs the project's boards (does this board exist?) is [`resolve`]'s.
pub(crate) fn check(text: &str, raw: RawOrders) -> Result<Vec<Order>, String> {
    let mut orders = Vec::new();
    for (name, order) in raw {
        let line = line_of(text, name.span().start);
        let fail = |line: usize, what: String| format!("{MANIFEST}:{line}: order `{name}`: {what}");
        if !is_project_name(name.get_ref()) {
            return Err(format!(
                "{MANIFEST}:{line}: order name {:?} must be letters, digits, `-` and `_`, and not empty (it is a folder name)",
                name.get_ref()
            ));
        }
        if order.boards.is_empty() {
            return Err(fail(line, "lists no boards".to_string()));
        }
        let mut boards = Vec::new();
        for (board, quantity) in order.boards {
            let line = line_of(text, board.span().start);
            if !is_board_name(board.get_ref()) {
                return Err(fail(
                    line,
                    format!(
                        "{:?} is not a board name; write `Board`, `System/member`, or `System` for all of a system's boards",
                        board.get_ref()
                    ),
                ));
            }
            if *quantity.get_ref() == 0 {
                return Err(fail(
                    line,
                    format!(
                        "the quantity of {:?} is 0; leave the board out to not order it",
                        board.get_ref()
                    ),
                ));
            }
            boards.push(OrderBoard {
                name: board.into_inner(),
                quantity: quantity.into_inner(),
                line,
            });
        }
        // The TOML crate hands a table over by key; the file's order is what a reader and
        // the messages follow.
        boards.sort_by_key(|b| b.line);
        orders.push(Order {
            name: name.into_inner(),
            line,
            boards,
        });
    }
    Ok(orders)
}

/// `Board` or `System/member`: one or two names, none of them empty.
fn is_board_name(name: &str) -> bool {
    let parts: Vec<&str> = name.split('/').collect();
    parts.len() <= 2
        && parts.iter().all(|p| {
            let mut chars = p.chars();
            chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && chars.all(|c| c.is_alphanumeric() || c == '_')
        })
}

/// One board of an order, with the quantity asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// The board's output folder: `Unit`, `DevKit/carrier`.
    pub label: String,
    pub quantity: u32,
}

/// The boards of `order`, with quantities, in the order of `boards` (every board of the
/// project, as its folder names it). A name that is a board selects it; one that is a
/// system selects all its members. Every name that matches nothing, and every board named
/// twice, is one error line naming the manifest line.
pub fn resolve(order: &Order, boards: &[String]) -> Result<Vec<Selected>, String> {
    let mut problems = Vec::new();
    // Board label -> the order's line that selected it, and its quantity.
    let mut chosen: BTreeMap<&str, (&OrderBoard, u32)> = BTreeMap::new();
    for entry in &order.boards {
        let prefix = format!("{}/", entry.name);
        let matches: Vec<&String> = if boards.contains(&entry.name) {
            boards.iter().filter(|b| **b == entry.name).collect()
        } else {
            boards.iter().filter(|b| b.starts_with(&prefix)).collect()
        };
        if matches.is_empty() {
            problems.push(format!(
                "{MANIFEST}:{}: order `{}` names `{}`, but the project has no such board, panel or system; the boards and panels are: {}",
                entry.line,
                order.name,
                entry.name,
                if boards.is_empty() {
                    "(none)".to_string()
                } else {
                    boards.join(", ")
                }
            ));
        }
        for label in matches {
            match chosen.get(label.as_str()) {
                Some((first, _)) => problems.push(format!(
                    "{MANIFEST}:{}: order `{}` names `{label}` twice: as `{}` on line {} and as `{}` here",
                    entry.line, order.name, first.name, first.line, entry.name
                )),
                None => {
                    chosen.insert(label, (entry, entry.quantity));
                }
            }
        }
    }
    if !problems.is_empty() {
        return Err(problems.join("\n") + "\n");
    }
    Ok(boards
        .iter()
        .filter_map(|b| {
            chosen.get(b.as_str()).map(|(_, quantity)| Selected {
                label: b.clone(),
                quantity: *quantity,
            })
        })
        .collect())
}

/// What the summary needs to know of a finished board.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardFacts {
    pub layers: u32,
    pub width_mm: f64,
    pub height_mm: f64,
    /// `None` when the board leaves the thickness open.
    pub thickness_mm: Option<f64>,
    /// Parts the board house places from its own stock (the consigned ones are in `consigned`).
    pub assembled_parts: usize,
    /// The BOM lines of those parts, each with a part number.
    pub bom: Vec<BomEntry>,
    /// The BOM lines of the consigned parts, which the owner ships to the board house
    /// (ADR 0084). They are not in `bom`, so the bill of materials of the order does not
    /// count them as parts to buy.
    pub consigned: Vec<BomEntry>,
    pub hand_solder: Vec<String>,
    pub not_fitted: Vec<String>,
    /// Parts to be assembled that have no part number (only when allowed through).
    pub unassembled: Vec<String>,
    /// Parts whose rotation or side could not be verified (see `fab-notes.txt`).
    pub unverified: Vec<String>,
    /// For a panel (ADR 0040), what it is made of; `None` for a board. A panel's size, bill
    /// of materials and lists of parts are those of the whole panel.
    pub panel: Option<PanelFacts>,
}

/// What a panel is made of, for an order's summary.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelFacts {
    /// The board it repeats.
    pub board: String,
    pub columns: u32,
    pub rows: u32,
    pub cell_width_mm: f64,
    pub cell_height_mm: f64,
    /// The rail on all four sides, 0 for none.
    pub rail_mm: f64,
}

impl PanelFacts {
    pub fn cells(&self) -> u32 {
        self.columns * self.rows
    }
}

/// The parts of a plan that the board house places from its own stock: the placed parts less
/// the consigned ones.
pub(crate) fn stock_parts(plan: &netsmith_fab::Plan) -> usize {
    plan.assembled.len()
        - plan
            .consigned
            .iter()
            .map(|e| e.designators.len())
            .sum::<usize>()
}

impl BoardFacts {
    pub fn of(board: &Board, package: &Package) -> Self {
        let stackup = &board.process.stackup;
        Self {
            layers: stackup.layers,
            width_mm: board.outline.width(),
            height_mm: board.outline.height(),
            thickness_mm: stackup.chosen_thickness(),
            assembled_parts: stock_parts(&package.plan),
            bom: package.plan.bom.clone(),
            consigned: package.plan.consigned.clone(),
            hand_solder: package.plan.hand_solder.clone(),
            not_fitted: package.plan.not_fitted.clone(),
            unassembled: package.plan.unassembled.clone(),
            unverified: package.unverified.clone(),
            panel: None,
        }
    }
}

/// One line of the order's bill of materials: a part number, how many in all, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BomLine {
    pub lcsc: String,
    pub value: String,
    pub footprint: String,
    /// The quantity of the order: the sum over boards of copies x parts on one board.
    pub quantity: u64,
    /// `(board, copies, parts on one board)`.
    pub used_on: Vec<(String, u32, usize)>,
}

/// The bill of materials of the whole order, one line for each part number, by number.
/// A line's quantity is the copies of each board times its parts, with no overage.
pub fn combined_bom(boards: &[(Selected, &BoardFacts)]) -> Vec<BomLine> {
    let mut lines: Vec<BomLine> = Vec::new();
    for (selected, facts) in boards {
        for entry in &facts.bom {
            let Some(lcsc) = &entry.lcsc else { continue };
            let per_board = entry.designators.len();
            let position = match lines.iter().position(|l| &l.lcsc == lcsc) {
                Some(i) => i,
                None => {
                    lines.push(BomLine {
                        lcsc: lcsc.clone(),
                        value: entry.value.clone(),
                        footprint: entry.footprint.clone(),
                        quantity: 0,
                        used_on: Vec::new(),
                    });
                    lines.len() - 1
                }
            };
            let line = &mut lines[position];
            line.quantity += u64::from(selected.quantity) * per_board as u64;
            line.used_on
                .push((selected.label.clone(), selected.quantity, per_board));
        }
    }
    lines.sort_by_key(|l| {
        (
            l.lcsc
                .trim_start_matches('C')
                .parse::<u64>()
                .unwrap_or(u64::MAX),
            l.lcsc.clone(),
        )
    });
    lines
}

/// A number of millimetres as short as it reads: `50`, `12.5`.
fn mm(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The text of `order-bom.csv`: `Comment,Footprint,LCSC Part #,Quantity,Used on`, where the
/// last column reads `2 x 4 DevKit/carrier; 10 x 1 Unit` (copies x parts on one board).
pub fn bom_csv(lines: &[BomLine]) -> String {
    let mut out = String::from("Comment,Footprint,LCSC Part #,Quantity,Used on\n");
    for l in lines {
        let used: Vec<String> = l
            .used_on
            .iter()
            .map(|(board, copies, parts)| format!("{copies} x {parts} {board}"))
            .collect();
        out.push_str(&format!(
            "{},{},{},{},{}\n",
            csv_field(&l.value),
            csv_field(&l.footprint),
            csv_field(&l.lcsc),
            l.quantity,
            csv_field(&used.join("; "))
        ));
    }
    out
}

/// The text of `order.txt`.
pub fn summary_text(
    order: &str,
    boards: &[(Selected, &BoardFacts, &str)],
    bom: &[BomLine],
) -> String {
    let pieces: u64 = boards
        .iter()
        .filter(|(_, f, _)| f.panel.is_none())
        .map(|(s, ..)| u64::from(s.quantity))
        .sum();
    let panels: u64 = boards
        .iter()
        .filter(|(_, f, _)| f.panel.is_some())
        .map(|(s, ..)| u64::from(s.quantity))
        .sum();
    let on_panels: u64 = boards
        .iter()
        .filter_map(|(s, f, _)| f.panel.as_ref().map(|p| u64::from(s.quantity * p.cells())))
        .sum();
    let designs = boards.len();
    let mut s = if panels == 0 {
        format!(
            "order {order}: {designs} board design{}, {pieces} board{} in all\n\n",
            if designs == 1 { "" } else { "s" },
            if pieces == 1 { "" } else { "s" },
        )
    } else {
        format!(
            "order {order}: {designs} design{}, {pieces} board{} and {panels} panel{} in all (the panels hold {on_panels} boards)\n\n",
            if designs == 1 { "" } else { "s" },
            if pieces == 1 { "" } else { "s" },
            if panels == 1 { "" } else { "s" },
        )
    };
    s.push_str("boards\n");
    for (selected, facts, _) in boards {
        let thickness = facts
            .thickness_mm
            .map_or("thickness open".to_string(), |t| {
                format!("{} mm thick", mm(t))
            });
        s.push_str(&format!(
            "  {} x{}: {} layers, {} x {} mm, {thickness}, finish not stated\n",
            selected.label,
            selected.quantity,
            facts.layers,
            mm(facts.width_mm),
            mm(facts.height_mm),
        ));
        if let Some(p) = &facts.panel {
            s.push_str(&format!(
                "    a panel of {} x {} {} ({} x {} mm each), V-cut, {}\n",
                p.columns,
                p.rows,
                p.board,
                mm(p.cell_width_mm),
                mm(p.cell_height_mm),
                if p.rail_mm > 0.0 {
                    format!("{} mm rails", mm(p.rail_mm))
                } else {
                    "no rails".to_string()
                }
            ));
        }
        let lines = facts.bom.len();
        s.push_str(&format!(
            "    assembled by the board house: {} part{} on {lines} BOM line{}\n",
            facts.assembled_parts,
            if facts.assembled_parts == 1 { "" } else { "s" },
            if lines == 1 { "" } else { "s" },
        ));
        if facts.hand_solder.is_empty() {
            s.push_str("    hand-solder: none\n");
        } else {
            s.push_str(&format!(
                "    hand-solder: {} ({} part{} on each {}, {} in all)\n",
                facts.hand_solder.join(", "),
                facts.hand_solder.len(),
                if facts.hand_solder.len() == 1 {
                    ""
                } else {
                    "s"
                },
                if facts.panel.is_some() {
                    "panel"
                } else {
                    "board"
                },
                facts.hand_solder.len() as u64 * u64::from(selected.quantity),
            ));
        }
        if !facts.consigned.is_empty() {
            let parts: Vec<String> = facts
                .consigned
                .iter()
                .map(|e| format!("{} ({})", e.designators.join(", "), e.value))
                .collect();
            let count: usize = facts.consigned.iter().map(|e| e.designators.len()).sum();
            s.push_str(&format!(
                "    consigned: {} ({count} part{} on each {}, {} in all): ship them to the board house, which cannot go on without them\n",
                parts.join("; "),
                if count == 1 { "" } else { "s" },
                if facts.panel.is_some() { "panel" } else { "board" },
                count as u64 * u64::from(selected.quantity),
            ));
        }
        if !facts.not_fitted.is_empty() {
            s.push_str(&format!(
                "    not fitted: {}\n",
                facts.not_fitted.join(", ")
            ));
        }
        if !facts.unassembled.is_empty() {
            s.push_str(&format!(
                "    unassembled (no LCSC part number): {}\n",
                facts.unassembled.join(", ")
            ));
        }
    }

    s.push_str(
        "\nbill of materials (copies x parts on one board, no spares; also in order-bom.csv)\n",
    );
    if bom.is_empty() {
        s.push_str("  no assembled parts\n");
    }
    for l in bom {
        let used: Vec<String> = l
            .used_on
            .iter()
            .map(|(board, copies, parts)| format!("{copies} x {parts} {board}"))
            .collect();
        s.push_str(&format!(
            "  {:<10} {:>6}  {} {} ({})\n",
            l.lcsc,
            l.quantity,
            l.value,
            l.footprint,
            used.join("; ")
        ));
    }

    s.push_str("\nbefore you order\n");
    for (selected, _, notes) in boards {
        s.push_str(&format!("  {}\n", selected.label));
        let lines: Vec<&str> = notes
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with("fab-notes for"))
            .collect();
        for l in lines {
            s.push_str(&format!("    {l}\n"));
        }
    }

    let unverified: usize = boards.iter().map(|(_, f, _)| f.unverified.len()).sum();
    s.push_str(&format!(
        "\nunverified items: {unverified}{}\n",
        if unverified == 0 {
            String::new()
        } else {
            " (part rotations and sides to check in the board house's order preview, counted once for each board and not for each copy)".to_string()
        }
    ));
    s
}

/// An order, made.
#[derive(Debug)]
pub struct Made {
    pub name: String,
    /// `<fab>/orders/<name>`.
    pub dir: PathBuf,
    /// Each board's package, with the copies ordered, in the project's order.
    pub boards: Vec<(Selected, Fabricated)>,
    pub bom: Vec<BomLine>,
    /// The text of `order.txt`.
    pub summary: String,
    /// `order.txt` and `order-bom.csv`.
    pub files: Vec<PathBuf>,
}

/// Every board and panel of the project, as its folder names it, with the index of the source file
/// that builds it. Only the checker and solver run (no KiCad is needed).
pub fn project_boards(
    target: &Target,
    loader: &Loader,
    catalogs: &Catalogs,
) -> Result<Vec<(String, usize)>, String> {
    let mut found: Vec<(String, usize)> = Vec::new();
    for (i, source) in target.files.iter().enumerate() {
        let text =
            fs::read_to_string(&source.path).map_err(|e| format!("{}: {e}\n", source.name))?;
        let outcome = outcome_with(&source.name, &text, loader, catalogs, None)
            .map_err(|problems| render_problems(&problems))?;
        // The boards, then the panels (ADR 0040): an order names either.
        let labels = outcome
            .boards
            .iter()
            .map(Built::label)
            .chain(outcome.panels.iter().map(|p| p.name.clone()));
        for label in labels {
            if let Some((_, first)) = found.iter().find(|(l, _)| *l == label) {
                return Err(format!(
                    "error: the board or panel {label} is built by both {} and {}; their names are the folders they are written to and must differ\n",
                    target.files[*first].name, source.name
                ));
            }
            found.push((label, i));
        }
    }
    Ok(found)
}

/// What an order does with the pick lock beside each source file it makes boards from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderLocks {
    /// Leaves the lock out: the boards are solved from the source alone (the editor's order).
    Ignored,
    /// As `fab` does without `--locked`: holds the parts to the lock when the file exists and
    /// writes it when it does not.
    Held,
    /// As `fab --locked`: holds the parts to the lock, and refuses a file that does not exist
    /// or that the picks differ from.
    Locked,
}

/// The catalogs to solve `source` with and what to do with its lock, for each file with
/// boards in the order (`None` for the others, and for every file when the locks are ignored).
type Opened = Option<(Option<Catalogs>, Use)>;

/// With `--locked`, every lock of `files` is compared with the picks before anything is made,
/// and every one that would change is said. Nothing is written.
fn refuse_changed_locks(
    files: &[(usize, &Opened)],
    target: &Target,
    loader: &Loader,
    catalogs: &Catalogs,
) -> Result<(), String> {
    let mut problems = String::new();
    for (i, opened) in files {
        let Some((held, using)) = opened else {
            continue;
        };
        let source = &target.files[*i];
        let text =
            fs::read_to_string(&source.path).map_err(|e| format!("{}: {e}\n", source.name))?;
        let catalogs = held.as_ref().unwrap_or(catalogs);
        match outcome_with(&source.name, &text, loader, catalogs, None) {
            Err(found) => problems.push_str(&render_problems(&found)),
            Ok(outcome) => {
                if let Err(e) = pick_lock::settle(using, catalogs.pick_lock(), &outcome.lock) {
                    problems.push_str(&format!("{e}\n"));
                }
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// Build the boards of `order` and write the package of each, `order.txt` and
/// `order-bom.csv` under `<out>/orders/<name>/`. On failure the error is every problem.
/// `locks` says what is done with the pick lock of each source file that builds a board of
/// the order ([`OrderLocks`]).
#[allow(clippy::too_many_arguments)]
pub fn fab_order(
    order: &Order,
    target: &Target,
    loader: &Loader,
    catalogs: &Catalogs,
    libraries: &Libraries,
    kicad: Option<&dyn KicadCli>,
    options: &FabOptions,
    locks: OrderLocks,
    out: &Path,
) -> Result<Made, String> {
    let found = project_boards(target, loader, catalogs)?;
    if let Some((label, _)) = found
        .iter()
        .find(|(l, _)| l.split('/').next() == Some(ORDERS_DIR))
    {
        return Err(format!(
            "error: the board {label} is written to {out}/{ORDERS_DIR}/..., the folder of the orders; rename it\n",
            out = out.display()
        ));
    }
    let labels: Vec<String> = found.iter().map(|(l, _)| l.clone()).collect();
    let selected = resolve(order, &labels)?;
    // The boards of the order that each source file builds, and the lock of each of those files.
    let mut files: Vec<(usize, Vec<String>, Opened)> = Vec::new();
    for (i, source) in target.files.iter().enumerate() {
        let mine: Vec<String> = selected
            .iter()
            .filter(|s| found.iter().any(|(l, f)| *l == s.label && *f == i))
            .map(|s| s.label.clone())
            .collect();
        if mine.is_empty() {
            continue;
        }
        let opened = match locks {
            OrderLocks::Ignored => None,
            OrderLocks::Held | OrderLocks::Locked => Some(pick_lock::open(
                &source.path,
                catalogs,
                locks == OrderLocks::Locked,
            )?),
        };
        files.push((i, mine, opened));
    }
    if locks == OrderLocks::Locked {
        let opened: Vec<(usize, &Opened)> = files.iter().map(|(i, _, o)| (*i, o)).collect();
        refuse_changed_locks(&opened, target, loader, catalogs)?;
    }
    let dir = out.join(ORDERS_DIR).join(&order.name);
    // An order folder netsmith made before is replaced whole, so that a board dropped from
    // the order does not stay in it. Anything else at that path is left alone.
    if dir.join(SUMMARY_FILE).is_file() {
        fs::remove_dir_all(&dir).map_err(|e| format!("error: {}: {e}\n", dir.display()))?;
    }

    let mut made: Vec<(Selected, Fabricated)> = Vec::new();
    let mut problems = String::new();
    for (i, mine, opened) in files {
        let source = &target.files[i];
        let text =
            fs::read_to_string(&source.path).map_err(|e| format!("{}: {e}\n", source.name))?;
        let (held, using) = match opened {
            Some((held, using)) => (held, Some(using)),
            None => (None, None),
        };
        let file_options = FabOptions {
            only: Some(mine),
            layout: Some(crate::layout::layout_path(&source.path)),
            lock: using,
            pin_map_source: Some(source.stable_name(target.project.as_ref())),
            ..options.clone()
        };
        match fab_with_options(
            &source.name,
            &text,
            loader,
            &dir,
            held.as_ref().unwrap_or(catalogs),
            libraries,
            kicad,
            &file_options,
        ) {
            Err(e) => problems.push_str(&e),
            Ok(boards) => {
                for fabricated in boards {
                    let selected = selected
                        .iter()
                        .find(|s| s.label == fabricated.board)
                        .cloned()
                        .expect("only the selected boards were made");
                    made.push((selected, fabricated));
                }
            }
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    // In the project's order.
    made.sort_by_key(|(s, _)| labels.iter().position(|l| *l == s.label));

    let rows: Vec<(Selected, &BoardFacts)> =
        made.iter().map(|(s, f)| (s.clone(), &f.facts)).collect();
    let bom = combined_bom(&rows);
    let listed: Vec<(Selected, &BoardFacts, &str)> = made
        .iter()
        .map(|(s, f)| (s.clone(), &f.facts, f.notes.as_str()))
        .collect();
    let summary = summary_text(&order.name, &listed, &bom);
    let mut files = Vec::new();
    for (name, text) in [(SUMMARY_FILE, summary.clone()), (BOM_FILE, bom_csv(&bom))] {
        let path = dir.join(name);
        fs::write(&path, text).map_err(|e| format!("error: {}: {e}\n", path.display()))?;
        files.push(path);
    }
    Ok(Made {
        name: order.name.clone(),
        dir,
        boards: made,
        bom,
        summary,
        files,
    })
}

/// What `netsmith fab --order` with no name prints.
pub fn list(orders: &[Order]) -> String {
    if orders.is_empty() {
        return format!(
            "{MANIFEST} has no orders; add one:\n\n[order.devkit.boards]\n\"System/member\" = 2\nBoard = 10\n"
        );
    }
    let mut s = format!("orders in {MANIFEST}:\n");
    for o in orders {
        s.push_str(&format!("  {}: {}\n", o.name, o.describe()));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Manifest;

    fn manifest(orders: &str) -> Result<Manifest, String> {
        Manifest::parse(&format!("[project]\nname = \"p\"\n{orders}"))
    }

    fn order(text: &str) -> Order {
        manifest(text).unwrap().orders.remove(0)
    }

    fn boards(names: &[&str]) -> Vec<String> {
        names.iter().map(ToString::to_string).collect()
    }

    const DEVKIT: &str = "\n[order.devkit.boards]\n\"DevKit/carrier\" = 2\nUnit = 10\n";

    #[test]
    fn an_order_is_a_table_of_boards_and_quantities() {
        let o = order(DEVKIT);
        assert_eq!(o.name, "devkit");
        assert_eq!(o.line, 4);
        assert_eq!(
            o.boards,
            [
                OrderBoard {
                    name: "DevKit/carrier".into(),
                    quantity: 2,
                    line: 5
                },
                OrderBoard {
                    name: "Unit".into(),
                    quantity: 10,
                    line: 6
                },
            ]
        );
        assert_eq!(o.describe(), "DevKit/carrier x2, Unit x10");
    }

    #[test]
    fn orders_survive_a_round_trip() {
        let m = manifest(&format!("{DEVKIT}\n[order.one.boards]\nDevKit = 1\n")).unwrap();
        assert_eq!(m.orders.len(), 2);
        let again = Manifest::parse(&m.render()).unwrap();
        assert_eq!(again.orders.len(), 2);
        assert_eq!(again.orders[0].boards[0].name, "DevKit/carrier");
        assert_eq!(again.orders[0].boards[0].quantity, 2);
        assert!(!Manifest::new("p").render().contains("order"));
    }

    #[test]
    fn manifest_mistakes_name_the_line() {
        let e = manifest("\n[order.d.boards]\nUnit = 0\n").unwrap_err();
        assert!(
            e.starts_with("netsmith.toml:5:")
                && e.contains("order `d`")
                && e.contains("\"Unit\" is 0"),
            "{e}"
        );
        let e = manifest("\n[order.d.boards]\nUnit = -1\n").unwrap_err();
        assert!(e.contains("line 5") && e.contains("-1"), "{e}");
        let e = manifest("\n[order.d.boards]\nUnit = \"two\"\n").unwrap_err();
        assert!(e.contains("line 5"), "{e}");
        let e = manifest("\n[order.d.boards]\nUnit = 1\nUnit = 2\n").unwrap_err();
        assert!(e.contains("line 6") && e.contains("duplicate key"), "{e}");
        let e = manifest("\n[order.d.boards]\nUnit = 1\n[order.d.boards]\n").unwrap_err();
        assert!(e.contains("line 6") || e.contains("line 5"), "{e}");
        let e = manifest("\n[order.d]\n").unwrap_err();
        assert!(
            e.contains("missing field `boards`") && e.contains("line 4"),
            "{e}"
        );
        let e = manifest("\n[order.d]\nboards = {}\n").unwrap_err();
        assert!(
            e.starts_with("netsmith.toml:4:") && e.contains("lists no boards"),
            "{e}"
        );
        let e = manifest("\n[order.d]\nquantity = 2\n").unwrap_err();
        assert!(
            e.contains("unknown field `quantity`") && e.contains("line 5"),
            "{e}"
        );
        let e = manifest("\n[order.\"two words\".boards]\nUnit = 1\n").unwrap_err();
        assert!(
            e.starts_with("netsmith.toml:4:") && e.contains("two words"),
            "{e}"
        );
        let e = manifest("\n[order.d.boards]\n\"A/b/c\" = 1\n").unwrap_err();
        assert!(
            e.starts_with("netsmith.toml:5:") && e.contains("\"A/b/c\" is not a board name"),
            "{e}"
        );
        let e = manifest("\n[order.d.boards]\n\"/b\" = 1\n").unwrap_err();
        assert!(e.contains("is not a board name"), "{e}");
        let e = manifest("\n[orders.d.boards]\nUnit = 1\n").unwrap_err();
        assert!(
            e.contains("unknown field `orders`") && e.contains("line 4"),
            "{e}"
        );
    }

    #[test]
    fn a_system_selects_its_members_and_boards_come_in_project_order() {
        let o = order("\n[order.d.boards]\nUnit = 10\nDevKit = 2\n");
        let all = boards(&["DevKit/carrier", "DevKit/power_card", "Unit", "Other"]);
        let chosen = resolve(&o, &all).unwrap();
        let got: Vec<(&str, u32)> = chosen
            .iter()
            .map(|s| (s.label.as_str(), s.quantity))
            .collect();
        assert_eq!(
            got,
            [
                ("DevKit/carrier", 2),
                ("DevKit/power_card", 2),
                ("Unit", 10)
            ]
        );
    }

    #[test]
    fn an_unknown_board_lists_the_boards_that_exist() {
        let o = order("\n[order.d.boards]\nUnit = 1\nUnti = 3\n\"DevKit/nope\" = 1\n");
        let e = resolve(&o, &boards(&["DevKit/carrier", "Unit"])).unwrap_err();
        assert!(
            e.contains("netsmith.toml:6: order `d` names `Unti`")
                && e.contains("netsmith.toml:7: order `d` names `DevKit/nope`")
                && e.contains("the boards and panels are: DevKit/carrier, Unit"),
            "{e}"
        );
        let e = resolve(&o, &[]).unwrap_err();
        assert!(e.contains("the boards and panels are: (none)"), "{e}");
    }

    #[test]
    fn a_board_named_twice_is_refused() {
        let o = order("\n[order.d.boards]\nDevKit = 1\n\"DevKit/carrier\" = 2\n");
        let e = resolve(&o, &boards(&["DevKit/carrier", "DevKit/power_card"])).unwrap_err();
        assert!(
            e.contains("names `DevKit/carrier` twice")
                && e.contains("as `DevKit` on line 5")
                && e.contains("as `DevKit/carrier` here"),
            "{e}"
        );
    }

    fn entry(designators: &[&str], value: &str, lcsc: &str) -> BomEntry {
        BomEntry {
            designators: designators.iter().map(ToString::to_string).collect(),
            value: value.into(),
            footprint: "C_0402_1005Metric".into(),
            lcsc: Some(lcsc.into()),
        }
    }

    fn facts(bom: Vec<BomEntry>) -> BoardFacts {
        BoardFacts {
            layers: 2,
            width_mm: 50.0,
            height_mm: 40.5,
            thickness_mm: Some(1.6),
            assembled_parts: bom.iter().map(|e| e.designators.len()).sum(),
            bom,
            consigned: vec![],
            hand_solder: vec!["J1".into(), "J2".into()],
            not_fitted: vec![],
            unassembled: vec![],
            unverified: vec!["D1".into()],
            panel: None,
        }
    }

    fn pick(label: &str, quantity: u32) -> Selected {
        Selected {
            label: label.into(),
            quantity,
        }
    }

    #[test]
    fn the_combined_bom_is_copies_times_parts_with_no_spares() {
        let carrier = facts(vec![entry(&["C1", "C2", "C3", "C4"], "100n", "C1525")]);
        let unit = facts(vec![
            entry(&["C1"], "100n", "C1525"),
            entry(&["R1", "R2"], "10k", "C25744"),
        ]);
        let lines = combined_bom(&[
            (pick("DevKit/carrier", 2), &carrier),
            (pick("Unit", 10), &unit),
        ]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].lcsc, "C1525");
        assert_eq!(lines[0].quantity, 2 * 4 + 10, "{lines:?}");
        assert_eq!(
            lines[0].used_on,
            [
                ("DevKit/carrier".to_string(), 2, 4),
                ("Unit".to_string(), 10, 1)
            ]
        );
        assert_eq!(lines[1].lcsc, "C25744");
        assert_eq!(lines[1].quantity, 20);
        assert_eq!(
            bom_csv(&lines),
            "Comment,Footprint,LCSC Part #,Quantity,Used on\n100n,C_0402_1005Metric,C1525,18,2 x 4 DevKit/carrier; 10 x 1 Unit\n10k,C_0402_1005Metric,C25744,20,10 x 2 Unit\n"
        );
    }

    #[test]
    fn the_summary_lists_each_board_the_bom_the_notes_and_the_unverified_total() {
        let carrier = facts(vec![entry(&["C1", "C2"], "100n", "C1525")]);
        let unit = facts(vec![entry(&["C1"], "100n", "C1525")]);
        let boards_made = [
            (
                pick("DevKit/carrier", 2),
                &carrier,
                "fab-notes for carrier\nhand-solder: J1, J2\nrotation unverified: D1\n",
            ),
            (
                pick("Unit", 10),
                &unit,
                "fab-notes for Unit\nprocess: JLCPCB TwoLayer\n",
            ),
        ];
        let bom = combined_bom(&[
            (pick("DevKit/carrier", 2), &carrier),
            (pick("Unit", 10), &unit),
        ]);
        let text = summary_text("devkit", &boards_made, &bom);
        for expected in [
            "order devkit: 2 board designs, 12 boards in all",
            "DevKit/carrier x2: 2 layers, 50 x 40.5 mm, 1.6 mm thick, finish not stated",
            "assembled by the board house: 2 parts on 1 BOM line",
            "hand-solder: J1, J2 (2 parts on each board, 4 in all)",
            "C1525          14  100n C_0402_1005Metric (2 x 2 DevKit/carrier; 10 x 1 Unit)",
            "before you order\n  DevKit/carrier\n    hand-solder: J1, J2\n    rotation unverified: D1\n  Unit\n    process: JLCPCB TwoLayer\n",
            "unverified items: 2 (part rotations and sides to check",
        ] {
            assert!(text.contains(expected), "missing {expected:?} in\n{text}");
        }
        assert!(!text.contains("fab-notes for"), "{text}");
    }

    #[test]
    fn the_listing_names_each_order_and_says_what_to_write_when_there_is_none() {
        let o = order(DEVKIT);
        assert_eq!(
            list(std::slice::from_ref(&o)),
            "orders in netsmith.toml:\n  devkit: DevKit/carrier x2, Unit x10\n"
        );
        assert!(list(&[]).contains("[order.devkit.boards]"));
    }
}
