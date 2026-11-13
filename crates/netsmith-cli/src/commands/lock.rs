use std::fs;
use std::process::ExitCode;

use clap::{ArgGroup, Args as ClapArgs};

use super::common::{no_boards, target};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`lock --update` solves each board without the pick lock's holding (a hand pick in the source, `.lcsc &= ...`, still holds its part) and writes the lock with the rows the solver picks now, printing for each part that changed the old row and the new one (nothing when nothing changed). `lock --check` does the same without writing, prints the same and exits 1 when the lock is not current or does not exist (for CI). Both exit 2 when they cannot tell: the lock is not readable, a picks file from an older netsmith is beside the source (move its entries into the source as the message says), or the design does not build. With no file it acts on the project's boards."
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
}

impl Args {
    pub fn mode(&self) -> LockMode {
        if self.update {
            LockMode::Update
        } else {
            LockMode::Check
        }
    }
}

/// Refreshes or checks the pick lock of each source file (ADR 0100).
pub fn run(args: Args) -> ExitCode {
    run_lock(args.file.as_deref(), args.mode())
}

/// `netsmith lock --update` and `--check`: the pick lock of each source file, solved again with
/// no holding. The diff goes to stdout; what the lock held that no longer passes, the reason a
/// check fails and every error go to stderr. Exit 0 when nothing is wrong (a lock was written,
/// or is current), 1 when `--check` finds a lock that is not current, 2 when it cannot tell.
fn run_lock(file: Option<&str>, mode: LockMode) -> ExitCode {
    let (target, loader, catalogs) = match target(file, false) {
        Ok(t) => t,
        Err(code) => return code,
    };
    if no_boards(&target) {
        return ExitCode::from(2);
    }
    let mut stale = false;
    for source in &target.files {
        let text = match fs::read_to_string(&source.path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}: {e}", source.name);
                return ExitCode::from(2);
            }
        };
        let found = match netsmith::pick_lock::refresh(
            &source.path,
            &source.name,
            &text,
            &loader,
            &catalogs,
        ) {
            Ok(found) => found,
            Err(e) => {
                eprintln!("{}", e.trim_end());
                return ExitCode::from(2);
            }
        };
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
    if stale {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
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
        assert_eq!(kind(&["lock"]), ErrorKind::MissingRequiredArgument);
        assert_eq!(
            kind(&["lock", "--update", "--check"]),
            ErrorKind::ArgumentConflict
        );
    }
}
