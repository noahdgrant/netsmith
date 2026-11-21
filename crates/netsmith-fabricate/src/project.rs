//! A netsmith project: the `netsmith.toml` manifest, finding it from any directory below
//! the project root, and the files it names (ADR 0011).
//!
//! The manifest is read with the `toml` crate and holds one `[project]` table whose values
//! are strings, whole numbers and lists of strings, and the `[order.*]` tables of ADR 0035
//! (`crate::order`), `[supply]`, the model table (`[models]`, `[[model]]`, `crate::models`) and
//! `[dependencies]`, the other projects whose modules and catalogs this one reads (issue #68).
//! Any other table or key is refused with its line and column, so the
//! file stays plain TOML.

use std::{
    collections::{BTreeMap, HashMap},
    env, fs, io,
    path::{Component, Path, PathBuf},
};

use netsmith_check::Loader;
use netsmith_solve::{Catalogs, Preferred, Series};
use serde::{Deserialize, Serialize};

use crate::order::{self, Order, RawOrders, RenderedOrder};
use netsmith_pipeline::supply::{Cache, Channel, FileProvider, Supply};

/// The manifest's file name.
pub const MANIFEST: &str = "netsmith.toml";

/// The project's preferred-parts file, beside the manifest (issue #802, ADR 0047): the parts
/// that resolution takes first for a generic part. See [`Project::preferred`].
pub const PREFERRED: &str = "preferred.csv";

/// The directory below the project root whose `.nsm` files other files can import:
/// `src/power.nsm` is the module `src::power`.
pub const SOURCE_DIR: &str = "src";

/// The one KiCad version netsmith writes files for.
pub const KICAD_VERSION: i64 = 9;

/// The most seeds `place_seeds` may ask the placer to try for a board (owner, issue #458): each
/// is a whole placement, so a board no seed places costs this many times as long.
pub const MAX_PLACE_SEEDS: u32 = 32;

/// The most characters of a project's `revision` (issue #1134): room for a letter, a version or
/// a short tag, and a line of the title block.
pub const MAX_REVISION: usize = 32;

/// What `netsmith.toml` says, the `[project]` table. Only `name` is required; the rest
/// are what `netsmith project init` writes out. A key that is not one of these is refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    /// The netsmith release the project's source is written for, `netsmith = "0.2.0"`, which
    /// `netsmith project init` writes. A command whose check fails on a project written for another
    /// release says so and points to `docs/MIGRATING.md` (`crate::release`, ADR 0134). Without
    /// it the project was made before the key, with netsmith 0.1.0 or earlier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub netsmith: Option<String>,
    /// The revision of the project's boards, `revision = "A"` or `"1.2"`: text on one line, up
    /// to [`MAX_REVISION`] characters (issue #1134). The title block of the schematic PDF shows
    /// `Rev A` beside the sheet's name, and `fab-notes.txt` and an order's `order.txt` begin
    /// with it. One revision for the whole project, not one for each board; file names do not
    /// carry it. `netsmith project init` does not write it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// The files that hold boards, relative to the project root. `netsmith board build`, `fab`
    /// and `editor` with no file act on these. A `system` is listed here the same way. A
    /// project that is only a library of modules lists none (`top_level = []`).
    #[serde(default = "default_top_level")]
    pub top_level: Vec<String>,
    /// Where `netsmith board build` writes, relative to the project root.
    #[serde(default = "default_out")]
    pub out: String,
    /// Where `netsmith board fab` writes, relative to the project root.
    #[serde(default = "default_fab")]
    pub fab: String,
    /// The KiCad version the output is for.
    #[serde(default = "default_kicad")]
    pub kicad: i64,
    /// The folders whose `*.csv` files are the project's own catalogs, relative to the
    /// project root (`..` is allowed, so projects can share a folder): each file is the
    /// catalog its name says (`catalogs/acme_ldo.csv` is the catalog `acme_ldo`). The
    /// default is `["catalogs"]`; `[]` reads no folder. Sub-folders are not read, files
    /// are read in file-name order (ADR 0022, amended).
    #[serde(default = "default_catalog_dirs")]
    pub catalog_dirs: Vec<String>,
    /// Single catalog files from anywhere inside the project, relative to the project
    /// root, one per catalog and named by it, in addition to the files of `catalog_dirs`
    /// and the standard JLCPCB catalogs (ADR 0022).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub catalogs: Vec<String>,
    /// The folders whose `*.csv` files are the project's offers (ADR 0143), relative to the
    /// project root (`..` is allowed): each file is named for its supplier (`offers/lcsc.csv`
    /// is LCSC's), and its offers come after the standard ones. The default is `["offers"]`,
    /// which is not written out; `[]` reads no folder. Sub-folders are not read, and a folder
    /// may not also be a `catalog_dirs` folder.
    #[serde(
        default = "default_offer_dirs",
        skip_serializing_if = "is_default_offer_dirs"
    )]
    pub offer_dirs: Vec<String>,
    /// Who assembles a part that nothing else speaks for: `"fab"`, `"hand"`, `"none"` or `"consigned"`
    /// (ADR 0028, ADR 0084). Without it a part is the board house's if it is surface mount and
    /// soldered by hand if it is through hole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<String>,
    /// The environment every board of the project has when it declares none (ADR 0099): the
    /// definition of an interface such as `std::env::Environment`, as `module::Name`, where the
    /// module is a standard one (`std::env`), a file of the project (`src/vehicle.nsm`) or a module
    /// of a dependency by its import path (`shared::env`, issue #1012). A
    /// board that declares its own `.environment` has that one alone. Without the key a board
    /// that declares none has no environment, and nothing is judged against one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
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
    /// What `fab` may do with the pins that pin requests got when a board does not route
    /// (ADR 0101): `"auto"`, the default, swaps the unlocked ones and writes the pin file;
    /// `"locked"` moves none and writes the proposal to `pins.proposed.toml`. A run can ask for
    /// either with `--pins`. A team sets `"locked"` once firmware is built on the pin map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pins: Option<String>,
    /// Which source files get a pick lock (ADR 0100): `"all"`, the default, every file a command
    /// reads; `"top_level"` only the files of `top_level`, so a unit test of one part, or a
    /// scratch file, has none; `"none"` no file. A source with no lock is not read for one,
    /// written one, or held to one, and `project lock --update` and `project lock --check` skip it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock: Option<String>,
    /// Whether `fab` writes the board's 3D scene as `<board>.glb` into the fab package
    /// (`glb = false` leaves it out; the default, `true`, writes it). A run can leave it out
    /// with `--no-glb`, which wins over this key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glb: Option<bool>,
    /// Whether `fab` writes the board as STEP, `<board>.step`, into the fab package
    /// (`step = false` leaves it out; the default, `true`, writes it). A run can leave it out
    /// with `--no-step`, which wins over this key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<bool>,
    /// Whether `fab` writes the board's schematic, `<board>-schematic.pdf`, into the fab
    /// package (`pdf = false` leaves it out; the default, `true`, writes it). A run can leave it
    /// out with `--no-pdf`, which wins over this key (issue #1091).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf: Option<bool>,
    /// How many times the process's least track a signal is drawn at, where there is room
    /// (`track_margin = 1.5`, the default; 1 is the least). A board the router cannot finish
    /// at that width is routed at the least (issue #431).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_margin: Option<f64>,
    /// The rounds of rip-up and re-route of whole nets the router gives a try that leaves
    /// connections (`rip_rounds = 6`, the default; `0` runs none, issue #1088). `--rip-rounds`
    /// wins over it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rip_rounds: Option<usize>,
    /// How many seeds the placer tries for a board before it gives up (`place_seeds = 8`, the
    /// default; 1 tries only the layout file's, and [`MAX_PLACE_SEEDS`] is the most): the layout
    /// file's seed, or 0, then the ones after it. The seed that places the board is written to the layout file (issue #458).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place_seeds: Option<u32>,
    /// How long the placer may look for a placement of a board before it gives the closest try
    /// (`place_budget = "120s"`, or `"5tries"`, or `"120s+5tries"`; none by default, issue
    /// #764): `fab` and `dev board3d` say what was out when it ran out. `--place-budget` wins over
    /// it. [`crate::layout::parse_place_budget`] reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place_budget: Option<String>,
    /// The E-series the solver may pick generic resistors from when no catalog row works:
    /// `"E12"`, `"E24"` (the default) or `"E96"` (ADR 0047). `"none"` offers
    /// only the catalog's rows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    /// The `[order.<name>.boards]` tables (ADR 0035): which boards go into one fabrication
    /// order, and how many of each. They are not part of the `[project]` table; they are
    /// read and written beside it.
    #[serde(skip)]
    pub orders: Vec<Order>,
    /// The `[supply]` table (ADR 0047): where price and stock come from. Read
    /// and written beside `[project]`. It has no field for a key or a password.
    #[serde(skip)]
    pub supply: Option<SupplyConfig>,
    /// The `[models]` table (issue #763): the folder of STEP models a part's footprint id
    /// finds its model in, as KiCad lays its library out. Read and written beside `[project]`.
    #[serde(skip)]
    pub models: Option<crate::models::ModelsConfig>,
    /// The `[[model]]` rows (issue #763): the STEP model of each footprint id, the default of a
    /// part's `.model` that the part's own statement overrides. Read and written beside
    /// `[project]`.
    #[serde(skip)]
    pub model: Vec<crate::models::ModelRow>,
    /// The `[dependencies]` table (issue #68): the other projects this one reads, each under the
    /// name its key gives, which is the first step of an import of its modules (`shared =
    /// { path = "../shared" }` makes `../shared/src/power.nsm` the module `shared::power`). Read
    /// and written beside `[project]`.
    #[serde(skip)]
    pub dependencies: BTreeMap<String, Dependency>,
    /// The `[lint]` table (issue #1104): the rules of `netsmith project lint` turned off for the
    /// project, and whether its findings are errors. Read and written beside `[project]`.
    #[serde(skip)]
    pub lint: Option<LintConfig>,
}

/// The `[lint]` table of `netsmith.toml` (issue #1104). `netsmith project lint` reads it, and
/// says so when `off` names a rule it does not have.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LintConfig {
    /// The rules turned off for the whole project, by name (`off = ["naming"]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub off: Vec<String>,
    /// What a finding is: `"warn"`, the default, which `project lint` lists and passes, or
    /// `"error"`, which fails it. A run asks for the second with `--strict`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub findings: Option<String>,
}

/// One entry of `[dependencies]`: where another project is. A folder (`path`), or a git
/// repository (`git`) at a `rev` (a branch, a tag or a commit) or at the newest tag a
/// `version` requirement admits (issue #1012, ADR 0137).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Dependency {
    /// The project's folder, the one that holds its `netsmith.toml`, relative to this project's
    /// root (`../shared`); `..` is allowed, an absolute path is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The URL of a git repository whose top folder is the project: `https://...`,
    /// `ssh://...`, `git@host:path`, `file:///...` or an absolute path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git: Option<String>,
    /// With `git`: the branch, tag or commit to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    /// With `git`: a version requirement in cargo's syntax (`"1.2"` admits 1.2.0 up to, not
    /// including, 2.0.0); the newest tag `vX.Y.Z` or `X.Y.Z` it admits is read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl Dependency {
    fn validate(&self, name: &str) -> Result<(), String> {
        if !is_identifier(name) || [SOURCE_DIR, "std"].contains(&name) {
            return Err(format!(
                "{MANIFEST}: [dependencies] {name:?} is not a name an import can start with: letters, digits and `_`, not starting with a digit, and not `src` or `std`"
            ));
        }
        let forms = format!(
            "`{name} = {{ path = \"../{name}\" }}`, `{name} = {{ git = \"https://example.com/{name}.git\", rev = \"main\" }}` or `{name} = {{ git = \"...\", version = \"1.2\" }}`"
        );
        match (&self.path, &self.git) {
            (Some(_), Some(_)) => Err(format!(
                "{MANIFEST}: [dependencies] {name} has both `path` and `git`; a dependency is in one place: {forms}"
            )),
            (Some(path), None) => {
                if self.rev.is_some() || self.version.is_some() {
                    return Err(format!(
                        "{MANIFEST}: [dependencies] {name}: `rev` and `version` choose a commit of a `git` repository; a `path` dependency is read as the folder is"
                    ));
                }
                check_folder(&format!("[dependencies] {name} path"), path)
            }
            (None, Some(git)) => {
                check_git_url(name, git)?;
                match (&self.rev, &self.version) {
                    (Some(_), Some(_)) => Err(format!(
                        "{MANIFEST}: [dependencies] {name} has both `rev` and `version`; give one"
                    )),
                    (None, None) => Err(format!(
                        "{MANIFEST}: [dependencies] {name} needs `rev` (a branch, tag or commit) or `version` with its `git`: {forms}"
                    )),
                    (Some(rev), None) => {
                        if rev.is_empty()
                            || rev.starts_with('-')
                            || rev.chars().any(char::is_control)
                        {
                            return Err(format!(
                                "{MANIFEST}: [dependencies] {name}: rev = {rev:?} is not a branch, tag or commit"
                            ));
                        }
                        Ok(())
                    }
                    (None, Some(version)) => self.version_req(name, version).map(|_| ()),
                }
            }
            (None, None) => Err(format!(
                "{MANIFEST}: [dependencies] {name} needs `path` or `git`: {forms}; there is no registry, so a `version` names tags of a `git` repository"
            )),
        }
    }

    fn version_req(&self, name: &str, version: &str) -> Result<semver::VersionReq, String> {
        semver::VersionReq::parse(version).map_err(|e| {
            format!(
                "{MANIFEST}: [dependencies] {name}: version = {version:?} is not a version requirement such as \"1.2\" or \"=1.2.3\": {e}"
            )
        })
    }

    /// What a `git` entry asks for, with no commit yet: `None` for a `path` entry.
    fn request(&self, name: &str) -> Option<crate::dependency_lock::Locked> {
        Some(crate::dependency_lock::Locked {
            name: name.to_string(),
            git: self.git.clone()?,
            rev: self.rev.clone(),
            version: self.version.clone(),
            commit: String::new(),
        })
    }
}

/// A `git` value that git reads as a place: a URL with a scheme, `user@host:path`, or an
/// absolute path. A relative path would be read from wherever netsmith runs, and a value that
/// starts with `-` as an option.
fn check_git_url(name: &str, git: &str) -> Result<(), String> {
    let scp = git
        .split_once(':')
        .is_some_and(|(host, _)| host.contains('@'));
    let ok = !git.starts_with('-')
        && !git.chars().any(char::is_control)
        && (git.contains("://") || scp || Path::new(git).is_absolute());
    if ok {
        Ok(())
    } else {
        Err(format!(
            "{MANIFEST}: [dependencies] {name}: git = {git:?} is not a repository URL (`https://...`, `ssh://...`, `git@host:path`, `file:///...`) or an absolute path; a folder on this machine is a `path` dependency"
        ))
    }
}

/// The `[supply]` table of `netsmith.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyConfig {
    /// The project's supply channels, in order (ADR 0143): `["jlcpcb-assembly", "lcsc-retail"]`
    /// when the table names none. The first with an offer of a part buys it, and its rule
    /// orders the rows that fit a generic part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<String>>,
    /// One channel, the list of one that `channels` would be; the spelling of a manifest from
    /// before the list. Not with `channels`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// `"file"`: prices and stock from a CSV file (`netsmith_pipeline::supply::file`). Providers that
    /// ask a supplier are not built. Without one, nothing is quoted: every price and stock is
    /// unknown, and the channels still say which offers the project buys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The file of the `file` provider, relative to the project root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// How many hours a cached answer is used without asking again (default 24).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_hours: Option<u64>,
    /// `[supply.nexar]`: the search provider of `netsmith catalog search` and `catalog add
    /// --from` (issue #1460), on only when the table is there and the variables it names are set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nexar: Option<NexarConfig>,
    /// `[supply.digikey]`: Digi-Key's own API as a second search provider (issue #1477), on only
    /// when the table is there and the variables it names are set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digikey: Option<DigiKeyConfig>,
    /// `[supply.parts]`: the parts that name their own supply channel (ADR 0143 item 4), each
    /// bought by that channel only. A part is named as its pick lock entry names it: under the
    /// table of its board or system, by its path (`[supply.parts.Board]` with
    /// `"ldo.regulator" = "digikey-retail"`), or by its path alone for a part of the project's
    /// only board (`"u3" = "digikey-retail"` in `[supply.parts]`).
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "parts_table"
    )]
    pub parts: Option<BTreeMap<String, PartsEntry>>,
}

/// One key of `[supply.parts]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum PartsEntry {
    /// `"u3" = "digikey-retail"`: a part of the project's only board, by its path, and the
    /// word of its channel.
    Part(String),
    /// `[supply.parts.Board]`: the parts of one board or system, each by its path there.
    Board(BTreeMap<String, String>),
}

/// `[supply.parts]` as TOML gives it: a string is a part of the only board, a table is a board.
/// A value of any other kind, and a table in a board's table (a path with a dot that is not
/// quoted), is refused with how to write it.
fn parts_table<'de, D>(d: D) -> Result<Option<BTreeMap<String, PartsEntry>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let table = toml::Table::deserialize(d)?;
    let channel = |at: &str, value: &toml::Value| match value {
        toml::Value::String(word) => Ok(word.clone()),
        toml::Value::Table(_) => Err(D::Error::custom(format!(
            "{at} holds a table, not a supply channel: a part's path with a dot in it is quoted, as in the pick lock (`\"ldo.regulator\" = \"digikey-retail\"`)"
        ))),
        other => Err(D::Error::custom(format!(
            "{at} = {other} is not the word of a supply channel: {}",
            Channel::words()
        ))),
    };
    let mut out = BTreeMap::new();
    for (key, value) in table {
        let entry = match value {
            toml::Value::Table(board) => {
                let mut parts = BTreeMap::new();
                for (path, value) in board {
                    let word = channel(&format!("[supply.parts.{key}] {path:?}"), &value)?;
                    parts.insert(path, word);
                }
                PartsEntry::Board(parts)
            }
            value => PartsEntry::Part(channel(&format!("[supply.parts] {key:?}"), &value)?),
        };
        out.insert(key, entry);
    }
    Ok(Some(out))
}

/// The `[supply.nexar]` table: the names of the environment variables that hold the client id and
/// the client secret of a Nexar application (ADR 0047 item 3: a credential is given by the name of
/// an environment variable, never by its value). Both are required, so nothing reads a variable
/// the project did not name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NexarConfig {
    /// The variable that holds the client id: `NEXAR_CLIENT_ID`.
    pub client_id_env: String,
    /// The variable that holds the client secret: `NEXAR_CLIENT_SECRET`.
    pub client_secret_env: String,
}

impl NexarConfig {
    /// A name that is not the name of an environment variable is refused: it may be the value
    /// itself, which must never be in the manifest.
    fn validate(&self) -> Result<(), String> {
        validate_env_names(
            "nexar",
            &[
                ("client_id_env", Some(&self.client_id_env)),
                ("client_secret_env", Some(&self.client_secret_env)),
            ],
            "make a new one at portal.nexar.com",
        )
    }
}

/// The `[supply.digikey]` table: the names of the environment variables that hold the client id
/// and the client secret of a Digi-Key application (two-legged OAuth), and, optionally, of the
/// account id Digi-Key's product details ask for. As for `[supply.nexar]`, the manifest holds the
/// names, never the values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DigiKeyConfig {
    /// The variable that holds the client id: `DIGIKEY_CLIENT_ID`.
    pub client_id_env: String,
    /// The variable that holds the client secret: `DIGIKEY_CLIENT_SECRET`.
    pub client_secret_env: String,
    /// The variable that holds the account id (`X-DIGIKEY-Account-Id`): with it, `catalog add
    /// --from digikey` asks Digi-Key's product details of the number; without, a keyword search.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id_env: Option<String>,
}

impl DigiKeyConfig {
    fn validate(&self) -> Result<(), String> {
        validate_env_names(
            "digikey",
            &[
                ("client_id_env", Some(&self.client_id_env)),
                ("client_secret_env", Some(&self.client_secret_env)),
                ("account_id_env", self.account_id_env.as_ref()),
            ],
            "make a new one at developer.digikey.com",
        )
    }
}

/// Each name of a provider's table is the name of an environment variable, and no two are the
/// same. A value written in a name's place is refused without being repeated, with how to replace
/// it (`renew`).
fn validate_env_names(
    table: &str,
    names: &[(&str, Option<&String>)],
    renew: &str,
) -> Result<(), String> {
    let given: Vec<(&str, &String)> = names
        .iter()
        .filter_map(|(key, name)| name.map(|n| (*key, n)))
        .collect();
    for (key, name) in &given {
        if !is_env_name(name) {
            return Err(format!(
                "{MANIFEST}: [supply.{table}] {key} is not the name of an environment variable (capital letters, digits and `_`, such as `{}_CLIENT_ID`); the manifest holds the variable's name and never its value, so if this is the value, remove it from the file and from the repository's history, and {renew}",
                table.to_ascii_uppercase()
            ));
        }
    }
    for (i, (key, name)) in given.iter().enumerate() {
        if let Some((other, _)) = given[..i].iter().find(|(_, n)| n == name) {
            return Err(format!(
                "{MANIFEST}: [supply.{table}] {other} and {key} name the same variable, {name:?}; they are two values"
            ));
        }
    }
    Ok(())
}

/// `NEXAR_CLIENT_ID`: a capital letter or `_`, then capital letters, digits and `_`, at most 64.
fn is_env_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && (bytes[0].is_ascii_uppercase() || bytes[0] == b'_')
        && bytes
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
}

impl SupplyConfig {
    fn validate(&self) -> Result<(), String> {
        if let Some(nexar) = &self.nexar {
            nexar.validate()?;
        }
        if let Some(digikey) = &self.digikey {
            digikey.validate()?;
        }
        match (self.provider.as_deref(), &self.file) {
            (Some("file"), None) => {
                return Err(format!(
                    "{MANIFEST}: [supply] provider = \"file\" needs `file = \"supply/quotes.csv\"`, the CSV of prices and stock"
                ));
            }
            (Some("file"), Some(file)) => check_relative("[supply] file", file)?,
            (Some(other), _) => {
                return Err(format!(
                    "{MANIFEST}: [supply] provider = {other:?} is not one netsmith has: only \"file\" is built (providers that ask a supplier need the supplier's terms of use read first, ADR 0047)"
                ));
            }
            (None, Some(file)) => {
                return Err(format!(
                    "{MANIFEST}: [supply] file = {file:?} is the file of a provider, and the table names none: write `provider = \"file\"` beside it"
                ));
            }
            (None, None) => {}
        }
        if let (Some(_), Some(_)) = (&self.channel, &self.channels) {
            return Err(format!(
                "{MANIFEST}: [supply] has both `channel` and `channels`; `channel = \"...\"` is a list of one: write the channels in one `channels = [...]`, in the order the project buys from them"
            ));
        }
        let words: Vec<&String> = self
            .channel
            .iter()
            .chain(self.channels.iter().flatten())
            .collect();
        if self.channels.as_ref().is_some_and(|c| c.is_empty()) {
            return Err(format!(
                "{MANIFEST}: [supply] channels = [] names no channel; leave the key out for the default, [\"jlcpcb-assembly\", \"lcsc-retail\"]"
            ));
        }
        for (i, word) in words.iter().enumerate() {
            let key = if self.channel.is_some() {
                "channel"
            } else {
                "channels"
            };
            if Channel::parse(word).is_none() {
                return Err(format!(
                    "{MANIFEST}: [supply] {key} names {word:?}, which is not a supply channel netsmith knows: {}",
                    Channel::words()
                ));
            }
            if words[..i].iter().any(|w| w.trim() == word.trim()) {
                return Err(format!(
                    "{MANIFEST}: [supply] channels lists {word:?} twice; a channel is listed once"
                ));
            }
        }
        for (key, value) in self.parts.iter().flatten() {
            let entries: Vec<(String, &String)> = match value {
                PartsEntry::Part(word) => vec![(format!("[supply.parts] {key:?}"), word)],
                PartsEntry::Board(parts) => parts
                    .iter()
                    .map(|(path, word)| (format!("[supply.parts.{key}] {path:?}"), word))
                    .collect(),
            };
            for (at, word) in entries {
                if Channel::parse(word).is_none() {
                    return Err(format!(
                        "{MANIFEST}: {at} = {word:?} is not a supply channel netsmith knows: {}",
                        Channel::words()
                    ));
                }
            }
        }
        Ok(())
    }

    /// The entries of `[supply.parts]` (ADR 0143 item 4), each with what a message calls it:
    /// `netsmith.toml: [supply.parts.Board] "u3"`. The words were checked by `validate`.
    pub fn part_channels(&self) -> Vec<netsmith_solve::PartChannel> {
        let entry = |unit: Option<&str>, path: &str, word: &str| {
            let table = unit.map_or("[supply.parts]".to_string(), |u| {
                format!("[supply.parts.{u}]")
            });
            netsmith_solve::PartChannel {
                unit: unit.map(str::to_string),
                path: path.to_string(),
                named: netsmith_solve::NamedChannel {
                    channel: Channel::parse(word).expect("the manifest was validated"),
                    key: format!("{MANIFEST}: {table} {path:?}"),
                },
            }
        };
        let mut out = Vec::new();
        for (key, value) in self.parts.iter().flatten() {
            match value {
                PartsEntry::Part(word) => out.push(entry(None, key, word)),
                PartsEntry::Board(parts) => {
                    for (path, word) in parts {
                        out.push(entry(Some(key), path, word));
                    }
                }
            }
        }
        out
    }

    /// The channels the table names, in order: `channels`, or `channel` as a list of one; empty
    /// when it names none, which is [`Channel::DEFAULT`].
    pub fn channel_list(&self) -> Vec<Channel> {
        self.channel
            .iter()
            .chain(self.channels.iter().flatten())
            .map(|w| Channel::parse(w).expect("the manifest was validated"))
            .collect()
    }
}

fn default_top_level() -> Vec<String> {
    vec![format!("{SOURCE_DIR}/main.nsm")]
}

/// The folder of a project's catalogs when the manifest says nothing.
pub const CATALOG_DIR: &str = "catalogs";

fn default_catalog_dirs() -> Vec<String> {
    vec![CATALOG_DIR.to_string()]
}

fn default_offer_dirs() -> Vec<String> {
    vec![netsmith_solve::offers::FOLDER.to_string()]
}

fn is_default_offer_dirs(dirs: &[String]) -> bool {
    dirs == default_offer_dirs().as_slice()
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

/// The whole file: the `[project]` table, the `[order.*]` tables, `[supply]`, `[models]` and the
/// `[[model]]` rows, and no other.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    project: Manifest,
    #[serde(default)]
    order: RawOrders,
    #[serde(default)]
    supply: Option<SupplyConfig>,
    #[serde(default)]
    models: Option<crate::models::ModelsConfig>,
    #[serde(default)]
    model: Vec<crate::models::ModelRow>,
    #[serde(default)]
    dependencies: BTreeMap<String, Dependency>,
    #[serde(default)]
    lint: Option<LintConfig>,
}

/// [`File`] as it is written back.
#[derive(Serialize)]
struct Rendered<'a> {
    project: &'a Manifest,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    order: BTreeMap<String, RenderedOrder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    supply: &'a Option<SupplyConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    models: &'a Option<crate::models::ModelsConfig>,
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    model: &'a [crate::models::ModelRow],
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    dependencies: &'a BTreeMap<String, Dependency>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lint: &'a Option<LintConfig>,
}

impl Manifest {
    pub fn new(name: &str) -> Manifest {
        Manifest {
            name: name.to_string(),
            netsmith: Some(crate::release::RELEASE.to_string()),
            revision: None,
            top_level: default_top_level(),
            out: default_out(),
            fab: default_fab(),
            kicad: default_kicad(),
            catalog_dirs: default_catalog_dirs(),
            catalogs: Vec::new(),
            offer_dirs: default_offer_dirs(),
            assembly: None,
            glb: None,
            step: None,
            pdf: None,
            track_margin: None,
            rip_rounds: None,
            place_seeds: None,
            place_budget: None,
            environment: None,
            guidelines: None,
            constraints: None,
            pins: None,
            lock: None,
            series: None,
            orders: Vec::new(),
            supply: None,
            models: None,
            model: Vec::new(),
            dependencies: BTreeMap::new(),
            lint: None,
        }
    }

    /// The text of the file, the same fields `parse` reads.
    pub fn render(&self) -> String {
        toml::to_string(&Rendered {
            project: self,
            order: order::rendered(&self.orders),
            supply: &self.supply,
            models: &self.models,
            model: &self.model,
            dependencies: &self.dependencies,
            lint: &self.lint,
        })
        .expect("a manifest of strings and a number is always TOML")
    }

    /// Read manifest text. TOML mistakes are reported by the `toml` crate with the line,
    /// column and key; values that cannot work are named by field.
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let file: File = toml::from_str(text).map_err(|e| format!("{MANIFEST}: {e}"))?;
        let mut manifest = file.project;
        manifest.supply = file.supply;
        manifest.models = file.models;
        manifest.model = file.model;
        manifest.dependencies = file.dependencies;
        manifest.lint = file.lint;
        manifest.validate()?;
        manifest.validate_revision(text)?;
        crate::models::check_table(manifest.models.as_ref(), &manifest.model)
            .map_err(|e| format!("{MANIFEST}: {e}"))?;
        manifest.orders = order::check(text, file.order)?;
        Ok(manifest)
    }

    /// `revision` is text on one line of 1 to [`MAX_REVISION`] characters; the error names the
    /// manifest line the key is on.
    fn validate_revision(&self, text: &str) -> Result<(), String> {
        let Some(revision) = &self.revision else {
            return Ok(());
        };
        let why = if revision.trim().is_empty() {
            Some("is empty".to_string())
        } else if revision.chars().any(char::is_control) {
            Some("must be on one line, with no line break or other control character".to_string())
        } else if revision.chars().count() > MAX_REVISION {
            Some(format!(
                "is {} characters; it may be at most {MAX_REVISION}",
                revision.chars().count()
            ))
        } else {
            None
        };
        match why {
            None => Ok(()),
            Some(why) => {
                let line = text
                    .lines()
                    .position(|l| {
                        l.trim_start()
                            .strip_prefix("revision")
                            .is_some_and(|rest| rest.trim_start().starts_with('='))
                    })
                    .map_or(String::new(), |i| format!(":{}", i + 1));
                Err(format!("{MANIFEST}{line}: revision {revision:?} {why}"))
            }
        }
    }

    fn validate(&self) -> Result<(), String> {
        if !is_project_name(&self.name) {
            return Err(format!(
                "{MANIFEST}: name {:?} must be letters, digits, `-` and `_`, and not empty",
                self.name
            ));
        }
        if let Some(release) = &self.netsmith
            && crate::release::parse(release).is_none()
        {
            return Err(format!(
                "{MANIFEST}: netsmith = {release:?} is not a release such as \"{}\"",
                crate::release::RELEASE
            ));
        }
        if self.kicad != KICAD_VERSION {
            return Err(format!(
                "{MANIFEST}: kicad = {}, but netsmith writes KiCad {KICAD_VERSION} files only",
                self.kicad
            ));
        }
        if let Some(word) = &self.assembly
            && !["fab", "hand", "none", "consigned"].contains(&word.as_str())
        {
            return Err(format!(
                "{MANIFEST}: assembly = {word:?} is not one of \"fab\", \"hand\", \"none\" or \"consigned\""
            ));
        }
        if let Some(spelled) = &self.environment {
            environment_key(spelled, &self.dependencies)?;
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
        if let Some(word) = &self.pins
            && !["auto", "locked"].contains(&word.as_str())
        {
            return Err(format!(
                "{MANIFEST}: pins = {word:?} is not one of \"auto\" or \"locked\""
            ));
        }
        if let Some(word) = &self.lock
            && !["all", "top_level", "none"].contains(&word.as_str())
        {
            return Err(format!(
                "{MANIFEST}: lock = {word:?} is not one of \"all\", \"top_level\" or \"none\""
            ));
        }
        if let Some(m) = self.track_margin
            && !(1.0..=4.0).contains(&m)
        {
            return Err(format!(
                "{MANIFEST}: track_margin = {m} must be from 1 (the process's least track) to 4"
            ));
        }
        if let Some(n) = self.place_seeds
            && !(1..=MAX_PLACE_SEEDS).contains(&n)
        {
            return Err(format!(
                "{MANIFEST}: place_seeds = {n} must be from 1 (the layout file's seed only) to {MAX_PLACE_SEEDS}"
            ));
        }
        if let Some(text) = &self.place_budget {
            crate::layout::parse_place_budget(text)
                .map_err(|e| format!("{MANIFEST}: place_budget = {text:?}: {e}"))?;
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
        if let Some(word) = self.lint.as_ref().and_then(|l| l.findings.as_ref())
            && !["warn", "error"].contains(&word.as_str())
        {
            return Err(format!(
                "{MANIFEST}: [lint] findings = {word:?} is not one of \"warn\" or \"error\""
            ));
        }
        for (name, dependency) in &self.dependencies {
            dependency.validate(name)?;
        }
        for file in &self.top_level {
            check_relative("top_level", file)?;
            if !file.ends_with(".nsm") {
                return Err(format!(
                    "{MANIFEST}: top_level file {file:?} must end in .nsm"
                ));
            }
        }
        for (index, dir) in self.catalog_dirs.iter().enumerate() {
            check_folder("catalog_dirs", dir)?;
            if self.catalog_dirs[..index]
                .iter()
                .any(|earlier| same_folder(earlier, dir))
            {
                return Err(format!(
                    "{MANIFEST}: catalog_dirs lists the folder {dir:?} twice"
                ));
            }
        }
        for (index, dir) in self.offer_dirs.iter().enumerate() {
            check_folder("offer_dirs", dir)?;
            if self.offer_dirs[..index]
                .iter()
                .any(|earlier| same_folder(earlier, dir))
            {
                return Err(format!(
                    "{MANIFEST}: offer_dirs lists the folder {dir:?} twice"
                ));
            }
            if let Some(catalogs) = self.catalog_dirs.iter().find(|c| same_folder(c, dir)) {
                return Err(format!(
                    "{MANIFEST}: the folder {dir:?} is in offer_dirs and in catalog_dirs ({catalogs:?}); a folder holds catalogs or offers, not both: put the offers files in a folder of their own"
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

/// A folder in the manifest: relative to the project root, `..` allowed (a folder several
/// projects share), never absolute and never empty.
fn check_folder(key: &str, path: &str) -> Result<(), String> {
    let usable = !path.is_empty()
        && Path::new(path).components().all(|c| {
            matches!(
                c,
                Component::Normal(_) | Component::CurDir | Component::ParentDir
            )
        })
        && Path::new(path)
            .components()
            .any(|c| !matches!(c, Component::CurDir));
    if usable {
        Ok(())
    } else {
        Err(format!(
            "{MANIFEST}: {key} {path:?} must be a folder relative to the project (`catalogs`, `../shared`), not absolute and not empty"
        ))
    }
}

/// A path as messages show it: with `/`, without `.` steps and without a trailing slash.
fn tidy(path: &str) -> String {
    Path::new(path)
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn same_folder(a: &str, b: &str) -> bool {
    tidy(a) == tidy(b)
}

/// A manifest and the directory it was found in.
#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub manifest: Manifest,
    /// `--offline` was given (`fab`, `editor`): [`Project::supply`] asks no provider that would
    /// use the network, and uses an answer kept earlier of any age (issue #803). Not a key of the
    /// manifest: it is a choice of the run, and [`Target::reloaded`] keeps it. A dependency by
    /// `git` that the cache does not hold is then an error, not a fetch.
    pub offline: bool,
    /// The cache of dependencies by `git` ([`crate::git_dependency`]); `None` is this machine's.
    pub git_cache: Option<PathBuf>,
}

/// A project that this one reads through `[dependencies]`, directly or through another
/// dependency (issue #68).
#[derive(Debug, Clone)]
pub struct DependencyProject {
    /// The name its modules are imported under, the key of its `[dependencies]` entry:
    /// `shared` makes its `src/power.nsm` the module `shared::power`.
    pub name: String,
    /// Its folder as messages name it: relative to the depending project's root, with `/`
    /// (`../shared`). A dependency by `git` is named by its key and its commit,
    /// `shared@0123456789ab`, since its files are in the cache.
    pub folder: String,
    pub project: Project,
    /// The commit of a dependency by `git`; `None` for one by `path`.
    pub commit: Option<String>,
}

/// One catalog file of a project: found in a `catalog_dirs` folder, or listed under
/// `catalogs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogFile {
    /// The catalog it holds: the file's name without `.csv`.
    pub name: String,
    /// The file as messages name it: relative to the project root, with `/`.
    pub file: String,
    pub path: PathBuf,
    /// The `catalog_dirs` entry that holds it; `None` for a file the `catalogs` list names.
    pub folder: Option<String>,
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
            offline: false,
            git_cache: None,
        })
    }

    /// This project for a run that asks no supplier (`--offline`): see [`Project::offline`].
    pub fn with_offline(mut self, offline: bool) -> Project {
        self.offline = offline;
        self
    }

    /// This project with its dependencies by `git` cached under `dir` instead of this machine's
    /// cache (tests).
    pub fn with_git_cache(mut self, dir: &Path) -> Project {
        self.git_cache = Some(dir.to_path_buf());
        self
    }

    /// The cache of dependencies by `git` this run uses.
    pub fn git_cache(&self) -> Result<crate::git_dependency::GitCache, String> {
        let dir = match &self.git_cache {
            Some(dir) => dir.clone(),
            None => crate::git_dependency::GitCache::default_dir().ok_or(
                "a dependency by `git` needs a cache folder: set NETSMITH_CACHE_DIR or HOME",
            )?,
        };
        Ok(crate::git_dependency::GitCache::at(&dir).offline(self.offline))
    }

    /// The directory `netsmith board build` writes to.
    pub fn out_dir(&self) -> PathBuf {
        self.root.join(&self.manifest.out)
    }

    /// The directory `netsmith board fab` writes to.
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

    /// The projects this one reads through `[dependencies]`, and theirs, each once, depth first,
    /// the entries of each manifest in the order of their names (issue #68). They share one set of names: two projects under
    /// one name, a dependency that is not a project, and a project that depends on itself
    /// through others are errors that name the manifest and the entry.
    ///
    /// A dependency by `git` is read at the commit the dependency lock holds for it, or, with no
    /// entry, at the one its `rev` or `version` names in the cache, fetched only when the cache
    /// cannot answer (issue #1012).
    pub fn dependencies(&self) -> Result<Vec<DependencyProject>, String> {
        let lock = crate::dependency_lock::DependencyLock::read(&self.root)?;
        Ok(self
            .resolve_dependencies(lock.as_ref(), crate::git_dependency::Fetch::WhenMissing)?
            .0)
    }

    /// The dependencies ([`Project::dependencies`]), each by `git` at the commit `lock` holds for
    /// it, or else resolved as `fetch` says, and the commit each by `git` was read at: the lock
    /// that holds what this run read.
    pub fn resolve_dependencies(
        &self,
        lock: Option<&crate::dependency_lock::DependencyLock>,
        fetch: crate::git_dependency::Fetch,
    ) -> Result<
        (
            Vec<DependencyProject>,
            crate::dependency_lock::DependencyLock,
        ),
        String,
    > {
        let mut walk = Walk {
            project: self,
            cache: None,
            lock,
            fetch,
            chain: vec![(
                fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone()),
                self.manifest.name.clone(),
            )],
            found: Vec::new(),
            locked: Vec::new(),
        };
        walk.walk(self, "", None)?;
        Ok((
            walk.found,
            crate::dependency_lock::DependencyLock::new(walk.locked),
        ))
    }

    /// What every dependency by `git` (this project's and its dependencies') asks for, with no
    /// commit, as far as the manifests can be read without fetching: a dependency by `git` that
    /// the cache does not hold contributes no dependencies of its own, and the flag is then
    /// false: the list is this project's own. For `project lock --check`.
    pub fn git_requests(&self) -> Result<(Vec<crate::dependency_lock::Locked>, bool), String> {
        let lock = crate::dependency_lock::DependencyLock::read(&self.root)?;
        let offline = self.clone().with_offline(true);
        let resolved =
            offline.resolve_dependencies(lock.as_ref(), crate::git_dependency::Fetch::WhenMissing);
        let complete = resolved.is_ok();
        let found = resolved.map(|(found, _)| found).unwrap_or_default();
        let mut wanted: Vec<_> = self
            .manifest
            .dependencies
            .iter()
            .filter_map(|(name, d)| d.request(name))
            .collect();
        for dependency in &found {
            for (name, d) in &dependency.project.manifest.dependencies {
                if let Some(request) = d.request(name)
                    && !wanted
                        .iter()
                        .any(|w: &crate::dependency_lock::Locked| w.same_request(&request))
                {
                    wanted.push(request);
                }
            }
        }
        Ok((wanted, complete))
    }

    /// The `.nsm` files of the dependencies' `src/` folders (issue #68), each with the module it
    /// is under its dependency's name (`shared::power`) and the name messages give it, from this
    /// project's root (`../shared/src/power.nsm`).
    pub fn dependency_modules(&self) -> Result<Vec<(Module, String)>, String> {
        let mut out = Vec::new();
        for dependency in self.dependencies()? {
            for module in dependency.project.modules()? {
                let shown = format!(
                    "{}/{}",
                    dependency.folder,
                    module.name(&dependency.project.root)
                );
                let rest = module
                    .key
                    .strip_prefix(SOURCE_DIR)
                    .expect("a module of a project is under src/");
                out.push((
                    Module {
                        key: format!("{}{rest}", dependency.name),
                        path: module.path,
                    },
                    shown,
                ));
            }
        }
        Ok(out)
    }

    /// The folder of each dependency, with the name messages give its `kicad.toml`
    /// (`../shared/kicad.toml`), for the KiCad mapping (issue #1012).
    pub fn kicad_folders(&self) -> Result<Vec<(PathBuf, String)>, String> {
        Ok(self
            .dependencies()?
            .into_iter()
            .map(|d| {
                (
                    d.project.root,
                    format!("{}/{}", d.folder, netsmith_kicad::PROJECT_FILE),
                )
            })
            .collect())
    }

    /// The KiCad libraries of this project: the installed ones, the standard mapping, and the
    /// `kicad.toml` of the project and of each dependency.
    pub fn libraries(&self) -> Result<netsmith_pipeline::Libraries, String> {
        netsmith_pipeline::Libraries::for_project_with(Some(&self.root), &self.kicad_folders()?)
    }

    /// The module path of a file of this project, however its name was typed: `src/power.nsm`,
    /// `./src/power.nsm` and its absolute path are all `src::power`. `None` for a file that
    /// is not under the project's root or cannot be read.
    pub fn key_of(&self, file: &Path) -> Option<String> {
        let file = fs::canonicalize(file).ok()?;
        let root = fs::canonicalize(&self.root).ok()?;
        let relative = file.strip_prefix(&root).ok()?;
        let mut steps: Vec<String> = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let last = steps.last_mut()?;
        if let Some(stem) = last.strip_suffix(".nsm") {
            *last = stem.to_string();
        }
        Some(steps.join("::"))
    }

    /// The `*.csv` files directly in each `catalog_dirs` folder (sub-folders are not
    /// read), each folder's in file-name order, as `(folder, shown name, path, file
    /// name)`. A folder that does not exist holds none: the default `catalogs` is
    /// there for a project that has no catalogs of its own yet.
    fn folder_csvs(&self) -> Result<Vec<(String, String, PathBuf, String)>, String> {
        self.csvs_in("catalog_dirs", &self.manifest.catalog_dirs)
    }

    /// [`Project::folder_csvs`] of the folders `dirs`, which the manifest's `key` lists.
    fn csvs_in(
        &self,
        key: &str,
        dirs: &[String],
    ) -> Result<Vec<(String, String, PathBuf, String)>, String> {
        let mut found = Vec::new();
        for dir in dirs {
            let path = self.root.join(dir);
            if !path.exists() {
                continue;
            }
            let io = |e: io::Error| format!("{MANIFEST}: {key} {dir:?}: {e}");
            if !path.is_dir() {
                return Err(format!(
                    "{MANIFEST}: {key} {dir:?} is not a folder ({})",
                    path.display()
                ));
            }
            let mut names: Vec<String> = Vec::new();
            for entry in fs::read_dir(&path).map_err(io)? {
                let entry = entry.map_err(io)?;
                let name = entry.file_name().to_string_lossy().into_owned();
                // `is_file` follows a link; a sub-folder is not read.
                if name.ends_with(".csv") && entry.path().is_file() {
                    names.push(name);
                }
            }
            names.sort();
            let folder = tidy(dir);
            for name in names {
                let shown = format!("{folder}/{name}");
                found.push((folder.clone(), shown, path.join(&name), name));
            }
        }
        Ok(found)
    }

    /// Every catalog file of the project: the `*.csv` files of the `catalog_dirs` folders
    /// (folders in the order the manifest lists them, files in file-name order), then the
    /// files `catalogs` lists. A `.csv` in a folder whose name is not a catalog name, a
    /// file that the folder and the list both name, and two files of one catalog are
    /// errors that name the files.
    pub fn catalog_files(&self) -> Result<Vec<CatalogFile>, String> {
        let found = self.own_catalog_files()?;
        one_file_each(&found)?;
        Ok(found)
    }

    /// The catalog files a build reads: the project's own ([`Project::catalog_files`]) and then
    /// those of each dependency (issue #68), named from this project's root
    /// (`../shared/catalogs/acme_ldo.csv`). A catalog that two of them hold is an error that names
    /// both files: a catalog is one file.
    pub fn all_catalog_files(&self) -> Result<Vec<CatalogFile>, String> {
        let mut found = self.own_catalog_files()?;
        for dependency in self.dependencies()? {
            for mut file in dependency.project.own_catalog_files()? {
                file.file = format!("{}/{}", dependency.folder, file.file);
                file.folder = file.folder.map(|f| format!("{}/{f}", dependency.folder));
                found.push(file);
            }
        }
        one_file_each(&found)?;
        Ok(found)
    }

    fn own_catalog_files(&self) -> Result<Vec<CatalogFile>, String> {
        let mut found: Vec<CatalogFile> = Vec::new();
        for (folder, file, path, file_name) in self.folder_csvs()? {
            let name = catalog_name(&file_name).ok_or_else(|| {
                format!(
                    "{MANIFEST}: {file} in the catalog_dirs folder {folder:?} is not a catalog: a catalog file is named after its catalog (letters, digits and `_`, not starting with a digit) and ends in .csv. Rename it, or move it out of the folder (a folder's sub-folders are not read)"
                )
            })?;
            found.push(CatalogFile {
                name: name.to_string(),
                file,
                path,
                folder: Some(folder),
            });
        }
        for file in &self.manifest.catalogs {
            found.push(CatalogFile {
                name: catalog_name(file)
                    .expect("the manifest was validated")
                    .to_string(),
                file: tidy(file),
                path: self.root.join(file),
                folder: None,
            });
        }
        Ok(found)
    }
}

/// Refuse two files of one catalog among `found`, naming both.
fn one_file_each(found: &[CatalogFile]) -> Result<(), String> {
    for (index, later) in found.iter().enumerate() {
        let Some(earlier) = found[..index].iter().find(|f| f.name == later.name) else {
            continue;
        };
        let same_file = earlier.file == later.file
            || matches!(
                (fs::canonicalize(&earlier.path), fs::canonicalize(&later.path)),
                (Ok(a), Ok(b)) if a == b
            );
        return Err(match (&earlier.folder, &later.folder) {
            (Some(folder), None) if same_file => format!(
                "{MANIFEST}: {} is named twice: the catalog_dirs folder {folder:?} holds it and `catalogs` lists it. Remove it from `catalogs`: a file in a catalog_dirs folder needs no entry",
                later.file
            ),
            _ => format!(
                "{MANIFEST}: the catalog `{}` is in two files, {} and {}; a catalog is one file. Rename or remove one",
                later.name, earlier.file, later.file
            ),
        });
    }
    Ok(())
}

/// One offers file of a project (ADR 0143): `<supplier>.csv` in an `offer_dirs` folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferFile {
    /// The supplier it is named for: the file's name without `.csv` (`jlcpcb`).
    pub supplier: String,
    /// The file as messages name it: relative to the project root, with `/`.
    pub file: String,
    pub path: PathBuf,
}

impl Project {
    /// The offers files a build reads: the `*.csv` files of the project's `offer_dirs` folders
    /// (folders in the order the manifest lists them, files in file-name order) and then those
    /// of each dependency, named from this project's root. A `.csv` whose name is not a
    /// supplier's, and two files of one supplier, are errors that name the files.
    pub fn all_offer_files(&self) -> Result<Vec<OfferFile>, String> {
        let mut found = self.own_offer_files()?;
        for dependency in self.dependencies()? {
            for mut file in dependency.project.own_offer_files()? {
                file.file = format!("{}/{}", dependency.folder, file.file);
                found.push(file);
            }
        }
        for (index, later) in found.iter().enumerate() {
            if let Some(earlier) = found[..index].iter().find(|f| f.supplier == later.supplier) {
                return Err(format!(
                    "{MANIFEST}: the offers of `{}` are in two files, {} and {}; a supplier's offers are one file. Move the offers into one, and remove the other",
                    later.supplier, earlier.file, later.file
                ));
            }
        }
        Ok(found)
    }

    /// The project's own offers files ([`Project::all_offer_files`] without its dependencies'):
    /// the ones a command that writes offers (`catalog refresh`, `catalog add`, `project
    /// migrate-offers`) acts on.
    pub fn offer_files(&self) -> Result<Vec<OfferFile>, String> {
        self.own_offer_files()
    }

    /// Where the project's offers file of `supplier` is, or would be made: the file in its
    /// `offer_dirs` folders when there is one, else `<first offer_dirs folder>/<supplier>.csv`
    /// (`offers/lcsc.csv`). `None` when the manifest lists no offers folder.
    pub fn offer_file_of(&self, supplier: &str) -> Result<Option<(PathBuf, String)>, String> {
        if let Some(f) = self
            .own_offer_files()?
            .into_iter()
            .find(|f| f.supplier == supplier)
        {
            return Ok(Some((f.path, f.file)));
        }
        Ok(self.manifest.offer_dirs.first().map(|dir| {
            let file = format!("{}/{supplier}.csv", dir.trim_end_matches('/'));
            (self.root.join(&file), file)
        }))
    }

    fn own_offer_files(&self) -> Result<Vec<OfferFile>, String> {
        let mut found = Vec::new();
        for (folder, file, path, file_name) in
            self.csvs_in("offer_dirs", &self.manifest.offer_dirs)?
        {
            let supplier = file_name.strip_suffix(".csv").expect("a .csv");
            if supplier == netsmith_solve::offers::MERGED_INTO_LCSC {
                return Err(netsmith_solve::offers::merge_into_lcsc(&file));
            }
            if !netsmith_solve::offers::is_supplier(supplier) {
                return Err(format!(
                    "{MANIFEST}: {file} in the offer_dirs folder {folder:?} is not an offers file: an offers file is named for its supplier, {}. Rename it, or move it out of the folder (a folder's sub-folders are not read)",
                    netsmith_solve::offers::supplier_files()
                ));
            }
            found.push(OfferFile {
                supplier: supplier.to_string(),
                file,
                path,
            });
        }
        Ok(found)
    }
}

/// One curve file of a project (`netsmith_solve::curve`, issue #1260):
/// `curves/<catalog>/<row>.csv` in a `catalog_dirs` folder or beside a file `catalogs` lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurveFile {
    /// The catalog it belongs to: the name of its folder.
    pub catalog: String,
    /// The row it is for: the file's name without `.csv`, the row's `lcsc` or `mpn`.
    pub row: String,
    /// The file as messages name it: relative to the project root, with `/`.
    pub file: String,
    pub path: PathBuf,
}

impl Project {
    /// The curve files a build reads: the project's own and then those of each dependency,
    /// named from this project's root. In a `catalog_dirs` folder every `curves/<catalog>/`
    /// folder is read (a curve may be for a row of a standard catalog); beside a file `catalogs`
    /// lists, the folder of its catalog. A `.csv` in `curves/` that is not in a catalog's folder,
    /// and a catalog folder whose name is not a catalog name, are errors that name them.
    pub fn all_curve_files(&self) -> Result<Vec<CurveFile>, String> {
        let mut found = self.own_curve_files()?;
        for dependency in self.dependencies()? {
            for mut file in dependency.project.own_curve_files()? {
                file.file = format!("{}/{}", dependency.folder, file.file);
                found.push(file);
            }
        }
        Ok(found)
    }

    fn own_curve_files(&self) -> Result<Vec<CurveFile>, String> {
        let curves = netsmith_solve::curve::FOLDER;
        // The folders to read, as (shown name, path, the one catalog to read or every one).
        let mut roots: Vec<(String, PathBuf, Option<String>)> = Vec::new();
        for dir in &self.manifest.catalog_dirs {
            let folder = tidy(dir);
            let shown = if folder.is_empty() {
                curves.to_string()
            } else {
                format!("{folder}/{curves}")
            };
            roots.push((shown, self.root.join(dir).join(curves), None));
        }
        for file in &self.manifest.catalogs {
            let name = catalog_name(file).expect("the manifest was validated");
            let parent = Path::new(file).parent().map(|p| tidy(&p.to_string_lossy()));
            let shown = match parent.as_deref() {
                None | Some("") => curves.to_string(),
                Some(p) => format!("{p}/{curves}"),
            };
            let path = self.root.join(file).with_file_name(curves);
            roots.push((shown, path, Some(name.to_string())));
        }
        let mut found: Vec<CurveFile> = Vec::new();
        let mut seen: Vec<PathBuf> = Vec::new();
        for (shown, root, only) in roots {
            if !root.is_dir() {
                continue;
            }
            let io = |e: io::Error| format!("{MANIFEST}: {shown}: {e}");
            let mut folders: Vec<(String, PathBuf)> = Vec::new();
            for entry in fs::read_dir(&root).map_err(io)? {
                let entry = entry.map_err(io)?;
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();
                if path.is_dir() {
                    if only.as_ref().is_none_or(|o| *o == name) {
                        folders.push((name, path));
                    }
                } else if name.ends_with(".csv") && only.is_none() {
                    return Err(format!(
                        "{MANIFEST}: {shown}/{name} is not in the folder of a catalog: a curve file is {shown}/<catalog>/<row>.csv"
                    ));
                }
            }
            folders.sort();
            for (catalog, folder) in folders {
                if !is_identifier(&catalog) {
                    return Err(format!(
                        "{MANIFEST}: {shown}/{catalog} is not named after a catalog (letters, digits and `_`, not starting with a digit): a curve file is {shown}/<catalog>/<row>.csv"
                    ));
                }
                let key = fs::canonicalize(&folder).unwrap_or_else(|_| folder.clone());
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
                let mut names: Vec<String> = Vec::new();
                for entry in fs::read_dir(&folder).map_err(io)? {
                    let entry = entry.map_err(io)?;
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.ends_with(".csv") && entry.path().is_file() {
                        names.push(name);
                    }
                }
                names.sort();
                for name in names {
                    let row = name.strip_suffix(".csv").expect("a .csv").to_string();
                    found.push(CurveFile {
                        catalog: catalog.clone(),
                        row,
                        file: format!("{shown}/{catalog}/{name}"),
                        path: folder.join(&name),
                    });
                }
            }
        }
        Ok(found)
    }

    /// The catalog files to watch for changes: those of `catalog_dirs` folders as they are
    /// now (a new or a removed file changes the list, so the editor sees it) and those
    /// `catalogs` lists. A mistake in a name is reported by [`Project::catalogs`], not here.
    fn catalog_paths(&self) -> Vec<PathBuf> {
        let mut paths = self.own_catalog_paths();
        // A dependency's manifest, catalogs and modules are read by every build (issue #68).
        for dependency in self.dependencies().unwrap_or_default() {
            paths.push(dependency.project.root.join(MANIFEST));
            paths.extend(dependency.project.own_catalog_paths());
            for module in dependency.project.modules().unwrap_or_default() {
                paths.push(module.path);
            }
        }
        paths
    }

    /// [`Project::catalog_paths`] without the dependencies'.
    fn own_catalog_paths(&self) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = self
            .folder_csvs()
            .unwrap_or_default()
            .into_iter()
            .map(|(_, _, path, _)| path)
            .collect();
        paths.extend(self.manifest.catalogs.iter().map(|c| self.root.join(c)));
        // The curve files as they are now: a new or a removed one changes the list.
        paths.extend(
            self.own_curve_files()
                .unwrap_or_default()
                .into_iter()
                .map(|c| c.path),
        );
        // The offers files as they are now: a new or a removed one changes the list.
        paths.extend(
            self.csvs_in("offer_dirs", &self.manifest.offer_dirs)
                .unwrap_or_default()
                .into_iter()
                .map(|(_, _, path, _)| path),
        );
        // Watched only when it is there (a missing file would read as changed on every look);
        // its first write lengthens the list, which is a change too.
        let preferred = self.root.join(PREFERRED);
        if preferred.is_file() {
            paths.push(preferred);
        }
        paths
    }

    /// The standard JLCPCB catalogs and the project's own catalog files
    /// ([`Project::catalog_files`]), with the standard offers and the project's
    /// ([`Project::all_offer_files`]), read again on each call. A problem names the file and
    /// line.
    pub fn catalogs(&self) -> Result<Catalogs, String> {
        let mut catalogs = Catalogs::jlcpcb()
            .with_default_assembly(self.manifest.assembly.as_deref())
            .with_strict_guidelines(self.manifest.guidelines.as_deref() == Some("error"))
            .with_constraints_warn(self.manifest.constraints.as_deref() == Some("warn"))
            .with_series(series_of(self.manifest.series.as_deref()));
        for CatalogFile {
            name, file, path, ..
        } in self.all_catalog_files()?
        {
            let text = fs::read_to_string(&path).map_err(|e| format!("{MANIFEST}: {file}: {e}"))?;
            catalogs.add_file(&name, &file, &text)?;
        }
        for CurveFile {
            catalog,
            row,
            file,
            path,
        } in self.all_curve_files()?
        {
            let text = fs::read_to_string(&path).map_err(|e| format!("{MANIFEST}: {file}: {e}"))?;
            catalogs.add_curve(&catalog, &row, &file, &text)?;
        }
        for OfferFile {
            supplier,
            file,
            path,
        } in self.all_offer_files()?
        {
            let text = fs::read_to_string(&path).map_err(|e| format!("{MANIFEST}: {file}: {e}"))?;
            catalogs.add_offers_file(&supplier, &file, &text)?;
        }
        if let Some(preferred) = self.preferred_with(Some(catalogs.offers()))? {
            catalogs = catalogs.with_preferred(preferred);
        }
        if let Some(config) = &self.manifest.supply {
            catalogs = catalogs.with_channels(config.channel_list());
            let entries = config.part_channels();
            if !entries.is_empty() {
                catalogs = catalogs.with_part_channels(netsmith_solve::PartChannels {
                    entries,
                    declared: self.declared_units(),
                });
            }
            if config.provider.is_some() {
                catalogs = catalogs.with_availability(self.supply()?.into_availability());
            }
        }
        Ok(catalogs)
    }

    /// The names of the boards and systems the project's top-level files declare, read from
    /// their syntax alone: what an entry of `[supply.parts]` is checked against by the build of
    /// one file, which sees only its own. A file that cannot be read or parsed adds none; its
    /// own build says why.
    fn declared_units(&self) -> Vec<String> {
        let mut out = Vec::new();
        for file in &self.manifest.top_level {
            let Ok(text) = fs::read_to_string(self.root.join(file)) else {
                continue;
            };
            let Ok(parsed) = netsmith_lang::parse(&text) else {
                continue;
            };
            out.extend(
                parsed
                    .definitions
                    .iter()
                    .filter(|d| {
                        matches!(
                            d.kind,
                            netsmith_lang::ast::DefKind::Board
                                | netsmith_lang::ast::DefKind::System
                        )
                    })
                    .map(|d| d.name.name.clone()),
            );
        }
        out
    }

    /// The project's preferred parts, `preferred.csv` beside the manifest (issue #802), or
    /// `None` when the project has no such file. A problem names the file and line.
    pub fn preferred(&self) -> Result<Option<Preferred>, String> {
        self.preferred_with(None)
    }

    /// [`Self::preferred`], with the offers that name the `mpn` of each number of a file that
    /// still has an `lcsc` column, in the message that refuses it (ADR 0143).
    fn preferred_with(
        &self,
        offers: Option<&netsmith_solve::offers::Offers>,
    ) -> Result<Option<Preferred>, String> {
        let path = self.root.join(PREFERRED);
        if !path.is_file() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("{PREFERRED}: {e}"))?;
        Preferred::parse_with(PREFERRED, &text, offers).map(Some)
    }

    /// The providers of the `[supply]` table, with the cache in front of them, read again on
    /// each call. Without the table no provider is configured, and every price and stock is
    /// unknown (ADR 0047). With [`Project::offline`] no provider that would use the network is
    /// asked.
    pub fn supply(&self) -> Result<Supply, String> {
        let Some(config) = &self.manifest.supply else {
            return Ok(Supply::none());
        };
        let mut supply = Supply::none()
            .with_offline(self.offline)
            .with_channels(config.channel_list());
        if config.provider.is_some() {
            let file = config.file.as_deref().expect("the manifest was validated");
            let provider = FileProvider::open(&self.root.join(file), file)?;
            supply = supply.with_provider(Box::new(provider));
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
            loader.attach_path(&name, &m.path);
            crate::physical::attach(&mut loader, &name, &m.path, unsaved)?;
        }
        for dependency in self.dependencies()? {
            loader.add_dependency(&dependency.name);
        }
        // A dependency's file is given no path (`Loader::attach_path`): the layout file beside
        // it is not the project's to write, so a board of it keeps the system's (issue #1327).
        for (m, name) in self.dependency_modules()? {
            let text = match unsaved.get(&m.path) {
                Some(text) => text.clone(),
                None => fs::read_to_string(&m.path).map_err(|e| format!("{name}: {e}"))?,
            };
            loader.insert(&m.key, &name, &text);
            crate::physical::attach(&mut loader, &name, &m.path, unsaved)?;
        }
        if let Some(spelled) = &self.manifest.environment {
            let (module, name) = environment_key(spelled, &self.manifest.dependencies)?;
            if !loader.modules().iter().any(|(key, _, _)| *key == module) {
                return Err(format!(
                    "{MANIFEST}: environment = {spelled:?} names the module `{module}`, which is not a module of the standard library, a file of `src/` or a module of a dependency"
                ));
            }
            loader = loader.with_default_environment(&module, &name);
        }
        Ok(loader)
    }
}

/// The walk of [`Project::resolve_dependencies`].
struct Walk<'a> {
    /// The project whose dependencies these are, whose cache and lock are used for all of them.
    project: &'a Project,
    cache: Option<crate::git_dependency::GitCache>,
    lock: Option<&'a crate::dependency_lock::DependencyLock>,
    fetch: crate::git_dependency::Fetch,
    /// The projects from the first to the one being walked, to find one that depends on itself.
    chain: Vec<(PathBuf, String)>,
    found: Vec<DependencyProject>,
    /// The commit each dependency by `git` was read at.
    locked: Vec<crate::dependency_lock::Locked>,
}

impl Walk<'_> {
    fn cache(&mut self) -> Result<&crate::git_dependency::GitCache, String> {
        if self.cache.is_none() {
            self.cache = Some(self.project.git_cache()?);
        }
        Ok(self.cache.as_ref().expect("set above"))
    }

    /// The folder of the dependency `name` of a git entry, and its commit: the locked one, or
    /// the one its `rev` or `version` names.
    fn git_folder(
        &mut self,
        name: &str,
        dependency: &Dependency,
    ) -> Result<(PathBuf, String), String> {
        use crate::git_dependency::Want;
        let mut request = dependency.request(name).expect("a git entry");
        let url = request.git.clone();
        let locked = self
            .lock
            .and_then(|lock| lock.commit_for(&request))
            .map(str::to_string);
        let commit = match locked {
            Some(commit) => commit,
            None => {
                let fetch = self.fetch;
                let want_version;
                let want = match (&dependency.rev, &dependency.version) {
                    (Some(rev), _) => Want::Rev(rev),
                    (_, Some(version)) => {
                        want_version = dependency.version_req(name, version)?;
                        Want::Version(&want_version)
                    }
                    _ => unreachable!("the manifest was validated"),
                };
                self.cache()?.resolve(&url, want, fetch)?
            }
        };
        let dir = self.cache()?.checkout(&url, &commit)?;
        request.commit = commit.clone();
        if !self.locked.contains(&request) {
            self.locked.push(request);
        }
        Ok((dir, commit))
    }

    /// Add the dependencies of `project` to `found`, depth first. `folder` is `project`'s folder
    /// from the first project's root (empty for the first project itself), and `repository` the
    /// checkout `project` is in when it came by `git`, which its `path` entries stay inside.
    fn walk(
        &mut self,
        project: &Project,
        folder: &str,
        repository: Option<&Path>,
    ) -> Result<(), String> {
        let manifest = if folder.is_empty() {
            MANIFEST.to_string()
        } else {
            format!("{folder}/{MANIFEST}")
        };
        for (name, dependency) in &project.manifest.dependencies {
            let (dir, shown, commit, inside) = match (&dependency.path, &dependency.git) {
                (Some(path), _) => {
                    if repository.is_some() && normal_folder(path).starts_with("..") {
                        return Err(outside_repository(&manifest, name, path, folder));
                    }
                    let shown = normal_folder(&if folder.is_empty() {
                        path.to_string()
                    } else {
                        format!("{folder}/{path}")
                    });
                    (
                        project.root.join(path),
                        shown,
                        None,
                        repository.map(Path::to_path_buf),
                    )
                }
                (None, Some(_)) => {
                    let (dir, commit) = self
                        .git_folder(name, dependency)
                        .map_err(|e| format!("{manifest}: [dependencies] {name}: {e}"))?;
                    let shown = format!("{name}@{}", &commit[..12]);
                    let inside = Some(dir.clone());
                    (dir, shown, Some(commit), inside)
                }
                (None, None) => unreachable!("the manifest was validated"),
            };
            if !dir.join(MANIFEST).is_file() {
                return Err(format!(
                    "{manifest}: [dependencies] {name}: {shown} is not a project: it has no {MANIFEST} ({})",
                    dir.display()
                ));
            }
            let canonical = fs::canonicalize(&dir)
                .map_err(|e| format!("{manifest}: [dependencies] {name}: {shown}: {e}"))?;
            if commit.is_none()
                && let Some(repository) = repository
                && !fs::canonicalize(repository).is_ok_and(|r| canonical.starts_with(r))
            {
                return Err(outside_repository(&manifest, name, &shown, folder));
            }
            if let Some(at) = self.chain.iter().position(|(root, _)| *root == canonical) {
                let mut names: Vec<&str> =
                    self.chain[at..].iter().map(|(_, n)| n.as_str()).collect();
                names.push(name);
                return Err(format!(
                    "{manifest}: [dependencies] {name}: {shown} depends on itself ({}); a project cannot depend on itself through others",
                    names.join(" -> ")
                ));
            }
            if let Some(earlier) = self.found.iter().find(|d| d.name == *name) {
                let same = fs::canonicalize(&earlier.project.root).is_ok_and(|r| r == canonical);
                if same {
                    continue;
                }
                return Err(format!(
                    "{manifest}: [dependencies] {name}: two projects are named `{name}`, {} and {shown}; the modules of a dependency are imported by its name, so the name is one project's",
                    earlier.folder
                ));
            }
            let loaded = Project::load(&dir)
                .map_err(|e| format!("{manifest}: [dependencies] {name}: {e}"))?;
            self.found.push(DependencyProject {
                name: name.clone(),
                folder: shown.clone(),
                project: loaded.clone(),
                commit,
            });
            self.chain.push((canonical, name.clone()));
            self.walk(&loaded, &shown, inside.as_deref())?;
            self.chain.pop();
        }
        Ok(())
    }
}

/// The error of a `path` entry of a dependency by `git` that leaves its repository.
fn outside_repository(manifest: &str, name: &str, path: &str, folder: &str) -> String {
    format!(
        "{manifest}: [dependencies] {name}: {path} is outside the git repository {folder} came from; a dependency by `git` names its own dependencies by `git`, or by a `path` inside it"
    )
}

/// A folder as messages show it: with `/`, without `.` steps, and with each `..` that undoes a
/// step before it taken out with that step (`../a/../b` is `../b`).
fn normal_folder(path: &str) -> String {
    let mut steps: Vec<&str> = Vec::new();
    for step in path.split(['/', '\\']) {
        match step {
            "" | "." => {}
            ".." if steps.last().is_some_and(|last| *last != "..") => {
                steps.pop();
            }
            step => steps.push(step),
        }
    }
    if steps.is_empty() {
        ".".to_string()
    } else {
        steps.join("/")
    }
}

/// The module key and the definition name that the manifest's `environment` spells
/// (ADR 0099): `std::env::Environment` is the definition `Environment` of the standard module
/// `std::env`, and `src/vehicle.nsm::Automotive` the definition `Automotive` of the project's
/// file `src/vehicle.nsm`, the module `src::vehicle`.
///
/// A spelling whose first step is the key of a `[dependencies]` entry is a module of that
/// dependency by its import path: `shared::env::Lab` is the definition `Lab` of the module
/// `shared::env`, the dependency's `src/env.nsm` (issue #1012). A dependency's files are named by
/// module path and not by file, since a dependency by `git` has no folder in the project.
fn environment_key(
    spelled: &str,
    dependencies: &BTreeMap<String, Dependency>,
) -> Result<(String, String), String> {
    let bad = |why: &str| {
        format!(
            "{MANIFEST}: environment = {spelled:?} {why}: write the definition as `std::env::Environment`, `src/vehicle.nsm::Automotive` or, for a dependency's, `shared::env::Lab`"
        )
    };
    let Some((module, name)) = spelled.rsplit_once("::") else {
        return Err(bad("is not a module and a definition"));
    };
    if !is_identifier(name) {
        return Err(bad("does not end in the name of a definition"));
    }
    if let Some((first, _)) = module.split_once("::").or(Some((module, "")))
        && dependencies.contains_key(first)
    {
        if !module.split("::").all(is_identifier) || !module.contains("::") {
            return Err(bad("is not a module of the dependency"));
        }
        return Ok((module.to_string(), name.to_string()));
    }
    if let Some(std) = module.strip_prefix("std::") {
        if !std.split("::").all(is_identifier) {
            return Err(bad("is not a standard module"));
        }
        return Ok((module.to_string(), name.to_string()));
    }
    let Some(file) = module.strip_suffix(".nsm") else {
        return Err(bad("names neither a standard module nor a `.nsm` file"));
    };
    check_relative("environment", module)?;
    let steps: Vec<&str> = file.split('/').collect();
    if steps.first() != Some(&SOURCE_DIR)
        || steps.len() < 2
        || !steps.iter().all(|s| is_identifier(s))
    {
        return Err(bad(
            "names a file that is not a module of the project (a file of `src/`)",
        ));
    }
    Ok((steps.join("::"), name.to_string()))
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

pub fn is_identifier(s: &str) -> bool {
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

impl SourceFile {
    /// The file's name for output that must not depend on where the command runs (the pin map
    /// for firmware): its path from the project's root (the folder of `netsmith.toml`) with `/`
    /// between the parts, or the file's own name when there is no project (or the file is not
    /// under its root).
    pub fn stable_name(&self, project: Option<&Project>) -> String {
        let file_name = || {
            self.path
                .file_name()
                .map_or_else(|| self.name.clone(), |n| n.to_string_lossy().into_owned())
        };
        let Some(project) = project else {
            return file_name();
        };
        let resolve = |p: &Path| fs::canonicalize(p).or_else(|_| std::path::absolute(p));
        let (Ok(root), Ok(path)) = (resolve(&project.root), resolve(&self.path)) else {
            return file_name();
        };
        match path.strip_prefix(&root) {
            Ok(rest) => rest
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
            Err(_) => file_name(),
        }
    }
}

impl Target {
    /// Whether `source` gets a pick lock (ADR 0100): the project's `lock` key says which files
    /// do. A file with no project always does.
    pub fn locks(&self, source: &SourceFile) -> bool {
        let Some(project) = &self.project else {
            return true;
        };
        match project.manifest.lock.as_deref() {
            None | Some("all") => true,
            Some("none") => false,
            Some(_) => {
                let name = source.stable_name(Some(project));
                project.manifest.top_level.contains(&name)
            }
        }
    }
}

/// What a command acts on: the files, the project they belong to if any, and where output
/// goes by default.
#[derive(Debug, Clone)]
pub struct Target {
    pub project: Option<Project>,
    pub files: Vec<SourceFile>,
    /// `netsmith board build`'s output directory when `--out` is not given.
    pub build_out: PathBuf,
    /// `netsmith board fab`'s output directory when `--out` is not given.
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
                        "no {MANIFEST} in {} or any directory above it: name a .nsm file, or run `netsmith project init` to start a project",
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

    /// This target built again from the manifest on disk, for a long-running editor that finds
    /// `netsmith.toml` changed: `None` if the manifest reads as it did (a comment, or white
    /// space), or if there is no project. A target of a project's top-level files (the
    /// manifest's own list) takes its files, title and output directories from the new
    /// manifest; one of a named file keeps those and takes only the project. A manifest that
    /// cannot be read, or that names a top-level file that is no file, is the error: the
    /// target is not changed by it.
    pub fn reloaded(&self) -> Result<Option<Target>, String> {
        let Some(old) = &self.project else {
            return Ok(None);
        };
        let project = Project::load(&old.root)?.with_offline(old.offline);
        if project.manifest == old.manifest {
            return Ok(None);
        }
        let from_manifest = self
            .files
            .iter()
            .map(|f| &f.name)
            .eq(old.manifest.top_level.iter());
        if !from_manifest {
            return Ok(Some(Target {
                project: Some(project),
                ..self.clone()
            }));
        }
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
        Ok(Some(Target {
            title: project.manifest.name.clone(),
            build_out: project.out_dir(),
            fab_out: project.fab_dir(),
            files,
            project: Some(project),
        }))
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
            loader.attach_path(&file.name, &file.path);
            crate::physical::attach(&mut loader, &file.name, &file.path, unsaved)?;
            crate::pins::attach(&mut loader, &file.name, &file.path, unsaved)?;
            // Its module path is the project's, not the spelling it was typed with: the
            // providers in it sort the same whichever way a person names the file (ADR 0068).
            if let Some(key) = self
                .project
                .as_ref()
                .and_then(|project| project.key_of(&file.path))
            {
                loader.attach_root_key(&file.name, &key);
            }
        }
        Ok(loader)
    }

    /// This target for a run that asks no supplier (`--offline`, issue #803): the project's
    /// supply asks no provider that would use the network. A target with no project has no
    /// supply, so nothing changes for it.
    pub fn with_offline(mut self, offline: bool) -> Target {
        self.project = self.project.map(|p| p.with_offline(offline));
        self
    }

    /// The KiCad libraries the target's boards are written with ([`Project::libraries`]); with
    /// no project, the standard mapping and the installed libraries.
    pub fn libraries(&self) -> Result<netsmith_pipeline::Libraries, String> {
        match &self.project {
            Some(project) => project.libraries(),
            None => netsmith_pipeline::Libraries::for_project(None),
        }
    }

    /// Whether the run asks no supplier ([`Target::with_offline`]).
    pub fn offline(&self) -> bool {
        self.project.as_ref().is_some_and(|p| p.offline)
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
        // The pin file of a source (ADR 0101) gives its requests their pins.
        paths.extend(
            self.files
                .iter()
                .map(|f| crate::pins::pins_path(&f.path))
                .filter(|p| p.is_file()),
        );
        // The pick lock of a source (ADR 0100) is shown by the editor, which says whether it is
        // current: a change to it, or its first write, changes what is shown.
        paths.extend(
            self.files
                .iter()
                .map(|f| netsmith_pipeline::pick_lock::lock_path(&f.path))
                .filter(|p| p.is_file()),
        );
        if let Some(project) = &self.project {
            paths.push(project.root.join(MANIFEST));
            let mapping = project.root.join(netsmith_kicad::PROJECT_FILE);
            if mapping.is_file() {
                paths.push(mapping);
            }
            paths.extend(project.catalog_paths());
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
    #[test]
    fn a_stable_name_is_the_path_from_the_root_or_the_file_name() {
        let file = SourceFile {
            name: "../led.nsm".to_string(),
            path: PathBuf::from("../led.nsm"),
        };
        // No project: the file's name, from whatever folder it was named.
        assert_eq!(file.stable_name(None), "led.nsm");
    }

    use super::*;

    fn parse(text: &str) -> Result<Manifest, String> {
        Manifest::parse(text)
    }

    #[test]
    fn the_nexar_table_holds_two_variable_names_and_renders_back() {
        let text = "[project]\nname = \"x\"\n\n[supply.nexar]\nclient_id_env = \"NEXAR_CLIENT_ID\"\nclient_secret_env = \"NEXAR_CLIENT_SECRET\"\n";
        let m = parse(text).unwrap();
        let nexar = m.supply.as_ref().and_then(|s| s.nexar.as_ref()).unwrap();
        assert_eq!(nexar.client_id_env, "NEXAR_CLIENT_ID");
        let rendered = m.render();
        assert!(rendered.contains("[supply.nexar]"), "{rendered}");
        assert_eq!(parse(&rendered).unwrap(), m);
        for (bad, says) in [
            (
                "client_id_env = \"A\"\nclient_secret_env = \"a1b2-c3d4\"\n",
                "client_secret_env is not the name of an environment variable",
            ),
            (
                "client_id_env = \"A\"\nclient_secret_env = \"A\"\n",
                "name the same variable",
            ),
            ("client_id_env = \"A\"\n", "client_secret_env"),
        ] {
            let e =
                parse(&format!("[project]\nname = \"x\"\n\n[supply.nexar]\n{bad}")).unwrap_err();
            assert!(e.contains(says), "{e}");
            assert!(!e.contains("a1b2-c3d4"), "{e}");
        }
    }

    #[test]
    fn the_digikey_table_holds_variable_names_with_an_optional_account_and_renders_back() {
        let text = "[project]\nname = \"x\"\n\n[supply.digikey]\nclient_id_env = \"DIGIKEY_CLIENT_ID\"\nclient_secret_env = \"DIGIKEY_CLIENT_SECRET\"\naccount_id_env = \"DIGIKEY_ACCOUNT_ID\"\n";
        let m = parse(text).unwrap();
        let dk = m.supply.as_ref().and_then(|s| s.digikey.as_ref()).unwrap();
        assert_eq!(dk.account_id_env.as_deref(), Some("DIGIKEY_ACCOUNT_ID"));
        let rendered = m.render();
        assert!(rendered.contains("[supply.digikey]"), "{rendered}");
        assert_eq!(parse(&rendered).unwrap(), m);
        for (bad, says) in [
            (
                "client_id_env = \"A\"\nclient_secret_env = \"B\"\naccount_id_env = \"98765-x\"\n",
                "account_id_env is not the name of an environment variable",
            ),
            (
                "client_id_env = \"A\"\nclient_secret_env = \"B\"\naccount_id_env = \"A\"\n",
                "client_id_env and account_id_env name the same variable",
            ),
            ("client_id_env = \"A\"\n", "client_secret_env"),
            (
                "client_id_env = \"A\"\nclient_secret_env = \"B\"\nsite = \"US\"\n",
                "unknown field",
            ),
        ] {
            let e = parse(&format!(
                "[project]\nname = \"x\"\n\n[supply.digikey]\n{bad}"
            ))
            .unwrap_err();
            assert!(e.contains(says), "{e}");
            assert!(!e.contains("98765-x"), "{e}");
        }
    }

    #[test]
    fn a_manifest_renders_and_reads_back() {
        let m = Manifest::new("blinky");
        assert_eq!(parse(&m.render()), Ok(m));
    }

    #[test]
    fn revision_is_one_line_of_at_most_32_characters() {
        let m = parse("[project]\nname = \"a\"\nrevision = \"1.2\"\n").unwrap();
        assert_eq!(m.revision.as_deref(), Some("1.2"));
        assert_eq!(parse(&m.render()), Ok(m));
        assert_eq!(parse("[project]\nname = \"a\"\n").unwrap().revision, None);
        assert!(!Manifest::new("a").render().contains("revision"));
        let long = "x".repeat(33);
        let e = parse(&format!("[project]\nname = \"a\"\nrevision = \"{long}\"\n")).unwrap_err();
        assert!(
            e.starts_with("netsmith.toml:3: revision ") && e.contains("is 33 characters"),
            "{e}"
        );
        let e = parse("[project]\nname = \"a\"\nrevision = \"A\\nB\"\n").unwrap_err();
        assert!(
            e.starts_with("netsmith.toml:3:") && e.contains("one line"),
            "{e}"
        );
        let e = parse("[project]\nname = \"a\"\nrevision = \"  \"\n").unwrap_err();
        assert!(e.contains("is empty"), "{e}");
    }

    #[test]
    fn pins_is_auto_or_locked() {
        for word in ["auto", "locked"] {
            let m = parse(&format!("[project]\nname = \"a\"\npins = \"{word}\"\n")).unwrap();
            assert_eq!(m.pins.as_deref(), Some(word));
            assert_eq!(parse(&m.render()), Ok(m));
        }
        let e = parse("[project]\nname = \"a\"\npins = \"free\"\n").unwrap_err();
        assert!(
            e.contains("pins = \"free\" is not one of \"auto\" or \"locked\""),
            "{e}"
        );
    }

    #[test]
    fn track_margin_is_from_one_to_four() {
        let m = parse("[project]\nname = \"a\"\ntrack_margin = 1.5\n").unwrap();
        assert_eq!(m.track_margin, Some(1.5));
        assert_eq!(parse(&m.render()), Ok(m));
        let e = parse("[project]\nname = \"a\"\ntrack_margin = 0.5\n").unwrap_err();
        assert!(e.contains("track_margin = 0.5 must be from 1"), "{e}");
    }

    #[test]
    fn rip_rounds_is_a_count_and_zero_turns_the_rounds_off() {
        let m = parse("[project]\nname = \"a\"\nrip_rounds = 0\n").unwrap();
        assert_eq!(m.rip_rounds, Some(0));
        assert_eq!(parse(&m.render()), Ok(m));
        assert_eq!(parse("[project]\nname = \"a\"\n").unwrap().rip_rounds, None);
        assert!(parse("[project]\nname = \"a\"\nrip_rounds = -1\n").is_err());
        assert!(parse("[project]\nname = \"a\"\nrip_rounds = \"many\"\n").is_err());
    }

    #[test]
    fn place_budget_is_read_and_must_be_a_time_or_tries() {
        let m = parse("[project]\nname = \"a\"\nplace_budget = \"90s+3tries\"\n").unwrap();
        assert_eq!(m.place_budget.as_deref(), Some("90s+3tries"));
        assert_eq!(
            parse("[project]\nname = \"a\"\nplace_budget = \"soon\"\n").unwrap_err(),
            "netsmith.toml: place_budget = \"soon\": place budget `soon`: write seconds (`120`, `120s`, `2m`), tries (`5tries`), or both (`120s+5tries`)"
        );
    }

    #[test]
    fn lock_is_all_top_level_or_none() {
        for word in ["all", "top_level", "none"] {
            let m = parse(&format!("[project]\nname = \"a\"\nlock = \"{word}\"\n")).unwrap();
            assert_eq!(m.lock.as_deref(), Some(word));
            assert_eq!(parse(&m.render()).unwrap(), m);
        }
        assert_eq!(parse("[project]\nname = \"a\"\n").unwrap().lock, None);
        assert_eq!(
            parse("[project]\nname = \"a\"\nlock = \"boards\"\n").unwrap_err(),
            "netsmith.toml: lock = \"boards\" is not one of \"all\", \"top_level\" or \"none\""
        );
    }

    #[test]
    fn place_seeds_is_read_and_is_from_one_to_thirty_two() {
        let m = parse("[project]\nname = \"a\"\nplace_seeds = 12\n").unwrap();
        assert_eq!(m.place_seeds, Some(12));
        assert_eq!(parse(&m.render()), Ok(m));
        assert_eq!(
            parse("[project]\nname = \"a\"\nplace_seeds = 32\n")
                .unwrap()
                .place_seeds,
            Some(32)
        );
        for n in [0, 33] {
            assert_eq!(
                parse(&format!("[project]\nname = \"a\"\nplace_seeds = {n}\n")).unwrap_err(),
                format!(
                    "netsmith.toml: place_seeds = {n} must be from 1 (the layout file's seed only) to 32"
                )
            );
        }
    }

    #[test]
    fn only_the_name_is_required() {
        let m = parse("[project]\nname = \"a\"\n").unwrap();
        assert_eq!(
            m,
            Manifest {
                netsmith: None,
                ..Manifest::new("a")
            }
        );
    }

    #[test]
    fn a_new_manifest_is_written_for_this_release() {
        let text = Manifest::new("a").render();
        assert!(
            text.contains(&format!(
                "name = \"a\"\nnetsmith = \"{}\"\n",
                crate::release::RELEASE
            )),
            "{text}"
        );
        assert_eq!(parse(&text).unwrap(), Manifest::new("a"));
        assert_eq!(
            parse("[project]\nname = \"a\"\nnetsmith = \"0.1\"\n").unwrap_err(),
            format!(
                "netsmith.toml: netsmith = \"0.1\" is not a release such as \"{}\"",
                crate::release::RELEASE
            )
        );
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
    fn the_default_assembly_is_fab_hand_none_or_consigned() {
        let with = |word: &str| parse(&format!("[project]\nname = \"a\"\nassembly = \"{word}\"\n"));
        for word in ["fab", "hand", "none", "consigned"] {
            let m = with(word).unwrap();
            assert_eq!(m.assembly.as_deref(), Some(word));
            assert_eq!(parse(&m.render()), Ok(m));
        }
        let e = with("solder").unwrap_err();
        assert!(
            e.contains("assembly = \"solder\"")
                && e.contains("\"fab\", \"hand\", \"none\" or \"consigned\""),
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

    #[test]
    fn catalog_dirs_default_to_the_catalogs_folder_and_survive_a_round_trip() {
        assert_eq!(
            parse("[project]\nname = \"a\"\n").unwrap().catalog_dirs,
            ["catalogs"]
        );
        // The scaffold writes the default out, beside `top_level`.
        let text = Manifest::new("a").render();
        assert!(text.contains("catalog_dirs = [\"catalogs\"]"), "{text}");
        assert!(text.contains("top_level = [\"src/main.nsm\"]"), "{text}");
        // An empty list reads no folder, and is not the same as leaving the key out.
        let none = parse("[project]\nname = \"a\"\ncatalog_dirs = []\n").unwrap();
        assert!(none.catalog_dirs.is_empty());
        assert_eq!(Manifest::parse(&none.render()).unwrap(), none);
        let two = parse("[project]\nname = \"a\"\ncatalog_dirs = [\"catalogs\", \"../shared\"]\n")
            .unwrap();
        assert_eq!(two.catalog_dirs, ["catalogs", "../shared"]);
        assert_eq!(Manifest::parse(&two.render()).unwrap(), two);
    }

    #[test]
    fn a_catalog_dirs_entry_is_a_relative_folder() {
        let with = |line: &str| parse(&format!("[project]\nname = \"a\"\n{line}\n")).unwrap_err();
        for line in [
            "catalog_dirs = [\"/etc\"]",
            "catalog_dirs = [\"\"]",
            "catalog_dirs = [\".\"]",
        ] {
            let e = with(line);
            assert!(
                e.contains("catalog_dirs")
                    && e.contains("must be a folder relative to the project"),
                "{line}: {e}"
            );
        }
        let e = with("catalog_dirs = [\"catalogs\", \"./catalogs/\"]");
        assert!(e.contains("lists the folder \"./catalogs/\" twice"), "{e}");
        assert!(
            parse("[project]\nname = \"a\"\ncatalog_dirs = [\"../shared\", \"./x\"]\n").is_ok()
        );
    }

    #[test]
    fn git_and_version_dependencies_are_read_and_survive_a_round_trip() {
        let text = "[project]\nname = \"a\"\n\n[dependencies]\nradio = { git = \"git@example.invalid:radio.git\", version = \"1.2\" }\nshared = { git = \"https://example.invalid/shared.git\", rev = \"main\" }\ntools = { git = \"/srv/git/tools\", rev = \"v3\" }\n";
        let m = parse(text).unwrap();
        assert_eq!(m.dependencies["shared"].rev.as_deref(), Some("main"));
        assert_eq!(m.dependencies["radio"].version.as_deref(), Some("1.2"));
        assert_eq!(parse(&m.render()).unwrap(), m);
    }

    #[test]
    fn an_environment_may_name_a_definition_of_a_dependency_by_its_module() {
        let with = |environment: &str| {
            parse(&format!(
                "[project]\nname = \"a\"\nenvironment = \"{environment}\"\n\n[dependencies]\nshared = {{ path = \"../shared\" }}\n"
            ))
        };
        assert_eq!(
            with("shared::env::Lab").unwrap().environment.as_deref(),
            Some("shared::env::Lab")
        );
        assert!(
            with("shared::Lab")
                .unwrap_err()
                .contains("not a module of the dependency")
        );
        assert!(
            with("other::env::Lab")
                .unwrap_err()
                .contains("names neither")
        );
    }

    #[test]
    fn dependencies_are_read_by_path_and_survive_a_round_trip() {
        let m =
            parse("[project]\nname = \"a\"\n\n[dependencies]\nshared = { path = \"../shared\" }\n")
                .unwrap();
        assert_eq!(m.dependencies["shared"].path.as_deref(), Some("../shared"));
        assert_eq!(parse(&m.render()).unwrap(), m);
        assert!(!Manifest::new("a").render().contains("dependencies"));
    }

    #[test]
    fn a_dependency_is_a_folder_under_a_name_an_import_can_start_with() {
        let with = |entry: &str| {
            parse(&format!(
                "[project]\nname = \"a\"\n\n[dependencies]\n{entry}\n"
            ))
            .unwrap_err()
        };
        for name in ["src", "std", "\"my-lib\"", "\"2lib\""] {
            let e = with(&format!("{name} = {{ path = \"../x\" }}"));
            assert!(e.contains("not a name an import can start with"), "{e}");
        }
        assert!(with("shared = { version = \"1.0.0\" }").contains("there is no registry"));
        assert!(
            with("shared = { git = \"https://example.invalid/s.git\" }").contains("needs `rev`")
        );
        assert!(
            with(
                "shared = { git = \"https://example.invalid/s.git\", rev = \"a\", version = \"1\" }"
            )
            .contains("both `rev` and `version`")
        );
        assert!(
            with("shared = { path = \"../s\", git = \"https://example.invalid/s.git\" }")
                .contains("both `path` and `git`")
        );
        assert!(
            with("shared = { path = \"../s\", rev = \"main\" }").contains("a `path` dependency")
        );
        for url in ["../shared", "shared.git", "-uupload-pack=x"] {
            let e = with(&format!("shared = {{ git = \"{url}\", rev = \"main\" }}"));
            assert!(e.contains("is not a repository URL"), "{url}: {e}");
        }
        assert!(
            with("shared = { git = \"https://example.invalid/s.git\", rev = \"--all\" }")
                .contains("not a branch, tag or commit")
        );
        assert!(
            with("shared = { git = \"https://example.invalid/s.git\", version = \"one\" }")
                .contains("not a version requirement")
        );
        assert!(with("shared = {}").contains("needs `path`"));
        assert!(with("shared = { path = \"/abs\" }").contains("[dependencies] shared path"));
        assert!(with("shared = { paht = \"../x\" }").contains("unknown field `paht`"));
    }
}
