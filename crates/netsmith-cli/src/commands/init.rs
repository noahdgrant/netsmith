use std::path::Path;
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_tools::init::{init, refresh};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`init` writes `netsmith.toml`, `src/main.nsm` (a working LED board) and the agent files: `AGENTS.md`, `CLAUDE.md`, the skills and the language guide. It never overwrites a file. `init --refresh`, in an existing project, brings the agent files up to this netsmith: it writes the ones that are missing and rewrites the ones that still have the text netsmith wrote; a file you edited is left alone and listed with a diff."
)]
#[command(args_override_self = true)]
pub struct Args {
    /// The project's name; the default is the folder's.
    #[arg(long)]
    pub name: Option<String>,
    /// Bring the project's agent files (AGENTS.md, the skills, the language guide) up to this
    /// netsmith instead of making a project: write the missing ones, rewrite the ones netsmith
    /// wrote and you have not edited, and list the others with a diff.
    #[arg(long, conflicts_with = "name")]
    pub refresh: bool,
}

/// Writes netsmith.toml, a working LED board in src/main.nsm and the agent files; with
/// `--refresh` updates the agent files of the project instead.
pub fn run(args: Args) -> ExitCode {
    let done = if args.refresh {
        refresh(Path::new("."))
    } else {
        init(Path::new("."), args.name.as_deref())
    };
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
    use crate::commands::common::testing::parse;
    use crate::commands::{Command, init};

    #[test]
    fn init_takes_a_name_or_refreshes() {
        assert_eq!(
            parse(&["init"]),
            Ok(Command::Init(init::Args {
                name: None,
                refresh: false
            }))
        );
        assert_eq!(
            parse(&["init", "--name", "demo"]),
            Ok(Command::Init(init::Args {
                name: Some("demo".into()),
                refresh: false
            }))
        );
        assert_eq!(
            parse(&["init", "--refresh"]),
            Ok(Command::Init(init::Args {
                name: None,
                refresh: true
            }))
        );
        assert!(
            parse(&["init", "--refresh", "--name", "demo"])
                .unwrap_err()
                .contains("--name")
        );
        assert!(parse(&["init", "a.nsm"]).unwrap_err().contains("a.nsm"));
        assert!(parse(&["init", "--name"]).is_err());
    }
}
