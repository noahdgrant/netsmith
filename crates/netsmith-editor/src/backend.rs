//! What the server needs from a project, in one trait, so the server is tested without a
//! compiler behind it and the compiler's crates do not depend on a web server.

use crate::api::{
    Analysis, BoardDetail, Bom, BuildReport, CatalogRefresh, Changed, Definition, FabReport,
    FileContent, OrderInfo, OrderReport, PanelView, PcbEdit, PcbEdited, PcbFindings, PcbReport,
    PcbScene, PowerTree, Project, Saved, SystemView,
};

/// A refusal, with the HTTP status that says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl ApiError {
    pub fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
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
}

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
    /// The check with `path` as `text`, in memory.
    fn check(&self, path: &str, text: &str) -> Result<Analysis, ApiError>;
    fn board(&self, name: &str) -> Result<BoardDetail, ApiError>;
    fn schematic(&self, board: &str) -> Result<String, ApiError>;
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
    /// Write the KiCad schematics.
    fn build(&self, say: &Say) -> BuildReport;
    /// `allow_drc_violations`: go on although the design rules check fails or cannot run.
    fn fab(
        &self,
        board: &str,
        allow_drc_violations: bool,
        say: &Say,
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
}
