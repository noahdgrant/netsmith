use std::fs;
use std::process::ExitCode;

use clap::{ArgGroup, Args as ClapArgs};
use netsmith_fabricate::dependency_lock;
use netsmith_fabricate::project::{Project, Target};
use netsmith_pipeline::pick_lock::Parts;

use super::common::{no_boards, target};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`lock --update` solves each board without the pick lock's holding (a hand pick in the source, `.lcsc &= ...`, still holds its part) and writes the lock with the rows the solver picks now, printing for each part that changed the old row and the new one (nothing when nothing changed). `lock --check` does the same without writing, prints the same and exits 1 when the lock is not current or does not exist (for CI). Both exit 2 when they cannot tell: the lock is not readable, a picks file from an older netsmith is beside the source (move its entries into the source as the message says), or the design does not build. With no file it acts on the project's boards. A source the project leaves out of the locks (`lock` in netsmith.toml) is skipped. A project with a dependency by `git` has a dependency lock too, `dependencies.lock.toml` beside netsmith.toml, the commit each one was read at: `--update` fetches each again and takes the commit its `rev` or `version` names now, printing the entries that changed, and `--check` fails when the lock lacks an entry for one or holds one no manifest asks for, fetching nothing. `lock --update --part r1,stage.cap` takes new picks for those parts only, and `--except r1` for every part but those: the other parts stay on the row the lock has for them, which is judged by everything the design says, and an error says so when it no longer passes. A part named is looked for by its path in every board of the file (or of the project), and one that no board has is an error that lists the parts there are."
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
    run_lock(args.file.as_deref(), args.mode(), &args.parts())
}

/// `netsmith lock --update` and `--check`: the pick lock of each source file, solved again with
/// no holding. The diff goes to stdout; what the lock held that no longer passes, the reason a
/// check fails and every error go to stderr. Exit 0 when nothing is wrong (a lock was written,
/// or is current), 1 when `--check` finds a lock that is not current, 2 when it cannot tell.
fn run_lock(file: Option<&str>, mode: LockMode, parts: &Parts) -> ExitCode {
    // The dependency lock first: `--update` fetches the dependencies by `git` again, and the
    // boards are then solved with what it took.
    let dependencies = match Target::resolve(file) {
        Ok(Target {
            project: Some(project),
            ..
        }) => match dependency_lock(&project, mode) {
            Ok(stale) => Some(stale),
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
        return exit(dependencies == Some(true));
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
    for found in &found {
        let name = found.file.display();
        let lines: String = found.diff.iter().map(|l| format!("  {l}\n")).collect();
        if !found.reported.is_empty() {
            let outcome = match mode {
                LockMode::Update => "replaced with the rows above",
                LockMode::Check => {
                    "`lock --update` replaces them with the rows the solver picks now"
                }
            };
            eprintln!(
                "note: the rows {name} holds that no longer pass, in the solver's words ({outcome}):\n{}",
                found.reported.trim_end()
            );
        }
        match mode {
            LockMode::Update => {
                if let Err(e) = found.write() {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
                if !found.is_current() {
                    let verb = if found.existed { "updated" } else { "wrote" };
                    println!("{verb} {name}");
                    print!("{lines}");
                }
            }
            LockMode::Check if !found.existed && !found.is_current() => {
                eprintln!(
                    "error: {name} does not exist: run `netsmith lock --update` to write it, and commit it"
                );
                stale = true;
            }
            LockMode::Check if !found.is_current() => {
                println!("{name} is not current");
                print!("{lines}");
                eprintln!(
                    "error: {name} is not current: run `netsmith lock --update` to refresh it, and commit it"
                );
                stale = true;
            }
            LockMode::Check => {}
        }
    }
    exit(stale || dependencies == Some(true))
}

fn exit(stale: bool) -> ExitCode {
    if stale {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The dependency lock of `project` (`dependencies.lock.toml`, issue #1012): written again by
/// `--update`, compared with the manifests by `--check`, which fetches nothing. Whether it is
/// stale (`--check` only), or the exit code of an error.
fn dependency_lock(project: &Project, mode: LockMode) -> Result<bool, ExitCode> {
    let name = dependency_lock::LOCK_FILE;
    match mode {
        LockMode::Update => {
            let found = dependency_lock::update(project).map_err(|e| {
                eprintln!("{e}");
                ExitCode::from(2)
            })?;
            if !found.diff.is_empty() {
                let verb = if found.existed { "updated" } else { "wrote" };
                println!("{verb} {name}");
                for line in &found.diff {
                    println!("  {line}");
                }
            }
            Ok(false)
        }
        LockMode::Check => {
            let found = dependency_lock::check(project).map_err(|e| {
                eprintln!("{e}");
                ExitCode::from(2)
            })?;
            for why in &found.stale {
                println!("  {why}");
            }
            if !found.stale.is_empty() {
                eprintln!(
                    "error: {name} is not current: run `netsmith lock --update` to refresh it, and commit it"
                );
            }
            Ok(!found.stale.is_empty())
        }
    }
}

/// What `netsmith lock` does with the pick lock (ADR 0100).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// `--update`: write the rows the solver picks now.
    Update,
    /// `--check`: write nothing; fail when the lock is not current.
    Check,
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::{Command, lock};
    use clap::error::ErrorKind;
    use netsmith_pipeline::pick_lock::Parts;

    #[test]
    fn lock_takes_update_or_check_and_not_both() {
        let mode = |args: &[&str]| match parse(args) {
            Ok(Command::Lock(l)) => (l.mode(), l.file),
            other => panic!("{args:?}: {other:?}"),
        };
        assert_eq!(mode(&["lock", "--update"]), (lock::LockMode::Update, None));
        assert_eq!(
            mode(&["lock", "--check", "a.nsm"]),
            (lock::LockMode::Check, Some("a.nsm".into()))
        );
        let parts = |args: &[&str]| match parse(args) {
            Ok(Command::Lock(l)) => l.parts(),
            other => panic!("{args:?}: {other:?}"),
        };
        assert_eq!(parts(&["lock", "--update"]), Parts::All);
        assert_eq!(
            parts(&["lock", "--update", "--part", "r1,stage.cap"]),
            Parts::Only(vec!["r1".into(), "stage.cap".into()])
        );
        assert_eq!(
            parts(&["lock", "--update", "--except", "r1"]),
            Parts::Except(vec!["r1".into()])
        );
        assert_eq!(
            kind(&["lock", "--update", "--part", "a", "--except", "b"]),
            ErrorKind::ArgumentConflict
        );
        assert_eq!(kind(&["lock"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(
            kind(&["lock", "--update", "--check"]),
            ErrorKind::ArgumentConflict
        );
    }
}
