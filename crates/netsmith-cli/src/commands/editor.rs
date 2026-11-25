use std::net::{Ipv4Addr, TcpListener};
use std::process::ExitCode;
use std::sync::Arc;

use clap::Args as ClapArgs;
use netsmith_workspace::Workspace;

use super::common::DEFAULT_PORT;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "With no file, `view editor` acts on the project's boards (see netsmith.toml). It serves on 127.0.0.1 and prints the address to open."
)]
#[command(args_override_self = true)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The port to listen on.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
    /// Ask no supplier for stock or price; say in the bill of materials that they are not checked.
    ///
    /// No provider that would use the network is asked, and an answer kept earlier is used at any
    /// age. The bill of materials and the `fab-notes.txt` the editor's Build writes say that the
    /// stock and price of the parts were not checked.
    #[arg(long)]
    pub offline: bool,
}

/// Serves the editor until it is stopped.
pub fn run(args: Args) -> ExitCode {
    run_editor(args.file.as_deref(), args.port, args.offline)
}

fn run_editor(file: Option<&str>, port: u16, offline: bool) -> ExitCode {
    // A binary built without Node serves a placeholder page that says so; the server still runs.
    if !netsmith_editor::bundle_is_present() {
        eprintln!("warning: {}", netsmith_editor::missing_bundle_message());
    }
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot listen on 127.0.0.1:{port}: {e} (try another with --port)");
            return ExitCode::from(2);
        }
    };
    let port = listener.local_addr().map_or(port, |a| a.port());
    let target = match super::common::resolve(file) {
        Ok(t) => t.with_offline(offline),
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    // Not analysed yet: the server listens first and the analysis runs behind it, so a page
    // that connects during it is told so (ADR 0019).
    let workspace = match Workspace::open_deferred(target) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    // A comparison's scratch folder is removed when it ends; one that a killed editor (or a
    // Ctrl-C that the system ended at once) left behind is removed now.
    let swept = netsmith_workspace::sweep_scratch();
    if swept > 0 {
        eprintln!("removed {swept} leftover scratch folder(s) of earlier comparisons");
    }
    // Said here, as the project is opened: the server runs until it is stopped. The page lists
    // it among its problems too (issue #1657).
    crate::release_warning::say();
    println!(
        "editing {} at http://127.0.0.1:{port}/ (Ctrl-C to stop)",
        workspace.title()
    );
    match netsmith_editor::serve(listener, Arc::new(workspace)) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("the editor stopped: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::parse;

    #[test]
    fn the_editor_takes_a_port_with_8787_as_the_default() {
        let port = |args: &[&str]| match parse(args) {
            Ok(Leaf::Editor(e)) => e.port,
            other => panic!("{args:?}: {other:?}"),
        };
        assert_eq!(port(&["view", "editor", "a.nsm"]), 8787);
        assert_eq!(port(&["view", "editor", "--port", "9000"]), 9000);
        assert!(
            parse(&["view", "editor", "--port", "http"])
                .unwrap_err()
                .contains("http")
        );
        assert!(parse(&["view", "editor", "--port", "70000"]).is_err());
        assert!(parse(&["view", "editor", "--port"]).is_err());
        assert!(parse(&["view", "editor", "--out", "x"]).is_err());
        assert!(parse(&["board", "build", "--port", "1"]).is_err());
    }
}
