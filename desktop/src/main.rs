//! The desktop shell: the editor's page in a window of its own (issue #799).
//!
//! `netsmith-desktop [FOLDER]` starts `netsmith view editor` in the project folder (the argument, or
//! the folder it is started in) on a free port and shows the editor in a window; closing the
//! window stops the server. When the editor cannot start, the window shows why. The work that
//! needs no window is in the library (`netsmith_desktop`), with its tests.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;

use netsmith_desktop::{
    LaunchError, PROGRAM_VARIABLE, Server, editor_command, netsmith_program, problem_script,
    project_folder,
};
use tauri::{RunEvent, WebviewUrl, WebviewWindowBuilder};

fn main() {
    let started = start();
    let title = match &started {
        Ok((folder, _)) => format!(
            "{} - netsmith",
            folder.file_name().map_or_else(
                || folder.display().to_string(),
                |n| n.to_string_lossy().into_owned()
            )
        ),
        Err(_) => "netsmith".to_string(),
    };
    let (page, script, server) = match started {
        Ok((_, server)) => match server.address().parse() {
            Ok(url) => (WebviewUrl::External(url), None, Some(server)),
            Err(e) => (
                WebviewUrl::App("index.html".into()),
                Some(problem_script(&format!(
                    "the editor printed an address the window cannot open ({}): {e}",
                    server.address()
                ))),
                Some(server),
            ),
        },
        Err(e) => {
            eprintln!("netsmith-desktop: {e}");
            (
                WebviewUrl::App("index.html".into()),
                Some(problem_script(&e.to_string())),
                None,
            )
        }
    };
    let server = Mutex::new(server);
    let app = tauri::Builder::default()
        .setup(move |app| {
            let mut window = WebviewWindowBuilder::new(app, "editor", page)
                .title(title)
                .inner_size(1280.0, 800.0);
            if let Some(script) = script {
                window = window.initialization_script(script);
            }
            window.build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the window could not be made");
    app.run(move |_, event| {
        if let RunEvent::Exit = event
            && let Ok(mut server) = server.lock()
            && let Some(server) = server.take()
        {
            let _ = server.stop();
        }
    });
}

/// The project folder and its running editor.
fn start() -> Result<(PathBuf, Server), LaunchError> {
    let cwd = std::env::current_dir()
        .map_err(|e| LaunchError::Usage(format!("cannot read the current folder: {e}")))?;
    let folder = project_folder(std::env::args_os(), &cwd)?;
    let exe = std::env::current_exe().ok();
    let program = netsmith_program(
        std::env::var_os(PROGRAM_VARIABLE),
        exe.as_deref().and_then(|e| e.parent()),
    );
    let server = Server::start(editor_command(&program, &folder))?;
    Ok((folder, server))
}
