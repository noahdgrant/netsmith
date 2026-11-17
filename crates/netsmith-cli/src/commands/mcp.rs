use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args as ClapArgs;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`mcp` reads requests on stdin and answers on stdout, one JSON-RPC message per line, until stdin closes; an agent host starts it (docs/language/27-commands.md, \"The MCP server\"). Every command is a tool with the arguments of its flags, except `mcp` and `view editor`; the tool has the first line of the command's help, and the whole `--help` of the command is the resource `netsmith://help/<its words joined by -, as board-fab>`. A call runs the command in the project folder (`--project`, or the folder the server was started in), with `--format json` where the command has it, and a command's exit code 1 or 2 is a tool error. The project's files and the language guide are resources, and the skills are prompts. Progress lines go to the host when it asks for them, and a call the host cancels stops its command as Ctrl+C does."
)]
pub struct Args {
    /// The project's folder: where the commands run and the files, resources and prompts are
    /// read from (default: the folder the server was started in)
    #[arg(long, value_name = "DIR")]
    pub project: Option<PathBuf>,
}

/// Serves until stdin closes.
pub fn run(args: Args) -> ExitCode {
    match crate::mcp::serve_stdio(args.project.as_deref()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: mcp: {e}");
            ExitCode::from(2)
        }
    }
}
