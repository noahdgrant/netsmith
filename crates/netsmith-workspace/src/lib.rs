//! `netsmith editor`: the project as the editor's server sees it (ADR 0019). A
//! [`Workspace`] implements [`netsmith_editor::Backend`] over the files of a [`Target`]:
//! it reads and saves them, builds them again when one changes on disk, and answers with
//! the views of a board computed from the solved model.

mod analysis;
mod catalogs;
mod compare;
mod definition;
mod diff;
mod fileops;
mod files;
mod findings;
mod git;
mod hints;
mod keepouts;
mod orders;
mod outline;
mod panel;
mod pcb;
mod pinwrite;
mod pours;
pub(crate) mod power;
mod progress;
mod release;
mod sidecars;
mod solved;
mod sourcing;
mod system;

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use netsmith_core::job::{self, Cancel};
use netsmith_editor::{
    AnalysisEvent, ApiError, Attachment, Backend, Observer, Say,
    api::{
        Analysis, BoardDetail, Bom, BuildReport, CatalogRefresh, Changed, Compare, Definition,
        FabReport, FileContent, GitInfo, KeepoutPlacement, OrderInfo, OrderReport, PanelView,
        PcbChange, PcbEdit, PcbEdited, PcbFindings, PcbReport, PcbScene, PinLock, PinMove,
        PinWritten, PowerTree, Project, Saved, SchematicHints, SystemView,
    },
};

use netsmith_fabricate::project::Target;

pub use analysis::{Snapshot, module_groups, must_connect, rails_of, schematic_layout};
pub use git::sweep_scratch;

/// The project behind the editor.
pub struct Workspace {
    /// What is built. Replaced by [`Workspace::reload_manifest`] when `netsmith.toml` has
    /// changed, so that the catalogs it lists, its top-level files and its name are the ones on
    /// disk, as for a command-line build.
    target: RwLock<Arc<Target>>,
    /// Why the manifest on disk cannot be read, if it cannot: the analysis then reports it
    /// and builds nothing, until the file is changed again. The target is the last good one.
    manifest_error: Mutex<Option<String>>,
    state: Mutex<State>,
    /// Held while a layout edit is made, so two edits are one after the other.
    editing: Mutex<()>,
    /// How "Check parts against LCSC" asks the endpoints (ADR 0039); the tests point it at a fake.
    network: netsmith_catalog::refresh::Network,
    /// Who hears of each analysis of the files on disk, once the server has said.
    observer: OnceLock<Observer>,
    /// Whether the first analysis has finished. Read without the state's lock, which an
    /// analysis holds for as long as it runs.
    ready: AtomicBool,
    /// Whether a change to a file is analysed at once; off, only [`Backend::run_analysis`] does.
    auto_analysis: AtomicBool,
    /// Set by `run_analysis`: the next reading is made though the page asked for none on change.
    forced: AtomicBool,
    /// When the workspace was opened, for how long the first analysis has taken so far.
    opened: Instant,
    /// What was read of the refs the project was compared with, by commit hash (ADR 0059).
    compare_cache: Mutex<compare::Cache>,
    /// Held while a ref is read, so two compares of one ref analyse it once.
    comparing: Mutex<()>,
    /// The compares waiting for `comparing`, so that a crowd of them is refused.
    compare_queue: compare::Queue,
    /// The token of the analysis of the files that is running now, if one is: what
    /// [`Backend::cancel_analysis`] sets. Outside `state`, whose lock the analysis holds.
    analysis_cancel: Mutex<Option<Cancel>>,
    /// The token of the job this workspace is made for, if it is one a job reads for itself (the
    /// scratch copy of a ref that a compare analyses): its analysis is stopped by that job's
    /// token, not by [`Backend::cancel_analysis`], and a cancelled one is told to no page.
    inherited: Option<Cancel>,
}

/// Files the token of the analysis that is running where [`Backend::cancel_analysis`] finds it,
/// and takes it away when dropped. A workspace that a job reads for itself is not filed: the job
/// cancels it. Filed before [`Announcing`] tells anyone, so that whoever has been told can cancel.
struct Running<'a>(Option<&'a Workspace>);

impl<'a> Running<'a> {
    fn new(workspace: &'a Workspace, token: &Cancel) -> Self {
        if workspace.inherited.is_some() {
            return Running(None);
        }
        *workspace
            .analysis_cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(token.clone());
        Running(Some(workspace))
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Some(workspace) = self.0 {
            *workspace
                .analysis_cancel
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
        }
    }
}

/// Tells the observer that an analysis has begun, and that it has ended when this is dropped,
/// however the analysis ends.
struct Announcing<'a>(&'a Workspace);

impl<'a> Announcing<'a> {
    fn start(workspace: &'a Workspace) -> Self {
        let since = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        workspace.announce(AnalysisEvent::Started { since });
        Announcing(workspace)
    }
}

impl Drop for Announcing<'_> {
    fn drop(&mut self) {
        self.0.announce(AnalysisEvent::Finished);
    }
}

/// Runs `work` as a job that `cancel` stops: its checkpoints end it, and the answer is
/// [`ApiError::cancelled`]. What the job had made is dropped by the unwind; a panic of any
/// other kind goes on as it would have.
fn cancellable<T>(cancel: &Cancel, what: &str, work: impl FnOnce() -> T) -> Result<T, ApiError> {
    job::catch(|| job::scope(cancel, work))
        .map_err(|_| ApiError::cancelled(format!("{what} was cancelled")))
}

/// The fingerprint of the program of each `.nsm` file in `paths` that reads and lexes.
fn code_of(paths: &[PathBuf]) -> HashMap<PathBuf, u64> {
    paths
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e == "nsm"))
        .filter_map(|p| {
            let text = std::fs::read_to_string(p).ok()?;
            Some((p.clone(), netsmith_lang::code_fingerprint(&text)?))
        })
        .collect()
}

/// Whether the watched files that changed since the last analysis are all `.nsm` files whose program,
/// apart from plain comments, is what was analysed: a catalog or a layout file that changed is
/// always a change, and so is the manifest, unless it is `same_manifest`, the one that reads as
/// the manifest the project was analysed with.
fn only_comments_changed(
    state: &State,
    paths: &[PathBuf],
    stamp: &[Option<(std::time::SystemTime, u64)>],
    same_manifest: Option<&Path>,
) -> bool {
    let Some(old) = &state.stamp else {
        return false;
    };
    if old.len() != stamp.len() {
        return false;
    }
    let mut changed = false;
    for (i, path) in paths.iter().enumerate() {
        if old[i] == stamp[i] {
            continue;
        }
        changed = true;
        if same_manifest == Some(path.as_path()) {
            continue;
        }
        let Some(before) = state.code.get(path) else {
            return false;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return false;
        };
        if netsmith_lang::code_fingerprint(&text) != Some(*before) {
            return false;
        }
    }
    changed
}

fn millis(since: Instant) -> u32 {
    since.elapsed().as_millis().min(u128::from(u32::MAX)) as u32
}

#[derive(Default)]
struct State {
    /// Modification time and length of each watched file as last read.
    stamp: Option<Vec<Option<(std::time::SystemTime, u64)>>>,
    /// The same for the layout files beside the top-level files, which do not make the
    /// project build again but do make a board placed again.
    layouts: Vec<Option<(std::time::SystemTime, u64)>>,
    /// What the program of each watched `.nsm` file said when it was last analysed, apart from its plain
    /// comments ([`netsmith_lang::code_fingerprint`]): a change that leaves it as it is is not analysed.
    code: HashMap<PathBuf, u64>,
    /// Goes up by one each time the files are read again.
    version: u64,
    /// The stamp of the files an analysis was cancelled on (ADR 0062). While the files are as
    /// they were then they are not analysed again, as the watcher would at once; a change to
    /// one, or [`Backend::run_analysis`], clears it. The reading stays the one before.
    cancelled: Option<Vec<Option<(std::time::SystemTime, u64)>>>,
    snapshot: Snapshot,
    /// How long the last analysis of the files took; none if the version went up with no
    /// analysis (a layout file changed).
    elapsed_ms: Option<u32>,
    /// Boards placed, and routed if asked, keyed by the board's name; cleared when the
    /// files are read again.
    pcb: HashMap<String, pcb::Entry>,
    /// What `lock --check` said of the pick lock of each source file, by the file's name,
    /// made when a board of the file is asked for and dropped when the files are read again.
    locks: HashMap<String, Option<Arc<netsmith_editor::api::PickLockInfo>>>,
    /// What `fab` wrote for each board, keyed by the board's name.
    fab: HashMap<String, Vec<PathBuf>>,
    /// Where each order was last written, by the order's name.
    orders: HashMap<String, PathBuf>,
}

impl Workspace {
    /// Read and build the target's files once. Fails only if a file cannot be read at all;
    /// mistakes in their text are kept for the page to show.
    pub fn open(target: Target) -> Result<Workspace, String> {
        let workspace = Workspace::open_deferred(target)?;
        drop(workspace.fresh());
        Ok(workspace)
    }

    /// [`Workspace::open`] without the first analysis: it is made by the first call that needs
    /// it (or by the server's watcher, as soon as the server runs), and
    /// [`Backend::pending`] says so until it is done. For a project whose analysis takes
    /// seconds, so that the server can listen first.
    pub fn open_deferred(target: Target) -> Result<Workspace, String> {
        for file in &target.files {
            std::fs::metadata(&file.path).map_err(|e| format!("{}: {e}", file.name))?;
        }
        Ok(Workspace {
            target: RwLock::new(Arc::new(target)),
            manifest_error: Mutex::new(None),
            state: Mutex::new(State::default()),
            editing: Mutex::new(()),
            network: netsmith_catalog::refresh::Network::default(),
            observer: OnceLock::new(),
            ready: AtomicBool::new(false),
            auto_analysis: AtomicBool::new(true),
            forced: AtomicBool::new(false),
            opened: Instant::now(),
            compare_cache: Mutex::new(compare::Cache::default()),
            comparing: Mutex::new(()),
            compare_queue: compare::Queue::default(),
            analysis_cancel: Mutex::new(None),
            inherited: None,
        })
    }

    /// [`Workspace::open_deferred`] for a workspace that a job reads for itself: the job's token
    /// stops its analysis.
    pub(crate) fn open_for_job(target: Target, cancel: &Cancel) -> Result<Workspace, String> {
        let mut workspace = Workspace::open_deferred(target)?;
        workspace.inherited = Some(cancel.clone());
        Ok(workspace)
    }

    fn announce(&self, event: AnalysisEvent) {
        if let Some(observer) = self.observer.get() {
            observer(event);
        }
    }

    /// Where "Check parts against LCSC" asks, how long it waits and where it keeps answers.
    pub fn with_network(mut self, network: netsmith_catalog::refresh::Network) -> Workspace {
        self.network = network;
        self
    }

    pub fn title(&self) -> String {
        self.target().title.clone()
    }

    /// The target as last read: a clone of a pointer, so a caller can keep it across a reload.
    fn target(&self) -> Arc<Target> {
        Arc::clone(&self.target.read().unwrap_or_else(|e| e.into_inner()))
    }

    /// The mistake in the manifest on disk, if there is one.
    fn manifest_error(&self) -> Option<String> {
        self.manifest_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Reads `netsmith.toml` again, and whether what it says differs from what the editor has
    /// (so a change of its comments alone is none). If it reads, the target is built from it;
    /// if not, the target stays as it was and the mistake is kept for the analysis to report
    /// (a project is not built from a manifest nobody can read). A manifest that is mended to
    /// what the last good one said is a change, as the mistake goes.
    fn reload_manifest(&self) -> bool {
        let reloaded = self.target().reloaded();
        let mut error = self
            .manifest_error
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match reloaded {
            Ok(target) => {
                let changed = target.is_some() || error.is_some();
                if let Some(target) = target {
                    *self.target.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(target);
                }
                *error = None;
                changed
            }
            Err(e) => {
                let changed = error.as_ref() != Some(&e);
                *error = Some(e);
                changed
            }
        }
    }

    /// The layout files beside the top-level files, and the routes file beside each (they may
    /// not exist): a change to one by another program places the boards again.
    fn layout_files(&self) -> Vec<PathBuf> {
        self.target()
            .files
            .iter()
            .flat_map(|f| {
                let layout = netsmith_fabricate::layout::layout_path(&f.path);
                let routes = netsmith_fabricate::routes::beside_layout(&layout);
                [layout, routes]
            })
            .collect()
    }

    /// The state, built again first if a watched file changed since it was last read.
    /// The system scene of `system` as a `.glb` (ADR 0110): each member placed as `pcb_3d` of
    /// `System/member` has it (routed, if it was routed since the files were read), and the
    /// members' layout stamps joined with `,` in the system's order.
    fn system_3d(&self, system: &analysis::SystemData) -> Result<netsmith_editor::Pcb3d, ApiError> {
        let name = &system.solved.name;
        let mut members = Vec::new();
        for (member, _, _) in &system.solved.boards {
            let label = format!("{name}/{member}");
            let (version, data) = {
                let state = self.fresh();
                let data = state.snapshot.board_handle(&label)?;
                if let Some(entry) = state.pcb.get(&label) {
                    members.push((data, entry.board.clone(), entry.layout_stamp().to_string()));
                    continue;
                }
                (state.version, data)
            };
            let entry = pcb::place(&data, &label)?;
            members.push((data, entry.board.clone(), entry.layout_stamp().to_string()));
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.version == version {
                state.pcb.entry(label).or_insert(entry);
            }
        }
        let pairs: Vec<(&netsmith_pipeline::Built, &netsmith_pcb::Board)> =
            members.iter().map(|(d, b, _)| (&d.built, b)).collect();
        let scene = netsmith_fabricate::system3d::scene(&system.solved, &pairs);
        Ok(netsmith_editor::Pcb3d {
            glb: netsmith_3d::system_glb(&scene),
            layout_stamp: members
                .iter()
                .map(|(_, _, s)| s.as_str())
                .collect::<Vec<_>>()
                .join(","),
        })
    }

    fn fresh(&self) -> MutexGuard<'_, State> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // A layout file changed by another program: the boards are placed again, and the page
        // is told as it is for a source (a change the editor made itself is recorded in
        // `pcb_edit` and is not one).
        let layouts = files::stamps(&self.layout_files());
        if state.version > 0 && state.layouts != layouts {
            state.version += 1;
            state.pcb.clear();
            state.elapsed_ms = None;
        }
        state.layouts = layouts;
        let mut paths = self.target().watched();
        let mut stamp = files::stamps(&paths);
        let mut all_read = stamp.iter().all(Option::is_some);
        if state.version > 0 && state.stamp.as_ref() == Some(&stamp) && all_read {
            return state;
        }
        // A watched file changed, and the manifest may be it: read it again first, as what is
        // watched (the catalogs it lists, the top-level files) comes from it. One that says
        // what it did (a comment was changed) is no change.
        let mut same_manifest = None;
        if state.version > 0 {
            let manifest = self
                .target()
                .project
                .as_ref()
                .map(|p| p.root.join(netsmith_fabricate::project::MANIFEST));
            if let Some(manifest) = manifest {
                if !self.reload_manifest() {
                    same_manifest = Some(manifest);
                }
                paths = self.target().watched();
                stamp = files::stamps(&paths);
                all_read = stamp.iter().all(Option::is_some);
            }
        }
        // Only plain comments changed: the reading is still right, and the files are as good as read.
        if state.version > 0
            && all_read
            && state.cancelled.is_none()
            && only_comments_changed(&state, &paths, &stamp, same_manifest.as_deref())
        {
            state.stamp = Some(stamp);
            return state;
        }
        // An analysis of these very files was cancelled: they are not analysed again until one
        // changes or the page asks (`run_analysis`).
        if all_read && state.cancelled.as_ref() == Some(&stamp) {
            return state;
        }
        // The page asked for no analysis on change: the files are read when it asks.
        let forced = self.forced.swap(false, Ordering::AcqRel);
        if !forced && state.version > 0 && all_read && !self.auto_analysis.load(Ordering::Acquire) {
            return state;
        }
        let started = Instant::now();
        let code = code_of(&paths);
        // An analysis that panics is a result like another: the last good reading and a
        // problem that says so. The stamp and the version change only here, after the
        // result, so that nothing is left half done and the files are not analysed again
        // (and panicked on again) until one changes.
        //
        // One that is cancelled is no result: nothing of the state changes but the stamp it
        // was cancelled on (so that the watcher does not start it again), and, for the first
        // analysis, which has no reading to keep, an empty one that says why.
        let token = self.inherited.clone().unwrap_or_default();
        let ran = {
            // The token is filed before the page is told: the page offers Cancel the moment it hears
            // `Started`, and a cancel asked then must reach this analysis, not find nothing running.
            let _running = Running::new(self, &token);
            let _announcing = Announcing::start(self);
            let observe = |event| self.announce(event);
            // The solver's counts go to the observer too, which `observe` borrows.
            let live = self.observer.get().cloned();
            let ran = job::catch(|| {
                analysis::contained(|| {
                    job::scope(&token, || {
                        self.analyze(&HashMap::new(), Some(&state.snapshot), &observe, live)
                    })
                })
            });
            match &ran {
                Ok(Err(message)) => {
                    eprintln!("netsmith editor: the analysis of the project panicked: {message}");
                    self.announce(AnalysisEvent::Failed {
                        message: message.clone(),
                    });
                }
                Err(_) if self.inherited.is_none() => self.announce(AnalysisEvent::Cancelled),
                _ => {}
            }
            ran
        };
        let snapshot = match ran {
            Ok(Ok(snapshot)) => snapshot,
            Ok(Err(message)) => Snapshot::after_panic(&message, Some(&state.snapshot)),
            Err(job::Cancelled) => {
                state.cancelled = Some(stamp);
                if state.version == 0 {
                    state.snapshot = Snapshot::cancelled_first();
                    state.version = 1;
                    state.elapsed_ms = Some(millis(started));
                    self.ready.store(true, Ordering::Release);
                }
                return state;
            }
        };
        state.pcb.clear();
        state.locks.clear();
        state.cancelled = None;
        state.stamp = Some(stamp);
        state.code = code;
        state.version += 1;
        state.snapshot = snapshot;
        state.elapsed_ms = Some(millis(started));
        self.ready.store(true, Ordering::Release);
        state
    }
}

/// What an edit of the PCB view does to the board's physical file: the keepouts it changes, and
/// the file's text before and after.
struct PhysicalChange {
    names: Vec<String>,
    old: Option<String>,
    text: String,
}

impl Workspace {
    /// Write an edit to the layout file beside the board's source (ADR 0032), and the keepouts
    /// the editor draws to the board's physical file (ADR 0048, [`keepouts`]), if the files and
    /// the entries are as the page last saw them and the board takes the result, and answer
    /// with the board placed again.
    fn edit_layout(&self, edit: &PcbEdit) -> Result<PcbEdited, ApiError> {
        let _one_at_a_time = self.editing.lock().unwrap_or_else(|e| e.into_inner());
        let name = &edit.board;
        let (version, data, layers) = {
            let state = self.fresh();
            (
                state.version,
                state.snapshot.board_handle(name)?,
                state.pcb.get(name).map(pcb::Entry::layers),
            )
        };
        let Some((file, boards)) = &data.layout else {
            return Err(ApiError::bad_request(format!(
                "`{name}` has no layout file to write"
            )));
        };
        let shown = file
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        if files::stamp_of(file) != edit.stamp {
            return Err(ApiError::conflict(format!(
                "{shown} changed on disk since the board was read: reload the board"
            )));
        }
        let layers = match layers {
            Some(layers) => layers,
            None => pcb::place(&data, name)?.layers(),
        };
        let mut layout = netsmith_fabricate::layout::read(file, boards)
            .map_err(|e| ApiError::internal(format!("{name}: {e}")))?
            .map_or_else(netsmith_pcb::Layout::default, |(layout, _)| layout);
        let before = layout.clone();
        let board = &data.built.design.name;
        let physical = self.physical_change(&data, &mut layout, &layers, &edit.changes)?;
        let to_layout: Vec<PcbChange> = edit
            .changes
            .iter()
            .filter(|c| !matches!(c, PcbChange::Keepout { name, .. } if physical.as_ref().is_some_and(|p| p.names.contains(name))))
            .cloned()
            .collect();
        pcb::change_layout(&mut layout, board, &layers, &to_layout)?;
        if let Some(physical) = physical {
            return self.write_physical(&data, name, layout, &before, physical);
        }
        // Placed with the new layout before anything is written: an entry the board does not
        // take is refused with the file as it was.
        let mut entry = pcb::place_with(
            &data,
            name,
            Some((layout.clone(), file.display().to_string())),
            String::new(),
        )
        .map_err(|e| match e {
            pcb::PlaceError::Layout(message) => ApiError::bad_request(message),
            e @ pcb::PlaceError::Unplaceable(..) => e.read(),
            pcb::PlaceError::Other(e) => e,
        })?;
        files::write_atomic(file, &layout.to_text())
            .map_err(|e| ApiError::internal(format!("{shown}: {e}")))?;
        entry.set_layout_stamp(files::stamp_of(file));
        let scene = pcb::scene(&entry, &data, name);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Our own write is not a change by another program.
        state.layouts = files::stamps(&self.layout_files());
        if state.version == version {
            state.pcb.insert(name.clone(), entry);
        }
        Ok(PcbEdited { scene })
    }

    /// The keepout changes of `changes` that go to the board's physical file: those of a keepout
    /// the editor draws (one with a size, before or after) and of one the physical file states
    /// in the editor's shape, with the file's text after them. `None` if there are none. A
    /// keepout drawn in the layout file before (a `size` there) is taken out of `layout` here,
    /// as its first change moves it.
    fn physical_change(
        &self,
        data: &analysis::BoardData,
        layout: &mut netsmith_pcb::Layout,
        layers: &[String],
        changes: &[PcbChange],
    ) -> Result<Option<PhysicalChange>, ApiError> {
        let board = &data.built.design.name;
        let shown = data
            .physical
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let old = match std::fs::read_to_string(&data.physical) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(ApiError::internal(format!("{shown}: {e}"))),
        };
        let have = old
            .as_deref()
            .map_or_else(Default::default, |t| keepouts::drawn_in(t, board));
        let stated = old
            .as_deref()
            .map_or_else(Default::default, |t| keepouts::stated_in(t, board));
        let mut wanted: Vec<(String, Option<netsmith_pcb::KeepoutPlace>)> = Vec::new();
        let mut restated: Vec<(String, &keepouts::Stated, [f64; 4], [f64; 4])> = Vec::new();
        for change in changes {
            let PcbChange::Keepout {
                name,
                before,
                after,
            } = change
            else {
                continue;
            };
            if let Some(s) = stated.get(name) {
                let (now, after) = Self::restated(data, &shown, layers, name, before, after)?;
                restated.push((name.clone(), s, now, after));
                continue;
            }
            let drawn =
                |p: &Option<KeepoutPlacement>| p.as_ref().is_some_and(|p| p.drawn.is_some());
            if !have.contains_key(name) && !drawn(before) && !drawn(after) {
                continue;
            }
            let table = layout.boards.get(board);
            let in_layout = table
                .and_then(|t| t.keepouts.get(name))
                .filter(|k| k.is_drawn());
            let now = have.get(name).map(|d| &d.place).or(in_layout);
            let taken = || {
                data.built
                    .design
                    .features
                    .keepouts
                    .iter()
                    .any(|k| k.part.is_none() && &k.name == name)
                    || data
                        .built
                        .design
                        .features
                        .cutouts
                        .iter()
                        .any(|c| &c.name == name)
            };
            if now.is_none() && taken() {
                return Err(ApiError::bad_request(format!(
                    "the board has a keepout or cutout `{name}` that the editor does not draw: change it in {shown}, or draw one of another name"
                )));
            }
            if !pcb::same_keepout(before.as_ref(), now, layers) {
                return Err(ApiError::conflict(format!(
                    "the keepout `{name}` is not what the page last saw (the file changed, or the page is out of date): reload the board"
                )));
            }
            let after = match after {
                Some(after) if after.drawn.is_none() => {
                    return Err(ApiError::bad_request(format!(
                        "the keepout `{name}` is drawn, and a change to it says what it is: its size, layers and what it forbids"
                    )));
                }
                Some(after) => {
                    let place = pcb::keepout_place(after, layers)?;
                    netsmith_pcb::check_drawn_keepout(name, &place, layers)
                        .map_err(|e| ApiError::bad_request(format!("{shown}: {e}")))?;
                    Some(place)
                }
                None => None,
            };
            if in_layout.is_some()
                && let Some(table) = layout.boards.get_mut(board)
            {
                table.keepouts.remove(name);
            }
            wanted.push((name.clone(), after));
        }
        if wanted.is_empty() && restated.is_empty() {
            return Ok(None);
        }
        // The span edits first, on the file as it is, whose places their spans are.
        let text = old.as_deref().map(|t| {
            let spans: Vec<_> = restated.iter().map(|(_, s, n, a)| (*s, *n, *a)).collect();
            keepouts::restate(t, &spans)
        });
        let text = keepouts::rewrite(text.as_deref(), board, &wanted)
            .map_err(|e| ApiError::bad_request(format!("{shown}: {e}")))?;
        Ok(Some(PhysicalChange {
            names: wanted
                .into_iter()
                .map(|(name, _)| name)
                .chain(restated.into_iter().map(|(name, ..)| name))
                .collect(),
            old,
            text,
        }))
    }

    /// The place and size, now and after the change from `before` to `after` (each
    /// `[x, y, width, height]` in mm), of the keepout `name` that the physical file states in a
    /// shape the editor does not write whole, for a span edit ([`keepouts::restate`]). Refused if
    /// the keepout is not as the page last saw it (409), or if the change removes it or changes
    /// anything but its place and size (400).
    fn restated(
        data: &analysis::BoardData,
        shown: &str,
        layers: &[String],
        name: &str,
        before: &Option<KeepoutPlacement>,
        after: &Option<KeepoutPlacement>,
    ) -> Result<([f64; 4], [f64; 4]), ApiError> {
        let now = data
            .built
            .design
            .features
            .keepouts
            .iter()
            .find(|k| k.part.is_none() && k.name == name)
            .and_then(|k| {
                let (x, y) = k.at?;
                let allow = |forbids: bool| (!forbids).then(|| "allow".to_string());
                let mut place = netsmith_pcb::KeepoutPlace::at(x, y);
                place.size = Some([k.size.0, k.size.1]);
                place.layers = Some(k.layers.join(","));
                place.tracks = allow(k.tracks);
                place.vias = allow(k.vias);
                place.copper = allow(k.copper);
                place.parts = allow(k.parts);
                Some(place)
            });
        if !pcb::same_keepout(before.as_ref(), now.as_ref(), layers) {
            return Err(ApiError::conflict(format!(
                "the keepout `{name}` is not what the page last saw (the file changed, or the page is out of date): reload the board"
            )));
        }
        let now = now.expect("the same as a keepout the page saw");
        let Some(after) = after.as_ref().filter(|a| a.drawn.is_some()) else {
            return Err(ApiError::bad_request(format!(
                "the keepout `{name}` is stated in {shown} in a shape the editor does not write: the editor moves and resizes it there, and it is removed or changed otherwise in the file"
            )));
        };
        let place = pcb::keepout_place(after, layers)?;
        let size = place.size.expect("a change with a size");
        if !place.at.iter().chain(&size).all(|v| v.is_finite()) {
            return Err(ApiError::bad_request(format!(
                "{shown}: the keepout `{name}` has a position or size that is not a number"
            )));
        }
        if size[0] <= 0.0 || size[1] <= 0.0 {
            return Err(ApiError::bad_request(format!(
                "{shown}: the keepout `{name}` has the size {} by {} mm; a keepout is wider and higher than nothing",
                size[0], size[1]
            )));
        }
        // What it is stays as the file states it: only its place and size are the editor's.
        let mut same = now.clone();
        same.at = place.at;
        same.size = place.size;
        if !pcb::same_keepout(Some(after), Some(&same), layers) {
            return Err(ApiError::bad_request(format!(
                "the keepout `{name}` is stated in {shown} in a shape the editor does not write: the editor changes only its place and size there, and what it forbids and its layers are changed in the file"
            )));
        }
        let [width, height] = now.size.expect("a stated keepout has a size");
        Ok((
            [now.at[0], now.at[1], width, height],
            [place.at[0], place.at[1], size[0], size[1]],
        ))
    }

    /// Write the physical file of an edit and the layout file beside it, if the project as it
    /// would be read then builds the board and places it: analysed with the new text first, and
    /// nothing is written if it does not. Then the project is analysed again from the files, as
    /// for a save, and the answer is the board placed from that.
    fn write_physical(
        &self,
        data: &analysis::BoardData,
        name: &str,
        layout: netsmith_pcb::Layout,
        before: &netsmith_pcb::Layout,
        physical: PhysicalChange,
    ) -> Result<PcbEdited, ApiError> {
        let Some((file, _)) = &data.layout else {
            return Err(ApiError::bad_request(format!(
                "`{name}` has no layout file to write"
            )));
        };
        let shown = |p: &std::path::Path| {
            p.file_name()
                .map_or(String::new(), |n| n.to_string_lossy().into_owned())
        };
        let unsaved = HashMap::from([(data.physical.clone(), physical.text.clone())]);
        let snapshot = analysis::contained(|| self.analyze(&unsaved, None, &|_| {}, None))
            .map_err(|message| {
                ApiError::internal(format!("the check of the edit panicked: {message}"))
            })?;
        let Ok(checked) = snapshot.board_handle(name) else {
            let physical_name = shown(&data.physical);
            let errors: Vec<String> = snapshot
                .problems
                .iter()
                .filter(|p| p.severity == netsmith_editor::api::Severity::Error)
                .map(|p| match &p.at {
                    Some(at) => format!("{}:{}: {}", at.file, at.line, p.message),
                    None => p.message.clone(),
                })
                .collect();
            return Err(ApiError::bad_request(format!(
                "the board does not build with this change to {physical_name}, and nothing was written: {}",
                errors.join("; ")
            )));
        };
        let mut entry = pcb::place_with(
            &checked,
            name,
            Some((layout.clone(), file.display().to_string())),
            String::new(),
        )
        .map_err(|e| match e {
            pcb::PlaceError::Layout(message) => ApiError::bad_request(message),
            e @ pcb::PlaceError::Unplaceable(..) => e.read(),
            pcb::PlaceError::Other(e) => e,
        })?;
        if physical.old.as_deref() != Some(physical.text.as_str()) {
            files::write_atomic(&data.physical, &physical.text)
                .map_err(|e| ApiError::internal(format!("{}: {e}", shown(&data.physical))))?;
        }
        if layout != *before {
            files::write_atomic(file, &layout.to_text())
                .map_err(|e| ApiError::internal(format!("{}: {e}", shown(file))))?;
        }
        entry.set_layout_stamp(files::stamp_of(file));
        // The physical file is the source's: the project is read again, as after a save, even
        // with the analysis on change turned off.
        self.forced.store(true, Ordering::Release);
        let mut state = self.fresh();
        state.layouts = files::stamps(&self.layout_files());
        let data = state.snapshot.board_handle(name).unwrap_or(checked);
        let scene = pcb::scene(&entry, &data, name);
        if state.cancelled.is_none() {
            state.pcb.insert(name.to_string(), entry);
        }
        Ok(PcbEdited { scene })
    }

    /// A change to the routes file from the PCB view, made by `netsmith_fabricate::routes::edit`
    /// as `netsmith routes` makes it, on the board as placed with no copper.
    fn edit_routes(
        &self,
        edit: &netsmith_editor::api::PcbRoutesEdit,
    ) -> Result<netsmith_editor::api::PcbRoutesEdited, ApiError> {
        let _one_at_a_time = self.editing.lock().unwrap_or_else(|e| e.into_inner());
        let name = &edit.board;
        // The board as the page last saw it placed, or placed now.
        let (version, data, entry) = {
            let state = self.fresh();
            let data = state.snapshot.board_handle(name)?;
            let entry = state.pcb.get(name).map(|e| e.routes_changed(&data, name));
            (state.version, data, entry)
        };
        let entry = match entry {
            Some(entry) => entry,
            None => pcb::place_checked(&data, name),
        }
        .map_err(pcb::PlaceError::read)?;
        let Some(file) = entry.routes_file().map(Path::to_path_buf) else {
            return Err(ApiError::bad_request(format!(
                "`{name}` has no routes file to write"
            )));
        };
        let shown = file
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        if files::stamp_of(&file) != edit.stamp || entry.routes_stamp() != edit.stamp {
            return Err(ApiError::conflict(format!(
                "{shown} changed on disk since the board was read: reload the board"
            )));
        }
        let changes = edit
            .changes
            .iter()
            .map(|c| pcb::routes_edit(entry.placed(), c))
            .collect::<Result<Vec<_>, _>>()?;
        // The nets the edit changes, in the order they first appear.
        let mut nets: Vec<String> = Vec::new();
        for c in &changes {
            if !nets.iter().any(|n| n == c.net()) {
                nets.push(c.net().to_string());
            }
        }
        if nets.is_empty() {
            return Err(ApiError::bad_request(
                "an edit of the routes file names a change",
            ));
        }
        // Each net's record before and after, as the file has it: the steps of undo.
        let records = |file: &Path| -> Result<Vec<Option<String>>, ApiError> {
            let routes = netsmith_fabricate::routes::load(file).map_err(ApiError::internal)?;
            Ok(nets
                .iter()
                .map(|net| {
                    netsmith_fabricate::routes::record_text(
                        name,
                        net,
                        routes.boards.get(name).and_then(|b| b.nets.get(net)),
                    )
                })
                .collect())
        };
        let before = records(&file)?;
        let said = netsmith_fabricate::routes::edit_file_all(entry.placed(), &file, &changes)
            .map_err(|refusal| match refusal {
                netsmith_fabricate::routes::Refusal::Refused(e) => {
                    ApiError::unprocessable(e, "routes_refused", &[("board", name)])
                }
                netsmith_fabricate::routes::Refusal::Failed(e) => ApiError::internal(e),
            })?;
        let after = records(&file)?;
        let undo = nets
            .iter()
            .zip(before)
            .zip(after)
            .map(|((net, before), after)| netsmith_editor::api::RoutesUndo {
                net: net.clone(),
                before,
                after,
            })
            .collect();
        let entry = entry
            .routes_changed(&data, name)
            .map_err(pcb::PlaceError::read)?;
        let scene = pcb::scene(&entry, &data, name);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Our own write is not a change by another program.
        state.layouts = files::stamps(&self.layout_files());
        if state.version == version {
            state.pcb.insert(name.clone(), entry);
        }
        Ok(netsmith_editor::api::PcbRoutesEdited {
            scene,
            undo,
            said: said.replace(
                &format!("; wrote {}", file.display()),
                &format!("; wrote {shown}"),
            ),
        })
    }
}

impl Backend for Workspace {
    fn title(&self) -> String {
        self.target().title.clone()
    }

    fn project(&self) -> Project {
        self.tree()
    }

    fn read_file(&self, path: &str) -> Result<FileContent, ApiError> {
        self.read(path)
    }

    fn write_file(&self, path: &str, text: &str, stamp: &str) -> Result<Saved, ApiError> {
        self.write(path, text, stamp)
    }

    fn create_file(&self, path: &str, text: &str) -> Result<Changed, ApiError> {
        self.make_file(path, text)
    }

    fn create_dir(&self, path: &str) -> Result<Changed, ApiError> {
        self.make_dir(path)
    }

    fn rename_path(&self, from: &str, to: &str) -> Result<Changed, ApiError> {
        self.move_path(from, to)
    }

    fn delete_path(&self, path: &str, confirm: bool) -> Result<Changed, ApiError> {
        self.remove_path(path, confirm)
    }

    fn analysis(&self) -> Analysis {
        let state = self.fresh();
        let stale = !self.auto_analysis.load(Ordering::Acquire)
            && state.version > 0
            && state.stamp.as_ref() != Some(&files::stamps(&self.target().watched()));
        Analysis {
            elapsed_ms: state.elapsed_ms,
            cancelled: state.cancelled.is_some().then_some(true),
            stale: stale.then_some(true),
            ..state.snapshot.summary(state.version)
        }
    }

    fn cancel_analysis(&self) -> bool {
        let running = self
            .analysis_cancel
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match &*running {
            Some(token) => {
                token.cancel();
                true
            }
            None => false,
        }
    }

    fn set_auto_analysis(&self, auto: bool) {
        self.auto_analysis.store(auto, Ordering::Release);
    }

    fn run_analysis(&self) -> Analysis {
        self.forced.store(true, Ordering::Release);
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancelled = None;
        let reading = self.analysis();
        // Nothing was read (no file changed): the request is spent, and must not carry over to the next change.
        self.forced.store(false, Ordering::Release);
        reading
    }

    fn pending(&self) -> Option<u32> {
        (!self.ready.load(Ordering::Acquire)).then(|| millis(self.opened))
    }

    fn observe(&self, observer: Observer) {
        let _ = self.observer.set(observer);
    }

    fn check(&self, path: &str, text: &str) -> Result<Analysis, ApiError> {
        let file = self.entry(path)?;
        let Some(fs) = file.fs.filter(|_| !file.readonly) else {
            return Err(ApiError::bad_request(format!("{path} cannot be edited")));
        };
        {
            // The typed text has the program that was analysed, only its plain comments differ: the reading of the
            // files is the reading of the text, and nothing is analysed for a word in a comment.
            let state = self.fresh();
            if state.version > 0
                && state.cancelled.is_none()
                && state.code.get(&fs).copied() == netsmith_lang::code_fingerprint(text)
                && state.code.contains_key(&fs)
            {
                return Ok(Analysis {
                    elapsed_ms: Some(0),
                    ..state.snapshot.summary(state.version)
                });
            }
        }
        let version = self.fresh().version;
        let unsaved = HashMap::from([(fs, text.to_string())]);
        // A check of typed text is not an analysis of the files on disk: nobody is told of it.
        let started = Instant::now();
        let snapshot = analysis::contained(|| self.analyze(&unsaved, None, &|_| {}, None))
            .unwrap_or_else(|message| {
                eprintln!("netsmith editor: the check of {path} panicked: {message}");
                Snapshot::after_panic(&message, None)
            });
        Ok(Analysis {
            elapsed_ms: Some(millis(started)),
            ..snapshot.summary(version)
        })
    }

    fn board(&self, name: &str) -> Result<BoardDetail, ApiError> {
        self.board_detail(name)
    }

    fn schematic(&self, board: &str) -> Result<String, ApiError> {
        let state = self.fresh();
        let data = state.snapshot.board(board)?;
        data.svg.clone().map_err(ApiError::internal)
    }

    fn schematic_hints(&self, board: &str) -> Result<SchematicHints, ApiError> {
        self.schematic_hints_of(board)
    }

    fn write_schematic_hints(
        &self,
        board: &str,
        text: &str,
        stamp: Option<&str>,
    ) -> Result<SchematicHints, ApiError> {
        self.write_schematic_hints_of(board, text, stamp)
    }

    fn power(&self, board: &str) -> Result<PowerTree, ApiError> {
        let state = self.fresh();
        power::tree(&state.snapshot, board)
    }

    fn system(&self, name: &str) -> Result<SystemView, ApiError> {
        let state = self.fresh();
        system::view(&state.snapshot, name)
    }

    fn panel(&self, name: &str) -> Result<PanelView, ApiError> {
        self.panel_view(name)
    }

    fn bom(&self, board: &str) -> Result<Bom, ApiError> {
        let state = self.fresh();
        let catalogs = self.target().catalogs().map_err(ApiError::internal)?;
        let today = netsmith_catalog::refresh::date_of(
            self.network
                .now
                .unwrap_or_else(netsmith_catalog::refresh::now_secs),
        );
        let own = match &self.target().project {
            Some(project) => project
                .catalog_files()
                .map_err(ApiError::internal)?
                .into_iter()
                .map(|f| f.file)
                .collect(),
            None => Vec::new(),
        };
        let supply = match &self.target().project {
            Some(project) => project.supply().map_err(ApiError::internal)?,
            None => netsmith_catalog::supply::Supply::none(),
        };
        sourcing::bom(
            state.snapshot.board(board)?,
            &catalogs,
            &supply,
            &today,
            own,
        )
    }

    fn refresh_catalogs(
        &self,
        dry_run: bool,
        offline: bool,
        say: &Say,
    ) -> Result<CatalogRefresh, ApiError> {
        self.refresh_project_catalogs(dry_run, offline, say)
    }

    fn route(&self, board: &str, say: &Say) -> Result<PcbReport, ApiError> {
        // Placed under the lock, routed without it: the router takes seconds, and every
        // other request (and the watcher) waits for the lock.
        say(&format!("{board}: placing"));
        let (version, data) = {
            let state = self.fresh();
            (state.version, state.snapshot.board_handle(board)?)
        };
        let mut entry = pcb::place(&data, board)?;
        entry.route(board, say);
        let report = entry.report(board);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Files that changed meanwhile made another reading; this board is of the old one.
        if state.version == version {
            state.pcb.insert(board.to_string(), entry);
        }
        Ok(report)
    }

    fn cancel_route(&self) {
        pcb::cancel();
    }

    fn pcb_scene(&self, board: &str) -> Result<PcbScene, ApiError> {
        let (version, data) = {
            let state = self.fresh();
            let data = state.snapshot.board_handle(board)?;
            if let Some(entry) = state.pcb.get(board) {
                return Ok(pcb::scene(entry, &data, board));
            }
            (state.version, data)
        };
        let entry = pcb::place(&data, board)?;
        let scene = pcb::scene(&entry, &data, board);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.version == version {
            state.pcb.entry(board.to_string()).or_insert(entry);
        }
        Ok(scene)
    }

    fn pcb_3d(&self, board: &str) -> Result<netsmith_editor::Pcb3d, ApiError> {
        // A system's name: its system scene (ADR 0110), each member as the 2D view has it.
        let system = {
            let state = self.fresh();
            state
                .snapshot
                .systems
                .iter()
                .find(|(s, _)| s.solved.name == board)
                .map(|(s, _)| Arc::clone(s))
        };
        if let Some(system) = system {
            return self.system_3d(&system);
        }
        // The placement `pcb_scene` draws, kept in `state.pcb`: placed here only when the board
        // has not been placed since the files were read, and then kept for the 2D view too.
        let (version, data) = {
            let state = self.fresh();
            let data = state.snapshot.board_handle(board)?;
            if let Some(entry) = state.pcb.get(board) {
                return Ok(pcb::glb(entry, &data));
            }
            (state.version, data)
        };
        let entry = pcb::place(&data, board)?;
        let glb = pcb::glb(&entry, &data);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.version == version {
            state.pcb.entry(board.to_string()).or_insert(entry);
        }
        Ok(glb)
    }

    fn pcb_pours(&self, board: &str) -> Result<netsmith_editor::api::PcbPours, ApiError> {
        // The placement `pcb_scene` draws, kept in `state.pcb`; placed here only when the board
        // has not been placed since the files were read, and then kept for the 2D view too.
        let (version, data) = {
            let state = self.fresh();
            let data = state.snapshot.board_handle(board)?;
            if let Some(entry) = state.pcb.get(board) {
                // Copied out: the fill runs with no lock held.
                let placement = pours::Placement::of(entry);
                drop(state);
                return Ok(placement.pours(board));
            }
            (state.version, data)
        };
        let entry = pcb::place(&data, board)?;
        let answer = pours::Placement::of(&entry).pours(board);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.version == version {
            state.pcb.entry(board.to_string()).or_insert(entry);
        }
        Ok(answer)
    }

    fn pcb_findings(&self, board: &str) -> Result<PcbFindings, ApiError> {
        let (version, data) = {
            let state = self.fresh();
            if let Some(entry) = state.pcb.get(board) {
                return Ok(entry.findings(board));
            }
            (state.version, state.snapshot.board_handle(board)?)
        };
        let entry = match pcb::place_checked(&data, board) {
            Ok(entry) => entry,
            // The requirements prove that no placement exists: the findings say so at once,
            // with no board placed (ADR 0045 section 14).
            Err(pcb::PlaceError::Unplaceable(_, pre)) => {
                return Ok(pcb::unplaceable_findings(board, &pre));
            }
            Err(e) => return Err(e.read()),
        };
        let found = entry.findings(board);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.version == version {
            state.pcb.entry(board.to_string()).or_insert(entry);
        }
        Ok(found)
    }

    fn pcb_drc(&self, board: &str, say: &Say) -> Result<PcbFindings, ApiError> {
        // The board as it is now (placed if need be), copied out from under the lock: KiCad
        // takes seconds.
        self.pcb_findings(board)?;
        let (version, stamp, copy, routed) = {
            let state = self.fresh();
            let entry = state
                .pcb
                .get(board)
                .ok_or_else(|| ApiError::not_found(format!("there is no board `{board}`")))?;
            (
                state.version,
                entry.layout_stamp().to_string(),
                entry.board.clone(),
                entry.is_routed(),
            )
        };
        let result = findings::run_kicad(&copy, routed, say);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // Kept only for the board it was run on: not one an edit has placed again meanwhile.
        let current = state.version == version;
        if let Some(entry) = state.pcb.get_mut(board)
            && current
            && entry.layout_stamp() == stamp
            && entry.board == copy
        {
            entry.kicad = Some(result);
        }
        match state.pcb.get(board) {
            Some(entry) => Ok(entry.findings(board)),
            None => Err(ApiError::conflict(format!(
                "`{board}` changed while KiCad's check ran: check again"
            ))),
        }
    }

    fn pcb_edit(&self, edit: &PcbEdit) -> Result<PcbEdited, ApiError> {
        self.edit_layout(edit)
    }

    fn pcb_routes(
        &self,
        edit: &netsmith_editor::api::PcbRoutesEdit,
    ) -> Result<netsmith_editor::api::PcbRoutesEdited, ApiError> {
        self.edit_routes(edit)
    }

    fn pcb_walk(
        &self,
        edit: &netsmith_editor::api::PcbWalk,
    ) -> Result<netsmith_editor::api::PcbWalked, ApiError> {
        let name = &edit.board;
        // The board a track is checked against, copied out so that the search holds no lock.
        let (data, board) = {
            let state = self.fresh();
            let data = state.snapshot.board_handle(name)?;
            let board = state.pcb.get(name).map(|entry| {
                let board = entry.board_for_walk();
                let router = entry.router_copper(&board);
                let shove = edit.shove.then(|| entry.board_for_shove());
                (board, router, shove)
            });
            (data, board)
        };
        let (board, router, shove) = match board {
            Some(all) => all,
            None => {
                let entry = pcb::place_checked(&data, name).map_err(pcb::PlaceError::read)?;
                (
                    entry.board_for_walk(),
                    pcb::RouterCopper::default(),
                    edit.shove.then(|| entry.board_for_shove()),
                )
            }
        };
        pcb::walk(&board, &router, shove.as_ref(), edit)
    }

    fn pin_lock(&self, edit: &PinLock) -> Result<PinWritten, ApiError> {
        self.write_pin(
            &edit.board,
            &edit.request,
            &pinwrite::Change::Lock(edit.locked),
        )
    }

    fn pin_move(&self, edit: &PinMove) -> Result<PinWritten, ApiError> {
        self.write_pin(
            &edit.board,
            &edit.request,
            &pinwrite::Change::Move(edit.pin.clone()),
        )
    }

    fn build(&self, say: &Say, cancel: &Cancel) -> Result<BuildReport, ApiError> {
        cancellable(cancel, "the build", || self.write_schematics(say))
    }

    fn fab(
        &self,
        board: &str,
        allow_drc_violations: bool,
        say: &Say,
        cancel: &Cancel,
    ) -> Result<FabReport, ApiError> {
        cancellable(cancel, "fab", || {
            self.make_fab(board, allow_drc_violations, say)
        })?
    }

    fn download(&self, board: &str, file: &str) -> Result<Attachment, ApiError> {
        self.fetch(board, file)
    }

    fn orders(&self) -> Vec<OrderInfo> {
        self.list_orders()
    }

    fn fab_order(
        &self,
        name: &str,
        allow_drc_violations: bool,
        say: &Say,
        cancel: &Cancel,
    ) -> Result<OrderReport, ApiError> {
        cancellable(cancel, "the order", || {
            self.make_order(name, allow_drc_violations, say)
        })?
    }

    fn order_download(&self, name: &str, file: &str) -> Result<Attachment, ApiError> {
        self.fetch_order(name, file)
    }

    fn git(&self) -> GitInfo {
        self.git_info()
    }

    fn compare(
        &self,
        rev: &str,
        board: Option<&str>,
        observe: &Observer,
        cancel: &Cancel,
    ) -> Result<Compare, ApiError> {
        self.compare_against(rev, None, board, observe, cancel)
    }

    fn compare_against(
        &self,
        rev: &str,
        against: Option<&str>,
        board: Option<&str>,
        observe: &Observer,
        cancel: &Cancel,
    ) -> Result<Compare, ApiError> {
        cancellable(cancel, "the compare", || {
            self.compare_with(rev, against, board, observe, cancel)
        })?
    }

    fn git_file(&self, rev: &str, path: &str) -> Result<FileContent, ApiError> {
        self.git_file_at(rev, path)
    }

    fn definition(
        &self,
        path: &str,
        text: &str,
        offset: usize,
    ) -> Result<Option<Definition>, ApiError> {
        let entry = self.entry(path)?;
        let loader = self.target().loader().map_err(ApiError::internal)?;
        Ok(
            definition::find(&loader, &entry.path, text, offset).map(|found| Definition {
                at: found.at,
                signature: found.signature,
                doc: found.doc,
            }),
        )
    }
}
