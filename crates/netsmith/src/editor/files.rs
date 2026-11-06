//! The files of the project as the editor names them, and reading and saving them.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use netsmith_check::Loader;
use netsmith_editor::{
    ApiError,
    api::{FileContent, FileRole, Project, Saved, TreeFile},
};

use super::{
    Workspace,
    outline::{self, Kinds},
};
use crate::project::MANIFEST;

/// Modification time and length of each file, `None` for one that cannot be read.
pub fn stamps(paths: &[PathBuf]) -> Vec<Option<(SystemTime, u64)>> {
    paths
        .iter()
        .map(|p| {
            fs::metadata(p)
                .and_then(|m| Ok((m.modified()?, m.len())))
                .ok()
        })
        .collect()
}

/// What a file was like when read, as text: a save gives it back, and is refused if the
/// file is different now.
pub(super) fn stamp_of(path: &Path) -> String {
    match fs::metadata(path).and_then(|m| Ok((m.modified()?, m.len()))) {
        Ok((time, len)) => {
            let nanos = time
                .duration_since(SystemTime::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            format!("{nanos}-{len}")
        }
        Err(_) => "none".to_string(),
    }
}

/// A file the editor may show.
pub struct Entry {
    pub path: String,
    pub role: FileRole,
    /// On disk, or `None` for the standard library, which is in the binary.
    pub fs: Option<PathBuf>,
    pub embedded: Option<String>,
    pub readonly: bool,
}

impl Workspace {
    /// Every file the editor knows, in tree order: the top-level files, the modules, the
    /// manifest and the standard library.
    fn entries(&self) -> Vec<Entry> {
        let mut entries: Vec<Entry> = self
            .target
            .files
            .iter()
            .map(|f| Entry {
                path: f.name.clone(),
                role: FileRole::TopLevel,
                fs: Some(f.path.clone()),
                embedded: None,
                readonly: false,
            })
            .collect();
        if let Some(project) = &self.target.project {
            for module in project.modules().unwrap_or_default() {
                if entries
                    .iter()
                    .any(|e| e.fs.as_deref().is_some_and(|p| same(p, &module.path)))
                {
                    continue;
                }
                entries.push(Entry {
                    path: module.name(&project.root),
                    role: FileRole::Module,
                    fs: Some(module.path.clone()),
                    embedded: None,
                    readonly: false,
                });
            }
            entries.push(Entry {
                path: MANIFEST.to_string(),
                role: FileRole::Manifest,
                fs: Some(project.root.join(MANIFEST)),
                embedded: None,
                readonly: true,
            });
        }
        // The physical file beside each source that has one (ADR 0048).
        let physical: Vec<Entry> = entries
            .iter()
            .filter(|e| matches!(e.role, FileRole::TopLevel | FileRole::Module))
            .filter_map(|e| {
                let source = e.fs.as_ref()?;
                let file = crate::physical::physical_path(source);
                file.is_file().then(|| Entry {
                    path: netsmith_check::physical_file_name(&e.path),
                    role: FileRole::Physical,
                    fs: Some(file),
                    embedded: None,
                    readonly: false,
                })
            })
            .collect();
        entries.extend(physical);
        let loader = Loader::std();
        for (_, file, text) in loader.modules() {
            entries.push(Entry {
                path: file.to_string(),
                role: FileRole::Std,
                fs: None,
                embedded: Some(text.to_string()),
                readonly: true,
            });
        }
        entries
    }

    /// The file named `path` in the tree. Nothing else can be read or written.
    pub(super) fn entry(&self, path: &str) -> Result<Entry, ApiError> {
        self.entries()
            .into_iter()
            .find(|e| e.path == path)
            .ok_or_else(|| ApiError::not_found(format!("`{path}` is not a file of this project")))
    }

    pub(super) fn tree(&self) -> Project {
        let kicad = netsmith_fab::KicadCliExe::locate();
        // What a declaration's type is, for the outline: the modules of the project and the
        // standard library, read again as `build` reads them. Without them (a module that
        // cannot be read) only the definitions of a file itself resolve.
        let loader = self.target.loader().ok();
        let kinds = loader.as_ref().map_or_else(Kinds::alone, Kinds::new);
        let files = self
            .entries()
            .into_iter()
            .map(|e| {
                let text = self.text_of(&e).ok();
                let defs = match (&e.role, &text) {
                    (FileRole::Manifest, _) | (_, None) => Vec::new(),
                    (_, Some(text)) => outline::definitions(text, &kinds),
                };
                TreeFile {
                    path: e.path,
                    role: e.role,
                    readonly: e.readonly,
                    defs,
                }
            })
            .collect();
        Project {
            title: self.target.title.clone(),
            has_manifest: self.target.project.is_some(),
            kicad: kicad.is_some(),
            files,
            folders: self.folders(),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
            git_describe: None,
            root: self.root().map(|r| r.display().to_string()),
            kicad_path: kicad.map(|k| k.exe().display().to_string()),
            bundle_hash: None,
        }
    }

    /// The project folder, absolute: the manifest's, or for a single file the one it is in.
    fn root(&self) -> Option<PathBuf> {
        let folder = match (&self.target.project, self.target.files.first()) {
            (Some(project), _) => project.root.clone(),
            (None, Some(file)) => file.path.parent()?.to_path_buf(),
            (None, None) => return None,
        };
        let folder = if folder.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            folder
        };
        std::path::absolute(folder).ok()
    }

    fn text_of(&self, entry: &Entry) -> Result<String, ApiError> {
        match (&entry.fs, &entry.embedded) {
            (Some(path), _) => fs::read_to_string(path)
                .map_err(|e| ApiError::internal(format!("{}: {e}", entry.path))),
            (None, Some(text)) => Ok(text.clone()),
            (None, None) => Err(ApiError::internal("a file with no text")),
        }
    }

    pub(super) fn read(&self, path: &str) -> Result<FileContent, ApiError> {
        let entry = self.entry(path)?;
        let text = self.text_of(&entry)?;
        Ok(FileContent {
            path: entry.path,
            text,
            stamp: entry.fs.as_deref().map_or("embedded".to_string(), stamp_of),
            readonly: entry.readonly,
        })
    }

    /// Write `text` over the file as a temporary file renamed into place, so a reader (or
    /// a crash) never sees half a file.
    pub(super) fn write(&self, path: &str, text: &str, stamp: &str) -> Result<Saved, ApiError> {
        let entry = self.entry(path)?;
        let Some(file) = entry.fs.filter(|_| !entry.readonly) else {
            return Err(ApiError::forbidden(format!("{path} is read-only")));
        };
        let now = stamp_of(&file);
        if now != stamp {
            return Err(ApiError::conflict(format!(
                "{path} changed on disk since it was opened; reload it before saving"
            )));
        }
        write_atomic(&file, text).map_err(|e| ApiError::internal(format!("{path}: {e}")))?;
        Ok(Saved {
            stamp: stamp_of(&file),
        })
    }
}

/// Write `text` over `file` as a temporary file renamed into place, so a reader (or a crash)
/// never sees half a file.
pub(super) fn write_atomic(file: &Path, text: &str) -> std::io::Result<()> {
    let name = file
        .file_name()
        .map_or(String::new(), |n| n.to_string_lossy().into_owned());
    let temporary = file.with_file_name(format!(".{name}.netsmith-save"));
    fs::write(&temporary, text)
        .and_then(|()| fs::rename(&temporary, file))
        .inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        })
}

fn same(a: &Path, b: &Path) -> bool {
    match (std::path::absolute(a), std::path::absolute(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
