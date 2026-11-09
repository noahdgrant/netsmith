//! `netsmith --help` lists the commands on stdout and exits 0; an unknown command exits 2 with
//! the error on stderr and a pointer to `--help`; `--version` still works.

use std::process::Command;

fn netsmith(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn help_lists_the_commands_on_stdout_and_exits_zero() {
    for args in [&["--help"][..], &["-h"], &["help"], &["fab", "--help"]] {
        let out = netsmith(args);
        assert!(out.status.success(), "{args:?}: {:?}", out.status);
        assert!(out.stderr.is_empty(), "{args:?}");
        let text = String::from_utf8_lossy(&out.stdout);
        for (name, _) in netsmith::cli::COMMANDS {
            assert!(text.contains(name), "{args:?}: no `{name}` in {text}");
        }
        assert!(text.contains("--cross-check"), "{args:?}: {text}");
    }
}

#[test]
fn an_unknown_command_exits_two_with_the_error_on_stderr() {
    let out = netsmith(&["frobnicate"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("unknown command `frobnicate`"), "{text}");
    assert!(text.contains("netsmith --help"), "{text}");
}

#[test]
fn the_version_still_prints() {
    let out = netsmith(&["--version"]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("netsmith {}\n", env!("CARGO_PKG_VERSION"))
    );
}
