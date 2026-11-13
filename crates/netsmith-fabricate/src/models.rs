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
//! [`locate`] settles a part model's place; [`uses`] reads it ([`read`], once per change of its
//! bytes) for the scene to draw, and [`crate::case_fit`] reads a case model the same way. The
//! model never gives the part a height, and a case model never gives the board an outline, a
//! hole or a height limit.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use netsmith_3d::scene3d::ModelUse;
use netsmith_3d::step::read::ReadModel;
use netsmith_core::Design;
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
        Places {
            root,
            files,
            project,
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
        }
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
pub fn locate(design: &mut Design, places: &Places) -> Located {
    let mut found = Located::default();
    for model in &mut design.features.models {
        let path = &model.path;
        let field = format!("{path}.model.file");
        match find(&field, &model.file, &model.source, &model.stated_in, places) {
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
        match find(&field, &case.file, &case.source, &case.stated_in, places) {
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
    found
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
fn find(field: &str, stated: &str, source: &str, stated_in: &[String], places: &Places) -> Find {
    let path = Path::new(stated);
    if path.has_root() || path.is_absolute() {
        return Find::Refused(format!(
            "`{field}` is `{stated}`, an absolute path ({source}); name the STEP file relative to the file that states it, inside the project"
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
            "`{field}` is `{stated}` ({source}), which is `{}`, outside the project (`{}`): put the STEP file inside the project, as netsmith never reads a model from elsewhere",
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
type Read = (String, Result<Arc<ReadModel>, String>);

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
pub fn read(path: &Path) -> Result<(String, Arc<ReadModel>), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let sha = sha256(&bytes);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((known, read)) = cache.get(path)
        && *known == sha
    {
        return read.clone().map(|r| (sha, r));
    }
    let read = netsmith_3d::step::read::load(&bytes)
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
