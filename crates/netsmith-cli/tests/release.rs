//! The release a project is written for (issues #740 and #1657): a command that builds or
//! checks a project written for another minor release than this netsmith warns of it, once, and
//! refuses nothing; a project of this minor release, or that names none, gets no warning, and one
//! that names none whose check fails is told it was made before the key.

use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

use netsmith_testkit::TempDir;
use serde_json::Value;

const LED: &str = include_str!("../../../examples/led.nsm");
const MIGRATING: &str = "docs/MIGRATING.md";
const THIS: &str = env!("CARGO_PKG_VERSION");
/// What every warning of another release says, and nothing else does.
const WARNED: &str = "the project is written for netsmith";

/// A project of the LED example whose manifest has the lines `release` after the name, and whose
/// board does not build when `broken`.
fn project(label: &str, release: &str, broken: bool) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        format!("[project]\nname = \"led\"\n{release}"),
    )
    .unwrap();
    let source = if broken {
        LED.replacen(".r1: Resistor", ".r1: Resistr", 1)
    } else {
        LED.to_string()
    };
    assert_eq!(source != LED, broken, "the example changed");
    fs::write(root.join("src/main.nsm"), source).unwrap();
    root
}

fn run(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        )
        .output()
        .unwrap()
}

fn stderr(run: &Output) -> String {
    String::from_utf8_lossy(&run.stderr).into_owned()
}

fn stdout(run: &Output) -> String {
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// The JSON document of a run, and its findings of the code `release`.
fn document(run: &Output) -> (Value, Vec<Value>) {
    let doc: Value = serde_json::from_slice(&run.stdout)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}{}", stdout(run), stderr(run)));
    let release = doc["findings"]
        .as_array()
        .map(|findings| {
            findings
                .iter()
                .filter(|f| f["code"] == "release")
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    (doc, release)
}

/// The commands the warning is tested with. `board drc` finds the connections no routes file
/// makes, so it exits 1: nothing is refused for the release.
const COMMANDS: [(&[&str], i32); 3] = [
    (&["board", "check"], 0),
    (&["board", "build"], 0),
    (&["board", "drc"], 1),
];

#[test]
fn a_project_of_an_older_minor_release_is_warned_of_once() {
    let root = project("release-warns", "netsmith = \"0.0.1\"\n", false);
    for (command, exit) in COMMANDS {
        let text = run(&root, command);
        let said = stderr(&text);
        assert_eq!(text.status.code(), Some(exit), "{command:?}: {said}");
        let warnings: Vec<&str> = said.lines().filter(|l| l.contains(WARNED)).collect();
        assert_eq!(warnings.len(), 1, "{command:?}: {said}");
        assert!(
            warnings[0].starts_with("warning: the project is written for netsmith 0.0.1"),
            "{}",
            warnings[0]
        );
        assert!(warnings[0].contains(THIS), "{}", warnings[0]);
        assert!(warnings[0].contains(MIGRATING), "{}", warnings[0]);
        assert!(warnings[0].contains("nothing is refused"), "{}", warnings[0]);
        assert!(!stdout(&text).contains(WARNED), "{}", stdout(&text));

        // With `--format json` it is a note of the document, the first, and said nowhere else.
        let json = run(&root, &[command, &["--format", "json"]].concat());
        assert_eq!(json.status.code(), Some(exit), "{}", stderr(&json));
        let (doc, release) = document(&json);
        assert_eq!(release.len(), 1, "{command:?}: {doc:#}");
        assert_eq!(release[0]["severity"], "note", "{doc:#}");
        assert_eq!(doc["findings"][0]["code"], "release", "{doc:#}");
        assert!(
            release[0]["message"].as_str().unwrap().contains(WARNED),
            "{doc:#}"
        );
        assert!(!stderr(&json).contains(WARNED), "{}", stderr(&json));
    }
}

#[test]
fn a_project_of_this_minor_release_or_of_none_is_not_warned_of() {
    let (minor, patch) = THIS.rsplit_once('.').unwrap();
    let later_patch = patch.parse::<u64>().unwrap() + 1;
    for (label, release) in [
        ("release-this", format!("netsmith = \"{THIS}\"\n")),
        (
            "release-patch",
            format!("netsmith = \"{minor}.{later_patch}\"\n"),
        ),
        ("release-minor", format!("netsmith = \"{minor}\"\n")),
        ("release-unnamed", String::new()),
    ] {
        let root = project(label, &release, false);
        for (command, exit) in COMMANDS {
            let text = run(&root, command);
            let said = stderr(&text);
            assert_eq!(text.status.code(), Some(exit), "{label} {command:?}: {said}");
            assert!(!said.contains(WARNED), "{label} {command:?}: {said}");
            assert!(!said.contains(MIGRATING), "{label} {command:?}: {said}");
            let json = run(&root, &[command, &["--format", "json"]].concat());
            let (doc, release) = document(&json);
            assert_eq!(release, Vec::<Value>::new(), "{label} {command:?}: {doc:#}");
        }
    }
}

/// A command whose JSON is not a document of findings says the warning on stderr, once, and
/// leaves the document as it is; `project lock` has it among findings of its own.
#[test]
fn a_view_says_the_warning_on_stderr_and_the_lock_in_its_document() {
    let root = project("release-views", "netsmith = \"0.0\"\n", false);
    let bom = run(&root, &["board", "bom", "--format", "json"]);
    assert!(bom.status.success(), "{}", stderr(&bom));
    assert_eq!(stderr(&bom).matches(WARNED).count(), 1, "{}", stderr(&bom));
    assert!(!stdout(&bom).contains(WARNED), "{}", stdout(&bom));

    let lock = run(
        &root,
        &["project", "lock", "--update", "--format", "json"],
    );
    assert!(lock.status.success(), "{}", stderr(&lock));
    let (doc, release) = document(&lock);
    assert_eq!(release.len(), 1, "{doc:#}");
    assert!(!stderr(&lock).contains(WARNED), "{}", stderr(&lock));
}

/// The warning says what the note of a failed check said (issue #740), so there is one message.
#[test]
fn a_failed_check_of_an_older_project_has_one_message_of_the_release() {
    let root = project("release-older", "netsmith = \"0.0.1\"\n", true);
    let text = run(&root, &["board", "check"]);
    assert_eq!(text.status.code(), Some(1), "{}", stderr(&text));
    let said = stderr(&text);
    assert_eq!(said.matches(MIGRATING).count(), 1, "{said}");
    // After the errors it may explain.
    assert!(
        said.lines().last().unwrap().starts_with(&format!("warning: {WARNED}")),
        "{said}"
    );
    assert!(said.contains(&format!("set `netsmith = \"{THIS}\"`")), "{said}");

    let json = run(&root, &["board", "check", "--format", "json"]);
    assert_eq!(json.status.code(), Some(1));
    let (doc, release) = document(&json);
    assert_eq!(release.len(), 1, "{doc:#}");
}

#[test]
fn a_failed_check_of_a_project_with_no_release_says_it_was_made_before_the_key() {
    let root = project("release-none", "", true);
    let said = stderr(&run(&root, &["board", "check"]));
    let note = said
        .lines()
        .find(|l| l.contains(MIGRATING))
        .unwrap_or_else(|| panic!("no note of the release: {said}"));
    assert!(
        note.starts_with(
            "note: netsmith.toml does not say which netsmith release the project is written for"
        ),
        "{note}"
    );
    let (doc, release) = document(&run(&root, &["board", "check", "--format", "json"]));
    assert_eq!(release.len(), 1, "{doc:#}");
    assert_eq!(doc["findings"].as_array().unwrap().last(), release.last());
    // Written for this netsmith: an error is only an error.
    let root = project("release-this-broken", &format!("netsmith = \"{THIS}\"\n"), true);
    let said = stderr(&run(&root, &["board", "check"]));
    assert!(!said.contains(MIGRATING), "{said}");
}
