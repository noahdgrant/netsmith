//! "Check parts against LCSC" in the bill of materials: `netsmith catalog refresh` over the project's own
//! catalog files, with its progress on the page (ADR 0039).

use netsmith_editor::{
    ApiError, Say,
    api::{CatalogChange, CatalogFile, CatalogRefresh, CatalogRow},
};

use super::Workspace;
use crate::refresh::{self, Network, Options, Report, Status};

impl Workspace {
    /// The same code as the command line, over the files the manifest lists. The standard
    /// catalogs are inside the binary and cannot be written, so a project that has no
    /// catalog files has nothing to refresh, and the page is told so. A run that has to stop
    /// (blocked, a changed endpoint) answers with the reason and has written nothing.
    pub(super) fn refresh_project_catalogs(
        &self,
        dry_run: bool,
        offline: bool,
        say: &Say,
    ) -> Result<CatalogRefresh, ApiError> {
        let answer = |error: Option<String>, files, requests, summary| CatalogRefresh {
            dry_run,
            offline,
            files,
            requests,
            error,
            summary,
            notice: refresh::NOTICE.to_string(),
        };
        let Some(project) = &self.target().project else {
            return Ok(answer(
                Some("this is not a project (there is no netsmith.toml), so it has no catalog files of its own to refresh".into()),
                Vec::new(),
                0,
                String::new(),
            ));
        };
        let paths: Vec<std::path::PathBuf> = project
            .catalog_files()
            .map_err(ApiError::bad_request)?
            .into_iter()
            .map(|f| f.path)
            .collect();
        if paths.is_empty() {
            return Ok(answer(
                Some("netsmith.toml has no catalog files of its own (no `*.csv` in its `catalog_dirs` folders and none under `catalogs`). The standard catalogs are built into netsmith; their rows are checked against LCSC in the netsmith repository (`netsmith catalog refresh` there) and reach you with a release".into()),
                Vec::new(),
                0,
                String::new(),
            ));
        }
        let targets =
            refresh::resolve_targets(&project.root, &paths).map_err(ApiError::bad_request)?;
        let options = Options {
            dry_run,
            network: Network {
                offline,
                ..self.network.clone()
            },
            ..Options::default()
        };
        Ok(match refresh::refresh(&targets, &options, &|m| say(m)) {
            Ok(report) => {
                let summary = refresh::render_summary(&report);
                answer(None, files_of(&report), report.requests, summary)
            }
            Err(e) => answer(Some(e), Vec::new(), 0, String::new()),
        })
    }
}

/// The rows that need a look; a verified row without a note is only counted.
fn files_of(report: &Report) -> Vec<CatalogFile> {
    report
        .files
        .iter()
        .map(|f| CatalogFile {
            file: f.file.clone(),
            looked_up: u32::try_from(f.rows.len()).unwrap_or(u32::MAX),
            verified: u32::try_from(f.count(|s| *s == Status::Verified)).unwrap_or(u32::MAX),
            rows: f
                .rows
                .iter()
                .filter(|r| !r.notes.is_empty() || r.status != Status::Verified)
                .map(|r| CatalogRow {
                    line: u32::try_from(r.line).unwrap_or(u32::MAX),
                    lcsc: r.lcsc.clone(),
                    status: r.status.word().to_string(),
                    message: r.status.why().unwrap_or_default().to_string(),
                    changes: r
                        .changes
                        .iter()
                        .map(|c| CatalogChange {
                            column: c.column.clone(),
                            from: c.from.clone(),
                            to: c.to.clone(),
                        })
                        .collect(),
                    notes: r.notes.clone(),
                })
                .collect(),
            written: f.written,
            notes: f.notes.clone(),
        })
        .collect()
}
