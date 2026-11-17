use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_editor::Backend;
use netsmith_editor::api::{
    BoardDetail, Location, PartInfo, PickLockInfo, PinInfo, PinRequestInfo, Solved, ValueHow,
};

use super::views::{self, Kind, Printing};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board show` prints a board's parts and nets as the editor's inspector shows them: each part with its designator, kind, value, package, path in the board, LCSC number, whether the board house assembles it, the declaration that made it and the scopes round it, what the solver worked out for it (the effective capacitance and bias, the tolerance, the ratings and the catalog row), the pins its pin requests got and where its package came from; each net with its pins (the pad, and the name and function the part's pin table gives); the board's notes; and the state of the pick lock beside the source (`project lock --check` says the same). It is made by the code the editor's `GET /api/board` calls, so the two are the same; `--format json` prints that route's answer. It writes nothing and needs no `fab`.\n\nWith no file it acts on the project's boards; a source with several boards needs `--board`. The exit code is 0 when the board is printed, 1 when the board does not build, and 2 when the command cannot run."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board to print (`Board`, or `System/Board` or just `Board` for a board of a system); needed when there are several.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    #[command(flatten)]
    pub printing: Printing,
}

/// Prints the parts and nets of one board.
pub fn run(args: Args) -> ExitCode {
    let (workspace, analysis) = match views::open(args.file.as_deref(), false) {
        Ok(w) => w,
        Err(code) => return code,
    };
    let board = match views::choose(&analysis, &Kind::Board, args.board.as_deref()) {
        Ok(b) => b,
        Err(code) => return code,
    };
    views::print(
        workspace.board(&board),
        &analysis,
        args.printing.format,
        (|v| serde_json::to_string_pretty(v), text),
    )
}

fn at(l: &Location) -> String {
    format!("{}:{}:{}", l.file, l.line, l.column)
}

/// What the solver settled for a part, one line for each thing it says.
fn solved(out: &mut String, solved: &Solved) {
    if let Some(c) = &solved.effective_capacitance {
        let bias = solved
            .bias
            .as_ref()
            .map_or(String::new(), |b| format!(" at {}", b.text));
        out.push_str(&format!("    effective capacitance {}{bias}\n", c.text));
    }
    if let Some(t) = &solved.tolerance {
        out.push_str(&format!("    tolerance {}\n", t.text));
    }
    for rating in solved.ratings.iter().flatten() {
        out.push_str(&format!("    {} {}\n", rating.name, rating.range.text));
    }
    if let Some(row) = &solved.row {
        let mut says = Vec::new();
        says.extend(row.catalog.as_ref().map(|c| format!("catalog {c}")));
        says.extend(row.mpn.clone());
        says.extend(row.lcsc.clone());
        says.extend(row.library.clone());
        if row.generic {
            says.push("generic: no row stands for it yet".to_string());
        }
        out.push_str(&format!("    row: {}\n", says.join(", ")));
    }
}

fn request(out: &mut String, r: &PinRequestInfo) {
    let got = match (&r.pin, &r.message) {
        (Some(pin), _) => {
            let pad = r
                .pad
                .as_ref()
                .map_or(String::new(), |p| format!(" (pad {p})"));
            let held = if r.locked { ", locked" } else { "" };
            let from = r
                .from
                .as_ref()
                .map_or(String::new(), |f| format!(", from the {f}"));
            format!("{pin}{pad}{held}{from}")
        }
        (None, Some(why)) => format!("no pin: {why}"),
        (None, None) => "no pin".to_string(),
    };
    out.push_str(&format!(
        "    pin request {} ({}): {got}\n",
        r.request, r.function
    ));
}

fn part(out: &mut String, p: &PartInfo) {
    let facts = [
        Some(format!("kind {}", p.kind)),
        p.package.as_ref().map(|k| format!("package {k}")),
        p.lcsc.as_ref().map(|l| format!("LCSC {l}")),
        Some(
            if p.assembled {
                "assembled"
            } else {
                "soldered by hand"
            }
            .to_string(),
        ),
    ];
    out.push_str(&format!(
        "{} {}: {}\n",
        p.refdes,
        p.value,
        facts.into_iter().flatten().collect::<Vec<_>>().join(", ")
    ));
    out.push_str(&format!("    path {}\n", p.path));
    if let Some(l) = &p.decl {
        out.push_str(&format!("    declared at {}\n", at(l)));
    }
    if !p.scopes.is_empty() {
        let scopes: Vec<String> = p.scopes.iter().map(at).collect();
        out.push_str(&format!("    inside {}\n", scopes.join(", ")));
    }
    if let Some(from) = &p.package_from {
        let how = match from.how {
            ValueHow::Stated => "stated",
            ValueHow::Default => "a default",
            ValueHow::BoardDefault => "a line of the board's defaults",
        };
        out.push_str(&format!("    package is {how} at {}\n", at(&from.at)));
    }
    if let Some(s) = &p.solved {
        solved(out, s);
    }
    for r in p.pin_requests.iter().flatten() {
        request(out, r);
    }
}

fn pin(p: &PinInfo) -> String {
    let mut s = format!("{}.{}", p.refdes, p.pin);
    let named: Vec<&str> = [p.name.as_deref(), p.function.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    if !named.is_empty() {
        s.push_str(&format!(" ({})", named.join(", ")));
    }
    s
}

fn lock(out: &mut String, lock: &PickLockInfo) {
    out.push_str(&format!("\nPick lock {}: ", lock.file));
    if let Some(e) = &lock.error {
        out.push_str(&format!("cannot be checked: {e}\n"));
    } else if !lock.exists {
        out.push_str("none (a source with no part to lock needs none)\n");
    } else if lock.current {
        out.push_str("current\n");
    } else {
        out.push_str("stale; `netsmith project lock --update` writes it again\n");
    }
    for line in &lock.diff {
        out.push_str(&format!("  {line}\n"));
    }
    if !lock.reported.trim().is_empty() {
        out.push_str(&format!("{}\n", lock.reported.trim_end()));
    }
}

/// The text of the board: the parts with what the solve left them, the nets with their pins, the
/// notes and the pick lock.
fn text(board: &BoardDetail) -> String {
    let mut out = format!(
        "{} ({}): {} part(s), {} net(s)\n\nParts:\n",
        board.name,
        board.file,
        board.parts.len(),
        board.nets.len()
    );
    for p in &board.parts {
        part(&mut out, p);
    }
    out.push_str("\nNets:\n");
    let rows: Vec<Vec<String>> = board
        .nets
        .iter()
        .map(|n| {
            vec![
                n.name.clone(),
                if n.named { "named" } else { "" }.to_string(),
                n.pins.iter().map(pin).collect::<Vec<_>>().join(" "),
            ]
        })
        .collect();
    out.push_str(&views::table(&rows));
    if !board.notes.is_empty() {
        out.push_str("\nNotes:\n");
        for note in &board.notes {
            out.push_str(&format!("  {note}\n"));
        }
    }
    if let Some(l) = &board.lock {
        lock(&mut out, l);
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn it_takes_a_file_a_board_and_a_format() {
        let Ok(Leaf::Show(a)) =
            parse(&["board", "show", "a.nsm", "--board", "B", "--format", "json"])
        else {
            panic!()
        };
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.board.as_deref(), Some("B"));
        assert_eq!(a.printing.format, crate::commands::common::Format::Json);
        assert!(parse(&["board", "show", "--offline"]).is_err());
    }
}
