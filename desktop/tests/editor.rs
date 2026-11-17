//! The launcher with the real `netsmith view editor`: the line it prints is the one the launcher
//! reads, two servers get two ports, and the address answers. It needs a `netsmith` binary, named
//! by NETSMITH_BIN (the `desktop` job of CI sets it to the `build` job's binary); without one it
//! skips and says so.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;

use netsmith_desktop::{PROGRAM_VARIABLE, Server, editor_command};
use netsmith_testkit::TempDir;

/// The status line of the answer to `GET /` at `address` (`http://127.0.0.1:<port>/`).
fn status_line(address: &str) -> String {
    let host = address
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let mut stream = TcpStream::connect(host).unwrap();
    write!(
        stream,
        "GET / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).unwrap();
    String::from_utf8_lossy(&answer)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn the_real_editor_starts_on_a_free_port_and_answers() {
    let Some(program) = std::env::var_os(PROGRAM_VARIABLE).map(PathBuf::from) else {
        eprintln!("skipped: {PROGRAM_VARIABLE} names no netsmith binary");
        return;
    };
    let dir = TempDir::new("desktop-editor");
    let init = Command::new(&program)
        .args(["project", "init", "--name", "Board"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let first = Server::start(editor_command(&program, dir.path())).unwrap();
    let second = Server::start(editor_command(&program, dir.path())).unwrap();
    assert_ne!(first.address(), second.address());
    // The real page (200) where the binary was built with Node, the placeholder (503) where not.
    for server in [&first, &second] {
        let status = status_line(server.address());
        assert!(
            status.starts_with("HTTP/1.1 200") || status.starts_with("HTTP/1.1 503"),
            "{}: {status}",
            server.address()
        );
    }
    let port = first.address().to_string();
    first.stop().unwrap();
    second.stop().unwrap();
    let host = port.trim_start_matches("http://").trim_end_matches('/');
    assert!(TcpStream::connect(host).is_err(), "{host} still answers");
}
