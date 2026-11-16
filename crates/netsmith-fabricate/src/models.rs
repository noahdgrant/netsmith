//! The STEP models of a design's parts on disk (`.model`, ADR 0108): each `.model.file` is
//! found relative to the file that states it, and must stay inside the project. A path
//! outside the project is an error, and so are two statements that name two files. A file
//! that is not there is a warning, and the part is drawn with its generated shape: a maker's
//! license often keeps its model out of a public repository, so a clone or a CI job may not
//! have it.
//!
//! A case model's file (`CaseModel` in the physical file's `case { }` section, ADR 0109) is
//! found the same way. One that is not there leaves the board's fit in its case unchecked: a
//! warning, and an error when constraints are strict.
//!
//! The project's model table (`[models]` and `[[model]]` in `netsmith.toml`, issue #763) gives a
//! model to each part that states none, by the id of the footprint the part is mapped to: a
//! `[[model]]` row of that id, else `<folder>/<Lib>.3dshapes/<Name>.step` in the models folder,
//! KiCad's own layout. It is a default: a part's own `.model.file` wins, and an empty one says
//! the part has none. A row's file that is not there is a warning; the folder is only looked in.
//!
//! [`locate`] settles a part model's place; [`uses`] reads it ([`read`], once per change of its
//! bytes) for the scene to draw, and [`crate::case_fit`] reads a case model the same way. The
//! model never gives the part a height, and a case model never gives the board an outline, a
//! hole or a height limit.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use netsmith_3d::{ModelUse, StepModel};
use netsmith_core::Design;
use netsmith_kicad::Mapping;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::physical::physical_path;
use crate::project::Target;

/// What [`locate`] found wrong.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Located {
    /// A model file that is not there: the part is drawn with its generated shape.
    pub warnings: Vec<String>,
    /// A path outside the project, an absolute path, or two statements that name two files.
    pub errors: Vec<String>,
    /// A case model's file that is not there (ADR 0109): the board's fit in its case is not
    /// checked. A warning, and an error when constraints are strict (`fab --constraints
    /// error` or `constraints = "error"`), as a part of unknown height under a limit is.
    pub unchecked: Vec<String>,
    /// What the project's model table did, for a project that has one (issue #763): how many
    /// parts it gave a model, and which parts are drawn with no model at all. Printed as a
    /// `note:`.
    pub notes: Vec<String>,
}

/// The `[models]` table of `netsmith.toml` (issue #763).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelsConfig {
    /// The folder a footprint `Lib:Name` finds its model in, as
    /// `<folder>/<Lib>.3dshapes/<Name>.step` (or `.stp`, `.STEP`): KiCad's layout of its
    /// `kicad-packages3D`. Relative to the project root, or absolute, and it may start with an
    /// environment variable, `${KICAD9_3DMODEL_DIR}`. It may be outside the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}

/// A `[[model]]` row of `netsmith.toml` (issue #763): the STEP model of every part whose
/// footprint is `footprint`, unless the part states its own `.model`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRow {
    /// The footprint id, as `kicad.toml` maps a part to it: `Resistor_SMD:R_0402_1005Metric`.
    pub footprint: String,
    /// The STEP file, relative to the project root, or absolute, and it may start with an
    /// environment variable (`${KICAD9_3DMODEL_DIR}/...`). It must be inside the project or
    /// inside the models folder.
    pub file: String,
    /// The offset in mm along x, y and z of the footprint's frame, as `.model.x`, `.y`, `.z`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: [f64; 3],
    /// The rotation in degrees about x, then y, then z, before the offset, as `.model.rx`,
    /// `.ry`, `.rz`: a KiCad footprint's `(model ... (rotate (xyz ...)))` copied in.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: [f64; 3],
}

fn is_zero(v: &[f64; 3]) -> bool {
    *v == [0.0; 3]
}

/// Whether the model table of a manifest can work: each row a footprint id and a file, each
/// footprint once, numbers that are finite, and a folder that is not empty.
pub fn check_table(models: Option<&ModelsConfig>, rows: &[ModelRow]) -> Result<(), String> {
    if let Some(folder) = models.and_then(|m| m.folder.as_deref())
        && folder.trim().is_empty()
    {
        return Err(
            "[models] folder is empty: name the folder of STEP models, or leave the key out"
                .to_string(),
        );
    }
    let mut seen: Vec<&str> = Vec::new();
    for row in rows {
        let fp = row.footprint.as_str();
        if !fp
            .split_once(':')
            .is_some_and(|(lib, name)| !lib.is_empty() && !name.is_empty())
        {
            return Err(format!(
                "[[model]] footprint = {fp:?} is not a footprint id: write it as `kicad.toml` maps a part, `Library:Name`"
            ));
        }
        if seen.contains(&fp) {
            return Err(format!(
                "[[model]] footprint = {fp:?} has two rows: give each footprint one model"
            ));
        }
        seen.push(fp);
        if row.file.trim().is_empty() {
            return Err(format!(
                "[[model]] footprint = {fp:?} has an empty file: name its STEP file, or remove the row"
            ));
        }
        if row
            .offset
            .iter()
            .chain(&row.rotation)
            .any(|v| !v.is_finite())
        {
            return Err(format!(
                "[[model]] footprint = {fp:?}: offset and rotation are three finite numbers"
            ));
        }
    }
    Ok(())
}

/// The project's model table as [`locate`] uses it.
#[derive(Debug, Clone, Default)]
struct Table {
    /// The `[models] folder`, as `netsmith.toml` spells it.
    folder: Option<String>,
    rows: Vec<ModelRow>,
}

/// Where the files of a target are on disk, to find a model relative to the file that names it.
#[derive(Debug, Clone)]
pub struct Places {
    /// The folder a model must stay inside: the project's root, or for a file with no project
    /// the folder of that file.
    pub root: PathBuf,
    /// The loaded name of each top-level file and its path.
    files: Vec<(String, PathBuf)>,
    /// The folder the names of the project's own modules are relative to.
    project: Option<PathBuf>,
    /// The project's model table (issue #763); `None` for a project with neither a
    /// `[models] folder` nor a `[[model]]` row, or a file with no project.
    table: Option<Table>,
}

impl Places {
    /// The places of `target`'s files.
    pub fn of(target: &Target) -> Places {
        let project = target.project.as_ref().map(|p| absolute(&p.root));
        let files: Vec<(String, PathBuf)> = target
            .files
            .iter()
            .map(|f| (f.name.clone(), absolute(&f.path)))
            .collect();
        let root = project.clone().unwrap_or_else(|| {
            files
                .first()
                .and_then(|(_, p)| p.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| absolute(Path::new(".")))
        });
        let table = target.project.as_ref().and_then(|p| {
            let m = &p.manifest;
            let folder = m.models.as_ref().and_then(|c| c.folder.clone());
            (folder.is_some() || !m.model.is_empty()).then(|| Table {
                folder,
                rows: m.model.clone(),
            })
        });
        Places {
            root,
            files,
            project,
            table,
        }
    }

    /// The places of the files of a project at `root` (or of a file in the folder `root` with
    /// no project), where every loaded name is relative to `root`. For tests.
    pub fn under(root: &Path) -> Places {
        let root = absolute(root);
        Places {
            project: Some(root.clone()),
            root,
            files: Vec::new(),
            table: None,
        }
    }

    /// These places with the model table `folder` and `rows`, as a manifest states them. For
    /// tests.
    pub fn with_table(mut self, folder: Option<&str>, rows: Vec<ModelRow>) -> Places {
        self.table = Some(Table {
            folder: folder.map(str::to_string),
            rows,
        });
        self
    }

    /// The file on disk that a source was loaded as: a top-level file, its physical file, or a
    /// module of the project.
    fn path_of(&self, name: &str) -> PathBuf {
        for (file, path) in &self.files {
            if file == name {
                return path.clone();
            }
            if netsmith_check::physical_file_name(file) == name {
                return physical_path(path);
            }
        }
        match &self.project {
            Some(root) => root.join(name),
            None => absolute(Path::new(name)),
        }
    }
}

/// Find the file of each STEP model of `design`, and of each case model (ADR 0109), and put it
/// in `resolved`, unless it is not there. `design.features.models` and `case_models` must have
/// their `stated_in` filled ([`netsmith_pipeline::constraints::name_model_sources`],
/// [`netsmith_pipeline::constraints::name_case_model_sources`]).
///
/// With a model table ([`Places::of`] a project that has one), each part that states no
/// `.model.file` is given the table's model of the footprint `mapping` maps it to
/// ([`from_table`]). With no `mapping` (the project's `kicad.toml` cannot be read) the table
/// gives none.
pub fn locate(design: &mut Design, places: &Places, mapping: Option<&Mapping>) -> Located {
    let mut found = Located::default();
    for model in &mut design.features.models {
        // An empty file says the part has no model (issue #763).
        if model.file.is_empty() {
            continue;
        }
        let path = &model.path;
        let field = format!("{path}.model.file");
        match find(
            &field,
            &model.file,
            &model.source,
            &model.stated_in,
            places,
            "STEP file",
        ) {
            Find::There(file) => model.resolved = Some(file),
            Find::Missing(file) => found.warnings.push(format!(
                "`{field}` names `{}` ({}), and there is no such file, so `{path}` is drawn with its generated shape; add the file, or leave it out on a machine that may not have it",
                shown(&file, &places.root),
                model.source
            )),
            Find::Refused(error) => found.errors.push(error),
            Find::Unstated => {}
        }
    }
    for case in &mut design.features.case_models {
        let field = format!("{}.file", case.name);
        match find(
            &field,
            &case.file,
            &case.source,
            &case.stated_in,
            places,
            "STEP file",
        ) {
            Find::There(file) => case.resolved = Some(file),
            Find::Missing(file) => found.unchecked.push(format!(
                "`{field}` names `{}` ({}), and there is no such file, so the board's fit in its case model `{}` is not checked; add the file, or leave it out on a machine that may not have it",
                shown(&file, &places.root),
                case.source,
                case.name
            )),
            Find::Refused(error) => found.errors.push(error),
            Find::Unstated => {}
        }
    }
    for logo in &mut design.features.logos {
        let field = format!("{}.file", logo.path);
        match find(
            &field,
            &logo.file,
            &logo.source,
            &logo.stated_in,
            places,
            "SVG or DXF file",
        ) {
            Find::There(file) => {
                let drawn = std::fs::read_to_string(&file)
                    .map_err(|e| format!("it cannot be read: {e}"))
                    .and_then(|text| netsmith_dxf::read_logo(&logo.file, &text, logo.size));
                match drawn {
                    Ok(outlines) => logo.outlines = outlines,
                    Err(e) => found.errors.push(format!(
                        "`{field}` ({}), the logo's file `{}`: {e}",
                        logo.source,
                        shown(&file, &places.root)
                    )),
                }
            }
            Find::Missing(file) => found.errors.push(format!(
                "`{field}` names `{}` ({}), and there is no such file, so `{}` has nothing to print: add the SVG or DXF file, or take the logo out",
                shown(&file, &places.root),
                logo.source,
                logo.path
            )),
            Find::Refused(error) => found.errors.push(error),
            Find::Unstated => {}
        }
    }
    if let (Some(table), Some(mapping)) = (&places.table, mapping) {
        from_table(design, places, table, mapping, &mut found);
    }
    // A part that states a placement and no file, whom the table gave no model: there is
    // nothing to place (issue #1005).
    for model in &design.features.models {
        if let Some(mask) = model.placement_only {
            found
                .errors
                .push(no_model_to_place(&model.path, mask, places.table.is_some()));
        }
    }
    found
}

/// The error of a part that states members of its model's placement and no `.model.file`, and
/// whom the project's model table (when there is one, `table`) gives no model.
fn no_model_to_place(path: &str, mask: [bool; 6], table: bool) -> String {
    let stated: Vec<String> = ["x", "y", "z", "rx", "ry", "rz"]
        .iter()
        .zip(mask)
        .filter(|(_, on)| *on)
        .map(|(n, _)| format!("`{path}.model.{n}`"))
        .collect();
    format!(
        "{} {} stated, and `{path}.model.file` is not{}, so there is no STEP model to place: name the file with `.{path}.model.file &= \"models/part.step\"`, or remove the placement",
        stated.join(", "),
        if stated.len() == 1 { "is" } else { "are" },
        if table {
            ", and the model table (netsmith.toml) has no model for the part's footprint"
        } else {
            ""
        }
    )
}

/// The environment variable a path starts with, `${NAME}`, put in: the path, or the name of
/// a variable that is not set.
fn expand(stated: &str) -> Result<PathBuf, String> {
    match stated.strip_prefix("${").and_then(|r| r.split_once('}')) {
        Some((name, rest)) => {
            let value = std::env::var_os(name).ok_or_else(|| name.to_string())?;
            let rest = rest.trim_start_matches(['/', '\\']);
            Ok(if rest.is_empty() {
                PathBuf::from(value)
            } else {
                PathBuf::from(value).join(rest)
            })
        }
        None => Ok(PathBuf::from(stated)),
    }
}

/// Where a path of the model table is: put its variable in and make it absolute from the
/// project root.
fn table_path(stated: &str, root: &Path) -> Result<PathBuf, String> {
    let path = expand(stated)?;
    Ok(normal(&if path.is_absolute() {
        path
    } else {
        root.join(path)
    }))
}

/// The file of footprint `id` in the models folder `folder`, KiCad's layout:
/// `<folder>/<Lib>.3dshapes/<Name>.step` (or `.stp`, `.STEP`), with the spelling a message and
/// the notes show (`<folder as stated>/<Lib>.3dshapes/<Name>.step`). `None` for an id that is
/// not `Lib:Name` of a library (a generated `gen:` footprint) or a file that is not there.
fn in_folder(id: &str, folder: &Path, stated: &str) -> Option<(PathBuf, String)> {
    if netsmith_kicad::FootprintSpec::from_id(id).is_some() {
        return None;
    }
    let (lib, name) = id.split_once(':')?;
    if lib.is_empty() || name.is_empty() || name.contains(['/', '\\']) || lib.contains(['/', '\\'])
    {
        return None;
    }
    ["step", "stp", "STEP"].iter().find_map(|ext| {
        let file = folder
            .join(format!("{lib}.3dshapes"))
            .join(format!("{name}.{ext}"));
        file.is_file().then(|| {
            let shown = format!(
                "{}/{lib}.3dshapes/{name}.{ext}",
                stated.trim_end_matches(['/', '\\'])
            );
            (file, shown)
        })
    })
}

/// Give each part of `design` that states no `.model.file` the model of its footprint in the
/// project's model table (issue #763): a `[[model]]` row of the footprint's id, else its file
/// in the `[models] folder`. A row whose file is outside the project and the folder is an
/// error; one whose file is not there, or names a variable that is not set, a warning. A
/// folder that is not there is one warning. The notes say how many parts the table drew and
/// which parts have no model.
fn from_table(
    design: &mut Design,
    places: &Places,
    table: &Table,
    mapping: &Mapping,
    found: &mut Located,
) {
    let root = &places.root;
    let folder: Option<(PathBuf, &str)> = match &table.folder {
        None => None,
        Some(stated) => match table_path(stated, root) {
            Ok(dir) if dir.is_dir() => Some((dir, stated.as_str())),
            Ok(dir) => {
                found.warnings.push(format!(
                    "the models folder `{stated}` (netsmith.toml `[models] folder`) is `{}`, which is not a folder, so no part is drawn from it; parts it would give a model keep their generated shape",
                    dir.display()
                ));
                None
            }
            Err(var) => {
                found.warnings.push(format!(
                    "the models folder `{stated}` (netsmith.toml `[models] folder`) names the environment variable `{var}`, which is not set, so no part is drawn from it; parts it would give a model keep their generated shape"
                ));
                None
            }
        },
    };
    // A part that states a placement and no file is still given its model by the table: its
    // numbers replace the row's (issue #1005).
    let stated: std::collections::BTreeSet<String> = design
        .features
        .models
        .iter()
        .filter(|m| m.placement_only.is_none())
        .map(|m| m.path.clone())
        .collect();
    // The parts of each row whose file is not there, by row, for one warning a row.
    let mut row_missing: BTreeMap<usize, (String, Vec<String>)> = BTreeMap::new();
    let (mut drawn, mut none): (usize, Vec<String>) = (0, Vec::new());
    let mut added = Vec::new();
    let mut replaced: Vec<(usize, netsmith_core::StepModel)> = Vec::new();
    for part in &design.parts {
        if part.declared_kind.is_some_and(|k| {
            k.is_mechanical()
                || matches!(
                    k,
                    netsmith_core::PartKind::Label | netsmith_core::PartKind::Logo
                )
        }) {
            continue;
        }
        if stated.contains(&part.path) {
            if design
                .features
                .models
                .iter()
                .any(|m| m.path == part.path && m.file.is_empty())
            {
                none.push(format!("`{}` (`.model.file` is empty)", part.path));
            }
            continue;
        }
        let Ok(mapped) = mapping.map_part(&part.path, &part.kind, part.package.as_deref()) else {
            continue;
        };
        let id = mapped.footprint;
        let model = if let Some((index, row)) = table
            .rows
            .iter()
            .enumerate()
            .find(|(_, r)| r.footprint == id)
        {
            let file = match table_path(&row.file, root) {
                Ok(file) => file,
                Err(var) => {
                    row_missing
                        .entry(index)
                        .or_insert_with(|| {
                            (
                                format!("names the environment variable `{var}`, which is not set"),
                                Vec::new(),
                            )
                        })
                        .1
                        .push(part.path.clone());
                    none.push(format!("`{}` ({id})", part.path));
                    continue;
                }
            };
            let inside = file.starts_with(root)
                || folder
                    .as_ref()
                    .is_some_and(|(dir, _)| file.starts_with(dir));
            if !inside {
                let error = format!(
                    "the `[[model]]` row of `{id}` (netsmith.toml) names `{}`, which is `{}`, outside the project (`{}`) and the models folder: put the STEP file inside one of them",
                    row.file,
                    file.display(),
                    root.display()
                );
                if !found.errors.contains(&error) {
                    found.errors.push(error);
                }
                continue;
            }
            if !file.is_file() {
                row_missing
                    .entry(index)
                    .or_insert_with(|| {
                        (
                            format!("names `{}`, and there is no such file", row.file),
                            Vec::new(),
                        )
                    })
                    .1
                    .push(part.path.clone());
                none.push(format!("`{}` ({id})", part.path));
                continue;
            }
            (
                file,
                row.file.clone(),
                row.offset,
                row.rotation,
                "the `[[model]]` row".to_string(),
            )
        } else if let Some((file, shown)) = folder
            .as_ref()
            .and_then(|(dir, stated)| in_folder(&id, dir, stated))
        {
            (
                file,
                shown,
                [0.0; 3],
                [0.0; 3],
                "the models folder".to_string(),
            )
        } else {
            none.push(format!("`{}` ({id})", part.path));
            continue;
        };
        let (file, shown, mut offset, mut rotation, from) = model;
        drawn += 1;
        let mut made = netsmith_core::StepModel {
            path: part.path.clone(),
            file: shown,
            offset,
            rotation,
            source: format!("netsmith.toml, {from} of `{id}`"),
            stated_in: Vec::new(),
            resolved: Some(file),
            footprint: Some(id),
            placement_only: None,
        };
        // The members the part states take the place of the row's numbers.
        if let Some(i) = design
            .features
            .models
            .iter()
            .position(|m| m.path == part.path && m.placement_only.is_some())
        {
            let own = &design.features.models[i];
            let mask = own.placement_only.unwrap_or_default();
            for k in 0..3 {
                if mask[k] {
                    offset[k] = own.offset[k];
                }
                if mask[k + 3] {
                    rotation[k] = own.rotation[k];
                }
            }
            made.offset = offset;
            made.rotation = rotation;
            made.stated_in = own.stated_in.clone();
            if !own.source.is_empty() {
                made.source = format!("{}, placed by {}", made.source, own.source);
            }
            replaced.push((i, made));
        } else {
            added.push(made);
        }
    }
    for (i, made) in replaced {
        design.features.models[i] = made;
    }
    for (index, (why, parts)) in row_missing {
        let id = &table.rows[index].footprint;
        found.warnings.push(format!(
            "the `[[model]]` row of `{id}` (netsmith.toml) {why}, so {} drawn with {} generated shape; add the file, or leave it out on a machine that may not have it",
            listed(&parts),
            if parts.len() == 1 { "its" } else { "their" }
        ));
    }
    design.features.models.extend(added);
    let mut note = format!(
        "the model table (netsmith.toml) draws {drawn} {} from a STEP model",
        if drawn == 1 { "part" } else { "parts" }
    );
    if none.is_empty() {
        note.push_str("; every part has a model");
    } else {
        note.push_str(&format!(
            "; {} {} no model, and {} drawn with {} generated shape: {}",
            none.len(),
            if none.len() == 1 {
                "part has"
            } else {
                "parts have"
            },
            if none.len() == 1 { "it is" } else { "they are" },
            if none.len() == 1 { "its" } else { "their" },
            none.join(", ")
        ));
    }
    found.notes.push(note);
}

/// `parts`, each as `` `path` ``, for a message: `` `c1` is `` or `` `c1`, `c2` are ``.
fn listed(parts: &[String]) -> String {
    let names: Vec<String> = parts.iter().map(|p| format!("`{p}`")).collect();
    format!(
        "{} {}",
        names.join(", "),
        if parts.len() == 1 { "is" } else { "are" }
    )
}

/// What [`find`] found of one file.
enum Find {
    /// The file, inside the project.
    There(PathBuf),
    /// Where the file would be, inside the project, and it is not there.
    Missing(PathBuf),
    /// Why the path is refused: absolute, outside the project, or naming two files.
    Refused(String),
    /// No statement says where it is.
    Unstated,
}

/// Find the file `stated` (the value of the field `field`, stated at `source` in the files
/// `stated_in`) relative to the file that states it.
fn find(
    field: &str,
    stated: &str,
    source: &str,
    stated_in: &[String],
    places: &Places,
    what: &str,
) -> Find {
    let path = Path::new(stated);
    if path.has_root() || path.is_absolute() {
        return Find::Refused(format!(
            "`{field}` is `{stated}`, an absolute path ({source}); name the {what} relative to the file that states it, inside the project"
        ));
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for name in stated_in {
        let base = places.path_of(name);
        let dir = base.parent().unwrap_or(&places.root);
        let file = normal(&dir.join(path));
        if !files.contains(&file) {
            files.push(file);
        }
    }
    let Some(file) = files.first().cloned() else {
        return Find::Unstated;
    };
    if files.len() > 1 {
        return Find::Refused(format!(
            "`{field}` is `{stated}` in each of {source}, and relative to each of those files it names a different file ({}): state it once, or with a path that names one file",
            files
                .iter()
                .map(|f| format!("`{}`", shown(f, &places.root)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !file.starts_with(&places.root) {
        return Find::Refused(format!(
            "`{field}` is `{stated}` ({source}), which is `{}`, outside the project (`{}`): put the {what} inside the project, as netsmith never reads a file from elsewhere",
            file.display(),
            places.root.display()
        ));
    }
    if file.is_file() {
        Find::There(file)
    } else {
        Find::Missing(file)
    }
}

/// A path from the current directory as an absolute one, with no `.` or `..`.
fn absolute(path: &Path) -> PathBuf {
    normal(&std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()))
}

/// `path` with its `.` and `..` taken out by its text alone, so that a path outside the
/// project is seen whether or not it exists.
fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(c);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// A file inside `root` as its path from `root` with `/`, else as it is.
fn shown(file: &Path, root: &Path) -> String {
    match file.strip_prefix(root) {
        Ok(rest) => rest
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => file.display().to_string(),
    }
}

/// A model file read and drawn, by its place on disk: its SHA-256 and what reading it gave.
type Read = (String, Result<Arc<StepModel>, String>);

/// The files read so far, by path, with the SHA-256 of the bytes they were read from: a file
/// is read again only when its bytes change, so a build, the editor's rebuilds and `fab` of
/// several boards read and tessellate each model once.
static CACHE: Mutex<BTreeMap<PathBuf, Read>> = Mutex::new(BTreeMap::new());

/// The SHA-256 of `bytes`, in lower-case hex.
pub fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Reads the file at `path` (ADR 0108), or takes it from the files read before when its bytes
/// are the same: its SHA-256, and its model or why it cannot be read.
pub fn read(path: &Path) -> Result<(String, Arc<StepModel>), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let sha = sha256(&bytes);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((known, read)) = cache.get(path)
        && *known == sha
    {
        return read.clone().map(|r| (sha, r));
    }
    let read = netsmith_3d::load_step(&bytes)
        .map(Arc::new)
        .map_err(|e| e.to_string());
    cache.insert(path.to_path_buf(), (sha.clone(), read.clone()));
    read.map(|r| (sha, r))
}

/// The models of `design` found on disk ([`locate`]), each read ([`read`]), for the scene to
/// draw; and a warning for each that cannot be read, whose part keeps its generated shape.
pub fn uses(design: &Design) -> (Vec<ModelUse>, Vec<String>) {
    let mut uses = Vec::new();
    let mut warnings = Vec::new();
    for model in &design.features.models {
        let Some(file) = &model.resolved else {
            continue;
        };
        let path = &model.path;
        match read(file) {
            Ok((sha256, read)) => uses.push(ModelUse {
                path: path.clone(),
                file: model.file.clone(),
                sha256,
                offset: model.offset,
                rotation: model.rotation,
                read,
            }),
            Err(e) => warnings.push(format!(
                "`{path}.model.file` names `{}` ({}), which netsmith cannot read as a STEP model ({e}), so `{path}` is drawn with its generated shape",
                model.file, model.source
            )),
        }
    }
    (uses, warnings)
}
