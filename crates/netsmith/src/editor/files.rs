//! The files of the project as the editor names them, and reading and saving them.

use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use netsmith_check::Loader;
use netsmith_editor::{
    ApiError,
    api::{Def, FileContent, FileRole, Project, Saved, TreeFile},
};

use super::Workspace;
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
        let files = self
            .entries()
            .into_iter()
            .map(|e| {
                let text = self.text_of(&e).ok();
                let defs = match (&e.role, &text) {
                    (FileRole::Manifest, _) | (_, None) => Vec::new(),
                    (_, Some(text)) => definitions(text),
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
            kicad: netsmith_fab::KicadCliExe::locate().is_some(),
            files,
        }
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

/// The definitions of a source text, with the line each starts on. A text that does not
/// parse has none shown.
fn definitions(text: &str) -> Vec<Def> {
    let Ok(file) = netsmith_lang::parse(text) else {
        return Vec::new();
    };
    file.definitions
        .iter()
        .map(|d| Def {
            kind: d.kind.keyword().to_string(),
            name: d.name.name.clone(),
            line: text[..d.span.start.min(text.len())].matches('\n').count() as u32 + 1,
        })
        .collect()
}
