use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_editor::Backend;
use netsmith_editor::api::{Budget, Draw, EdgeKind, PowerTree, Range, Severity};

use super::views::{self, Kind, Printing};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board power` prints the power tree the editor's Power view draws for a board: each rail with its voltage, the supplies and loads on it with the current each can give or draw, what the loads add up to (`demand`) against what the supply can give (`capacity`), whether that is within budget, the converters and joins between rails with the efficiency the solve settled, and the power issues (an overdrawn supply is an error; a rail nothing supplies, a load or an efficiency not settled are notes). It is made by the code the editor's `GET /api/power` calls, so the two are the same; `--format json` prints that route's answer. It writes nothing.\n\nWith no file it acts on the project's boards; a source with several boards needs `--board`. The exit code is 0 when the tree is printed (its issues are part of it: `board check` is the command that fails on them), 1 when the board does not build, and 2 when the command cannot run."
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

/// Prints the power tree of one board.
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
        workspace.power(&board),
        &analysis,
        args.printing.format,
        (|v| serde_json::to_string_pretty(v), text),
    )
}

fn range(r: &Option<Range>) -> &str {
    r.as_ref().map_or("not settled", |r| r.text.as_str())
}

fn draws(out: &mut String, label: &str, draws: &[Draw]) {
    for draw in draws {
        out.push_str(&format!(
            "    {label} {} ({}): {}\n",
            draw.who,
            draw.kind,
            range(&draw.range)
        ));
    }
}

fn budget(b: Budget) -> &'static str {
    match b {
        Budget::Within => "within budget",
        Budget::Over => "can exceed what the supply gives",
        Budget::Unlimited => "no limit stated for the supply",
        Budget::Unknown => "a current is not settled",
    }
}

/// The text of the tree: each rail with what it supplies and loads, then the edges, the
/// converters' efficiencies and the issues.
fn text(tree: &PowerTree) -> String {
    let mut out = format!("{}: power tree\n", tree.title);
    for node in &tree.nodes {
        out.push_str(&format!("\n{}", node.net));
        if let Some(board) = &node.board {
            out.push_str(&format!(" (on {board})"));
        }
        out.push_str(&format!(": {}\n", range(&node.voltage)));
        draws(&mut out, "supplied by", &node.supplies);
        draws(&mut out, "load", &node.loads);
        out.push_str(&format!("    demand {}", range(&node.demand)));
        if let Some(capacity) = &node.capacity {
            out.push_str(&format!(", capacity {}", capacity.text));
            if let Some(from) = &node.capacity_from {
                out.push_str(&format!(" (from {from})"));
            }
        }
        out.push_str(&format!(": {}\n", budget(node.budget)));
    }
    if !tree.edges.is_empty() {
        out.push_str("\nBetween rails:\n");
        let name = |id: &str| {
            tree.nodes
                .iter()
                .find(|n| n.id == id)
                .map_or_else(|| id.to_string(), |n| n.net.clone())
        };
        for edge in &tree.edges {
            let how = match edge.kind {
                EdgeKind::Converts => "converts",
                EdgeKind::Join => "joins",
                EdgeKind::Net => "shares a net",
            };
            out.push_str(&format!(
                "    {} -> {} ({how}: {})",
                name(&edge.from),
                name(&edge.to),
                edge.label
            ));
            if let Some(stage) = tree.stages.iter().find(|s| s.label == edge.label) {
                match (&stage.efficiency, stage.efficiency_unsettled) {
                    (Some(e), _) => out.push_str(&format!(", efficiency {}", e.text)),
                    (None, true) => out.push_str(", efficiency not settled"),
                    (None, false) => {}
                }
            }
            out.push('\n');
        }
    }
    if tree.issues.is_empty() {
        out.push_str("\nNo power issues.\n");
    } else {
        out.push_str("\nPower issues, worst first:\n");
        for issue in &tree.issues {
            let severity = match issue.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Note => "note",
            };
            out.push_str(&format!("    {severity}: {} {}", issue.subject, issue.text));
            if let Some(at) = &issue.at {
                out.push_str(&format!(" ({}:{})", at.file, at.line));
            }
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn it_takes_a_file_a_board_and_a_format() {
        let Ok(Leaf::Power(a)) = parse(&["board", "power", "a.nsm", "--board", "B"]) else {
            panic!()
        };
        assert_eq!(
            (a.file.as_deref(), a.board.as_deref()),
            (Some("a.nsm"), Some("B"))
        );
        assert_eq!(a.printing.format, crate::commands::common::Format::Text);
        assert!(parse(&["board", "power", "--offline"]).is_err());
    }
}
