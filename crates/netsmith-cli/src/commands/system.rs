use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_editor::Backend;
use netsmith_editor::api::SystemView;

use super::views::{self, Kind, Printing};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board system` prints the view the editor's System tab draws: for a `system`, its members (boards, modules, parts, ports) with their ports and the joins between them, the mated connectors with what each board has behind each contact, and the grounds the mates join; for a board, its members and the nets between them. It is made by the code the editor's `GET /api/system` calls, so the two are the same; `--format json` prints that route's answer. It writes nothing.\n\nWith no file it acts on the project's boards. With no `--name` it shows the only system, or with no system the only board; a source with several needs `--name`. The exit code is 0 when the view is printed, 1 when the source does not build, and 2 when the command cannot run."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The system to print, or a board (`Board`, or `System/Board`) for its members and nets; needed when there are several.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
    #[command(flatten)]
    pub printing: Printing,
}

/// Prints one system, or the members of one board.
pub fn run(args: Args) -> ExitCode {
    let (workspace, analysis) = match views::open(args.file.as_deref(), false) {
        Ok(w) => w,
        Err(code) => return code,
    };
    let name = match views::choose(&analysis, &Kind::System, args.name.as_deref()) {
        Ok(n) => n,
        Err(code) => return code,
    };
    views::print(
        workspace.system(&name),
        &analysis,
        args.printing.format,
        (|v| serde_json::to_string_pretty(v), text),
    )
}

fn ports(ports: &[netsmith_editor::api::Port]) -> String {
    ports
        .iter()
        .map(|p| {
            let mut s = p.name.clone();
            for part in [&p.voltage, &p.current].into_iter().flatten() {
                s.push(' ');
                s.push_str(part);
            }
            s
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn text(view: &SystemView) -> String {
    let mut out = format!("{}: {}\n", view.name, view.kind);
    out.push_str("\nMembers:\n");
    let mut rows = Vec::new();
    for node in &view.nodes {
        rows.push(vec![
            format!("    {}", node.name),
            node.kind.clone(),
            node.type_name.clone(),
            ports(&node.ports),
        ]);
    }
    out.push_str(&views::table(&rows));
    if !view.edges.is_empty() {
        out.push_str("\nJoins:\n");
        let name = |id: &str| {
            view.nodes
                .iter()
                .find(|n| n.id == id)
                .map_or_else(|| id.to_string(), |n| n.name.clone())
        };
        for edge in &view.edges {
            out.push_str(&format!(
                "    {} -> {}: {}\n",
                name(&edge.from),
                name(&edge.to),
                edge.label
            ));
        }
    }
    for mate in &view.mates {
        out.push_str(&format!("\nMate {} to {}", mate.a, mate.b));
        if !mate.ports.is_empty() {
            out.push_str(&format!(" (joins {})", mate.ports.join(", ")));
        }
        out.push('\n');
        if let Some(reversible) = &mate.reversible {
            out.push_str(&format!("    reversible: {reversible}\n"));
        }
        let mut rows = vec![
            [
                "CONTACT".to_string(),
                format!("{} ROLE", mate.a_board),
                format!("{} NET", mate.a_board),
                format!("{} ROLE", mate.b_board),
                format!("{} NET", mate.b_board),
            ]
            .to_vec(),
        ];
        for c in &mate.contacts {
            rows.push(vec![
                format!("    {}", c.number),
                c.a_role.clone(),
                c.a_net.clone().unwrap_or_else(|| "-".into()),
                c.b_role.clone(),
                c.b_net.clone().unwrap_or_else(|| "-".into()),
            ]);
        }
        out.push_str(&views::table(&rows));
    }
    if !view.ground_groups.is_empty() {
        out.push_str("\nGrounds the mates join:\n");
        for g in &view.ground_groups {
            out.push_str(&format!(
                "    {} (through {})\n",
                g.nets.join(" = "),
                g.contacts.join(", ")
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
        let Ok(Leaf::System(a)) = parse(&["board", "system", "a.nsm", "--name", "S"]) else {
            panic!()
        };
        assert_eq!(
            (a.file.as_deref(), a.name.as_deref()),
            (Some("a.nsm"), Some("S"))
        );
        assert!(parse(&["board", "system", "--board", "B"]).is_err());
    }
}
