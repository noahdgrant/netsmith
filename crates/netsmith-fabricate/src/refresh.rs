//! The catalog refresh: the library is in `netsmith-catalog`; which files a catalog command acts
//! on needs the project's manifest, so it is here.

use std::path::{Path, PathBuf};

pub use netsmith_catalog::refresh::*;

use crate::project::{MANIFEST, Project};

/// The standard offers' directory in the netsmith repository.
pub const STD_OFFERS_DIR: &str = "data/offers";

/// The files to act on: catalog files, and offers files (a `.csv` named for a supplier,
/// `lcsc.csv`, ADR 0143). With `paths`, each is a catalog or offers file or a
/// directory of them; when none of them is an offers file, the offers are the project's (that
/// contains `cwd`), else those of an `offers` folder beside or above a catalog folder named. With
/// none: the catalog and offers files of the project that contains `cwd` (its `catalog_dirs`
/// folders and its `catalogs` list, its `offer_dirs` folders) and, in the netsmith repository,
/// the standard ones in `data/jlcpcb/catalog/` and `data/offers/`. The standard catalogs and
/// offers are inside the `netsmith` binary too, but the binary's copy cannot be written, so they
/// are refreshed in the repository's files.
pub fn resolve_targets(cwd: &Path, paths: &[PathBuf]) -> Result<Targets, String> {
    let mut targets = Targets::default();
    let project = Project::find(cwd)?;
    let own_offers = |targets: &mut Targets| -> Result<(), String> {
        if let Some(project) = &project {
            for file in project.offer_files()? {
                targets.offers.push(Target::of(file.path, cwd)?);
            }
        }
        Ok(())
    };
    if paths.is_empty() {
        if let Some(project) = &project {
            for file in project.catalog_files()? {
                targets.catalogs.push(Target::of(file.path, cwd)?);
            }
        }
        own_offers(&mut targets)?;
        let std = cwd.join(STD_DIR);
        if std.is_dir() {
            targets.catalogs.extend(directory(&std, cwd)?);
        }
        let std_offers = cwd.join(STD_OFFERS_DIR);
        if std_offers.is_dir() {
            targets.offers.extend(directory(&std_offers, cwd)?);
        }
        if targets.catalogs.is_empty() {
            return Err(format!(
                "there are no catalog files to act on: {MANIFEST} (here or above) has no catalog files (`catalog_dirs`, `catalogs`), and there is no {STD_DIR}/ here; name a catalog file or a directory of them"
            ));
        }
        return Ok(targets);
    }
    let mut folders = Vec::new();
    for path in paths {
        let path = cwd.join(path);
        let found = if path.is_dir() {
            folders.push(path.clone());
            directory(&path, cwd)?
        } else if path.is_file() {
            if let Some(dir) = path.parent() {
                folders.push(dir.to_path_buf());
            }
            vec![Target::of(path, cwd)?]
        } else {
            return Err(format!("{}: no such file or directory", path.display()));
        };
        for target in found {
            // `jlcpcb.csv` is an offers file, refused with what to do when it is read.
            if netsmith_solve::offers::is_supplier(&target.catalog)
                || target.catalog == netsmith_solve::offers::MERGED_INTO_LCSC
            {
                targets.offers.push(target);
            } else {
                targets.catalogs.push(target);
            }
        }
    }
    if targets.offers.is_empty() {
        own_offers(&mut targets)?;
    }
    if targets.offers.is_empty() {
        // An `offers` folder beside the catalogs' folder (`catalogs/`, `offers/`) or above it
        // (`data/jlcpcb/catalog/`, `data/offers/`).
        let near = folders.iter().find_map(|f| {
            [f.parent(), f.parent().and_then(Path::parent)]
                .into_iter()
                .flatten()
                .map(|p| p.join(netsmith_solve::offers::FOLDER))
                .find(|p| p.is_dir())
        });
        if let Some(dir) = near {
            targets.offers.extend(directory(&dir, cwd)?);
        }
    }
    Ok(targets)
}
