//! `netsmith --help` lists the commands on stdout and exits 0, and each command's `--help` lists
//! every flag of clap's model of it (#514); a mistake exits 2 with the error on stderr;
//! `--version` still works.

use std::process::Command;

use clap::Subcommand;

/// clap's model of the commands, as main.rs puts them under `netsmith`.
pub fn model() -> clap::Command {
    netsmith_cli::commands::Command::augment_subcommands(clap::Command::new("netsmith"))
}

fn netsmith(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .output()
        .unwrap()
}

/// Every command of the model as the words that name it (`["catalog", "refresh"]`), with its
/// model, nested commands included.
fn commands() -> Vec<(Vec<String>, clap::Command)> {
    fn walk(path: &[String], command: &clap::Command, all: &mut Vec<(Vec<String>, clap::Command)>) {
        for sub in command.get_subcommands().filter(|s| s.get_name() != "help") {
            let mut path = path.to_vec();
            path.push(sub.get_name().to_string());
            all.push((path.clone(), sub.clone()));
            walk(&path, sub, all);
        }
    }
    let mut all = Vec::new();
    walk(&[], &model(), &mut all);
    all
}

#[test]
fn help_lists_every_command_on_stdout_and_exits_zero() {
    for args in [&["--help"][..], &["-h"], &["help"]] {
        let out = netsmith(args);
        assert!(out.status.success(), "{args:?}: {:?}", out.status);
        assert!(out.stderr.is_empty(), "{args:?}");
        let text = String::from_utf8_lossy(&out.stdout);
        for sub in model().get_subcommands() {
            let name = sub.get_name();
            assert!(
                text.contains(&format!("  {name} ")),
                "{args:?}: no `{name}` in {text}"
            );
        }
    }
}

#[test]
fn a_commands_help_lists_its_usage_and_every_flag_it_takes() {
    let all = commands();
    for name in [
        "build",
        "fab",
        "route",
        "catalog refresh",
        "dxf import",
        "kicad-libs import",
    ] {
        assert!(all.iter().any(|(path, _)| path.join(" ") == name), "{name}");
    }
    for (path, model) in all {
        let mut args: Vec<&str> = path.iter().map(String::as_str).collect();
        args.push("--help");
        let out = netsmith(&args);
        assert!(out.status.success(), "{args:?}");
        let text = String::from_utf8_lossy(&out.stdout);
        let name = path.join(" ");
        assert!(
            text.contains(&format!("netsmith {name}")),
            "{args:?}: no usage in {text}"
        );
        for arg in model.get_arguments().filter(|a| !a.is_hide_set()) {
            if let Some(long) = arg.get_long() {
                assert!(
                    text.contains(&format!("--{long}")),
                    "{name}: no --{long} in {text}"
                );
            }
            for value in arg
                .get_possible_values()
                .iter()
                .filter(|v| arg.get_action().takes_values() && !v.is_hide_set())
            {
                assert!(
                    text.contains(value.get_name()),
                    "{name} --{:?}: no {}",
                    arg.get_long(),
                    value.get_name()
                );
            }
        }
    }
    let fab = String::from_utf8_lossy(&netsmith(&["fab", "--help"]).stdout).to_string();
    for word in [
        "--cross-check",
        "--pins <PINS>",
        "--save-placed",
        "INCOMPLETE",
        "routing-failure.txt",
    ] {
        assert!(fab.contains(word), "{word}: {fab}");
    }
    let build = String::from_utf8_lossy(&netsmith(&["build", "--help"]).stdout).to_string();
    for word in [
        "note.not_stated",
        "summary",
        "[default: all]",
        "[default: text]",
    ] {
        assert!(build.contains(word), "{word}: {build}");
    }
}

#[test]
fn a_mistake_exits_two_with_the_error_on_stderr() {
    for args in [
        &["frobnicate"][..],
        &[],
        &["build", "--bogus"],
        &["build", "a.nsm", "b.nsm"],
        &["lock"],
        &["lock", "--update", "--check"],
        &["render", "--ortho", "--perspective"],
        &["view", "--theme", "dark"],
    ] {
        let out = netsmith(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        assert!(!out.stderr.is_empty(), "{args:?}");
    }
    let text = String::from_utf8_lossy(&netsmith(&["frobnicate"]).stderr).to_string();
    assert!(
        text.contains("frobnicate") && text.contains("--help"),
        "{text}"
    );
}

#[test]
fn the_version_still_prints() {
    for args in [&["--version"][..], &["-V"]] {
        let out = netsmith(args);
        assert!(out.status.success(), "{args:?}");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            format!("netsmith {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
}
