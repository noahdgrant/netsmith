use std::path::Path;
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith::init::init;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`init` writes `netsmith.toml`, `src/main.nsm` (a working LED board) and the agent files."
)]
#[command(args_override_self = true)]
pub struct Args {
    /// The project's name; the default is the folder's.
    #[arg(long)]
    pub name: Option<String>,
}

/// Writes netsmith.toml, a working LED board in src/main.nsm and the agent files.
pub fn run(args: Args) -> ExitCode {
    run_init(args.name.as_deref())
}

fn run_init(name: Option<&str>) -> ExitCode {
    match init(Path::new("."), name) {
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
    fn init_takes_only_a_name() {
        assert_eq!(
            parse(&["init"]),
            Ok(Command::Init(init::Args { name: None }))
        );
        assert_eq!(
            parse(&["init", "--name", "demo"]),
            Ok(Command::Init(init::Args {
                name: Some("demo".into())
            }))
        );
        assert!(parse(&["init", "a.nsm"]).unwrap_err().contains("a.nsm"));
        assert!(parse(&["init", "--name"]).is_err());
    }
}
