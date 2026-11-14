//! The routes file beside a source file (ADR 0030, decision 2): `<stem>.routes.toml`, beside the
//! layout file `<stem>.pcb.toml`. Its format and what its copper does are in the header of
//! the routes module of `netsmith-pcb` (`netsmith_pcb::Routes`); this module finds it, reads it and puts it on a placed board, for
//! `fab`, the editor's PCB view and the `netsmith routes` commands. The import of a KiCad
//! board's copper is `netsmith_tools::routes_import`.

use std::path::{Path, PathBuf};

use netsmith_pcb::{
    Board, LayoutFace as Face, NetRoutes, RouteTrack, RouteVia, Routes, apply_routes,
};

/// The routes file of a source file: `led.nsm` has `led.routes.toml`.
pub fn routes_path(source: &Path) -> PathBuf {
    netsmith_pcb::routes_path(source)
}

/// The routes file beside a layout file: `led.pcb.toml` has `led.routes.toml`. Whatever reads
/// a source's layout file reads its routes file too.
pub fn beside_layout(layout: &Path) -> PathBuf {
    let name = layout
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let stem = name.strip_suffix(".pcb.toml").unwrap_or(name);
    layout.with_file_name(format!("{stem}.routes.toml"))
}

/// The routes in the file at `file`, with the name to quote it by, or `None` if there is no
/// such file. `boards` are the names of all the boards of the source file, so that a table for
/// a board that is gone is found. A file that cannot be read or is wrong is an error naming it.
pub fn read(file: &Path, boards: &[String]) -> Result<Option<(Routes, String)>, String> {
    let name = file.display().to_string();
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{name}: {e}")),
    };
    let routes = Routes::parse(&text, &name).map_err(|e| e.to_string())?;
    let known: Vec<&str> = boards.iter().map(String::as_str).collect();
    routes
        .check_boards(&name, &known)
        .map_err(|e| e.to_string())?;
    Ok(Some((routes, name)))
}

/// Put the routes file beside the layout file `layout` (if there is one) on the placed `board`,
/// and return what of it was not used as written, as warnings (the routes module of `netsmith-pcb` (`netsmith_pcb::Routes`)).
pub fn apply_beside(board: &mut Board, layout: Option<&Path>) -> Result<Vec<String>, String> {
    let Some(layout) = layout else {
        return Ok(Vec::new());
    };
    let file = beside_layout(layout);
    let Some((routes, name)) = read_unchecked(&file)? else {
        return Ok(Vec::new());
    };
    let notes = apply_routes(board, &routes, &name).map_err(|e| e.to_string())?;
    Ok(notes.iter().map(|n| format!("{name}: {n}")).collect())
}

/// [`read`] without the check of the boards' names, which `fab` makes once for the source: a
/// file with the tables of several boards is applied to each board in turn.
fn read_unchecked(file: &Path) -> Result<Option<(Routes, String)>, String> {
    let name = file.display().to_string();
    match std::fs::read_to_string(file) {
        Ok(text) => Ok(Some((
            Routes::parse(&text, &name).map_err(|e| e.to_string())?,
            name,
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

/// The routes file at `file`, or none when there is none.
pub fn load(file: &Path) -> Result<Routes, String> {
    Ok(read_unchecked(file)?.map(|(r, _)| r).unwrap_or_default())
}

/// Write `routes` to `file`, or remove the file when it holds no record.
pub fn save(file: &Path, routes: &Routes) -> Result<(), String> {
    let empty = routes.boards.values().all(|b| b.nets.is_empty());
    let result = if empty {
        match std::fs::remove_file(file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        std::fs::write(file, routes.to_text())
    };
    result.map_err(|e| format!("{}: {e}", file.display()))
}

/// `board` with `routes` on it, as `fab` would route it, and what of the file was not used.
pub fn with_routes(board: &Board, routes: &Routes) -> Result<(Board, Vec<String>), String> {
    let mut board = board.clone();
    let notes = apply_routes(&mut board, routes, "the routes file").map_err(|e| e.to_string())?;
    Ok((board, notes.iter().map(ToString::to_string).collect()))
}

/// The errors of netsmith's design rules check of `board` as it is, each as a line, less those of
/// copper not yet drawn: the connections left (the router makes them) and a track or via end that
/// touches nothing (a route is drawn one shape at a time, so its first shape ends in the open
/// until the next is added; `fab`'s check still finds an end left so).
pub fn drc_errors(board: &Board) -> Result<Vec<String>, String> {
    let report = netsmith_geom::drc_check(
        &netsmith_pcb::kicad_pcb(board),
        Some(&netsmith_pcb::kicad_pro(board)),
        &crate::fab::drc_extras(board),
    )
    .map_err(|e| format!("the design rules check: {e}"))?;
    Ok(report
        .errors()
        .filter(|f| {
            !matches!(
                f.rule,
                netsmith_geom::DrcRule::UnconnectedItems
                    | netsmith_geom::DrcRule::TrackDangling
                    | netsmith_geom::DrcRule::ViaDangling
            )
        })
        .map(ToString::to_string)
        .collect())
}

/// The errors of the design rules check that `after` has and `before` does not: what a change
/// to the routes file broke. A finding is the same finding when its text is.
pub fn new_errors(before: &Board, after: &Board) -> Result<Vec<String>, String> {
    let mut old = drc_errors(before)?;
    let mut new = Vec::new();
    for e in drc_errors(after)? {
        match old.iter().position(|o| *o == e) {
            Some(k) => {
                old.swap_remove(k);
            }
            None => new.push(e),
        }
    }
    Ok(new)
}

/// One shape of a net's record, numbered from 1 as `netsmith routes show` lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Track(usize),
    Via(usize),
}

/// One change to the routes file: what `netsmith routes add-track`, `add-via`, `delete`, `lock`
/// and `unlock` do, and the editor's PCB view with them. [`edit`] is the one implementation.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// A track of `net`; `width` `None` is the net's own width.
    AddTrack {
        net: String,
        layer: Face,
        width: Option<f64>,
        from: [f64; 2],
        mid: Option<[f64; 2]>,
        to: [f64; 2],
        lock: bool,
    },
    /// A via of `net`; `None` is the process's via.
    AddVia {
        net: String,
        at: [f64; 2],
        diameter: Option<f64>,
        drill: Option<f64>,
        lock: bool,
    },
    /// The net's whole record (`shape` `None`), or one shape of it. A locked one is refused.
    Delete { net: String, shape: Option<Shape> },
    /// Lock (`on`) or unlock the net (`shape` `None`) or one shape. Locking a net writes its
    /// record again with its pads as they are now.
    Lock {
        net: String,
        shape: Option<Shape>,
        on: bool,
    },
}

impl Edit {
    /// The net the edit changes.
    pub fn net(&self) -> &str {
        match self {
            Edit::AddTrack { net, .. }
            | Edit::AddVia { net, .. }
            | Edit::Delete { net, .. }
            | Edit::Lock { net, .. } => net,
        }
    }
}

/// Why [`edit`] did not change the file.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    /// The edit is refused, and why: a shape that breaks the design rules, a locked shape, a net
    /// or shape the board or the file does not have. The file is as it was.
    Refused(String),
    /// It could not be tried: the file or the board could not be read.
    Failed(String),
}

impl From<String> for Refusal {
    fn from(e: String) -> Self {
        Refusal::Failed(e)
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Refused(e) | Refusal::Failed(e) => f.write_str(e),
        }
    }
}

/// Make `change` to `routes` for `board` (placed, with no routes on it), or refuse it and leave
/// `routes` as it was. An added shape, and a net locked again (which can draw shapes a record
/// whose pads moved had dropped), is refused when the board with it has an error of the design
/// rules check that it did not have before. Says what it did.
pub fn edit(board: &Board, routes: &mut Routes, change: &Edit) -> Result<String, Refusal> {
    let net = change.net();
    let Some(hash) = netsmith_pcb::pad_hash(board, net) else {
        return Err(Refusal::Refused(format!(
            "the board {} has no net `{net}`",
            board.name
        )));
    };
    let check = matches!(
        change,
        Edit::AddTrack { .. } | Edit::AddVia { .. } | Edit::Lock { on: true, .. }
    );
    let refresh = matches!(change, Edit::Lock { on: true, .. });
    let mut next = routes.clone();
    let table = next.boards.entry(board.name.clone()).or_default();
    let record = table
        .nets
        .entry(net.to_string())
        .or_insert_with(|| NetRoutes {
            pads: hash.clone(),
            ..NetRoutes::default()
        });
    let said = change_record(board, record, change)?;
    if refresh {
        record.pads = hash;
    }
    if record.shapes() == 0 && !record.lock {
        table.nets.remove(net);
    }
    if table.nets.is_empty() {
        next.boards.remove(&board.name);
    }
    if check {
        let before = with_routes(board, routes)?.0;
        let after = with_routes(board, &next)?.0;
        let new = new_errors(&before, &after)?;
        if !new.is_empty() {
            return Err(Refusal::Refused(format!(
                "refused: the design rules check finds {} new error(s) with it, and the routes file is not changed:\n  {}",
                new.len(),
                new.join("\n  ")
            )));
        }
    }
    *routes = next;
    Ok(said)
}

/// [`edit`] of the routes file at `file`, written when the edit is made. Says what it did and
/// the file it wrote.
pub fn edit_file(board: &Board, file: &Path, change: &Edit) -> Result<String, Refusal> {
    let mut routes = load(file)?;
    let said = edit(board, &mut routes, change)?;
    save(file, &routes)?;
    Ok(format!("{said}; wrote {}", file.display()))
}

fn change_record(board: &Board, record: &mut NetRoutes, change: &Edit) -> Result<String, Refusal> {
    let missing = |net: &str, what: &str, n: usize, have: usize| {
        Refusal::Refused(format!(
            "net {net} has no {what} {n} in the routes file (it has {have}; `netsmith routes show` numbers them)"
        ))
    };
    match change {
        Edit::AddTrack {
            net,
            layer,
            width,
            from,
            mid,
            to,
            lock,
        } => {
            record.tracks.push(RouteTrack {
                layer: *layer,
                width: width.unwrap_or_else(|| board.track_width(net)),
                from: *from,
                mid: *mid,
                to: *to,
                lock: *lock,
            });
            Ok(format!("added a track to net {net}"))
        }
        Edit::AddVia {
            net,
            at,
            diameter,
            drill,
            lock,
        } => {
            record.vias.push(RouteVia {
                at: *at,
                diameter: diameter.unwrap_or(board.rules.via_diameter),
                drill: drill.unwrap_or(board.rules.via_drill),
                lock: *lock,
            });
            Ok(format!("added a via to net {net}"))
        }
        Edit::Delete { net, shape: None } => {
            let locked = record.lock
                || record.tracks.iter().any(|t| t.lock)
                || record.vias.iter().any(|v| v.lock);
            if locked {
                return Err(Refusal::Refused(format!(
                    "net {net} or a shape of it is locked: unlock it first (`netsmith routes unlock`)"
                )));
            }
            let n = record.shapes();
            record.tracks.clear();
            record.vias.clear();
            Ok(format!("deleted net {net}'s record ({n} shape(s))"))
        }
        Edit::Delete {
            net,
            shape: Some(Shape::Track(n)),
        } => {
            let have = record.tracks.len();
            let t = record
                .tracks
                .get(n.wrapping_sub(1))
                .ok_or_else(|| missing(net, "track", *n, have))?;
            if t.lock {
                return Err(Refusal::Refused(format!(
                    "track {n} of net {net} is locked: unlock it first"
                )));
            }
            record.tracks.remove(n - 1);
            Ok(format!("deleted track {n} of net {net}"))
        }
        Edit::Delete {
            net,
            shape: Some(Shape::Via(n)),
        } => {
            let have = record.vias.len();
            let v = record
                .vias
                .get(n.wrapping_sub(1))
                .ok_or_else(|| missing(net, "via", *n, have))?;
            if v.lock {
                return Err(Refusal::Refused(format!(
                    "via {n} of net {net} is locked: unlock it first"
                )));
            }
            record.vias.remove(n - 1);
            Ok(format!("deleted via {n} of net {net}"))
        }
        Edit::Lock {
            net,
            shape: None,
            on,
        } => {
            record.lock = *on;
            Ok(format!(
                "{} net {net}",
                if *on { "locked" } else { "unlocked" }
            ))
        }
        Edit::Lock {
            net,
            shape: Some(shape),
            on,
        } => {
            let word = if *on { "locked" } else { "unlocked" };
            match *shape {
                Shape::Track(n) => {
                    let have = record.tracks.len();
                    record
                        .tracks
                        .get_mut(n.wrapping_sub(1))
                        .ok_or_else(|| missing(net, "track", n, have))?
                        .lock = *on;
                    Ok(format!("{word} track {n} of net {net}"))
                }
                Shape::Via(n) => {
                    let have = record.vias.len();
                    record
                        .vias
                        .get_mut(n.wrapping_sub(1))
                        .ok_or_else(|| missing(net, "via", n, have))?
                        .lock = *on;
                    Ok(format!("{word} via {n} of net {net}"))
                }
            }
        }
    }
}

/// What `netsmith routes show` says of a board: each record, what of the file is not used, and
/// the connections left once the file's copper is on the board.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Shown {
    pub board: String,
    pub nets: Vec<ShownNet>,
    /// What of the file is not used as written.
    pub notes: Vec<String>,
    /// The connections the router would route.
    pub to_route: Vec<String>,
    /// The connections of locked nets that no copper of the file makes: left unrouted.
    pub locked_left: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ShownNet {
    pub net: String,
    pub lock: bool,
    pub tracks: Vec<RouteTrack>,
    pub vias: Vec<RouteVia>,
}

/// [`Shown`] of `board` (placed, with no routes) and `routes`.
pub fn show(board: &Board, routes: &Routes) -> Result<Shown, String> {
    let (fixed, notes) = with_routes(board, routes)?;
    let nets = routes
        .boards
        .get(&board.name)
        .map(|t| {
            t.nets
                .iter()
                .map(|(net, r)| ShownNet {
                    net: net.clone(),
                    lock: r.lock,
                    tracks: r.tracks.clone(),
                    vias: r.vias.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    let (mut to_route, mut locked_left) = (Vec::new(), Vec::new());
    for line in netsmith_pcb::Ratsnest::of(&fixed).lines {
        if fixed.fixed.is_locked(&line.net) {
            locked_left.push(line.to_string());
        } else {
            to_route.push(line.to_string());
        }
    }
    Ok(Shown {
        board: board.name.clone(),
        nets,
        notes,
        to_route,
        locked_left,
    })
}

impl Shown {
    /// The text for a person: each net's record with its shapes numbered from 1, as `routes
    /// delete`, `lock` and `unlock` name them.
    pub fn text(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let pt = |p: [f64; 2]| format!("({}, {})", p[0], p[1]);
        for n in &self.nets {
            let _ = writeln!(
                s,
                "net {}{}: {} track(s), {} via(s)",
                n.net,
                if n.lock { " (locked)" } else { "" },
                n.tracks.len(),
                n.vias.len()
            );
            for (i, t) in n.tracks.iter().enumerate() {
                let layer = if t.layer == Face::Top {
                    "top"
                } else {
                    "bottom"
                };
                let mid = t
                    .mid
                    .map(|m| format!(" through {}", pt(m)))
                    .unwrap_or_default();
                let _ = writeln!(
                    s,
                    "  track {}: {layer}, {} mm, {}{mid} to {}{}",
                    i + 1,
                    t.width,
                    pt(t.from),
                    pt(t.to),
                    if t.lock { " (locked)" } else { "" }
                );
            }
            for (i, v) in n.vias.iter().enumerate() {
                let _ = writeln!(
                    s,
                    "  via {}: {}, {} mm, drill {} mm{}",
                    i + 1,
                    pt(v.at),
                    v.diameter,
                    v.drill,
                    if v.lock { " (locked)" } else { "" }
                );
            }
        }
        if self.nets.is_empty() {
            let _ = writeln!(s, "{}: the routes file fixes no copper", self.board);
        }
        for note in &self.notes {
            let _ = writeln!(s, "warning: {note}");
        }
        let _ = writeln!(
            s,
            "the router would route {} connection(s):",
            self.to_route.len()
        );
        for l in &self.to_route {
            let _ = writeln!(s, "  {l}");
        }
        if !self.locked_left.is_empty() {
            let _ = writeln!(
                s,
                "{} connection(s) of locked nets have no copper, and stay unrouted:",
                self.locked_left.len()
            );
            for l in &self.locked_left {
                let _ = writeln!(s, "  {l}");
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_routes_file_is_beside_the_layout_file() {
        assert_eq!(
            beside_layout(Path::new("src/main.pcb.toml")),
            PathBuf::from("src/main.routes.toml")
        );
        assert_eq!(
            routes_path(Path::new("src/main.nsm")),
            PathBuf::from("src/main.routes.toml")
        );
    }
}
