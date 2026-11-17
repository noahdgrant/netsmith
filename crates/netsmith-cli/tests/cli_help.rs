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
        "board build",
        "board fab",
        "dev route",
        "catalog refresh",
        "import dxf",
        "import kicad-libs libraries",
        "import kicad-libs models",
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
    let fab = String::from_utf8_lossy(&netsmith(&["board", "fab", "--help"]).stdout).to_string();
    for word in [
        "--cross-check",
        "--pins <PINS>",
        "--save-placed",
        "--reuse-routes",
        "INCOMPLETE",
        "routing-failure.txt",
    ] {
        assert!(fab.contains(word), "{word}: {fab}");
    }
    let build = String::from_utf8_lossy(&netsmith(&["board", "build", "--help"]).stdout).to_string();
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
        &["board", "build", "--bogus"],
        &["board", "build", "a.nsm", "b.nsm"],
        &["project", "lock"],
        &["project", "lock", "--update", "--check"],
        &["view", "3d", "--ortho", "--perspective"],
        &["view", "editor-3d", "--theme", "dark"],
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
fn a_command_of_an_earlier_release_names_the_one_to_write_and_exits_two() {
    for (args, says) in [
        (&["build", "a.nsm"][..], "write: netsmith board build a.nsm"),
        (&["fab", "--order", "kit"], "write: netsmith project order kit"),
        (&["init", "--refresh"], "write: netsmith project refresh"),
        (&["render", "--out", "a.png"], "write: netsmith view 3d --picture a.png"),
        (&["editor", "--port", "0"], "write: netsmith view editor --port 0"),
        (&["view", "--board", "X"], "write: netsmith view editor-3d --board X"),
        (&["dxf", "import", "a.dxf"], "write: netsmith import dxf a.dxf"),
        (&["layout", "show", "--into", "a.nsm"], "write: netsmith layout show --file a.nsm"),
    ] {
        let out = netsmith(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty(), "{args:?}");
        let text = String::from_utf8_lossy(&out.stderr);
        assert!(
            text.contains(says) && text.contains("is now") && text.contains("docs/MIGRATING.md"),
            "{args:?}: {text}"
        );
    }
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
