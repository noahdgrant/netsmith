//! The check of a catalog row's package against the footprint it names (issue #1658):
//! `netsmith catalog check` over the project's and the built-in catalogs, and the check
//! `catalog add` makes before it writes a row.
//!
//! A row names its package; the footprint is the `kicad.toml` row of the part's kind and that
//! package (the project's mapping, then the standard one). The kinds of a catalog are the
//! definitions whose `catalog` statement names it, in the standard library and the project's
//! sources; a catalog no definition names yet is checked against every footprint the mapping
//! gives its package. Each footprint is measured against the known size of the package
//! ([`netsmith_core::package_sizes`], [`netsmith_pcb::check_package`]); a package netsmith does
//! not know the size of is listed and not measured.

use std::collections::{BTreeMap, BTreeSet};

use netsmith_core::package_sizes;
use netsmith_kicad::Mapping;
use netsmith_lang::ast::Item;
use netsmith_pcb::{FootprintLibrary, PackageCheck, check_package};

use crate::project::Project;

/// What the check reads: which kinds of part use each catalog, and the libraries.
pub struct Context {
    kinds: BTreeMap<String, BTreeSet<String>>,
    mapping: Mapping,
    footprints: FootprintLibrary,
}

/// One footprint a row's package names, and what its measure found.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    /// The kind of part the `kicad.toml` row is for.
    pub kind: String,
    /// The footprint's id.
    pub footprint: String,
    /// The `kicad.toml` row: `std/kicad.toml line 12`.
    pub origin: String,
    /// What the measure found: empty when the footprint is the package.
    pub problems: Vec<String>,
    /// Why the footprint was not measured: it could not be read (a KiCad library that is not
    /// installed); the build names it too.
    pub unread: Option<String>,
}

/// What the check found of one package.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    /// The package is not one netsmith knows the size of.
    NotKnown,
    /// The mapping has no footprint for the package for any kind of the catalog.
    NoFootprint,
    /// The footprints measured.
    Measured(Vec<Measured>),
}

impl Found {
    /// The problems of every footprint.
    pub fn problems(&self) -> Vec<String> {
        match self {
            Found::Measured(all) => all.iter().flat_map(|m| m.problems.clone()).collect(),
            _ => Vec::new(),
        }
    }
}

/// The definitions whose `catalog` statement names each catalog, in `texts` (a parse error
/// leaves that file out: the checker says it).
pub fn catalog_kinds<'a>(
    texts: impl IntoIterator<Item = &'a str>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut kinds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for text in texts {
        let Ok(file) = netsmith_lang::parse(text) else {
            continue;
        };
        for d in &file.definitions {
            for item in &d.items {
                if let Item::Catalog(c) = item {
                    kinds
                        .entry(c.name.name.clone())
                        .or_default()
                        .insert(d.name.name.clone());
                }
            }
        }
    }
    kinds
}

impl Context {
    /// The project's (its sources, `kicad.toml` and libraries), or the standard library's with
    /// no project.
    pub fn of(project: Option<&Project>) -> Result<Context, String> {
        let (loader, libraries) = match project {
            Some(p) => (p.loader()?, p.libraries()?),
            None => (
                netsmith_check::Loader::std(),
                netsmith_pipeline::Libraries::for_project(None)?,
            ),
        };
        Ok(Context {
            kinds: catalog_kinds(loader.texts().map(|(_, text)| text)),
            mapping: libraries.mapping,
            footprints: libraries.footprints,
        })
    }

    /// A context of its parts, for tests.
    pub fn with(
        kinds: BTreeMap<String, BTreeSet<String>>,
        mapping: Mapping,
        footprints: FootprintLibrary,
    ) -> Context {
        Context {
            kinds,
            mapping,
            footprints,
        }
    }

    /// The footprints `package` names for the kinds of `catalog`: each (kind, footprint id,
    /// origin), each footprint once.
    fn footprints_of(&self, catalog: &str, package: &str) -> Vec<(String, String, String)> {
        let mut out: Vec<(String, String, String)> = Vec::new();
        let mut push = |kind: &str, footprint: &str, origin: String| {
            if !out.iter().any(|(_, f, _)| f == footprint) {
                out.push((kind.to_string(), footprint.to_string(), origin));
            }
        };
        match self.kinds.get(catalog) {
            Some(kinds) => {
                for kind in kinds {
                    if let Ok(mapped) = self.mapping.map(kind, Some(package)) {
                        push(kind, &mapped.footprint, mapped.origin);
                    }
                }
            }
            None => {
                for row in self.mapping.rows().iter().filter(|r| r.package == package) {
                    push(
                        &row.kind,
                        &row.footprint,
                        format!("{} line {}", row.source, row.line),
                    );
                }
            }
        }
        out
    }

    /// Measure the footprints a row of `catalog` with `package` names.
    pub fn check(&self, catalog: &str, package: &str) -> Found {
        if package_sizes::known(package).is_none() {
            return Found::NotKnown;
        }
        let footprints = self.footprints_of(catalog, package);
        if footprints.is_empty() {
            return Found::NoFootprint;
        }
        Found::Measured(
            footprints
                .into_iter()
                .map(|(kind, footprint, origin)| {
                    let (problems, unread) = match self.footprints.load(&footprint) {
                        Ok(fp) => match check_package(package, &footprint, &fp) {
                            PackageCheck::Disagrees(_, problems) => (
                                problems
                                    .into_iter()
                                    .map(|p| format!("{p} (the `{kind}` row of {origin})"))
                                    .collect(),
                                None,
                            ),
                            _ => (Vec::new(), None),
                        },
                        Err(e) => (Vec::new(), Some(e.to_string())),
                    };
                    Measured {
                        kind,
                        footprint,
                        origin,
                        problems,
                        unread,
                    }
                })
                .collect(),
        )
    }

    /// What `catalog add` finds before it writes a row of `catalog`: the measure of the
    /// footprints of `package`, and every description that names another package.
    pub fn check_record(
        &self,
        catalog: &str,
        package: Option<&str>,
        descriptions: &[&str],
    ) -> Vec<String> {
        let Some(package) = package else {
            return Vec::new();
        };
        let mut found = self.check(catalog, package).problems();
        for d in descriptions {
            if let Some(disagreement) = package_sizes::disagreement_in(package, d) {
                found.push(disagreement.to_string());
            }
        }
        found
    }
}

/// One row of a catalog file and what its package's check found.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The file as messages name it, and the row's line.
    pub file: String,
    pub line: usize,
    pub catalog: String,
    pub mpn: String,
    pub package: String,
    pub found: Found,
}

/// What `catalog check` found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    /// Every row with a package.
    pub rows: Vec<Row>,
}

impl Report {
    /// The rows whose footprint is not their package.
    pub fn mismatches(&self) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(|r| !r.found.problems().is_empty())
    }

    /// The packages of the rows that netsmith does not know the size of, each once.
    pub fn not_known(&self) -> BTreeSet<&str> {
        self.rows
            .iter()
            .filter(|r| r.found == Found::NotKnown)
            .map(|r| r.package.as_str())
            .collect()
    }

    /// The packages of the rows whose kinds have no footprint of them, each once.
    pub fn no_footprint(&self) -> BTreeSet<(&str, &str)> {
        self.rows
            .iter()
            .filter(|r| r.found == Found::NoFootprint)
            .map(|r| (r.catalog.as_str(), r.package.as_str()))
            .collect()
    }

    /// The footprints that could not be read, each once, with why.
    pub fn unread(&self) -> BTreeSet<(&str, &str)> {
        self.rows
            .iter()
            .filter_map(|r| match &r.found {
                Found::Measured(all) => Some(all),
                _ => None,
            })
            .flatten()
            .filter_map(|m| m.unread.as_deref().map(|why| (m.footprint.as_str(), why)))
            .collect()
    }

    pub fn measured(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r.found, Found::Measured(_)))
            .count()
    }
}

/// Check every row of the catalog files `files` (each the catalog's name, the file as messages
/// name it and its text). A row with no `package` cell, or an empty one, is left out.
pub fn check_files(
    context: &Context,
    files: &[(String, String, String)],
) -> Result<Report, String> {
    let mut report = Report::default();
    for (catalog, shown, text) in files {
        let records = netsmith_solve::parse_csv(text).map_err(|e| format!("{shown}: {e}"))?;
        let Some(((_, header), body)) = records.split_first() else {
            continue;
        };
        let column = |name: &str| header.iter().position(|h| h.trim() == name);
        let (Some(package_at), mpn_at) = (column("package"), column("mpn")) else {
            continue;
        };
        for (line, cells) in body {
            let package = cells.get(package_at).map(|p| p.trim()).unwrap_or("");
            if package.is_empty() {
                continue;
            }
            report.rows.push(Row {
                file: shown.clone(),
                line: *line,
                catalog: catalog.clone(),
                mpn: mpn_at
                    .and_then(|m| cells.get(m))
                    .map(|m| m.trim().to_string())
                    .unwrap_or_default(),
                package: package.to_string(),
                found: context.check(catalog, package),
            });
        }
    }
    Ok(report)
}

/// The names of the packages netsmith knows the size of, for a message.
pub fn known_names() -> Vec<&'static str> {
    package_sizes::KNOWN.iter().map(|k| k.name).collect()
}

/// What `catalog check` prints.
pub fn render(report: &Report) -> String {
    let mut out = String::new();
    for row in report.mismatches() {
        for problem in row.found.problems() {
            out.push_str(&format!(
                "{}:{}: {} ({}): {problem}\n",
                row.file, row.line, row.mpn, row.catalog
            ));
        }
    }
    let mismatches = report.mismatches().count();
    out.push_str(&format!(
        "{} rows with a package: {} measured against their footprint, {} that disagree\n",
        report.rows.len(),
        report.measured(),
        mismatches
    ));
    let not_known = report.not_known();
    if !not_known.is_empty() {
        out.push_str(&format!(
            "not measured, netsmith knows no size of the package: {}\n",
            not_known.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    let no_footprint = report.no_footprint();
    if !no_footprint.is_empty() {
        out.push_str(&format!(
            "not measured, no kicad.toml row gives the package a footprint for the catalog's kinds: {}\n",
            no_footprint
                .into_iter()
                .map(|(c, p)| format!("{p} ({c})"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for (footprint, why) in report.unread() {
        out.push_str(&format!(
            "not measured, the footprint `{footprint}` could not be read: {why}\n"
        ));
    }
    out.push_str(&format!(
        "the packages netsmith knows the size of: {}\n",
        known_names().join(", ")
    ));
    out
}

/// What `catalog check --format json` prints.
pub fn json(report: &Report) -> serde_json::Value {
    use serde_json::json;
    json!({
        "command": "catalog check",
        "ok": report.mismatches().next().is_none(),
        "rows": report.rows.len(),
        "measured": report.measured(),
        "mismatches": report.mismatches().map(|r| json!({
            "file": r.file,
            "line": r.line,
            "catalog": r.catalog,
            "mpn": r.mpn,
            "package": r.package,
            "problems": r.found.problems(),
        })).collect::<Vec<_>>(),
        "not_known": report.not_known(),
        "no_footprint": report.no_footprint().into_iter().map(|(c, p)| json!({"catalog": c, "package": p})).collect::<Vec<_>>(),
        "unread": report.unread().into_iter().map(|(f, why)| json!({"footprint": f, "why": why})).collect::<Vec<_>>(),
        "known": known_names(),
    })
}
