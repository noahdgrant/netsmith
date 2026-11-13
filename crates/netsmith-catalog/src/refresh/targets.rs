//! A catalog file a catalog command acts on. Which ones a command acts on (it reads the project's
//! manifest) is `resolve_targets` in `netsmith`.

use std::path::{Path, PathBuf};

/// The standard catalogs' directory in the netsmith repository.
pub const STD_DIR: &str = "data/jlcpcb/catalog";

/// A catalog file and what messages call it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    /// The path as shown in a message: relative to the directory the command ran in when it
    /// can be.
    pub shown: String,
    /// The file's name without `.csv`.
    pub catalog: String,
}

impl Target {
    pub fn of(path: PathBuf, base: &Path) -> Result<Target, String> {
        let catalog = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".csv"))
            .ok_or_else(|| {
                format!(
                    "{}: a catalog file is named after its catalog and ends in .csv",
                    path.display()
                )
            })?
            .to_string();
        let shown = path
            .strip_prefix(base)
            .unwrap_or(&path)
            .display()
            .to_string();
        Ok(Target {
            path,
            shown,
            catalog,
        })
    }
}

/// The `.csv` files directly in a directory, sorted.
pub fn directory(dir: &Path, base: &Path) -> Result<Vec<Target>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "csv"))
        .collect();
    files.sort();
    files.into_iter().map(|p| Target::of(p, base)).collect()
}
