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

/// Scaffold a project in the current directory.
///
/// `init` writes `netsmith.toml`, `src/main.nsm` (a working LED board) and the agent files.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct InitArgs {
    /// The project's name; the default is the directory's.
    #[arg(long)]
    pub name: Option<String>,
}
