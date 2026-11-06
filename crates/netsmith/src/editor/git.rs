//! What the editor asks git (ADR 0059), through the `git` command line: no library, no
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
/// How many untracked files have their lines counted, how many bytes are read for that in all,
/// and how many are listed.
const MAX_UNTRACKED_COUNTED: usize = 500;
const MAX_UNTRACKED_TOTAL: u64 = 32 * 1024 * 1024;
const MAX_UNTRACKED_LISTED: usize = 5000;

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
        // The branch that is checked out is the current one whatever kind it is listed as: the
        // default-branch guess lists `main` before the branch listing does.
        let current = current || (kind != GitRefKind::Head && branch.as_deref() == Some(&name));
        if let Some(listed) = refs.iter_mut().find(|r| r.name == name) {
            listed.current |= current;
        } else {
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
            push(name, GitRefKind::Branch, &c, false);
        }
        for (name, c) in listing("refs/tags", "-creatordate", MAX_TAGS) {
            push(name, GitRefKind::Tag, &c, false);
        }
    }
    GitInfo {
        available: true,
        reason: None,
        root: Some(place.root.to_string_lossy().into_owned()),
        project_dir: Some(place.prefix.clone()),
        branch,
        head,
        dirty: dirty(dir, &[]),
        project_dirty: project_dirty(dir),
        refs,
        commits,
    }
}

/// A folder made for one comparison and removed when it is dropped. A server that is killed
/// (`kill -9`, the power) leaves its folder behind, and so may Ctrl-C (the operating system
/// ends the process without running destructors): [`sweep_scratch`] removes such folders when
/// the next editor starts.
#[derive(Debug)]
pub struct Scratch(PathBuf);

/// What the names of the scratch folders start with; the process's id follows.
const SCRATCH_PREFIX: &str = "netsmith-compare-";

impl Scratch {
    pub fn new() -> std::io::Result<Scratch> {
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let dir = std::env::temp_dir().join(format!(
            "{SCRATCH_PREFIX}{}-{}-{nanos}",
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

/// How old a scratch folder is before it is removed where it cannot be told whether the process
/// that made it still runs (a system with no `/proc`).
const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

/// Whether the process `pid` is running, if this system can say (Linux: `/proc/<pid>`).
fn process_runs(proc: &Path, pid: u32) -> Option<bool> {
    proc.is_dir().then(|| proc.join(pid.to_string()).exists())
}

/// Removes the scratch folders in `temp` whose process is gone: the folders a killed editor left.
/// A folder of a process that runs (this one included) is never touched. Where processes cannot
/// be looked up (`proc` is not a folder), one that is older than a day is removed. Gives how many
/// folders were removed. Only folders (never links) whose names are exactly ours are looked at.
pub fn sweep_scratch_in(temp: &Path, proc: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(temp) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix(SCRATCH_PREFIX) else {
            continue;
        };
        let mut parts = rest.split('-');
        let (Some(Ok(pid)), Some(count), Some(nanos), None) = (
            parts.next().map(str::parse::<u32>),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            continue;
        };
        if count.parse::<u64>().is_err() || nanos.parse::<u64>().is_err() {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !meta.is_dir() || pid == std::process::id() {
            continue;
        }
        let gone = match process_runs(proc, pid) {
            Some(runs) => !runs,
            None => meta
                .modified()
                .ok()
                .and_then(|m| m.elapsed().ok())
                .is_some_and(|age| age > STALE_AFTER),
        };
        if gone && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// [`sweep_scratch_in`] for the system's temporary folder. Called when the editor starts.
pub fn sweep_scratch() -> usize {
    sweep_scratch_in(&std::env::temp_dir(), Path::new("/proc"))
}

/// The most a file of a ref may weigh. A bigger one is not read: it is not a source, and a
/// file that size read into memory by the analysis is how a hostile ref would stop the server.
pub const MAX_REF_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// The most the files of a ref may weigh together, before they are written anywhere.
const MAX_REF_TREE_BYTES: u64 = 256 * 1024 * 1024;
/// The folders of a project that are made by the tools or by other programs, never sources:
/// left out when a ref is read (at the top of the project, and `node_modules` at any depth).
const NOT_SOURCES: [&str; 3] = ["out", "target", "node_modules"];

/// Folders of tools and editors (`.claude`, `.agents`, `.git`, ...): never sources of a project. A link in one of them (the
/// `.claude/skills` link `netsmith init` writes) is removed like any other, but is not what the analysis would miss, so the answer
/// does not name it.
const TOOL_FOLDERS: [&str; 7] = [
    ".claude", ".agents", ".git", ".github", ".vscode", ".idea", ".cursor",
];

/// Whether a path of a project (with `/`, relative to its folder or the scratch folder) is one the analysis may have wanted: it is
/// not inside a folder of a tool.
fn matters(path: &str) -> bool {
    !path.split('/').any(|part| TOOL_FOLDERS.contains(&part))
}

/// `text` with the paths of the server's scratch folder (and of the project's folder inside it) taken out of it, so that what a person
/// reads names `src/main.nsm` and not `/tmp/netsmith-compare-1234-0-5678/src/main.nsm`. `folders` are tried in the order given, so the
/// deepest goes first; each is also tried as the folder's canonical path, the form an error from the disk may use.
pub fn project_relative(text: &str, folders: &[&Path]) -> String {
    let mut out = text.to_string();
    for folder in folders {
        let mut forms = vec![folder.to_string_lossy().into_owned()];
        if let Ok(real) = std::fs::canonicalize(folder) {
            let real = real.to_string_lossy().into_owned();
            if !forms.contains(&real) {
                forms.push(real);
            }
        }
        for form in forms {
            let form = form.trim_end_matches('/');
            if form.is_empty() {
                continue;
            }
            out = out.replace(&format!("{form}/"), "").replace(form, ".");
        }
    }
    out
}

/// What was not taken from a ref, and why, so that a file that is missing says so.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// Symbolic links (a ref's links are never followed: they may point anywhere on this
    /// machine).
    pub links: Vec<String>,
    /// Files that are not regular files (a device, a socket, a pipe).
    pub special: Vec<String>,
    /// Files above [`MAX_REF_FILE_BYTES`], with their sizes.
    pub too_big: Vec<(String, u64)>,
}

impl Skipped {
    /// A sentence for the answer of a ref that could not be read, or none if nothing that matters was left out: a link or a file in a
    /// folder of a tool (`.claude`, `.agents`, `.git`) is removed all the same, and not named (it is not what the analysis misses).
    /// The names are the project's own paths, never the server's scratch folder.
    pub fn describe(&self) -> Option<String> {
        let matter = Skipped {
            links: self.links.iter().filter(|n| matters(n)).cloned().collect(),
            special: self
                .special
                .iter()
                .filter(|n| matters(n))
                .cloned()
                .collect(),
            too_big: self
                .too_big
                .iter()
                .filter(|(n, _)| matters(n))
                .cloned()
                .collect(),
        };
        matter.describe_all()
    }

    fn describe_all(&self) -> Option<String> {
        let names = |list: &[String]| {
            let shown: Vec<_> = list.iter().take(5).map(|n| format!("`{n}`")).collect();
            let more = list.len().saturating_sub(5);
            if more > 0 {
                format!("{} and {more} more", shown.join(", "))
            } else {
                shown.join(", ")
            }
        };
        let mut parts = Vec::new();
        if !self.links.is_empty() {
            parts.push(format!(
                "left out as symbolic links, which are never followed: {}",
                names(&self.links)
            ));
        }
        if !self.special.is_empty() {
            parts.push(format!(
                "left out as not regular files: {}",
                names(&self.special)
            ));
        }
        if !self.too_big.is_empty() {
            let list: Vec<String> = self
                .too_big
                .iter()
                .map(|(n, size)| format!("{n} ({} MB)", size.div_ceil(1024 * 1024)))
                .collect();
            parts.push(format!(
                "left out for being over {} MB: {}",
                MAX_REF_FILE_BYTES / (1024 * 1024),
                names(&list)
            ));
        }
        (!parts.is_empty()).then(|| format!("in the ref, files were {}", parts.join("; ")))
    }
}

/// Why a ref could not be written out.
#[derive(Debug)]
pub enum ExtractError {
    /// The ref is not something to read (too big): said in the answer.
    Refused(String),
    /// git, tar or the disk failed.
    Broken(String),
}

/// Removes from `dir` every symbolic link, every file that is not a regular file and every file
/// above [`MAX_REF_FILE_BYTES`], below it at any depth, and says what it removed. It looks at
/// each entry without following links, so a link to a folder is removed and not entered.
pub fn sanitize(dir: &Path) -> Result<Skipped, String> {
    let mut skipped = Skipped::default();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(folder) = pending.pop() {
        let entries =
            std::fs::read_dir(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("{}: {e}", folder.display()))?;
            let path = entry.path();
            let shown = path
                .strip_prefix(dir)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();
            let meta =
                std::fs::symlink_metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let kind = meta.file_type();
            let remove = |path: &Path| {
                std::fs::remove_file(path)
                    .map_err(|e| format!("a file of the ref could not be removed: {e}"))
            };
            if kind.is_symlink() {
                remove(&path)?;
                skipped.links.push(shown);
            } else if kind.is_dir() {
                pending.push(path);
            } else if !kind.is_file() {
                remove(&path)?;
                skipped.special.push(shown);
            } else if meta.len() > MAX_REF_FILE_BYTES {
                remove(&path)?;
                skipped.too_big.push((shown, meta.len()));
            }
        }
    }
    skipped.links.sort();
    skipped.special.sort();
    skipped.too_big.sort();
    Ok(skipped)
}

/// The pathspec `git archive` takes for the files of a project: its folder (`prefix`, empty for
/// the top of the work tree) without the folders [`NOT_SOURCES`].
fn archive_pathspec(prefix: &str) -> Vec<String> {
    let mut spec = vec![if prefix.is_empty() { "." } else { prefix }.to_string()];
    for folder in NOT_SOURCES {
        let at = if prefix.is_empty() {
            folder.to_string()
        } else {
            format!("{prefix}/{folder}")
        };
        spec.push(format!(":(exclude,literal){at}"));
    }
    spec.push(":(exclude,glob)**/node_modules/**".to_string());
    spec
}

/// Whether a path of the work tree is in a folder that [`archive_pathspec`] leaves out.
fn left_out(path: &str, prefix: &str) -> bool {
    let inside = if prefix.is_empty() {
        path
    } else {
        path.strip_prefix(prefix)
            .and_then(|p| p.strip_prefix('/'))
            .unwrap_or(path)
    };
    let mut parts = inside.split('/');
    let first = parts.next().unwrap_or("");
    (first == "out" || first == "target" || first == "node_modules")
        || inside.split('/').any(|p| p == "node_modules")
}

/// The total size of the files `git archive` would write for `spec`, from the tree's own sizes.
fn tree_size(root: &Path, hash: &str, prefix: &str) -> Result<u64, String> {
    let mut args = vec!["ls-tree", "-r", "-l", "-z", hash];
    if !prefix.is_empty() {
        args.extend(["--", prefix]);
    }
    let text = run(root, &args).map_err(|e| e.message())?;
    let mut total = 0u64;
    for record in text.split('\0').filter(|r| !r.is_empty()) {
        let Some((meta, path)) = record.split_once('\t') else {
            continue;
        };
        if left_out(path, prefix) {
            continue;
        }
        let size = meta
            .split_whitespace()
            .nth(3)
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        // A file over the cap is not written to stay, but it is written before it is removed.
        total = total.saturating_add(size);
    }
    Ok(total)
}

/// Write the files of the project folder `prefix` (everything, if it is empty, except the
/// folders [`NOT_SOURCES`]) as they are at `hash` into `into`, below their paths in the work
/// tree: `git archive | tar -x`. Nothing in the work tree or the index is touched. Afterwards
/// every symbolic link, every file that is not a regular file and every file above
/// [`MAX_REF_FILE_BYTES`] is removed from `into` ([`sanitize`]), so that what reads the folder
/// can never be led outside it or into a file that does not end. A ref whose files add up to
/// more than 256 MB is refused before anything is written.
pub fn extract_project(
    root: &Path,
    hash: &str,
    prefix: &str,
    into: &Path,
) -> Result<Skipped, ExtractError> {
    let size = tree_size(root, hash, prefix).map_err(ExtractError::Broken)?;
    if size > MAX_REF_TREE_BYTES {
        return Err(ExtractError::Refused(format!(
            "the project's files at this ref weigh {} MB, over the {} MB that are read from a ref",
            size.div_ceil(1024 * 1024),
            MAX_REF_TREE_BYTES / (1024 * 1024)
        )));
    }
    extract(root, hash, &archive_pathspec(prefix), into)
}

/// [`extract_project`] for the named files only (a single source and the files beside it).
pub fn extract_files(
    root: &Path,
    hash: &str,
    files: &[String],
    into: &Path,
) -> Result<Skipped, ExtractError> {
    extract(root, hash, files, into)
}

fn extract(
    root: &Path,
    hash: &str,
    pathspec: &[String],
    into: &Path,
) -> Result<Skipped, ExtractError> {
    let broken = ExtractError::Broken;
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
        .map_err(|e| {
            broken(match e.kind() {
                std::io::ErrorKind::NotFound => Fault::Missing.message(),
                _ => format!("git could not be started: {e}"),
            })
        })?;
    let tar = Command::new("tar")
        .args(["-x", "--no-same-owner", "--no-same-permissions", "-C"])
        .arg(into)
        .stdin(
            archive
                .stdout
                .take()
                .ok_or_else(|| broken("git archive has no output".to_string()))?,
        )
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn();
    let tar = match tar {
        Ok(tar) => tar,
        Err(e) => {
            let _ = archive.kill();
            let _ = archive.wait();
            return Err(broken(match e.kind() {
                std::io::ErrorKind::NotFound => {
                    "`tar` is not installed, or is not on the PATH".to_string()
                }
                _ => format!("tar could not be started: {e}"),
            }));
        }
    };
    // git's stderr is read as it comes: a git that says a lot while tar is still reading would
    // otherwise wait for a pipe that nobody empties.
    let said = archive.stderr.take().map(|mut err| {
        std::thread::spawn(move || {
            let mut said = String::new();
            let _ = err.by_ref().take(1 << 20).read_to_string(&mut said);
            // Whatever is left is read and dropped, so that git is never blocked on it.
            let _ = std::io::copy(&mut err, &mut std::io::sink());
            said
        })
    });
    let tar = tar
        .wait_with_output()
        .map_err(|e| broken(format!("tar failed: {e}")))?;
    let said = said.and_then(|t| t.join().ok()).unwrap_or_default();
    let status = archive
        .wait()
        .map_err(|e| broken(format!("git archive failed: {e}")))?;
    if !status.success() {
        return Err(broken(format!(
            "git archive of {hash} failed: {}",
            said.trim().lines().next().unwrap_or("no message")
        )));
    }
    if !tar.status.success() {
        return Err(broken(format!(
            "tar failed: {}",
            String::from_utf8_lossy(&tar.stderr).trim()
        )));
    }
    sanitize(into).map_err(broken)
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

/// The lines of a text file, counted as it is read in chunks (0 for a binary file, a file that
/// is not a regular file, a link, a file over `MAX_UNTRACKED_BYTES` or more than `budget`
/// bytes); the bytes read are taken from `budget`.
fn count_lines(file: &Path, budget: &mut u64) -> usize {
    let Ok(meta) = std::fs::symlink_metadata(file) else {
        return 0;
    };
    if !meta.is_file() || meta.len() > MAX_UNTRACKED_BYTES || meta.len() > *budget {
        return 0;
    }
    let Ok(mut open) = std::fs::File::open(file) else {
        return 0;
    };
    // Reads no more than the length it had: a file that grows is not read for ever.
    let mut open = (&mut open).take(meta.len());
    let (mut lines, mut last, mut buf) = (0usize, b'\n', [0u8; 64 * 1024]);
    loop {
        match open.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                *budget = budget.saturating_sub(n as u64);
                if buf[..n].contains(&0) {
                    return 0;
                }
                lines += buf[..n].iter().filter(|&&b| b == b'\n').count();
                last = buf[n - 1];
            }
            Err(_) => return 0,
        }
    }
    lines + usize::from(last != b'\n')
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
    // Untracked files that git does not ignore (`out/` and `target/` are, in most projects), as
    // all added. Their lines are counted by reading each file in chunks, for the first
    // `MAX_UNTRACKED_COUNTED` files and as long as `MAX_UNTRACKED_TOTAL` bytes are not read in all;
    // a link is never read. Later files are listed with no lines counted.
    if let Ok(text) = run(dir, &["ls-files", "--others", "--exclude-standard"]) {
        let mut budget = MAX_UNTRACKED_TOTAL;
        for (listed, path) in text.lines().enumerate() {
            if listed == MAX_UNTRACKED_LISTED {
                error.get_or_insert_with(|| {
                    format!(
                        "only the first {MAX_UNTRACKED_LISTED} files that are not in git are listed"
                    )
                });
                break;
            }
            let lines = if listed < MAX_UNTRACKED_COUNTED {
                count_lines(&dir.join(path), &mut budget)
            } else {
                0
            };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scratch_folder_is_private_and_is_gone_when_it_is_dropped() {
        let (path, other) = {
            let scratch = Scratch::new().unwrap();
            let other = Scratch::new().unwrap();
            std::fs::write(scratch.path().join("a"), "x").unwrap();
            assert!(scratch.path().is_dir());
            assert_ne!(scratch.path(), other.path());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = std::fs::metadata(scratch.path())
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o077, 0, "only its owner can read it");
            }
            (scratch.path().to_path_buf(), other.path().to_path_buf())
        };
        assert!(!path.exists() && !other.exists());
    }

    #[test]
    fn a_commit_is_read_from_its_record() {
        let c = commit_of(
            "abc123\u{1f}abc\u{1f}A subject\u{1f}An Author\u{1f}2026-11-05T10:00:00+01:00",
        )
        .unwrap();
        assert_eq!((c.short.as_str(), c.subject.as_str()), ("abc", "A subject"));
        assert_eq!(c.date, "2026-11-05T10:00:00+01:00");
        assert!(commit_of("only\u{1f}two").is_none());
    }

    #[test]
    fn something_that_is_not_a_revision_is_refused_before_git_is_asked() {
        let nowhere = Path::new("/nonexistent-for-the-test");
        for rev in ["", "-p", "--output=x", "a\nb"] {
            assert_eq!(resolve(nowhere, rev).unwrap_err().status, 400, "{rev:?}");
        }
    }

    use netsmith_kicad::testlib::TempDir;

    fn dead_pid() -> u32 {
        // A process id above any the system hands out: no folder of it can be of a live process.
        u32::MAX - 1
    }

    #[test]
    fn the_sweep_removes_the_scratch_folders_of_dead_processes_and_nothing_else() {
        let temp = TempDir::new("sweep-temp");
        let proc = TempDir::new("sweep-proc");
        // A fake /proc with one live process, 4242.
        std::fs::create_dir(proc.join("4242")).unwrap();
        let make = |name: &str| {
            let dir = temp.join(name);
            std::fs::create_dir_all(dir.join("src")).unwrap();
            std::fs::write(dir.join("src/a.nsm"), "x").unwrap();
            dir
        };
        let dead = make(&format!("netsmith-compare-{}-0-123", dead_pid()));
        let dead_too = make("netsmith-compare-77-3-456789");
        let live = make("netsmith-compare-4242-0-123");
        let mine = make(&format!("netsmith-compare-{}-9-1", std::process::id()));
        // Not ours: a name that does not end as ours do, a plain file, a link, another prefix.
        let odd = make("netsmith-compare-77-x-1");
        let longer = make("netsmith-compare-77-1-2-3");
        let other = make("netsmith-comparison-77-1-2");
        let file = temp.join("netsmith-compare-78-0-1");
        std::fs::write(&file, "x").unwrap();
        let outside = TempDir::new("sweep-outside");
        std::fs::write(outside.join("keep"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&*outside, temp.join("netsmith-compare-79-0-1")).unwrap();

        assert_eq!(sweep_scratch_in(&temp, &proc), 2);
        assert!(!dead.exists() && !dead_too.exists(), "dead processes'");
        for kept in [&live, &mine, &odd, &longer, &other, &file] {
            assert!(kept.exists(), "{} was removed", kept.display());
        }
        assert!(outside.join("keep").exists(), "a link is not followed");
        // Again: nothing more to remove.
        assert_eq!(sweep_scratch_in(&temp, &proc), 0);
    }

    #[test]
    fn where_processes_cannot_be_listed_only_a_folder_older_than_a_day_is_removed() {
        let temp = TempDir::new("sweep-age-temp");
        let no_proc = temp.join("no-such-proc");
        let fresh = temp.join("netsmith-compare-77-0-1");
        std::fs::create_dir(&fresh).unwrap();
        assert_eq!(sweep_scratch_in(&temp, &no_proc), 0);
        assert!(fresh.exists(), "a new folder may be a running compare's");
        let old = temp.join("netsmith-compare-78-0-1");
        std::fs::create_dir(&old).unwrap();
        let two_days = SystemTime::now() - std::time::Duration::from_secs(2 * 24 * 60 * 60);
        std::fs::File::open(&old)
            .unwrap()
            .set_modified(two_days)
            .unwrap();
        assert_eq!(sweep_scratch_in(&temp, &no_proc), 1);
        assert!(!old.exists() && fresh.exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_links_and_the_big_files_of_an_extracted_ref_are_removed_and_named() {
        let outside = TempDir::new("sanitize-outside");
        std::fs::write(outside.join("secret"), "the outside").unwrap();
        let dir = TempDir::new("sanitize-dir");
        std::fs::create_dir_all(dir.join("src/deep")).unwrap();
        std::fs::write(dir.join("src/main.nsm"), "board B {}").unwrap();
        std::os::unix::fs::symlink(outside.join("secret"), dir.join("src/led.nsm")).unwrap();
        std::os::unix::fs::symlink(&*outside, dir.join("src/deep/folder")).unwrap();
        std::os::unix::fs::symlink("/nonexistent/target", dir.join("dangling")).unwrap();
        let big = std::fs::File::create(dir.join("big.csv")).unwrap();
        big.set_len(MAX_REF_FILE_BYTES + 1).unwrap();
        let skipped = sanitize(&dir).unwrap();
        assert_eq!(
            skipped.links,
            ["dangling", "src/deep/folder", "src/led.nsm"]
        );
        assert_eq!(
            skipped.too_big,
            [("big.csv".to_string(), MAX_REF_FILE_BYTES + 1)]
        );
        assert!(skipped.special.is_empty());
        assert!(dir.join("src/main.nsm").is_file());
        for gone in ["src/led.nsm", "src/deep/folder", "dangling", "big.csv"] {
            assert!(
                std::fs::symlink_metadata(dir.join(gone)).is_err(),
                "{gone} is still there"
            );
        }
        // Where the links pointed is untouched.
        assert_eq!(
            std::fs::read_to_string(outside.join("secret")).unwrap(),
            "the outside"
        );
        let said = skipped.describe().unwrap();
        assert!(
            said.contains("symbolic links, which are never followed: `dangling`")
                && said.contains("`big.csv (9 MB)`")
                && said.contains("over 8 MB"),
            "{said}"
        );
        assert_eq!(Skipped::default().describe(), None);
    }

    #[test]
    fn the_note_names_what_the_analysis_would_miss_and_not_the_links_of_the_tools() {
        let skipped = Skipped {
            links: vec![
                ".claude/skills".to_string(),
                ".agents/skills/x/SKILL.md".to_string(),
                "src/main.nsm".to_string(),
                "sub/.git/hooks".to_string(),
            ],
            special: vec![".claude/pipe".to_string()],
            too_big: vec![(".agents/big.bin".to_string(), MAX_REF_FILE_BYTES + 1)],
        };
        let said = skipped.describe().unwrap();
        assert!(said.contains("`src/main.nsm`"), "{said}");
        for tool in [".claude", ".agents", ".git", "pipe", "big.bin"] {
            assert!(!said.contains(tool), "{tool} is in the note: {said}");
        }
        // Everything is still removed and recorded (the safety is not the note's business).
        assert_eq!(skipped.links.len(), 4);
        // Only tool folders: nothing to say.
        let tools = Skipped {
            links: vec![".claude/skills".to_string(), ".git/config".to_string()],
            ..Skipped::default()
        };
        assert_eq!(tools.describe(), None);
        // A name that only looks like a tool's folder is a source's.
        let like = Skipped {
            links: vec![
                "src/.claude-notes/a.nsm".to_string(),
                "claude/skills".to_string(),
            ],
            ..Skipped::default()
        };
        let said = like.describe().unwrap();
        assert!(
            said.contains("`claude/skills`") && said.contains("`src/.claude-notes/a.nsm`"),
            "{said}"
        );
    }

    #[test]
    fn a_message_names_the_projects_paths_and_never_the_scratch_folder() {
        let scratch = TempDir::new("project-relative");
        std::fs::create_dir_all(scratch.join("proj/src")).unwrap();
        let tree = scratch.join("proj");
        let message = format!(
            "cannot read {}/src/main.nsm: no such file (in {})",
            tree.display(),
            scratch.display()
        );
        let shown = project_relative(&message, &[tree.as_path(), scratch.as_ref()]);
        assert_eq!(shown, "cannot read src/main.nsm: no such file (in .)");
        assert!(!shown.contains("project-relative"), "{shown}");
        // Nothing of the folders in it: the text is unchanged.
        assert_eq!(
            project_relative("the project has no netsmith.toml", &[tree.as_path()]),
            "the project has no netsmith.toml"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_pipe_in_an_extracted_ref_is_removed_without_being_opened() {
        let dir = TempDir::new("sanitize-fifo");
        let made = Command::new("mkfifo").arg(dir.join("pipe")).status();
        if !made.is_ok_and(|s| s.success()) {
            eprintln!("skipped: mkfifo is not available");
            return;
        }
        let skipped = sanitize(&dir).unwrap();
        assert_eq!(skipped.special, ["pipe"]);
        assert!(std::fs::symlink_metadata(dir.join("pipe")).is_err());
    }

    #[test]
    fn a_project_at_the_top_leaves_out_what_the_tools_write_and_a_subfolder_its_own() {
        assert_eq!(
            archive_pathspec(""),
            [
                ".",
                ":(exclude,literal)out",
                ":(exclude,literal)target",
                ":(exclude,literal)node_modules",
                ":(exclude,glob)**/node_modules/**"
            ]
        );
        assert_eq!(archive_pathspec("boards/led")[0], "boards/led");
        assert_eq!(
            archive_pathspec("boards/led")[1],
            ":(exclude,literal)boards/led/out"
        );
        assert!(left_out("out/led.kicad_pcb", ""));
        assert!(left_out("web/node_modules/x/index.js", ""));
        assert!(left_out("boards/led/target/x", "boards/led"));
        assert!(!left_out("src/out.nsm", ""));
        assert!(!left_out("src/outputs/a.nsm", ""));
        // Not at the top of the project: an `out` below `src` is a module folder.
        assert!(!left_out("src/out/a.nsm", ""));
    }

    #[test]
    fn untracked_lines_are_counted_in_chunks_and_never_through_a_link() {
        let dir = TempDir::new("count-lines");
        let mut budget = 1 << 20;
        let write = |name: &str, bytes: &[u8]| {
            std::fs::write(dir.join(name), bytes).unwrap();
            dir.join(name)
        };
        assert_eq!(count_lines(&write("a", b"one\ntwo\nthree"), &mut budget), 3);
        assert_eq!(count_lines(&write("b", b"one\ntwo\n"), &mut budget), 2);
        assert_eq!(count_lines(&write("c", b""), &mut budget), 0);
        assert_eq!(count_lines(&write("d", b"bin\0ary\n"), &mut budget), 0);
        let long = "line\n".repeat(50_000);
        assert_eq!(
            count_lines(&write("e", long.as_bytes()), &mut budget),
            50_000
        );
        // The budget is spent by what was read, and a file over it is not read.
        let mut tiny = 4;
        assert_eq!(count_lines(&dir.join("e"), &mut tiny), 0);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.join("a"), dir.join("link")).unwrap();
            assert_eq!(count_lines(&dir.join("link"), &mut budget), 0);
        }
    }
}
