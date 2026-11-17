use std::path::Path;
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_tools::init::{init, refresh};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`project init` writes `netsmith.toml`, `src/main.nsm` (a working LED board) and the agent files: `AGENTS.md`, `CLAUDE.md`, the skills and the language guide. It never overwrites a file. `project refresh`, in an existing project, brings the agent files up to this netsmith."
)]
#[command(args_override_self = true)]
pub struct Args {
    /// The project's name; the default is the folder's.
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`project refresh`, in an existing project, brings the agent files (`AGENTS.md`, the skills, the language guide) up to this netsmith: it writes the ones that are missing and rewrites the ones that still have the text netsmith wrote; a file you edited is left alone and listed with a diff (ADR 0131). It does not touch the sources."
)]
pub struct RefreshArgs {}

/// Writes netsmith.toml, a working LED board in src/main.nsm and the agent files.
pub fn run(args: Args) -> ExitCode {
    report(init(Path::new("."), args.name.as_deref()))
}

/// Updates the agent files of the project in the current folder.
pub fn run_refresh(_: RefreshArgs) -> ExitCode {
    report(refresh(Path::new(".")))
}

fn report<R: std::fmt::Display, E: std::fmt::Display>(done: Result<R, E>) -> ExitCode {
    match done {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, parse};
    use crate::commands::init;

    #[test]
    fn project_init_takes_a_name_and_project_refresh_nothing() {
        assert_eq!(
            parse(&["project", "init"]),
            Ok(Leaf::Init(init::Args { name: None }))
        );
        assert_eq!(
            parse(&["project", "init", "--name", "demo"]),
            Ok(Leaf::Init(init::Args {
                name: Some("demo".into()),
            }))
        );
        assert_eq!(
            parse(&["project", "refresh"]),
            Ok(Leaf::Refresh(init::RefreshArgs {}))
        );
        assert!(
            parse(&["project", "refresh", "--name", "demo"])
                .unwrap_err()
                .contains("--name")
        );
        assert!(parse(&["project", "init", "--refresh"]).is_err());
        assert!(
            parse(&["project", "init", "a.nsm"])
                .unwrap_err()
                .contains("a.nsm")
        );
        assert!(parse(&["project", "init", "--name"]).is_err());
    }
}
