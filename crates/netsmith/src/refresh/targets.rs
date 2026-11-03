//! Which catalog files a catalog command acts on.

use std::path::{Path, PathBuf};

use crate::project::{MANIFEST, Project};

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
    fn of(path: PathBuf, base: &Path) -> Result<Target, String> {
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

/// The files to act on. With `paths`, each is a catalog file or a directory of them. With
/// none: the catalogs the project's `netsmith.toml` lists (the project that contains `cwd`)
/// and, in the netsmith repository, the standard ones in `data/jlcpcb/catalog/`. The standard
/// catalogs are inside the `netsmith` binary too, but the binary's copy cannot be written, so
/// they are refreshed in the repository's files.
pub fn resolve_targets(cwd: &Path, paths: &[PathBuf]) -> Result<Vec<Target>, String> {
    let mut targets = Vec::new();
    if paths.is_empty() {
        if let Some(project) = Project::find(cwd)? {
            for file in &project.manifest.catalogs {
                targets.push(Target::of(project.root.join(file), cwd)?);
            }
        }
        let std = cwd.join(STD_DIR);
        if std.is_dir() {
            targets.extend(directory(&std, cwd)?);
        }
        if targets.is_empty() {
            return Err(format!(
                "there are no catalog files to act on: {MANIFEST} (here or above) lists no `catalogs`, and there is no {STD_DIR}/ here; name a catalog file or a directory of them"
            ));
        }
        return Ok(targets);
    }
    for path in paths {
        let path = cwd.join(path);
        if path.is_dir() {
            targets.extend(directory(&path, cwd)?);
        } else if path.is_file() {
            targets.push(Target::of(path, cwd)?);
        } else {
            return Err(format!("{}: no such file or directory", path.display()));
        }
    }
    Ok(targets)
}

/// The `.csv` files directly in a directory, sorted.
fn directory(dir: &Path, base: &Path) -> Result<Vec<Target>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "csv"))
        .collect();
    files.sort();
    files.into_iter().map(|p| Target::of(p, base)).collect()
}
