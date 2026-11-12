//! A placed board saved to a file, and routed again from it alone: `netsmith fab
//! --save-placed` writes `<board>.placed.json` into the board's output folder, the board as
//! placed just before it is routed, and `netsmith route <file>` routes that board the way
//! `fab` does and writes the board files, with no checking, solving or placing first. It is a
//! router tool, for experiments on the router: what it writes is not a fab package, and no
//! design rules check is run on it.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use netsmith_pcb::{Board, kicad_pcb, kicad_pro, svg};
use serde::{Deserialize, Serialize};

use crate::fab::{ROUTING_FAILURE, route_with_margin, routing_failure};

/// The version of the file's layout. A file of another version is refused, since the board's
/// fields change with netsmith: save the board again with the `fab` of this build.
pub const FORMAT: u32 = 1;

/// What `<board>.placed.json` holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlacedFile {
    pub format: u32,
    /// The project's `track_margin` that `fab` routed with, which `route` uses unless told
    /// another.
    pub track_margin: f64,
    pub board: Board,
}

/// The name of the file a board is saved to.
pub fn file_name(board: &Board) -> String {
    format!("{}.placed.json", board.name)
}

/// The file's text for `board`, placed and not routed, which `fab` routes with `track_margin`.
pub fn to_json(board: &Board, track_margin: f64) -> String {
    let file = PlacedFile {
        format: FORMAT,
        track_margin,
        board: board.clone(),
    };
    serde_json::to_string(&file).expect("a board is plain data")
}

/// Read a file that [`to_json`] wrote.
pub fn from_json(text: &str) -> Result<PlacedFile, String> {
    #[derive(Deserialize)]
    struct Version {
        format: Option<u32>,
    }
    let version: Version =
        serde_json::from_str(text).map_err(|e| format!("not a placed board: {e}"))?;
    if version.format != Some(FORMAT) {
        return Err(format!(
            "a placed board of format {}, but this netsmith reads format {FORMAT}: save it again with `netsmith fab --save-placed`",
            version
                .format
                .map_or("(none)".to_string(), |f| f.to_string())
        ));
    }
    serde_json::from_str(text).map_err(|e| format!("not a placed board of format {FORMAT}: {e}"))
}

/// Write `board` into `dir` as [`file_name`], and return the file's path.
pub fn save(dir: &Path, board: &Board, track_margin: f64) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(file_name(board));
    fs::write(&path, to_json(board, track_margin))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// With `save`, [`save`] `board` into `dir` and return the path; without it, remove the file an
/// earlier run saved there, if there is one.
pub fn save_or_remove(
    dir: &Path,
    board: &Board,
    track_margin: f64,
    save: bool,
) -> Result<Option<PathBuf>, String> {
    if save {
        return self::save(dir, board, track_margin).map(Some);
    }
    let stale = dir.join(file_name(board));
    if stale.exists() {
        fs::remove_file(&stale).map_err(|e| format!("{}: {e}", stale.display()))?;
    }
    Ok(None)
}

/// How `route` routes a placed board.
#[derive(Debug, Clone, Default)]
pub struct RouteRequest {
    /// The router's time limit; `None` is no limit.
    pub route_timeout: Option<Duration>,
    /// The track margin over the file's.
    pub margin: Option<f64>,
    /// Told how far the router has got.
    pub progress: Option<fn(&str)>,
}

/// What `route` made of a placed board.
#[derive(Debug, Clone)]
pub struct Routed {
    pub board: Board,
    /// The connections left, as `fab` lists them.
    pub left: Vec<String>,
    pub tries: usize,
    /// The router stopped at its time limit.
    pub stopped: bool,
    pub elapsed: Duration,
    /// The files written, in the order written.
    pub written: Vec<PathBuf>,
}

/// Route the placed board of `file` the way `fab` does (its track margin first where there is
/// room, then the least track) and write its `.kicad_pcb`, `.kicad_pro` and `.svg` into `out`,
/// and `routing-failure.txt` when connections are left (one of an earlier run is removed when
/// none are).
pub fn route(
    file: PlacedFile,
    out: &Path,
    request: &RouteRequest,
    say: &dyn Fn(String),
) -> Result<Routed, String> {
    let mut board = file.board;
    let margin = request.margin.unwrap_or(file.track_margin);
    let started = Instant::now();
    let routing = route_with_margin(
        &mut board,
        request.route_timeout,
        request.progress,
        margin,
        say,
    );
    let elapsed = started.elapsed();
    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let name = &board.name;
    let mut written = Vec::new();
    for (path, contents) in [
        (out.join(format!("{name}.kicad_pcb")), kicad_pcb(&board)),
        (out.join(format!("{name}.kicad_pro")), kicad_pro(&board)),
        (out.join(format!("{name}.svg")), svg::render(&board)),
    ] {
        fs::write(&path, contents).map_err(|e| format!("{}: {e}", path.display()))?;
        written.push(path);
    }
    let failure = out.join(ROUTING_FAILURE);
    if routing.left.is_empty() {
        if failure.exists() {
            fs::remove_file(&failure).map_err(|e| format!("{}: {e}", failure.display()))?;
        }
    } else {
        let report = routing_failure(name, board.outline.min, &routing.left, routing.tries);
        fs::write(&failure, report).map_err(|e| format!("{}: {e}", failure.display()))?;
        written.push(failure);
    }
    Ok(Routed {
        left: routing.left.lines.iter().map(ToString::to_string).collect(),
        tries: routing.tries,
        stopped: routing.stopped,
        elapsed,
        written,
        board,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_of_another_format_is_refused_with_what_to_do() {
        let e = from_json(r#"{"format": 0, "track_margin": 1.5, "board": {}}"#).unwrap_err();
        assert!(e.contains("format 0"), "{e}");
        assert!(e.contains("--save-placed"), "{e}");
        let e = from_json(r#"{"board": {}}"#).unwrap_err();
        assert!(e.contains("(none)"), "{e}");
        let e = from_json("not json").unwrap_err();
        assert!(e.starts_with("not a placed board"), "{e}");
    }
}
