//! The build panel: writing the schematics, running `fab`, and fetching what it wrote.

use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use netsmith_editor::{
    ApiError, Attachment, Say,
    api::{BuildReport, Download, Drc, FabReport},
};

use super::{
    Workspace,
    progress::{Reporting, forward},
};
use netsmith_fabricate::fab::{FabOptions, fab_with_options};

/// The files `fab` writes that the page offers, by the name it asks for them with.
const DOWNLOADS: [(&str, &str); 5] = [
    ("gerbers", "-gerbers.zip"),
    ("bom", "bom.csv"),
    ("cpl", "cpl.csv"),
    ("notes", "fab-notes.txt"),
    ("pcb", ".kicad_pcb"),
];

impl Workspace {
    /// What `netsmith build` does: a KiCad schematic for each board of each top-level file.
    pub(super) fn write_schematics(&self, say: &Say) -> BuildReport {
        // No lock is held while the files are written: it takes long enough to matter.
        let started = Instant::now();
        say("reading the project");
        let mut written = Vec::new();
        let mut output = String::new();
        let mut ok = true;
        // The schematic needs KiCad's symbol libraries, which the editor's own reading of
        // the project does not, so the files are built again here, as the command does.
        let built = (|| -> Result<_, String> {
            let loader = self.target().loader()?;
            let catalogs = self.target().catalogs()?;
            let libraries = netsmith_pipeline::Libraries::for_project(
                self.target().project.as_ref().map(|p| p.root.as_path()),
            )?;
            Ok((loader, catalogs, libraries))
        })();
        match built {
            Err(e) => {
                ok = false;
                output.push_str(&format!("{e}\n"));
            }
            Ok((loader, catalogs, libraries)) => {
                for source in &self.target().files {
                    let text = match fs::read_to_string(&source.path) {
                        Ok(t) => t,
                        Err(e) => {
                            ok = false;
                            output.push_str(&format!("{}: {e}\n", source.name));
                            continue;
                        }
                    };
                    match netsmith_pipeline::build_with(
                        &source.name,
                        &text,
                        &loader,
                        &catalogs,
                        &libraries,
                    ) {
                        Err(problems) => {
                            ok = false;
                            output.push_str(&format!(
                                "{}: not built, it has a mistake\n{problems}",
                                source.name
                            ));
                        }
                        Ok(boards) => {
                            for b in boards {
                                say(&format!("{}: writing the schematic", b.label()));
                                match b.write(&self.target().build_out) {
                                    Ok(dir) => {
                                        if let Err(e) = libraries.write_tables(&dir, &b.design) {
                                            ok = false;
                                            output.push_str(&format!("{}: {e}\n", dir.display()));
                                        }
                                        output.push_str(&b.report);
                                        written.push(dir.display().to_string());
                                    }
                                    Err(e) => {
                                        ok = false;
                                        output.push_str(&format!("{}: {e}\n", b.label()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if written.is_empty() && ok {
            ok = false;
            output.push_str("nothing built: see the problems\n");
        }
        self.fresh().pcb.clear();
        BuildReport {
            ok,
            written,
            output,
            elapsed_ms: millis(started.elapsed()),
        }
    }

    /// What `netsmith fab` does for the file that holds `board`.
    pub(super) fn make_fab(
        &self,
        board: &str,
        allow_drc_violations: bool,
        say: &Say,
    ) -> Result<FabReport, ApiError> {
        let started = Instant::now();
        let (file, name) = {
            let state = self.fresh();
            let data = state.snapshot.board(board)?;
            (data.file.clone(), data.built.label())
        };
        let target = self.target();
        let source = target
            .files
            .iter()
            .find(|f| f.name == file)
            .ok_or_else(|| ApiError::internal(format!("{file} is not a top-level file")))?;
        let report =
            |ok, drc, summary: String, notes: String, output: String, downloads| FabReport {
                board: name.clone(),
                ok,
                drc,
                summary,
                notes,
                output,
                downloads,
                elapsed_ms: millis(started.elapsed()),
            };
        let text = fs::read_to_string(&source.path)
            .map_err(|e| ApiError::internal(format!("{}: {e}", source.name)))?;
        let loader = target.loader().map_err(ApiError::internal)?;
        let libraries = netsmith_pipeline::Libraries::for_project(
            target.project.as_ref().map(|p| p.root.as_path()),
        )
        .map_err(ApiError::internal)?;
        // The stages `netsmith fab` prints to stderr go to the page instead.
        let _reporting = Reporting::to(say);
        netsmith_catalog::picks::refuse(&source.path).map_err(ApiError::internal)?;
        let made = fab_with_options(
            &source.name,
            &text,
            &loader,
            &target.fab_out,
            &target.catalogs().map_err(ApiError::internal)?,
            &libraries,
            None,
            &FabOptions {
                progress: Some(forward),
                allow_drc_violations,
                layout: Some(netsmith_fabricate::layout::layout_path(&source.path)),
                pin_map_source: Some(source.stable_name(target.project.as_ref())),
                place_seeds: netsmith_fabricate::layout::place_seeds(target.project.as_ref()),
                seed_writes: netsmith_fabricate::fab::SeedWrites::EditorRelease,
                ..FabOptions::default()
            },
        );
        match made {
            Err(problems) => {
                let drc = if problems.contains("design rules check") {
                    Drc::Failed
                } else {
                    Drc::NotRun
                };
                Ok(report(
                    false,
                    drc,
                    String::new(),
                    String::new(),
                    problems,
                    Vec::new(),
                ))
            }
            Ok(boards) => {
                let mut state = self.fresh();
                let mut mine = None;
                for b in boards {
                    if b.board == name {
                        mine = Some((b.summary.clone(), b.notes.clone()));
                    }
                    state.fab.insert(b.board, b.files);
                }
                let (summary, notes) = mine.unwrap_or_default();
                let downloads = state
                    .fab
                    .get(&name)
                    .map(|files| downloads_of(files))
                    .unwrap_or_default();
                // What the notes say the check did: it passed, or was waived.
                let drc = if notes.contains("design rules check: passed") {
                    Drc::Passed
                } else if notes.contains("design rules check: NOT clean") {
                    Drc::Failed
                } else {
                    Drc::NotRun
                };
                Ok(report(true, drc, summary, notes, String::new(), downloads))
            }
        }
    }

    /// A file `fab` wrote for `board`.
    pub(super) fn fetch(&self, board: &str, key: &str) -> Result<Attachment, ApiError> {
        let state = self.fresh();
        let files = state
            .fab
            .get(board)
            .ok_or_else(|| ApiError::not_found(format!("`fab` has not been run for {board}")))?;
        let (_, suffix) = DOWNLOADS
            .iter()
            .find(|(k, _)| *k == key)
            .ok_or_else(|| ApiError::bad_request(format!("there is no download `{key}`")))?;
        let path = files
            .iter()
            .find(|p| p.to_string_lossy().ends_with(suffix))
            .ok_or_else(|| ApiError::not_found(format!("`fab` wrote no {key} for {board}")))?;
        let bytes =
            fs::read(path).map_err(|e| ApiError::internal(format!("{}: {e}", path.display())))?;
        let content_type = match *suffix {
            "-gerbers.zip" => "application/zip",
            "bom.csv" | "cpl.csv" => "text/csv; charset=utf-8",
            _ => "text/plain; charset=utf-8",
        };
        Ok(Attachment {
            name: path
                .file_name()
                .map_or(key.to_string(), |n| n.to_string_lossy().into_owned()),
            content_type: content_type.to_string(),
            bytes,
        })
    }
}

fn millis(d: Duration) -> u32 {
    d.as_millis().min(u128::from(u32::MAX)) as u32
}

fn downloads_of(files: &[PathBuf]) -> Vec<Download> {
    DOWNLOADS
        .iter()
        .filter_map(|(key, suffix)| {
            files
                .iter()
                .find(|p| p.to_string_lossy().ends_with(suffix))
                .map(|p| Download {
                    key: (*key).to_string(),
                    name: p
                        .file_name()
                        .map_or(String::new(), |n| n.to_string_lossy().into_owned()),
                })
        })
        .collect()
}
