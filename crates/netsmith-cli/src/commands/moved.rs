//! The commands and flags of an earlier release that moved when the command line was grouped
//! under nouns (issue #1090). Each is an error that names the spelling to write, with the whole
//! line rewritten, and none is a second spelling that still runs (ADR 0001). `docs/MIGRATING.md`
//! and the guide's commands chapter have the same table.

/// A command line of an earlier release, in today's spelling.
#[derive(Debug, PartialEq, Eq)]
pub struct Moved {
    /// Each change, as the old spelling and the new one (`fab`, `board fab`).
    pub changes: Vec<(String, String)>,
    /// The whole line as it is written now, without the program's name.
    pub line: Vec<String>,
}

impl Moved {
    /// The error `netsmith` prints for it.
    pub fn message(&self) -> String {
        let mut text = String::new();
        for (old, new) in &self.changes {
            text.push_str(&format!("error: `{old}` is now `{new}`\n"));
        }
        text.push_str(&format!(
            "  write: netsmith {}\n  (the command line is grouped under nouns now; docs/MIGRATING.md lists every move)\n",
            self.line.join(" ")
        ));
        text
    }
}

/// The commands of the top level that moved, and where to: the new words, in order.
const COMMANDS: &[(&str, &[&str])] = &[
    ("init", &["project", "init"]),
    ("lock", &["project", "lock"]),
    ("check", &["board", "check"]),
    ("build", &["board", "build"]),
    ("fab", &["board", "fab"]),
    ("explain", &["board", "explain"]),
    ("compare", &["board", "compare"]),
    ("route", &["dev", "route"]),
    ("parity", &["dev", "parity"]),
    ("board3d", &["dev", "board3d"]),
    ("render", &["view", "3d"]),
    ("editor", &["view", "editor"]),
    ("part", &["import", "part"]),
    ("port", &["import", "port"]),
    ("kicad-libs", &["import", "kicad-libs"]),
    ("dxf", &["import", "dxf"]),
    ("case", &["import", "case"]),
];

/// The verbs of a moved group that were renamed too: the group's new words, the old verb and
/// what it is now (`dxf import` is `import dxf`, with no verb).
const VERBS: &[(&[&str], &str, &[&str])] = &[
    (&["import", "dxf"], "import", &[]),
    (&["import", "kicad-libs"], "import", &["libraries"]),
    (&["import", "case"], "import", &["model"]),
];

/// The flags that were renamed: the command (its new words), the old flag and the new one.
/// A flag with no new name takes a value that moves with it (`--json` is `--format json`).
const FLAGS: &[(&[&str], &str, &[&str])] = &[
    (&["layout"], "--into", &["--file"]),
    (&["routes"], "--into", &["--file"]),
    (&["layout", "export"], "--out", &["--kicad-pcb"]),
    (&["import", "port", "skeleton"], "--out", &["--write"]),
    (&["view", "3d"], "--out", &["--picture"]),
    (&["dev", "board3d"], "--json", &["--format", "json"]),
    (&["view", "editor-3d"], "--json", &["--format", "json"]),
];

/// `args` (without the program's name) in today's spelling, when a command or a flag of it
/// moved; `None` when nothing did, and clap reads the line as it is.
pub fn moved(args: &[String]) -> Option<Moved> {
    let mut changes = Vec::new();
    let line = rewrite(args, &mut changes);
    (!changes.is_empty()).then_some(Moved { changes, line })
}

fn rewrite(args: &[String], changes: &mut Vec<(String, String)>) -> Vec<String> {
    let Some(first) = args.first() else {
        return Vec::new();
    };
    // `help fab` is `help board fab`.
    if first == "help" {
        let mut line = vec![first.clone()];
        line.extend(rewrite(&args[1..], changes));
        return line;
    }
    let rest = &args[1..];
    let mut words: Vec<String>;
    let mut tail: Vec<String> = rest.to_vec();
    match first.as_str() {
        "init" if tail.iter().any(|a| a == "--refresh") => {
            tail.retain(|a| a != "--refresh");
            changes.push(("init --refresh".into(), "project refresh".into()));
            words = vec!["project".into(), "refresh".into()];
        }
        "fab"
            if tail
                .iter()
                .any(|a| a == "--order" || a.starts_with("--order=")) =>
        {
            let at = tail
                .iter()
                .position(|a| a == "--order" || a.starts_with("--order="))
                .expect("found above");
            let flag = tail.remove(at);
            let name = match flag.strip_prefix("--order=") {
                Some(name) => Some(name.to_string()),
                None if at < tail.len() && !tail[at].starts_with('-') => Some(tail.remove(at)),
                None => None,
            };
            changes.push(("fab --order".into(), "project order".into()));
            words = vec!["project".into(), "order".into()];
            words.extend(name);
        }
        // `view` with a flag was the view of a running editor; `view` alone is the group's help.
        "view"
            if tail
                .first()
                .is_some_and(|a| a.starts_with('-') && a != "-h" && a != "--help") =>
        {
            changes.push(("view".into(), "view editor-3d".into()));
            words = vec!["view".into(), "editor-3d".into()];
        }
        name => match COMMANDS.iter().find(|(old, _)| *old == name) {
            Some((old, new)) => {
                words = new.iter().map(|w| w.to_string()).collect();
                changes.push((old.to_string(), new.join(" ")));
                if let Some((group, verb, now)) = VERBS.iter().find(|(group, verb, _)| {
                    **group == **new && tail.first().is_some_and(|a| a == verb)
                }) {
                    tail.remove(0);
                    let mut was = vec![*old, *verb];
                    was.retain(|w| !w.is_empty());
                    let mut is: Vec<&str> = group.to_vec();
                    is.extend(now.iter());
                    changes.pop();
                    changes.push((was.join(" "), is.join(" ")));
                    words.extend(now.iter().map(|w| w.to_string()));
                }
            }
            None => words = vec![first.clone()],
        },
    }
    // The verbs after the group, as far as a renamed flag's command goes (`layout export`).
    while let Some(next) = tail.first() {
        let reach = words.len() + 1;
        let leads = FLAGS.iter().any(|(command, _, _)| {
            command.len() >= reach
                && command[..reach - 1].iter().eq(words.iter())
                && command[reach - 1] == next
        });
        if !leads {
            break;
        }
        words.push(tail.remove(0));
    }
    let mut out = Vec::new();
    for arg in tail {
        let (flag, value) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag.to_string(), Some(value)),
            _ => (arg.clone(), None),
        };
        let renamed = FLAGS.iter().find(|(command, old, _)| {
            *old == flag
                && words
                    .iter()
                    .map(String::as_str)
                    .take(command.len())
                    .eq(command.iter().copied())
        });
        match renamed {
            Some((_, old, new)) => {
                changes.push((old.to_string(), new.join(" ")));
                match value {
                    Some(value) => {
                        out.extend(new[..new.len() - 1].iter().map(|w| w.to_string()));
                        out.push(format!("{}={value}", new[new.len() - 1]));
                    }
                    None => out.extend(new.iter().map(|w| w.to_string())),
                }
            }
            None => out.push(arg),
        }
    }
    words.extend(out);
    words
}

#[cfg(test)]
mod tests {
    use super::moved;

    fn line(args: &str) -> Option<(Vec<(String, String)>, String)> {
        let args: Vec<String> = args.split_whitespace().map(String::from).collect();
        moved(&args).map(|m| (m.changes, m.line.join(" ")))
    }

    fn now(args: &str) -> String {
        line(args).unwrap_or_else(|| panic!("{args}: not moved")).1
    }

    #[test]
    fn each_old_command_is_the_new_one_with_its_arguments() {
        for (old, new) in [
            ("init --name demo", "project init --name demo"),
            ("init --refresh", "project refresh"),
            ("lock --update a.nsm", "project lock --update a.nsm"),
            ("check a.nsm --strict", "board check a.nsm --strict"),
            ("build a.nsm --out out", "board build a.nsm --out out"),
            ("fab a.nsm --out out", "board fab a.nsm --out out"),
            (
                "fab --order devkit --offline",
                "project order devkit --offline",
            ),
            ("fab --order", "project order"),
            ("fab --order --out x", "project order --out x"),
            ("fab --order=devkit", "project order devkit"),
            ("explain r1 a.nsm", "board explain r1 a.nsm"),
            ("compare a b", "board compare a b"),
            (
                "route b.placed.json --turn 3",
                "dev route b.placed.json --turn 3",
            ),
            ("parity a b", "dev parity a b"),
            ("board3d a.nsm --json", "dev board3d a.nsm --format json"),
            ("render a.nsm --out a.png", "view 3d a.nsm --picture a.png"),
            ("render --out=a.png", "view 3d --picture=a.png"),
            ("editor --port 9000", "view editor --port 9000"),
            (
                "view --board X --json",
                "view editor-3d --board X --format json",
            ),
            (
                "part from-symbol a.kicad_sym U",
                "import part from-symbol a.kicad_sym U",
            ),
            (
                "port skeleton a.kicad_pcb --out src/b.nsm",
                "import port skeleton a.kicad_pcb --write src/b.nsm",
            ),
            (
                "port tables a.kicad_pcb --check",
                "import port tables a.kicad_pcb --check",
            ),
            ("kicad-libs import s f", "import kicad-libs libraries s f"),
            ("kicad-libs models f", "import kicad-libs models f"),
            (
                "dxf import case.dxf --write a.nsm",
                "import dxf case.dxf --write a.nsm",
            ),
            (
                "case import c.step --at -1",
                "import case model c.step --at -1",
            ),
            ("case place 1 0 0 0", "import case place 1 0 0 0"),
            ("help fab", "help board fab"),
        ] {
            assert_eq!(now(old), new, "{old}");
        }
    }

    #[test]
    fn a_renamed_flag_of_a_command_that_stayed_is_moved_too() {
        for (old, new) in [
            ("layout show --into a.nsm", "layout show --file a.nsm"),
            (
                "routes add-via --into=a.nsm --net GND",
                "routes add-via --file=a.nsm --net GND",
            ),
            (
                "layout export --out b.kicad_pcb",
                "layout export --kicad-pcb b.kicad_pcb",
            ),
            (
                "import port skeleton a --out b.nsm",
                "import port skeleton a --write b.nsm",
            ),
            ("view 3d --out a.png", "view 3d --picture a.png"),
            ("dev board3d --json", "dev board3d --format json"),
            ("view editor-3d --json", "view editor-3d --format json"),
        ] {
            assert_eq!(now(old), new, "{old}");
        }
    }

    #[test]
    fn the_changes_name_each_old_spelling_and_its_new_one() {
        let (changes, _) = line("dxf import a.dxf").unwrap();
        assert_eq!(changes, [("dxf import".into(), "import dxf".into())]);
        let (changes, _) = line("render --out a.png").unwrap();
        assert_eq!(
            changes,
            [
                ("render".into(), "view 3d".into()),
                ("--out".into(), "--picture".into())
            ]
        );
        let message = super::moved(&["fab".into(), "a.nsm".into()])
            .unwrap()
            .message();
        assert!(
            message.contains("`fab` is now `board fab`")
                && message.contains("write: netsmith board fab a.nsm")
                && message.contains("docs/MIGRATING.md"),
            "{message}"
        );
    }

    #[test]
    fn a_line_in_todays_spelling_is_not_moved() {
        for args in [
            "",
            "--help",
            "view",
            "view --help",
            "view 3d --picture a.png",
            "board fab --out out",
            "layout export --kicad-pcb a.kicad_pcb",
            "layout show --file a.nsm",
            "board build --out out",
            "dev route b.placed.json --out o",
            "catalog refresh",
            "frobnicate",
            "help board",
        ] {
            assert_eq!(line(args), None, "{args}");
        }
    }
}
