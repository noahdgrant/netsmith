//! `netsmith board compare --ref` (issue #1097): the project compared with a git ref, in a
//! repository the test makes. Its `--format json` is held to what the editor's
//! `GET /api/compare` answers for the same refs (the real server, over a socket), and its exit
//! code says whether the circuit differs. Skipped when `git` or `tar` is not on the PATH.

use std::{
    fs,
    net::{Ipv4Addr, TcpListener},
    path::Path,
    process::Command,
    sync::{Arc, Mutex, MutexGuard},
    thread,
};

use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;
use serde_json::Value;

const LED: &str = include_str!("../../../examples/led.nsm");

/// A compare reads a ref into a scratch folder: one at a time, as the workspace's own tests do.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
}

fn tools_here() -> bool {
    let works = |program: &str| {
        Command::new(program)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    };
    let here = works("git") && works("tar");
    if !here {
        eprintln!("skipped: git or tar is not on the PATH");
    }
    here
}

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_project(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
}

fn edit(root: &Path, from: &str, to: &str) {
    let path = root.join("src/main.nsm");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(from), "main.nsm has no `{from}`");
    fs::write(path, text.replacen(from, to, 1)).unwrap();
}

/// A repository with the example board committed, then its supply net renamed and committed,
/// then (not committed) the LED's current range narrowed.
fn repository(label: &str) -> TempDir {
    let root = TempDir::new(label);
    git(&root, &["init", "-q"]);
    git(&root, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(&root, &["config", "user.name", "Test Author"]);
    git(&root, &["config", "user.email", "test@example.com"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    write_project(&root);
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "the board"]);
    edit(&root, "\"VCC\"", "\"VBUS\"");
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "rename the supply net"]);
    edit(&root, "5mA..15mA", "5mA..10mA");
    root
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn netsmith(dir: &Path, args: &[&str]) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(["board", "compare"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    Run {
        code: out.status.code(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// The editor's server over the project, on a free port; `GET` of a path with its query.
fn editor(root: &Path) -> impl Fn(&str) -> Value {
    let workspace = Workspace::open(Target::resolve_from(root, None).unwrap()).unwrap();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || netsmith_editor::serve(listener, Arc::new(workspace)));
    move |path: &str| {
        let mut reply = ureq::get(format!("http://127.0.0.1:{port}{path}"))
            .header("X-Netsmith", "1")
            .call()
            .unwrap_or_else(|e| panic!("GET {path}: {e}"));
        serde_json::from_str(&reply.body_mut().read_to_string().unwrap()).unwrap()
    }
}

/// What differs between two asks for the same comparison: how long it took and whether the
/// reading was kept are not the circuit.
fn circuit(mut answer: Value) -> Value {
    let object = answer.as_object_mut().unwrap();
    object.remove("elapsedMs");
    object.remove("cached");
    answer
}

#[test]
fn the_changes_equal_the_editors_and_the_exit_code_says_the_circuit_differs() {
    if !tools_here() {
        return;
    }
    let _one = lock();
    let root = repository("compare-ref");
    let get = editor(&root);

    // The earlier commit against the files on disk: the renamed net and the narrowed current.
    let run = netsmith(&root, &["--ref", "HEAD~1", "--format", "json"]);
    assert_eq!(run.code, Some(1), "{}{}", run.stdout, run.stderr);
    let ours: Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(ours["changed"], true, "{ours:#}");
    assert!(
        ours["boards"][0]["nets"].to_string().contains("VBUS"),
        "{ours:#}"
    );
    assert_eq!(circuit(ours), circuit(get("/api/compare?ref=HEAD~1")));

    // Two commits: the files on disk are not read.
    let run = netsmith(
        &root,
        &["--ref", "HEAD~1", "--against-ref", "HEAD", "--format", "json"],
    );
    assert_eq!(run.code, Some(1), "{}{}", run.stdout, run.stderr);
    let ours: Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(ours["against"], "HEAD");
    assert_eq!(
        circuit(ours),
        circuit(get("/api/compare?ref=HEAD~1&against=HEAD"))
    );

    // The text lists the same changes for a person.
    let run = netsmith(&root, &["--ref", "HEAD~1", "--against-ref", "HEAD"]);
    assert_eq!(run.code, Some(1), "{}{}", run.stdout, run.stderr);
    assert!(
        run.stdout.contains("compare ") && run.stdout.contains("board LedExample"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("VCC") && run.stdout.contains("VBUS"),
        "{}",
        run.stdout
    );
}

#[test]
fn the_exit_code_is_0_when_the_circuit_is_the_same_and_2_for_a_mistake() {
    if !tools_here() {
        return;
    }
    let _one = lock();
    let root = repository("compare-ref-same");

    // A commit against itself: nothing changed.
    let run = netsmith(&root, &["--ref", "HEAD", "--against-ref", "HEAD"]);
    assert_eq!(run.code, Some(0), "{}{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("No circuit changes"), "{}", run.stdout);
    let run = netsmith(
        &root,
        &["--ref", "HEAD", "--against-ref", "HEAD", "--format", "json"],
    );
    assert_eq!(run.code, Some(0), "{}{}", run.stdout, run.stderr);
    let json: Value = serde_json::from_str(&run.stdout).unwrap();
    assert_eq!(json["changed"], false, "{json:#}");

    // A ref that does not exist.
    let run = netsmith(&root, &["--ref", "no-such-branch"]);
    assert_eq!(run.code, Some(2), "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("no-such-branch"), "{}", run.stderr);

    // A project that is in no git work tree.
    let plain = TempDir::new("compare-ref-no-git");
    write_project(&plain);
    let run = netsmith(&plain, &["--ref", "HEAD"]);
    assert_eq!(run.code, Some(2), "{}{}", run.stdout, run.stderr);
    assert!(run.stderr.contains("git"), "{}", run.stderr);
}
