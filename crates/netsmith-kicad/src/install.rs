//! Where KiCad's own symbol and footprint libraries are on this machine.
//!
//! `build` and `fab` read the stock libraries from a KiCad install (ADR 0021) until the
//! libraries are self-contained (ADR 0041). [`locate`] finds the `symbols` and `footprints`
//! folders, in this order:
//!
//! 1. the environment variable, [`SYMBOLS_VARIABLE`] or [`FOOTPRINTS_VARIABLE`], used as it
//!    is even if the folder is missing (the error then names it);
//! 2. the folder beside a `kicad-cli` (`$KICAD_CLI`, else `kicad-cli` on `PATH`): the
//!    install's `share/kicad` next to its `bin`, or `SharedSupport` next to `MacOS` in a
//!    macOS app bundle;
//! 3. the usual install places of each system, KiCad 9 before 8 (netsmith is tested with
//!    KiCad 9): see [`Search::usual_places`].
//!
//! A folder counts only if it holds what the libraries are made of: a `*.kicad_sym` file for
//! symbols, a `*.pretty` folder for footprints. A [`Search`] reads the machine; tests build
//! one over a temporary tree and give it the variables, so they run anywhere.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The environment variable that names the folder holding the `.kicad_sym` libraries.
pub const SYMBOLS_VARIABLE: &str = "NETSMITH_KICAD_SYMBOLS";

/// The environment variable that names the folder holding the `.pretty` libraries.
pub const FOOTPRINTS_VARIABLE: &str = "NETSMITH_KICAD_FOOTPRINTS";

/// The environment variable that names the `kicad-cli` program.
pub const CLI_VARIABLE: &str = "KICAD_CLI";

/// The KiCad versions looked for in the install places that carry one, the preferred first.
pub const VERSIONS: [&str; 2] = ["9.0", "8.0"];

/// Which of KiCad's two library folders is wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Symbols,
    Footprints,
}

impl Kind {
    pub const fn variable(self) -> &'static str {
        match self {
            Kind::Symbols => SYMBOLS_VARIABLE,
            Kind::Footprints => FOOTPRINTS_VARIABLE,
        }
    }

    /// The folder's name in an install's `share/kicad`.
    const fn folder(self) -> &'static str {
        match self {
            Kind::Symbols => "symbols",
            Kind::Footprints => "footprints",
        }
    }

    const fn noun(self) -> &'static str {
        match self {
            Kind::Symbols => "symbol",
            Kind::Footprints => "footprint",
        }
    }

    /// What the folder must hold.
    const fn expects(self) -> &'static str {
        match self {
            Kind::Symbols => "*.kicad_sym files",
            Kind::Footprints => "*.pretty folders",
        }
    }

    /// Whether `dir` holds what this kind of library folder holds.
    fn holds_libraries(self, dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let path = entry.path();
            match self {
                Kind::Symbols => {
                    path.extension().is_some_and(|e| e == "kicad_sym") && path.is_file()
                }
                Kind::Footprints => {
                    path.extension().is_some_and(|e| e == "pretty") && path.is_dir()
                }
            }
        })
    }
}

/// How a folder was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// From the environment variable.
    Variable,
    /// Beside this `kicad-cli`.
    BesideCli(PathBuf),
    /// One of the usual install places; the text names the system.
    Usual(&'static str),
}

/// A folder of libraries and how it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub dir: PathBuf,
    pub source: Source,
}

/// One place looked at, and what was there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tried {
    pub place: String,
    /// The folder, for a place that is one.
    pub dir: Option<PathBuf>,
    pub result: String,
}

/// Nothing was found: every place looked at, for the error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotFound {
    pub kind: Kind,
    pub tried: Vec<Tried>,
}

impl NotFound {
    /// The first folder tried, for code that wants some path to show.
    pub fn first_dir(&self) -> Option<&Path> {
        self.tried.iter().find_map(|t| t.dir.as_deref())
    }
}

impl std::fmt::Display for NotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "no KiCad {} libraries found (a folder with {}); looked in:",
            self.kind.noun(),
            self.kind.expects()
        )?;
        for t in &self.tried {
            write!(f, "\n  {}: {}", t.place, t.result)?;
        }
        write!(
            f,
            "\nset {SYMBOLS_VARIABLE} to the folder that holds the .kicad_sym libraries and \
             {FOOTPRINTS_VARIABLE} to the folder that holds the .pretty libraries \
             (netsmith is tested with KiCad 9)"
        )
    }
}

impl std::error::Error for NotFound {}

/// The machine to look at: the environment and the places on disk.
#[derive(Debug, Clone)]
pub struct Search {
    /// Prefixed to the fixed absolute places (`/usr/share/kicad`, `/mnt/c/...`): `/` for
    /// the real machine, a temporary folder in a test.
    root: PathBuf,
    vars: HashMap<String, OsString>,
    /// The directories of `PATH`.
    path: Vec<PathBuf>,
    /// The user's home folder, for the per-user flatpak.
    home: Option<PathBuf>,
    /// `%ProgramFiles%`.
    program_files: Option<PathBuf>,
}

impl Search {
    /// This process's environment and the real disk.
    pub fn machine() -> Self {
        let var = |name: &str| std::env::var_os(name);
        let mut vars = HashMap::new();
        for name in [SYMBOLS_VARIABLE, FOOTPRINTS_VARIABLE, CLI_VARIABLE] {
            if let Some(value) = var(name) {
                vars.insert(name.to_string(), value);
            }
        }
        Self {
            root: PathBuf::from("/"),
            vars,
            path: var("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            home: var("HOME")
                .or_else(|| var("USERPROFILE"))
                .map(PathBuf::from),
            program_files: var("ProgramFiles")
                .map(PathBuf::from)
                .or_else(|| cfg!(windows).then(|| PathBuf::from(r"C:\Program Files"))),
        }
    }

    /// An empty machine whose fixed places are under `root`: no variables, no `PATH`, no
    /// home. The `with_` methods add to it.
    pub fn under(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            vars: HashMap::new(),
            path: Vec::new(),
            home: None,
            program_files: None,
        }
    }

    pub fn with_var(mut self, name: &str, value: impl Into<OsString>) -> Self {
        self.vars.insert(name.to_string(), value.into());
        self
    }

    pub fn with_path(mut self, dirs: Vec<PathBuf>) -> Self {
        self.path = dirs;
        self
    }

    pub fn with_home(mut self, home: impl Into<PathBuf>) -> Self {
        self.home = Some(home.into());
        self
    }

    pub fn with_program_files(mut self, dir: impl Into<PathBuf>) -> Self {
        self.program_files = Some(dir.into());
        self
    }

    /// A fixed absolute place, under the search's root.
    fn fixed(&self, absolute: &str) -> PathBuf {
        self.root.join(absolute.trim_start_matches('/'))
    }

    /// `kicad-cli`: `$KICAD_CLI` if it is set (and nothing else then), else the first
    /// `kicad-cli` (or `kicad-cli.exe`) on `PATH`. `None` if it is not there.
    pub fn kicad_cli(&self) -> Option<PathBuf> {
        if let Some(path) = self.vars.get(CLI_VARIABLE) {
            return Some(PathBuf::from(path)).filter(|p| p.is_file());
        }
        self.path
            .iter()
            .flat_map(|d| [d.join("kicad-cli"), d.join("kicad-cli.exe")])
            .find(|p| p.is_file())
    }

    /// The install places that are not found through a program, for every system in turn:
    /// Linux, snap, flatpak, macOS, Windows, and Windows seen from WSL, each with the
    /// KiCad versions in [`VERSIONS`] order where the place carries one. The text is the
    /// system.
    ///
    /// Verified on a real install: only Windows KiCad 9 seen from WSL. The Linux, macOS,
    /// Windows, snap and flatpak places are the layouts the packages are known to use and
    /// have not been checked on a real install; the snap and flatpak ones are the least
    /// certain.
    pub fn usual_places(&self, kind: Kind) -> Vec<(&'static str, PathBuf)> {
        let folder = kind.folder();
        let mut places = vec![
            ("Linux", self.fixed("/usr/share/kicad").join(folder)),
            ("Linux", self.fixed("/usr/local/share/kicad").join(folder)),
            (
                "snap",
                self.fixed("/snap/kicad/current/usr/share/kicad")
                    .join(folder),
            ),
        ];
        // A flatpak app's files are under `<installation>/app/<id>/current/active/files`.
        let flatpak = "app/org.kicad.KiCad/current/active/files/share/kicad";
        if let Some(home) = &self.home {
            places.push((
                "flatpak",
                home.join(".local/share/flatpak").join(flatpak).join(folder),
            ));
        }
        places.push((
            "flatpak",
            self.fixed("/var/lib/flatpak").join(flatpak).join(folder),
        ));
        places.push((
            "macOS",
            self.fixed("/Applications/KiCad/KiCad.app/Contents/SharedSupport")
                .join(folder),
        ));
        for version in VERSIONS {
            if let Some(pf) = &self.program_files {
                places.push((
                    "Windows",
                    pf.join("KiCad")
                        .join(version)
                        .join("share")
                        .join("kicad")
                        .join(folder),
                ));
            }
        }
        for version in VERSIONS {
            places.push((
                "WSL",
                self.fixed("/mnt/c/Program Files/KiCad")
                    .join(version)
                    .join("share/kicad")
                    .join(folder),
            ));
        }
        places
    }

    /// The folders next to `cli`, as the installs lay them out: `<prefix>/bin/kicad-cli`
    /// beside `<prefix>/share/kicad`, and `Contents/MacOS/kicad-cli` beside
    /// `Contents/SharedSupport`. The program is looked at as given and, if it is a link,
    /// where it points.
    fn beside(cli: &Path, kind: Kind) -> Vec<PathBuf> {
        let mut programs = vec![cli.to_path_buf()];
        if let Ok(real) = cli.canonicalize()
            && real != cli
        {
            programs.push(real);
        }
        let mut dirs = Vec::new();
        for program in programs {
            let Some(prefix) = program.parent().and_then(Path::parent) else {
                continue;
            };
            for dir in [
                prefix.join("share/kicad").join(kind.folder()),
                prefix.join("SharedSupport").join(kind.folder()),
            ] {
                if !dirs.contains(&dir) {
                    dirs.push(dir);
                }
            }
        }
        dirs
    }

    /// The folder of the libraries of `kind`, or every place looked at.
    pub fn find(&self, kind: Kind) -> Result<Found, NotFound> {
        let mut tried = Vec::new();

        match self.vars.get(kind.variable()) {
            Some(dir) => {
                return Ok(Found {
                    dir: PathBuf::from(dir),
                    source: Source::Variable,
                });
            }
            None => tried.push(Tried {
                place: format!("${}", kind.variable()),
                dir: None,
                result: "not set".to_string(),
            }),
        }

        match self.kicad_cli() {
            Some(cli) => {
                for dir in Self::beside(&cli, kind) {
                    if kind.holds_libraries(&dir) {
                        return Ok(Found {
                            dir,
                            source: Source::BesideCli(cli),
                        });
                    }
                    tried.push(Tried {
                        result: Self::why_not(kind, &dir),
                        place: format!("{} (beside {})", dir.display(), cli.display()),
                        dir: Some(dir),
                    });
                }
            }
            None => tried.push(Tried {
                place: "beside kicad-cli".to_string(),
                dir: None,
                result: match self.vars.get(CLI_VARIABLE) {
                    Some(p) => format!("${CLI_VARIABLE} is {}, which is not a file", p.display()),
                    None => format!("no kicad-cli on PATH, and ${CLI_VARIABLE} is not set"),
                },
            }),
        }

        for (system, dir) in self.usual_places(kind) {
            if kind.holds_libraries(&dir) {
                return Ok(Found {
                    dir,
                    source: Source::Usual(system),
                });
            }
            tried.push(Tried {
                result: Self::why_not(kind, &dir),
                place: format!("{} ({system})", dir.display()),
                dir: Some(dir),
            });
        }
        Err(NotFound { kind, tried })
    }

    fn why_not(kind: Kind, dir: &Path) -> String {
        if dir.is_dir() {
            format!("exists but holds no {}", kind.expects())
        } else {
            "does not exist".to_string()
        }
    }
}

/// The folder of the libraries of `kind` on this machine.
pub fn locate(kind: Kind) -> Result<Found, NotFound> {
    Search::machine().find(kind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree() -> tempfile_dir::Dir {
        tempfile_dir::Dir::new()
    }

    /// A small temporary folder that is removed on drop (no extra dependency).
    mod tempfile_dir {
        use std::path::{Path, PathBuf};

        pub struct Dir(PathBuf);

        impl Dir {
            pub fn new() -> Self {
                use std::sync::atomic::{AtomicU32, Ordering};
                static N: AtomicU32 = AtomicU32::new(0);
                let path = std::env::temp_dir().join(format!(
                    "netsmith-install-{}-{}",
                    std::process::id(),
                    N.fetch_add(1, Ordering::Relaxed)
                ));
                std::fs::create_dir_all(&path).unwrap();
                Dir(path)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for Dir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    /// A fake `share/kicad` at `share`: one symbol library and one footprint library.
    fn install(share: &Path) {
        fs::create_dir_all(share.join("symbols")).unwrap();
        fs::write(share.join("symbols/Device.kicad_sym"), "(kicad_symbol_lib)").unwrap();
        fs::create_dir_all(share.join("footprints/Resistor_SMD.pretty")).unwrap();
    }

    fn program(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }

    fn both(search: &Search) -> (Found, Found) {
        (
            search.find(Kind::Symbols).unwrap(),
            search.find(Kind::Footprints).unwrap(),
        )
    }

    #[test]
    fn a_linux_install_is_found() {
        let t = tree();
        install(&t.path().join("usr/share/kicad"));
        let (s, f) = both(&Search::under(t.path()));
        assert_eq!(s.dir, t.path().join("usr/share/kicad/symbols"));
        assert_eq!(f.dir, t.path().join("usr/share/kicad/footprints"));
        assert_eq!(s.source, Source::Usual("Linux"));
    }

    #[test]
    fn a_local_prefix_install_is_found() {
        let t = tree();
        install(&t.path().join("usr/local/share/kicad"));
        let (s, _) = both(&Search::under(t.path()));
        assert_eq!(s.dir, t.path().join("usr/local/share/kicad/symbols"));
    }

    #[test]
    fn a_snap_install_is_found() {
        let t = tree();
        install(&t.path().join("snap/kicad/current/usr/share/kicad"));
        let (s, _) = both(&Search::under(t.path()));
        assert_eq!(s.source, Source::Usual("snap"));
    }

    #[test]
    fn a_per_user_and_a_system_flatpak_are_found() {
        let t = tree();
        let layout = "app/org.kicad.KiCad/current/active/files/share/kicad";
        install(&t.path().join("home/me/.local/share/flatpak").join(layout));
        let search = Search::under(t.path()).with_home(t.path().join("home/me"));
        assert_eq!(
            search.find(Kind::Symbols).unwrap().source,
            Source::Usual("flatpak")
        );

        let t = tree();
        install(&t.path().join("var/lib/flatpak").join(layout));
        let (s, _) = both(&Search::under(t.path()));
        assert_eq!(s.source, Source::Usual("flatpak"));
    }

    #[test]
    fn a_macos_app_bundle_is_found() {
        let t = tree();
        install(
            &t.path()
                .join("Applications/KiCad/KiCad.app/Contents/SharedSupport"),
        );
        let (s, f) = both(&Search::under(t.path()));
        assert_eq!(s.source, Source::Usual("macOS"));
        assert!(f.dir.ends_with("SharedSupport/footprints"));
    }

    #[test]
    fn a_windows_install_is_found_under_program_files() {
        let t = tree();
        let pf = t.path().join("Program Files");
        install(&pf.join("KiCad/9.0/share/kicad"));
        let search = Search::under(t.path()).with_program_files(&pf);
        let (s, _) = both(&search);
        assert_eq!(s.dir, pf.join("KiCad/9.0/share/kicad/symbols"));
        assert_eq!(s.source, Source::Usual("Windows"));
    }

    #[test]
    fn windows_seen_from_wsl_is_found() {
        let t = tree();
        install(&t.path().join("mnt/c/Program Files/KiCad/9.0/share/kicad"));
        let (s, f) = both(&Search::under(t.path()));
        assert_eq!(s.source, Source::Usual("WSL"));
        assert_eq!(
            f.dir,
            t.path()
                .join("mnt/c/Program Files/KiCad/9.0/share/kicad/footprints")
        );
    }

    #[test]
    fn kicad_9_wins_over_8_and_8_is_used_when_it_is_alone() {
        let t = tree();
        let wsl = t.path().join("mnt/c/Program Files/KiCad");
        install(&wsl.join("8.0/share/kicad"));
        let (s, _) = both(&Search::under(t.path()));
        assert_eq!(s.dir, wsl.join("8.0/share/kicad/symbols"));
        install(&wsl.join("9.0/share/kicad"));
        let (s, _) = both(&Search::under(t.path()));
        assert_eq!(s.dir, wsl.join("9.0/share/kicad/symbols"));
    }

    #[test]
    fn a_folder_without_libraries_does_not_count() {
        let t = tree();
        // The folders exist but are empty, or hold the wrong thing.
        let share = t.path().join("usr/share/kicad");
        fs::create_dir_all(share.join("symbols")).unwrap();
        fs::create_dir_all(share.join("footprints")).unwrap();
        fs::write(share.join("symbols/readme.txt"), "").unwrap();
        fs::write(
            share.join("footprints/Loose.pretty"),
            "a file, not a folder",
        )
        .unwrap();
        let e = Search::under(t.path()).find(Kind::Symbols).unwrap_err();
        let text = e.to_string();
        assert!(
            text.contains("exists but holds no *.kicad_sym files"),
            "{text}"
        );
        let e = Search::under(t.path()).find(Kind::Footprints).unwrap_err();
        assert!(
            e.to_string()
                .contains("exists but holds no *.pretty folders")
        );
    }

    #[test]
    fn a_kicad_cli_on_the_path_leads_to_the_install_beside_it() {
        let t = tree();
        let prefix = t.path().join("opt/kicad-custom");
        install(&prefix.join("share/kicad"));
        program(&prefix.join("bin/kicad-cli"));
        let search = Search::under(t.path()).with_path(vec![prefix.join("bin")]);
        let (s, f) = both(&search);
        assert_eq!(s.dir, prefix.join("share/kicad/symbols"));
        assert_eq!(f.dir, prefix.join("share/kicad/footprints"));
        assert_eq!(s.source, Source::BesideCli(prefix.join("bin/kicad-cli")));
    }

    #[test]
    fn kicad_cli_exe_on_the_path_is_found_too() {
        let t = tree();
        let prefix = t.path().join("KiCad/9.0");
        install(&prefix.join("share/kicad"));
        program(&prefix.join("bin/kicad-cli.exe"));
        let search = Search::under(t.path()).with_path(vec![prefix.join("bin")]);
        assert!(matches!(
            search.find(Kind::Symbols).unwrap().source,
            Source::BesideCli(_)
        ));
    }

    #[test]
    fn the_kicad_cli_variable_leads_to_the_install_beside_it() {
        let t = tree();
        let prefix = t.path().join("elsewhere");
        install(&prefix.join("share/kicad"));
        program(&prefix.join("bin/kicad-cli"));
        let search = Search::under(t.path()).with_var(CLI_VARIABLE, prefix.join("bin/kicad-cli"));
        assert_eq!(
            search.find(Kind::Footprints).unwrap().dir,
            prefix.join("share/kicad/footprints")
        );
    }

    #[test]
    fn a_macos_kicad_cli_leads_to_shared_support() {
        let t = tree();
        let contents = t.path().join("Apps/KiCad.app/Contents");
        install(&contents.join("SharedSupport"));
        program(&contents.join("MacOS/kicad-cli"));
        let search =
            Search::under(t.path()).with_var(CLI_VARIABLE, contents.join("MacOS/kicad-cli"));
        assert_eq!(
            search.find(Kind::Symbols).unwrap().dir,
            contents.join("SharedSupport/symbols")
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_kicad_cli_that_is_a_link_leads_to_where_it_points() {
        let t = tree();
        let prefix = t.path().join("opt/kicad");
        install(&prefix.join("share/kicad"));
        program(&prefix.join("bin/kicad-cli"));
        fs::create_dir_all(t.path().join("usr/bin")).unwrap();
        let link = t.path().join("usr/bin/kicad-cli");
        std::os::unix::fs::symlink(prefix.join("bin/kicad-cli"), &link).unwrap();
        let search = Search::under(t.path()).with_path(vec![t.path().join("usr/bin")]);
        let found = search.find(Kind::Symbols).unwrap();
        assert_eq!(
            found.dir.canonicalize().unwrap(),
            prefix.join("share/kicad/symbols").canonicalize().unwrap()
        );
    }

    #[test]
    fn the_variables_win_over_everything_and_are_taken_as_given() {
        let t = tree();
        install(&t.path().join("usr/share/kicad"));
        let search = Search::under(t.path())
            .with_var(SYMBOLS_VARIABLE, "/my/symbols")
            .with_var(FOOTPRINTS_VARIABLE, t.path().join("not-there"));
        let (s, f) = both(&search);
        assert_eq!((s.dir, s.source), ("/my/symbols".into(), Source::Variable));
        assert_eq!(f.dir, t.path().join("not-there"));
    }

    #[test]
    fn one_variable_does_not_move_the_other_folder() {
        let t = tree();
        install(&t.path().join("usr/share/kicad"));
        let search = Search::under(t.path()).with_var(SYMBOLS_VARIABLE, "/my/symbols");
        assert_eq!(
            search.find(Kind::Footprints).unwrap().source,
            Source::Usual("Linux")
        );
    }

    #[test]
    fn a_kicad_cli_variable_that_points_nowhere_is_not_a_cli_but_the_usual_places_still_count() {
        let t = tree();
        install(&t.path().join("usr/share/kicad"));
        let search = Search::under(t.path()).with_var(CLI_VARIABLE, "/nonexistent/kicad-cli");
        assert_eq!(
            search.find(Kind::Symbols).unwrap().source,
            Source::Usual("Linux")
        );
    }

    #[test]
    fn nothing_found_lists_every_place_and_both_variables() {
        let t = tree();
        let home = t.path().join("home/me");
        let search = Search::under(t.path())
            .with_home(&home)
            .with_program_files(t.path().join("PF"));
        let e = search.find(Kind::Symbols).unwrap_err();
        let text = e.to_string();
        assert!(
            text.starts_with("no KiCad symbol libraries found"),
            "{text}"
        );
        assert!(text.contains("*.kicad_sym files"), "{text}");
        assert!(text.contains("$NETSMITH_KICAD_SYMBOLS: not set"), "{text}");
        assert!(text.contains("no kicad-cli on PATH"), "{text}");
        for place in [
            "usr/share/kicad/symbols",
            "usr/local/share/kicad/symbols",
            "snap/kicad/current/usr/share/kicad/symbols",
            "home/me/.local/share/flatpak/app/org.kicad.KiCad",
            "var/lib/flatpak/app/org.kicad.KiCad",
            "Applications/KiCad/KiCad.app/Contents/SharedSupport/symbols",
            "PF/KiCad/9.0/share/kicad/symbols",
            "PF/KiCad/8.0/share/kicad/symbols",
            "mnt/c/Program Files/KiCad/9.0/share/kicad/symbols",
            "mnt/c/Program Files/KiCad/8.0/share/kicad/symbols",
        ] {
            assert!(text.contains(place), "{place} missing from:\n{text}");
        }
        assert!(text.contains("does not exist"), "{text}");
        assert!(text.contains(SYMBOLS_VARIABLE), "{text}");
        assert!(text.contains(FOOTPRINTS_VARIABLE), "{text}");
        assert!(text.contains("KiCad 9"), "{text}");
        assert_eq!(e.tried.len(), 2 + search.usual_places(Kind::Symbols).len());
    }

    #[test]
    fn nothing_found_with_a_cli_lists_the_places_beside_it() {
        let t = tree();
        program(&t.path().join("tools/bin/kicad-cli"));
        let search = Search::under(t.path()).with_path(vec![t.path().join("tools/bin")]);
        let text = search.find(Kind::Footprints).unwrap_err().to_string();
        assert!(text.contains("tools/share/kicad/footprints"), "{text}");
        assert!(text.contains("(beside "), "{text}");
        assert!(text.contains("tools/SharedSupport/footprints"), "{text}");
    }
}
