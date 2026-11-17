use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_editor::Backend;
use netsmith_editor::api::PanelView;

use super::views::{self, Kind, Printing};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board panel` prints the panel the editor's Panel view draws (ADR 0040): the board it repeats, the grid, the size of a cell and of the panel, the rails, and the panel checks (an error stops `fab`; the rest is advice). It is made by the code the editor's `GET /api/panel` calls, so the two are the same; `--format json` prints that route's answer, with the drawing as `svg`. It writes nothing.\n\nWith no file it acts on the project's boards; with no `--name` it shows the only panel, and a source with several needs `--name`. The exit code is 0 when the panel is printed, 1 when it does not build, and 2 when the command cannot run (no panel among them)."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The panel to print; needed when there are several.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    #[command(flatten)]
    pub printing: Printing,
}

/// Prints one panel.
pub fn run(args: Args) -> ExitCode {
    let (workspace, analysis) = match views::open(args.file.as_deref(), false) {
        Ok(w) => w,
        Err(code) => return code,
    };
    let name = match views::choose(&analysis, &Kind::Panel, args.name.as_deref()) {
        Ok(n) => n,
        Err(code) => return code,
    };
    views::print(
        workspace.panel(&name),
        &analysis,
        args.printing.format,
        (|v| serde_json::to_string_pretty(v), text),
    )
}

/// The text of the panel: its size and grid, then the checks. The drawing is only in the JSON.
fn text(panel: &PanelView) -> String {
    let mut out = format!(
        "{}: a panel of {} x {} {}\n",
        panel.name, panel.columns, panel.rows, panel.board
    );
    out.push_str(&format!(
        "    cell {} x {} mm, panel {} x {} mm, rails {} mm\n",
        panel.cell_width, panel.cell_height, panel.width, panel.height, panel.rail
    ));
    if panel.findings.is_empty() {
        out.push_str("\nNo panel findings.\n");
    } else {
        out.push_str("\nPanel checks:\n");
        for f in &panel.findings {
            out.push_str(&format!(
                "    {}: {}\n",
                if f.error { "error" } else { "advice" },
                f.message
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn it_takes_a_file_a_name_and_a_format() {
        let Ok(Leaf::Panel(a)) = parse(&["board", "panel", "a.nsm", "--name", "P"]) else {
            panic!()
        };
        assert_eq!(
            (a.file.as_deref(), a.name.as_deref()),
            (Some("a.nsm"), Some("P"))
        );
        assert!(parse(&["board", "panel", "--board", "B"]).is_err());
    }
}
