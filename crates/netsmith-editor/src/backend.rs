//! What the server needs from a project, in one trait, so the server is tested without a
//! compiler behind it and the compiler's crates do not depend on a web server.

use netsmith_core::job::Cancel;

use crate::api::{
    Analysis, BoardDetail, Bom, BuildReport, CatalogRefresh, Changed, Compare, Definition,
    FabReport, FileContent, GitInfo, OrderInfo, OrderReport, PanelView, PcbEdit, PcbEdited,
    PcbFindings, PcbReport, PcbScene, PowerTree, Project, Saved, SchematicHints, SystemView,
};

/// A refusal, with the HTTP status that says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
    /// What the failure is, for a page that asks for it (`Accept: application/json`); the
    /// plain-text body is `message` either way.
    pub detail: Option<ErrorDetail>,
}

/// The structured form of a failure of the project's own data (a part with no footprint, a
/// pad the footprint lacks, ...), as opposed to a fault of the server. The server answers a
/// request that accepts JSON with
/// `{"code": "...", "message": "...", "<field>": "...", ...}`: `code` is one of a fixed list
/// of snake_case words, `message` is the text of the plain-text body, and the other keys are
/// the strings of `fields` (which ones depends on the code).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorDetail {
    pub code: String,
    pub fields: Vec<(String, String)>,
}

impl ApiError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            detail: None,
        }
    }

    /// The project's own data cannot be used for what was asked (`422`): the request is
    /// right and the server is working, the files are what is wrong. `code` and `fields` are
    /// an [`ErrorDetail`].
    pub fn unprocessable(message: impl Into<String>, code: &str, fields: &[(&str, &str)]) -> Self {
        Self {
            status: 422,
            message: message.into(),
            detail: Some(ErrorDetail {
                code: code.to_string(),
                fields: fields
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            }),
        }
    }

    /// The JSON body of a failure that has a [`detail`](Self::detail).
    pub fn json(&self) -> Option<String> {
        let detail = self.detail.as_ref()?;
        let mut body = serde_json::Map::new();
        body.insert("code".into(), detail.code.clone().into());
        body.insert("message".into(), self.message.clone().into());
        for (key, value) in &detail.fields {
            body.entry(key.clone())
                .or_insert_with(|| value.clone().into());
        }
        serde_json::to_string(&body).ok()
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(400, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(403, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(404, message)
    }

    /// The file changed since the page read it.
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(409, message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(500, message)
    }

    /// The job was stopped on request (or because the request that asked for it went away):
    /// `499`, with the structured form `{"code": "cancelled", "message": ...}`. Nothing the job
    /// was making was kept.
    pub fn cancelled(message: impl Into<String>) -> Self {
        Self {
            status: 499,
            message: message.into(),
            detail: Some(ErrorDetail {
                code: CANCELLED.to_string(),
                fields: Vec::new(),
            }),
        }
    }

    /// Whether this is [`ApiError::cancelled`].
    pub fn is_cancelled(&self) -> bool {
        self.status == 499
    }
}

/// The `code` of the structured body of a cancelled job.
pub const CANCELLED: &str = "cancelled";

/// A file to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// How a long job tells the page what it is doing: one line at a time, as the command line
/// prints them. It may be called from any thread.
pub type Say = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

/// How far a long call has got through a unit of work it counts as it goes (ADR 0062): `done`
/// of `total` `unit`s, or only `done` when the total is not known. Never an estimate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Work {
    /// What is counted, in the plural (`parts chosen`).
    pub unit: String,
    pub done: u32,
    pub total: Option<u32>,
}

/// What an analysis of the files on disk tells the server while it runs (not the analysis of
/// text on the page, which is a check).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisEvent {
    /// It began; `since` is when, in milliseconds since the Unix epoch.
    Started { since: u64 },
    /// How far it has got: `stage` is `check`, `solve`, `lower`, `draw` or `modules`, `done`
    /// of `total` of what that stage works through, and `board` is the board or file it is on.
    Progress {
        stage: String,
        board: String,
        done: u32,
        total: u32,
        message: String,
        /// What the stage has counted so far inside the board, if it counts anything (the
        /// parts the solver has chosen).
        work: Option<Work>,
    },
    /// It was cancelled: nothing it made was kept, and the last reading stays. Sent before
    /// [`AnalysisEvent::Finished`].
    Cancelled,
    /// It ended in a panic. Sent before [`AnalysisEvent::Finished`]; the next `analysis` is the
    /// last good reading with an error problem that names the panic, and its version is new, so
    /// the page hears `rebuilt` and leaves "analysing".
    Failed { message: String },
    /// It ended, however it ended (the result is the next `analysis`).
    Finished,
}

/// Who hears of an [`AnalysisEvent`]. It may be called from any thread and must not block.
pub type Observer = std::sync::Arc<dyn Fn(AnalysisEvent) + Send + Sync>;

/// A project as the editor sees it. Each call is a whole request: the server runs them on
/// threads of their own, so a long one (`fab`) does not hold up a keystroke's check.
pub trait Backend: Send + Sync + 'static {
    /// What the page is titled: the project's name, or the file's.
    fn title(&self) -> String;
    fn project(&self) -> Project;
    fn read_file(&self, path: &str) -> Result<FileContent, ApiError>;
    /// Write `text` to the file if it is still as it was at `stamp`.
    fn write_file(&self, path: &str, text: &str, stamp: &str) -> Result<Saved, ApiError>;
    /// Make a file that is not there yet, under the project's `src/`. Refused with 409 if the
    /// path is taken (nothing is overwritten), with 400 for a name that is not a module's, and
    /// with 403 for a path that leaves the project.
    fn create_file(&self, path: &str, text: &str) -> Result<Changed, ApiError>;
    /// Make a folder under `src/`, refused as [`Backend::create_file`] is.
    fn create_dir(&self, path: &str) -> Result<Changed, ApiError>;
    /// Move a file or folder of the tree to a name that is free, in the same folder or another.
    fn rename_path(&self, from: &str, to: &str) -> Result<Changed, ApiError>;
    /// Remove a file or folder of the tree. Without `confirm` a folder with something in it and
    /// a top-level file are refused with 428; the manifest and the standard library never are
    /// removed.
    fn delete_path(&self, path: &str, confirm: bool) -> Result<Changed, ApiError>;
    /// The check of the files on disk, made again first if one changed. Its `version` goes
    /// up each time that happens.
    fn analysis(&self) -> Analysis;
    /// Stop the analysis of the files on disk that is running, if one is: it ends at its next
    /// checkpoint, publishes nothing, and the last good reading stays (ADR 0062). True if one was
    /// running. The next change to a file, or [`Backend::run_analysis`], analyses again.
    fn cancel_analysis(&self) -> bool {
        false
    }
    /// Analyse the files on disk now if the last analysis of them was cancelled (or none is
    /// running and a file changed): the page's "Run again". Answers with the reading.
    fn run_analysis(&self) -> Analysis {
        self.analysis()
    }
    /// `Some(milliseconds the first analysis has been running)` until it has finished, `None`
    /// from then on. A backend whose files are read before it is served is never pending.
    /// While it is `Some`, `analysis` blocks until the first analysis is done, and `version`
    /// is `0` before that. It must not wait for anything: the server calls it from its
    /// event loop.
    fn pending(&self) -> Option<u32> {
        None
    }
    /// Told once, before the server accepts a connection: every analysis of the files on disk
    /// (the first, and each that a change on disk makes) reports to `observer`. A backend with
    /// nothing to report ignores it.
    fn observe(&self, observer: Observer) {
        let _ = observer;
    }
    /// The check with `path` as `text`, in memory.
    fn check(&self, path: &str, text: &str) -> Result<Analysis, ApiError>;
    fn board(&self, name: &str) -> Result<BoardDetail, ApiError>;
    fn schematic(&self, board: &str) -> Result<String, ApiError>;
    /// The schematic layout file of the source file `board` is in, with what it says and how
    /// it matches the board (ADR 0061). A file that is not there is no error: no hints, no
    /// findings.
    fn schematic_hints(&self, board: &str) -> Result<SchematicHints, ApiError> {
        let _ = board;
        Err(ApiError::not_found(
            "this backend has no schematic layout file",
        ))
    }
    /// Write the file: `text` is read as the file is, and what is written is the canonical text
    /// of what it says (`to_toml`). Refused with `422` (`schematic_hints_invalid`, with the `line`
    /// of the first fault) if `text` does not read, and with `409` if `stamp` is given and the
    /// file is not as it was at that stamp; then nothing is written. The project is analysed
    /// again as the file changed, once, and the answer is the file as the drawing now has it.
    fn write_schematic_hints(
        &self,
        board: &str,
        text: &str,
        stamp: Option<&str>,
    ) -> Result<SchematicHints, ApiError> {
        let _ = (board, text, stamp);
        Err(ApiError::not_found(
            "this backend has no schematic layout file",
        ))
    }
    fn power(&self, board: &str) -> Result<PowerTree, ApiError>;
    /// A system by name, or a board by its name.
    fn system(&self, name: &str) -> Result<SystemView, ApiError>;
    /// A panel by name, drawn from its board as it is placed now (ADR 0040).
    fn panel(&self, name: &str) -> Result<PanelView, ApiError>;
    fn bom(&self, board: &str) -> Result<Bom, ApiError>;
    /// Check the rows of the project's own catalog files against LCSC's record and record the
    /// day in their `checked` column, as `netsmith catalog refresh` does (ADR 0039); no price
    /// or stock is written.
    /// `dry_run` writes nothing; `offline` uses only what an earlier run kept. `say` is told
    /// which part is being looked up.
    fn refresh_catalogs(
        &self,
        dry_run: bool,
        offline: bool,
        say: &Say,
    ) -> Result<CatalogRefresh, ApiError>;
    /// Place and route the board and keep the result for [`Backend::pcb`]. `say` is told the
    /// stage and, while routing, how far it has got.
    fn route(&self, board: &str, say: &Say) -> Result<PcbReport, ApiError>;
    /// Stop a `route` that is running: it returns what it has routed so far.
    fn cancel_route(&self);
    /// The board as placed (and routed, if it was routed since the files were last read), with
    /// the drawing and what the canvas needs to find things in it.
    fn pcb_scene(&self, board: &str) -> Result<PcbScene, ApiError>;
    /// What is wrong with the board as it is placed (and routed) now, with where: netsmith's
    /// own checks, and KiCad's if it was run on this layout.
    fn pcb_findings(&self, board: &str) -> Result<PcbFindings, ApiError>;
    /// Run KiCad's design rules check on the board as it is, keep what it found for
    /// [`Backend::pcb_findings`] until the board changes, and answer with all the findings.
    /// `say` is told what it is doing.
    fn pcb_drc(&self, board: &str, say: &Say) -> Result<PcbFindings, ApiError>;
    /// Write changes to the layout file beside the board's source and answer with the board as
    /// it is placed now. Refused with 409 if the file is not as the page read it, or an entry
    /// is not as the change says it was, and with 400 if an entry is wrong; then nothing is
    /// written.
    fn pcb_edit(&self, edit: &PcbEdit) -> Result<PcbEdited, ApiError>;
    /// Write the KiCad schematics. `cancel` stops it at its next checkpoint (`ApiError::cancelled`);
    /// the schematics of a file written before are kept, none of the board it was on.
    fn build(&self, say: &Say, cancel: &Cancel) -> Result<BuildReport, ApiError>;
    /// `allow_drc_violations`: go on although the design rules check fails or cannot run.
    /// `cancel` stops it at its next checkpoint (`ApiError::cancelled`); the packages of the
    /// boards it finished are kept, nothing of the board it was on.
    fn fab(
        &self,
        board: &str,
        allow_drc_violations: bool,
        say: &Say,
        cancel: &Cancel,
    ) -> Result<FabReport, ApiError>;
    fn download(&self, board: &str, file: &str) -> Result<Attachment, ApiError>;
    /// The orders of the manifest (ADR 0035).
    fn orders(&self) -> Vec<OrderInfo>;
    /// Run the named order, as `netsmith fab --order` does.
    fn fab_order(
        &self,
        name: &str,
        allow_drc_violations: bool,
        say: &Say,
        cancel: &Cancel,
    ) -> Result<OrderReport, ApiError>;
    /// `summary` or `bom` of an order that was run.
    fn order_download(&self, name: &str, file: &str) -> Result<Attachment, ApiError>;
    /// Where the name at character `offset` of `text` is defined.
    fn definition(
        &self,
        path: &str,
        text: &str,
        offset: usize,
    ) -> Result<Option<Definition>, ApiError>;
    /// Whether the project is in a git work tree and what it can be compared with (ADR 0059).
    /// A backend with no git says so.
    fn git(&self) -> GitInfo {
        GitInfo {
            available: false,
            reason: Some("this backend has no git".to_string()),
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
    /// What changed in the circuit between `rev` and the files on disk, for one board or all.
    /// It may take as long as an analysis; `observe` is told how far it has got (the events of
    /// an analysis, and the stages `extract` and `diff`), from any thread. Refused with 404 for
    /// a ref that does not exist and with 422 (`git_unavailable`) for a project that is not in
    /// a git work tree. `cancel` stops it at its next checkpoint (`ApiError::cancelled`); what it
    /// had read of the ref is dropped, and the live project's analysis is not touched.
    fn compare(
        &self,
        rev: &str,
        board: Option<&str>,
        observe: &Observer,
        cancel: &Cancel,
    ) -> Result<Compare, ApiError> {
        let _ = (rev, board, observe, cancel);
        Err(ApiError::not_found("this backend cannot compare"))
    }
    /// [`Backend::compare`] with the other side a ref too: `against` is the head, a commit
    /// read out of git as `rev` is, instead of the files on disk (`None`: the files on disk).
    /// Refused with 404 for an `against` that does not exist, as `rev` is.
    fn compare_against(
        &self,
        rev: &str,
        against: Option<&str>,
        board: Option<&str>,
        observe: &Observer,
        cancel: &Cancel,
    ) -> Result<Compare, ApiError> {
        match against {
            None => self.compare(rev, board, observe, cancel),
            Some(_) => Err(ApiError::not_found("this backend cannot compare two refs")),
        }
    }
    /// The text of a file of the project as it is at `rev`, read from git (`readonly`, the
    /// stamp is the commit's hash).
    fn git_file(&self, rev: &str, path: &str) -> Result<FileContent, ApiError> {
        let _ = (rev, path);
        Err(ApiError::not_found("this backend cannot read from git"))
    }
}
