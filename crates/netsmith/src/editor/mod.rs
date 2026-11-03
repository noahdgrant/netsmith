//! `netsmith editor`: the project as the editor's server sees it (ADR 0019). A
//! [`Workspace`] implements [`netsmith_editor::Backend`] over the files of a [`Target`]:
//! it reads and saves them, builds them again when one changes on disk, and answers with
//! the views of a board computed from the solved model.

mod analysis;
mod catalogs;
mod definition;
mod files;
mod findings;
mod orders;
mod panel;
mod pcb;
mod power;
mod progress;
mod release;
mod sourcing;
mod system;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use netsmith_editor::{
    ApiError, Attachment, Backend, Say,
    api::{
        Analysis, BoardDetail, Bom, BuildReport, CatalogRefresh, Definition, FabReport,
        FileContent, OrderInfo, OrderReport, PanelView, PcbEdit, PcbEdited, PcbFindings, PcbReport,
        PcbScene, PowerTree, Project, Saved, SystemView,
    },
};

use crate::project::Target;

pub use analysis::Snapshot;

/// The project behind the editor.
pub struct Workspace {
    target: Target,
    state: Mutex<State>,
    /// Held while a layout edit is made, so two edits are one after the other.
    editing: Mutex<()>,
    /// How "Check parts against LCSC" asks the endpoints (ADR 0039); the tests point it at a fake.
    network: crate::refresh::Network,
}

#[derive(Default)]
struct State {
    /// Modification time and length of each watched file as last read.
    stamp: Option<Vec<Option<(std::time::SystemTime, u64)>>>,
    /// The same for the layout files beside the top-level files, which do not make the
    /// project build again but do make a board placed again.
    layouts: Vec<Option<(std::time::SystemTime, u64)>>,
    /// Goes up by one each time the files are read again.
    version: u64,
    snapshot: Snapshot,
    /// Boards placed, and routed if asked, keyed by the board's name; cleared when the
    /// files are read again.
    pcb: HashMap<String, pcb::Entry>,
    /// What `fab` wrote for each board, keyed by the board's name.
    fab: HashMap<String, Vec<PathBuf>>,
    /// Where each order was last written, by the order's name.
    orders: HashMap<String, PathBuf>,
}

impl Workspace {
    /// Read and build the target's files once. Fails only if a file cannot be read at all;
    /// mistakes in their text are kept for the page to show.
    pub fn open(target: Target) -> Result<Workspace, String> {
        for file in &target.files {
            std::fs::metadata(&file.path).map_err(|e| format!("{}: {e}", file.name))?;
        }
        let workspace = Workspace {
            target,
            state: Mutex::new(State::default()),
            editing: Mutex::new(()),
            network: crate::refresh::Network::default(),
        };
        drop(workspace.fresh());
        Ok(workspace)
    }

    /// Where "Check parts against LCSC" asks, how long it waits and where it keeps answers.
    pub fn with_network(mut self, network: crate::refresh::Network) -> Workspace {
        self.network = network;
        self
    }

    pub fn title(&self) -> &str {
        &self.target.title
    }

    /// The layout files beside the top-level files (they may not exist).
    fn layout_files(&self) -> Vec<PathBuf> {
        self.target
            .files
            .iter()
            .map(|f| crate::layout::layout_path(&f.path))
            .collect()
    }

    /// The state, built again first if a watched file changed since it was last read.
    fn fresh(&self) -> MutexGuard<'_, State> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        // A layout file changed by another program: the boards are placed again, and the page
        // is told as it is for a source (a change the editor made itself is recorded in
        // `pcb_edit` and is not one).
        let layouts = files::stamps(&self.layout_files());
        if state.version > 0 && state.layouts != layouts {
            state.version += 1;
            state.pcb.clear();
        }
        state.layouts = layouts;
        let stamp = files::stamps(&self.target.watched());
        let all_read = stamp.iter().all(Option::is_some);
        if state.version > 0 && state.stamp.as_ref() == Some(&stamp) && all_read {
            return state;
        }
        state.stamp = Some(stamp);
        state.version += 1;
        state.pcb.clear();
        let snapshot = self.analyze(&HashMap::new(), Some(&state.snapshot));
        state.snapshot = snapshot;
        state
    }
}

impl Workspace {
    /// Write an edit to the layout file beside the board's source (ADR 0032), if the file and
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
        let mut layout = crate::layout::read(file, boards)
            .map_err(|e| ApiError::internal(format!("{name}: {e}")))?
            .map_or_else(netsmith_pcb::Layout::default, |(layout, _)| layout);
        pcb::change_layout(&mut layout, &data.built.design.name, &layers, &edit.changes)?;
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
}

impl Backend for Workspace {
    fn title(&self) -> String {
        self.target.title.clone()
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

    fn analysis(&self) -> Analysis {
        let state = self.fresh();
        state.snapshot.summary(state.version)
    }

    fn check(&self, path: &str, text: &str) -> Result<Analysis, ApiError> {
        let file = self.entry(path)?;
        let Some(fs) = file.fs.filter(|_| !file.readonly) else {
            return Err(ApiError::bad_request(format!("{path} cannot be edited")));
        };
        let version = self.fresh().version;
        let unsaved = HashMap::from([(fs, text.to_string())]);
        Ok(self.analyze(&unsaved, None).summary(version))
    }

    fn board(&self, name: &str) -> Result<BoardDetail, ApiError> {
        self.board_detail(name)
    }

    fn schematic(&self, board: &str) -> Result<String, ApiError> {
        let state = self.fresh();
        let data = state.snapshot.board(board)?;
        data.svg.clone().map_err(ApiError::internal)
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
        let catalogs = self.target.catalogs().map_err(ApiError::internal)?;
        let today =
            crate::refresh::date_of(self.network.now.unwrap_or_else(crate::refresh::now_secs));
        let own = self
            .target
            .project
            .as_ref()
            .map(|p| p.manifest.catalogs.clone())
            .unwrap_or_default();
        sourcing::bom(state.snapshot.board(board)?, &catalogs, &today, own)
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

    fn pcb_findings(&self, board: &str) -> Result<PcbFindings, ApiError> {
        let (version, data) = {
            let state = self.fresh();
            if let Some(entry) = state.pcb.get(board) {
                return Ok(entry.findings(board));
            }
            (state.version, state.snapshot.board_handle(board)?)
        };
        let entry = pcb::place(&data, board)?;
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

    fn build(&self, say: &Say) -> BuildReport {
        self.write_schematics(say)
    }

    fn fab(
        &self,
        board: &str,
        allow_drc_violations: bool,
        say: &Say,
    ) -> Result<FabReport, ApiError> {
        self.make_fab(board, allow_drc_violations, say)
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
    ) -> Result<OrderReport, ApiError> {
        self.make_order(name, allow_drc_violations, say)
    }

    fn order_download(&self, name: &str, file: &str) -> Result<Attachment, ApiError> {
        self.fetch_order(name, file)
    }

    fn definition(
        &self,
        path: &str,
        text: &str,
        offset: usize,
    ) -> Result<Option<Definition>, ApiError> {
        let entry = self.entry(path)?;
        let loader = self.target.loader().map_err(ApiError::internal)?;
        Ok(definition::find(&loader, &entry.path, text, offset).map(|at| Definition { at }))
    }
}
