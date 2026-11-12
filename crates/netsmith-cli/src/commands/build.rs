/// What `build` and `check` are asked to do: `check` is `build` that writes nothing (#455).
struct BuildRun<'a> {
    file: Option<&'a str>,
    out: Option<&'a Path>,
    strict: bool,
    locked: bool,
    format: Format,
    notes: NotesMode,
    /// `build` writes the schematic, the lock and the libraries' tables; `check` writes nothing.
    write: bool,
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

fn run_build(run: &BuildRun) -> ExitCode {
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
        let boards = outcome.boards;
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
            });
        }
    }
    if json {
        report.ok = code == ExitCode::SUCCESS;
        print!("{}", report.to_json());
    }
    code
}

/// Check, solve and write a KiCad schematic of the boards.
///
/// With no file, `build` acts on the project's boards (see netsmith.toml).
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct BuildArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The folder to write into; the default is the target's.
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Make a guideline finding an error for this run.
    ///
    /// Like `guidelines = "error"` in netsmith.toml (ADR 0036); without it a finding against a
    /// guideline is a warning.
    #[arg(long)]
    pub strict: bool,
    /// Neither the pick lock nor the pin file may change.
    ///
    /// The run ends with an error, before anything is written, when the parts it picks differ
    /// from the pick lock beside the source (`<file>.lock.toml`) or when it does not exist, when
    /// the pins that its requests get differ from the pin file beside it (`<file>.pins.toml`,
    /// ADR 0101), or when the picks file beside it (`<file>.picks.toml`, #462) names a part that
    /// is gone (see _The pick lock_ in docs/LANGUAGE.md).
    #[arg(long)]
    pub locked: bool,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there. It holds `command`,
    /// `ok`, the `boards` with the folders written, the `findings`, and the count of the `notes`
    /// for each cause. Each finding has a `severity` (`error`, `warning` or `note`), a `code`,
    /// and the `board`, the member `path`, the `file`, `line` and `column` and the `message` it
    /// has. The code is the stage that found an error or a warning (`syntax`, `check`, `solve`,
    /// `design`, `guideline`, `power`, `lock`, `place`, `fab`), or `note.` and the cause of a
    /// note (`note.not_stated`, `note.part_model`, `note.unjudged`, `note.circular`,
    /// `note.supply`, `note.pick`, `note.track`, `note.other`): there are no codes for each rule.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
    /// Each note, a count for each cause, or none.
    ///
    /// `summary` prints a board's notes as one line with a count for each cause
    /// (`note: TwoBuck: 7 notes: 6 not_stated, 1 unjudged`), `none` leaves them out, and `all`,
    /// the default, prints each. With `--format json` the notes are findings only with `all`;
    /// their counts are always there.
    #[arg(long, value_enum, default_value = "all")]
    pub notes: NotesMode,
}
