//! `build_web.rs` (what `build.rs` runs) against fake `node` and `npm` scripts: the reason it
//! gives when a step fails, when it installs again, what it takes for a bundle, and that two
//! builds of one `web/` do not run at once, and which page a build takes: a fresh build when Node
//! is there, else none, judged by the sources' bytes and never by file times.

#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

use crate::build_script as build_web;
use build_web::{Done, INSTALL_STAMP, Provided};
use netsmith_testkit::TempDir;

/// A scratch directory with a `web/` (sources and a lockfile) and a `bin/` with a fake `node`
/// and `npm`. The fake npm appends each command to `log`; `ci` makes `node_modules`; `run build`
/// makes `dist/index.html` and an asset. A file `fail-ci` or `fail-build` in `web/` makes that
/// step print its text and exit 2; `slow` makes the build sleep a moment.
struct Fixture {
    root: TempDir,
    web: PathBuf,
    bin: PathBuf,
}

impl Fixture {
    fn new(node_version: &str) -> Fixture {
        let root = TempDir::new("build-web");
        let web = root.join("web");
        let bin = root.join("bin");
        fs::create_dir_all(web.join("src")).unwrap();
        fs::create_dir_all(&bin).unwrap();
        fs::write(web.join("src/main.tsx"), "x").unwrap();
        for f in [
            "index.html",
            "package.json",
            "tsconfig.json",
            "vite.config.ts",
        ] {
            fs::write(web.join(f), "x").unwrap();
        }
        fs::write(web.join("package-lock.json"), "{}").unwrap();
        script(
            &bin.join("node"),
            &format!("#!/bin/sh\necho {node_version}\n"),
        );
        script(
            &bin.join("npm"),
            r#"#!/bin/sh
echo "npm $*" >> "$(dirname "$0")/../log"
case "$1" in
  --version) echo 10.9.9 ;;
  ci)
    if [ -f fail-ci ]; then cat fail-ci; exit 2; fi
    rm -rf node_modules; mkdir -p node_modules/.bin
    : > node_modules/.bin/tsc; : > node_modules/.bin/vite ;;
  run)
    echo "start" >> "$(dirname "$0")/../order"
    if [ -f slow ]; then sleep 0.4; fi
    if [ -f fail-build ]; then
      for i in $(seq 1 40); do echo "line $i"; done
      echo "src/App.tsx(3,1): error TS2304: Cannot find name 'x'." >&2
      exit 2
    fi
    rm -rf dist; mkdir -p dist/assets
    echo '<script src="/assets/app.js"></script>' > dist/index.html
    echo 'js' > dist/assets/app.js
    echo "end" >> "$(dirname "$0")/../order" ;;
esac
"#,
        );
        Fixture { root, web, bin }
    }

    fn dirs(&self) -> Vec<PathBuf> {
        vec![self.bin.clone()]
    }

    fn ensure(&self) -> Result<Done, String> {
        build_web::ensure(&self.web, &self.dirs(), Duration::from_secs(30))
    }

    fn log(&self) -> Vec<String> {
        fs::read_to_string(self.root.join("log"))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.contains("--version"))
            .map(str::to_string)
            .collect()
    }
}

/// Rewrites a source and makes it newer than anything built, whatever the clock's grain.
fn change(path: &Path) {
    let text = fs::read_to_string(path).unwrap();
    fs::write(path, format!("{text} changed")).unwrap();
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(5))
        .unwrap();
}

fn script(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn a_build_installs_builds_and_marks_the_bundle() {
    let f = Fixture::new("v22.23.4");
    assert_eq!(f.ensure(), Ok(Done::Built { installed: true }));
    assert!(build_web::bundle_is_complete(&f.web.join("dist")));
    assert_eq!(f.log().len(), 2, "{:?}", f.log());
    // Nothing changed: nothing runs.
    assert_eq!(f.ensure(), Ok(Done::Fresh));
    assert_eq!(f.log().len(), 2);
}

#[test]
fn no_node_says_what_was_looked_at() {
    let f = Fixture::new("v18.19.0");
    let reason = f.ensure().unwrap_err();
    assert!(reason.contains("Node 20 or newer"), "{reason}");
    assert!(reason.contains("v18.19.0, too old"), "{reason}");
    let none = build_web::ensure(&f.web, &[], Duration::from_secs(1)).unwrap_err();
    assert!(none.contains("0 directories searched"), "{none}");
}

#[test]
fn a_failed_build_gives_the_command_the_exit_code_and_the_end_of_its_output() {
    let f = Fixture::new("v22.23.4");
    fs::write(f.web.join("fail-build"), "").unwrap();
    let reason = f.ensure().unwrap_err();
    assert!(
        reason.contains("`npm run build` failed with exit code 2"),
        "{reason}"
    );
    assert!(
        reason.contains(&format!("in {}", f.web.display())),
        "{reason}"
    );
    assert!(reason.contains("node v22.23.4 and npm 10.9.9"), "{reason}");
    assert!(reason.contains("PATH starts "), "{reason}");
    assert!(reason.contains("error TS2304"), "{reason}");
    assert!(reason.contains("line 40"), "{reason}");
    // The tail is the last 15 lines, not everything.
    assert!(!reason.contains("line 20\n"), "{reason}");
    // A failed build leaves no bundle that looks finished.
    assert!(!build_web::bundle_is_complete(&f.web.join("dist")));
    assert_eq!(
        build_web::copy_bundle(&f.web, &f.root.join("copy")),
        Ok(false)
    );
}

#[test]
fn a_failed_install_says_why_it_installed() {
    let f = Fixture::new("v22.23.4");
    fs::write(f.web.join("fail-ci"), "npm ERR! network is down").unwrap();
    let reason = f.ensure().unwrap_err();
    assert!(
        reason.contains("installing the packages (node_modules is missing)"),
        "{reason}"
    );
    assert!(
        reason.contains("`npm ci --include=dev --no-audit --no-fund` failed with exit code 2"),
        "{reason}"
    );
    assert!(reason.contains("network is down"), "{reason}");
    assert!(
        !f.log().iter().any(|l| l.contains("run build")),
        "{:?}",
        f.log()
    );
}

#[test]
fn packages_installed_by_another_node_major_are_installed_again() {
    let f = Fixture::new("v22.23.4");
    f.ensure().unwrap();
    let stamp = f.web.join("node_modules").join(INSTALL_STAMP);
    let current = fs::read_to_string(&stamp).unwrap();
    assert!(current.starts_with("node 22 "), "{current}");
    fs::write(&stamp, current.replace("node 22", "node 18")).unwrap();
    change(&f.web.join("src/main.tsx"));
    assert_eq!(f.ensure(), Ok(Done::Built { installed: true }));
    assert_eq!(
        f.log().iter().filter(|l| l.starts_with("npm ci")).count(),
        2
    );
}

#[test]
fn a_changed_lockfile_installs_again_and_a_changed_source_does_not() {
    let f = Fixture::new("v22.23.4");
    f.ensure().unwrap();
    change(&f.web.join("src/main.tsx"));
    assert_eq!(f.ensure(), Ok(Done::Built { installed: false }));
    fs::write(f.web.join("package-lock.json"), "{\"v\":2}").unwrap();
    change(&f.web.join("package-lock.json"));
    assert_eq!(f.ensure(), Ok(Done::Built { installed: true }));
}

#[test]
fn a_missing_compiler_installs_again() {
    let f = Fixture::new("v22.23.4");
    f.ensure().unwrap();
    fs::remove_file(f.web.join("node_modules/.bin/tsc")).unwrap();
    change(&f.web.join("src/main.tsx"));
    assert_eq!(f.ensure(), Ok(Done::Built { installed: true }));
}

#[test]
fn a_partial_bundle_is_not_a_bundle() {
    let f = Fixture::new("v22.23.4");
    f.ensure().unwrap();
    let dist = f.web.join("dist");
    // The marker is there but a file the page names is not (a build was killed midway).
    fs::remove_file(dist.join("assets/app.js")).unwrap();
    assert!(!build_web::bundle_is_complete(&dist));
    assert_eq!(f.ensure(), Ok(Done::Built { installed: false }));
    // No marker: also unfinished, whatever is in the folder.
    fs::remove_file(dist.join(build_web::DONE_MARKER)).unwrap();
    assert!(build_web::is_stale(&f.web));
}

#[test]
fn the_embedded_copy_leaves_out_the_marker_and_survives_a_later_build() {
    let f = Fixture::new("v22.23.4");
    f.ensure().unwrap();
    let copy = f.root.join("copy");
    assert_eq!(build_web::copy_bundle(&f.web, &copy), Ok(true));
    assert!(copy.join("index.html").is_file());
    assert!(copy.join("assets/app.js").is_file());
    assert!(!copy.join(build_web::DONE_MARKER).exists());
    fs::remove_dir_all(f.web.join("dist")).unwrap();
    assert!(copy.join("assets/app.js").is_file());
}

#[test]
fn two_builds_of_one_web_folder_run_one_after_the_other() {
    let f = Fixture::new("v22.23.4");
    fs::write(f.web.join("slow"), "").unwrap();
    let results: Vec<_> = std::thread::scope(|s| {
        let a = s.spawn(|| f.ensure());
        let b = s.spawn(|| f.ensure());
        vec![a.join().unwrap(), b.join().unwrap()]
    });
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert!(results.contains(&Ok(Done::Fresh)), "{results:?}");
    // One build, not two: its start and end are not interleaved with another's.
    let order = fs::read_to_string(f.root.join("order")).unwrap();
    assert_eq!(order.lines().collect::<Vec<_>>(), ["start", "end"]);
}

#[test]
fn a_build_gives_up_waiting_for_a_lock_that_is_never_released() {
    let f = Fixture::new("v22.23.4");
    let held = fs::File::create(f.web.join(".build.lock")).unwrap();
    held.lock().unwrap();
    let reason = build_web::ensure(&f.web, &f.dirs(), Duration::from_millis(200)).unwrap_err();
    assert!(reason.contains(".build.lock"), "{reason}");
    assert!(reason.contains("another build"), "{reason}");
}

#[test]
fn the_message_for_a_binary_without_the_page_names_the_manual_command() {
    let message = netsmith_editor::missing_bundle_message();
    assert!(message.contains("cd crates/netsmith-editor/web && npm ci && npm run build"));
    assert!(message.starts_with("this netsmith was built without the editor's web app: "));
}

#[test]
fn search_dirs_orders_nvm_versions_newest_first() {
    let root = TempDir::new("nvm");
    for v in ["v20.13.1", "v22.23.4", "v22.9.0"] {
        fs::create_dir_all(root.join(".nvm/versions/node").join(v).join("bin")).unwrap();
    }
    let dirs = build_web::search_dirs(None, Some(root.path().to_path_buf().into()));
    let names: Vec<String> = dirs
        .iter()
        .map(|d| {
            d.parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into()
        })
        .collect();
    assert_eq!(names, ["v22.23.4", "v22.9.0", "v20.13.1"]);
}

#[test]
fn the_hash_of_the_sources_follows_their_bytes_not_their_times() {
    let f = Fixture::new("v22.23.4");
    let before = build_web::source_hash(&f.web);
    // Same bytes, later time (a checkout): same hash.
    let main = f.web.join("src/main.tsx");
    fs::File::options()
        .write(true)
        .open(&main)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + Duration::from_secs(3600))
        .unwrap();
    assert_eq!(build_web::source_hash(&f.web), before);
    // Same time, other bytes; a new file; a file that is not a source: only the first two count.
    fs::write(f.web.join("src/other.ts"), "y").unwrap();
    let with_file = build_web::source_hash(&f.web);
    assert_ne!(with_file, before);
    fs::write(f.web.join("notes.txt"), "not a source").unwrap();
    fs::create_dir_all(f.web.join("node_modules/x")).unwrap();
    fs::write(f.web.join("node_modules/x/a.js"), "z").unwrap();
    assert_eq!(build_web::source_hash(&f.web), with_file);
    fs::write(&main, "x").unwrap();
    assert_eq!(
        build_web::source_hash(&f.web),
        with_file,
        "same bytes as before"
    );
    fs::write(&main, "xx").unwrap();
    assert_ne!(build_web::source_hash(&f.web), with_file);
}

#[test]
fn a_bundle_is_stale_when_its_sources_changed_even_if_the_files_are_older() {
    let f = Fixture::new("v22.23.4");
    f.ensure().unwrap();
    assert!(!build_web::is_stale(&f.web));
    // A source with an old time but new bytes: the bundle is not of it.
    let main = f.web.join("src/main.tsx");
    fs::write(&main, "different").unwrap();
    fs::File::options()
        .write(true)
        .open(&main)
        .unwrap()
        .set_modified(std::time::SystemTime::UNIX_EPOCH)
        .unwrap();
    assert!(build_web::is_stale(&f.web));
    // Back to the old bytes with a new time: the bundle is of them again.
    fs::write(&main, "x").unwrap();
    assert!(!build_web::is_stale(&f.web));
}

#[test]
fn the_sources_are_built_into_the_page_when_node_is_there() {
    let f = Fixture::new("v22.23.4");
    let to = f.root.join("embedded");
    let got = build_web::provide(&f.web, &f.dirs(), Duration::from_secs(30), &to);
    assert_eq!(got, Provided::Built);
    assert!(to.join("index.html").is_file());
    assert!(to.join("assets/app.js").is_file());
    // The marker is not part of the page.
    assert!(!to.join(build_web::DONE_MARKER).exists());
    // Changed sources are built again.
    change(&f.web.join("src/main.tsx"));
    let log = f.log().len();
    let again = build_web::provide(&f.web, &f.dirs(), Duration::from_secs(30), &to);
    assert_eq!(again, Provided::Built);
    assert!(f.log().len() > log, "the changed sources were built again");
}

#[test]
fn no_node_gives_no_page_and_the_reason() {
    let f = Fixture::new("v22.23.4");
    let to = f.root.join("embedded");
    let got = build_web::provide(&f.web, &[], Duration::from_secs(1), &to);
    match got {
        Provided::Missing(reason) => assert!(reason.contains("Node 20 or newer"), "{reason}"),
        other => panic!("{other:?}"),
    }
    assert!(!to.exists());
}
