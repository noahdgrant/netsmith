//! A netsmith project: the `netsmith.toml` manifest, finding it from any directory below
//! the project root, and the files it names (ADR 0011).
//!
//! The manifest is read with the `toml` crate and holds one `[project]` table whose values
//! are strings, whole numbers and lists of strings, and the `[order.*]` tables of ADR 0035
//! (`crate::order`). Any other table or key is refused with its line and column, so the
//! file stays plain TOML.

use std::{
    collections::{BTreeMap, HashMap},
    env, fs, io,
    path::{Component, Path, PathBuf},
};

use netsmith_check::Loader;
use netsmith_solve::{Catalogs, Series};
use serde::{Deserialize, Serialize};

use crate::order::{self, Order, RawOrders, RenderedOrder};
use crate::supply::{Cache, Channel, FileProvider, Supply};

/// The manifest's file name.
pub const MANIFEST: &str = "netsmith.toml";

/// The directory below the project root whose `.nsm` files other files can import:
/// `src/power.nsm` is the module `src::power`.
pub const SOURCE_DIR: &str = "src";

/// The one KiCad version netsmith writes files for.
pub const KICAD_VERSION: i64 = 9;

/// What `netsmith.toml` says, the `[project]` table. Only `name` is required; the rest
/// are what `netsmith init` writes out. A key that is not one of these is refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    /// The files that hold boards, relative to the project root. `netsmith build`, `fab`
    /// and `editor` with no file act on these. A `system` is listed here the same way. A
    /// project that is only a library of modules lists none (`top_level = []`).
    #[serde(default = "default_top_level")]
    pub top_level: Vec<String>,
    /// Where `netsmith build` writes, relative to the project root.
    #[serde(default = "default_out")]
    pub out: String,
    /// Where `netsmith fab` writes, relative to the project root.
    #[serde(default = "default_fab")]
    pub fab: String,
    /// The KiCad version the output is for.
    #[serde(default = "default_kicad")]
    pub kicad: i64,
    /// The project's own catalog files, relative to the project root, one per catalog and
    /// named by it (`catalogs/acme_ldo.csv` is the catalog `acme_ldo`), in addition to the
    /// standard JLCPCB catalogs (ADR 0022).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catalogs: Vec<String>,
    /// Who assembles a part that nothing else speaks for: `"fab"`, `"hand"` or `"none"`
    /// (ADR 0028). Without it a part is the board house's if it is surface mount and
    /// soldered by hand if it is through hole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<String>,
    /// What a design that goes against a guideline gets (ADR 0036, ADR 0030): `"warn"`,
    /// the default, reports it and builds; `"error"` refuses to build. A run can ask for
    /// the second with `--strict`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guidelines: Option<String>,
    /// What a requirement on where a part goes that the placed board does not meet is
    /// (ADR 0045): `"error"`, the default, stops `fab`; `"warn"` lists it. A run can ask
    /// for the second with `--constraints warn`. It is not a guideline: `guidelines` and
    /// `--strict` do not govern it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constraints: Option<String>,
    /// The E-series the solver may pick generic resistors from when no catalog row works:
    /// `"E12"`, `"E24"` (the default) or `"E96"` (ADR 0047, provisional). `"none"` offers
    /// only the catalog's rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    /// The `[order.<name>.boards]` tables (ADR 0035): which boards go into one fabrication
    /// order, and how many of each. They are not part of the `[project]` table; they are
    /// read and written beside it.
    #[serde(skip)]
    pub orders: Vec<Order>,
    /// The `[supply]` table (ADR 0047, provisional): where price and stock come from. Read
    /// and written beside `[project]`. It has no field for a key or a password.
    #[serde(skip)]
    pub supply: Option<SupplyConfig>,
}

/// The `[supply]` table of `netsmith.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyConfig {
    /// `"file"`: prices and stock from a CSV file (`crate::supply::file`). Providers that
    /// ask a supplier are not built.
    pub provider: String,
    /// The file of the `file` provider, relative to the project root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The stock the bill of materials shows: `"jlcpcb-assembly"` (the default) or
    /// `"lcsc-retail"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// How many hours a cached answer is used without asking again (default 24).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_hours: Option<u64>,
}

impl SupplyConfig {
    fn validate(&self) -> Result<(), String> {
        if self.provider != "file" {
            return Err(format!(
                "{MANIFEST}: [supply] provider = {:?} is not one netsmith has: only \"file\" is built (providers that ask a supplier need the supplier's terms of use read first, ADR 0047)",
                self.provider
            ));
        }
        match &self.file {
            None => {
                return Err(format!(
                    "{MANIFEST}: [supply] provider = \"file\" needs `file = \"supply/quotes.csv\"`, the CSV of prices and stock"
                ));
            }
            Some(file) => check_relative("[supply] file", file)?,
        }
        if let Some(word) = &self.channel
            && Channel::parse(word).is_none()
        {
            return Err(format!(
                "{MANIFEST}: [supply] channel = {word:?} is not \"jlcpcb-assembly\" or \"lcsc-retail\""
            ));
        }
        Ok(())
    }
}

fn default_top_level() -> Vec<String> {
    vec![format!("{SOURCE_DIR}/main.nsm")]
}

fn default_out() -> String {
    "out".to_string()
}

fn default_fab() -> String {
    "fab".to_string()
}

fn default_kicad() -> i64 {
    KICAD_VERSION
}

/// The whole file: the `[project]` table and the `[order.*]` tables, and no other.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    project: Manifest,
    #[serde(default)]
    order: RawOrders,
    #[serde(default)]
    supply: Option<SupplyConfig>,
}

/// [`File`] as it is written back.
#[derive(Serialize)]
struct Rendered<'a> {
    project: &'a Manifest,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    order: BTreeMap<String, RenderedOrder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    supply: &'a Option<SupplyConfig>,
}

impl Manifest {
    pub fn new(name: &str) -> Manifest {
        Manifest {
            name: name.to_string(),
            top_level: default_top_level(),
            out: default_out(),
            fab: default_fab(),
            kicad: default_kicad(),
            catalogs: Vec::new(),
            assembly: None,
            guidelines: None,
            constraints: None,
            series: None,
            orders: Vec::new(),
            supply: None,
        }
    }

    /// The text of the file, the same fields `parse` reads.
    pub fn render(&self) -> String {
        toml::to_string(&Rendered {
            project: self,
            order: order::rendered(&self.orders),
            supply: &self.supply,
        })
        .expect("a manifest of strings and a number is always TOML")
    }

    /// Read manifest text. TOML mistakes are reported by the `toml` crate with the line,
    /// column and key; values that cannot work are named by field.
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let file: File = toml::from_str(text).map_err(|e| format!("{MANIFEST}: {e}"))?;
        let mut manifest = file.project;
        manifest.supply = file.supply;
        manifest.validate()?;
        manifest.orders = order::check(text, file.order)?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), String> {
        if !is_project_name(&self.name) {
            return Err(format!(
                "{MANIFEST}: name {:?} must be letters, digits, `-` and `_`, and not empty",
                self.name
            ));
        }
        if self.kicad != KICAD_VERSION {
            return Err(format!(
                "{MANIFEST}: kicad = {}, but netsmith writes KiCad {KICAD_VERSION} files only",
                self.kicad
            ));
        }
        if let Some(word) = &self.assembly
            && !["fab", "hand", "none"].contains(&word.as_str())
        {
            return Err(format!(
                "{MANIFEST}: assembly = {word:?} is not one of \"fab\", \"hand\" or \"none\""
            ));
        }
        if let Some(word) = &self.guidelines
            && !["warn", "error"].contains(&word.as_str())
        {
            return Err(format!(
                "{MANIFEST}: guidelines = {word:?} is not one of \"warn\" or \"error\""
            ));
        }
        if let Some(word) = &self.constraints
            && !["warn", "error"].contains(&word.as_str())
        {
            return Err(format!(
                "{MANIFEST}: constraints = {word:?} is not one of \"warn\" or \"error\""
            ));
        }
        if let Some(word) = &self.series
            && word != "none"
            && Series::parse(word).is_none()
        {
            return Err(format!(
                "{MANIFEST}: series = {word:?} is not one of \"E12\", \"E24\", \"E96\" or \"none\""
            ));
        }
        if let Some(supply) = &self.supply {
            supply.validate()?;
        }
        for file in &self.top_level {
            check_relative("top_level", file)?;
            if !file.ends_with(".nsm") {
                return Err(format!(
                    "{MANIFEST}: top_level file {file:?} must end in .nsm"
                ));
            }
        }
        let mut names: Vec<&str> = Vec::new();
        for file in &self.catalogs {
            check_relative("catalogs", file)?;
            let name = catalog_name(file).ok_or_else(|| {
                format!(
                    "{MANIFEST}: catalogs file {file:?} must be named after its catalog and end in .csv (`catalogs/ldo.csv` is the catalog `ldo`)"
                )
            })?;
            if names.contains(&name) {
                return Err(format!(
                    "{MANIFEST}: catalogs lists two files for the catalog `{name}`; a catalog is one file"
                ));
            }
            names.push(name);
        }
        check_relative("out", &self.out)?;
        check_relative("fab", &self.fab)?;
        if Path::new(&self.out) == Path::new(&self.fab) {
            return Err(format!(
                "{MANIFEST}: out and fab are both {:?}; they must differ",
                self.out
            ));
        }
        Ok(())
    }
}

/// The catalog a file holds: its name without `.csv`, which must be a name a `catalog`
/// declaration can have.
fn catalog_name(file: &str) -> Option<&str> {
    let stem = Path::new(file)
        .file_name()?
        .to_str()?
        .strip_suffix(".csv")?;
    is_identifier(stem).then_some(stem)
}

/// A project name is also what `init` takes from a directory name.
pub fn is_project_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// A path in the manifest stays inside the project: relative, and no `..`.
/// The E-series of generic resistors a manifest's `series` asks for: E24 unless it says
/// another or `"none"` (the manifest was validated).
fn series_of(word: Option<&str>) -> Option<Series> {
    match word {
        None => Some(Series::E24),
        Some("none") => None,
        Some(word) => Series::parse(word),
    }
}

fn check_relative(key: &str, path: &str) -> Result<(), String> {
    let inside = !path.is_empty()
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if inside {
        Ok(())
    } else {
        Err(format!(
            "{MANIFEST}: {key} {path:?} must be a path inside the project (relative, no `..`)"
        ))
    }
}

/// A manifest and the directory it was found in.
#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub manifest: Manifest,
}

impl Project {
    /// The project that contains `start`, searching it and then each parent directory for
    /// a `netsmith.toml`. `None` if there is none.
    pub fn find(start: &Path) -> Result<Option<Project>, String> {
        let start = std::path::absolute(start).map_err(|e| format!("{}: {e}", start.display()))?;
        for dir in start.ancestors() {
            if dir.join(MANIFEST).is_file() {
                return Project::load(dir).map(Some);
            }
        }
        Ok(None)
    }

    pub fn load(root: &Path) -> Result<Project, String> {
        let path = root.join(MANIFEST);
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let manifest = Manifest::parse(&text).map_err(|e| format!("{}: {e}", root.display()))?;
        Ok(Project {
            root: root.to_path_buf(),
            manifest,
        })
    }

    /// The directory `netsmith build` writes to.
    pub fn out_dir(&self) -> PathBuf {
        self.root.join(&self.manifest.out)
    }

    /// The directory `netsmith fab` writes to.
    pub fn fab_dir(&self) -> PathBuf {
        self.root.join(&self.manifest.fab)
    }

    /// Every `.nsm` file under `src/`, sorted, with the module it is: `src/power.nsm` is
    /// `src::power`. A name that cannot be written in an import is refused.
    pub fn modules(&self) -> Result<Vec<Module>, String> {
        let mut found = Vec::new();
        let dir = self.root.join(SOURCE_DIR);
        if dir.is_dir() {
            collect(&dir, &[SOURCE_DIR.to_string()], &mut found)?;
        }
        found.sort_by(|a: &Module, b| a.path.cmp(&b.path));
        Ok(found)
    }

    /// The standard JLCPCB catalogs and the catalog files the manifest lists, read again on
    /// each call. A problem names the file and line.
    pub fn catalogs(&self) -> Result<Catalogs, String> {
        let mut catalogs = Catalogs::jlcpcb()
            .with_default_assembly(self.manifest.assembly.as_deref())
            .with_strict_guidelines(self.manifest.guidelines.as_deref() == Some("error"))
            .with_constraints_warn(self.manifest.constraints.as_deref() == Some("warn"))
            .with_series(series_of(self.manifest.series.as_deref()));
        for file in &self.manifest.catalogs {
            let path = self.root.join(file);
            let text = fs::read_to_string(&path).map_err(|e| format!("{MANIFEST}: {file}: {e}"))?;
            let name = catalog_name(file).expect("the manifest was validated");
            catalogs.add_file(name, file, &text)?;
        }
        if self.manifest.supply.is_some() {
            catalogs = catalogs.with_availability(self.supply()?.into_availability());
        }
        Ok(catalogs)
    }

    /// The providers of the `[supply]` table, with the cache in front of them, read again on
    /// each call. Without the table no provider is configured, and every price and stock is
    /// unknown (ADR 0047).
    pub fn supply(&self) -> Result<Supply, String> {
        let Some(config) = &self.manifest.supply else {
            return Ok(Supply::none());
        };
        let file = config.file.as_deref().expect("the manifest was validated");
        let provider = FileProvider::open(&self.root.join(file), file)?;
        let mut supply = Supply::none().with_provider(Box::new(provider));
        if let Some(word) = &config.channel {
            supply = supply.with_channel(Channel::parse(word).expect("the manifest was validated"));
        }
        if let Some(hours) = config.cache_hours {
            supply = supply.with_cache_hours(hours);
        }
        if let Some(dir) = Cache::default_dir() {
            supply = supply.with_cache_dir(dir);
        }
        Ok(supply)
    }

    /// The standard library and this project's modules.
    pub fn loader(&self) -> Result<Loader, String> {
        self.loader_with(&HashMap::new())
    }

    /// [`Project::loader`], reading a module's text from `unsaved` instead of from disk
    /// where an editor holds a newer copy.
    pub fn loader_with(&self, unsaved: &HashMap<PathBuf, String>) -> Result<Loader, String> {
        let mut loader = Loader::std();
        for m in self.modules()? {
            let text = match unsaved.get(&m.path) {
                Some(text) => text.clone(),
                None => {
                    fs::read_to_string(&m.path).map_err(|e| format!("{}: {e}", m.path.display()))?
                }
            };
            let name = m.name(&self.root);
            loader.insert(&m.key, &name, &text);
            crate::physical::attach(&mut loader, &name, &m.path, unsaved)?;
        }
        Ok(loader)
    }
}

/// One importable file of a project.
#[derive(Debug, Clone)]
pub struct Module {
    /// `src::power`.
    pub key: String,
    pub path: PathBuf,
}

impl Module {
    /// The path as diagnostics show it: relative to the project root, with `/`.
    pub fn name(&self, root: &Path) -> String {
        let relative = self.path.strip_prefix(root).unwrap_or(&self.path);
        relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    }
}

pub(crate) fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn collect(dir: &Path, prefix: &[String], out: &mut Vec<Module>) -> Result<(), String> {
    let io = |e: io::Error| format!("{}: {e}", dir.display());
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(io)?
        .collect::<io::Result<_>>()
        .map_err(io)?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let kind = entry.file_type().map_err(io)?;
        if kind.is_dir() {
            if !is_identifier(&file_name) {
                return Err(not_a_module(&path, "directory"));
            }
            let mut inner = prefix.to_vec();
            inner.push(file_name);
            collect(&path, &inner, out)?;
        } else if kind.is_file()
            && !crate::physical::is_physical_file(&file_name)
            && let Some(stem) = file_name.strip_suffix(".nsm")
        {
            if !is_identifier(stem) {
                return Err(not_a_module(&path, "file"));
            }
            let mut key = prefix.to_vec();
            key.push(stem.to_string());
            out.push(Module {
                key: key.join("::"),
                path,
            });
        }
    }
    Ok(())
}

fn not_a_module(path: &Path, what: &str) -> String {
    format!(
        "{}: a {what} name under {SOURCE_DIR}/ is a module name and must be letters, digits and `_`, not starting with a digit",
        path.display()
    )
}

/// One source file to act on.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// What diagnostics call it.
    pub name: String,
    pub path: PathBuf,
}

/// What a command acts on: the files, the project they belong to if any, and where output
/// goes by default.
#[derive(Debug, Clone)]
pub struct Target {
    pub project: Option<Project>,
    pub files: Vec<SourceFile>,
    /// `netsmith build`'s output directory when `--out` is not given.
    pub build_out: PathBuf,
    /// `netsmith fab`'s output directory when `--out` is not given.
    pub fab_out: PathBuf,
    /// What the editor's page is titled: the project's name, or the file's.
    pub title: String,
}

impl Target {
    /// The target of a command line from the current directory: the named file, or, with
    /// no file, the top-level files of the project the current directory is in.
    pub fn resolve(file: Option<&str>) -> Result<Target, String> {
        let cwd = env::current_dir().map_err(|e| format!("current directory: {e}"))?;
        Target::resolve_from(&cwd, file)
    }

    pub fn resolve_from(cwd: &Path, file: Option<&str>) -> Result<Target, String> {
        match file {
            Some(file) => {
                let path = cwd.join(file);
                let project = Project::find(path.parent().unwrap_or(cwd))?;
                // A named file keeps today's default output directories, even inside a
                // project: only the imports of the project's modules are added.
                Ok(Target {
                    project,
                    files: vec![SourceFile {
                        name: file.to_string(),
                        path: PathBuf::from(file),
                    }],
                    build_out: PathBuf::from("out"),
                    fab_out: PathBuf::from("fab"),
                    title: file.to_string(),
                })
            }
            None => {
                let project = Project::find(cwd)?.ok_or_else(|| {
                    format!(
                        "no {MANIFEST} in {} or any directory above it: name a .nsm file, or run `netsmith init` to start a project",
                        cwd.display()
                    )
                })?;
                let mut files = Vec::new();
                for name in &project.manifest.top_level {
                    let path = project.root.join(name);
                    if !path.is_file() {
                        return Err(format!(
                            "{MANIFEST} lists {name} in top_level, but {} is not a file",
                            path.display()
                        ));
                    }
                    files.push(SourceFile {
                        name: name.clone(),
                        path,
                    });
                }
                Ok(Target {
                    title: project.manifest.name.clone(),
                    build_out: project.out_dir(),
                    fab_out: project.fab_dir(),
                    files,
                    project: Some(project),
                })
            }
        }
    }

    /// What `import` can reach: the standard library, and the project's `src/` if the
    /// files belong to a project. Read again on each call, so a long-running `editor` sees
    /// new files.
    pub fn loader(&self) -> Result<Loader, String> {
        self.loader_with(&HashMap::new())
    }

    /// [`Target::loader`] with the text of modules an editor has changed and not saved.
    pub fn loader_with(&self, unsaved: &HashMap<PathBuf, String>) -> Result<Loader, String> {
        let mut loader = match &self.project {
            Some(project) => project.loader_with(unsaved)?,
            None => Loader::std(),
        };
        // A top-level file may stand outside `src/`: it has its physical file all the same.
        for file in &self.files {
            crate::physical::attach(&mut loader, &file.name, &file.path, unsaved)?;
        }
        Ok(loader)
    }

    /// The catalogs a build picks parts from: the standard ones, and the project's own if
    /// the files belong to a project. Read again on each call, like [`Self::loader`].
    pub fn catalogs(&self) -> Result<Catalogs, String> {
        match &self.project {
            Some(project) => project.catalogs(),
            None => Ok(Catalogs::jlcpcb().with_series(series_of(None))),
        }
    }

    /// The files a change to which should build again: the files, the manifest, the
    /// project's modules and its catalog files.
    pub fn watched(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self.files.iter().map(|f| f.path.clone()).collect();
        // The physical file of a source, if there is one, builds the source again too.
        paths.extend(
            self.files
                .iter()
                .map(|f| crate::physical::physical_path(&f.path))
                .filter(|p| p.is_file()),
        );
        // The schematic layout file of a source (ADR 0061) changes the drawing, which is made
        // by the analysis.
        paths.extend(
            self.files
                .iter()
                .map(|f| netsmith_view::schematic_hints_path(&f.path))
                .filter(|p| p.is_file()),
        );
        if let Some(project) = &self.project {
            paths.push(project.root.join(MANIFEST));
            let mapping = project.root.join(netsmith_kicad::PROJECT_FILE);
            if mapping.is_file() {
                paths.push(mapping);
            }
            paths.extend(
                project
                    .manifest
                    .catalogs
                    .iter()
                    .map(|c| project.root.join(c)),
            );
            // A name that is no module is reported by `loader`, not here.
            for module in project.modules().unwrap_or_default() {
                let physical = crate::physical::physical_path(&module.path);
                paths.push(module.path);
                if physical.is_file() {
                    paths.push(physical);
                }
            }
        }
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Manifest, String> {
        Manifest::parse(text)
    }

    #[test]
    fn a_manifest_renders_and_reads_back() {
        let m = Manifest::new("blinky");
        assert_eq!(parse(&m.render()), Ok(m));
    }

    #[test]
    fn only_the_name_is_required() {
        let m = parse("[project]\nname = \"a\"\n").unwrap();
        assert_eq!(m, Manifest::new("a"));
    }

    #[test]
    fn comments_blank_lines_and_multiline_lists_are_read() {
        let m = parse(
            "# my board\n\n[project]  # table\nname = \"a\" # the name\ntop_level = [\n  \"src/a.nsm\", # one\n  \"src/b.nsm\",\n]\nout = \"build\"\nkicad = 9\n",
        )
        .unwrap();
        assert_eq!(m.top_level, ["src/a.nsm", "src/b.nsm"]);
        assert_eq!(m.out, "build");
    }

    #[test]
    fn a_mistake_names_the_line_and_the_key() {
        let e = parse("[project]\nname = \"a\"\nkicad = \"9\"\n").unwrap_err();
        assert!(
            e.starts_with("netsmith.toml:") && e.contains("line 3") && e.contains("kicad"),
            "{e}"
        );
        let e = parse("[project]\nname = \"a\"\ncolour = \"red\"\n").unwrap_err();
        assert!(
            e.contains("line 3") && e.contains("unknown field `colour`") && e.contains("kicad"),
            "{e}"
        );
        let e = parse("[project]\nname = \"a\"\nname = \"b\"\n").unwrap_err();
        assert!(e.contains("line 3") && e.contains("duplicate key"), "{e}");
        let e = parse("name = \"a\"\n").unwrap_err();
        assert!(
            e.contains("line 1") && e.contains("unknown field `name`"),
            "{e}"
        );
        let e = parse("[project]\nname = \"a\n").unwrap_err();
        assert!(e.contains("line 2"), "{e}");
        let e = parse("[project]\nname = a\n").unwrap_err();
        assert!(e.contains("line 2"), "{e}");
        let e = parse("[project]\nname = \"a\"\n[deps]\n").unwrap_err();
        assert!(
            e.contains("line 3") && e.contains("unknown field `deps`"),
            "{e}"
        );
        assert!(parse("").unwrap_err().contains("missing field `project`"));
        assert!(
            parse("[project]\n")
                .unwrap_err()
                .contains("missing field `name`")
        );
    }

    #[test]
    fn an_empty_top_level_is_a_project_of_modules_only_and_survives_a_round_trip() {
        let m = parse("[project]\nname = \"a\"\ntop_level = []\n").unwrap();
        assert!(m.top_level.is_empty());
        assert_eq!(Manifest::parse(&m.render()).unwrap(), m);
        // Leaving the key out is not the same: that is the default `src/main.nsm`.
        assert_eq!(
            parse("[project]\nname = \"a\"\n").unwrap().top_level,
            ["src/main.nsm"]
        );
    }

    #[test]
    fn the_default_assembly_is_fab_hand_or_none() {
        let with = |word: &str| parse(&format!("[project]\nname = \"a\"\nassembly = \"{word}\"\n"));
        for word in ["fab", "hand", "none"] {
            let m = with(word).unwrap();
            assert_eq!(m.assembly.as_deref(), Some(word));
            assert_eq!(parse(&m.render()), Ok(m));
        }
        let e = with("solder").unwrap_err();
        assert!(
            e.contains("assembly = \"solder\"") && e.contains("\"fab\", \"hand\" or \"none\""),
            "{e}"
        );
        assert_eq!(parse("[project]\nname = \"a\"\n").unwrap().assembly, None);
        // Without the key the manifest writes none, so `init` leaves today's rules alone.
        assert!(!Manifest::new("a").render().contains("assembly"));
    }

    #[test]
    fn values_that_cannot_work_are_refused_by_field() {
        let with = |line: &str| parse(&format!("[project]\nname = \"a\"\n{line}\n")).unwrap_err();
        let e = with("kicad = 10");
        assert!(e.contains("kicad = 10") && e.contains("KiCad 9"), "{e}");
        let e = with("top_level = [\"../x.nsm\"]");
        assert!(
            e.contains("top_level") && e.contains("../x.nsm") && e.contains("inside"),
            "{e}"
        );
        let e = with("top_level = [\"/etc/x.nsm\"]");
        assert!(e.contains("/etc/x.nsm"), "{e}");
        let e = with("top_level = [\"src/main.txt\"]");
        assert!(e.contains("must end in .nsm"), "{e}");
        let e = with("out = \"fab\"");
        assert!(e.contains("out and fab are both \"fab\""), "{e}");
        let e = with("fab = \"../fab\"");
        assert!(e.contains("fab \"../fab\""), "{e}");
        let e = parse("[project]\nname = \"two words\"\n").unwrap_err();
        assert!(e.contains("\"two words\""), "{e}");
    }
}
