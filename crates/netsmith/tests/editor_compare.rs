//! Comparing the project with a git ref (ADR 0059): throwaway repositories with a small
//! project, committed, changed and compared. Skipped when `git` or `tar` is not on the PATH.
//!
//! The tests take a lock, so one compare runs at a time: each reads a ref with a scratch
//! folder, and the last test looks for any that was left behind.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, MutexGuard},
};

use netsmith::{editor::Workspace, project::Target};
use netsmith_editor::{
    AnalysisEvent, ApiError, Backend, Observer,
    api::{BoardStatus, Compare, CompareBoard, GitRefKind, Severity, SourceStatus},
};
use netsmith_testkit::TempDir;

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");
const MODULE: &str = "//! A module.\n\nimport std::sources::Rail\n\n/// A supply.\ninterface Supply {\n    .rail: Rail\n}\n";

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

fn git(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// A repository whose project is in `sub` (or at its top, for an empty `sub`).
struct Repo {
    dir: TempDir,
    project: PathBuf,
}

impl Repo {
    fn new(label: &str, sub: &str) -> Repo {
        let dir = TempDir::new(&format!("compare-{label}"));
        git(&dir, &["init", "-q"]);
        git(&dir, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        git(&dir, &["config", "user.name", "Test Author"]);
        git(&dir, &["config", "user.email", "test@example.com"]);
        git(&dir, &["config", "commit.gpgsign", "false"]);
        git(&dir, &["config", "tag.gpgsign", "false"]);
        let project = if sub.is_empty() {
            dir.to_path_buf()
        } else {
            dir.join(sub)
        };
        fs::create_dir_all(project.join("src")).unwrap();
        fs::write(
            project.join("netsmith.toml"),
            "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
        )
        .unwrap();
        fs::write(project.join("src/main.nsm"), TWO_BOARDS).unwrap();
        fs::write(project.join("src/power.nsm"), MODULE).unwrap();
        Repo { dir, project }
    }

    fn commit(&self, message: &str) -> String {
        git(&self.dir, &["add", "-A"]);
        git(&self.dir, &["commit", "-q", "-m", message]);
        git(&self.dir, &["rev-parse", "HEAD"])
    }

    /// Change `file` of the project: `from` becomes `to`.
    fn edit(&self, file: &str, from: &str, to: &str) {
        let path = self.project.join(file);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} has no `{from}`");
        fs::write(path, text.replacen(from, to, 1)).unwrap();
    }

    fn workspace(&self) -> Workspace {
        Workspace::open(Target::resolve_from(&self.project, None).unwrap())
            .unwrap_or_else(|e| panic!("{e}"))
    }
}

fn silent() -> Observer {
    Arc::new(|_| {})
}

/// The scratch folders of this process that are still there.
fn scratch_folders() -> Vec<String> {
    let mine = format!("netsmith-compare-{}-", std::process::id());
    fs::read_dir(std::env::temp_dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(&mine))
        .collect()
}

fn compare_with(
    w: &Workspace,
    rev: &str,
    board: Option<&str>,
    observe: &Observer,
) -> Result<Compare, ApiError> {
    let _one = lock();
    let result = w.compare(rev, board, observe, &netsmith_editor::Cancel::new());
    assert_eq!(
        scratch_folders(),
        Vec::<String>::new(),
        "a scratch folder was left"
    );
    result
}

fn compare(w: &Workspace, rev: &str) -> Result<Compare, ApiError> {
    compare_with(w, rev, None, &silent())
}

/// Every `from` becomes `to`, in a file of the project.
fn edit_all(r: &Repo, file: &str, from: &str, to: &str) {
    let path = r.project.join(file);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains(from), "{file} has no `{from}`");
    fs::write(path, text.replace(from, to)).unwrap();
}

const BULK: &str =
    "    .bulk: Capacitor { .effective_capacitance >= 10uF }\n    .vin.pos ~> .bulk ~> .vin.neg\n";
const INPUT_CAP: &str = "    .input_cap:  Capacitor { .effective_capacitance >= 1uF }\n";

/// A repository with the project committed once, and its workspace.
fn committed(label: &str, sub: &str) -> (Repo, String, Workspace) {
    let r = Repo::new(label, sub);
    let first = r.commit("first");
    let w = r.workspace();
    (r, first, w)
}

macro_rules! need_tools {
    () => {
        if !tools_here() {
            return;
        }
    };
}

fn board<'a>(c: &'a Compare, name: &str) -> &'a CompareBoard {
    c.boards.iter().find(|b| b.name == name).unwrap_or_else(|| {
        panic!(
            "no board {name} in {:?}",
            c.boards.iter().map(|b| &b.name).collect::<Vec<_>>()
        )
    })
}

#[test]
fn nothing_changed_is_an_empty_diff() {
    need_tools!();
    let (_r, _, w) = committed("same", "");
    let c = compare(&w, "HEAD").unwrap();
    assert!(!c.changed, "{c:#?}");
    assert_eq!(c.summary, "No circuit changes");
    assert_eq!(c.error, None);
    assert!(!c.head.dirty && c.head.errors == 0);
    assert_eq!(
        (c.source.files_changed, c.source.added, c.source.removed),
        (0, 0, 0)
    );
    assert!(c.boards.iter().all(|b| b.status == BoardStatus::Unchanged));
    let names: Vec<&str> = c.boards.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["TwoBoards/led", "TwoBoards/power"]);
    for b in &c.boards {
        assert!(
            b.parts.added.is_empty() && b.parts.removed.is_empty() && b.parts.changed.is_empty()
        );
        assert!(b.nets.added.is_empty() && b.nets.changed.is_empty() && b.nets.renamed.is_empty());
        assert!(b.problems.new.is_empty() && b.problems.resolved.is_empty());
        assert!(b.parts.unchanged > 0 && b.nets.unchanged > 0);
    }
}

#[test]
fn a_changed_value_is_a_field_with_old_and_new_and_a_designator_on_each_side() {
    need_tools!();
    let (r, _, _) = committed("value", "");
    r.edit(
        "src/main.nsm",
        ".r1: Resistor {}",
        ".r1: Resistor { .resistance &= 150ohm +/- 1% }",
    );
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let led = board(&c, "TwoBoards/led");
    assert_eq!(led.status, BoardStatus::Changed);
    let change = led
        .parts
        .changed
        .iter()
        .find(|p| p.path == "r1")
        .expect("r1 changed");
    let value = change.fields.iter().find(|f| f.field == "value").unwrap();
    assert_ne!(value.old, value.new);
    assert_eq!(value.new.as_deref(), Some("150"));
    assert_eq!(
        (
            change.designator_base.as_str(),
            change.designator_head.as_str()
        ),
        ("R1", "R1")
    );
    // Where it is declared: the line that changed, and the line it was.
    let at = change.at.as_ref().unwrap();
    assert_eq!(at.file, "src/main.nsm");
    assert_eq!(change.base_at.as_ref().unwrap().line, at.line);
    // The LED the solver picks for the new current is another one too.
    assert!(led.parts.changed.iter().any(|p| p.path == "led"));
    assert_eq!(
        c.summary,
        "2 values changed, 1 system port changed, 1 rail changed"
    );
    // The power board is untouched.
    assert_eq!(board(&c, "TwoBoards/power").status, BoardStatus::Unchanged);
    // The line of the bill of materials changed with it.
    let bom = led.bom.as_ref().expect("a bill of materials");
    assert!(!bom.added.is_empty() && !bom.removed.is_empty(), "{bom:#?}");
    assert_eq!(
        (c.source.files_changed, c.source.added, c.source.removed),
        (1, 1, 1)
    );
}

#[test]
fn a_part_added_comes_with_its_nets_its_bom_line_and_renumbers_the_rest() {
    need_tools!();
    let (r, _, _) = committed("added", "");
    r.edit("src/main.nsm", INPUT_CAP, &format!("{BULK}{INPUT_CAP}"));
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let power = board(&c, "TwoBoards/power");
    assert_eq!(power.status, BoardStatus::Changed);
    assert_eq!(power.parts.added.len(), 1);
    let bulk = &power.parts.added[0];
    assert_eq!(
        (bulk.path.as_str(), bulk.kind.as_str()),
        ("stage.bulk", "Capacitor")
    );
    assert_eq!(bulk.at.as_ref().unwrap().line, 24);
    assert!(bulk.lcsc.is_some());
    // It is on the two nets of the stage's input, which gain its pins.
    let names: Vec<&str> = power.nets.changed.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["GND", "VIN"]);
    let vin = power.nets.changed.iter().find(|n| n.name == "VIN").unwrap();
    assert_eq!(vin.joined.len(), 1);
    assert_eq!(
        (vin.joined[0].part.as_str(), vin.joined[0].pin.as_str()),
        ("stage.bulk", "1")
    );
    assert!(vin.left.is_empty());
    assert_eq!(vin.at.as_ref().unwrap().line, 24);
    // The capacitors after it take the next designators, and that is not a change.
    assert!(power.parts.changed.is_empty());
    assert_eq!(power.parts.renumbered.len(), 2);
    assert!(
        power
            .parts
            .renumbered
            .iter()
            .all(|p| p.fields.len() == 1 && p.fields[0].field == "designator")
    );
    let bom = power.bom.as_ref().unwrap();
    assert_eq!(bom.added.len(), 1);
    assert_eq!(bom.added[0].designators, ["C1"]);
    assert_eq!(c.summary, "1 part added, 2 nets changed");
    assert_eq!(board(&c, "TwoBoards/led").status, BoardStatus::Unchanged);
}

#[test]
fn a_part_removed_is_listed_with_the_line_it_was_declared_on_at_the_ref() {
    need_tools!();
    let r = Repo::new("removed", "");
    r.edit("src/main.nsm", INPUT_CAP, &format!("{BULK}{INPUT_CAP}"));
    r.commit("with the bulk capacitor");
    r.edit("src/main.nsm", BULK, "");
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let power = board(&c, "TwoBoards/power");
    assert_eq!(power.parts.removed.len(), 1);
    assert_eq!(power.parts.removed[0].path, "stage.bulk");
    assert_eq!(power.parts.removed[0].at.as_ref().unwrap().line, 24);
    assert!(power.parts.added.is_empty());
    let vin = power.nets.changed.iter().find(|n| n.name == "VIN").unwrap();
    assert!(vin.joined.is_empty() && vin.left.len() == 1);
    assert_eq!(vin.left[0].designator, "C1");
    assert_eq!(power.bom.as_ref().unwrap().removed.len(), 1);
    assert_eq!(c.summary, "1 part removed, 2 nets changed");
}

#[test]
fn a_renamed_net_is_the_same_net_under_another_name() {
    need_tools!();
    let (r, _, _) = committed("renamed", "");
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let power = board(&c, "TwoBoards/power");
    assert_eq!(power.nets.renamed.len(), 1, "{:#?}", power.nets);
    let n = &power.nets.renamed[0];
    assert_eq!((n.base_name.as_str(), n.name.as_str()), ("3V3", "VOUT"));
    assert!(n.renamed && n.joined.is_empty() && n.left.is_empty());
    assert!(power.nets.added.is_empty() && power.nets.removed.is_empty());
    // The rail on it is the same rail: nothing else in the board changed.
    assert!(power.rails.added.is_empty() && power.rails.removed.is_empty());
    assert!(power.parts.changed.is_empty() && power.parts.added.is_empty());
    assert_eq!(c.summary, "1 net renamed");
}

#[test]
fn a_module_that_moved_is_the_same_parts_at_new_paths() {
    need_tools!();
    let (r, _, _) = committed("moved", "");
    edit_all(&r, "src/main.nsm", ".stage", ".ldo");
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let power = board(&c, "TwoBoards/power");
    assert!(
        power.parts.added.is_empty() && power.parts.removed.is_empty(),
        "{:#?}",
        power.parts
    );
    assert_eq!(power.parts.changed.len(), 3);
    let r = power
        .parts
        .changed
        .iter()
        .find(|p| p.path == "ldo.regulator")
        .unwrap();
    assert_eq!(r.base_path.as_deref(), Some("stage.regulator"));
    assert_eq!(r.fields.len(), 1);
    assert_eq!(r.fields[0].field, "path");
    assert_eq!(
        (r.fields[0].old.as_deref(), r.fields[0].new.as_deref()),
        (Some("stage.regulator"), Some("ldo.regulator"))
    );
    // The module is a new path and an old one gone, and the nets did not change.
    assert_eq!(
        power
            .modules
            .added
            .iter()
            .map(|m| m.path.as_str())
            .collect::<Vec<_>>(),
        ["ldo"]
    );
    assert_eq!(
        power
            .modules
            .removed
            .iter()
            .map(|m| m.path.as_str())
            .collect::<Vec<_>>(),
        ["stage"]
    );
    assert!(
        power.nets.added.is_empty()
            && power.nets.removed.is_empty()
            && power.nets.changed.is_empty()
    );
    assert!(
        power.nets.renamed.is_empty(),
        "a net netsmith named from its pins follows the pins"
    );
    assert!(power.bom.as_ref().unwrap().added.is_empty());
    assert!(c.summary.starts_with("3 parts moved"), "{}", c.summary);
}

#[test]
fn a_rail_that_lost_headroom_says_from_what_to_what() {
    need_tools!();
    let (r, _, _) = committed("headroom", "");
    r.edit(
        "src/main.nsm",
        ".current &= 0mA..50mA",
        ".current &= 0mA..80mA",
    );
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let power = board(&c, "TwoBoards/power");
    let rail = power
        .rails
        .changed
        .iter()
        .find(|r| r.net == "3V3")
        .expect("the 3V3 rail changed");
    assert_eq!(
        rail.demand.as_ref().unwrap().base.as_ref().unwrap().hi,
        0.05
    );
    assert_eq!(
        rail.demand.as_ref().unwrap().head.as_ref().unwrap().hi,
        0.08
    );
    assert!(rail.capacity.is_none(), "the supply is the same 100 mA");
    assert!((rail.headroom_base.unwrap() - 0.05).abs() < 1e-9);
    assert!((rail.headroom_head.unwrap() - 0.02).abs() < 1e-9);
    assert_eq!(rail.budget, None);
    assert!(rail.at.is_some());
    assert!(
        c.summary
            .contains("1 rail's headroom shrank from 50 mA to 20 mA"),
        "{}",
        c.summary
    );
}

#[test]
fn a_ref_that_does_not_build_is_reported_with_its_problems_and_not_as_a_failure() {
    need_tools!();
    let r = Repo::new("broken", "");
    r.edit("src/main.nsm", ".r1: Resistor {}", ".r1: Resistr {}");
    let broken = r.commit("a typo");
    r.edit("src/main.nsm", ".r1: Resistr {}", ".r1: Resistor {}");
    let w = r.workspace();
    let c = compare(&w, &broken[..10]).unwrap();
    let error = c.error.as_deref().expect("an error");
    assert!(
        error.contains("does not analyse at") && error.contains("Resistr"),
        "{error}"
    );
    assert!(error.contains(&broken[..7]));
    assert_eq!(c.base_problems.len(), 1);
    assert_eq!(c.base_problems[0].severity, Severity::Error);
    assert!(c.base_problems[0].at.is_some());
    assert!(c.boards.is_empty() && !c.changed);
    assert_eq!(c.summary, *error);
    assert_eq!(c.base.hash, broken);
    // The files on disk are fine.
    assert_eq!(c.head.errors, 0);
    // Said again from the cache, the same.
    let again = compare(&w, &broken).unwrap();
    assert!(again.cached && again.error == c.error);
}

#[test]
fn a_ref_that_has_no_such_project_says_so() {
    need_tools!();
    let r = Repo::new("early", "");
    fs::remove_file(r.project.join("netsmith.toml")).unwrap();
    fs::remove_dir_all(r.project.join("src")).unwrap();
    fs::write(r.project.join("README"), "nothing yet\n").unwrap();
    let early = r.commit("before the project");
    // Now the project is there on disk.
    fs::write(
        r.project.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::create_dir_all(r.project.join("src")).unwrap();
    fs::write(r.project.join("src/main.nsm"), TWO_BOARDS).unwrap();
    let c = compare(&r.workspace(), &early).unwrap();
    let error = c.error.expect("an error");
    assert!(
        error.contains("has no netsmith.toml at") && error.contains(&early[..7]),
        "{error}"
    );
    assert!(c.boards.is_empty());
}

#[test]
fn a_ref_that_does_not_exist_is_404_and_something_that_is_not_a_ref_is_400() {
    need_tools!();
    let (r, _, w) = committed("norev", "");
    let e = compare(&w, "no-such-branch").unwrap_err();
    assert_eq!(e.status, 404);
    assert!(e.message.contains("no-such-branch"), "{}", e.message);
    let e = compare(&w, "--output=/tmp/x").unwrap_err();
    assert_eq!(e.status, 400, "{}", e.message);
    assert_eq!(compare(&w, "").unwrap_err().status, 400);
    assert_eq!(compare(&w, "HEAD~5").unwrap_err().status, 404);
    // A file in the tree is not a revision, even when it has the name.
    fs::write(r.project.join("topic"), "x").unwrap();
    assert_eq!(compare(&w, "topic").unwrap_err().status, 404);
}

#[test]
fn a_folder_that_is_not_in_a_git_work_tree_cannot_be_compared_and_says_why() {
    need_tools!();
    let dir = TempDir::new("compare-nogit");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("src/main.nsm"), TWO_BOARDS).unwrap();
    let w = Workspace::open(Target::resolve_from(&dir, None).unwrap()).unwrap();
    let info = w.git();
    assert!(!info.available);
    assert!(
        info.reason
            .as_deref()
            .unwrap()
            .contains("not inside a git work tree"),
        "{:?}",
        info.reason
    );
    assert!(info.refs.is_empty() && info.commits.is_empty() && info.head.is_none());
    let e = compare(&w, "HEAD").unwrap_err();
    assert_eq!(e.status, 422);
    assert!(
        e.json().unwrap().contains("\"code\":\"git_unavailable\""),
        "{}",
        e.message
    );
}

#[test]
fn a_project_in_a_subfolder_of_the_repository_is_compared_by_its_own_files() {
    need_tools!();
    let (r, _, w) = committed("sub", "hardware/board");
    // A file of the repository outside the project changes too: it is not in the numbers.
    fs::write(r.dir.join("README"), "outside\n").unwrap();
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    let info = w.git();
    assert!(info.available, "{:?}", info.reason);
    assert_eq!(info.project_dir.as_deref(), Some("hardware/board"));
    assert!(
        info.dirty && info.project_dirty,
        "a change inside the project"
    );
    let c = compare(&w, "HEAD").unwrap();
    assert_eq!(c.summary, "1 net renamed");
    assert_eq!(
        c.source
            .files
            .iter()
            .map(|f| f.path.as_str())
            .collect::<Vec<_>>(),
        ["src/main.nsm"],
        "paths are the project's"
    );
    // A change outside the project makes the tree dirty and not the project.
    r.commit("rename, and a README");
    fs::write(r.dir.join("README"), "changed\n").unwrap();
    let info = w.git();
    assert!(info.dirty && !info.project_dirty, "{info:?}");
    assert!(!compare(&w, "HEAD").unwrap().head.dirty);
    // Against a commit before the project was in that folder, it is not there.
    fs::rename(r.dir.join("hardware"), r.dir.join("hw")).unwrap();
    r.commit("move the project");
    let w = Workspace::open(Target::resolve_from(&r.dir.join("hw/board"), None).unwrap()).unwrap();
    let c = compare(&w, "HEAD~1").unwrap();
    assert!(
        c.error
            .as_deref()
            .unwrap()
            .contains("folder `hw/board` does not exist at"),
        "{:?}",
        c.error
    );
}

#[test]
fn uncommitted_changes_show_against_head_with_the_files_that_are_not_in_git_yet() {
    need_tools!();
    let (r, _, w) = committed("dirty", "");
    assert!(!w.git().dirty);
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    fs::write(r.project.join("src/extra.nsm"), "//! Not committed.\n").unwrap();
    let info = w.git();
    assert!(info.dirty && info.project_dirty);
    let c = compare(&r.workspace(), "HEAD").unwrap();
    assert!(c.head.dirty);
    assert_eq!(c.summary, "1 net renamed");
    let files: Vec<(&str, SourceStatus)> = c
        .source
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.status))
        .collect();
    assert_eq!(
        files,
        [
            ("src/extra.nsm", SourceStatus::Untracked),
            ("src/main.nsm", SourceStatus::Modified)
        ]
    );
    assert_eq!(c.source.files_changed, 2);
    assert_eq!((c.source.added, c.source.removed), (2, 1));
    // Committed, the same change is against the earlier commit, and HEAD has nothing to show.
    r.commit("rename");
    let w = r.workspace();
    assert!(!compare(&w, "HEAD").unwrap().changed);
    let c = compare(&w, "HEAD~1").unwrap();
    assert_eq!(c.summary, "1 net renamed");
    assert!(!c.head.dirty);
}

#[test]
fn a_second_compare_of_a_ref_is_served_from_the_cache_by_commit_hash() {
    need_tools!();
    let (r, first, w) = committed("cache", "");
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    r.commit("rename");
    let events: Arc<Mutex<Vec<(String, String)>>> = Arc::default();
    let sink = Arc::clone(&events);
    let observe: Observer = Arc::new(move |e| {
        if let AnalysisEvent::Progress { stage, message, .. } = e {
            sink.lock().unwrap().push((stage, message));
        }
    });
    let cold = compare_with(&w, "HEAD~1", None, &observe).unwrap();
    assert!(!cold.cached);
    let stages = |events: &Arc<Mutex<Vec<(String, String)>>>| -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for (stage, _) in events.lock().unwrap().iter() {
            if seen.last() != Some(stage) {
                seen.push(stage.clone());
            }
        }
        seen
    };
    assert_eq!(
        stages(&events),
        [
            "extract", "check", "solve", "design", "draw", "modules", "diff"
        ]
    );
    assert!(events.lock().unwrap()[0].1.contains("out of git"));
    events.lock().unwrap().clear();
    // By the commit's hash, not by the name that was asked with.
    let warm = compare_with(&w, &first, None, &observe).unwrap();
    assert!(warm.cached);
    assert_eq!(stages(&events), ["diff"]);
    assert_eq!(warm.boards, cold.boards);
    assert_eq!(warm.summary, cold.summary);
    assert!(
        warm.elapsed_ms < cold.elapsed_ms,
        "{} < {}",
        warm.elapsed_ms,
        cold.elapsed_ms
    );
    // Another ref is another reading.
    assert!(!compare_with(&w, "HEAD", None, &silent()).unwrap().cached);
}

#[test]
fn comparing_changes_nothing_in_the_work_tree_or_the_index() {
    need_tools!();
    let (r, _, w) = committed("pure", "");
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    fs::write(r.project.join("scratch.txt"), "untracked\n").unwrap();
    let before = (
        git(&r.dir, &["status", "--porcelain"]),
        fs::read(r.dir.join(".git/index")).unwrap(),
        fs::read_to_string(r.project.join("src/main.nsm")).unwrap(),
        git(&r.dir, &["stash", "list"]),
        git(&r.dir, &["rev-parse", "HEAD"]),
    );
    compare(&w, "HEAD").unwrap();
    w.git();
    let after = (
        git(&r.dir, &["status", "--porcelain"]),
        fs::read(r.dir.join(".git/index")).unwrap(),
        fs::read_to_string(r.project.join("src/main.nsm")).unwrap(),
        git(&r.dir, &["stash", "list"]),
        git(&r.dir, &["rev-parse", "HEAD"]),
    );
    assert_eq!(before, after);
}

#[test]
fn the_git_info_lists_the_refs_to_compare_with_and_the_last_commits() {
    need_tools!();
    let (r, first, w) = committed("info", "");
    git(&r.dir, &["tag", "v1"]);
    git(&r.dir, &["tag", "-a", "v2", "-m", "second tag"]);
    git(&r.dir, &["checkout", "-q", "-b", "feature"]);
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    let second = r.commit("rename the rail");
    let info = w.git();
    assert!(info.available && info.reason.is_none(), "{:?}", info.reason);
    assert_eq!(info.branch.as_deref(), Some("feature"));
    assert_eq!(info.project_dir.as_deref(), Some(""));
    let head = info.head.as_ref().unwrap();
    assert_eq!(
        (head.hash.as_str(), head.subject.as_str()),
        (second.as_str(), "rename the rail")
    );
    assert_eq!(head.author, "Test Author");
    assert!(head.date.contains('T'), "{}", head.date);
    assert!(!info.dirty && !info.project_dirty);
    let refs: Vec<(&str, GitRefKind, bool)> = info
        .refs
        .iter()
        .map(|r| (r.name.as_str(), r.kind, r.current))
        .collect();
    assert_eq!(refs[0], ("HEAD", GitRefKind::Head, false));
    assert!(
        refs.contains(&("main", GitRefKind::Default, false)),
        "{refs:?}"
    );
    assert!(
        refs.contains(&("feature", GitRefKind::Branch, true)),
        "{refs:?}"
    );
    // `main` is the default branch and is listed once, as that.
    assert_eq!(refs.iter().filter(|r| r.0 == "main").count(), 1);
    // Tags point at the commit, an annotated one too.
    let tag = |name: &str| info.refs.iter().find(|r| r.name == name).unwrap();
    assert_eq!(tag("v1").kind, GitRefKind::Tag);
    assert_eq!(tag("v1").hash, first);
    assert_eq!(tag("v2").hash, first);
    assert_eq!(
        info.commits
            .iter()
            .map(|c| c.subject.as_str())
            .collect::<Vec<_>>(),
        ["rename the rail", "first"]
    );
    // Every ref can be compared with.
    for name in ["HEAD", "main", "v1", "v2"] {
        let c = compare(&w, name).unwrap();
        assert_eq!(c.error, None, "{name}");
    }
}

#[test]
fn a_file_of_the_project_can_be_read_as_it_was_at_a_ref_and_nothing_else() {
    need_tools!();
    let (r, first, w) = committed("show", "hardware");
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    let was = w.git_file("HEAD", "src/main.nsm").unwrap();
    assert_eq!(was.text, TWO_BOARDS);
    assert!(was.readonly);
    assert_eq!(was.stamp, first);
    assert_eq!(was.path, "src/main.nsm");
    assert_eq!(
        w.git_file("HEAD", "netsmith.toml").unwrap().path,
        "netsmith.toml"
    );
    assert_eq!(w.git_file("HEAD", "src/none.nsm").unwrap_err().status, 404);
    assert_eq!(w.git_file("nope", "src/main.nsm").unwrap_err().status, 404);
    for path in [
        "../README",
        "/etc/passwd",
        "src/../../README",
        "src\\main.nsm",
        "",
        "bin/tool",
    ] {
        assert_eq!(w.git_file("HEAD", path).unwrap_err().status, 403, "{path}");
    }
}

#[test]
fn one_board_can_be_asked_for_and_an_unknown_one_is_404() {
    need_tools!();
    let (r, _, w) = committed("one", "");
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    let _one = lock();
    let c = w
        .compare(
            "HEAD",
            Some("TwoBoards/led"),
            &silent(),
            &netsmith_editor::Cancel::new(),
        )
        .unwrap();
    assert_eq!(c.boards.len(), 1);
    assert_eq!(c.boards[0].name, "TwoBoards/led");
    assert_eq!(c.summary, "No circuit changes");
    assert_eq!(
        w.compare(
            "HEAD",
            Some("Nope"),
            &silent(),
            &netsmith_editor::Cancel::new()
        )
        .unwrap_err()
        .status,
        404
    );
}

#[test]
fn boards_that_exist_on_one_side_only_are_added_or_removed_boards() {
    need_tools!();
    let (r, _, w) = committed("boards", "");
    let extra = "\nboard Extra {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .j: Header1x2\n}\n";
    r.edit(
        "src/main.nsm",
        "system TwoBoards",
        &format!("{extra}\nsystem TwoBoards"),
    );
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let names: Vec<(&str, BoardStatus)> = c
        .boards
        .iter()
        .map(|b| (b.name.as_str(), b.status))
        .collect();
    assert!(names.contains(&("Extra", BoardStatus::Added)), "{names:?}");
    let added = board(&c, "Extra");
    assert_eq!(added.parts.added.len(), 1);
    assert!(added.base_file.is_none() && added.head_file.as_deref() == Some("src/main.nsm"));
    assert!(c.summary.starts_with("board Extra added"), "{}", c.summary);
    // The other way round, from the commit that has it.
    r.commit("an extra board");
    r.edit("src/main.nsm", extra, "");
    let c = compare(&w, "HEAD").unwrap();
    let removed = board(&c, "Extra");
    assert_eq!(removed.status, BoardStatus::Removed);
    assert!(removed.head_file.is_none() && removed.base_file.is_some());
    assert!(
        c.summary.starts_with("board Extra removed"),
        "{}",
        c.summary
    );
}

#[test]
fn a_file_that_does_not_build_on_disk_keeps_its_last_boards_marked_stale() {
    need_tools!();
    let (r, _, _) = committed("stale", "");
    let w = r.workspace();
    r.edit("src/main.nsm", ".r1: Resistor {}", ".r1: Resistr {}");
    // Read again by the workspace: it keeps the boards of its last good reading.
    assert_eq!(w.analysis().errors, 1);
    let c = compare(&w, "HEAD").unwrap();
    assert_eq!(c.head.errors, 1);
    assert!(board(&c, "TwoBoards/led").stale);
    // The mistake is new, at the line on disk, and is the only thing that differs: the boards
    // are not said to be gone, and the last good reading has nothing else changed.
    let led = board(&c, "TwoBoards/led");
    assert_eq!(led.problems.new.len(), 1, "{:#?}", led.problems);
    let error = &led.problems.new[0];
    assert!(error.message.contains("Resistr") && error.at.as_ref().unwrap().line == 91);
    assert!(
        led.parts.changed.is_empty() && led.parts.added.is_empty() && led.parts.removed.is_empty()
    );
    assert_eq!(board(&c, "TwoBoards/power").status, BoardStatus::Unchanged);
    assert!(c.boards.iter().all(|b| b.status != BoardStatus::Removed));
    assert!(c.summary.starts_with("1 error on disk: "), "{}", c.summary);
}

#[test]
fn files_that_never_built_on_disk_say_so_in_the_summary() {
    need_tools!();
    let (r, _, _) = committed("neverbuilt", "");
    r.edit("src/main.nsm", ".r1: Resistor {}", ".r1: Resistr {}");
    let c = compare(&r.workspace(), "HEAD").unwrap();
    assert_eq!(c.head.errors, 1);
    assert!(c.summary.starts_with("1 error on disk: "), "{}", c.summary);
    assert!(c.boards.iter().all(|b| b.status == BoardStatus::Removed));
}

#[test]
fn a_single_file_with_no_project_is_compared_by_that_file() {
    need_tools!();
    let dir = TempDir::new("compare-file");
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.name", "Test Author"]);
    git(&dir, &["config", "user.email", "test@example.com"]);
    git(&dir, &["config", "commit.gpgsign", "false"]);
    let led = include_str!("../../../examples/led.nsm");
    let file = dir.join("led.nsm");
    fs::write(&file, led).unwrap();
    fs::write(dir.join("notes.txt"), "not a source\n").unwrap();
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "the led"]);
    let w = Workspace::open(Target::resolve_from(&dir, Some(file.to_str().unwrap())).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(w.git().available);
    let same = compare(&w, "HEAD").unwrap();
    assert_eq!(same.error, None);
    assert_eq!(same.summary, "No circuit changes", "{:#?}", same.boards);
    assert!(!same.boards.is_empty());
    // A change to the file shows, as a change to its board.
    let changed = led.replacen("Resistor {", "Resistor { .resistance &= 220ohm +/- 1%", 1);
    assert_ne!(changed, led, "the example has a resistor to change");
    fs::write(&file, &changed).unwrap();
    let c = compare(&w, "HEAD").unwrap();
    assert_eq!(c.error, None, "{c:#?}");
    assert_eq!(c.source.files_changed, 1);
    // The resistor and, with the new current, the LED the solver picks.
    assert_eq!(
        c.summary, "2 values changed, 1 rail changed",
        "{:#?}",
        c.boards
    );
    let part = c.boards[0]
        .parts
        .changed
        .iter()
        .find(|p| p.path == "r1")
        .unwrap();
    assert_eq!(part.fields[0].new.as_deref(), Some("220"));
}

// ---- the whole server ----------------------------------------------------------------------

fn http(port: u16, target: &str) -> (u16, String) {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(120)))
        .unwrap();
    write!(
        s,
        "GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap();
    (
        head.split(' ').nth(1).unwrap().parse().unwrap(),
        body.to_string(),
    )
}

#[test]
fn the_server_answers_git_and_compare_and_tells_the_socket_how_far_it_is() {
    need_tools!();
    let _one = lock();
    let (r, _, w) = committed("served", "hw");
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || netsmith_editor::serve(listener, Arc::new(w)));

    let (status, body) = http(port, "/api/git");
    assert_eq!(status, 200, "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        (v["available"].as_bool(), v["projectDir"].as_str()),
        (Some(true), Some("hw"))
    );
    assert_eq!(v["branch"], "main");
    assert_eq!(v["dirty"], true);
    assert_eq!(v["refs"][0]["name"], "HEAD");

    let stream = std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(120)))
        .unwrap();
    let (mut socket, _) =
        tungstenite::client(format!("ws://127.0.0.1:{port}/api/events").as_str(), stream).unwrap();
    let asked = std::thread::spawn(move || http(port, "/api/compare?ref=HEAD"));
    let mut stages: Vec<String> = Vec::new();
    loop {
        let tungstenite::Message::Text(text) = socket.read().expect("a message") else {
            continue;
        };
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        if v["type"] == "progress" && v["task"] == "compare" {
            assert_eq!(v["board"], "HEAD");
            let stage = v["stage"].as_str().unwrap().to_string();
            if stages.last() != Some(&stage) {
                stages.push(stage.clone());
            }
            if stage == "diff" {
                break;
            }
        }
    }
    assert_eq!(
        stages,
        [
            "extract", "check", "solve", "design", "draw", "modules", "diff"
        ]
    );
    let (status, body) = asked.join().unwrap();
    assert_eq!(status, 200, "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["ref"], "HEAD");
    assert_eq!(v["summary"], "1 net renamed");
    assert_eq!(v["source"]["files"][0]["path"], "src/main.nsm");
    let (status, body) = http(port, "/api/compare?ref=nothing-here");
    assert_eq!(status, 404, "{body}");
    // The other side a ref too: `against` is read as `ref` is.
    let (status, body) = http(port, "/api/compare?ref=HEAD&against=HEAD");
    assert_eq!(status, 200, "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["against"], "HEAD");
    assert_eq!(v["summary"], "No circuit changes");
    assert_eq!(v["head"]["dirty"], false);
    let (status, body) = http(port, "/api/compare?ref=HEAD&against=nothing-here");
    assert_eq!(status, 404, "{body}");
    let (status, body) = http(port, "/api/git/file?ref=HEAD&path=src%2Fmain.nsm");
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("TwoBoards"));
}

/// A `main` that is the checked out branch is `current`, listed once, as the default branch.
#[test]
fn the_branch_that_is_checked_out_is_current_whatever_kind_it_is_listed_as() {
    need_tools!();
    let (r, _, w) = committed("current", "");
    let info = w.git();
    assert_eq!(info.branch.as_deref(), Some("main"));
    let main: Vec<_> = info.refs.iter().filter(|r| r.name == "main").collect();
    assert_eq!(main.len(), 1, "{:?}", info.refs);
    assert!(main[0].current, "{:?}", main[0]);
    assert!(
        info.refs
            .iter()
            .filter(|r| r.kind == GitRefKind::Head)
            .all(|r| !r.current)
    );
    // On another branch, `main` is not current and the branch is.
    git(&r.dir, &["checkout", "-q", "-b", "topic"]);
    let info = r.workspace().git();
    let current: Vec<_> = info
        .refs
        .iter()
        .filter(|r| r.current)
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(current, ["topic"], "{:?}", info.refs);
}

/// A repository with a hostile branch: `src/main.nsm` is a link to a file outside the
/// repository, there is a link to a folder, and a file bigger than a ref's files may be.
#[cfg(unix)]
fn hostile(label: &str) -> (Repo, TempDir, Workspace) {
    use std::os::unix::fs::symlink;
    let (r, _, w) = committed(label, "");
    let outside = TempDir::new(&format!("compare-{label}-outside"));
    fs::write(outside.join("secret.nsm"), "OUTSIDE-SECRET-12345\n").unwrap();
    fs::create_dir_all(outside.join("folder")).unwrap();
    fs::write(outside.join("folder/inner.nsm"), "OUTSIDE-INNER-67890\n").unwrap();
    git(&r.dir, &["checkout", "-q", "-b", "hostile"]);
    fs::remove_file(r.project.join("src/main.nsm")).unwrap();
    symlink(outside.join("secret.nsm"), r.project.join("src/main.nsm")).unwrap();
    symlink(&*outside.join("folder"), r.project.join("src/linked")).unwrap();
    // The link `netsmith init` writes for the agents' skills: removed like the others, but nothing the analysis would miss.
    fs::create_dir_all(r.project.join(".claude")).unwrap();
    symlink(outside.join("folder"), r.project.join(".claude/skills")).unwrap();
    r.commit("hostile");
    git(&r.dir, &["checkout", "-q", "main"]);
    (r, outside, w)
}

#[cfg(unix)]
#[test]
fn a_link_in_a_ref_is_never_followed_and_the_answer_says_it_was_left_out() {
    need_tools!();
    let (_r, outside, w) = hostile("links");
    let c = compare(&w, "hostile").unwrap();
    let wire = serde_json::to_string(&c).unwrap();
    for secret in ["OUTSIDE-SECRET-12345", "OUTSIDE-INNER-67890"] {
        assert!(!wire.contains(secret), "{secret} is in the answer: {wire}");
    }
    // The top-level file is a link, so it is not there: the ref cannot be read, and why is said.
    let error = c.error.expect("the ref cannot be read");
    assert!(
        error.contains("symbolic links, which are never followed")
            && error.contains("`src/main.nsm`"),
        "{error}"
    );
    // What the links pointed at is untouched.
    assert_eq!(
        fs::read_to_string(outside.join("secret.nsm")).unwrap(),
        "OUTSIDE-SECRET-12345\n"
    );
}

#[cfg(unix)]
#[test]
fn the_note_of_a_ref_that_cannot_be_read_names_the_links_that_matter_and_no_scratch_folder() {
    need_tools!();
    let (_r, _outside, w) = hostile("wording");
    let c = compare(&w, "hostile").unwrap();
    let error = c.error.expect("the ref cannot be read");
    assert!(
        error.contains("`src/main.nsm`") && error.contains("`src/linked`"),
        "{error}"
    );
    assert!(
        !error.contains(".claude") && !error.contains("skills"),
        "a tool's link is in the note: {error}"
    );
    // Project-relative paths only: nothing of this machine's scratch folder or its temporary folder.
    let temp = std::env::temp_dir();
    assert!(
        !error.contains("netsmith-compare-") && !error.contains(&*temp.to_string_lossy()),
        "the scratch folder is named: {error}"
    );
}

#[cfg(unix)]
#[test]
fn a_file_of_a_ref_over_the_cap_is_not_read_and_the_answer_says_so() {
    need_tools!();
    let (r, _, w) = committed("big", "");
    git(&r.dir, &["checkout", "-q", "-b", "big"]);
    // A source of 9 MB: a valid file with a long comment after it.
    let mut text = fs::read_to_string(r.project.join("src/main.nsm")).unwrap();
    text.push_str("\n//");
    text.push_str(&"x".repeat(9 * 1024 * 1024));
    text.push('\n');
    fs::write(r.project.join("src/main.nsm"), text).unwrap();
    r.commit("big");
    git(&r.dir, &["checkout", "-q", "main"]);
    let c = compare(&w, "big").unwrap();
    let error = c.error.expect("the ref cannot be read");
    assert!(
        error.contains("over 8 MB") && error.contains("src/main.nsm"),
        "{error}"
    );
}

#[test]
fn what_the_tools_write_is_not_read_out_of_a_ref_nor_counted_among_the_new_files() {
    need_tools!();
    let (r, _, _) = committed("ignored", "");
    // `out/` and `node_modules/` are committed at the ref (somebody added them by mistake).
    fs::create_dir_all(r.project.join("out")).unwrap();
    fs::write(r.project.join("out/huge.txt"), "x\n".repeat(1000)).unwrap();
    fs::create_dir_all(r.project.join("web/node_modules/pkg")).unwrap();
    fs::write(r.project.join("web/node_modules/pkg/index.js"), "x").unwrap();
    git(&r.dir, &["add", "-f", "out", "web"]);
    r.commit("with outputs");
    let w = r.workspace();
    let c = compare(&w, "HEAD").unwrap();
    assert_eq!(c.error, None);
    // Untracked and ignored: not in the source summary, whatever its size.
    fs::write(r.project.join(".gitignore"), "ignored/\n").unwrap();
    fs::create_dir_all(r.project.join("ignored")).unwrap();
    fs::write(r.project.join("ignored/big.txt"), "line\n".repeat(200_000)).unwrap();
    fs::write(r.project.join("src/new.nsm"), "//! New.\n").unwrap();
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let paths: Vec<&str> = c.source.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, [".gitignore", "src/new.nsm"]);
    let new = c
        .source
        .files
        .iter()
        .find(|f| f.path == "src/new.nsm")
        .unwrap();
    assert_eq!(new.added, 1);
    let ignore = c
        .source
        .files
        .iter()
        .find(|f| f.path == ".gitignore")
        .unwrap();
    assert_eq!(ignore.added, 1);
}

#[test]
fn a_compare_that_is_cancelled_leaves_no_scratch_folder_no_reading_and_the_live_project_as_it_was()
{
    if !tools_here() {
        return;
    }
    let repo = Repo::new("cancelled", "");
    let first = repo.commit("first");
    repo.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    let w = repo.workspace();
    let before = w.analysis();
    assert_eq!(before.cancelled, None);

    // Cancelled from inside, when the analysis of the ref begins to solve: the token is the
    // compare's own, and the live project's analysis does not hear of it.
    let _one = lock();
    let token = netsmith_editor::Cancel::new();
    let observe: Observer = {
        let token = token.clone();
        Arc::new(move |event| {
            if let AnalysisEvent::Progress { stage, .. } = &event
                && stage == "solve"
            {
                token.cancel();
            }
        })
    };
    let result = w.compare(&first, None, &observe, &token);
    let error = result.expect_err("cancelled");
    assert!(error.is_cancelled(), "{error:?}");
    assert_eq!(error.status, 499);
    assert_eq!(
        scratch_folders(),
        Vec::<String>::new(),
        "the scratch folder went with the compare"
    );
    // The live project: the same reading, not cancelled, nothing began again.
    let after = w.analysis();
    assert_eq!(after, before);

    // Nothing was kept of the ref: it is read again from the start, and is then kept.
    let again = w
        .compare(&first, None, &silent(), &netsmith_editor::Cancel::new())
        .unwrap();
    assert!(!again.cached, "the cancelled reading was not kept");
    assert!(again.changed);
    let third = w
        .compare(&first, None, &silent(), &netsmith_editor::Cancel::new())
        .unwrap();
    assert!(third.cached);

    // A token cancelled before it starts ends it before anything is read.
    let early = netsmith_editor::Cancel::new();
    early.cancel();
    let error = w
        .compare("HEAD", None, &silent(), &early)
        .expect_err("cancelled");
    assert!(error.is_cancelled());
    assert_eq!(scratch_folders(), Vec::<String>::new());
}

#[test]
fn a_compare_is_refused_while_the_analysis_of_the_files_on_disk_is_cancelled_and_works_after_it_runs_again()
 {
    if !tools_here() {
        return;
    }
    let repo = Repo::new("cancelled-head", "");
    repo.commit("first");
    let w = Arc::new(
        Workspace::open_deferred(Target::resolve_from(&repo.project, None).unwrap())
            .unwrap_or_else(|e| panic!("{e}")),
    );
    // The first analysis is cancelled, once, when it begins to solve.
    let asked = std::sync::atomic::AtomicBool::new(false);
    let weak = Arc::downgrade(&w);
    w.observe(Arc::new(move |event| {
        if let AnalysisEvent::Progress { stage, .. } = &event
            && stage == "solve"
            && !asked.swap(true, std::sync::atomic::Ordering::SeqCst)
            && let Some(w) = weak.upgrade()
        {
            assert!(w.cancel_analysis());
        }
    }));
    assert_eq!(w.analysis().cancelled, Some(true));
    let _one = lock();
    let refused = w
        .compare("HEAD", None, &silent(), &netsmith_editor::Cancel::new())
        .expect_err("there is no reading of the files to compare");
    assert_eq!(refused.status, 409, "{refused:?}");
    assert!(
        refused.message.contains("run the analysis again"),
        "{}",
        refused.message
    );
    assert_eq!(scratch_folders(), Vec::<String>::new());
    // Run again: the analysis is whole, and the compare is too.
    assert_eq!(w.run_analysis().cancelled, None);
    let c = w
        .compare("HEAD", None, &silent(), &netsmith_editor::Cancel::new())
        .unwrap();
    assert_eq!(c.error, None);
}

// ---- the extensions of ADR 0059: ref against ref ----

/// `rev` against the commit `against`, as a request for two refs is made.
fn compare_refs(w: &Workspace, rev: &str, against: &str) -> Result<Compare, ApiError> {
    let _one = lock();
    let result = w.compare_against(
        rev,
        Some(against),
        None,
        &silent(),
        &netsmith_editor::Cancel::new(),
    );
    assert_eq!(
        scratch_folders(),
        Vec::<String>::new(),
        "a scratch folder was left"
    );
    result
}

/// Three commits (the design, a renamed net, a resistor of another value) and a work tree that
/// has a change of its own, which no comparison of two refs may see.
fn three_commits(label: &str) -> (Repo, [String; 3], Workspace) {
    let r = Repo::new(label, "");
    let first = r.commit("first");
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    let second = r.commit("rename the output net");
    r.edit(
        "src/main.nsm",
        ".r1: Resistor {}",
        ".r1: Resistor { .resistance &= 150ohm +/- 1% }",
    );
    let third = r.commit("a bigger resistor");
    // Uncommitted: the files on disk differ from every commit.
    r.edit("src/main.nsm", INPUT_CAP, &format!("{BULK}{INPUT_CAP}"));
    fs::write(r.project.join("NOTES.md"), "not in git\n").unwrap();
    let w = r.workspace();
    (r, [first, second, third], w)
}

#[test]
fn two_refs_are_compared_with_each_other_and_the_files_on_disk_are_not_read() {
    need_tools!();
    let (_r, [first, second, third], w) = three_commits("refs");
    let c = compare_refs(&w, &first, &second).unwrap();
    assert_eq!(c.error, None);
    assert_eq!(c.rev, first);
    assert_eq!(c.against.as_deref(), Some(second.as_str()));
    assert_eq!(c.base.hash, first);
    // The head side is the other commit: not dirty, no errors, and its hash.
    assert_eq!(c.head.commit.as_ref().unwrap().hash, second);
    assert!(!c.head.dirty && c.head.errors == 0);
    assert_eq!(c.summary, "1 net renamed");
    // The uncommitted capacitor and the untracked file are not in it.
    let power = board(&c, "TwoBoards/power");
    assert!(power.parts.added.is_empty() && power.parts.removed.is_empty());
    assert_eq!(
        (c.source.files_changed, c.source.added, c.source.removed),
        (1, 1, 1)
    );
    assert_eq!(c.source.files[0].path, "src/main.nsm");
    assert!(
        c.source
            .files
            .iter()
            .all(|f| f.status != SourceStatus::Untracked)
    );

    // Later refs: the resistor is the change between them.
    let c = compare_refs(&w, &second, &third).unwrap();
    assert_eq!(
        c.summary,
        "2 values changed, 1 system port changed, 1 rail changed"
    );
    let led = board(&c, "TwoBoards/led");
    let r1 = led.parts.changed.iter().find(|p| p.path == "r1").unwrap();
    // Each side's line is in that side's own commit.
    assert_eq!(r1.at.as_ref().unwrap().file, "src/main.nsm");
    assert!(r1.base_at.is_some());

    // The files on disk against the same first commit say more: the capacitor and the notes.
    let on_disk = compare(&w, &first).unwrap();
    assert_eq!(on_disk.against, None);
    assert!(on_disk.head.dirty);
    assert!(
        on_disk.summary.contains("1 part added"),
        "{}",
        on_disk.summary
    );
    assert!(
        on_disk
            .source
            .files
            .iter()
            .any(|f| f.status == SourceStatus::Untracked)
    );
}

#[test]
fn the_same_ref_on_both_sides_and_the_two_directions_of_a_pair() {
    need_tools!();
    let (_r, [first, second, _], w) = three_commits("refs-pair");
    let same = compare_refs(&w, &second, &second).unwrap();
    assert!(!same.changed);
    assert_eq!(same.summary, "No circuit changes");
    assert_eq!(same.source.files_changed, 0);
    // A name and the hash of the same commit are the same ref.
    let by_name = compare_refs(&w, "HEAD~2", &first).unwrap();
    assert!(!by_name.changed, "{}", by_name.summary);

    let forward = compare_refs(&w, &first, &second).unwrap();
    let back = compare_refs(&w, &second, &first).unwrap();
    let net = |c: &Compare| {
        let n = &board(c, "TwoBoards/power").nets;
        let ren = n.renamed.first().expect("a renamed net");
        (ren.base_name.clone(), ren.name.clone())
    };
    assert_eq!(net(&forward), ("3V3".to_string(), "VOUT".to_string()));
    assert_eq!(net(&back), ("VOUT".to_string(), "3V3".to_string()));
}

#[test]
fn a_second_ref_that_is_no_ref_is_refused_before_anything_is_read() {
    need_tools!();
    let (_r, [first, ..], w) = three_commits("refs-bad");
    let missing = compare_refs(&w, &first, "nothing-here").unwrap_err();
    assert_eq!(missing.status, 404, "{missing:?}");
    let hostile = compare_refs(&w, &first, "--output=x").unwrap_err();
    assert_eq!(hostile.status, 400, "{hostile:?}");
    // Nothing was read of the first ref either: asking again is not served from the cache.
    assert!(!compare_refs(&w, &first, "HEAD").unwrap().cached);
}

#[test]
fn a_second_ref_that_does_not_analyse_is_an_answer_naming_that_commit() {
    need_tools!();
    let (r, [first, ..], w) = three_commits("refs-broken");
    r.edit(
        "src/main.nsm",
        "Resistor { .resistance",
        "Resistr { .resistance",
    );
    let broken = r.commit("a typo");
    let c = compare_refs(&w, &first, &broken).unwrap();
    let error = c.error.as_deref().expect("the second ref does not analyse");
    assert!(error.contains("does not analyse at"), "{error}");
    assert!(error.contains(&broken[..7]), "{error}");
    assert!(error.contains("Resistr"), "{error}");
    assert!(!c.base_problems.is_empty());
    assert!(c.boards.is_empty());
    assert_eq!(c.head.commit.as_ref().unwrap().hash, broken);
    assert!(!c.head.dirty);
    // The broken one as the first ref says so too, and the second is not read.
    let c = compare_refs(&w, &broken, &first).unwrap();
    assert!(c.error.as_deref().unwrap().contains(&broken[..7]));
}

#[test]
fn a_comparison_of_two_refs_is_cached_by_each_commit() {
    need_tools!();
    let (_r, [first, second, third], w) = three_commits("refs-cache");
    assert!(!compare_refs(&w, &first, &second).unwrap().cached);
    assert!(compare_refs(&w, &first, &second).unwrap().cached);
    // The second is kept: against a new one it is read once, and the pair is only half cached.
    assert!(!compare_refs(&w, &second, &third).unwrap().cached);
    assert!(compare_refs(&w, &third, &first).unwrap().cached);
}

#[test]
fn a_comparison_of_two_refs_that_is_cancelled_leaves_nothing_behind() {
    if !tools_here() {
        return;
    }
    let (r, [first, second, _], w) = three_commits("refs-cancel");
    let before = (
        git(&r.dir, &["status", "--porcelain"]),
        fs::read_to_string(r.project.join("src/main.nsm")).unwrap(),
    );
    let _one = lock();
    // Cancelled when the second ref begins to solve: the first ref's reading is whole and kept.
    let token = netsmith_editor::Cancel::new();
    let solves = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observe: Observer = {
        let (token, solves) = (token.clone(), Arc::clone(&solves));
        Arc::new(move |event| {
            if let AnalysisEvent::Progress { stage, .. } = &event
                && stage == "solve"
                && solves.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1
            {
                token.cancel();
            }
        })
    };
    let error = w
        .compare_against(&first, Some(&second), None, &observe, &token)
        .expect_err("cancelled");
    assert!(error.is_cancelled(), "{error:?}");
    assert_eq!(scratch_folders(), Vec::<String>::new());
    // Nothing of the second was kept: it is read again; the first was.
    let again = w
        .compare_against(
            &first,
            Some(&second),
            None,
            &silent(),
            &netsmith_editor::Cancel::new(),
        )
        .unwrap();
    assert!(!again.cached);
    assert_eq!(again.summary, "1 net renamed");
    // A token cancelled before it starts reads nothing.
    let early = netsmith_editor::Cancel::new();
    early.cancel();
    assert!(
        w.compare_against("HEAD", Some("HEAD~1"), None, &silent(), &early)
            .expect_err("cancelled")
            .is_cancelled()
    );
    assert_eq!(scratch_folders(), Vec::<String>::new());
    // The work tree is as it was.
    let after = (
        git(&r.dir, &["status", "--porcelain"]),
        fs::read_to_string(r.project.join("src/main.nsm")).unwrap(),
    );
    assert_eq!(before, after);
}

// ---- the extensions of ADR 0059: systems and the PCB ----

fn system<'a>(c: &'a Compare, name: &str) -> &'a netsmith_editor::api::CompareSystem {
    c.systems
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no system {name} in {:?}",
                c.systems.iter().map(|s| &s.name).collect::<Vec<_>>()
            )
        })
}

/// The layout file of the two boards: the resistor of the LED board and the output capacitor of
/// the power board put by hand, and nothing else (the placer chooses the rest).
fn layout(r1: &str, extra: &str) -> String {
    format!(
        "version = 1\n\n[LedBoard.parts.\"r1\"]\nat = [{r1}]\nrotation = 0.0\nside = \"top\"\nsource = \"hand\"\n{extra}\n[PowerBoard.parts.\"stage.output_cap\"]\nat = [10.0, 4.0]\nrotation = 0.0\nside = \"top\"\nsource = \"hand\"\n"
    )
}

#[test]
fn a_system_whose_port_changed_says_what_it_was_and_what_it_is() {
    need_tools!();
    let (r, _, _) = committed("system-port", "");
    r.edit(
        "src/main.nsm",
        ".current &= 0mA..50mA",
        ".current &= 0mA..30mA",
    );
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let s = system(&c, "TwoBoards");
    assert_eq!(s.status, BoardStatus::Changed);
    let port = s
        .ports
        .changed
        .iter()
        .find(|p| (p.board.as_str(), p.name.as_str()) == ("power", "output"))
        .unwrap_or_else(|| panic!("{:#?}", s.ports));
    let current = port.fields.iter().find(|f| f.field == "current").unwrap();
    assert!(
        current.old.as_deref().unwrap().contains("50"),
        "{current:?}"
    );
    assert!(
        current.new.as_deref().unwrap().contains("30"),
        "{current:?}"
    );
    assert!(
        s.boards.added.is_empty() && s.joins.added.is_empty() && s.mates.changed.is_empty(),
        "{s:#?}"
    );
    assert!(c.summary.contains("system port"), "{}", c.summary);
    assert!(c.changed);
    // The board's own rail says it too, and the system does not say it again.
    assert!(
        s.rails.changed.iter().all(|r| !r.net.starts_with("power:")),
        "{:#?}",
        s.rails
    );
    // A board asked for alone has no systems in its answer.
    let one = compare_with(&r.workspace(), "HEAD", Some("TwoBoards/power"), &silent()).unwrap();
    assert!(one.systems.is_empty());
}

#[test]
fn a_renamed_net_is_not_a_mate_or_a_system_rail_that_changed() {
    need_tools!();
    let (r, _, _) = committed("system-rename", "");
    r.edit("src/main.nsm", "\"3V3\"", "\"VOUT\"");
    let c = compare(&r.workspace(), "HEAD").unwrap();
    assert_eq!(c.summary, "1 net renamed");
    let s = system(&c, "TwoBoards");
    assert_eq!(s.status, BoardStatus::Unchanged, "{s:#?}");
    assert!(s.rails.added.is_empty() && s.rails.removed.is_empty() && s.rails.changed.is_empty());
}

#[test]
fn a_system_that_is_only_on_one_side_is_added_or_removed() {
    need_tools!();
    let (r, first, w) = committed("system-gone", "");
    // The second commit has no system: its boards are the system's, so they go with it.
    let text = fs::read_to_string(r.project.join("src/main.nsm")).unwrap();
    let cut = text.find("system TwoBoards").unwrap();
    fs::write(r.project.join("src/main.nsm"), &text[..cut]).unwrap();
    let second = r.commit("no system");
    let gone = compare_refs(&w, &first, &second).unwrap();
    assert_eq!(gone.error, None, "{:?}", gone.error);
    assert_eq!(system(&gone, "TwoBoards").status, BoardStatus::Removed);
    assert!(
        gone.summary.contains("system TwoBoards removed"),
        "{}",
        gone.summary
    );
    let back = compare_refs(&w, &second, &first).unwrap();
    assert_eq!(system(&back, "TwoBoards").status, BoardStatus::Added);
    assert!(
        back.summary.contains("system TwoBoards added"),
        "{}",
        back.summary
    );
    assert!(back.changed);
}

#[test]
fn a_part_moved_on_the_pcb_has_its_old_and_new_place_and_a_part_the_placer_chose_is_not_listed() {
    need_tools!();
    let r = Repo::new("pcb-moved", "");
    fs::write(r.project.join("src/main.pcb.toml"), layout("5.0, 2.0", "")).unwrap();
    let first = r.commit("a layout");
    // The resistor is 2.5 mm to the right, turned, on the other face; and a part is added, which
    // moves the parts the placer chose.
    fs::write(
        r.project.join("src/main.pcb.toml"),
        layout("7.5, 2.0", "").replace(
            "rotation = 0.0\nside = \"top\"\nsource = \"hand\"\n\n[PowerBoard",
            "rotation = 90.0\nside = \"bottom\"\nsource = \"hand\"\n\n[PowerBoard",
        ),
    )
    .unwrap();
    r.edit("src/main.nsm", INPUT_CAP, &format!("{BULK}{INPUT_CAP}"));
    let w = r.workspace();
    let c = compare(&w, &first).unwrap();
    let led = board(&c, "TwoBoards/led");
    assert_eq!(led.pcb_error, None);
    let pcb = led.pcb.as_ref().expect("a PCB comparison");
    assert_eq!(pcb.layout_file, "src/main.pcb.toml");
    assert_eq!(pcb.moved.len(), 1, "{:#?}", pcb.moved);
    let m = &pcb.moved[0];
    assert_eq!(m.path, "r1");
    assert_eq!((m.base.x, m.base.y), (5.0, 2.0));
    assert_eq!((m.head.x, m.head.y), (7.5, 2.0));
    assert!((m.distance_mm - 2.5).abs() < 1e-9);
    let fields: Vec<(&str, &str, &str)> = m
        .fields
        .iter()
        .map(|f| {
            (
                f.field.as_str(),
                f.old.as_deref().unwrap(),
                f.new.as_deref().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        fields,
        [
            ("position", "5, 2 mm", "7.5, 2 mm"),
            ("rotation", "0°", "90°"),
            ("side", "top", "bottom"),
        ]
    );
    assert_eq!(
        (m.base.placement.as_str(), m.head.placement.as_str()),
        ("hand", "hand")
    );
    // The LED the placer chose is where the placer put it on both sides: not a statement.
    assert!(pcb.moved.iter().all(|p| p.path != "led"));
    assert!(led.parts.changed.is_empty(), "the circuit is the same");
    assert_eq!(
        led.status,
        BoardStatus::Changed,
        "a part moved on the PCB is a change"
    );
    // The power board gained a part, which the placer-chosen parts follow: still no move listed,
    // and the outline is the parts' size, not something the design said.
    let power = board(&c, "TwoBoards/power");
    let power_pcb = power.pcb.as_ref().unwrap();
    assert!(
        power_pcb.moved.is_empty() && power_pcb.outline.is_none(),
        "{power_pcb:#?}"
    );
    assert!(
        c.summary.contains("1 part moved on the PCB"),
        "{}",
        c.summary
    );
    assert!(c.changed);
    // Against a ref with no layout file the same part is derived on that side: it is compared.
    let none = Repo::new("pcb-none", "");
    let base = none.commit("no layout");
    fs::write(
        none.project.join("src/main.pcb.toml"),
        layout("7.5, 2.0", ""),
    )
    .unwrap();
    let c = compare(&none.workspace(), &base).unwrap();
    let moved = &board(&c, "TwoBoards/led").pcb.as_ref().unwrap().moved;
    assert_eq!(moved.len(), 1, "{moved:#?}");
    assert_eq!(moved[0].base.placement, "derived");
    assert_eq!(moved[0].head.placement, "hand");
}

#[test]
fn a_layout_file_that_cannot_be_placed_says_so_instead_of_comparing_half_a_board() {
    need_tools!();
    let r = Repo::new("pcb-error", "");
    fs::write(r.project.join("src/main.pcb.toml"), layout("5.0, 2.0", "")).unwrap();
    let first = r.commit("a layout");
    // A part the board does not have: the layout does not fit the board.
    fs::write(
        r.project.join("src/main.pcb.toml"),
        layout(
            "5.0, 2.0",
            "\n[LedBoard.parts.\"nothing\"]\nat = [1.0, 1.0]\n",
        ),
    )
    .unwrap();
    let c = compare(&r.workspace(), &first).unwrap();
    let led = board(&c, "TwoBoards/led");
    assert!(led.pcb.is_none());
    let error = led
        .pcb_error
        .as_deref()
        .expect("why the PCB is not compared");
    assert!(error.contains("head side"), "{error}");
}

#[test]
fn the_pcb_and_the_systems_of_two_refs_are_read_from_the_commits() {
    need_tools!();
    let r = Repo::new("pcb-refs", "");
    fs::write(r.project.join("src/main.pcb.toml"), layout("5.0, 2.0", "")).unwrap();
    let first = r.commit("a layout");
    fs::write(r.project.join("src/main.pcb.toml"), layout("6.0, 3.0", "")).unwrap();
    let second = r.commit("the resistor moved");
    // The files on disk say something else again, which two refs do not read.
    fs::write(r.project.join("src/main.pcb.toml"), layout("9.0, 9.0", "")).unwrap();
    let w = r.workspace();
    let c = compare_refs(&w, &first, &second).unwrap();
    let moved = &board(&c, "TwoBoards/led").pcb.as_ref().unwrap().moved;
    assert_eq!(moved.len(), 1);
    assert_eq!((moved[0].head.x, moved[0].head.y), (6.0, 3.0));
    assert_eq!(c.summary, "1 part moved on the PCB");
}

#[test]
fn a_file_the_editor_does_not_open_can_be_read_from_disk_for_the_comparison_of_a_file() {
    need_tools!();
    let (r, _, w) = committed("worktree-file", "");
    fs::write(r.project.join("src/main.pcb.toml"), "version = 1\n").unwrap();
    let f = w.git_file(":worktree", "src/main.pcb.toml").unwrap();
    assert_eq!((f.text.as_str(), f.readonly), ("version = 1\n", true));
    assert_eq!(f.stamp, "worktree");
    // The same rules as for a ref: the project's own text files, inside it.
    assert_eq!(
        w.git_file(":worktree", "src/main.rs").unwrap_err().status,
        403
    );
    assert_eq!(
        w.git_file(":worktree", "../x.toml").unwrap_err().status,
        403
    );
    assert_eq!(
        w.git_file(":worktree", "src/none.toml").unwrap_err().status,
        404
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/hostname", r.project.join("src/link.toml")).unwrap();
        assert_eq!(
            w.git_file(":worktree", "src/link.toml").unwrap_err().status,
            403
        );
    }
}

// ---- the extensions of ADR 0059: the kind of a part ----

#[test]
fn a_part_that_is_another_library_part_has_both_kinds_and_is_not_counted_as_a_value() {
    need_tools!();
    let (r, _, _) = committed("kind", "");
    // The same two pins on the same nets, from another part of the library.
    r.edit(
        "src/main.nsm",
        ".power_in: Header1x2",
        ".power_in: Socket1x2",
    );
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let power = board(&c, "TwoBoards/power");
    let change = power
        .parts
        .changed
        .iter()
        .find(|p| p.path == "power_in")
        .unwrap_or_else(|| panic!("{:#?}", power.parts));
    assert_eq!(change.kind, "Socket1x2");
    assert_eq!(change.base_kind.as_deref(), Some("Header1x2"));
    let kind = change.fields.iter().find(|f| f.field == "kind").unwrap();
    assert_eq!(
        (kind.old.as_deref(), kind.new.as_deref()),
        (Some("Header1x2"), Some("Socket1x2"))
    );
    assert!(
        c.summary
            .contains("1 part's kind changed from Header1x2 to Socket1x2"),
        "{}",
        c.summary
    );
    assert!(!c.summary.contains("value changed"), "{}", c.summary);
    // The same part with another value says its kind and no base kind.
    let (r, _, _) = committed("kind-same", "");
    r.edit(
        "src/main.nsm",
        ".r1: Resistor {}",
        ".r1: Resistor { .resistance &= 150ohm +/- 1% }",
    );
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let r1 = board(&c, "TwoBoards/led")
        .parts
        .changed
        .iter()
        .find(|p| p.path == "r1")
        .unwrap();
    assert_eq!((r1.kind.as_str(), r1.base_kind.clone()), ("Resistor", None));
}

// ---- the extensions of ADR 0059: warnings and notes from a ref ----

/// A 2 A supply and load on a `5V` / `GND` pair with the rail drawn `width` wide: the track guideline
/// (ADR 0036) warns for a narrow one, twice (the load and the source), and for none that is wide enough.
fn heater(width: &str) -> String {
    format!(
        "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

board Heater {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .load: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= 0A..2A
        .min_track &= {width}..1m
    }}
    .source: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= -2A..0A
    }}
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}}
"
    )
}

fn severities(list: &[netsmith_editor::api::Problem]) -> (usize, usize, usize) {
    let n = |s: Severity| list.iter().filter(|p| p.severity == s).count();
    (n(Severity::Error), n(Severity::Warning), n(Severity::Note))
}

#[test]
fn a_new_warning_and_a_resolved_one_are_compared_between_two_refs_each_by_its_severity() {
    need_tools!();
    let r = Repo::new("warnings", "");
    fs::write(r.project.join("src/main.nsm"), heater("1mm")).unwrap();
    let wide = r.commit("a wide track");
    fs::write(r.project.join("src/main.nsm"), heater("0.2mm")).unwrap();
    let narrow = r.commit("a narrow track");
    let w = r.workspace();
    // The warnings of the second side are new.
    let c = compare_refs(&w, &wide, &narrow).unwrap();
    assert_eq!(c.error, None, "{:?}", c.error);
    let h = board(&c, "Heater");
    let (errors, warnings, _) = severities(&h.problems.new);
    assert_eq!((errors, warnings), (0, 2), "{:#?}", h.problems);
    assert!(
        h.problems
            .resolved
            .iter()
            .all(|p| p.severity != Severity::Warning)
    );
    assert!(
        h.problems
            .new
            .iter()
            .all(|p| p.message.contains("IPC-2221") || p.severity == Severity::Note)
    );
    assert!(c.summary.contains("2 new warnings"), "{}", c.summary);
    assert!(!c.summary.contains("error"), "{}", c.summary);
    assert_eq!(h.status, BoardStatus::Changed);
    assert!(c.changed);
    // Each warning has the line of the side it is on.
    assert!(h.problems.new.iter().any(|p| p.at.is_some()));
    // The other way round they are resolved, and the sentence says so.
    let back = compare_refs(&w, &narrow, &wide).unwrap();
    let h = board(&back, "Heater");
    assert_eq!(severities(&h.problems.resolved).1, 2, "{:#?}", h.problems);
    assert!(
        back.summary.contains("2 warnings resolved"),
        "{}",
        back.summary
    );
    assert!(!back.summary.contains("new warning"), "{}", back.summary);
    // A warning that is on both sides is neither.
    let same = compare_refs(&w, &narrow, &narrow).unwrap();
    let h = board(&same, "Heater");
    assert_eq!(severities(&h.problems.new), (0, 0, 0));
    assert!(h.problems.unchanged >= 2);
    assert_eq!(same.summary, "No circuit changes");
}

#[test]
fn a_warning_of_the_files_on_disk_against_a_ref_and_the_notes_in_the_sentence() {
    need_tools!();
    let r = Repo::new("warnings-disk", "");
    fs::write(r.project.join("src/main.nsm"), heater("1mm")).unwrap();
    r.commit("a wide track");
    fs::write(r.project.join("src/main.nsm"), heater("0.2mm")).unwrap();
    let c = compare(&r.workspace(), "HEAD").unwrap();
    let h = board(&c, "Heater");
    assert_eq!(severities(&h.problems.new).1, 2, "{:#?}", h.problems);
    assert!(c.summary.starts_with("2 new warnings"), "{}", c.summary);
    // Notes are counted too, and come after the warnings.
    let notes = severities(&h.problems.new).2;
    let said = c.summary.contains(&format!("{notes} new note"));
    assert_eq!(said, notes > 0, "{} ({notes} notes)", c.summary);
}
