use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith::findings::{BoardOut, Finding, Format, NotesMode, Report, Severity};
use netsmith::{outcome_with, render_problems};
use netsmith_core::timings;

use super::common::{self, libraries, no_boards, sync_pins, target};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "With no file, `build` acts on the project's boards (see netsmith.toml)."
)]
#[command(args_override_self = true)]
pub struct Args {
    /// The source file; with none, the project's boards (see netsmith.toml).
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The folder to write into.
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Make a guideline finding an error for this run.
    ///
    /// Like `guidelines = "error"` in netsmith.toml (ADR 0036); without it a finding against a
    /// guideline is a warning.
    #[arg(long)]
    pub strict: bool,
    /// Stop, before anything is written, when the pick lock, the pin file or the picks file
    /// would change.
    ///
    /// The run ends with an error, before anything is written, when the parts it picks differ from
    /// the pick lock beside the source (`<file>.lock.toml`) or when it does not exist, when the
    /// pins that its requests get differ from the pin file beside it (`<file>.pins.toml`, ADR
    /// 0101), or when the picks file beside it (`<file>.picks.toml`, #462) names a part that is
    /// gone (see _The pick lock_ in docs/LANGUAGE.md).
    #[arg(long)]
    pub locked: bool,
    #[command(flatten)]
    pub output: common::Output,
}

/// Writes the pin file, then checks and solves each board and writes its schematic.
pub fn run(args: Args) -> ExitCode {
    timings::stage("build.total", "", || {
        // The pin file first: a request must have its pin before anything is built, and
        // `--locked` stops here before the pick lock or any file is written.
        if let Some(code) = sync_pins(args.file.as_deref(), args.locked) {
            return code;
        }
        run_build(&BuildRun {
            file: args.file.as_deref(),
            out: args.out.as_deref(),
            strict: args.strict,
            locked: args.locked,
            format: args.output.format.into(),
            notes: args.output.notes.into(),
            write: true,
        })
    })
}

/// What `build` and `check` are asked to do: `check` is `build` that writes nothing (#455).
pub(super) struct BuildRun<'a> {
    pub(super) file: Option<&'a str>,
    pub(super) out: Option<&'a Path>,
    pub(super) strict: bool,
    pub(super) locked: bool,
    pub(super) format: Format,
    pub(super) notes: NotesMode,
    /// `build` writes the schematic, the lock and the libraries' tables; `check` writes nothing.
    pub(super) write: bool,
}

/// What a board said that did not stop it: printed as text, or kept for the JSON report.
fn tell_built(b: &netsmith::Built, run: &BuildRun, report: &mut Report) {
    if run.format == Format::Json {
        report.add_built(b, run.notes);
        return;
    }
    let notes = netsmith::findings::notes(b);
    match run.notes {
        NotesMode::All => {
            for (_, note) in &notes {
                eprintln!("note: {}", note.message);
            }
        }
        NotesMode::Summary => {
            if let Some(line) = netsmith::findings::summary_line(&b.label(), &notes) {
                eprintln!("{line}");
            }
        }
        NotesMode::None => {}
    }
    for warning in b.guideline_warnings() {
        eprintln!("warning: {warning}");
    }
}

pub(super) fn run_build(run: &BuildRun) -> ExitCode {
    let json = run.format == Format::Json;
    let mut report = Report::new(if run.write { "build" } else { "check" });
    let (target, loader, catalogs) = match target(run.file, run.strict) {
        Ok(t) => t,
        Err(code) => return code,
    };
    let libraries = match timings::stage("libraries", "", || libraries(&target)) {
        Ok(l) => l,
        Err(code) => return code,
    };
    if no_boards(&target) {
        return ExitCode::from(2);
    }
    let out = run.out.unwrap_or(&target.build_out);
    let mut code = ExitCode::SUCCESS;
    // An error that stops a file, printed or kept.
    let fail = |report: &mut Report, finding: Finding, text: &str| {
        if json {
            report.findings.push(finding);
        } else {
            eprint!("{text}");
        }
    };
    for source in &target.files {
        let text = match fs::read_to_string(&source.path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{}: {e}", source.name);
                return ExitCode::from(2);
            }
        };

        // The pick lock beside the source holds the parts it names to their rows.
        let (held, using) =
            match netsmith::pick_lock::open(&source.path, &catalogs, run.locked && run.write) {
                Ok(opened) => opened,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(2);
                }
            };
        let catalogs = held.as_ref().unwrap_or(&catalogs);
        let outcome = match outcome_with(&source.name, &text, &loader, catalogs, Some(&libraries)) {
            Ok(outcome) => outcome,
            Err(problems) => {
                code = ExitCode::FAILURE;
                if json {
                    report
                        .findings
                        .extend(netsmith::findings::of_problems(&problems));
                } else {
                    eprint!("{}", render_problems(&problems));
                }
                continue;
            }
        };
        // `build` writes the lock when there is none, and says how the picks differ from one;
        // `check` writes nothing and only says how they differ.
        let lock_notes = if run.write {
            match netsmith::pick_lock::settle(&using, catalogs.pick_lock(), &outcome.lock) {
                Ok(settled) => {
                    if let Some(file) = &settled.wrote
                        && !json
                    {
                        println!("wrote {}", file.display());
                    }
                    settled.notes
                }
                Err(e) => {
                    code = ExitCode::FAILURE;
                    fail(
                        &mut report,
                        Finding::plain(Severity::Error, "lock", e.trim_start_matches("error: ")),
                        &format!("{e}\n"),
                    );
                    continue;
                }
            }
        } else {
            let indent = |lines: &[String]| {
                lines
                    .iter()
                    .map(|l| format!("  {l}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let mut notes: Vec<String> = catalogs
                .pick_lock()
                .map(|old| netsmith::pick_lock::compare(old, &outcome.lock).lines())
                .filter(|lines| !lines.is_empty())
                .map(|lines| {
                    vec![format!(
                        "the parts picked differ from {}:\n{}",
                        using.file.display(),
                        indent(&lines)
                    )]
                })
                .unwrap_or_default();
            if let Some(picks) = &using.picks {
                let stale = netsmith::picks::stale(picks, &outcome.lock);
                if !stale.is_empty() {
                    notes.push(format!(
                        "{} names parts the design no longer picks a row for:\n{}",
                        picks.file,
                        indent(&stale)
                    ));
                }
            }
            notes
        };
        for note in &lock_notes {
            if json {
                report
                    .findings
                    .push(Finding::plain(Severity::Note, "note.other", note.clone()));
            } else {
                eprintln!("note: {note}");
            }
        }
        let mut boards = outcome.boards;
        // The STEP models parts are drawn from, found on disk (ADR 0108): a path outside the
        // project stops the file, a file that is not there is a warning.
        let places = netsmith::models::Places::of(&target);
        let mut misplaced = false;
        for b in &mut boards {
            let label = b.label();
            let found = netsmith::models::locate(&mut b.design, &places);
            for warning in &found.warnings {
                if json {
                    report.findings.push(Finding {
                        board: Some(label.clone()),
                        ..Finding::plain(Severity::Warning, "design", warning.clone())
                    });
                } else {
                    eprintln!("warning: {label}: {warning}");
                }
            }
            for error in &found.errors {
                misplaced = true;
                fail(
                    &mut report,
                    Finding {
                        board: Some(label.clone()),
                        ..Finding::plain(Severity::Error, "design", error.clone())
                    },
                    &format!("error: {label}: {error}\n"),
                );
            }
        }
        if misplaced {
            code = ExitCode::FAILURE;
            continue;
        }
        // What the requirements prove cannot be placed is said now, with no placing
        // (ADR 0045 section 14); the layout file's locks count.
        let names: Vec<String> = boards.iter().map(|b| b.design.name.clone()).collect();
        let layout = netsmith::layout::read(&netsmith::layout::layout_path(&source.path), &names)
            .ok()
            .flatten();
        let mut unplaceable = false;
        for b in &boards {
            let found = netsmith::fab::precheck_board(
                b,
                layout.as_ref().map(|l| &l.0),
                &libraries,
                catalogs.constraints_warn(),
            );
            for warning in &found.warnings {
                if json {
                    report.findings.push(Finding {
                        board: Some(found.board.clone()),
                        ..Finding::plain(Severity::Warning, "place", warning.clone())
                    });
                } else {
                    eprintln!("warning: {}: {warning}", found.board);
                }
            }
            for error in &found.errors {
                unplaceable = true;
                fail(
                    &mut report,
                    Finding {
                        board: Some(found.board.clone()),
                        ..Finding::plain(Severity::Error, "place", error.clone())
                    },
                    &format!("error: {}: {error}\n", found.board),
                );
            }
        }
        if unplaceable {
            code = ExitCode::FAILURE;
            continue;
        }
        for b in boards {
            let mut written = None;
            if run.write {
                let dir = match b.write(out) {
                    Ok(dir) => dir,
                    Err(e) => {
                        eprintln!("{}: {e}", b.dir(out).display());
                        return ExitCode::from(2);
                    }
                };
                // The project's own libraries and the standard library's own files that
                // the board uses, so KiCad finds the symbols.
                if let Err(e) = libraries.write_tables(&dir, &b.design) {
                    eprintln!("{}: {e}", dir.display());
                    return ExitCode::from(2);
                }
                if !json {
                    print!("{}", b.report);
                }
                written = Some(dir);
            }
            tell_built(&b, run, &mut report);
            match &written {
                Some(dir) if !json => println!("wrote {}", dir.display()),
                _ => {}
            }
            report.boards.push(BoardOut {
                board: b.label(),
                dir: written.map(|d| d.display().to_string()),
                parts: Vec::new(),
            });
        }
    }
    if json {
        report.ok = code == ExitCode::SUCCESS;
        print!("{}", report.to_json());
    }
    code
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{output, parse};
    use crate::commands::{Command, build, lock};

    fn build(file: Option<&str>) -> build::Args {
        build::Args {
            file: file.map(str::to_string),
            out: None,
            strict: false,
            locked: false,
            output: output(),
        }
    }

    #[test]
    fn a_command_takes_one_nsm_file_before_or_after_its_flags() {
        assert_eq!(parse(&["build"]), Ok(Command::Build(build(None))));
        assert_eq!(
            parse(&["build", "a.nsm"]),
            Ok(Command::Build(build(Some("a.nsm"))))
        );
        let expected = Ok(Command::Build(build::Args {
            out: Some("x".into()),
            ..build(Some("a.nsm"))
        }));
        assert_eq!(parse(&["build", "a.nsm", "--out", "x"]), expected);
        assert_eq!(parse(&["build", "--out", "x", "a.nsm"]), expected);
        assert!(parse(&["editor", "a.nsm", "b.nsm"]).is_err());
        assert!(parse(&["init", "dir"]).is_err());
    }

    #[test]
    fn a_flag_given_twice_is_the_last_one() {
        let Ok(Command::Build(b)) =
            parse(&["build", "--out", "a", "--out", "b", "--strict", "--strict"])
        else {
            panic!()
        };
        assert_eq!(b.out, Some("b".into()));
        assert!(b.strict);
        let Ok(Command::Lock(l)) = parse(&["lock", "--check", "--check"]) else {
            panic!()
        };
        assert_eq!(l.mode(), lock::LockMode::Check);
    }
}
