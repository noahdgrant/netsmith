//! A self-contained KiCad project (issue #1198): the folder of a board, with everything its
//! KiCad files refer to inside it, so that it opens in KiCad anywhere.
//!
//! Without it, `build` and `fab` write the schematic and the board and, beside them, the library
//! tables of the project's own libraries (paths relative to the folder, but pointing wherever the
//! libraries are, `../../libs/x.pretty`) and netsmith's own library. The libraries KiCad
//! installs are left to KiCad's own tables, and each footprint's `(model ...)` names a variable
//! of KiCad's (`${KICAD9_3DMODEL_DIR}/...`). Move the folder to a machine without the same
//! libraries and the tables and the models break.
//!
//! A [`PortableProject`] writes the folder the other way, as `--portable` asks:
//!
//! - `libs/<Library>.kicad_sym` holds the symbols of that library the board uses, and
//!   `libs/<Library>.pretty/` the footprints, copied from the project's own libraries and from
//!   KiCad's, and read as the build read them, so a footprint a generator makes (ADR 0043) is
//!   written too. netsmith's own library (ADR 0087) stays where it always is, `netsmith.kicad_sym`
//!   and `netsmith.pretty/`;
//! - `models/` holds the 3D models: `models/<Library>.3dshapes/<file>` for those a footprint
//!   names in KiCad's model folder, as KiCad lays them out, and `models/project/<file>` for those
//!   a project states (a part's `.model`, ADR 0108, or the project's model table), which the
//!   board file gives to each part that has one in place of the library's model;
//! - `sym-lib-table` and `fp-lib-table` list every library with a `${KIPRJMOD}/...` path inside
//!   the folder, and every `(model ...)` of a footprint written here is `${KIPRJMOD}/models/...`
//!   (`${KIPRJMOD}` is the project's folder);
//! - `LICENSES.md` carries the notice that KiCad's libraries ask for, when any of their entries
//!   or models were copied.
//!
//! A model that cannot be found (the KiCad model folder is not installed, a project's file is
//! gone) is a warning and the footprint is written without that `(model ...)`: no path outside
//! the folder stays. KiCad's own folder of models is found as [`kicad_models`] says; the
//! libraries come from the search of ADR 0021 as for any build, and a design with only netsmith's
//! own and generated footprints and symbols needs no KiCad at all.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use netsmith_core::{Design, StepModel};
use netsmith_kicad::{Sexp, Symbol, is_library_id};
use netsmith_pcb::PATH_PROPERTY;

use crate::libraries::Libraries;

/// The folder of the copied symbol and footprint libraries, in the project's folder.
pub const LIBS_FOLDER: &str = "libs";

/// The folder of the copied 3D models, in the project's folder.
pub const MODELS_FOLDER: &str = "models";

/// The folder, under [`MODELS_FOLDER`], of the models a project states.
const PROJECT_MODELS: &str = "project";

/// The notice, in the project's folder, of the licence of what was copied from KiCad's libraries.
pub const LICENSES_FILE: &str = "LICENSES.md";

/// Where KiCad's own 3D models are: the folder a `${KICAD9_3DMODEL_DIR}` stands for. The variable
/// itself when it is set (KiCad's own, `KICAD9_3DMODEL_DIR`, then 8's and the unversioned one),
/// else the `3dmodels` folder beside the footprint libraries (`share/kicad/footprints` and
/// `share/kicad/3dmodels` of an install). `None` when neither is a folder.
pub fn kicad_models(footprints_root: &Path) -> Option<PathBuf> {
    [
        "KICAD9_3DMODEL_DIR",
        "KICAD8_3DMODEL_DIR",
        "KICAD_3DMODEL_DIR",
    ]
    .iter()
    .filter_map(std::env::var_os)
    .map(PathBuf::from)
    .chain(footprints_root.parent().map(|p| p.join("3dmodels")))
    .find(|p| p.is_dir())
}

/// What a [`PortableProject`] did that a person should hear of.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Portable {
    /// A model or entry that could not be copied, and what was done about it.
    pub warnings: Vec<String>,
    /// The files written, as paths from the project's folder, `/`-separated and sorted.
    pub files: Vec<String>,
}

/// The writer of one board's self-contained folder: [`Libraries::portable`].
pub struct PortableProject<'a> {
    libraries: &'a Libraries,
    dir: PathBuf,
    kicad_models: Option<PathBuf>,
    /// The symbol libraries written (their names), and the footprint libraries.
    symbol_libs: BTreeSet<String>,
    footprint_libs: BTreeSet<String>,
    /// The files written, relative to `dir`.
    files: BTreeSet<String>,
    /// For each file under `models/`, the file it is a copy of.
    models: BTreeMap<String, PathBuf>,
    /// What came from KiCad's libraries, for the notice: `Library:Name` of each symbol and
    /// footprint, and the path under `models/` of each model.
    kicad_symbols: BTreeSet<String>,
    kicad_footprints: BTreeSet<String>,
    kicad_model_files: BTreeSet<String>,
    warnings: Vec<String>,
    with_std: bool,
}

impl Libraries {
    /// The writer of a self-contained KiCad folder at `dir` ([`crate::portable`]), reading KiCad's
    /// models from where [`kicad_models`] looks.
    pub fn portable(&self, dir: &Path) -> PortableProject<'_> {
        self.portable_with(dir, kicad_models(self.footprints.root()))
    }

    /// [`Libraries::portable`] with KiCad's model folder given (`None`: there is none), for a
    /// test that must not read the machine.
    pub fn portable_with(&self, dir: &Path, kicad_models: Option<PathBuf>) -> PortableProject<'_> {
        PortableProject {
            libraries: self,
            dir: dir.to_path_buf(),
            kicad_models,
            symbol_libs: BTreeSet::new(),
            footprint_libs: BTreeSet::new(),
            files: BTreeSet::new(),
            models: BTreeMap::new(),
            kicad_symbols: BTreeSet::new(),
            kicad_footprints: BTreeSet::new(),
            kicad_model_files: BTreeSet::new(),
            warnings: Vec::new(),
            with_std: false,
        }
    }
}

/// `path` with its `..` and `.` taken out, `None` when it climbs out of its start or is not
/// relative.
fn clean(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(p) => out.push(p),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(out)
}

fn slashes(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// The `${NAME}` a path starts with and the rest of it.
fn variable(stated: &str) -> Option<(&str, &str)> {
    let (name, rest) = stated.strip_prefix("${")?.split_once('}')?;
    Some((name, rest.trim_start_matches(['/', '\\'])))
}

/// Whether the variable stands for KiCad's folder of 3D models.
fn is_models_variable(name: &str) -> bool {
    name == "KISYS3DMOD" || (name.starts_with("KICAD") && name.ends_with("3DMODEL_DIR"))
}

impl PortableProject<'_> {
    fn write(&mut self, relative: &str, text: &str) -> Result<(), String> {
        let file = self.dir.join(relative);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
        self.files.insert(relative.to_string());
        Ok(())
    }

    /// Where the file a model line names is, and where it goes under `models/`: the part under
    /// KiCad's model folder keeps its place there; any other file goes to `models/project/`.
    /// `Err` says why it cannot be found. `bases` are what `${KIPRJMOD}` and a relative path
    /// start from, the folders of the project the footprint came from, tried in order.
    fn find_model(
        &self,
        stated: &str,
        bases: &[PathBuf],
    ) -> Result<(PathBuf, String, bool), String> {
        let named = |source: PathBuf, dest: String, kicad: bool| {
            // The model a footprint of KiCad's names is often in two forms (`.wrl` and `.step`).
            let alternatives = ["step", "wrl", "stp"].map(|e| source.with_extension(e));
            std::iter::once(source.clone())
                .chain(alternatives)
                .find(|p| p.is_file())
                .map(|p| {
                    let dest = Path::new(&dest).with_file_name(p.file_name().unwrap_or_default());
                    (p, slashes(&dest), kicad)
                })
                .ok_or_else(|| format!("`{}` is not there", source.display()))
        };
        let project_dest = |source: &Path| {
            format!(
                "{PROJECT_MODELS}/{}",
                source
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            )
        };
        // The first folder of the project the file is in, or the first folder when none has it.
        let in_bases = |rest: &str| {
            bases
                .iter()
                .map(|b| b.join(rest))
                .find(|p| p.is_file())
                .or_else(|| bases.first().map(|b| b.join(rest)))
        };
        match variable(stated) {
            Some((name, rest)) if is_models_variable(name) => {
                let folder = self
                    .kicad_models
                    .as_ref()
                    .ok_or("KiCad's model folder (`3dmodels`, or `KICAD9_3DMODEL_DIR`) is not installed here")?;
                let rest =
                    clean(Path::new(rest)).ok_or("its path climbs out of KiCad's model folder")?;
                named(folder.join(&rest), slashes(&rest), true)
            }
            Some(("KIPRJMOD", rest)) => {
                let source =
                    in_bases(rest).ok_or("`${KIPRJMOD}` of a library is not known here")?;
                named(source.clone(), project_dest(&source), false)
            }
            Some((name, rest)) => {
                let value = std::env::var_os(name).ok_or(format!("`${{{name}}}` is not set"))?;
                let source = PathBuf::from(value).join(rest);
                named(source.clone(), project_dest(&source), false)
            }
            None => {
                let path = Path::new(stated);
                let source = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    in_bases(stated).unwrap_or_else(|| path.to_path_buf())
                };
                named(source.clone(), project_dest(&source), false)
            }
        }
    }

    /// Copy `source` to `models/<dest>` and say which line names it: a different file already
    /// there under that name gets a number in its name. Returns the path under `models/`.
    fn copy_model(&mut self, source: &Path, dest: &str) -> Result<String, String> {
        let mut dest = dest.to_string();
        let bytes = std::fs::read(source).map_err(|e| format!("{}: {e}", source.display()))?;
        let mut n = 1;
        loop {
            match self.models.get(&dest) {
                None => break,
                Some(known) if std::fs::read(known).is_ok_and(|b| b == bytes) => {
                    return Ok(dest);
                }
                Some(_) => {
                    let path = Path::new(source.file_name().unwrap_or_default());
                    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                    let ext = path
                        .extension()
                        .map(|e| format!(".{}", e.to_string_lossy()))
                        .unwrap_or_default();
                    n += 1;
                    let parent = Path::new(&dest).parent().map(slashes).unwrap_or_default();
                    dest = format!("{parent}/{stem}-{n}{ext}");
                }
            }
        }
        let file = self.dir.join(MODELS_FOLDER).join(&dest);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&file, &bytes).map_err(|e| format!("{}: {e}", file.display()))?;
        self.files.insert(format!("{MODELS_FOLDER}/{dest}"));
        self.models.insert(dest.clone(), source.to_path_buf());
        Ok(dest)
    }

    /// `model` (a `(model "path" ...)` of a footprint called `whose`) with its path inside the
    /// folder, after the file is copied there; `None`, with a warning, when it cannot be.
    fn portable_model(&mut self, model: &Sexp, whose: &str, bases: &[PathBuf]) -> Option<Sexp> {
        let stated = model.args().first().and_then(Sexp::text)?.to_string();
        let copied = self
            .find_model(&stated, bases)
            .and_then(|(source, dest, kicad)| {
                self.copy_model(&source, &dest).map(|dest| (dest, kicad))
            });
        match copied {
            Ok((dest, kicad)) => {
                if kicad {
                    self.kicad_model_files.insert(dest.clone());
                }
                let mut items = model.items().to_vec();
                items[1] = Sexp::string(format!("${{KIPRJMOD}}/{MODELS_FOLDER}/{dest}"));
                Some(Sexp::List(items))
            }
            Err(why) => {
                self.warnings.push(format!(
                    "{whose}: the 3D model `{stated}` is not in the project folder ({why}), so the footprint has no 3D model there"
                ));
                None
            }
        }
    }

    /// `footprint` (the tree of a `.kicad_mod` or a footprint of a `.kicad_pcb`) with each of its
    /// models made portable. `bases` are the folders `${KIPRJMOD}` of a library's file may mean.
    fn portable_footprint(&mut self, footprint: &Sexp, whose: &str, bases: &[PathBuf]) -> Sexp {
        let items: Vec<Sexp> = footprint
            .items()
            .iter()
            .filter_map(|child| match child.head() {
                Some("model") => self.portable_model(child, whose, bases),
                _ => Some(child.clone()),
            })
            .collect();
        Sexp::List(items)
    }

    /// Write the symbols and footprints the board's parts use into `libs/`, the library tables
    /// and, when a part uses netsmith's own library, its files; copy the models the footprints
    /// name. Call it before [`PortableProject::finish`].
    pub fn libraries(&mut self, design: &Design) -> Result<(), String> {
        let libs = self.libraries;
        self.with_std = libs.uses_std_library(design);
        if self.with_std {
            for (path, text) in netsmith_kicad::stdlib_files(&libs.generated_symbols(design)) {
                self.write(&path, &text)?;
            }
        }
        let project: BTreeSet<&str> = libs
            .mapping
            .libraries()
            .iter()
            .map(|l| l.name.as_str())
            .collect();

        // The symbols as the schematic embeds them, so that the library holds what the sheet has.
        let mut symbols: BTreeMap<String, Vec<std::sync::Arc<Symbol>>> = BTreeMap::new();
        for drawn in netsmith_kicad::part_symbols(design, &libs.mapping, &libs.symbols) {
            let Ok((_, symbol)) = drawn else { continue };
            let id = symbol.lib_id();
            let Some((library, _)) = id.split_once(':') else {
                continue;
            };
            if is_library_id(id) {
                continue;
            }
            let held = symbols.entry(library.to_string()).or_default();
            if !held.iter().any(|s| s.lib_id() == id) {
                held.push(symbol.clone());
            }
            if !project.contains(library) {
                self.kicad_symbols.insert(id.to_string());
            }
        }
        for (library, mut held) in symbols {
            held.sort_by(|a, b| a.lib_id().cmp(b.lib_id()));
            let mut items = vec![
                Sexp::atom("kicad_symbol_lib"),
                Sexp::node("version", vec![Sexp::atom("20241209")]),
                Sexp::node("generator", vec![Sexp::string("netsmith")]),
                Sexp::node("generator_version", vec![Sexp::string("0")]),
            ];
            items.extend(held.iter().map(|s| s.library_definition()));
            let text = Sexp::List(items).to_kicad_string();
            self.write(&format!("{LIBS_FOLDER}/{library}.kicad_sym"), &text)?;
            self.symbol_libs.insert(library);
        }

        // The footprints, each once, as the build reads them.
        let mut ids: BTreeSet<String> = BTreeSet::new();
        for part in &design.parts {
            if netsmith_core::is_artwork(&part.kind) {
                continue;
            }
            if let Ok(mapped) =
                libs.mapping
                    .map_part(&part.path, &part.kind, part.package.as_deref())
            {
                ids.insert(mapped.footprint);
            }
        }
        for id in ids {
            let Some((library, name)) = id.split_once(':') else {
                continue;
            };
            // netsmith's own library is written whole above; a `gen:` footprint has no library.
            if is_library_id(&id) || netsmith_kicad::FootprintSpec::from_id(&id).is_some() {
                continue;
            }
            let footprint = libs
                .footprints
                .load(&id)
                .map_err(|e| format!("footprint `{id}`: {e}"))?;
            // `${KIPRJMOD}` of a library's file is the project that library belongs to: the
            // folder that holds its `.pretty` folder, or the one above it.
            let bases: Vec<PathBuf> = libs
                .footprints
                .path_of(&id)
                .ok()
                .and_then(|p| p.parent().and_then(Path::parent).map(Path::to_path_buf))
                .map(|folder| {
                    let mut bases = vec![folder.clone()];
                    bases.extend(folder.parent().map(Path::to_path_buf));
                    bases
                })
                .unwrap_or_default();
            let written = self.portable_footprint(&footprint.source, &id, &bases);
            let text = written.to_kicad_string();
            self.write(
                &format!("{LIBS_FOLDER}/{library}.pretty/{name}.kicad_mod"),
                &text,
            )?;
            self.footprint_libs.insert(library.to_string());
            // A footprint a generator makes is netsmith's own (MIT), though its model is KiCad's.
            if footprint.package.is_none() && !project.contains(library) {
                self.kicad_footprints.insert(id.clone());
            }
        }
        Ok(())
    }

    /// `pcb`, the text of a `.kicad_pcb`, with the 3D model of each footprint inside the folder:
    /// each library model copied in, and, for a part with a STEP model of its own (`models` of
    /// the design, found on disk), that model in place of the library's, with the offset and
    /// rotation the design states (ADR 0108: the numbers of a KiCad model line). A footprint of
    /// the file names its part by the hidden property [`PATH_PROPERTY`].
    pub fn pcb(&mut self, pcb: &str, models: &[StepModel]) -> Result<String, String> {
        let mut tree = Sexp::parse(pcb).map_err(|e| format!("the board file: {e}"))?;
        let Sexp::List(items) = &mut tree else {
            return Err("the board file is not a KiCad board".to_string());
        };
        for item in items.iter_mut() {
            if item.head() != Some("footprint") {
                continue;
            }
            let whose = item
                .args()
                .first()
                .and_then(Sexp::text)
                .unwrap_or("footprint")
                .to_string();
            let path = item
                .find_all("property")
                .find(|p| p.args().first().and_then(Sexp::text) == Some(PATH_PROPERTY))
                .and_then(|p| p.args().get(1))
                .and_then(Sexp::text)
                .map(str::to_string);
            let own = path.as_deref().and_then(|path| {
                models
                    .iter()
                    .find(|m| m.path == path && m.resolved.is_some())
            });
            let mut written = match own {
                Some(own) => self.own_model(item, own, &whose),
                None => self.portable_footprint(item, &whose, &[]),
            };
            std::mem::swap(item, &mut written);
        }
        Ok(tree.to_kicad_string())
    }

    /// `footprint` with the one model of `own`, a part's STEP model, and the library's models
    /// left out. The file is copied; one that cannot be leaves the library's models, made
    /// portable.
    fn own_model(&mut self, footprint: &Sexp, own: &StepModel, whose: &str) -> Sexp {
        let source = own.resolved.as_deref().unwrap_or(Path::new(""));
        let dest = format!(
            "{PROJECT_MODELS}/{}",
            source
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        );
        match self.copy_model(source, &dest) {
            Ok(dest) => {
                let xyz = |v: [f64; 3]| {
                    Sexp::node(
                        "xyz",
                        v.iter().map(|n| Sexp::num(*n)).collect::<Vec<Sexp>>(),
                    )
                };
                let model = Sexp::node(
                    "model",
                    vec![
                        Sexp::string(format!("${{KIPRJMOD}}/{MODELS_FOLDER}/{dest}")),
                        Sexp::node("offset", vec![xyz(own.offset)]),
                        Sexp::node("scale", vec![xyz([1.0, 1.0, 1.0])]),
                        Sexp::node("rotate", vec![xyz(own.rotation)]),
                    ],
                );
                let mut items: Vec<Sexp> = footprint
                    .items()
                    .iter()
                    .filter(|c| c.head() != Some("model"))
                    .cloned()
                    .collect();
                items.push(model);
                Sexp::List(items)
            }
            Err(why) => {
                self.warnings.push(format!(
                    "{whose}: the part's STEP model could not be copied ({why}), so the footprint keeps the library's 3D model"
                ));
                self.portable_footprint(footprint, whose, &[])
            }
        }
    }

    /// Write the library tables, the notice and, when nothing else was, nothing more: the folder
    /// is complete. Returns the files written and what could not be copied.
    pub fn finish(mut self) -> Result<Portable, String> {
        let table = |head: &str, libraries: &[(&str, String)]| {
            let mut out = format!("({head}\n\t(version 7)\n");
            for (name, uri) in libraries {
                out.push_str(&format!(
                    "\t(lib (name {name:?})(type \"KiCad\")(uri \"${{KIPRJMOD}}/{uri}\")(options \"\")(descr \"\"))\n"
                ));
            }
            out.push_str(")\n");
            out
        };
        let mut symbols: Vec<(&str, String)> = Vec::new();
        let mut footprints: Vec<(&str, String)> = Vec::new();
        if self.with_std {
            symbols.push((
                netsmith_kicad::STDLIB_NAME,
                format!("{}.kicad_sym", netsmith_kicad::STDLIB_NAME),
            ));
            footprints.push((
                netsmith_kicad::STDLIB_NAME,
                netsmith_kicad::FOOTPRINT_FOLDER.to_string(),
            ));
        }
        symbols.extend(
            self.symbol_libs
                .iter()
                .map(|l| (l.as_str(), format!("{LIBS_FOLDER}/{l}.kicad_sym"))),
        );
        footprints.extend(
            self.footprint_libs
                .iter()
                .map(|l| (l.as_str(), format!("{LIBS_FOLDER}/{l}.pretty"))),
        );
        let mut tables = Vec::new();
        if !symbols.is_empty() {
            tables.push(("sym-lib-table", table("sym_lib_table", &symbols)));
        }
        if !footprints.is_empty() {
            tables.push(("fp-lib-table", table("fp_lib_table", &footprints)));
        }
        for (name, text) in tables {
            self.write(name, &text)?;
        }
        if !self.kicad_symbols.is_empty()
            || !self.kicad_footprints.is_empty()
            || !self.kicad_model_files.is_empty()
        {
            let notice = self.notice();
            self.write(LICENSES_FILE, &notice)?;
        }
        Ok(Portable {
            warnings: self.warnings,
            files: self.files.into_iter().collect(),
        })
    }

    /// The text of [`LICENSES_FILE`].
    fn notice(&self) -> String {
        let mut out = String::from(KICAD_NOTICE);
        let list = |out: &mut String, head: &str, items: Vec<String>| {
            if !items.is_empty() {
                out.push_str(&format!("\n{head}\n\n"));
                for item in items {
                    out.push_str(&format!("- `{item}`\n"));
                }
            }
        };
        list(
            &mut out,
            &format!("Symbols, in `{LIBS_FOLDER}/<Library>.kicad_sym`:"),
            self.kicad_symbols.iter().cloned().collect(),
        );
        list(
            &mut out,
            &format!("Footprints, in `{LIBS_FOLDER}/<Library>.pretty/`:"),
            self.kicad_footprints.iter().cloned().collect(),
        );
        list(
            &mut out,
            &format!("3D models, in `{MODELS_FOLDER}/`, unchanged:"),
            self.kicad_model_files.iter().cloned().collect(),
        );
        out
    }
}

/// What KiCad's libraries ask of one who passes copies on (their licence page, as of KiCad 9):
/// CC-BY-SA 4.0 for the libraries, with an exception for the designs made with them.
const KICAD_NOTICE: &str = "\
# Licence of the KiCad library entries in this folder

This folder holds copies of entries of KiCad's official libraries (symbols, footprints and
3D models), listed below. They are © the KiCad Library Team and its contributors, and are
licensed under the Creative Commons Attribution-ShareAlike 4.0 International licence
(CC-BY-SA 4.0, <https://creativecommons.org/licenses/by-sa/4.0/legalcode>) with the exception
that KiCad adds, quoted here:

> To the extent that the creation of electronic designs that use 'Licensed Material' can be
> considered to be 'Adapted Material', then the copyright holder waives article 3 of the
> license with respect to these designs and any generated files which use data provided as
> part of the 'Licensed Material'.

(<https://www.kicad.org/libraries/license/>.)

What that means for this folder:

- The design itself (the schematic, the board and what is generated from them, such as Gerber
  files) is not bound by CC-BY-SA: the exception waives the licence's article 3 for it.
- The copied library files are Licensed Material. Anyone who passes them on, with this folder
  or alone, must keep this notice and the credit to the KiCad Library Team, say that they were
  changed (see below), and pass them on under CC-BY-SA 4.0 or a later compatible licence.
- The footprints and symbols were rewritten by netsmith: the files are read and written again, so
  the spacing differs, and each `(model ...)` line names its model inside this folder
  (`${KIPRJMOD}/models/...`) instead of a variable of KiCad's. The 3D models are copied byte
  for byte.

netsmith's own library (`netsmith.kicad_sym`, `netsmith.pretty/`), the symbols netsmith draws and
the footprints its generators make are not KiCad's and are in no list below (netsmith is MIT
licensed).
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_variable_and_its_rest_are_told_apart() {
        assert_eq!(
            variable("${KICAD9_3DMODEL_DIR}/R.3dshapes/R.wrl"),
            Some(("KICAD9_3DMODEL_DIR", "R.3dshapes/R.wrl"))
        );
        assert_eq!(variable("models/a.step"), None);
        assert!(is_models_variable("KICAD9_3DMODEL_DIR"));
        assert!(is_models_variable("KISYS3DMOD"));
        assert!(!is_models_variable("KIPRJMOD"));
    }

    #[test]
    fn a_path_that_climbs_out_is_refused() {
        assert_eq!(clean(Path::new("a/../b")), Some(PathBuf::from("b")));
        assert_eq!(clean(Path::new("../b")), None);
        assert_eq!(clean(Path::new("/b")), None);
    }
}
