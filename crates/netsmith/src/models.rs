//! The STEP models of a design's parts on disk (`.model`, ADR 0108): each `.model.file` is
//! found relative to the file that states it, and must stay inside the project. A path
//! outside the project is an error, and so are two statements that name two files. A file
//! that is not there is a warning, and the part is drawn with its generated shape: a maker's
//! license often keeps its model out of a public repository, so a clone or a CI job may not
//! have it.
//!
//! Only the file's place is settled here. It is not read: the model never gives the part a
//! height.

use std::path::{Component, Path, PathBuf};

use netsmith_core::Design;

use crate::physical::physical_path;
use crate::project::Target;

/// What [`locate`] found wrong.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Located {
    /// A model file that is not there: the part is drawn with its generated shape.
    pub warnings: Vec<String>,
    /// A path outside the project, an absolute path, or two statements that name two files.
    pub errors: Vec<String>,
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

/// Find the file of each STEP model of `design` and put it in `resolved`, unless it is not
/// there. `design.features.models` must have its `stated_in` filled
/// ([`crate::constraints::name_model_sources`]).
pub fn locate(design: &mut Design, places: &Places) -> Located {
    let mut found = Located::default();
    for model in &mut design.features.models {
        let path = &model.path;
        let stated = Path::new(&model.file);
        if stated.has_root() || stated.is_absolute() {
            found.errors.push(format!(
                "`{path}.model.file` is `{}`, an absolute path ({}); name the STEP file relative to the file that states it, inside the project",
                model.file, model.source
            ));
            continue;
        }
        let mut files: Vec<PathBuf> = Vec::new();
        for name in &model.stated_in {
            let base = places.path_of(name);
            let dir = base.parent().unwrap_or(&places.root);
            let file = normal(&dir.join(stated));
            if !files.contains(&file) {
                files.push(file);
            }
        }
        let Some(file) = files.first().cloned() else {
            continue;
        };
        if files.len() > 1 {
            found.errors.push(format!(
                "`{path}.model.file` is `{}` in each of {}, and relative to each of those files it names a different file ({}): state it once, or with a path that names one file",
                model.file,
                model.source,
                files
                    .iter()
                    .map(|f| format!("`{}`", shown(f, &places.root)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            continue;
        }
        if !file.starts_with(&places.root) {
            found.errors.push(format!(
                "`{path}.model.file` is `{}` ({}), which is `{}`, outside the project (`{}`): put the STEP file inside the project, as netsmith never reads a model from elsewhere",
                model.file,
                model.source,
                file.display(),
                places.root.display()
            ));
            continue;
        }
        if file.is_file() {
            model.resolved = Some(file);
        } else {
            found.warnings.push(format!(
                "`{path}.model.file` names `{}` ({}), and there is no such file, so `{path}` is drawn with its generated shape; add the file, or leave it out on a machine that may not have it",
                shown(&file, &places.root),
                model.source
            ));
        }
    }
    found
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
