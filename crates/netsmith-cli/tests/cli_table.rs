//! The command line as the documents name it (issue #1090): the table of the guide's commands
//! chapter has every command with each of its flags, and every `netsmith ...` line of the guide,
//! the skills and the `AGENTS.md` that `project init` writes names a command that parses, with
//! flags it has. They are what `scripts/ci-changes` maps to `netsmith-tools`, whose dependents
//! (this crate) run when they change; the README and the repository's `AGENTS.md` are prose to
//! it, so they are not read here.

use std::fs;
use std::path::{Path, PathBuf};

/// The chapter with the table.
const CHAPTER: &str = "docs/language/27-commands.md";

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every command that runs (a leaf of clap's model) as the words that name it, with its model.
pub(crate) fn leaves() -> Vec<(String, clap::Command)> {
    fn walk(path: &[String], command: &clap::Command, all: &mut Vec<(String, clap::Command)>) {
        let subs: Vec<_> = command
            .get_subcommands()
            .filter(|s| s.get_name() != "help")
            .collect();
        if subs.is_empty() && !path.is_empty() {
            all.push((path.join(" "), command.clone()));
        }
        for sub in subs {
            let mut path = path.to_vec();
            path.push(sub.get_name().to_string());
            walk(&path, sub, all);
        }
    }
    let mut all = Vec::new();
    walk(&[], &crate::cli_help::model(), &mut all);
    all
}

/// The rows of the table: the command (without `netsmith`) and the text of its flags' cell.
fn table() -> Vec<(String, String)> {
    let text = fs::read_to_string(repo().join(CHAPTER)).unwrap();
    text.lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.trim().trim_matches('|').split(" | ").collect();
            let command = cells.first()?.trim().strip_prefix("`netsmith ")?;
            Some((
                command.strip_suffix('`')?.to_string(),
                cells.get(2)?.trim().to_string(),
            ))
        })
        .collect()
}

#[test]
fn every_command_is_in_the_guides_table_with_each_of_its_flags() {
    let rows = table();
    let leaves = leaves();
    assert!(leaves.len() > 30, "{}", leaves.len());
    for (name, model) in &leaves {
        let found: Vec<_> = rows.iter().filter(|(c, _)| c == name).collect();
        assert_eq!(found.len(), 1, "`netsmith {name}` is in {CHAPTER}'s table once");
        let flags = &found[0].1;
        for arg in model.get_arguments().filter(|a| !a.is_hide_set()) {
            if let Some(long) = arg.get_long().filter(|l| *l != "help") {
                assert!(
                    flags.contains(&format!("`--{long}`")),
                    "`netsmith {name}`: no `--{long}` in its row of {CHAPTER}: {flags}"
                );
            }
        }
    }
    for (command, _) in &rows {
        assert!(
            leaves.iter().any(|(name, _)| name == command),
            "{CHAPTER} has a row for `netsmith {command}`, which is not a command"
        );
    }
}

/// The commands of an earlier release (`commands::moved` has where each went): a document that
/// names one is out of date.
const BEFORE: &[&str] = &[
    "init", "lock", "check", "build", "fab", "explain", "compare", "route", "parity", "board3d",
    "render", "editor", "part", "port", "kicad-libs", "dxf", "case",
];

/// Every `netsmith <command> ...` in a code span or a shell block of `text`, up to the end of
/// the span or the line (and a `#` comment), as its words. A word after `netsmith` that is no
/// command of today or of before is prose (`netsmith reads...`) and is left out.
fn named_commands(text: &str, model: &clap::Command) -> Vec<Vec<String>> {
    let nouns: Vec<&str> = model.get_subcommands().map(|s| s.get_name()).collect();
    let mut found = Vec::new();
    let mut block: Option<bool> = None;
    for line in text.lines() {
        if let Some(fence) = line.trim_start().strip_prefix("```") {
            block = match block {
                Some(_) => None,
                None => Some(matches!(fence.trim(), "" | "sh" | "bash" | "shell" | "console")),
            };
            continue;
        }
        let pieces: Vec<&str> = match block {
            Some(true) => vec![line.split(" #").next().unwrap_or("")],
            Some(false) => vec![],
            // The odd pieces between backticks are code spans.
            None => line.split('`').skip(1).step_by(2).collect(),
        };
        for piece in pieces {
            let mut rest = piece;
            while let Some(at) = rest.find("netsmith ") {
                let before = &rest[..at];
                rest = &rest[at + "netsmith ".len()..];
                // `cargo run -p netsmith-cli -- ...` and a path ending in netsmith are not it.
                if before.ends_with(['-', '/', '_']) {
                    continue;
                }
                let words: Vec<String> = rest
                    .split_whitespace()
                    .take_while(|w| !matches!(*w, "#" | "|" | "&&" | ";" | ">" | "2>&1"))
                    .map(|w| w.trim_end_matches([',', ';', ')', '.']).to_string())
                    .filter(|w| !w.is_empty())
                    .collect();
                let first = words.first().map(String::as_str).unwrap_or("");
                if nouns.contains(&first) || BEFORE.contains(&first) {
                    found.push(words);
                }
            }
        }
    }
    found
}

/// Whether `words` (after `netsmith`) name a command of `model` with flags it has: `Err` says
/// what is wrong.
fn parses(model: &clap::Command, words: &[String]) -> Result<(), String> {
    let mut command = model;
    let mut rest = words;
    while let Some(word) = rest.first() {
        let subs: Vec<_> = command
            .get_subcommands()
            .filter(|s| s.get_name() != "help")
            .collect();
        if subs.is_empty() {
            break;
        }
        if word.starts_with('-') || word.starts_with('<') || word.starts_with('[') {
            break;
        }
        match subs.iter().find(|s| s.get_name() == word) {
            Some(sub) => {
                command = sub;
                rest = &rest[1..];
            }
            None => {
                return Err(format!(
                    "`{word}` is not a command of `netsmith {}`",
                    words[..words.len() - rest.len()].join(" ")
                ));
            }
        }
    }
    let takes: Vec<&str> = command
        .get_arguments()
        .filter_map(|a| a.get_long())
        .chain(["help", "version"])
        .collect();
    for word in rest {
        let Some(flag) = word.strip_prefix("--") else {
            continue;
        };
        let flag = flag.split(['=', '`', '[', '<']).next().unwrap_or("");
        if !flag.is_empty() && !takes.contains(&flag) {
            return Err(format!(
                "`netsmith {}` has no `--{flag}`",
                words[..words.len() - rest.len()].join(" ")
            ));
        }
    }
    Ok(())
}

/// The documents an agent or a person runs commands from.
fn documents() -> Vec<PathBuf> {
    let root = repo();
    let mut files = vec![
        root.join("docs/LANGUAGE.md"),
        root.join("crates/netsmith-tools/templates/AGENTS.md"),
    ];
    for dir in ["docs/language", ".agents/skills"] {
        let mut stack = vec![root.join(dir)];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "md") {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}

#[test]
fn every_command_the_skills_and_the_guide_name_parses() {
    let model = crate::cli_help::model();
    let mut wrong = Vec::new();
    let mut seen = 0;
    for file in documents() {
        let mut text = fs::read_to_string(&file).unwrap();
        if file.ends_with(CHAPTER) {
            // The table's middle column is the spelling before, which is an error now.
            text = text
                .lines()
                .map(|l| match l.starts_with("| `netsmith ") {
                    true => l.split(" | ").next().unwrap_or(""),
                    false => l,
                })
                .collect::<Vec<_>>()
                .join("\n");
        }
        for words in named_commands(&text, &model) {
            seen += 1;
            if let Err(e) = parses(&model, &words) {
                let shown = file.strip_prefix(repo()).unwrap_or(&file).display();
                wrong.push(format!("{shown}: `netsmith {}`: {e}", words.join(" ")));
            }
        }
    }
    assert!(seen > 100, "only {seen} commands found");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_reader_of_the_documents_finds_commands_and_their_mistakes() {
    let model = crate::cli_help::model();
    let text = "Run `netsmith board fab --out fab` and `netsmith fab`.\n\
                ```sh\nnetsmith layout show --file a.nsm  # a comment\n```\n\
                `cargo run -p netsmith-cli -- board build` and `netsmith.toml` and `netsmith <noun>`\n\
                and `netsmith reads it` and `netsmith editor: diagnostics`.\n\
                ```text\nerror: netsmith fab is not\n```";
    let found = named_commands(text, &model);
    assert_eq!(
        found,
        [
            vec!["board", "fab", "--out", "fab"],
            vec!["fab"],
            vec!["layout", "show", "--file", "a.nsm"],
        ]
    );
    assert_eq!(parses(&model, &found[0]), Ok(()));
    assert!(parses(&model, &found[1]).unwrap_err().contains("`fab`"));
    assert_eq!(parses(&model, &found[2]), Ok(()));
    let words = |s: &str| s.split(' ').map(String::from).collect::<Vec<_>>();
    assert!(
        parses(&model, &words("layout show --into a.nsm"))
            .unwrap_err()
            .contains("--into")
    );
    assert_eq!(parses(&model, &words("layout")), Ok(()));
}
