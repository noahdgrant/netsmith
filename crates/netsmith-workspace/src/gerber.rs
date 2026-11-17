//! The Gerber view of the PCB area (issue #1107): a fab package read and drawn by
//! `netsmith_fab::viewer`, the reader `netsmith view gerber` prints and draws with, so the page
//! and the command cannot differ. The package is the one `fab` last wrote for a board (in this
//! session, else the one an earlier run left in the fab folder) or one the page sent, which is
//! kept so that another view of it is drawn without sending it again.

use std::path::{Path, PathBuf};

use netsmith_editor::{
    ApiError,
    api::{GerberExtent, GerberFile, GerberHoles, GerberSkipped, GerberUnread, GerberView},
};
use netsmith_fab::viewer::{self, Extent, Package, Show, ViewError};

use super::Workspace;

impl Workspace {
    /// The package `fab` last wrote for `board`, and its view `layer`.
    pub(super) fn gerber_of_board(
        &self,
        board: &str,
        layer: Option<&str>,
    ) -> Result<GerberView, ApiError> {
        let (label, written) = {
            let state = self.fresh();
            let label = state.snapshot.board(board)?.built.label();
            let written = state.fab.get(&label).and_then(|files| {
                files
                    .iter()
                    .find(|p| p.to_string_lossy().ends_with("-gerbers.zip"))
                    .cloned()
            });
            (label, written)
        };
        let target = self.target();
        let path = written
            .or_else(|| zip_in(&target.fab_out.join(&label)))
            .ok_or_else(|| {
                ApiError::not_found(format!(
                    "`fab` has written no package for `{label}`: run Make fab files first"
                ))
            })?;
        let pkg = viewer::open(&path).map_err(|e| ApiError::not_found(e.0))?;
        let root = target.project.as_ref().map(|p| p.root.as_path());
        let shown = shown_path(&path, root);
        view(&pkg, "fab", Some(shown), layer)
    }

    /// Reads a package the page sent, keeps it and draws its view `layer`.
    pub(super) fn gerber_open_package(
        &self,
        name: &str,
        bytes: Vec<u8>,
        layer: Option<&str>,
    ) -> Result<GerberView, ApiError> {
        let pkg = viewer::read_bytes(name, bytes)
            .map_err(|e| ApiError::bad_request(format!("{name}: {e}")))?;
        let answer = view(&pkg, "opened", None, layer)?;
        *self.gerber.lock().unwrap_or_else(|e| e.into_inner()) = Some(pkg);
        Ok(answer)
    }

    /// Another view of the package the page sent last.
    pub(super) fn gerber_of_opened(
        &self,
        name: &str,
        layer: Option<&str>,
    ) -> Result<GerberView, ApiError> {
        let kept = self.gerber.lock().unwrap_or_else(|e| e.into_inner());
        match kept.as_ref() {
            Some(pkg) if pkg.name == name => view(pkg, "opened", None, layer),
            _ => Err(ApiError::not_found(format!(
                "no package `{name}` is open: open it again"
            ))),
        }
    }
}

/// The package in a board's fab folder: its `-gerbers.zip`, else the folder when it has one.
fn zip_in(dir: &Path) -> Option<PathBuf> {
    let mut zips: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with("-gerbers.zip"))
        .collect();
    zips.sort();
    zips.into_iter().next()
}

/// A path as the command line would be given it: from the project's folder when it is in it.
fn shown_path(path: &Path, root: Option<&Path>) -> String {
    let rel = root.and_then(|r| path.strip_prefix(r).ok()).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

fn extent(e: Extent) -> GerberExtent {
    GerberExtent {
        x: e.x,
        y: e.y,
        width: e.width,
        height: e.height,
    }
}

/// The summary of `pkg` and its view `layer` (the first of its views when none is asked).
fn view(
    pkg: &Package,
    source: &str,
    path: Option<String>,
    layer: Option<&str>,
) -> Result<GerberView, ApiError> {
    if pkg.files.is_empty() {
        return Err(ApiError::bad_request(format!(
            "{} has no Gerber or drill file",
            pkg.name
        )));
    }
    let summary = pkg.summary();
    let show: Show = match layer {
        Some(l) => l.parse().map_err(ApiError::bad_request)?,
        None => summary
            .views
            .first()
            .and_then(|v| v.parse().ok())
            .unwrap_or(Show::All),
    };
    let (svg, region, mirrored, drawn) = match viewer::draw(pkg, show, None) {
        Ok(d) => (d.svg, d.region.to_vec(), d.mirrored, d.drawn),
        // Nothing of the view could be read: the answer still lists what was not understood.
        Err(ViewError::Unread { .. }) => (String::new(), Vec::new(), false, Vec::new()),
        Err(e @ ViewError::NoLayer { .. }) => return Err(ApiError::bad_request(e.to_string())),
        Err(e) => return Err(ApiError::internal(e.to_string())),
    };
    Ok(GerberView {
        package: summary.package,
        source: source.to_string(),
        path,
        board: summary.board.map(|b| GerberExtent {
            x: 0.0,
            y: 0.0,
            width: b.width,
            height: b.height,
        }),
        views: summary.views,
        files: summary
            .layers
            .into_iter()
            .map(|f| GerberFile {
                file: f.file,
                format: f.format.to_string(),
                function: f.function.to_string(),
                layer: f.layer,
                identified_by: f.identified_by.map(str::to_string),
                file_function: f.file_function,
                read: f.read,
                extent: f.extent.map(extent),
                holes: f.holes,
                slots: f.slots,
            })
            .collect(),
        holes: summary
            .holes
            .into_iter()
            .map(|h| GerberHoles {
                diameter: h.diameter,
                plated: h.plated,
                holes: h.holes,
                slots: h.slots,
            })
            .collect(),
        unread: summary
            .unread
            .into_iter()
            .map(|u| GerberUnread {
                file: u.file,
                line: u.line,
                message: u.message,
            })
            .collect(),
        skipped: summary
            .skipped
            .into_iter()
            .map(|s| GerberSkipped {
                file: s.file,
                why: s.why,
            })
            .collect(),
        layer: show.to_string(),
        svg,
        region,
        mirrored,
        drawn,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_in_the_project_is_shown_from_its_folder() {
        assert_eq!(
            shown_path(
                Path::new("/p/fab/led/led-gerbers.zip"),
                Some(Path::new("/p"))
            ),
            "fab/led/led-gerbers.zip"
        );
        assert_eq!(
            shown_path(Path::new("/q/x.zip"), Some(Path::new("/p"))),
            "/q/x.zip"
        );
    }
}
