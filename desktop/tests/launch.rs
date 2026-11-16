//! The launcher without a window: the project folder, the program, the editor's address line,
//! and a stand-in server (a shell script) started, waited for and stopped.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use netsmith_desktop::{
    LaunchError, Server, editor_command, netsmith_program, problem_script, project_folder,
    ready_address,
};
use netsmith_testkit::TempDir;

fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

#[test]
fn the_project_folder_is_the_argument_or_the_current_folder() {
    let dir = TempDir::new("desktop-folder");
    let project = dir.path().join("board");
    fs::create_dir(&project).unwrap();
    let canonical = project.canonicalize().unwrap();
    assert_eq!(
        project_folder(args(&["netsmith-desktop"]), &project).unwrap(),
        canonical
    );
    assert_eq!(
        project_folder(args(&["netsmith-desktop", "board"]), dir.path()).unwrap(),
        canonical
    );
    let absolute = project.to_str().unwrap();
    assert_eq!(
        project_folder(args(&["netsmith-desktop", absolute]), Path::new("/")).unwrap(),
        canonical
    );
}

#[test]
fn a_missing_folder_or_a_second_argument_is_refused() {
    let dir = TempDir::new("desktop-folder-refused");
    let missing = project_folder(args(&["netsmith-desktop", "nowhere"]), dir.path())
        .unwrap_err()
        .to_string();
    assert!(missing.contains("nowhere") && missing.contains("not a folder"), "{missing}");
    fs::write(dir.path().join("a.nsm"), "").unwrap();
    let file = project_folder(args(&["netsmith-desktop", "a.nsm"]), dir.path()).unwrap_err();
    assert!(matches!(file, LaunchError::Usage(_)), "{file}");
    let two = project_folder(args(&["netsmith-desktop", ".", "other"]), dir.path())
        .unwrap_err()
        .to_string();
    assert!(two.contains("other") && two.contains("usage"), "{two}");
}

#[test]
fn the_program_is_the_variables_then_the_one_beside_the_shell_then_the_paths() {
    let dir = TempDir::new("desktop-program");
    let name = format!("netsmith{}", std::env::consts::EXE_SUFFIX);
    assert_eq!(
        netsmith_program(Some("/opt/x/netsmith".into()), Some(dir.path())),
        PathBuf::from("/opt/x/netsmith")
    );
    // Nothing beside the shell: the name alone, for the system to find on PATH.
    assert_eq!(netsmith_program(None, Some(dir.path())), PathBuf::from(&name));
    assert_eq!(netsmith_program(Some("".into()), None), PathBuf::from(&name));
    fs::write(dir.path().join(&name), "").unwrap();
    assert_eq!(
        netsmith_program(None, Some(dir.path())),
        dir.path().join(&name)
    );
    assert_eq!(
        netsmith_program(Some("/opt/x/netsmith".into()), Some(dir.path())),
        PathBuf::from("/opt/x/netsmith")
    );
}

#[test]
fn the_editor_is_asked_for_a_free_port_in_the_project_folder() {
    let command = editor_command(Path::new("/bin/netsmith"), Path::new("/work/board"));
    assert_eq!(command.get_program(), "/bin/netsmith");
    let given: Vec<_> = command.get_args().collect();
    assert_eq!(given, ["editor", "--port", "0"]);
    assert_eq!(command.get_current_dir(), Some(Path::new("/work/board")));
}

#[test]
fn the_address_is_read_from_the_line_the_editor_prints() {
    assert_eq!(
        ready_address("editing Led at http://127.0.0.1:40123/ (Ctrl-C to stop)"),
        Some("http://127.0.0.1:40123/")
    );
    // A title may hold " at ": the address is the last one.
    assert_eq!(
        ready_address("editing Look at me at http://127.0.0.1:8787/ (Ctrl-C to stop)"),
        Some("http://127.0.0.1:8787/")
    );
    for other in [
        "removed 2 leftover scratch folder(s) of earlier comparisons",
        "editing Led at http://example.com:80/ (Ctrl-C to stop)",
        "editing Led at http://127.0.0.1:port/ (Ctrl-C to stop)",
        "editing Led at http://127.0.0.1:99999/ (Ctrl-C to stop)",
        "",
    ] {
        assert_eq!(ready_address(other), None, "{other}");
    }
}

#[test]
fn a_problem_is_quoted_for_the_shells_page() {
    assert_eq!(
        problem_script("no \"netsmith.toml\"\nin C:\\x </script>"),
        "window.__netsmithProblem = \"no \\\"netsmith.toml\\\"\\nin C:\\\\x \\u003c/script>\";"
    );
}

/// A stand-in for `netsmith`: a shell script with `body` as its text, in `dir`.
#[cfg(unix)]
fn stand_in(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("netsmith");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn a_server_is_ready_at_the_address_it_prints_and_ends_when_stopped() {
    let dir = TempDir::new("desktop-server");
    // It is run as `netsmith editor --port 0` in the folder, says something first, and then
    // serves until it is stopped (`sleep` stands for the serving).
    let program = stand_in(
        dir.path(),
        r#"[ "$*" = "editor --port 0" ] || { echo "wrong arguments: $*" >&2; exit 2; }
[ "$(pwd)" = "$EXPECTED_DIR" ] || { echo "wrong folder: $(pwd)" >&2; exit 2; }
echo "removed 1 leftover scratch folder(s) of earlier comparisons"
echo "editing Board at http://127.0.0.1:40123/ (Ctrl-C to stop)"
exec sleep 600"#,
    );
    let folder = dir.path().canonicalize().unwrap();
    let mut command = editor_command(&program, &folder);
    command.env("EXPECTED_DIR", &folder);
    let server = Server::start(command).unwrap();
    assert_eq!(server.address(), "http://127.0.0.1:40123/");
    let proc = PathBuf::from(format!("/proc/{}", server.id()));
    let status = server.stop().unwrap();
    // Ended by the stop (a signal), not by itself, and reaped: its process is gone.
    assert!(!status.success() && status.code().is_none(), "{status}");
    if Path::new("/proc/self").exists() {
        assert!(!proc.exists(), "{} is still there", proc.display());
    }
}

#[cfg(unix)]
#[test]
fn a_dropped_server_is_stopped() {
    let dir = TempDir::new("desktop-server-drop");
    let program = stand_in(
        dir.path(),
        "echo 'editing Board at http://127.0.0.1:40124/ (Ctrl-C to stop)'\nexec sleep 600",
    );
    let server = Server::start(editor_command(&program, dir.path())).unwrap();
    let proc = PathBuf::from(format!("/proc/{}", server.id()));
    drop(server);
    if Path::new("/proc/self").exists() {
        assert!(!proc.exists(), "{} is still there", proc.display());
    }
}

#[cfg(unix)]
#[test]
fn a_server_that_stops_before_it_is_ready_says_why() {
    let dir = TempDir::new("desktop-server-fails");
    let program = stand_in(
        dir.path(),
        "echo 'no netsmith.toml in this folder or above it' >&2\nexit 2",
    );
    let e = Server::start(editor_command(&program, dir.path())).unwrap_err();
    let LaunchError::Stopped { status, said, .. } = &e else {
        panic!("{e:?}");
    };
    assert_eq!(status.and_then(|s| s.code()), Some(2));
    assert_eq!(said, "no netsmith.toml in this folder or above it");
    let message = e.to_string();
    assert!(
        message.contains("stopped before it was ready")
            && message.contains("no netsmith.toml in this folder or above it"),
        "{message}"
    );
}

#[test]
fn a_program_that_is_not_there_names_the_variable() {
    let dir = TempDir::new("desktop-no-program");
    let missing = dir.path().join("no-such-netsmith");
    let e = Server::start(Command::new(&missing)).unwrap_err().to_string();
    assert!(
        e.contains("no-such-netsmith") && e.contains("NETSMITH_BIN"),
        "{e}"
    );
}
