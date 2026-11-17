//! The router's copper kept on disk (ADR 0030, issue #1019): one file for each board of a
//! source, `.netsmith/routes/<stem>.<board>.toml` in the project's folder (the folder of
//! `netsmith.toml`, or the source's when it has none), holding a [`netsmith_router::kept::Kept`].
//!
//! It is a cache, never committed: the folder `.netsmith` gets a `.gitignore` of its own that
//! ignores everything in it, and `netsmith init` ignores it in the project's. The editor writes
//! it after every route that ran to its end and starts a route from it when it has no routed
//! board of its own; `netsmith layout regenerate` says what of it a regeneration keeps; `fab`
//! reads and writes it only with `--reuse-routes`, so that a clean checkout gives the same board
//! on every machine. A file that cannot be read, or that is stale ([`netsmith_router::kept::Stale`]),
//! is not used: the board is routed from nothing, and the reason is said.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use netsmith_pcb::Board;
use netsmith_router::kept::Kept;

/// The folder of the kept files, in the project's folder.
pub const DIR: &str = ".netsmith/routes";

/// Where the kept files of one source are.
#[derive(Debug, Clone, PartialEq)]
pub struct KeptRoutes {
    /// The project's folder.
    pub root: PathBuf,
    /// The source's file name without `.nsm`.
    pub stem: String,
    /// The source, from the project's folder with `/`: a kept file of another source of the same
    /// stem is not used.
    pub source: String,
}

/// What [`KeptRoutes::start_from`] found.
pub struct Start {
    /// The board the kept copper was routed on, to carry from.
    pub before: Board,
    /// The nets whose pads are not where the kept file has them: routed again.
    pub rip: BTreeSet<String>,
    /// How many nets the file keeps copper of.
    pub nets: usize,
    pub file: PathBuf,
}

impl KeptRoutes {
    /// The kept files of the source whose layout file is `layout` (`<stem>.pcb.toml`).
    pub fn beside_layout(layout: &Path) -> KeptRoutes {
        let layout = std::path::absolute(layout).unwrap_or_else(|_| layout.to_path_buf());
        let folder = layout.parent().unwrap_or(Path::new(".")).to_path_buf();
        let name = layout
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        let stem = name
            .strip_suffix(".pcb.toml")
            .unwrap_or(name.as_str())
            .to_string();
        let root = crate::project::Project::find(&folder)
            .ok()
            .flatten()
            .map_or(folder.clone(), |p| p.root);
        let source = folder
            .join(format!("{stem}.nsm"))
            .strip_prefix(&root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| format!("{stem}.nsm"));
        KeptRoutes { root, stem, source }
    }

    /// The kept file of `board`.
    pub fn file(&self, board: &str) -> PathBuf {
        let board = board.replace(['/', '\\'], "-");
        self.root
            .join(DIR)
            .join(format!("{}.{board}.toml", self.stem))
    }

    /// The kept copper of `board`, if there is a file: `Err` says why a file is not used.
    pub fn read(&self, board: &str) -> Result<Option<Kept>, String> {
        let file = self.file(board);
        match std::fs::read_to_string(&file) {
            Ok(text) => toml::from_str(&text)
                .map(Some)
                .map_err(|e| format!("{}: {}", file.display(), e.message())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("{}: {e}", file.display())),
        }
    }

    /// The board to carry the copper of onto `placed` (the board as placed now, with the routes
    /// file's copper and no other), from the kept file of its board. `Ok(None)` when there is no
    /// file; `Err` says why the file is not used.
    pub fn start_from(&self, placed: &Board) -> Result<Option<Start>, String> {
        let file = self.file(&placed.name);
        let Some(kept) = self.read(&placed.name)? else {
            return Ok(None);
        };
        let (before, rip) = kept
            .before(placed, &self.source)
            .map_err(|why| format!("{}: not used: {why}", file.display()))?;
        Ok(Some(Start {
            before,
            rip,
            nets: kept.len(),
            file,
        }))
    }

    /// Keep the router's copper of `routed`, routed from `start` (as placed, with the routes
    /// file's copper), over the board's kept file. Gives the file written.
    pub fn write(&self, start: &Board, routed: &Board) -> Result<PathBuf, String> {
        let file = self.file(&start.name);
        let kept = Kept::of(start, routed, &self.source);
        let text = toml::to_string(&kept).map_err(|e| format!("{}: {e}", file.display()))?;
        let folder = self.root.join(DIR);
        std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        // The cache ignores itself, whatever the project's `.gitignore` says.
        let ignore = self.root.join(".netsmith/.gitignore");
        if !ignore.exists() {
            std::fs::write(&ignore, "# netsmith's cache: never committed\n*\n")
                .map_err(|e| format!("{}: {e}", ignore.display()))?;
        }
        crate::routes::write_atomic(&file, &text)
            .map_err(|e| format!("{}: {e}", file.display()))?;
        Ok(file)
    }
}
