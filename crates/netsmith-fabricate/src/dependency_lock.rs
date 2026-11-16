//! The dependency lock (issue #1012, ADR 0137): `dependencies.lock.toml` beside `netsmith.toml`,
//! the commit each dependency by `git` resolved to, its own and those of its dependencies.
//!
//! `build` and `fab` write it when it does not exist and the project has a dependency by `git`,
//! and never rewrite it. While it exists, a build reads each such dependency at the commit the
//! lock holds for it, as long as the entry still says what the manifest says (the same `git`
//! and the same `rev` or `version`); an entry that no longer does is resolved again, and
//! `lock --check` fails on it. `netsmith lock --update` fetches every one again and rewrites the
//! file; `lock --check` compares the lock with the manifests without fetching anything, so it
//! passes with no network.
//!
//! ```toml
//! # The commit of each dependency by git (issue #1012). `netsmith lock --update` rewrites it.
//! version = 1
//!
//! [[dependency]]
//! name = "shared"
//! git = "https://example.com/shared.git"
//! rev = "main"
//! commit = "0123456789abcdef0123456789abcdef01234567"
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::git_dependency::Fetch;
use crate::project::Project;

/// The lock's file name, beside `netsmith.toml`.
pub const LOCK_FILE: &str = "dependencies.lock.toml";

/// The lock format's version.
const VERSION: u32 = 1;

const HEADER: &str =
    "# The commit of each dependency by git (issue #1012). `netsmith lock --update` rewrites it.\n";

/// One dependency by `git`, and the commit it resolved to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Locked {
    /// The key of its `[dependencies]` entry.
    pub name: String,
    pub git: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The full commit hash.
    pub commit: String,
}

impl Locked {
    /// Whether `other` asks for the same thing (the commit aside).
    pub fn same_request(&self, other: &Locked) -> bool {
        self.name == other.name
            && self.git == other.git
            && self.rev == other.rev
            && self.version == other.version
    }

    /// How the diff of `lock --update` and `--check` shows it.
    fn line(&self) -> String {
        let asked = match (&self.rev, &self.version) {
            (Some(rev), _) => format!("rev {rev}"),
            (_, Some(version)) => format!("version {version}"),
            _ => String::new(),
        };
        format!(
            "{} {} {asked} at {}",
            self.name,
            self.git,
            short(&self.commit)
        )
    }
}

fn short(commit: &str) -> &str {
    &commit[..commit.len().min(12)]
}

/// The dependency lock of one project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyLock {
    version: u32,
    #[serde(default, rename = "dependency")]
    pub entries: Vec<Locked>,
}

impl DependencyLock {
    pub fn new(mut entries: Vec<Locked>) -> DependencyLock {
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        DependencyLock {
            version: VERSION,
            entries,
        }
    }

    /// The lock file of the project at `root`.
    pub fn path(root: &Path) -> PathBuf {
        root.join(LOCK_FILE)
    }

    /// The lock of the project at `root`, `None` when there is no file.
    pub fn read(root: &Path) -> Result<Option<DependencyLock>, String> {
        let path = Self::path(root);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("{LOCK_FILE}: {e}")),
        };
        Self::parse(&text).map(Some)
    }

    pub fn parse(text: &str) -> Result<DependencyLock, String> {
        let lock: DependencyLock = toml::from_str(text).map_err(|e| format!("{LOCK_FILE}: {e}"))?;
        if lock.version != VERSION {
            return Err(format!(
                "{LOCK_FILE}: version = {} is not one this netsmith reads ({VERSION}); delete the file and build again to write it anew",
                lock.version
            ));
        }
        for entry in &lock.entries {
            if entry.commit.len() < 40 || !entry.commit.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!(
                    "{LOCK_FILE}: dependency {}: commit {:?} is not a full commit hash",
                    entry.name, entry.commit
                ));
            }
        }
        Ok(lock)
    }

    pub fn render(&self) -> String {
        format!(
            "{HEADER}{}",
            toml::to_string(self).expect("a lock of strings is always TOML")
        )
    }

    pub fn write(&self, root: &Path) -> Result<(), String> {
        let path = Self::path(root);
        fs::write(&path, self.render()).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The commit this lock holds for a dependency that asks for what `wanted` asks for.
    pub fn commit_for(&self, wanted: &Locked) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.same_request(wanted))
            .map(|e| e.commit.as_str())
    }

    /// What differs from `old` to `self`, one line each: `+` an entry `self` has and `old` has
    /// not, `-` the other way, and both for one whose commit moved. Empty when they are the same.
    pub fn diff(&self, old: Option<&DependencyLock>) -> Vec<String> {
        let none = Vec::new();
        let old = old.map_or(&none, |o| &o.entries);
        let mut lines = Vec::new();
        for entry in old {
            if !self.entries.contains(entry) {
                lines.push(format!("- {}", entry.line()));
            }
        }
        for entry in &self.entries {
            if !old.contains(entry) {
                lines.push(format!("+ {}", entry.line()));
            }
        }
        lines
    }

    /// Why this lock does not hold what `wanted` (each dependency by `git`, with no commit
    /// considered) asks for, one line each; empty when it does. With `complete` false, `wanted`
    /// may lack the dependencies of a dependency, so an entry it lacks is not named.
    pub fn stale(&self, wanted: &[Locked], complete: bool) -> Vec<String> {
        let mut why = Vec::new();
        for w in wanted {
            if !self.entries.iter().any(|e| e.same_request(w)) {
                let asked = Locked {
                    commit: String::new(),
                    ..w.clone()
                };
                why.push(format!(
                    "{} has no commit for {}",
                    LOCK_FILE,
                    asked.line().trim_end_matches(" at ")
                ));
            }
        }
        for e in self.entries.iter().filter(|_| complete) {
            if !wanted.iter().any(|w| e.same_request(w)) {
                why.push(format!(
                    "{LOCK_FILE} holds {}, which no manifest asks for",
                    e.line()
                ));
            }
        }
        why
    }
}

/// `build` and `fab`: write the lock of `project` when it has none and it reads a dependency by
/// `git`, and say what was written. An existing lock is never rewritten (`lock --update` does).
pub fn write_if_missing(project: &Project) -> Result<Option<DependencyLock>, String> {
    if DependencyLock::path(&project.root).exists() {
        return Ok(None);
    }
    let (_, lock) = project.resolve_dependencies(None, Fetch::WhenMissing)?;
    if lock.entries.is_empty() {
        return Ok(None);
    }
    lock.write(&project.root)?;
    Ok(Some(lock))
}

/// What `lock --update` did with the lock of a project, or what `lock --check` found.
#[derive(Debug)]
pub struct Refreshed {
    /// The lines of [`DependencyLock::diff`] from the old lock to the new one (`--update`).
    pub diff: Vec<String>,
    /// Why the lock is not current (`--check`); empty when it is.
    pub stale: Vec<String>,
    /// Whether the file existed before the run.
    pub existed: bool,
}

/// `lock --update`: fetch every dependency by `git` again, take the commit its `rev` or
/// `version` names now, and write the lock (remove it when no dependency is by `git`).
pub fn update(project: &Project) -> Result<Refreshed, String> {
    let old = DependencyLock::read(&project.root)?;
    let (_, new) = project.resolve_dependencies(None, Fetch::Always)?;
    let diff = new.diff(old.as_ref());
    let path = DependencyLock::path(&project.root);
    if new.entries.is_empty() {
        if old.is_some() {
            fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    } else if old.as_ref() != Some(&new) {
        new.write(&project.root)?;
    }
    Ok(Refreshed {
        diff,
        stale: Vec::new(),
        existed: old.is_some(),
    })
}

/// `lock --check`: whether the lock holds a commit for what each dependency by `git` asks for,
/// and nothing else. Nothing is fetched: what the cache does not hold is not looked into.
pub fn check(project: &Project) -> Result<Refreshed, String> {
    let (wanted, complete) = project.git_requests()?;
    let old = DependencyLock::read(&project.root)?;
    let stale = match &old {
        None if wanted.is_empty() => Vec::new(),
        None => vec![format!(
            "{LOCK_FILE} does not exist, and the project has a dependency by `git`"
        )],
        Some(lock) => lock.stale(&wanted, complete),
    };
    Ok(Refreshed {
        diff: Vec::new(),
        stale,
        existed: old.is_some(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, commit: char) -> Locked {
        Locked {
            name: name.to_string(),
            git: "file:///repos/shared".to_string(),
            rev: Some("main".to_string()),
            version: None,
            commit: commit.to_string().repeat(40),
        }
    }

    #[test]
    fn a_lock_survives_a_round_trip_and_sorts_its_entries() {
        let lock = DependencyLock::new(vec![entry("zeta", 'b'), entry("alpha", 'a')]);
        let text = lock.render();
        assert!(text.starts_with("# The commit"), "{text}");
        assert!(
            text.find("alpha").unwrap() < text.find("zeta").unwrap(),
            "{text}"
        );
        assert_eq!(DependencyLock::parse(&text).unwrap(), lock);
    }

    #[test]
    fn a_lock_with_another_version_or_a_short_commit_is_refused() {
        let e = DependencyLock::parse("version = 2\n").unwrap_err();
        assert!(e.contains("version = 2"), "{e}");
        let e = DependencyLock::parse(
            "version = 1\n[[dependency]]\nname = \"a\"\ngit = \"x\"\nrev = \"main\"\ncommit = \"abc\"\n",
        )
        .unwrap_err();
        assert!(e.contains("not a full commit hash"), "{e}");
    }

    #[test]
    fn stale_names_what_is_missing_and_what_no_manifest_asks_for() {
        let lock = DependencyLock::new(vec![entry("old", 'a')]);
        let why = lock.stale(&[entry("new", '0')], true);
        assert_eq!(why.len(), 2, "{why:?}");
        assert!(why[0].contains("no commit for new"), "{why:?}");
        assert!(why[1].contains("holds old"), "{why:?}");
        assert!(lock.stale(&[entry("old", 'f')], true).is_empty());
    }

    #[test]
    fn the_diff_shows_a_moved_commit_as_a_line_out_and_a_line_in() {
        let old = DependencyLock::new(vec![entry("shared", 'a')]);
        let new = DependencyLock::new(vec![entry("shared", 'b')]);
        let diff = new.diff(Some(&old));
        assert_eq!(diff.len(), 2, "{diff:?}");
        assert!(diff[0].starts_with("- shared") && diff[0].ends_with("aaaaaaaaaaaa"));
        assert!(diff[1].starts_with("+ shared") && diff[1].ends_with("bbbbbbbbbbbb"));
        assert!(new.diff(Some(&new)).is_empty());
    }
}
