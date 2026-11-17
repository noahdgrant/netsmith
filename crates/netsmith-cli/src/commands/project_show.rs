//! `netsmith project show` (issue #1138): what the editor's tree is made of, printed. The
//! project's boards, systems and panels with their summaries, the modules and the boards that
//! use them, the files with their roles and definitions, the orders, whether KiCad was found and
//! where, and the version of netsmith. It is made by the code the editor's `GET /api/project`,
//! `GET /api/analysis` and `GET /api/orders` call (`netsmith_workspace::Workspace`'s `Backend`),
//! so the command line and the editor say the same; nothing here reads the project itself.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_editor::api::{Analysis, FileRole, OrderInfo, Project, Severity};
use netsmith_editor::{Backend, served_project};
use netsmith_fabricate::project::Target;
use netsmith_workspace::Workspace;
use serde_json::json;

use super::common::Format;
use super::views;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`project show` prints what the editor's project tree is made of: the project (its title, folder, version of netsmith, and whether KiCad was found and where), the boards with their definition, file, system and counts of parts and nets, the systems with their boards, the panels with the board they repeat and their grid, the modules with the boards that use them, the files with their roles (source, physical file, layout file, pin file, lock, ...) and the definitions in them, and the orders of `netsmith.toml`. It is made by the code the editor's `GET /api/project`, `GET /api/analysis` and `GET /api/orders` call, so the two are the same, and the names it prints are the ones `board bom --board`, `board system --name` and `board panel --name` take. It writes nothing and builds no board.\n\nWith no file it acts on the project; a project that lists no `top_level` file shows its files and orders. A mistake in a source is counted, not listed: `board check` lists the problems. The exit code is 0 when the project is printed and 2 when the command cannot run."
)]
pub struct Args {
    /// The source file; with none, the project.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// Text for a person, or one JSON document for a program on stdout.
    ///
    /// `json` prints `{"project": ..., "analysis": ..., "orders": [...]}`: the answers of the
    /// editor's `GET /api/project`, `GET /api/analysis` and `GET /api/orders`, each the same
    /// document, with the field names of the editor's page.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

/// Prints the project as the editor's tree shows it.
pub fn run(args: Args) -> ExitCode {
    let fail = |e: String| {
        eprintln!("{e}");
        ExitCode::from(2)
    };
    let target = match Target::resolve(args.file.as_deref()) {
        Ok(t) => t,
        Err(e) => return fail(e),
    };
    let workspace = match Workspace::open(target) {
        Ok(w) => w,
        Err(e) => return fail(e),
    };
    let project = served_project(&workspace);
    let analysis = workspace.analysis();
    let orders = workspace.orders();
    match args.format {
        Format::Json => {
            let all = json!({ "project": project, "analysis": analysis, "orders": orders });
            match serde_json::to_string_pretty(&all) {
                Ok(text) => println!("{text}"),
                Err(e) => return fail(format!("error: {e}")),
            }
        }
        Format::Text => print!("{}", text(&project, &analysis, &orders)),
    }
    ExitCode::SUCCESS
}

fn role(role: FileRole) -> &'static str {
    match role {
        FileRole::TopLevel => "top level",
        FileRole::Module => "module",
        FileRole::Dependency => "dependency",
        FileRole::Std => "std",
        FileRole::Manifest => "manifest",
        FileRole::Physical => "physical",
        FileRole::SchematicLayout => "schematic layout",
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The project as text: one section for each of the tree's lists, left out when empty.
fn text(project: &Project, analysis: &Analysis, orders: &[OrderInfo]) -> String {
    let dash = || "-".to_string();
    let mut out = format!("{}: project", project.title);
    if let Some(version) = &project.version {
        out.push_str(&format!(", netsmith {version}"));
    }
    out.push('\n');
    if let Some(root) = &project.root {
        out.push_str(&format!("Folder: {root}\n"));
    }
    out.push_str(&format!(
        "Manifest: {}\nKiCad: {}\n",
        if project.has_manifest {
            "netsmith.toml"
        } else {
            "none (a single file)"
        },
        match (&project.kicad_path, project.kicad) {
            (Some(path), _) => format!("found, {path}"),
            (None, true) => "found".to_string(),
            (None, false) => "not found (nothing needs it)".to_string(),
        }
    ));

    let problems = |severity| {
        analysis
            .problems
            .iter()
            .filter(|p| p.severity == severity)
            .count()
    };
    out.push_str(&format!(
        "Analysis: {}, {}, {} (`board check` lists them)\n",
        plural(problems(Severity::Error), "error", "errors"),
        plural(problems(Severity::Warning), "warning", "warnings"),
        plural(problems(Severity::Note), "note", "notes"),
    ));

    if !analysis.boards.is_empty() {
        let mut rows = vec![
            ["NAME", "DEFINITION", "FILE", "SYSTEM", "PARTS", "NETS", ""]
                .map(String::from)
                .to_vec(),
        ];
        for b in &analysis.boards {
            rows.push(vec![
                b.name.clone(),
                b.definition.clone(),
                b.file.clone(),
                b.system.clone().unwrap_or_else(dash),
                b.parts.to_string(),
                b.nets.to_string(),
                if b.stale {
                    "the source has a mistake: the last board that built".to_string()
                } else {
                    String::new()
                },
            ]);
        }
        out.push_str(&format!("\nBoards ({}):\n", analysis.boards.len()));
        out.push_str(&views::table(&rows));
    }
    if !analysis.systems.is_empty() {
        let mut rows = vec![["NAME", "FILE", "BOARDS"].map(String::from).to_vec()];
        for s in &analysis.systems {
            rows.push(vec![s.name.clone(), s.file.clone(), s.boards.join(", ")]);
        }
        out.push_str(&format!("\nSystems ({}):\n", analysis.systems.len()));
        out.push_str(&views::table(&rows));
    }
    if !analysis.panels.is_empty() {
        let mut rows = vec![["NAME", "FILE", "BOARD", "GRID"].map(String::from).to_vec()];
        for p in &analysis.panels {
            rows.push(vec![
                p.name.clone(),
                p.file.clone(),
                p.board.clone(),
                format!("{} x {}", p.columns, p.rows),
            ]);
        }
        out.push_str(&format!("\nPanels ({}):\n", analysis.panels.len()));
        out.push_str(&views::table(&rows));
    }
    if let Some(uses) = analysis.module_uses.as_ref().filter(|u| !u.is_empty()) {
        let mut rows = vec![
            ["MODULE", "FILE", "LINE", "USED BY"]
                .map(String::from)
                .to_vec(),
        ];
        for u in uses {
            rows.push(vec![
                u.name.clone(),
                u.file.clone(),
                u.line.to_string(),
                u.used_by.join(", "),
            ]);
        }
        out.push_str(&format!("\nModules in use ({}):\n", uses.len()));
        out.push_str(&views::table(&rows));
    }

    // The standard library is the same in every project: counted, and in the JSON.
    let (std, files): (Vec<_>, Vec<_>) =
        project.files.iter().partition(|f| f.role == FileRole::Std);
    let mut rows = vec![["FILE", "ROLE", "DEFINITIONS"].map(String::from).to_vec()];
    for f in &files {
        let defs = f
            .defs
            .iter()
            .map(|d| format!("{} {}", d.kind, d.name))
            .collect::<Vec<_>>()
            .join(", ");
        rows.push(vec![
            f.path.clone(),
            format!(
                "{}{}",
                role(f.role),
                if f.readonly { ", read-only" } else { "" }
            ),
            defs,
        ]);
    }
    out.push_str(&format!("\nFiles ({}):\n", files.len()));
    out.push_str(&views::table(&rows));
    if !std.is_empty() {
        out.push_str(&format!(
            "and {} of the standard library (`--format json` lists them)\n",
            plural(std.len(), "file", "files")
        ));
    }
    if !project.folders.is_empty() {
        out.push_str(&format!("Folders: {}\n", project.folders.join(", ")));
    }

    if orders.is_empty() {
        out.push_str("\nOrders: none (`[[order]]` in netsmith.toml names one)\n");
    } else {
        out.push_str(&format!("\nOrders ({}):\n", orders.len()));
        for o in orders {
            let boards = o
                .boards
                .iter()
                .map(|b| format!("{} x{}", b.name, b.quantity))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("{}: {boards}\n", o.name));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn it_takes_a_file_and_a_format() {
        let Ok(Leaf::ProjectShow(a)) = parse(&["project", "show", "a.nsm", "--format", "json"])
        else {
            panic!()
        };
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.format, crate::commands::common::Format::Json);
        assert!(parse(&["project", "show", "--board", "B"]).is_err());
    }
}
