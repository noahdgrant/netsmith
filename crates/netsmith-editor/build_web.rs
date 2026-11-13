//! The web app's build: find a Node, install the packages, run the build, and say exactly why
//! when any step fails. Used by `build.rs` and, through `#[path]`, by `tests/build_web.rs`, which
//! runs it against fake `node` and `npm` scripts.
//!
//! Two cargo builds with different target directories share one `web/`, so every step that
//! writes `node_modules` or `dist` happens while holding an exclusive lock on `web/.build.lock`;
//! a second build waits for the first and then finds a fresh bundle. A finished bundle carries
//! a marker file, so a `dist` that a killed or failed build left half written is never taken
//! for a bundle.
//!
//! Whether a bundle is up to date is decided by a hash of the sources' bytes ([`source_hash`]),
//! never by file times: a checkout, a package and a copy all give files arbitrary times. No
//! built page is committed (ADR 0114): [`provide`] builds it from the sources when Node is
//! there, and says why when it cannot, and `build.rs` then embeds a placeholder page instead.

use std::{
    env,
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

/// The oldest Node the web app builds with.
pub const NODE_MAJOR: u32 = 20;

/// Written into `dist` when a build finished, and absent while one runs or after one failed. It
/// holds the [`source_hash`] of the sources the bundle was built from.
pub const DONE_MARKER: &str = ".netsmith-built";

/// Written into `node_modules` when `npm ci` finished: the Node major and a hash of the lockfile
/// it installed from.
pub const INSTALL_STAMP: &str = ".netsmith-installed";

/// How long a build waits for another build of the same `web/` before it gives up.
pub const LOCK_WAIT: Duration = Duration::from_secs(15 * 60);

/// How many lines of a failed command's output the reason keeps.
const TAIL_LINES: usize = 15;

/// The sources a bundle is built from, relative to `web/`.
pub const SOURCES: [&str; 6] = [
    "src",
    "index.html",
    "package.json",
    "package-lock.json",
    "tsconfig.json",
    "vite.config.ts",
];

/// A Node and its npm that are new enough.
#[derive(Debug, Clone)]
pub struct Tools {
    pub dir: PathBuf,
    pub node_version: String,
    pub npm_version: String,
}

/// The directories to look for `node` in: `PATH`, then the newest of `~/.nvm/versions/node/*/bin`.
pub fn search_dirs(path: Option<OsString>, home: Option<OsString>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = path
        .map(|p| env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = home {
        let versions = PathBuf::from(home).join(".nvm/versions/node");
        let mut found: Vec<PathBuf> = fs::read_dir(versions)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path().join("bin"))
            .collect();
        // Newest first: `v22.23.4` before `v20.13.1`.
        found.sort_by_key(|p| std::cmp::Reverse(version_of(p)));
        dirs.extend(found);
    }
    dirs
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn npm_exe() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}

fn run_version(program: &Path, path: Option<&OsString>) -> Option<String> {
    // A program that was written a moment ago can be busy while another thread forks.
    for _ in 0..20 {
        let mut command = Command::new(program);
        command.arg("--version");
        if let Some(path) = path {
            command.env("PATH", path);
        }
        match command.output() {
            Ok(out) => {
                return out
                    .status
                    .success()
                    .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string());
            }
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => return None,
        }
    }
    None
}

/// The first directory with a `node` of at least [`NODE_MAJOR`] and an `npm` beside it. The
/// error says what was looked at.
pub fn find_node(dirs: &[PathBuf]) -> Result<Tools, String> {
    let mut seen = Vec::new();
    for dir in dirs {
        let node = dir.join(exe("node"));
        if !node.is_file() {
            continue;
        }
        let Some(version) = run_version(&node, None) else {
            seen.push(format!("{} did not report a version", node.display()));
            continue;
        };
        if major(&version) < NODE_MAJOR {
            seen.push(format!("{} is {version}, too old", node.display()));
            continue;
        }
        let path = join_path(dir);
        let Some(npm_version) = run_version(&dir.join(npm_exe()), path.as_ref()) else {
            seen.push(format!(
                "{} is {version} but there is no working npm beside it",
                node.display()
            ));
            continue;
        };
        return Ok(Tools {
            dir: dir.clone(),
            node_version: version,
            npm_version,
        });
    }
    let mut reason = format!(
        "Node {NODE_MAJOR} or newer (with npm) was not found on PATH or in ~/.nvm/versions/node ({} directories searched)",
        dirs.len()
    );
    for line in seen {
        reason.push_str("\n  ");
        reason.push_str(&line);
    }
    Err(reason)
}

pub fn major(version: &str) -> u32 {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// `.../node/v22.23.4/bin` as numbers, to order versions.
fn version_of(bin: &Path) -> Vec<u32> {
    bin.parent()
        .and_then(|p| p.file_name())
        .map(|n| {
            n.to_string_lossy()
                .trim_start_matches('v')
                .split('.')
                .filter_map(|x| x.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// `PATH` with `first` in front of the current one.
fn join_path(first: &Path) -> Option<OsString> {
    let rest = env::var_os("PATH").unwrap_or_default();
    env::join_paths(std::iter::once(first.to_path_buf()).chain(env::split_paths(&rest))).ok()
}

/// Whether `dist` is a finished bundle: the page is there, and every file it names under
/// `assets/` is.
pub fn page_is_complete(dist: &Path) -> bool {
    let Ok(page) = fs::read_to_string(dist.join("index.html")) else {
        return false;
    };
    let mut rest = page.as_str();
    while let Some(at) = rest.find("assets/") {
        rest = &rest[at..];
        let end = rest
            .find(|c: char| c == '"' || c == '\'' || c == ')' || c.is_whitespace())
            .unwrap_or(rest.len());
        if !dist.join(&rest[..end]).is_file() {
            return false;
        }
        rest = &rest[end..];
    }
    true
}

/// Whether `dist` is a finished bundle of a build: the marker is there and the page is complete.
pub fn bundle_is_complete(dist: &Path) -> bool {
    dist.join(DONE_MARKER).is_file() && page_is_complete(dist)
}

/// Every file under `path`, as `/`-separated names relative to `root`, in byte order.
fn files_under(root: &Path, path: &Path, out: &mut Vec<String>) {
    if path.is_dir() {
        for entry in fs::read_dir(path).into_iter().flatten().flatten() {
            files_under(root, &entry.path(), out);
        }
    } else if let Ok(relative) = path.strip_prefix(root) {
        out.push(
            relative
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
        );
    }
}

/// One step of FNV-1a over `bytes`.
fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// A hash of the sources of the bundle: the name and the bytes of every file of [`SOURCES`], in
/// byte order of the names. It depends on nothing else (not on file times, not on the place of
/// the checkout), so a copy of the sources has the same hash. FNV-1a is fixed for all Rust
/// versions; the hash is there to notice a change, not to resist an attack.
pub fn source_hash(web: &Path) -> String {
    let mut names = Vec::new();
    for source in SOURCES {
        files_under(web, &web.join(source), &mut names);
    }
    names.sort();
    let mut hash: u64 = 0xcbf29ce484222325;
    for name in &names {
        let bytes = fs::read(web.join(name)).unwrap_or_default();
        hash = fnv(hash, name.as_bytes());
        hash = fnv(hash, &[0]);
        hash = fnv(hash, &(bytes.len() as u64).to_le_bytes());
        hash = fnv(hash, &bytes);
    }
    format!("{hash:016x}")
}

/// Whether the bundle must be built: it is not complete, or it was built from other sources
/// than the ones there are now.
pub fn is_stale(web: &Path) -> bool {
    let dist = web.join("dist");
    if !bundle_is_complete(&dist) {
        return true;
    }
    match fs::read_to_string(dist.join(DONE_MARKER)) {
        Ok(built_from) => built_from.trim() != source_hash(web),
        Err(_) => true,
    }
}

/// A hash of a file's bytes (FNV-1a: it must not change between Rust versions).
fn fingerprint(path: &Path) -> String {
    format!(
        "{:016x}",
        fnv(0xcbf29ce484222325, &fs::read(path).unwrap_or_default())
    )
}

fn install_stamp(web: &Path, tools: &Tools) -> String {
    format!(
        "node {} lock {}",
        major(&tools.node_version),
        fingerprint(&web.join("package-lock.json"))
    )
}

/// Why `node_modules` cannot be used as it is, or `None` if it can: it was installed by `npm ci`
/// of this lockfile under this Node major, and the programs the build runs are there.
pub fn install_problem(web: &Path, tools: &Tools) -> Option<String> {
    let modules = web.join("node_modules");
    if !modules.is_dir() {
        return Some("node_modules is missing".into());
    }
    match fs::read_to_string(modules.join(INSTALL_STAMP)) {
        Ok(stamp) if stamp.trim() == install_stamp(web, tools) => {}
        Ok(stamp) => {
            return Some(format!(
                "node_modules was installed as `{}` and this is `{}`",
                stamp.trim(),
                install_stamp(web, tools)
            ));
        }
        Err(_) => return Some("node_modules was not installed by this build".into()),
    }
    for program in ["tsc", "vite"] {
        if !modules.join(".bin").join(program).exists() {
            return Some(format!("node_modules/.bin/{program} is missing"));
        }
    }
    None
}

/// The last lines of what a command wrote.
fn tail(stdout: &[u8], stderr: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(stderr));
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
}

/// Runs `npm <args>` in `web` with the Node of `tools` first on `PATH`. The error is the whole
/// reason: the command, where it ran, which Node and npm, the start of `PATH`, how it ended and
/// the end of its output.
fn npm(web: &Path, tools: &Tools, args: &[&str]) -> Result<(), String> {
    let path = join_path(&tools.dir);
    let mut command = Command::new(tools.dir.join(npm_exe()));
    command.args(args).current_dir(web);
    if let Some(path) = &path {
        command.env("PATH", path);
    }
    // Cargo sets some variables a build tool reads (`DEBUG`, `TARGET`, `HOST`), and a shell
    // with `NODE_ENV=production` makes `npm ci` leave out the compiler.
    for name in [
        "NODE_ENV",
        "DEBUG",
        "TARGET",
        "HOST",
        "PROFILE",
        "OPT_LEVEL",
    ] {
        command.env_remove(name);
    }
    let command_line = format!("npm {}", args.join(" "));
    let out = command.output().map_err(|e| {
        format!(
            "`{command_line}` could not be started: {e}{}",
            context(web, tools, path.as_ref())
        )
    })?;
    if out.status.success() {
        return Ok(());
    }
    let how = match out.status.code() {
        Some(code) => format!("failed with exit code {code}"),
        None => "was stopped by a signal".to_string(),
    };
    Err(format!(
        "`{command_line}` {how}{}\nlast lines of its output:\n{}",
        context(web, tools, path.as_ref()),
        tail(&out.stdout, &out.stderr)
    ))
}

fn context(web: &Path, tools: &Tools, path: Option<&OsString>) -> String {
    let head: Vec<String> = path
        .map(|p| {
            env::split_paths(p)
                .take(3)
                .map(|d| d.display().to_string())
                .collect()
        })
        .unwrap_or_default();
    format!(
        "\n  in {}\n  node {} and npm {} from {}\n  PATH starts {}",
        web.display(),
        tools.node_version,
        tools.npm_version,
        tools.dir.display(),
        head.join(":")
    )
}

/// Takes the exclusive lock on `web/.build.lock`, waiting up to `wait` for another build.
fn lock(web: &Path, wait: Duration) -> Result<File, String> {
    let path = web.join(".build.lock");
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| format!("cannot open the lock file {}: {e}", path.display()))?;
    let step = Duration::from_millis(50);
    let mut waited = Duration::ZERO;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(fs::TryLockError::WouldBlock) if waited < wait => {
                thread::sleep(step);
                waited += step;
            }
            Err(fs::TryLockError::WouldBlock) => {
                return Err(format!(
                    "another build of {} held {} for more than {} seconds",
                    web.display(),
                    path.display(),
                    wait.as_secs()
                ));
            }
            Err(fs::TryLockError::Error(e)) => {
                return Err(format!("cannot lock {}: {e}", path.display()));
            }
        }
    }
}

/// What `ensure` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Done {
    /// The bundle was already fresh.
    Fresh,
    /// The bundle was built (and the packages installed if `installed`).
    Built { installed: bool },
}

/// Makes `web/dist` a finished, fresh bundle, with `lock_wait` as the longest it waits for a
/// concurrent build. The error is the reason it could not, for the warning and for the message
/// `netsmith editor` prints.
pub fn ensure(web: &Path, dirs: &[PathBuf], lock_wait: Duration) -> Result<Done, String> {
    let _lock = lock(web, lock_wait)?;
    // Checked under the lock: a build that held it a moment ago has just made it fresh.
    if !is_stale(web) {
        return Ok(Done::Fresh);
    }
    let tools = find_node(dirs)?;
    let dist = web.join("dist");
    let _ = fs::remove_file(dist.join(DONE_MARKER));

    let mut installed = false;
    if let Some(why) = install_problem(web, &tools) {
        let _ = fs::remove_file(web.join("node_modules").join(INSTALL_STAMP));
        npm(
            web,
            &tools,
            &["ci", "--include=dev", "--no-audit", "--no-fund"],
        )
        .map_err(|e| format!("installing the packages ({why}) failed: {e}"))?;
        fs::write(
            web.join("node_modules").join(INSTALL_STAMP),
            install_stamp(web, &tools),
        )
        .map_err(|e| format!("cannot write the install stamp: {e}"))?;
        installed = true;
    }
    npm(web, &tools, &["run", "build"])?;
    if !dist.join("index.html").is_file() {
        return Err(format!(
            "`npm run build` succeeded but wrote no {}",
            dist.join("index.html").display()
        ));
    }
    fs::write(dist.join(DONE_MARKER), source_hash(web))
        .map_err(|e| format!("cannot mark the bundle as built: {e}"))?;
    if !bundle_is_complete(&dist) {
        let _ = fs::remove_file(dist.join(DONE_MARKER));
        return Err("`npm run build` left a page that names files it did not write".into());
    }
    Ok(Done::Built { installed })
}

/// Copies the finished bundle (`dist`) to `to`, while holding the build lock, so a build that
/// starts next cannot empty `dist` under the copy. `Ok(false)` if there is no finished bundle.
pub fn copy_bundle(web: &Path, to: &Path) -> Result<bool, String> {
    let _lock = lock(web, LOCK_WAIT)?;
    let dist = web.join("dist");
    let _ = fs::remove_dir_all(to);
    if !bundle_is_complete(&dist) {
        return Ok(false);
    }
    copy_dir(&dist, to).map_err(|e| format!("cannot copy the bundle to {}: {e}", to.display()))?;
    Ok(true)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if entry.file_name() != DONE_MARKER {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// Where the page in the binary came from.
#[derive(Debug, PartialEq, Eq)]
pub enum Provided {
    /// Built from the sources (or a fresh build of them was already there).
    Built,
    /// There is no page in the binary. The reason is why.
    Missing(String),
}

/// Puts the page for the sources in `web` into `to`: a build of them, which needs Node, else
/// nothing and the reason.
pub fn provide(web: &Path, dirs: &[PathBuf], lock_wait: Duration, to: &Path) -> Provided {
    let _ = fs::remove_dir_all(to);
    let reason = match ensure(web, dirs, lock_wait) {
        Ok(_) => match copy_bundle(web, to) {
            Ok(true) => return Provided::Built,
            Ok(false) => {
                "the build left no finished bundle in crates/netsmith-editor/web/dist".into()
            }
            Err(e) => e,
        },
        Err(e) => e,
    };
    Provided::Missing(reason)
}
