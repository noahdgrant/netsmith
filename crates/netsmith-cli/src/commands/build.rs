use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_core::timings;
use netsmith_pipeline::{outcome_with, render_problems};
use netsmith_tools::findings::{BoardOut, Finding, Format, NotesMode, Report, Severity};

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
    /// Stop, before anything is written, when the pick lock or the pin file would change.
    ///
    /// The run ends with an error, before anything is written, when the parts it picks differ from
    /// the pick lock beside the source (`<file>.lock.toml`) or when it does not exist, when the
    /// pins that its requests get differ from the pin file beside it (`<file>.pins.toml`, ADR
    /// 0101) (see _The pick lock_ in docs/LANGUAGE.md).
    #[arg(long)]
    pub locked: bool,
    /// Also write the design of each board to this file, as JSON.
    ///
    /// The design is what the design stage hands the placer: the board's parts with their pins,
    /// its nets, its process and what its physical file states. The file is a JSON array with one
    /// design per board, in the order they are built, and the same design is always the same
    /// bytes. Nothing is written when a board fails. Each example keeps the file of its source
    /// beside it (`<stem>.design.json`), and scripts/check-example-designs.sh fails when one is
    /// stale.
    #[arg(long, value_name = "FILE")]
    pub emit_design: Option<PathBuf>,
    /// Also write the values the solve left each member of each board with to this file, as JSON.
    ///
    /// The values are what the DC analysis, the picks and the source left each member with: the
    /// range of current an inductor carries, the voltage a chain leaves at a node, the power in a
    /// resistor, the row a part was picked as. The file is a JSON array with one object per
    /// board, in the order they are built, each with the board's name and its members: a
    /// quantity's `unit` and its `range` in that unit (`"inf"` for an open end), text's `value`,
    /// and `shown`, the value as a message prints it. Each board also has its `picks`: each part
    /// the solver chose a row for, the row, and the rows it passed over before it with `why`
    /// (`unfit`, `refused`, `later`, `same`, `skipped`, `partly`), the `message` and, for a
    /// refusal, `at`, the relation's `file:line`. A board whose solve fails is written too, with
    /// `"failed": true`: the members and picks the solve got to, and `unpicked`, for each part of
    /// the group that failed the rows its first turn passed over (`with` the choices before it).
    /// The file is written when a board fails only if some board left such a record; a mistake
    /// before the solve (a name that is not defined) leaves none and nothing is written.
    #[arg(long, value_name = "FILE")]
    pub emit_solved: Option<PathBuf>,
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
            emit_design: args.emit_design.as_deref(),
            emit_solved: args.emit_solved.as_deref(),
            explain: None,
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
    /// The file to write the boards' designs to (`board build --emit-design`).
    pub(super) emit_design: Option<&'a Path>,
    /// The file to write the members' solved values to (`board build --emit-solved`).
    pub(super) emit_solved: Option<&'a Path>,
    /// What to print as text of what the solve left (`explain`).
    pub(super) explain: Option<Explain<'a>>,
}

/// What `explain` prints: the part or member `path` and what is inside it, in the board named
/// `board` or in every board.
pub(super) struct Explain<'a> {
    pub(super) path: &'a str,
    pub(super) board: Option<&'a str>,
    /// How many rows passed over each part lists; every row when `None`.
    pub(super) rows: Option<usize>,
}

/// What a board said that did not stop it: printed as text, or kept for the JSON report.
fn tell_built(b: &netsmith_pipeline::Built, run: &BuildRun, report: &mut Report) {
    if run.format == Format::Json {
        report.add_built(b, run.notes);
        return;
    }
    let notes = netsmith_tools::findings::notes(b);
    match run.notes {
        NotesMode::All => {
            for (_, note) in &notes {
                eprintln!("note: {}", note.message);
            }
        }
        NotesMode::Summary => {
            if let Some(line) = netsmith_tools::findings::summary_line(&b.label(), &notes) {
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
    if run.write
        && let Err(code) = common::write_dependency_lock(&target)
    {
        return code;
    }
    // The solved file says why each row was taken or passed over, which the solver records only
    // when asked (issue #744).
    let catalogs = catalogs.with_explain(run.emit_solved.is_some() || run.explain.is_some());
    let libraries = match timings::stage("libraries", "", || libraries(&target)) {
        Ok(l) => l,
        Err(code) => return code,
    };
    if no_boards(&target) {
        return ExitCode::from(2);
    }
    let out = run.out.unwrap_or(&target.build_out);
    let mut code = ExitCode::SUCCESS;
    // The designs of the boards built, for `--emit-design`.
    let mut designs = Vec::new();
    // The values each board's members were solved to, for `--emit-solved`.
    let mut solved = Vec::new();
    // Whether a board failed its check, which may be a spelling that changed (issue #740).
    let mut unchecked = false;
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
        let (held, using) = match netsmith_pipeline::pick_lock::open(
            &source.path,
            &catalogs,
            run.locked && run.write,
            target.locks(source),
        ) {
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
                unchecked = true;
                // What the solve got to before it failed (issue 1008).
                solved.extend(problems.iter().filter_map(|p| p.solved.clone()));
                if json {
                    report
                        .findings
                        .extend(netsmith_tools::findings::of_problems(&problems));
                } else {
                    eprint!("{}", render_problems(&problems));
                }
                continue;
            }
        };
        // `build` writes the lock when there is none, and says how the picks differ from one;
        // `check` writes nothing and only says how they differ.
        let lock_notes = if run.write {
            match netsmith_pipeline::pick_lock::settle(&using, catalogs.pick_lock(), &outcome.lock)
            {
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
            let notes: Vec<String> = catalogs
                .pick_lock()
                .map(|old| netsmith_pipeline::pick_lock::compare(old, &outcome.lock).lines())
                .filter(|lines| !lines.is_empty())
                .map(|lines| {
                    vec![format!(
                        "the parts picked differ from {}:\n{}",
                        using.file.display(),
                        indent(&lines)
                    )]
                })
                .unwrap_or_default();
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
        // The STEP models parts are drawn from and the case models, found on disk (ADR 0108,
        // ADR 0109): a path outside the project stops the file, a file that is not there is a
        // warning.
        let places = netsmith_fabricate::models::Places::of(&target);
        let mut misplaced = false;
        for b in &mut boards {
            let label = b.label();
            let found = netsmith_fabricate::models::locate(
                &mut b.design,
                &places,
                Some(&libraries.mapping),
            );
            // A case model that is not there leaves the fit unchecked (ADR 0109): a warning
            // here, and an error from `fab` when constraints are strict.
            for warning in found.warnings.iter().chain(&found.unchecked) {
                if json {
                    report.findings.push(Finding {
                        board: Some(label.clone()),
                        ..Finding::plain(Severity::Warning, "design", warning.clone())
                    });
                } else {
                    eprintln!("warning: {label}: {warning}");
                }
            }
            // What the project's model table drew, and which parts have no model (#763).
            for note in &found.notes {
                if json {
                    report.findings.push(Finding {
                        board: Some(label.clone()),
                        ..Finding::plain(Severity::Note, "note.other", note.clone())
                    });
                } else {
                    eprintln!("note: {label}: {note}");
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
        let layout = netsmith_fabricate::layout::read(
            &netsmith_fabricate::layout::layout_path(&source.path),
            &names,
        )
        .ok()
        .flatten();
        let mut unplaceable = false;
        for b in &boards {
            let found = netsmith_fabricate::fab::precheck_board(
                b,
                layout.as_ref(),
                &libraries,
                catalogs.constraints_warn(),
            );
            for warning in &found.place_warnings() {
                let warning_text = &warning.message;
                if json {
                    report.findings.push(Finding {
                        board: Some(found.board.clone()),
                        conflicts: warning.conflicts.clone(),
                        ..Finding::plain(Severity::Warning, "place", warning_text.clone())
                    });
                } else {
                    eprintln!("warning: {}: {warning_text}", found.board);
                }
            }
            for (n, error) in found.errors.iter().enumerate() {
                unplaceable = true;
                fail(
                    &mut report,
                    Finding {
                        board: Some(found.board.clone()),
                        requirements: found.requirements.clone(),
                        conflicts: found.error_proofs.get(n).cloned().unwrap_or_default(),
                        placement: found.placement.clone(),
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
            if run.emit_solved.is_some() || run.explain.is_some() {
                solved.push(netsmith_pipeline::solved::to_value(&b));
            }
            if run.emit_design.is_some() {
                designs.push(b.design);
            }
        }
    }
    if let Some(explain) = &run.explain {
        let shown: Vec<serde_json::Value> = solved
            .iter()
            .filter(|b| {
                explain.board.is_none_or(|name| {
                    let label = b["board"].as_str().unwrap_or("");
                    label == name || label.rsplit('/').next() == Some(name)
                })
            })
            .cloned()
            .collect();
        match netsmith_pipeline::solved::explain(&shown, explain.path, explain.rows) {
            Ok(text) => print!("{text}"),
            Err(message) => {
                eprintln!("error: {message}");
                code = ExitCode::FAILURE;
            }
        }
    }
    // The solved file is written when a board failed too, if the failure left a record of what
    // the solve got to (issue 1008); the design file only when every board is built.
    let failed_with_record = solved.iter().any(|b| b["failed"] == true);
    let emits = [
        (
            run.emit_design,
            netsmith_core::design_file::to_json(&designs),
            false,
        ),
        (
            run.emit_solved,
            netsmith_pipeline::solved::to_json(solved),
            failed_with_record,
        ),
    ];
    for (file, text, with_record) in emits {
        let Some(file) = file else { continue };
        if code != ExitCode::SUCCESS && !with_record {
            eprintln!("note: {} is not written, as a board failed", file.display());
        } else if let Err(e) = fs::write(file, text) {
            eprintln!("{}: {e}", file.display());
            return ExitCode::from(2);
        } else if !json {
            println!("wrote {}", file.display());
        }
    }
    if unchecked {
        common::release_note(&target, json.then_some(&mut report));
    }
    if json {
        report.ok = code == ExitCode::SUCCESS;
        print!("{}", report.to_json());
    }
    code
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::{output, parse};
    use crate::commands::{build, lock};

    fn build(file: Option<&str>) -> build::Args {
        build::Args {
            file: file.map(str::to_string),
            out: None,
            strict: false,
            locked: false,
            emit_design: None,
            emit_solved: None,
            output: output(),
        }
    }

    #[test]
    fn a_command_takes_one_nsm_file_before_or_after_its_flags() {
        assert_eq!(parse(&["board", "build"]), Ok(Leaf::Build(build(None))));
        assert_eq!(
            parse(&["board", "build", "a.nsm"]),
            Ok(Leaf::Build(build(Some("a.nsm"))))
        );
        let expected = Ok(Leaf::Build(build::Args {
            out: Some("x".into()),
            ..build(Some("a.nsm"))
        }));
        assert_eq!(parse(&["board", "build", "a.nsm", "--out", "x"]), expected);
        assert_eq!(parse(&["board", "build", "--out", "x", "a.nsm"]), expected);
        assert!(parse(&["view", "editor", "a.nsm", "b.nsm"]).is_err());
        assert!(parse(&["init", "dir"]).is_err());
    }

    #[test]
    fn a_flag_given_twice_is_the_last_one() {
        let Ok(Leaf::Build(b)) = parse(&[
            "board", "build", "--out", "a", "--out", "b", "--strict", "--strict",
        ]) else {
            panic!()
        };
        assert_eq!(b.out, Some("b".into()));
        assert!(b.strict);
        let Ok(Leaf::Lock(l)) = parse(&["project", "lock", "--check", "--check"]) else {
            panic!()
        };
        assert_eq!(l.mode(), lock::LockMode::Check);
    }
}
