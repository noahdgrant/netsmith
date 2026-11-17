use std::fs;
use std::process::ExitCode;

use clap::{ArgGroup, Args as ClapArgs};
use netsmith_fabricate::dependency_lock;
use netsmith_fabricate::project::{Project, Target};
use netsmith_pipeline::pick_lock::Parts;

use super::common::{Format, no_boards, print_json, target};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`project lock --update` solves each board without the pick lock's holding (a hand pick in the source, `.lcsc &= ...`, still holds its part) and writes the lock with the rows the solver picks now, printing for each part that changed the old row and the new one (nothing when nothing changed). `project lock --check` does the same without writing, prints the same and exits 1 when the lock is not current or does not exist (for CI). Both exit 2 when they cannot tell: the lock is not readable, a picks file from an older netsmith is beside the source (move its entries into the source as the message says), or the design does not build. With no file it acts on the project's boards. A source the project leaves out of the locks (`lock` in netsmith.toml) is skipped. A project with a dependency by `git` has a dependency lock too, `dependencies.lock.toml` beside netsmith.toml, the commit each one was read at: `--update` fetches each again and takes the commit its `rev` or `version` names now, printing the entries that changed, and `--check` fails when the lock lacks an entry for one or holds one no manifest asks for, fetching nothing. `project lock --update --part r1,stage.cap` takes new picks for those parts only, and `--except r1` for every part but those: the other parts stay on the row the lock has for them, which is judged by everything the design says, and an error says so when it no longer passes. A part named is looked for by its path in every board of the file (or of the project), and one that no board has is an error that lists the parts there are."
)]
#[command(group(ArgGroup::new("mode").required(true).args(["update", "check"])))]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// Write the rows the solver picks now.
    #[arg(long)]
    pub update: bool,
    /// Write nothing; fail when the lock is not current.
    #[arg(long)]
    pub check: bool,
    /// With `--update`, take new picks only for these parts (by path, comma-separated) and keep the rest on their rows.
    #[arg(
        long,
        value_name = "PATH",
        value_delimiter = ',',
        conflicts_with = "except"
    )]
    pub part: Vec<String>,
    /// With `--update`, take new picks for every part but these (by path, comma-separated).
    #[arg(long, value_name = "PATH", value_delimiter = ',')]
    pub except: Vec<String>,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there: `command` (`lock`),
    /// `ok` (the exit code is 0), the `mode` (`update` or `check`), the `files` and, for a project with a dependency by `git`, the
    /// `dependencies`. Each of the `files` is a pick lock: its `file`, its `status` (`wrote` and
    /// `updated` for `--update` when it changed, `current`, and for `--check` `not_current`, or
    /// `missing` when there is none), the `changes`, one line for each part whose row changed, as
    /// the text prints them, and `no_longer_pass`, what the solver said of the rows the file holds
    /// that no longer pass, when there are some. The `dependencies` are the dependency lock's
    /// `file`, its `status` (as for a pick lock, without `missing`) and its `changes`, the lines
    /// the text prints. What the text says on stderr as an error (a lock that cannot be read, a
    /// part no board has, a design that does not build) is still on stderr, as text, with exit
    /// code 2, and nothing on stdout.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

impl Args {
    pub fn mode(&self) -> LockMode {
        if self.update {
            LockMode::Update
        } else {
            LockMode::Check
        }
    }

    /// The parts that take a new pick.
    pub fn parts(&self) -> Parts {
        if !self.part.is_empty() {
            Parts::Only(self.part.clone())
        } else if !self.except.is_empty() {
            Parts::Except(self.except.clone())
        } else {
            Parts::All
        }
    }
}

/// Refreshes or checks the pick lock of each source file (ADR 0100).
pub fn run(args: Args) -> ExitCode {
    if args.mode() == LockMode::Check && args.parts() != Parts::All {
        eprintln!("error: --part and --except go with --update: a check compares every part");
        return ExitCode::from(2);
    }
    run_lock(
        args.file.as_deref(),
        args.mode(),
        &args.parts(),
        args.format,
    )
}

/// `netsmith project lock --update` and `--check`: the pick lock of each source file, solved again with
/// no holding. The diff goes to stdout; what the lock held that no longer passes, the reason a
/// check fails and every error go to stderr. Exit 0 when nothing is wrong (a lock was written,
/// or is current), 1 when `--check` finds a lock that is not current, 2 when it cannot tell.
fn run_lock(file: Option<&str>, mode: LockMode, parts: &Parts, format: Format) -> ExitCode {
    let json = format == Format::Json;
    // The dependency lock first: `--update` fetches the dependencies by `git` again, and the
    // boards are then solved with what it took.
    let dependencies = match Target::resolve(file) {
        Ok(Target {
            project: Some(project),
            ..
        }) => match dependency_lock(&project, mode, json) {
            Ok(found) => Some(found),
            Err(code) => return code,
        },
        Ok(_) => None,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let (target, loader, catalogs) = match target(file, false) {
        Ok(t) => t,
        Err(code) => return code,
    };
    // A project of modules only, with dependencies by `git`, has a dependency lock and no
    // pick lock.
    let has_git = target
        .project
        .as_ref()
        .is_some_and(|p| p.manifest.dependencies.values().any(|d| d.git.is_some()));
    if target.files.is_empty() && has_git {
        let stale = dependencies.as_ref().is_some_and(|d| d.stale);
        if json {
            print_json(&document(
                mode,
                stale,
                &[],
                dependencies.as_ref().map(|d| &d.locked),
            ));
        }
        return exit(stale);
    }
    if no_boards(&target) {
        return ExitCode::from(2);
    }
    // Every source is solved before any lock is written, so that a part named that no board has
    // stops the run with nothing changed.
    let mut found = Vec::new();
    for source in target.files.iter().filter(|s| target.locks(s)) {
        let text = match fs::read_to_string(&source.path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}: {e}", source.name);
                return ExitCode::from(2);
            }
        };
        match netsmith_pipeline::pick_lock::refresh_parts(
            &source.path,
            &source.name,
            &text,
            &loader,
            &catalogs,
            parts,
        ) {
            Ok(f) => found.push(f),
            Err(e) => {
                eprintln!("{}", e.trim_end());
                return ExitCode::from(2);
            }
        }
    }
    let unknown: Vec<&String> = parts
        .named()
        .iter()
        .filter(|n| !found.iter().any(|f| f.paths.contains(n)))
        .collect();
    if !unknown.is_empty() {
        let mut every: Vec<&String> = found.iter().flat_map(|f| &f.paths).collect();
        every.sort();
        every.dedup();
        let list = |v: &[&String]| {
            v.iter()
                .map(|p| format!("`{p}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        eprintln!(
            "error: no board has the part {}; the parts there are: {}",
            list(&unknown),
            list(&every)
        );
        return ExitCode::from(2);
    }
    let mut stale = false;
    let mut files = Vec::new();
    for found in &found {
        let name = found.file.display().to_string();
        let lines: String = found.diff.iter().map(|l| format!("  {l}\n")).collect();
        if !found.reported.is_empty() {
            let outcome = match mode {
                LockMode::Update => "replaced with the rows above",
                LockMode::Check => {
                    "`project lock --update` replaces them with the rows the solver picks now"
                }
            };
            eprintln!(
                "note: the rows {name} holds that no longer pass, in the solver's words ({outcome}):\n{}",
                found.reported.trim_end()
            );
        }
        let status = match mode {
            LockMode::Update => {
                if let Err(e) = found.write() {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
                match (found.is_current(), found.existed) {
                    (true, _) => "current",
                    (false, true) => "updated",
                    (false, false) => "wrote",
                }
            }
            LockMode::Check if !found.existed && !found.is_current() => {
                stale = true;
                "missing"
            }
            LockMode::Check if !found.is_current() => {
                stale = true;
                "not_current"
            }
            LockMode::Check => "current",
        };
        match status {
            "updated" | "wrote" if !json => {
                println!("{status} {name}");
                print!("{lines}");
            }
            "missing" => eprintln!(
                "error: {name} does not exist: run `netsmith project lock --update` to write it, and commit it"
            ),
            "not_current" => {
                if !json {
                    println!("{name} is not current");
                    print!("{lines}");
                }
                eprintln!(
                    "error: {name} is not current: run `netsmith project lock --update` to refresh it, and commit it"
                );
            }
            _ => {}
        }
        files.push(Locked {
            file: name,
            status,
            changes: found.diff.clone(),
            no_longer_pass: found.reported.trim_end().to_string(),
        });
    }
    let stale = stale || dependencies.as_ref().is_some_and(|d| d.stale);
    if json {
        print_json(&document(
            mode,
            stale,
            &files,
            dependencies.as_ref().map(|d| &d.locked),
        ));
    }
    exit(stale)
}

/// One lock of the JSON document: a pick lock, or the dependency lock.
struct Locked {
    file: String,
    status: &'static str,
    changes: Vec<String>,
    /// The solver's words about the rows the lock holds that no longer pass.
    no_longer_pass: String,
}

/// The document of `project lock --format json`.
fn document(
    mode: LockMode,
    stale: bool,
    files: &[Locked],
    dependencies: Option<&Locked>,
) -> serde_json::Value {
    let one = |l: &Locked| {
        let mut lock = serde_json::json!({
            "file": l.file,
            "status": l.status,
            "changes": l.changes,
        });
        if !l.no_longer_pass.is_empty() {
            lock["no_longer_pass"] = l.no_longer_pass.clone().into();
        }
        lock
    };
    let mut document = serde_json::json!({
        "command": "lock",
        "ok": !stale,
        "mode": match mode {
            LockMode::Update => "update",
            LockMode::Check => "check",
        },
        "files": files.iter().map(one).collect::<Vec<_>>(),
    });
    if let Some(d) = dependencies {
        document["dependencies"] = one(d);
    }
    document
}

fn exit(stale: bool) -> ExitCode {
    if stale {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The dependency lock of `project` (`dependencies.lock.toml`, issue #1012): written again by
/// `--update`, compared with the manifests by `--check`, which fetches nothing. What it did or
/// found, or the exit code of an error. The text of the run is printed here unless `json`.
fn dependency_lock(
    project: &Project,
    mode: LockMode,
    json: bool,
) -> Result<DependencyLock, ExitCode> {
    let name = dependency_lock::LOCK_FILE;
    match mode {
        LockMode::Update => {
            let found = dependency_lock::update(project).map_err(|e| {
                eprintln!("{e}");
                ExitCode::from(2)
            })?;
            let verb = if found.existed { "updated" } else { "wrote" };
            if !found.diff.is_empty() && !json {
                println!("{verb} {name}");
                for line in &found.diff {
                    println!("  {line}");
                }
            }
            Ok(DependencyLock {
                stale: false,
                locked: Locked {
                    file: name.to_string(),
                    status: if found.diff.is_empty() {
                        "current"
                    } else {
                        verb
                    },
                    changes: found.diff,
                    no_longer_pass: String::new(),
                },
            })
        }
        LockMode::Check => {
            let found = dependency_lock::check(project).map_err(|e| {
                eprintln!("{e}");
                ExitCode::from(2)
            })?;
            if !json {
                for why in &found.stale {
                    println!("  {why}");
                }
            }
            if !found.stale.is_empty() {
                eprintln!(
                    "error: {name} is not current: run `netsmith project lock --update` to refresh it, and commit it"
                );
            }
            Ok(DependencyLock {
                stale: !found.stale.is_empty(),
                locked: Locked {
                    file: name.to_string(),
                    status: if found.stale.is_empty() {
                        "current"
                    } else {
                        "not_current"
                    },
                    changes: found.stale,
                    no_longer_pass: String::new(),
                },
            })
        }
    }
}

/// What [`dependency_lock`] did: whether `--check` found it stale, and the entry of the document.
struct DependencyLock {
    stale: bool,
    locked: Locked,
}

/// What `netsmith project lock` does with the pick lock (ADR 0100).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// `--update`: write the rows the solver picks now.
    Update,
    /// `--check`: write nothing; fail when the lock is not current.
    Check,
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::lock;
    use clap::error::ErrorKind;
    use netsmith_pipeline::pick_lock::Parts;

    #[test]
    fn lock_takes_update_or_check_and_not_both() {
        let mode = |args: &[&str]| match parse(args) {
            Ok(Leaf::Lock(l)) => (l.mode(), l.file),
            other => panic!("{args:?}: {other:?}"),
        };
        assert_eq!(
            mode(&["project", "lock", "--update"]),
            (lock::LockMode::Update, None)
        );
        assert_eq!(
            mode(&["project", "lock", "--check", "a.nsm"]),
            (lock::LockMode::Check, Some("a.nsm".into()))
        );
        let parts = |args: &[&str]| match parse(args) {
            Ok(Leaf::Lock(l)) => l.parts(),
            other => panic!("{args:?}: {other:?}"),
        };
        assert_eq!(parts(&["project", "lock", "--update"]), Parts::All);
        assert_eq!(
            parts(&["project", "lock", "--update", "--part", "r1,stage.cap"]),
            Parts::Only(vec!["r1".into(), "stage.cap".into()])
        );
        assert_eq!(
            parts(&["project", "lock", "--update", "--except", "r1"]),
            Parts::Except(vec!["r1".into()])
        );
        assert_eq!(
            kind(&[
                "project", "lock", "--update", "--part", "a", "--except", "b"
            ]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(
            kind(&["project", "lock"]),
            ErrorKind::MissingRequiredArgument
        );
        assert_eq!(
            kind(&["project", "lock", "--update", "--check"]),
            ErrorKind::ArgumentConflict
        );
    }
}
