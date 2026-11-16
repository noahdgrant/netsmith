//! Fetching a dependency named by a git repository (issue #1012, ADR 0137): the cache of
//! repositories and of the checkouts of their commits, and the resolution of a `rev` or a
//! `version` to a commit.
//!
//! **Through the `git` command line**, as the editor's comparison reads a ref (ADR 0059): a
//! library would be a large dependency for five commands. Every command runs with no terminal
//! prompt (`GIT_TERMINAL_PROMPT=0`), so a repository that asks for a password fails at once
//! rather than waiting for a person, and with standard input closed.
//!
//! **The cache.** Under the folder `NETSMITH_CACHE_DIR`, `$XDG_CACHE_HOME/netsmith` or
//! `~/.cache/netsmith` names (the one the supplier cache uses), in `git/`:
//! `git/db/<key>` is a bare repository that holds every branch and tag of one URL, and
//! `git/checkouts/<key>/<commit>` the files of one commit, which a build reads and never
//! writes. `<key>` is the first 16 hex digits of the SHA-256 of the URL. A checkout is made in
//! a folder beside it and renamed into place, so a build never sees half of one.
//!
//! **When the network is used.** A commit whose checkout exists needs no git command at all. A
//! commit, a `rev` or a `version` that the cached repository already answers is answered from
//! it, with no fetch: a branch moves only when `netsmith lock --update` asks for it
//! ([`Fetch::Always`]). Only what the cache cannot answer is fetched, and `--offline` refuses
//! that fetch with a message that names the URL.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

/// What a `[dependencies]` entry by `git` asks for.
#[derive(Debug, Clone, Copy)]
pub enum Want<'a> {
    /// A branch, a tag or a commit: anything `git rev-parse` reads.
    Rev(&'a str),
    /// The newest tag `vX.Y.Z` or `X.Y.Z` that the requirement (cargo's syntax: `"1.2"` is
    /// `>=1.2.0, <2.0.0`) admits.
    Version(&'a semver::VersionReq),
}

/// When the cached repository is brought up to date from its URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetch {
    /// Only when it cannot answer: a build, which reads what was fetched before.
    WhenMissing,
    /// Before every answer: `netsmith lock --update`, which takes the branches as they are now.
    Always,
}

/// The cache of git dependencies, and how this run may use the network.
#[derive(Debug, Clone)]
pub struct GitCache {
    dir: PathBuf,
    offline: bool,
}

impl GitCache {
    /// The cache under `dir` (the folder that holds `db/` and `checkouts/`).
    pub fn at(dir: &Path) -> GitCache {
        GitCache {
            dir: dir.to_path_buf(),
            offline: false,
        }
    }

    /// The cache of this machine: `git/` of the folder the supplier cache is in.
    pub fn default_dir() -> Option<PathBuf> {
        netsmith_catalog::refresh::cache_base().map(|b| b.join("git"))
    }

    /// This cache for a run that fetches nothing (`--offline`): what the cache does not hold is
    /// an error.
    pub fn offline(mut self, offline: bool) -> GitCache {
        self.offline = offline;
        self
    }

    fn key(url: &str) -> String {
        let digest = Sha256::digest(url.as_bytes());
        digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
    }

    fn db(&self, url: &str) -> PathBuf {
        self.dir.join("db").join(Self::key(url))
    }

    /// The folder of `commit`'s files.
    pub fn checkout_dir(&self, url: &str, commit: &str) -> PathBuf {
        self.dir.join("checkouts").join(Self::key(url)).join(commit)
    }

    /// The commit that `want` means in the repository at `url`, and the tag it was found by for
    /// a `version`.
    pub fn resolve(&self, url: &str, want: Want, fetch: Fetch) -> Result<String, String> {
        let db = self.db(url);
        let answer = |db: &Path| match want {
            Want::Rev(rev) => rev_parse(db, rev),
            Want::Version(req) => newest_tag(db, req),
        };
        if fetch == Fetch::WhenMissing
            && db.is_dir()
            && let Some(commit) = answer(&db)?
        {
            return Ok(commit);
        }
        self.fetch(url, &db)?;
        answer(&db)?.ok_or_else(|| match want {
            Want::Rev(rev) => format!("{url} has no branch, tag or commit `{rev}`"),
            Want::Version(req) => {
                format!("{url} has no tag `vX.Y.Z` or `X.Y.Z` that version \"{req}\" admits")
            }
        })
    }

    /// The folder of `commit`'s files, checked out first if the cache does not have it.
    pub fn checkout(&self, url: &str, commit: &str) -> Result<PathBuf, String> {
        let dir = self.checkout_dir(url, commit);
        if dir.is_dir() {
            return Ok(dir);
        }
        let db = self.db(url);
        if !(db.is_dir() && has_commit(&db, commit)?) {
            self.fetch(url, &db)?;
            if !has_commit(&db, commit)? {
                return Err(format!("{url} has no commit {commit}"));
            }
        }
        let parent = dir.parent().expect("a checkout is in a folder");
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        let partial = parent.join(format!(".{commit}.{}", std::process::id()));
        let _ = fs::remove_dir_all(&partial);
        let made = (|| {
            git(
                None,
                [
                    OsStr::new("clone"),
                    OsStr::new("--quiet"),
                    OsStr::new("--no-checkout"),
                    OsStr::new("--"),
                    db.as_os_str(),
                    partial.as_os_str(),
                ],
            )?;
            git(
                Some(&partial),
                [
                    "-c",
                    "advice.detachedHead=false",
                    "checkout",
                    "--quiet",
                    "--detach",
                    commit,
                ]
                .map(OsStr::new),
            )?;
            // The files are what a build reads; the history is in the cached repository.
            fs::remove_dir_all(partial.join(".git"))
                .map_err(|e| format!("{}: {e}", partial.display()))?;
            match fs::rename(&partial, &dir) {
                Ok(()) => Ok(()),
                // Another run made it first: its files are the same commit's.
                Err(_) if dir.is_dir() => Ok(()),
                Err(e) => Err(format!("{}: {e}", dir.display())),
            }
        })();
        if made.is_err() {
            let _ = fs::remove_dir_all(&partial);
        }
        made.map(|()| dir)
    }

    /// Bring the cached repository of `url` up to date: every branch and tag.
    fn fetch(&self, url: &str, db: &Path) -> Result<(), String> {
        if self.offline {
            return Err(format!(
                "{url} would be fetched, and --offline was given: run once without --offline to fill the cache"
            ));
        }
        if !db.is_dir() {
            fs::create_dir_all(db).map_err(|e| format!("{}: {e}", db.display()))?;
            git(
                None,
                [
                    OsStr::new("init"),
                    OsStr::new("--quiet"),
                    OsStr::new("--bare"),
                    db.as_os_str(),
                ],
            )?;
        }
        git(
            Some(db),
            [
                "fetch",
                "--quiet",
                "--force",
                "--prune",
                "--",
                url,
                "+refs/heads/*:refs/heads/*",
                "+refs/tags/*:refs/tags/*",
            ]
            .map(OsStr::new),
        )
        .map_err(|e| format!("fetching {url}: {e}"))?;
        Ok(())
    }
}

/// Run git in `dir` (or here), and give its standard output, or its standard error as the error.
fn git<I, S>(dir: Option<&Path>, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    if let Some(dir) = dir {
        command.arg("-C").arg(dir);
    }
    let output = command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            format!("git could not be run ({e}): a dependency by `git` needs git installed")
        })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn has_commit(db: &Path, commit: &str) -> Result<bool, String> {
    Ok(rev_parse(db, commit)?.is_some_and(|c| c == commit))
}

/// The commit `rev` names in `db`, or `None` when it names none.
fn rev_parse(db: &Path, rev: &str) -> Result<Option<String>, String> {
    let spec = format!("{rev}^{{commit}}");
    match git(
        Some(db),
        [
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &spec,
        ],
    ) {
        Ok(out) => Ok(Some(out.trim().to_string())),
        Err(e) if e.is_empty() => Ok(None),
        Err(e) => Err(e),
    }
}

/// The commit of the newest tag in `db` whose version `req` admits.
fn newest_tag(db: &Path, req: &semver::VersionReq) -> Result<Option<String>, String> {
    let tags = git(
        Some(db),
        ["for-each-ref", "--format=%(refname:strip=2)", "refs/tags"],
    )?;
    let best = tags
        .lines()
        .filter_map(|tag| {
            let version = semver::Version::parse(tag.strip_prefix('v').unwrap_or(tag)).ok()?;
            req.matches(&version).then_some((version, tag))
        })
        .max_by(|a, b| a.0.cmp(&b.0));
    match best {
        Some((_, tag)) => rev_parse(db, &format!("refs/tags/{tag}")),
        None => Ok(None),
    }
}
