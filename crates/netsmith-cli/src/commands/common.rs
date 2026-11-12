/// Write the pin file beside each source that has pin requests, when a run changes it (ADR 0101),
/// and say what was written and what is stale. `--locked` writes nothing and stops instead. The
/// exit code of the command when it stops, `None` to go on.
fn sync_pins(file: Option<&str>, locked: bool) -> Option<ExitCode> {
    // Only what the check needs: the catalogs are loaded, and timed, by the command itself.
    let Ok(target) = Target::resolve(file) else {
        // The command says it again.
        return None;
    };
    let Ok(loader) = target.loader() else {
        return None;
    };
    for source in &target.files {
        let Ok(text) = fs::read_to_string(&source.path) else {
            return None;
        };
        match netsmith::pins::sync(&source.name, &source.path, &text, &loader, locked) {
            Ok(synced) => {
                for note in synced.notes {
                    eprintln!("note: {note}");
                }
            }
            Err(message) => {
                eprint!("{message}");
                return Some(ExitCode::FAILURE);
            }
        }
    }
    None
}

/// What a command acts on: the named file or the project's top-level files, what their
/// imports can reach, and the catalogs to pick parts from. A problem is printed and is the
/// exit code.
fn target(file: Option<&str>, strict: bool) -> Result<(Target, Loader, Catalogs), ExitCode> {
    let fail = |e: String| {
        eprintln!("{e}");
        ExitCode::from(2)
    };
    let target = Target::resolve(file).map_err(fail)?;
    let loader = target.loader().map_err(fail)?;
    let mut catalogs = timings::stage("catalogs", "", || target.catalogs()).map_err(fail)?;
    // `--strict` makes guideline findings errors for this run; the manifest's
    // `guidelines = "error"` already has (ADR 0036).
    if strict {
        catalogs = catalogs.with_strict_guidelines(true);
    }
    Ok((target, loader, catalogs))
}

/// The KiCad libraries of the target's project (ADR 0021). A mistake in `kicad.toml` is
/// printed and is the exit code.
fn libraries(target: &Target) -> Result<Libraries, ExitCode> {
    let root = target.project.as_ref().map(|p| p.root.as_path());
    Libraries::for_project(root).map_err(|e| {
        eprintln!("{e}");
        ExitCode::from(2)
    })
}

/// Whether a project with no file to act on was found, which it says. A library-only
/// project lists no `top_level` file (ADR 0011).
fn no_boards(target: &Target) -> bool {
    let empty = target.files.is_empty();
    if empty {
        eprintln!(
            "error: this project has no boards: `top_level` in netsmith.toml is empty; name a .nsm file to build it"
        );
    }
    empty
}

/// What `board3d` and `render` do first: the one board `file` and `--board` pick, built and
/// placed as `fab` places it (the pick lock and the layout file beside its source are read,
/// nothing is written), and `then` of it. A project of several top-level files takes the first
/// that has the board `--board` names. The error is printed and is the exit code: 1 when no file
/// builds the board or places it, 2 when the command cannot start.
fn with_placed_board<T>(
    file: Option<&str>,
    board: Option<&str>,
    then: impl Fn(&netsmith::board3d::Placed) -> Result<T, String>,
) -> Result<T, ExitCode> {
    let (target, loader, catalogs) = target(file, false)?;
    if no_boards(&target) {
        return Err(ExitCode::from(2));
    }
    if target.files.len() > 1 && board.is_none() {
        eprintln!(
            "error: the project has {} top-level files; name one, or a board with `--board <name>`",
            target.files.len()
        );
        return Err(ExitCode::from(2));
    }
    let libraries = timings::stage("libraries", "", || libraries(&target))?;
    let mut errors = String::new();
    for source in &target.files {
        let text = fs::read_to_string(&source.path).map_err(|e| {
            eprintln!("{}: {e}", source.name);
            ExitCode::from(2)
        })?;
        // The pick lock holds the parts it names to their rows, as for `fab`; it is not written.
        let (held, _) = netsmith::pick_lock::open(&source.path, &catalogs, false).map_err(|e| {
            eprintln!("{e}");
            ExitCode::from(2)
        })?;
        let placed = netsmith::board3d::Placed {
            name: &source.name,
            text: &text,
            loader: &loader,
            catalogs: held.as_ref().unwrap_or(&catalogs),
            libraries: &libraries,
            layout: netsmith::layout::layout_path(&source.path),
            board,
        };
        match then(&placed) {
            Ok(done) => return Ok(done),
            Err(e) => errors.push_str(&e),
        }
    }
    eprint!("{errors}");
    Err(ExitCode::FAILURE)
}

/// `--view` of `render` and `view`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ViewName {
    /// Straight at the top face.
    Top,
    /// Straight at the bottom face.
    Bottom,
    /// Along the board's front edge.
    Front,
    /// Along the board's back edge.
    Back,
    /// Along the board's left edge.
    Left,
    /// Along the board's right edge.
    Right,
    /// From a corner, above.
    Iso,
    /// From a corner, below.
    IsoBottom,
}

impl ViewName {
    fn raster(self) -> netsmith_pcb::raster3d::View {
        use netsmith_pcb::raster3d::View;
        match self {
            ViewName::Top => View::Top,
            ViewName::Bottom => View::Bottom,
            ViewName::Front => View::Front,
            ViewName::Back => View::Back,
            ViewName::Left => View::Left,
            ViewName::Right => View::Right,
            ViewName::Iso => View::Iso,
            ViewName::IsoBottom => View::IsoBottom,
        }
    }

    fn editor(self) -> netsmith_editor::api::RenderView {
        use netsmith_editor::api::RenderView;
        match self {
            ViewName::Top => RenderView::Top,
            ViewName::Bottom => RenderView::Bottom,
            ViewName::Front => RenderView::Front,
            ViewName::Back => RenderView::Back,
            ViewName::Left => RenderView::Left,
            ViewName::Right => RenderView::Right,
            ViewName::Iso => RenderView::Iso,
            ViewName::IsoBottom => RenderView::IsoBottom,
        }
    }
}

/// The ids of one `--highlight` or `--select`, split at commas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ids(pub Vec<String>);

fn parse_ids(given: &str) -> Result<Ids, String> {
    let ids: Vec<String> = given
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if ids.len() > 1 && ids.iter().any(|id| id == "none") {
        return Err("`none` clears it, so no id goes with it".to_string());
    }
    Ok(Ids(ids))
}
