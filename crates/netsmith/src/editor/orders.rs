//! The build panel's orders: listing the manifest's `[order.*]` tables and running one
//! (ADR 0035), which is `netsmith fab --order`.

use std::{fs, time::Instant};

use netsmith_editor::{
    ApiError, Attachment, Say,
    api::{Download, OrderBoardInfo, OrderInfo, OrderReport},
};

use super::{
    Workspace,
    progress::{Reporting, forward},
};
use crate::fab::FabOptions;
use crate::order::{BOM_FILE, SUMMARY_FILE, fab_order};

/// The files of an order the page offers, by the name it asks for them with.
const DOWNLOADS: [(&str, &str); 2] = [("summary", SUMMARY_FILE), ("bom", BOM_FILE)];

impl Workspace {
    /// The orders of the manifest, as it was read when the editor started.
    pub(super) fn list_orders(&self) -> Vec<OrderInfo> {
        let Some(project) = &self.target().project else {
            return Vec::new();
        };
        project
            .manifest
            .orders
            .iter()
            .map(|o| OrderInfo {
                name: o.name.clone(),
                boards: o
                    .boards
                    .iter()
                    .map(|b| OrderBoardInfo {
                        name: b.name.clone(),
                        quantity: b.quantity,
                    })
                    .collect(),
            })
            .collect()
    }

    /// What `netsmith fab --order <name>` does.
    pub(super) fn make_order(
        &self,
        name: &str,
        allow_drc_violations: bool,
        say: &Say,
    ) -> Result<OrderReport, ApiError> {
        let started = Instant::now();
        let target = self.target();
        let order = target
            .project
            .as_ref()
            .and_then(|p| p.manifest.orders.iter().find(|o| o.name == name))
            .ok_or_else(|| ApiError::not_found(format!("netsmith.toml has no order `{name}`")))?;
        let report = |ok, dir: String, summary: String, output: String, downloads| OrderReport {
            name: name.to_string(),
            ok,
            dir,
            summary,
            output,
            downloads,
            elapsed_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
        };
        let loader = target.loader().map_err(ApiError::internal)?;
        let catalogs = target.catalogs().map_err(ApiError::internal)?;
        let libraries =
            crate::Libraries::for_project(target.project.as_ref().map(|p| p.root.as_path()))
                .map_err(ApiError::internal)?;
        // The stages `netsmith fab` prints to stderr go to the page instead.
        let _reporting = Reporting::to(say);
        let made = fab_order(
            order,
            &target,
            &loader,
            &catalogs,
            &libraries,
            None,
            &FabOptions {
                progress: Some(forward),
                allow_drc_violations,
                ..FabOptions::default()
            },
            &target.fab_out,
        );
        Ok(match made {
            Err(problems) => report(false, String::new(), String::new(), problems, Vec::new()),
            Ok(made) => {
                self.fresh()
                    .orders
                    .insert(name.to_string(), made.dir.clone());
                let downloads = DOWNLOADS
                    .iter()
                    .map(|(key, file)| Download {
                        key: (*key).to_string(),
                        name: (*file).to_string(),
                    })
                    .collect();
                report(
                    true,
                    made.dir.display().to_string(),
                    made.summary,
                    String::new(),
                    downloads,
                )
            }
        })
    }

    /// A file an order wrote.
    pub(super) fn fetch_order(&self, name: &str, key: &str) -> Result<Attachment, ApiError> {
        let dir = self
            .fresh()
            .orders
            .get(name)
            .cloned()
            .ok_or_else(|| ApiError::not_found(format!("order `{name}` has not been run")))?;
        let (_, file) = DOWNLOADS
            .iter()
            .find(|(k, _)| *k == key)
            .ok_or_else(|| ApiError::bad_request(format!("there is no download `{key}`")))?;
        let path = dir.join(file);
        let bytes =
            fs::read(&path).map_err(|e| ApiError::internal(format!("{}: {e}", path.display())))?;
        Ok(Attachment {
            name: (*file).to_string(),
            content_type: if *file == BOM_FILE {
                "text/csv; charset=utf-8"
            } else {
                "text/plain; charset=utf-8"
            }
            .to_string(),
            bytes,
        })
    }
}
