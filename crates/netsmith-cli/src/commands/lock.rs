use std::fs;
use std::process::ExitCode;

use clap::{ArgGroup, Args as ClapArgs};

use super::common::{no_boards, target};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`lock --update` solves each board without the pick lock's holding (the picks file still holds its parts) and writes the lock with the rows the solver picks now, printing for each part that changed the old row and the new one (nothing when nothing changed). `lock --check` does the same without writing, prints the same and exits 1 when the lock is not current or does not exist, or when the picks file names a part that is gone (for CI). Both exit 2 when they cannot tell: the lock or the picks file is not readable, or the design does not build. With no file it acts on the project's boards."
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
        // The entries of the picks file whose part is gone (#462): the file is a person's, so a
        // refresh leaves it alone, and `--check` fails on them.
        if !found.stale.is_empty() {
            let picks = netsmith::picks::picks_path(&source.path);
            let lines: String = found.stale.iter().map(|l| format!("  {l}\n")).collect();
            match mode {
                LockMode::Update => eprint!(
                    "note: {} names parts the design no longer picks a row for; remove their entries:\n{lines}",
                    picks.display()
                ),
                LockMode::Check => {
                    eprint!(
                        "error: {} names parts the design no longer picks a row for; remove their entries:\n{lines}",
                        picks.display()
                    );
                    stale = true;
                }
            }
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
