//! Making, moving and removing the files of the project from the editor.
//!
//! A path of a request is written from the project root with `/` (`src/sheets/adc.nsm`) and
//! is checked in two steps before anything is touched. First its words: no `..`, no absolute
//! path, no `\`, no hidden or reserved folder, and every name below `src/` a module's name
//! (letters, digits and `_`), because the project does not build with any other. Then the
//! disk: the folder it is in must exist and, with its symbolic links followed, still be below
//! the project's `src/`. A link is never followed to a file: it is neither moved nor removed.
//! Nothing is overwritten: a path that is taken (in any case of letters) is a 409.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use netsmith_editor::{
    ApiError,
    api::{Changed, FileRole},
};

use super::Workspace;
use crate::project::{MANIFEST, SOURCE_DIR, is_identifier};

/// A new file's text is a source file, not a download.
const MAX_TEXT: usize = 1 << 20;
const MAX_NAME: usize = 64;
/// Folders the tool writes, which a request never names.
const RESERVED_ROOTS: [&str; 4] = [".git", "target", "out", "fab"];
/// Names a file system of Windows refuses.
const WINDOWS_RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

fn leaves(message: impl Into<String>) -> ApiError {
    ApiError::forbidden(message)
}

/// The words of a path, or why it is refused.
pub(super) fn segments(path: &str) -> Result<Vec<String>, ApiError> {
    if path.is_empty() {
        return Err(ApiError::bad_request("the path is empty"));
    }
    if path.chars().any(|c| c.is_control()) {
        return Err(ApiError::bad_request("the path has a control character"));
    }
    if path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return Err(leaves(format!(
            "`{path}` is not a path from the project root: write it with `/`, as `src/power.nsm`"
        )));
    }
    let mut out = Vec::new();
    for word in path.split('/') {
        match word {
            "" => {
                return Err(ApiError::bad_request(format!(
                    "`{path}` has an empty name in it"
                )));
            }
            "." | ".." => {
                return Err(leaves(format!(
                    "`{path}` leaves the project: `{word}` is not allowed"
                )));
            }
            w if w.starts_with('.') => {
                return Err(leaves(format!(
                    "`{path}`: a name that starts with `.` is hidden, and the editor does not touch it"
                )));
            }
            w => out.push(w.to_string()),
        }
    }
    if RESERVED_ROOTS.contains(&out[0].as_str()) {
        return Err(leaves(format!(
            "`{}` is written by netsmith, not edited",
            out[0]
        )));
    }
    Ok(out)
}

/// What a name below `src/` must be, said for the person.
pub(super) const NAME_RULE: &str =
    "a name below src/ is a module name: letters, digits and `_`, not starting with a digit";

fn windows_reserved(stem: &str) -> bool {
    WINDOWS_RESERVED.contains(&stem.to_lowercase().as_str())
}

/// A folder's name.
pub(super) fn check_folder_name(name: &str) -> Result<(), ApiError> {
    if name.len() > MAX_NAME {
        return Err(ApiError::bad_request(format!(
            "`{name}` is longer than {MAX_NAME} characters"
        )));
    }
    if !is_identifier(name) || windows_reserved(name) {
        return Err(ApiError::bad_request(format!(
            "`{name}` is not a folder name: {NAME_RULE}"
        )));
    }
    Ok(())
}

/// A file's name: a module's `power.nsm`, or a physical file's `power.physical.nsm`. Answers
/// the stem and whether it is a physical file's.
pub(super) fn check_file_name(name: &str) -> Result<(String, bool), ApiError> {
    if name.len() > MAX_NAME {
        return Err(ApiError::bad_request(format!(
            "`{name}` is longer than {MAX_NAME} characters"
        )));
    }
    let Some(base) = name.strip_suffix(".nsm") else {
        return Err(ApiError::bad_request(format!(
            "`{name}` is not a source file: its name ends in `.nsm`"
        )));
    };
    let (stem, physical) = match base.strip_suffix(".physical") {
        Some(stem) => (stem, true),
        None => (base, false),
    };
    if !is_identifier(stem) || windows_reserved(stem) {
        return Err(ApiError::bad_request(format!(
            "`{name}` is not a source file's name: {NAME_RULE}"
        )));
    }
    Ok((stem.to_string(), physical))
}

fn io_error(path: &str, e: &io::Error) -> ApiError {
    match e.kind() {
        io::ErrorKind::AlreadyExists => ApiError::conflict(format!("`{path}` is already there")),
        io::ErrorKind::NotFound => ApiError::not_found(format!("`{path}` is not there")),
        io::ErrorKind::PermissionDenied => leaves(format!("`{path}`: {e}")),
        _ => ApiError::internal(format!("`{path}`: {e}")),
    }
}

/// Whether something (a link too, a broken one too) is at `path`.
fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Whether the folder has an entry with this name in any case of letters.
fn taken(parent: &Path, name: &str) -> bool {
    if exists(&parent.join(name)) {
        return true;
    }
    let lower = name.to_lowercase();
    fs::read_dir(parent).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|e| e.file_name().to_string_lossy().to_lowercase() == lower)
    })
}

impl Workspace {
    /// The project's root as the disk has it (links resolved), or why there is none.
    fn project_root(&self) -> Result<PathBuf, ApiError> {
        let Some(project) = &self.target.project else {
            return Err(ApiError::bad_request(
                "files can be made and changed only in a project: there is no netsmith.toml here",
            ));
        };
        fs::canonicalize(&project.root)
            .map_err(|e| ApiError::internal(format!("{}: {e}", project.root.display())))
    }

    /// The folder `segments` is in, which must exist and, links followed, be below `src/`.
    fn parent_of(root: &Path, segments: &[String]) -> Result<PathBuf, ApiError> {
        let mut dir = root.to_path_buf();
        for word in &segments[..segments.len() - 1] {
            dir.push(word);
        }
        let shown = segments[..segments.len() - 1].join("/");
        let canonical = fs::canonicalize(&dir).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => {
                ApiError::not_found(format!("the folder `{shown}` is not there"))
            }
            _ => io_error(&shown, &e),
        })?;
        let src = root.join(SOURCE_DIR);
        if !canonical.starts_with(&src) {
            return Err(leaves(format!(
                "`{shown}` is not inside the project's {SOURCE_DIR}/ folder"
            )));
        }
        if !canonical.is_dir() {
            return Err(ApiError::bad_request(format!("`{shown}` is not a folder")));
        }
        Ok(canonical)
    }

    /// A path below `src/` that is to be made: its words, and the folder it goes in.
    fn place(root: &Path, path: &str, file: bool) -> Result<Placed, ApiError> {
        let words = segments(path)?;
        if words.len() < 2 || words[0] != SOURCE_DIR {
            return Err(leaves(format!(
                "`{path}`: new files and folders go below {SOURCE_DIR}/"
            )));
        }
        for dir in &words[1..words.len() - 1] {
            check_folder_name(dir)?;
        }
        let last = &words[words.len() - 1];
        let named = if file {
            Some(check_file_name(last)?)
        } else {
            check_folder_name(last)?;
            None
        };
        let parent = Self::parent_of(root, &words)?;
        Ok(Placed {
            words,
            parent,
            named,
        })
    }

    /// The check of the files, made again now, so the page's next question sees the change.
    fn changed(&self, path: String) -> Changed {
        drop(self.fresh());
        Changed { path }
    }

    pub(super) fn make_file(&self, path: &str, text: &str) -> Result<Changed, ApiError> {
        let root = self.project_root()?;
        let _one_at_a_time = self.editing.lock().unwrap_or_else(|e| e.into_inner());
        let Placed {
            words,
            parent,
            named,
        } = Self::place(&root, path, true)?;
        let name = words.last().expect("a path has a name");
        if text.len() > MAX_TEXT || text.contains('\0') {
            return Err(ApiError::bad_request(
                "the text is too long or has a NUL in it",
            ));
        }
        let (stem, physical) = named.expect("a file has a name");
        if physical && !parent.join(format!("{stem}.nsm")).is_file() {
            return Err(ApiError::bad_request(format!(
                "`{name}` needs its source beside it: make `{stem}.nsm` first"
            )));
        }
        if taken(&parent, name) {
            return Err(ApiError::conflict(format!(
                "`{}` is already there",
                words.join("/")
            )));
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(parent.join(name))
            .map_err(|e| io_error(&words.join("/"), &e))?;
        file.write_all(text.as_bytes())
            .map_err(|e| io_error(&words.join("/"), &e))?;
        Ok(self.changed(words.join("/")))
    }

    pub(super) fn make_dir(&self, path: &str) -> Result<Changed, ApiError> {
        let root = self.project_root()?;
        let _one_at_a_time = self.editing.lock().unwrap_or_else(|e| e.into_inner());
        let Placed { words, parent, .. } = Self::place(&root, path, false)?;
        let name = words.last().expect("a path has a name");
        if taken(&parent, name) {
            return Err(ApiError::conflict(format!(
                "`{}` is already there",
                words.join("/")
            )));
        }
        fs::create_dir(parent.join(name)).map_err(|e| io_error(&words.join("/"), &e))?;
        Ok(self.changed(words.join("/")))
    }

    /// What `path` is in the tree: a file the editor may change, or a folder below `src/`.
    fn thing(&self, root: &Path, path: &str) -> Result<Thing, ApiError> {
        let words = segments(path)?;
        if let Ok(entry) = self.entry(path) {
            match entry.role {
                FileRole::Manifest => {
                    return Err(leaves(format!(
                        "`{MANIFEST}` is the project's manifest: edit it outside the editor"
                    )));
                }
                FileRole::Std => {
                    return Err(leaves(format!(
                        "`{path}` is part of the standard library, which is built into netsmith"
                    )));
                }
                _ => {}
            }
            let Some(file) = entry.fs.filter(|_| !entry.readonly) else {
                return Err(leaves(format!("`{path}` is read-only")));
            };
            return Ok(Thing::File {
                abs: file,
                top_level: entry.role == FileRole::TopLevel,
                physical: entry.role == FileRole::Physical,
                layout: entry.role == FileRole::SchematicLayout,
            });
        }
        if words.len() >= 2 && words[0] == SOURCE_DIR {
            let parent = Self::parent_of(root, &words)?;
            let abs = parent.join(&words[words.len() - 1]);
            match fs::symlink_metadata(&abs) {
                Ok(m) if m.file_type().is_symlink() => {
                    return Err(leaves(format!(
                        "`{path}` is a symbolic link: the editor does not follow it"
                    )));
                }
                Ok(m) if m.is_dir() => {
                    return Ok(Thing::Folder {
                        abs,
                        rel: words.join("/"),
                    });
                }
                _ => {}
            }
        }
        if words.len() == 1 && words[0] == SOURCE_DIR {
            return Err(leaves(format!(
                "`{SOURCE_DIR}` is where the modules are: it is not renamed or removed"
            )));
        }
        Err(ApiError::not_found(format!(
            "`{path}` is not a file or folder of the project's tree"
        )))
    }

    /// The top-level files below a folder, by name.
    fn top_level_in(&self, folder: &str) -> Vec<String> {
        let prefix = format!("{folder}/");
        self.target
            .files
            .iter()
            .filter(|f| f.name.starts_with(&prefix))
            .map(|f| f.name.clone())
            .collect()
    }

    pub(super) fn move_path(&self, from: &str, to: &str) -> Result<Changed, ApiError> {
        let root = self.project_root()?;
        let _one_at_a_time = self.editing.lock().unwrap_or_else(|e| e.into_inner());
        let thing = self.thing(&root, from)?;
        let words = segments(to)?;
        if words.len() < 2 || words[0] != SOURCE_DIR {
            return Err(leaves(format!(
                "`{to}`: files and folders are moved only below {SOURCE_DIR}/"
            )));
        }
        for dir in &words[1..words.len() - 1] {
            check_folder_name(dir)?;
        }
        let name = words[words.len() - 1].clone();
        let parent = Self::parent_of(&root, &words)?;
        let shown = words.join("/");
        match thing {
            Thing::File {
                top_level: true, ..
            } => Err(ApiError::bad_request(format!(
                "`{from}` is a top-level file, which netsmith.toml lists by name: change the list there, then rename it"
            ))),
            Thing::File { physical: true, .. } => Err(ApiError::bad_request(format!(
                "`{from}` is a physical file: rename its source, and it goes with it"
            ))),
            Thing::File { layout: true, .. } => Err(ApiError::bad_request(format!(
                "`{from}` is a schematic layout file: rename its source, and it goes with it"
            ))),
            Thing::File { abs, .. } => {
                let (stem, physical) = check_file_name(&name)?;
                if physical {
                    return Err(ApiError::bad_request(format!(
                        "`{name}` is a physical file's name: a module is `{stem}.nsm`"
                    )));
                }
                if taken(&parent, &name) {
                    return Err(ApiError::conflict(format!("`{shown}` is already there")));
                }
                // The physical file beside the source goes with it.
                let side = crate::physical::physical_path(&abs);
                let side_to = parent.join(format!("{stem}.physical.nsm"));
                let moves_side = side.is_file();
                if moves_side && taken(&parent, &format!("{stem}.physical.nsm")) {
                    return Err(ApiError::conflict(format!(
                        "`{stem}.physical.nsm` is already there"
                    )));
                }
                // So does the schematic layout file (ADR 0061): it is `<stem>.sch.toml`.
                let drawn = netsmith_view::schematic_hints_path(&abs);
                let drawn_to = parent.join(format!("{stem}.sch.toml"));
                let moves_drawn = drawn.is_file();
                if moves_drawn && taken(&parent, &format!("{stem}.sch.toml")) {
                    return Err(ApiError::conflict(format!(
                        "`{stem}.sch.toml` is already there"
                    )));
                }
                fs::rename(&abs, parent.join(&name)).map_err(|e| io_error(from, &e))?;
                if moves_side && let Err(e) = fs::rename(&side, &side_to) {
                    let _ = fs::rename(parent.join(&name), &abs);
                    return Err(io_error(from, &e));
                }
                if moves_drawn && let Err(e) = fs::rename(&drawn, &drawn_to) {
                    if moves_side {
                        let _ = fs::rename(&side_to, &side);
                    }
                    let _ = fs::rename(parent.join(&name), &abs);
                    return Err(io_error(from, &e));
                }
                Ok(self.changed(shown))
            }
            Thing::Folder { abs, rel } => {
                check_folder_name(&name)?;
                if shown == rel || shown.starts_with(&format!("{rel}/")) {
                    return Err(ApiError::bad_request(format!(
                        "`{from}` cannot be moved into itself"
                    )));
                }
                if let Some(top) = self.top_level_in(&rel).first() {
                    return Err(ApiError::bad_request(format!(
                        "`{from}` holds the top-level file `{top}`, which netsmith.toml lists by name: change the list there first"
                    )));
                }
                if taken(&parent, &name) {
                    return Err(ApiError::conflict(format!("`{shown}` is already there")));
                }
                fs::rename(&abs, parent.join(&name)).map_err(|e| io_error(from, &e))?;
                Ok(self.changed(shown))
            }
        }
    }

    pub(super) fn remove_path(&self, path: &str, confirm: bool) -> Result<Changed, ApiError> {
        let root = self.project_root()?;
        let _one_at_a_time = self.editing.lock().unwrap_or_else(|e| e.into_inner());
        match self.thing(&root, path)? {
            Thing::File {
                abs,
                top_level,
                physical,
                layout,
            } => {
                if top_level && !confirm {
                    return Err(ApiError::new(
                        428,
                        format!(
                            "`{path}` is a top-level file: removing it needs a yes (it is listed in netsmith.toml, which still names it)"
                        ),
                    ));
                }
                fs::remove_file(&abs).map_err(|e| io_error(path, &e))?;
                // The physical file beside a source would be left with nothing to describe.
                let side = crate::physical::physical_path(&abs);
                if !physical && side.is_file() {
                    fs::remove_file(&side).map_err(|e| io_error(path, &e))?;
                }
                // And the schematic layout file: an arrangement of a drawing that is gone.
                let drawn = netsmith_view::schematic_hints_path(&abs);
                if !layout && !physical && drawn.is_file() {
                    fs::remove_file(&drawn).map_err(|e| io_error(path, &e))?;
                }
            }
            Thing::Folder { abs, rel } => {
                let empty = fs::read_dir(&abs)
                    .map_err(|e| io_error(path, &e))?
                    .next()
                    .is_none();
                if !empty && !confirm {
                    let top = self.top_level_in(&rel);
                    let more = if top.is_empty() {
                        String::new()
                    } else {
                        format!(", among them the top-level file `{}`", top[0])
                    };
                    return Err(ApiError::new(
                        428,
                        format!("`{path}` has files in it{more}: removing it needs a yes"),
                    ));
                }
                if empty {
                    fs::remove_dir(&abs).map_err(|e| io_error(path, &e))?;
                } else {
                    fs::remove_dir_all(&abs).map_err(|e| io_error(path, &e))?;
                }
            }
        }
        Ok(self.changed(path.to_string()))
    }

    /// The folders below `src/`, with their paths from the project root, for the tree to show
    /// a folder that has no file in it yet. A link is not followed.
    pub(super) fn folders(&self) -> Vec<String> {
        let Some(project) = &self.target.project else {
            return Vec::new();
        };
        fn walk(dir: &Path, rel: &str, depth: usize, out: &mut Vec<String>) {
            if depth > 16 || out.len() > 2000 {
                return;
            }
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') || !entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }
                let path = format!("{rel}/{name}");
                out.push(path.clone());
                walk(&entry.path(), &path, depth + 1, out);
            }
        }
        let mut out = Vec::new();
        walk(&project.root.join(SOURCE_DIR), SOURCE_DIR, 0, &mut out);
        out.sort();
        out
    }
}

/// Where a new file or folder goes.
struct Placed {
    words: Vec<String>,
    /// The folder it goes in, as the disk has it.
    parent: PathBuf,
    /// A file's stem and whether it is a physical file's; `None` for a folder.
    named: Option<(String, bool)>,
}

enum Thing {
    File {
        abs: PathBuf,
        top_level: bool,
        physical: bool,
        /// A schematic layout file (`<stem>.sch.toml`): removed on its own, renamed with its source.
        layout: bool,
    },
    Folder {
        abs: PathBuf,
        rel: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(path: &str) -> u16 {
        segments(path).unwrap_err().status
    }

    #[test]
    fn a_path_that_leaves_the_project_or_is_hidden_or_reserved_is_refused() {
        for path in [
            "../x.nsm",
            "src/../x.nsm",
            "src/./x.nsm",
            "/etc/passwd",
            "src\\x.nsm",
            "C:/x.nsm",
            ".git/config",
            "src/.hidden/x.nsm",
            "target/x.nsm",
            "out/x.nsm",
            "fab/x.nsm",
        ] {
            assert_eq!(refused(path), 403, "{path}");
        }
        for path in ["", "src//x.nsm", "src/x.nsm/", "src/a\0b.nsm"] {
            assert_eq!(refused(path), 400, "{path:?}");
        }
        assert_eq!(
            segments("src/sheets/adc.nsm").unwrap(),
            ["src", "sheets", "adc.nsm"]
        );
    }

    #[test]
    fn a_file_is_a_modules_name_or_a_physical_files() {
        assert_eq!(
            check_file_name("power.nsm").unwrap(),
            ("power".to_string(), false)
        );
        assert_eq!(
            check_file_name("power.physical.nsm").unwrap(),
            ("power".to_string(), true)
        );
        for bad in [
            "power",
            "power.txt",
            "1power.nsm",
            "po-wer.nsm",
            "po wer.nsm",
            ".nsm",
            "a.b.nsm",
            "con.nsm",
            "NUL.nsm",
            &format!("{}.nsm", "a".repeat(70)),
        ] {
            assert_eq!(check_file_name(bad).unwrap_err().status, 400, "{bad}");
        }
    }

    #[test]
    fn a_folder_is_a_modules_name() {
        assert!(check_folder_name("sheets").is_ok() && check_folder_name("_x9").is_ok());
        for bad in ["a-b", "a b", "1a", "", "a.b", "com1"] {
            assert_eq!(check_folder_name(bad).unwrap_err().status, 400, "{bad}");
        }
    }
}
