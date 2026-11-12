fn run_editor(file: Option<&str>, port: u16) -> ExitCode {
    if !netsmith_editor::bundle_is_present() {
        eprintln!("{}", netsmith_editor::missing_bundle_message());
        return ExitCode::from(2);
    }
    let listener = match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("cannot listen on 127.0.0.1:{port}: {e} (try another with --port)");
            return ExitCode::from(2);
        }
    };
    let port = listener.local_addr().map_or(port, |a| a.port());
    let target = match Target::resolve(file) {
        Ok(t) => t,
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
    let swept = netsmith::editor::sweep_scratch();
    if swept > 0 {
        eprintln!("removed {swept} leftover scratch folder(s) of earlier comparisons");
    }
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

/// The source and the board in a browser.
///
/// With no file, `editor` acts on the project's boards (see netsmith.toml). It serves on
/// 127.0.0.1 and prints the address to open.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct EditorArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The port to listen on.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    pub port: u16,
}
