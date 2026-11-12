//! The layout file beside a source file (ADR 0020 item 7, ADR 0032): `<stem>.pcb.toml`.
//!
//! It is optional. `fab` and the editor's PCB view read it when it exists. It tells the placer
//! which face each part is on and what to keep clear of
//! ([`netsmith_pcb::place_options`]), and then puts what it holds on the placed board with
//! [`netsmith_pcb::apply_layout`].
//!
//! Why a file beside the source, with the geometry of a placement in it (ADR 0020 item 7,
//! decided by the owner on 2026-11-02, and amended by ADR 0048: a requirement on where a part
//! goes is in the physical file, and the solutions stay here). A generated `.kicad_pcb` is
//! never read back (ADR 0006), so a hand layout survives regeneration only if netsmith owns
//! where it is stored. The file is keyed by instance path (`stage.output_cap`), which does not
//! change when reference designators are renumbered. A part with no entry is placed by the
//! placer, a `locked` entry is fixed and the placer places the rest round it, and an entry for
//! a path that is gone is an error naming it. The source stays free of geometry. What the
//! circuit implies is derived by the placer (a capacitor between a rail's two pins on the net
//! of a supply pin of a part is a decoupling capacitor, and parts of one module are grouped
//! by instance path). Rejected: constraints in the source (`.c1 near .u.vdd`): a hint that
//! travels with the module is an advantage, but `near` is an unordered relation over geometry
//! that needs a distance and a checker, it cannot say a fixed position (so a file is needed
//! anyway), and a module can name only its own parts. A `near` or `group` statement waits for
//! a case the derived placement gets wrong, and is then a decision with a real example.

use std::path::{Path, PathBuf};

use netsmith_pcb::Layout;

/// The layout file of a source file: `led.nsm` has `led.pcb.toml`.
pub fn layout_path(source: &Path) -> PathBuf {
    source.with_extension("pcb.toml")
}

/// The layout in the file at `file`, with the name to quote it by, or `None` if there is no
/// such file. `boards` are the names of all the boards of the source file, so that a table
/// for a board that is gone is found. A file that cannot be read or has a wrong table is an
/// error naming it.
pub fn read(file: &Path, boards: &[String]) -> Result<Option<(Layout, String)>, String> {
    let name = file.display().to_string();
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let layout = Layout::parse(&text, &name).map_err(|e| e.to_string())?;
    let known: Vec<&str> = boards.iter().map(String::as_str).collect();
    layout
        .check_boards(&name, &known)
        .map_err(|e| e.to_string())?;
    Ok(Some((layout, name)))
}

/// Why [`place`] made no board.
pub enum PlaceFailure {
    /// The design cannot be made into a placed board (a footprint, a process, the placer).
    Board(netsmith_pcb::BoardError),
    /// The layout file does not fit the placed board.
    Layout(String),
}

/// The board of `design` placed as `fab`, the editor's PCB view and `board3d` place it: the
/// layout file's faces and locks tell the placer what to keep, and its positions are then put on
/// the board. `lines_warn` makes an unmet requirement of the physical file a warning (ADR 0045).
/// The options are returned for what measures the requirements afterwards.
pub fn place(
    design: &netsmith_core::Design,
    layout: Option<&(Layout, String)>,
    libraries: &crate::Libraries,
    lines_warn: bool,
) -> Result<(netsmith_pcb::Board, netsmith_pcb::PlaceOptions), PlaceFailure> {
    let mut options = netsmith_pcb::place_options(design, layout.map(|l| &l.0));
    options.lines_warn = lines_warn;
    let mut board = netsmith_place_route::board_from_design_placed(
        design,
        &libraries.mapping,
        &libraries.footprints,
        &options,
    )
    .map_err(PlaceFailure::Board)?;
    if let Some((layout, file)) = layout {
        netsmith_pcb::apply_layout(&mut board, layout, file)
            .map_err(|e| PlaceFailure::Layout(e.to_string()))?;
    }
    Ok((board, options))
}
