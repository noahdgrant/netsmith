//! The dependency lock from the command line (issue #1012): `build` writes
//! `dependencies.lock.toml` for a dependency by git, `lock --check` fails when it does not hold
//! what the manifest asks for and fetches nothing, and `lock --update` takes the commit a branch
//! names now. The repository is a local one the test makes, so nothing uses the network.

use std::{fs, path::Path, process::Command};

use netsmith_testkit::TempDir;

const LED: &str = include_str!("../../../examples/led.nsm");

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// Run git in `dir` with no configuration of the machine's.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.invalid"])
        .args(["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit(repo: &Path, message: &str) -> String {
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "--quiet", "-m", message]);
    git(repo, &["rev-parse", "HEAD"])
}

fn netsmith(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(root.join("app"))
        .env("NETSMITH_CACHE_DIR", root.join("cache"))
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        )
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).to_string()
}

fn manifest(root: &Path, entry: &str) {
    write(
        &root.join("app/netsmith.toml"),
        &format!("[project]\nname = \"app\"\n\n[dependencies]\n{entry}\n"),
    );
}

#[test]
fn build_writes_the_dependency_lock_and_lock_checks_and_updates_it() {
    let root = TempDir::new("cli-dependency-lock");
    let repo = root.join("repo");
    write(
        &repo.join("netsmith.toml"),
        "[project]\nname = \"shared\"\ntop_level = []\n",
    );
    write(&repo.join("src/link.nsm"), "interface Link {\n}\n");
    git(&repo, &["init", "--quiet"]);
    let first = commit(&repo, "first");
    let url = format!("file://{}", repo.display());
    manifest(&root, &format!("shared = {{ git = \"{url}\", rev = \"main\" }}"));
    write(&root.join("app/src/main.nsm"), LED);

    let build = netsmith(&root, &["build"]);
    assert!(build.status.success(), "{}", text(&build.stderr));
    assert!(
        text(&build.stderr).contains("wrote dependencies.lock.toml, the commit of each dependency by git"),
        "{}",
        text(&build.stderr)
    );
    let lock = fs::read_to_string(root.join("app/dependencies.lock.toml")).unwrap();
    assert!(lock.contains(&format!("commit = \"{first}\"")), "{lock}");
    let check = netsmith(&root, &["lock", "--check"]);
    assert!(check.status.success(), "{}", text(&check.stderr));

    // The branch moves: the lock still holds, and the check, which fetches nothing, passes.
    write(&repo.join("src/more.nsm"), "interface More {\n}\n");
    let second = commit(&repo, "second");
    let check = netsmith(&root, &["lock", "--check"]);
    assert!(check.status.success(), "{}", text(&check.stderr));
    let update = netsmith(&root, &["lock", "--update"]);
    assert!(update.status.success(), "{}", text(&update.stderr));
    let out = text(&update.stdout);
    assert!(
        out.contains("updated dependencies.lock.toml")
            && out.contains(&format!("+ shared {url} rev main at {}", &second[..12])),
        "{out}"
    );

    // The manifest asks for something else: the check fails, naming it.
    git(&repo, &["tag", "v1.0.0"]);
    manifest(&root, &format!("shared = {{ git = \"{url}\", version = \"1\" }}"));
    let check = netsmith(&root, &["lock", "--check"]);
    assert_eq!(check.status.code(), Some(1), "{}", text(&check.stderr));
    assert!(
        text(&check.stdout).contains("no commit for shared")
            && text(&check.stderr).contains("dependencies.lock.toml is not current"),
        "{}{}",
        text(&check.stdout),
        text(&check.stderr)
    );
}
