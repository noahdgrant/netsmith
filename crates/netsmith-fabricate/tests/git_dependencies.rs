//! Dependencies by `git` and by `version` (issue #1012): fetched into a cache, read at the commit
//! the dependency lock (`dependencies.lock.toml`) holds, and the lock written, updated and
//! checked. Every repository is a local one that the test makes, named by a `file://` URL, so no
//! test uses the network. Every part and name is invented.

use std::fs;
use std::path::Path;
use std::process::Command;

use netsmith_fabricate::dependency_lock::{self, DependencyLock, LOCK_FILE};
use netsmith_fabricate::project::{Project, Target};
use netsmith_pipeline::outcome_with;
use netsmith_testkit::TempDir;

const RF: &str = "\
generic CoaxReceptacle {
    .designator &= \"J\"
    pins {
        .signal passive
        .shield passive
    }
}

part AcmeMicroCoax provides CoaxReceptacle {
    pins {
        .signal passive \"S\" \"1\"
        .shield passive \"G\" \"2\"
    }
}
";

const MAIN: &str = "\
import shared::rf::AcmeMicroCoax

board Feed {
    .rf: AcmeMicroCoax
}
";

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// Run git in `dir` with no user configuration, so a signing key or a hook of the machine plays
/// no part.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Commit everything in `repo` as `message`, and give the commit.
fn commit(repo: &Path, message: &str) -> String {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "--quiet", "-m", message]);
    git(repo, &["rev-parse", "HEAD"])
}

/// `root/repo`, a git repository of a project of modules (one commit on `main`), and `root/app`,
/// whose manifest has `entry` under `[dependencies]` (`{url}` in it is the repository's URL).
fn setup(entry: &str) -> (TempDir, String, String) {
    let root = TempDir::new("git-dependencies");
    let repo = root.join("repo");
    write(
        &repo.join("netsmith.toml"),
        "[project]\nname = \"shared-lib\"\ntop_level = []\n",
    );
    write(&repo.join("src/rf.nsm"), RF);
    git(&repo, &["init", "--quiet"]);
    let first = commit(&repo, "first");
    let url = format!("file://{}", repo.display());
    app_manifest(&root, &entry.replace("{url}", &url));
    write(&root.join("app/src/main.nsm"), MAIN);
    (root, url, first)
}

fn app_manifest(root: &Path, entry: &str) {
    write(
        &root.join("app/netsmith.toml"),
        &format!(
            "[project]\nname = \"app\"\ntop_level = [\"src/main.nsm\"]\n\n[dependencies]\n{entry}\n"
        ),
    );
}

/// The app's project with its git cache in `root/cache`.
fn app(root: &Path) -> Project {
    Project::load(&root.join("app"))
        .unwrap()
        .with_git_cache(&root.join("cache"))
}

/// The kind of the board's one part, built as `build` builds it, with the cache in `root/cache`.
fn built(root: &Path) -> Result<String, String> {
    let mut target = Target::resolve_from(&root.join("app"), None)?;
    target.project = target.project.map(|p| p.with_git_cache(&root.join("cache")));
    let file = &target.files[0];
    let text = fs::read_to_string(&file.path).unwrap();
    let loader = target.loader()?;
    let catalogs = target.catalogs()?;
    let outcome = outcome_with(&file.name, &text, &loader, &catalogs, None).map_err(|e| {
        e.iter()
            .map(|p| p.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    Ok(outcome.boards[0].design.parts[0].kind.clone())
}

#[test]
fn a_dependency_by_git_is_fetched_imported_under_its_name_and_locked() {
    let (root, _, first) = setup("shared = { git = \"{url}\", rev = \"main\" }");
    assert_eq!(built(&root).unwrap(), "AcmeMicroCoax");
    let project = app(&root);
    let dependencies = project.dependencies().unwrap();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].commit.as_deref(), Some(first.as_str()));
    // Messages name its files by its name and commit, since they are in the cache.
    assert_eq!(dependencies[0].folder, format!("shared@{}", &first[..12]));
    let shown: Vec<String> = project
        .dependency_modules()
        .unwrap()
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    assert_eq!(shown, [format!("shared@{}/src/rf.nsm", &first[..12])]);
    // `build` writes the lock when there is none, and not again.
    let written = dependency_lock::write_if_missing(&project).unwrap().unwrap();
    assert_eq!(written.entries[0].commit, first);
    assert!(dependency_lock::write_if_missing(&project).unwrap().is_none());
    let text = fs::read_to_string(root.join("app").join(LOCK_FILE)).unwrap();
    assert!(text.contains(&format!("commit = \"{first}\"")), "{text}");
    assert!(
        dependency_lock::check(&project).unwrap().stale.is_empty(),
        "a lock just written is current"
    );
}

#[test]
fn the_lock_holds_a_branch_that_moved_until_lock_update() {
    let (root, _, first) = setup("shared = { git = \"{url}\", rev = \"main\" }");
    let project = app(&root);
    dependency_lock::write_if_missing(&project).unwrap();
    let repo = root.join("repo");
    write(&repo.join("src/extra.nsm"), "interface Extra {\n}\n");
    let second = commit(&repo, "second");
    // The lock holds the first commit, so the new file is not read.
    let modules = |p: &Project| -> Vec<String> {
        p.dependency_modules()
            .unwrap()
            .into_iter()
            .map(|(m, _)| m.key)
            .collect()
    };
    assert_eq!(modules(&project), ["shared::rf"]);
    let update = dependency_lock::update(&project).unwrap();
    assert_eq!(update.diff.len(), 2, "{:?}", update.diff);
    assert!(update.diff[0].starts_with("- shared") && update.diff[0].ends_with(&first[..12]));
    assert!(update.diff[1].starts_with("+ shared") && update.diff[1].ends_with(&second[..12]));
    assert_eq!(modules(&project), ["shared::extra", "shared::rf"]);
    // Nothing moved since: an update changes nothing.
    assert!(dependency_lock::update(&project).unwrap().diff.is_empty());
}

#[test]
fn a_locked_dependency_in_the_cache_builds_with_its_repository_gone_and_offline() {
    let (root, _, _) = setup("shared = { git = \"{url}\", rev = \"main\" }");
    dependency_lock::write_if_missing(&app(&root)).unwrap();
    fs::remove_dir_all(root.join("repo")).unwrap();
    assert_eq!(built(&root).unwrap(), "AcmeMicroCoax");
    let offline = app(&root).with_offline(true);
    assert_eq!(offline.dependencies().unwrap().len(), 1);
}

#[test]
fn offline_a_dependency_the_cache_does_not_hold_is_an_error_that_says_so() {
    let (root, url, _) = setup("shared = { git = \"{url}\", rev = \"main\" }");
    let e = app(&root).with_offline(true).dependencies().unwrap_err();
    assert!(
        e.contains("[dependencies] shared") && e.contains(&url) && e.contains("--offline"),
        "{e}"
    );
}

#[test]
fn a_version_dependency_reads_the_newest_tag_its_requirement_admits() {
    let (root, url, first) = setup("shared = { git = \"{url}\", version = \"1\" }");
    let repo = root.join("repo");
    git(&repo, &["tag", "v1.0.0", &first]);
    write(&repo.join("src/extra.nsm"), "interface Extra {\n}\n");
    let second = commit(&repo, "second");
    git(&repo, &["tag", "1.2.0"]);
    write(&repo.join("src/third.nsm"), "interface Third {\n}\n");
    let third = commit(&repo, "third");
    git(&repo, &["tag", "v2.0.0"]);
    git(&repo, &["tag", "not-a-version"]);
    let commit_of = |root: &Path| app(root).dependencies().unwrap()[0].commit.clone().unwrap();
    assert_eq!(commit_of(&root), second);
    app_manifest(&root, &format!("shared = {{ git = \"{url}\", version = \"=1.0.0\" }}"));
    assert_eq!(commit_of(&root), first);
    app_manifest(&root, &format!("shared = {{ git = \"{url}\", version = \">=2\" }}"));
    assert_eq!(commit_of(&root), third);
    app_manifest(&root, &format!("shared = {{ git = \"{url}\", version = \"3\" }}"));
    let e = app(&root).dependencies().unwrap_err();
    assert!(e.contains("no tag") && e.contains("\"^3\""), "{e}");
}

#[test]
fn a_rev_the_repository_does_not_have_is_named() {
    let (root, url, _) = setup("shared = { git = \"{url}\", rev = \"no-such-branch\" }");
    let e = app(&root).dependencies().unwrap_err();
    assert!(
        e.contains(&format!("{url} has no branch, tag or commit `no-such-branch`")),
        "{e}"
    );
}

#[test]
fn check_names_an_entry_the_lock_lacks_and_one_no_manifest_asks_for() {
    let (root, url, _) = setup("shared = { git = \"{url}\", rev = \"main\" }");
    let project = app(&root);
    // No lock yet, and a dependency by git: not current.
    let stale = dependency_lock::check(&project).unwrap().stale;
    assert!(stale[0].contains("does not exist"), "{stale:?}");
    dependency_lock::write_if_missing(&project).unwrap();
    // The manifest now asks for a tag: the lock has no commit for that, and still holds `main`.
    git(&root.join("repo"), &["tag", "v1.0.0"]);
    app_manifest(&root, &format!("shared = {{ git = \"{url}\", rev = \"v1.0.0\" }}"));
    let stale = dependency_lock::check(&app(&root)).unwrap().stale;
    assert!(
        stale[0].contains("no commit for shared") && stale[0].contains("rev v1.0.0"),
        "{stale:?}"
    );
    // The check fetches nothing, and the cache has not seen the tag: what the dependency asks
    // for in turn is unknown, so an entry no manifest names is not judged yet.
    assert_eq!(stale.len(), 1, "{stale:?}");
    // A build reads the tag all the same (fetching it), and the lock is left as it is.
    assert_eq!(built(&root).unwrap(), "AcmeMicroCoax");
    let lock = DependencyLock::read(&root.join("app")).unwrap().unwrap();
    assert_eq!(lock.entries[0].rev.as_deref(), Some("main"));
    // Now the cache holds it, and the entry for `main` is named too.
    let stale = dependency_lock::check(&app(&root)).unwrap().stale;
    assert_eq!(stale.len(), 2, "{stale:?}");
    assert!(stale[1].contains("rev main") && stale[1].contains("which no manifest asks for"));
}

#[test]
fn a_dependency_of_a_git_dependency_is_locked_in_the_first_projects_lock() {
    // `repo` depends on `radio` by git and on a folder inside itself by path.
    let (root, _, _) = setup("shared = { git = \"{url}\", rev = \"main\" }");
    let radio = root.join("radio");
    write(
        &radio.join("netsmith.toml"),
        "[project]\nname = \"radio\"\ntop_level = []\n",
    );
    write(&radio.join("src/link.nsm"), "interface Link {\n}\n");
    git(&radio, &["init", "--quiet"]);
    let radio_commit = commit(&radio, "radio");
    let repo = root.join("repo");
    write(
        &repo.join("netsmith.toml"),
        &format!(
            "[project]\nname = \"shared-lib\"\ntop_level = []\n\n[dependencies]\nradio = {{ git = \"file://{}\", rev = \"main\" }}\nparts = {{ path = \"parts\" }}\n",
            radio.display()
        ),
    );
    write(
        &repo.join("parts/netsmith.toml"),
        "[project]\nname = \"parts\"\ntop_level = []\n",
    );
    write(&repo.join("parts/src/pin.nsm"), "interface Pin {\n}\n");
    commit(&repo, "with dependencies");
    let project = app(&root);
    let mut names: Vec<(String, String)> = project
        .dependencies()
        .unwrap()
        .into_iter()
        .map(|d| (d.name, d.folder))
        .collect();
    names.sort();
    assert_eq!(names[0].0, "parts");
    assert!(names[0].1.starts_with("shared@") && names[0].1.ends_with("/parts"));
    assert_eq!(
        names[1],
        ("radio".to_string(), format!("radio@{}", &radio_commit[..12]))
    );
    let lock = dependency_lock::write_if_missing(&project).unwrap().unwrap();
    let locked: Vec<&str> = lock.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(locked, ["radio", "shared"]);
    assert!(dependency_lock::check(&project).unwrap().stale.is_empty());

    // A `path` entry of a dependency by git may not leave its repository.
    write(
        &repo.join("netsmith.toml"),
        "[project]\nname = \"shared-lib\"\ntop_level = []\n\n[dependencies]\nradio = { path = \"../radio\" }\n",
    );
    commit(&repo, "leaves");
    fs::remove_file(root.join("app").join(LOCK_FILE)).unwrap();
    let e = dependency_lock::update(&project).unwrap_err();
    assert!(e.contains("is outside the git repository"), "{e}");
}
