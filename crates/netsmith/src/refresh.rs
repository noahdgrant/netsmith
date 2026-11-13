//! The catalog refresh: the library is in `netsmith-catalog`; which files a catalog command acts
//! on needs the project's manifest, so it is here.

use std::path::{Path, PathBuf};

pub use netsmith_catalog::refresh::*;

use crate::project::{MANIFEST, Project};

/// The files to act on. With `paths`, each is a catalog file or a directory of them. With
/// none: the catalog files of the project that contains `cwd` (its `catalog_dirs` folders and
/// its `catalogs` list)
/// and, in the netsmith repository, the standard ones in `data/jlcpcb/catalog/`. The standard
/// catalogs are inside the `netsmith` binary too, but the binary's copy cannot be written, so
/// they are refreshed in the repository's files.
pub fn resolve_targets(cwd: &Path, paths: &[PathBuf]) -> Result<Vec<Target>, String> {
    let mut targets = Vec::new();
    if paths.is_empty() {
        if let Some(project) = Project::find(cwd)? {
            for file in project.catalog_files()? {
                targets.push(Target::of(file.path, cwd)?);
            }
        }
        let std = cwd.join(STD_DIR);
        if std.is_dir() {
            targets.extend(directory(&std, cwd)?);
        }
        if targets.is_empty() {
            return Err(format!(
                "there are no catalog files to act on: {MANIFEST} (here or above) has no catalog files (`catalog_dirs`, `catalogs`), and there is no {STD_DIR}/ here; name a catalog file or a directory of them"
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
