use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args as ClapArgs;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`mcp` reads requests on stdin and answers on stdout, one JSON-RPC message per line, until stdin closes; an agent host starts it (docs/language/27-commands.md, \"The MCP server\"). Every command is a tool with the arguments of its flags, except `mcp` and `view editor`; the tool has the first line of the command's help, and the whole `--help` of the command is the resource `netsmith://help/<its words joined by -, as board-fab>`. A call runs the command in the project folder (`--project`, or the folder the server was started in), with `--format json` where the command has it, and a command's exit code 1 or 2 is a tool error. With `--editor [PORT]` the tool `view_editor_3d` acts on the page of a running editor (`netsmith view editor`), the port given or the editor found for the project, and returns its picture as an image. The project's files and the language guide are resources, and the skills are prompts. Progress lines go to the host when it asks for them, and a call the host cancels stops its command as Ctrl+C does."
)]
pub struct Args {
    /// The project's folder: where the commands run and the files, resources and prompts are
    /// read from (default: the folder the server was started in)
    #[arg(long, value_name = "DIR")]
    pub project: Option<PathBuf>,
    /// Attach to a running editor: the tools that read or change its page act on it.
    ///
    /// With a port, the editor on 127.0.0.1:PORT; with none, the editor that is running for the
    /// project, found among the ports from 8787 to 8796 when a call needs it. The tool
    /// `view_editor_3d` then reads and changes the page the person has open (its state, the
    /// selection, the highlighted parts and nets, the camera and the cut) and returns its picture;
    /// it needs no `port`. The editor is looked for at each call, so it may start after the server.
    #[arg(long, value_name = "PORT", num_args = 0..=1, value_parser = clap::value_parser!(u16).range(1..))]
    pub editor: Option<Option<u16>>,
}

/// Serves until stdin closes.
pub fn run(args: Args) -> ExitCode {
    match crate::mcp::serve_stdio(
        args.project.as_deref(),
        args.editor.map(|port| match port {
            Some(port) => crate::mcp::Attach::Port(port),
            None => crate::mcp::Attach::Find,
        }),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: mcp: {e}");
            ExitCode::from(2)
        }
    }
}
