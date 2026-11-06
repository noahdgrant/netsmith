//! What the editor asks git (ADR 0051), through the `git` command line: no library, no
//! writes. Each command is told not to take the index's lock (`GIT_OPTIONAL_LOCKS=0`), so a
//! `git status` here never touches the repository, and a ref is read out with `git archive`
//! into a scratch folder, never checked out.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use netsmith_editor::{
    ApiError,
    api::{GitCommit, GitInfo, GitRef, GitRefKind, SourceDiff, SourceFile, SourceStatus},
};

/// The most local branches `GET /api/git` lists.
const MAX_BRANCHES: usize = 50;
/// The most tags it lists, the newest first.
const MAX_TAGS: usize = 30;
/// How many commits it lists.
const COMMITS: usize = 30;
/// A file of this size or more is not counted line by line when it is untracked.
const MAX_UNTRACKED_BYTES: u64 = 4 * 1024 * 1024;

/// Why a git command did not give an answer.
#[derive(Debug)]
pub enum Fault {
    /// There is no `git` to run.
    Missing,
    /// It ran and said no (`stderr`, trimmed).
    Failed(String),
}

impl Fault {
    pub fn message(&self) -> String {
        match self {
            Fault::Missing => "git is not installed, or is not on the PATH".to_string(),
            Fault::Failed(e) => e.clone(),
        }
    }
}

fn command(dir: &Path) -> Command {
    let mut git = Command::new("git");
    git.arg("--no-pager")
        .args(["-c", "core.quotepath=off"])
        .arg("-C")
        .arg(dir)
        // Another program's settings must not point git elsewhere.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C");
    git
}

/// Run `git <args>` in `dir` and give what it printed.
pub fn run(dir: &Path, args: &[&str]) -> Result<String, Fault> {
    let output = command(dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Fault::Missing,
            _ => Fault::Failed(format!("git could not be started: {e}")),
        })?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let said = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(Fault::Failed(if said.is_empty() {
            format!("git {} failed ({})", args.join(" "), output.status)
        } else {
            said
        }))
    }
}

/// Where a folder is in its work tree.
#[derive(Debug, Clone)]
pub struct Place {
    /// The top of the work tree.
    pub root: PathBuf,
    /// The folder's path in it, `a/b`, empty for the top.
    pub prefix: String,
}

/// The work tree that contains `dir`.
pub fn place(dir: &Path) -> Result<Place, String> {
    let text =
        run(dir, &["rev-parse", "--show-toplevel", "--show-prefix"]).map_err(|e| match e {
            Fault::Failed(said) if said.contains("not a git repository") => {
                "the project is not inside a git work tree".to_string()
            }
            other => other.message(),
        })?;
    let mut lines = text.lines();
    let root = lines
        .next()
        .filter(|l| !l.is_empty())
        .ok_or("git did not say where the work tree is")?;
    let prefix = lines.next().unwrap_or("").trim_end_matches('/').to_string();
    Ok(Place {
        root: PathBuf::from(root),
        prefix,
    })
}

const COMMIT_FORMAT: &str = "%H%x1f%h%x1f%s%x1f%an%x1f%aI";

fn commit_of(record: &str) -> Option<GitCommit> {
    let mut fields = record.split('\u{1f}');
    Some(GitCommit {
        hash: fields.next()?.trim().to_string(),
        short: fields.next()?.to_string(),
        subject: fields.next()?.to_string(),
        author: fields.next()?.to_string(),
        date: fields.next()?.trim().to_string(),
    })
}

/// The commits that `git log <args>` lists, newest first.
fn log(dir: &Path, args: &[&str]) -> Result<Vec<GitCommit>, Fault> {
    let format = format!("--format={COMMIT_FORMAT}%x1e");
    let mut all = vec!["log", format.as_str()];
    all.extend_from_slice(args);
    // So that a name that is also a file is a revision.
    all.push("--");
    let text = run(dir, &all)?;
    Ok(text
        .split('\u{1e}')
        .filter_map(|r| commit_of(r.trim_start_matches('\n')))
        .collect())
}

/// What a revision names, as a commit. `404` if it names nothing, `400` if it is not a
/// revision at all (an option, a control character).
pub fn resolve(dir: &Path, rev: &str) -> Result<GitCommit, ApiError> {
    if rev.is_empty() || rev.starts_with('-') || rev.chars().any(char::is_control) {
        return Err(ApiError::bad_request(format!(
            "`{rev}` is not a git revision (a branch, a tag, a commit or an expression such as `HEAD~3`)"
        )));
    }
    let spec = format!("{rev}^{{commit}}");
    let hash = match run(dir, &["rev-parse", "--verify", "--quiet", &spec]) {
        Ok(text) => text.trim().to_string(),
        Err(Fault::Missing) => {
            return Err(unavailable(Fault::Missing.message()));
        }
        Err(Fault::Failed(_)) => {
            return Err(ApiError::not_found(format!(
                "there is no commit, branch or tag `{rev}` in this repository"
            )));
        }
    };
    let found = log(dir, &["-1", &hash]).map_err(|e| unavailable(e.message()))?;
    found
        .into_iter()
        .next()
        .ok_or_else(|| ApiError::not_found(format!("there is no commit `{rev}`")))
}

/// The refusal for a project git cannot be asked about.
pub fn unavailable(reason: String) -> ApiError {
    ApiError::unprocessable(reason.clone(), "git_unavailable", &[("reason", &reason)])
}

fn nothing(reason: String) -> GitInfo {
    GitInfo {
        available: false,
        reason: Some(reason),
        root: None,
        project_dir: None,
        branch: None,
        head: None,
        dirty: false,
        project_dirty: false,
        refs: Vec::new(),
        commits: Vec::new(),
    }
}

fn dirty(dir: &Path, pathspec: &[&str]) -> bool {
    let mut args = vec!["status", "--porcelain", "--untracked-files=normal"];
    if !pathspec.is_empty() {
        args.push("--");
        args.extend_from_slice(pathspec);
    }
    run(dir, &args).is_ok_and(|text| !text.trim().is_empty())
}

/// Whether the folder `dir` (the project's) has changes that are not committed.
pub fn project_dirty(dir: &Path) -> bool {
    dirty(dir, &["."])
}

/// Whether `path` is a folder at `hash`.
pub fn is_tree(root: &Path, hash: &str, path: &str) -> bool {
    run(root, &["cat-file", "-t", &format!("{hash}:{path}")])
        .is_ok_and(|kind| kind.trim() == "tree")
}

/// `GET /api/git` for the folder `dir`.
pub fn info(dir: &Path) -> GitInfo {
    let place = match place(dir) {
        Ok(place) => place,
        Err(reason) => return nothing(reason),
    };
    let head = log(dir, &["-1", "HEAD"])
        .ok()
        .and_then(|c| c.into_iter().next());
    let branch = run(dir, &["symbolic-ref", "--quiet", "--short", "HEAD"])
        .ok()
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty());
    let commits = if head.is_some() {
        log(dir, &["-n", &COMMITS.to_string(), "HEAD"]).unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut refs: Vec<GitRef> = Vec::new();
    let mut push = |name: String, kind: GitRefKind, c: &GitCommit, current: bool| {
        if !refs.iter().any(|r| r.name == name) {
            refs.push(GitRef {
                name,
                kind,
                hash: c.hash.clone(),
                short: c.short.clone(),
                subject: c.subject.clone(),
                date: c.date.clone(),
                current,
            });
        }
    };
    if let Some(head) = &head {
        push("HEAD".to_string(), GitRefKind::Head, head, false);
        let at = |name: &str| {
            log(dir, &["-1", name])
                .ok()
                .and_then(|c| c.into_iter().next())
        };
        let upstream = run(
            dir,
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
        )
        .ok()
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty());
        if let Some(upstream) = upstream
            && let Some(c) = at(&upstream)
        {
            push(upstream, GitRefKind::Upstream, &c, false);
        }
        // The default branch: where the remote says its HEAD is, else what is usually it.
        let remote_head = run(
            dir,
            &[
                "symbolic-ref",
                "--quiet",
                "--short",
                "refs/remotes/origin/HEAD",
            ],
        )
        .ok()
        .map(|h| h.trim().to_string());
        let guesses = ["origin/main", "origin/master", "main", "master"].map(str::to_string);
        for name in remote_head.into_iter().chain(guesses) {
            if let Some(c) = at(&name) {
                push(name, GitRefKind::Default, &c, false);
                break;
            }
        }
        let listing = |pattern: &str, sort: &str, max: usize| -> Vec<(String, GitCommit)> {
            // The commit of a tag is the peeled object, which only an annotated tag has.
            let format = "%(refname:short)%1f%(*objectname)%1f%(objectname)";
            let sort = format!("--sort={sort}");
            let Ok(text) = run(
                dir,
                &[
                    "for-each-ref",
                    &sort,
                    &format!("--format={format}"),
                    pattern,
                ],
            ) else {
                return Vec::new();
            };
            text.lines()
                .take(max)
                .filter_map(|line| {
                    let mut f = line.split('\u{1f}');
                    let (name, peeled, object) = (f.next()?, f.next()?, f.next()?);
                    let hash = if peeled.is_empty() { object } else { peeled };
                    let c = log(dir, &["-1", hash]).ok()?.into_iter().next()?;
                    Some((name.to_string(), c))
                })
                .collect()
        };
        for (name, c) in listing("refs/heads", "-committerdate", MAX_BRANCHES) {
            let current = branch.as_deref() == Some(name.as_str());
            push(name, GitRefKind::Branch, &c, current);
        }
        for (name, c) in listing("refs/tags", "-creatordate", MAX_TAGS) {
            push(name, GitRefKind::Tag, &c, false);
        }
    }
    let project_pathspec: Vec<&str> = if place.prefix.is_empty() {
        Vec::new()
    } else {
        vec![place.prefix.as_str()]
    };
    GitInfo {
        available: true,
        reason: None,
        root: Some(place.root.to_string_lossy().into_owned()),
        project_dir: Some(place.prefix.clone()),
        branch,
        head,
        dirty: dirty(dir, &[]),
        project_dirty: dirty(dir, &project_pathspec),
        refs,
        commits,
    }
}

/// A folder made for one comparison and removed when it is dropped.
#[derive(Debug)]
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new() -> std::io::Result<Scratch> {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let dir = std::env::temp_dir().join(format!(
            "netsmith-compare-{}-{}-{nanos}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::SeqCst)
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(&dir)?;
        Ok(Scratch(dir))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Write the files of `pathspec` (everything, if there is none) as they are at `hash` into
/// `into`, below their paths in the work tree: `git archive | tar -x`. Nothing in the work
/// tree or the index is touched.
pub fn extract(root: &Path, hash: &str, pathspec: &[String], into: &Path) -> Result<(), String> {
    let mut archive = command(root);
    archive.args(["archive", "--format=tar", hash]);
    if !pathspec.is_empty() {
        archive.arg("--");
        archive.args(pathspec);
    }
    let mut archive = archive
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Fault::Missing.message(),
            _ => format!("git could not be started: {e}"),
        })?;
    let tar = Command::new("tar")
        .args(["-x", "--no-same-owner", "--no-same-permissions", "-C"])
        .arg(into)
        .stdin(archive.stdout.take().ok_or("git archive has no output")?)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let tar = match tar {
        Ok(tar) => tar,
        Err(e) => {
            let _ = archive.kill();
            let _ = archive.wait();
            return Err(match e.kind() {
                std::io::ErrorKind::NotFound => {
                    "`tar` is not installed, or is not on the PATH".to_string()
                }
                _ => format!("tar could not be started: {e}"),
            });
        }
    };
    let tar = tar
        .wait_with_output()
        .map_err(|e| format!("tar failed: {e}"))?;
    let mut said = String::new();
    if let Some(mut err) = archive.stderr.take() {
        let _ = err.read_to_string(&mut said);
    }
    let status = archive
        .wait()
        .map_err(|e| format!("git archive failed: {e}"))?;
    if !status.success() {
        return Err(format!(
            "git archive of {hash} failed: {}",
            said.trim().lines().next().unwrap_or("no message")
        ));
    }
    if !tar.status.success() {
        return Err(format!(
            "tar failed: {}",
            String::from_utf8_lossy(&tar.stderr).trim()
        ));
    }
    Ok(())
}

/// The names (full paths in the work tree) of the files of `dir` at `hash` whose names start
/// with `stem.`: a source and what sits beside it (its physical file, its layout).
pub fn beside(root: &Path, hash: &str, dir: &str, stem: &str) -> Result<Vec<String>, String> {
    let spec = if dir.is_empty() {
        ".".to_string()
    } else {
        format!("{dir}/")
    };
    let text =
        run(root, &["ls-tree", "--name-only", hash, "--", &spec]).map_err(|e| e.message())?;
    let prefix = format!("{stem}.");
    Ok(text
        .lines()
        .filter(|path| {
            path.rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .map(str::to_string)
        .collect())
}

/// The text of `path` (in the work tree) at `hash`, if it is a file there.
pub fn show(root: &Path, hash: &str, path: &str) -> Result<Option<String>, String> {
    let object = format!("{hash}:{path}");
    match run(root, &["cat-file", "-t", &object]) {
        Ok(kind) if kind.trim() == "blob" => {}
        Ok(_) | Err(Fault::Failed(_)) => return Ok(None),
        Err(Fault::Missing) => return Err(Fault::Missing.message()),
    }
    let output = command(root)
        .args(["cat-file", "blob", &object])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("git could not be started: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    String::from_utf8(output.stdout)
        .map(Some)
        .map_err(|_| format!("{path} is not text"))
}

/// What differs in `dir` between `hash` and the files on disk, as `git diff --stat` counts it
/// (and untracked files as all added).
pub fn source_diff(dir: &Path, hash: &str) -> SourceDiff {
    let mut files: Vec<SourceFile> = Vec::new();
    let mut error = None;
    // `--relative` with no value makes the paths relative to `dir`, and leaves out what is
    // outside it.
    let numstat = run(
        dir,
        &[
            "diff",
            "--numstat",
            "--no-renames",
            "--relative",
            hash,
            "--",
            ".",
        ],
    );
    let status = run(
        dir,
        &[
            "diff",
            "--name-status",
            "--no-renames",
            "--relative",
            hash,
            "--",
            ".",
        ],
    );
    match (numstat, status) {
        (Ok(numstat), Ok(status)) => {
            let kinds: std::collections::HashMap<&str, SourceStatus> = status
                .lines()
                .filter_map(|line| {
                    let (letter, path) = line.split_once('\t')?;
                    Some((
                        path,
                        match letter {
                            "A" => SourceStatus::Added,
                            "D" => SourceStatus::Deleted,
                            _ => SourceStatus::Modified,
                        },
                    ))
                })
                .collect();
            for line in numstat.lines() {
                let mut f = line.splitn(3, '\t');
                let (Some(added), Some(removed), Some(path)) = (f.next(), f.next(), f.next())
                else {
                    continue;
                };
                files.push(SourceFile {
                    path: path.to_string(),
                    status: kinds.get(path).copied().unwrap_or(SourceStatus::Modified),
                    // A binary file is `-`.
                    added: added.parse().unwrap_or(0),
                    removed: removed.parse().unwrap_or(0),
                });
            }
        }
        (Err(e), _) | (_, Err(e)) => error = Some(e.message()),
    }
    if let Ok(text) = run(dir, &["ls-files", "--others", "--exclude-standard"]) {
        for path in text.lines() {
            let file = dir.join(path);
            let lines = std::fs::metadata(&file)
                .ok()
                .filter(|m| m.is_file() && m.len() <= MAX_UNTRACKED_BYTES)
                .and_then(|_| std::fs::read(&file).ok())
                .map_or(0, |bytes| {
                    if bytes.contains(&0) {
                        0
                    } else {
                        bytes.iter().filter(|&&b| b == b'\n').count()
                            + usize::from(bytes.last().is_some_and(|&b| b != b'\n'))
                    }
                });
            files.push(SourceFile {
                path: path.to_string(),
                status: SourceStatus::Untracked,
                added: lines as u32,
                removed: 0,
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    SourceDiff {
        files_changed: files.len() as u32,
        added: files.iter().map(|f| f.added).sum(),
        removed: files.iter().map(|f| f.removed).sum(),
        files,
        error,
    }
}
