//! Comparing the project with a git ref (ADR 0059): the ref's files are read out of git into a
//! scratch folder, analysed by a `Workspace` of their own with the code the live project is,
//! reduced to a [`Digest`], and compared with the digest of the files on disk. What was read
//! of a ref is kept by commit hash.

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use netsmith_core::job::{self, Cancel};
use netsmith_editor::{
    AnalysisEvent, ApiError, Backend, Observer,
    api::{
        Compare, CompareHead, FileContent, GitCommit, GitInfo, PowerNode, PowerTree, Problem,
        Severity,
    },
};
use netsmith_solve::Catalogs;

use super::{
    Workspace,
    analysis::{BoardData, api_location},
    diff::{self, BoardDigest, Digest, InstanceDigest, NetDigest, PartDigest},
    git::{self, Place, Scratch},
    millis, power,
};
use crate::{
    locate,
    project::{MANIFEST, Project, SourceFile, Target},
    resolve::resolve_generics,
};

/// How many refs' readings are kept.
const KEPT: usize = 8;
/// How many compares may wait for the one that is reading a ref (they read one at a time: a
/// reading takes seconds to a minute). A compare that finds this many waiting is refused (429).
const MAX_WAITING: usize = 3;

/// The compares that wait for their turn to read a ref.
#[derive(Default)]
pub struct Queue(std::sync::atomic::AtomicUsize);

/// A place in the queue, given up when dropped.
pub struct Waiting<'a>(&'a Queue);

impl Queue {
    /// A place among the waiting, or none if `limit` are waiting already.
    fn join(&self, limit: usize) -> Option<Waiting<'_>> {
        use std::sync::atomic::Ordering;
        let mut waiting = self.0.load(Ordering::SeqCst);
        loop {
            if waiting >= limit {
                return None;
            }
            match self
                .0
                .compare_exchange(waiting, waiting + 1, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return Some(Waiting(self)),
                Err(now) => waiting = now,
            }
        }
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// What a ref came to when it was read: the project reduced to its digest, or why it could
/// not be read.
pub enum Reading {
    Digest(Digest),
    Failed {
        error: String,
        problems: Vec<Problem>,
    },
}

/// The readings kept, by commit hash, the oldest first out.
#[derive(Default)]
pub struct Cache {
    readings: HashMap<String, Arc<Reading>>,
    order: VecDeque<String>,
}

impl Cache {
    fn get(&self, hash: &str) -> Option<Arc<Reading>> {
        self.readings.get(hash).cloned()
    }

    fn put(&mut self, hash: &str, reading: Arc<Reading>) {
        if self.readings.insert(hash.to_string(), reading).is_none() {
            self.order.push_back(hash.to_string());
        }
        while self.order.len() > KEPT {
            if let Some(old) = self.order.pop_front() {
                self.readings.remove(&old);
            }
        }
    }
}

fn tell(observe: &Observer, stage: &str, done: u32, total: u32, message: String) {
    observe(AnalysisEvent::Progress {
        stage: stage.to_string(),
        board: String::new(),
        done,
        total,
        message,
        work: None,
    });
}

impl Workspace {
    /// The folder git is asked about: the project's, else the folder of the file.
    fn git_dir(&self) -> PathBuf {
        let dir = match (&self.target.project, self.target.files.first()) {
            (Some(project), _) => project.root.clone(),
            (None, Some(file)) => file
                .path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf),
            (None, None) => PathBuf::from("."),
        };
        std::path::absolute(&dir).unwrap_or(dir)
    }

    pub(super) fn git_info(&self) -> GitInfo {
        git::info(&self.git_dir())
    }

    pub(super) fn git_file_at(&self, rev: &str, path: &str) -> Result<FileContent, ApiError> {
        let inside = !path.is_empty()
            && !path.contains(['\\', '\0'])
            && Path::new(path)
                .components()
                .all(|c| matches!(c, Component::Normal(_)));
        let text_file = [".nsm", ".toml", ".csv"].iter().any(|e| path.ends_with(e));
        if !inside || !text_file {
            return Err(ApiError::forbidden(format!(
                "`{path}` is not a source file of the project (a path inside it, ending .nsm, .toml or .csv)"
            )));
        }
        let dir = self.git_dir();
        let place = git::place(&dir).map_err(git::unavailable)?;
        let commit = git::resolve(&dir, rev)?;
        let full = join(&place.prefix, path);
        match git::show(&place.root, &commit.hash, &full) {
            Ok(Some(text)) => Ok(FileContent {
                path: path.to_string(),
                text,
                stamp: commit.hash,
                readonly: true,
            }),
            Ok(None) => Err(ApiError::not_found(format!(
                "`{path}` is not a file of the project at {}",
                commit.short
            ))),
            Err(e) => Err(ApiError::internal(e)),
        }
    }

    /// Compare `rev` with the files on disk, or with the commit `against` when there is one.
    pub(super) fn compare_with(
        &self,
        rev: &str,
        against: Option<&str>,
        board: Option<&str>,
        observe: &Observer,
        cancel: &Cancel,
    ) -> Result<Compare, ApiError> {
        let started = Instant::now();
        let dir = self.git_dir();
        let place = git::place(&dir).map_err(git::unavailable)?;
        let base = git::resolve(&dir, rev)?;
        // Both names are resolved before anything is read: a name that is no commit is a 404 at
        // once, not after the first ref has been analysed.
        let other = against.map(|a| git::resolve(&dir, a)).transpose()?;
        let (reading, base_cached) = self.read_ref(&place, &base, observe, cancel)?;
        let head_reading = match &other {
            Some(commit) if !matches!(&*reading, Reading::Failed { .. }) => {
                Some(self.read_ref(&place, commit, observe, cancel)?)
            }
            _ => None,
        };
        // Between the reading of the ref(s) and the reading of the files on disk.
        job::checkpoint();
        let (head_commit, dirty) = match &other {
            Some(commit) => (Some(commit.clone()), false),
            None => (git::resolve(&dir, "HEAD").ok(), git::project_dirty(&dir)),
        };
        let cached = base_cached && head_reading.as_ref().is_none_or(|(_, c)| *c);
        let compared_with = other
            .as_ref()
            .map_or("the files on disk", |c| c.short.as_str());
        tell(
            observe,
            "diff",
            0,
            1,
            format!("comparing {} with {compared_with}", base.short),
        );
        let source = git::source_diff(&dir, &base.hash, other.as_ref().map(|c| c.hash.as_str()));
        let broken = |error: String, problems: Vec<Problem>, head: CompareHead| Compare {
            rev: rev.to_string(),
            against: against.map(str::to_string),
            base: base.clone(),
            head,
            error: Some(error.clone()),
            base_problems: problems,
            cached,
            elapsed_ms: millis(started),
            summary: error,
            changed: false,
            source: source.clone(),
            boards: Vec::new(),
            problems: empty_problems(),
        };
        let digest = match &*reading {
            Reading::Failed { error, problems } => {
                let head = if other.is_some() {
                    CompareHead {
                        commit: head_commit,
                        dirty,
                        errors: 0,
                    }
                } else {
                    self.head_status(None, head_commit, dirty)
                };
                return Ok(broken(error.clone(), problems.clone(), head));
            }
            Reading::Digest(digest) => digest,
        };
        // The head side: the other ref's reading, or the files on disk as they are now.
        let now_owned;
        let now = match &head_reading {
            Some((reading, _)) => match &**reading {
                Reading::Digest(d) => d,
                Reading::Failed { error, problems } => {
                    let head = CompareHead {
                        commit: head_commit,
                        dirty,
                        errors: problems.len() as u32,
                    };
                    return Ok(broken(error.clone(), problems.clone(), head));
                }
            },
            None => {
                now_owned = digest_of(self)?;
                &now_owned
            }
        };
        let head = self.head_status(Some(now), head_commit, dirty);
        let mut result = diff::diff(digest, now, board);
        // A file that does not build has no boards on disk, which is not that they are gone.
        if head.errors > 0 && other.is_none() {
            result.summary = format!(
                "{} on disk: {}",
                diff::count(head.errors as usize, "error", "errors"),
                result.summary
            );
        }
        if let Some(board) = board
            && result.boards.is_empty()
        {
            return Err(ApiError::not_found(format!(
                "there is no board `{board}` at {} or in {}",
                base.short,
                other
                    .as_ref()
                    .map_or_else(|| "the files on disk".to_string(), |c| c.short.clone())
            )));
        }
        Ok(Compare {
            rev: rev.to_string(),
            against: against.map(str::to_string),
            base,
            head,
            error: None,
            base_problems: Vec::new(),
            cached,
            elapsed_ms: millis(started),
            summary: result.summary,
            changed: result.changed,
            source,
            boards: result.boards,
            problems: result.problems,
        })
    }

    fn head_status(
        &self,
        now: Option<&Digest>,
        commit: Option<GitCommit>,
        dirty: bool,
    ) -> CompareHead {
        let errors = match now {
            Some(now) => now
                .problems
                .iter()
                .filter(|p| p.severity == Severity::Error)
                .count() as u32,
            None => self
                .fresh()
                .snapshot
                .problems
                .iter()
                .filter(|p| p.severity == Severity::Error)
                .count() as u32,
        };
        CompareHead {
            commit,
            dirty,
            errors,
        }
    }

    /// The reading of `base`, from the cache if it is there, else made: one at a time, so two
    /// requests for the same ref analyse it once.
    fn read_ref(
        &self,
        place: &Place,
        base: &GitCommit,
        observe: &Observer,
        cancel: &Cancel,
    ) -> Result<(Arc<Reading>, bool), ApiError> {
        let kept = |this: &Workspace| {
            this.compare_cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&base.hash)
        };
        if let Some(reading) = kept(self) {
            return Ok((reading, true));
        }
        let Some(place_in_line) = self.compare_queue.join(MAX_WAITING) else {
            return Err(ApiError::new(
                429,
                format!(
                    "{MAX_WAITING} compares are already waiting for the one that is reading a ref: wait for one to finish and compare again"
                ),
            ));
        };
        let _one_at_a_time = self.comparing.lock().unwrap_or_else(|e| e.into_inner());
        drop(place_in_line);
        // A compare cancelled while it waited for its turn ends now, before it reads anything.
        job::checkpoint();
        if let Some(reading) = kept(self) {
            return Ok((reading, true));
        }
        tell(
            observe,
            "extract",
            0,
            1,
            format!("reading {} ({}) out of git", base.short, base.subject),
        );
        let scratch = Scratch::new()
            .map_err(|e| ApiError::internal(format!("no scratch folder for the compare: {e}")))?;
        let mut skipped = git::Skipped::default();
        let reading = match self.extract_target(place, base, &scratch, &mut skipped) {
            Err(Failure::Unreadable(error)) => Reading::Failed {
                error,
                problems: Vec::new(),
            },
            Err(Failure::Broken(message)) => return Err(ApiError::internal(message)),
            Ok(target) => {
                let workspace = match Workspace::open_for_job(target, cancel) {
                    Ok(w) => w,
                    Err(e) => {
                        let tree = scratch.path().join(&place.prefix);
                        let error = git::project_relative(
                            &format!("the project at {} cannot be read: {e}", base.short),
                            &[tree.as_path(), scratch.path()],
                        );
                        let reading = Arc::new(Reading::Failed {
                            error,
                            problems: Vec::new(),
                        });
                        self.compare_cache
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .put(&base.hash, reading.clone());
                        return Ok((reading, false));
                    }
                };
                workspace.observe(observe.clone());
                // A cancelled analysis has no boards: the reading is not made, and nothing of
                // it is kept (the scratch folder goes with `scratch`, unwinding or not).
                let digest = digest_of(&workspace)?;
                job::checkpoint();
                let errors: Vec<Problem> = digest
                    .problems
                    .iter()
                    .filter(|p| p.severity == Severity::Error)
                    .cloned()
                    .collect();
                match errors.first() {
                    None => Reading::Digest(digest),
                    Some(first) => Reading::Failed {
                        error: format!(
                            "the project does not analyse at {}: {} error{}, the first: {}{}",
                            base.short,
                            errors.len(),
                            if errors.len() == 1 { "" } else { "s" },
                            first.message,
                            first
                                .at
                                .as_ref()
                                .map_or(String::new(), |l| format!(" ({}:{})", l.file, l.line))
                        ),
                        problems: errors,
                    },
                }
            }
        };
        // What a person reads names the project's own paths, not the server's scratch folder.
        let reading = {
            let tree = scratch.path().join(&place.prefix);
            let folders = [tree.as_path(), scratch.path()];
            match reading {
                Reading::Failed { error, problems } => Reading::Failed {
                    error: git::project_relative(&error, &folders),
                    problems: problems
                        .into_iter()
                        .map(|mut p| {
                            p.message = git::project_relative(&p.message, &folders);
                            p.rendered = git::project_relative(&p.rendered, &folders);
                            p
                        })
                        .collect(),
                },
                other => other,
            }
        };
        drop(scratch);
        // A file that was left out is the reason a project may not read: say so.
        let reading = match (reading, skipped.describe()) {
            (Reading::Failed { error, problems }, Some(note)) => Reading::Failed {
                error: format!("{error} ({note})"),
                problems,
            },
            (reading, _) => reading,
        };
        let reading = Arc::new(reading);
        self.compare_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .put(&base.hash, reading.clone());
        Ok((reading, false))
    }

    /// The live target's counterpart in the files of `base`, written into `scratch`.
    fn extract_target(
        &self,
        place: &Place,
        base: &GitCommit,
        scratch: &Scratch,
        skipped: &mut git::Skipped,
    ) -> Result<Target, Failure> {
        let hash = &base.hash;
        let tree = scratch.path().join(&place.prefix);
        match &self.target.project {
            Some(project) => {
                // The folder must have been there, or `git archive` fails on its name.
                if !place.prefix.is_empty() && !git::is_tree(&place.root, hash, &place.prefix) {
                    return Err(Failure::Unreadable(format!(
                        "the project's folder `{}` does not exist at {}",
                        place.prefix, base.short
                    )));
                }
                *skipped = git::extract_project(&place.root, hash, &place.prefix, scratch.path())
                    .map_err(Failure::from)?;
                if !tree.join(MANIFEST).is_file() {
                    return Err(Failure::Unreadable(format!(
                        "the project has no {MANIFEST} at {}: it did not exist there yet",
                        base.short
                    )));
                }
                let manifest_based = self.target.files.iter().map(|f| f.name.as_str()).eq(project
                    .manifest
                    .top_level
                    .iter()
                    .map(String::as_str));
                if manifest_based {
                    Target::resolve_from(&tree, None).map_err(|e| {
                        Failure::Unreadable(format!(
                            "the project cannot be read at {}: {e}",
                            base.short
                        ))
                    })
                } else {
                    let base_project = Project::load(&tree).map_err(|e| {
                        Failure::Unreadable(format!(
                            "the project cannot be read at {}: {e}",
                            base.short
                        ))
                    })?;
                    let mut files = Vec::new();
                    for file in &self.target.files {
                        let absolute =
                            std::path::absolute(&file.path).unwrap_or_else(|_| file.path.clone());
                        let relative = absolute.strip_prefix(&project.root).map_err(|_| {
                            Failure::Unreadable(format!(
                                "{} is not inside the project folder",
                                file.name
                            ))
                        })?;
                        let path = tree.join(relative);
                        if !path.is_file() {
                            return Err(Failure::Unreadable(format!(
                                "{} does not exist at {}",
                                file.name, base.short
                            )));
                        }
                        files.push(SourceFile {
                            name: file.name.clone(),
                            path,
                        });
                    }
                    Ok(Target {
                        project: Some(base_project),
                        files,
                        build_out: self.target.build_out.clone(),
                        fab_out: self.target.fab_out.clone(),
                        title: self.target.title.clone(),
                    })
                }
            }
            None => {
                let mut spec = Vec::new();
                let mut files = Vec::new();
                for file in &self.target.files {
                    let absolute =
                        std::path::absolute(&file.path).unwrap_or_else(|_| file.path.clone());
                    let name = absolute
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let stem = name.strip_suffix(".nsm").unwrap_or(&name).to_string();
                    let found = git::beside(&place.root, hash, &place.prefix, &stem)
                        .map_err(Failure::Broken)?;
                    if !found.contains(&join(&place.prefix, &name)) {
                        return Err(Failure::Unreadable(format!(
                            "{} does not exist at {}",
                            file.name, base.short
                        )));
                    }
                    spec.extend(found);
                    files.push(SourceFile {
                        name: file.name.clone(),
                        path: tree.join(&name),
                    });
                }
                *skipped = git::extract_files(&place.root, hash, &spec, scratch.path())
                    .map_err(Failure::from)?;
                Ok(Target {
                    project: None,
                    files,
                    build_out: self.target.build_out.clone(),
                    fab_out: self.target.fab_out.clone(),
                    title: self.target.title.clone(),
                })
            }
        }
    }
}

/// Why a ref could not be turned into a target.
enum Failure {
    /// The ref has no such project: said in the answer.
    Unreadable(String),
    /// Something on this machine failed (git, tar, the disk): a `500`.
    Broken(String),
}

impl From<git::ExtractError> for Failure {
    fn from(e: git::ExtractError) -> Failure {
        match e {
            git::ExtractError::Refused(message) => Failure::Unreadable(message),
            git::ExtractError::Broken(message) => Failure::Broken(message),
        }
    }
}

fn empty_problems() -> netsmith_editor::api::ProblemsDiff {
    netsmith_editor::api::ProblemsDiff {
        new: Vec::new(),
        resolved: Vec::new(),
        unchanged: 0,
    }
}

fn join(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        path.to_string()
    } else {
        format!("{prefix}/{path}")
    }
}

/// How many times the files are read again if they change under a reading of them.
const DIGEST_TRIES: u32 = 5;

/// The files of a workspace as they are now, reduced to what a comparison reads. Tried again
/// if the files change under it.
pub(super) fn digest_of(ws: &Workspace) -> Result<Digest, ApiError> {
    for attempt in 0..DIGEST_TRIES {
        if attempt > 0 {
            // The files changed under the last reading (a save, an editor writing layouts):
            // give it a moment to settle.
            std::thread::sleep(std::time::Duration::from_millis(50 * u64::from(attempt)));
        }
        let (version, handles, problems, trees) = {
            let state = ws.fresh();
            // The analysis of a workspace that a job reads for itself ends when the job is
            // cancelled, with no boards: that is no reading to digest.
            if ws.inherited.as_ref().is_some_and(Cancel::is_cancelled) {
                return Err(ApiError::cancelled("the compare was cancelled"));
            }
            // The live project's analysis was cancelled: its reading is not of the files on disk,
            // so it is not the head of a comparison (ADR 0062).
            if ws.inherited.is_none() && state.cancelled.is_some() {
                return Err(ApiError::conflict(
                    "the analysis of the files on disk was cancelled, so there is no reading of them to compare: run the analysis again, then compare",
                ));
            }
            let handles = state.snapshot.boards.clone();
            let trees: Vec<Option<PowerTree>> = handles
                .iter()
                .map(|(b, _)| power::tree(&state.snapshot, &b.built.label()).ok())
                .collect();
            (
                state.version,
                handles,
                state.snapshot.problems.clone(),
                trees,
            )
        };
        let catalogs = ws.target.catalogs();
        let mut boards = BTreeMap::new();
        for ((data, stale), tree) in handles.iter().zip(trees) {
            let label = data.built.label();
            let bom = Backend::bom(ws, &label).map_err(|e| e.message);
            boards.insert(
                label,
                board_digest(data, *stale, tree, bom, catalogs.as_ref().ok()),
            );
        }
        if ws.fresh().version == version {
            return Ok(Digest { boards, problems });
        }
    }
    Err(ApiError::conflict(
        "the files changed while they were being compared: compare again",
    ))
}

fn board_digest(
    data: &BoardData,
    stale: bool,
    tree: Option<PowerTree>,
    bom: Result<netsmith_editor::api::Bom, String>,
    catalogs: Option<&Catalogs>,
) -> BoardDigest {
    let built = &data.built;
    let trace = &built.trace;
    let model = &trace.model;
    let sourcing = match catalogs {
        Some(catalogs) => resolve_generics(&built.sourcing, &built.rechecks, catalogs).0,
        None => built.sourcing.clone(),
    };
    let at = |inst| {
        let (src, span) = model.instance(inst).at();
        api_location(locate(&trace.sources, src, span))
    };
    let mut path_of: HashMap<&str, &str> = HashMap::new();
    let mut pins_of: HashMap<&str, Vec<(String, String)>> = HashMap::new();
    for net in &built.design.nets {
        for pin in &net.pins {
            pins_of
                .entry(pin.refdes.as_str())
                .or_default()
                .push((pin.pin.clone(), net.name.clone()));
        }
    }
    let mut parts = Vec::new();
    for ((part, source), &inst) in built.design.parts.iter().zip(&sourcing).zip(&trace.origins) {
        path_of.insert(part.refdes.as_str(), part.path.as_str());
        parts.push(PartDigest {
            path: part.path.clone(),
            designator: part.refdes.clone(),
            kind: part.kind.clone(),
            value: part.value.clone(),
            package: part.package.clone(),
            lcsc: source.lcsc.clone(),
            mpn: source.mpn.clone(),
            assembly: source.assembly.word().to_string(),
            at: model.instance(inst).declared.map(|_| at(inst)),
            pins: pins_of.remove(part.refdes.as_str()).unwrap_or_default(),
        });
    }
    let nets = built
        .design
        .nets
        .iter()
        .map(|n| NetDigest {
            name: n.name.clone(),
            named: netsmith_view::layout::is_named(&n.name),
            pins: n
                .pins
                .iter()
                .map(|p| {
                    (
                        path_of
                            .get(p.refdes.as_str())
                            .map_or_else(|| p.refdes.clone(), |s| s.to_string()),
                        p.pin.clone(),
                    )
                })
                .collect(),
        })
        .collect();
    let mut modules = Vec::new();
    let mut interfaces = Vec::new();
    for (id, inst) in model.instances() {
        if id == model.root {
            continue;
        }
        let digest = || InstanceDigest {
            path: model.path_of(id),
            type_name: inst.type_name.clone(),
            at: inst.declared.map(|_| at(id)),
        };
        match inst.kind {
            netsmith_check::InstKind::Module => modules.push(digest()),
            // The ports of the board and of its modules; what is inside a part is the part's.
            netsmith_check::InstKind::Interface
                if inst.parent.is_some_and(|p| {
                    matches!(
                        model.instance(p).kind,
                        netsmith_check::InstKind::Board | netsmith_check::InstKind::Module
                    )
                }) =>
            {
                interfaces.push(digest());
            }
            _ => {}
        }
    }
    let rails: Vec<PowerNode> = tree.map(|t| t.nodes).unwrap_or_default();
    BoardDigest {
        file: data.file.clone(),
        stale,
        parts,
        nets,
        rails,
        modules,
        interfaces,
        bom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crowd_of_compares_is_refused_after_the_limit_and_places_are_given_back() {
        let queue = Queue::default();
        let first = queue.join(3).expect("a place");
        let second = queue.join(3).expect("a place");
        let third = queue.join(3).expect("a place");
        assert!(queue.join(3).is_none(), "the fourth waits no more");
        drop(second);
        let fourth = queue.join(3).expect("a place was given back");
        assert!(queue.join(3).is_none());
        drop((first, third, fourth));
        assert!(queue.join(3).is_some());
    }
}
