//! `netsmith layout export` and `layout import` (issue #781): the placed board as a KiCad board,
//! with no copper, whatever the placement check finds, and the moves a person made to it in
//! KiCad brought back as entries of the layout file.
//!
//! [`export`] writes the board as `dev board3d` and the editor place it (a layout file the placer
//! refuses gives the board where the placer ended, with the file's places): the `.kicad_pcb`, the
//! `.kicad_pro` beside it with the same name, and a list of the findings beside them
//! (`<stem>.findings.txt`). The board says it is not for fabrication in its title block and in a
//! text on `Cmts.User` above the outline: it is not routed, and `fab` writes the board to order.
//!
//! [`import`] reads the footprints of such a board after a person moved them. A footprint is the
//! design's part whose path its hidden `netsmith_path` property names (every `.kicad_pcb`
//! netsmith writes has it), so a designator renumbered in KiCad changes nothing; a footprint
//! without one is said and left out. A part KiCad has elsewhere than the board placed now (its
//! place, turn or face) gets an entry, written as `layout set` writes one: an entry that exists
//! keeps its lock and source, a new one is `source = "hand"`, unlocked unless asked. When a part
//! moved, every part the placer placed (one with no entry) gets an entry where the KiCad board
//! has it, unlocked with `source = "auto"` as `layout capture` writes one: else the placer would
//! arrange those parts again round the moved ones, and the board would not be the one the person
//! left in KiCad. Places are in the layout file's frame, from the outline's top left corner, so
//! the outline must stay where netsmith put it.

use std::path::Path;

use netsmith_pcb::{Board, Layout, LayoutFace as Face, PartPlace, Side};
use netsmith_sexp::Sexp;
use serde::Serialize;

use crate::board3d::Placing;

/// The text the exported board carries in its title block and above its outline.
pub const NOT_FOR_FABRICATION: &str = "NOT FOR FABRICATION: placed by netsmith, not routed; `netsmith board fab` writes the board to order";

/// What [`export`] wrote and what the placement check found on the board.
#[derive(Debug, Serialize)]
pub struct Exported {
    pub board: String,
    /// The `.kicad_pcb`, the `.kicad_pro` and the list of findings.
    pub file: String,
    pub project: String,
    pub findings_file: String,
    pub parts: usize,
    /// The faults of the placement check on the board as written, in the layout file's frame.
    pub placement: Vec<netsmith_pcb::PlacementFault>,
    /// The lines of the physical file and locks of the layout file that cannot all hold.
    pub conflicts: Vec<netsmith_pcb::Conflict>,
    /// Why the placer refused the layout file, when it did: the board is where it ended.
    pub placement_refused: Option<String>,
}

impl Exported {
    /// How many findings the board has: faults, conflicts and a refusal.
    pub fn findings(&self) -> usize {
        self.placement.len() + self.conflicts.len() + usize::from(self.placement_refused.is_some())
    }

    /// One line for each finding, as the findings file lists them.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(refused) = &self.placement_refused {
            lines.push(format!("refused: {}", refused.trim_end()));
        }
        lines.extend(
            self.conflicts
                .iter()
                .map(|c| format!("conflict: {}", c.text)),
        );
        lines.extend(
            self.placement
                .iter()
                .map(|f| format!("placement: {}", f.text)),
        );
        lines
    }

    /// What the command prints for a person.
    pub fn text(&self) -> String {
        let mut out = format!(
            "{}: wrote {} ({} parts, placed, not routed: not for fabrication) and {}\n",
            self.board, self.file, self.parts, self.project
        );
        match self.findings() {
            0 => out.push_str("the placement check finds nothing\n"),
            n => {
                out.push_str(&format!(
                    "{n} finding(s) of the placement, listed in {}:\n",
                    self.findings_file
                ));
                for line in self.lines() {
                    out.push_str(&format!("  {line}\n"));
                }
            }
        }
        out
    }
}

/// The text of the `.kicad_pcb` of a placed `board`, marked as not for fabrication: the mark and
/// the number of findings in the title block's comments, and the mark as a text on `Cmts.User`
/// above the outline's top left corner (a layer that no Gerber of a fab package plots).
pub fn marked_kicad_pcb(board: &Board, findings: usize) -> String {
    let text = netsmith_pcb::kicad_pcb(board);
    let Ok(Sexp::List(mut items)) = Sexp::parse(&text) else {
        unreachable!("netsmith reads the boards it writes")
    };
    let count = match findings {
        0 => "the placement check finds nothing".to_string(),
        n => format!("{n} placement finding(s): see the .findings.txt beside this board"),
    };
    for item in &mut items {
        if item.head() == Some("title_block")
            && let Sexp::List(block) = item
        {
            block.push(Sexp::node(
                "comment",
                vec![Sexp::atom("1"), Sexp::string(NOT_FOR_FABRICATION)],
            ));
            block.push(Sexp::node(
                "comment",
                vec![Sexp::atom("2"), Sexp::string(count.clone())],
            ));
        }
    }
    let corner = board.outline.min;
    items.push(Sexp::node(
        "gr_text",
        vec![
            Sexp::string(NOT_FOR_FABRICATION),
            Sexp::node(
                "at",
                vec![
                    Sexp::num(corner.x),
                    Sexp::num(corner.y - 2.0),
                    Sexp::num(0.0),
                ],
            ),
            Sexp::node("layer", vec![Sexp::string("Cmts.User")]),
            Sexp::node(
                "effects",
                vec![
                    Sexp::node(
                        "font",
                        vec![
                            Sexp::node("size", vec![Sexp::num(1.5), Sexp::num(1.5)]),
                            Sexp::node("thickness", vec![Sexp::num(0.3)]),
                        ],
                    ),
                    Sexp::node("justify", vec![Sexp::atom("left"), Sexp::atom("bottom")]),
                ],
            ),
        ],
    ));
    Sexp::List(items).to_kicad_string()
}

/// The three files of an export, in memory: [`export`] writes them, and the editor's download
/// hands them out as a zip (issue #1001).
#[derive(Debug)]
pub struct Package {
    pub exported: Exported,
    /// The text of the `.kicad_pcb`, marked as not for fabrication.
    pub pcb: String,
    /// The text of the `.kicad_pro` beside it.
    pub pro: String,
    /// The list of the placement's findings.
    pub findings: String,
}

impl Package {
    /// The files in a zip, inside a folder of the board file's stem so that unzipping gives one
    /// folder KiCad opens as a project. Every entry has the same fixed timestamp and
    /// permissions, so equal input gives equal bytes.
    pub fn zip(&self) -> Result<Vec<u8>, String> {
        use std::io::Write as _;
        use zip::write::SimpleFileOptions;
        let err = |e: &dyn std::fmt::Display| format!("zip: {e}");
        let named = |file: &str| {
            Path::new(file)
                .file_name()
                .map_or_else(|| file.to_string(), |n| n.to_string_lossy().into_owned())
        };
        let board_file = named(&self.exported.file);
        let folder = board_file
            .strip_suffix(".kicad_pcb")
            .unwrap_or(&board_file)
            .to_string();
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip::DateTime::default());
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        writer
            .add_directory(format!("{folder}/"), options.unix_permissions(0o755))
            .map_err(|e| err(&e))?;
        for (file, text) in [
            (board_file, &self.pcb),
            (named(&self.exported.project), &self.pro),
            (named(&self.exported.findings_file), &self.findings),
        ] {
            writer
                .start_file(format!("{folder}/{file}"), options.unix_permissions(0o644))
                .map_err(|e| err(&e))?;
            writer.write_all(text.as_bytes()).map_err(|e| err(&e))?;
        }
        Ok(writer.finish().map_err(|e| err(&e))?.into_inner())
    }
}

/// The files of the placed `board` (named `label`) as [`export`] writes them to `out`, with
/// nothing written.
pub fn package(label: &str, board: &Board, placing: Placing, out: &Path) -> Package {
    let project = out.with_extension("kicad_pro");
    let findings_file = out.with_extension("findings.txt");
    let exported = Exported {
        board: label.to_string(),
        file: out.display().to_string(),
        project: project.display().to_string(),
        findings_file: findings_file.display().to_string(),
        parts: board.parts.len(),
        placement: board.placement_faults(),
        conflicts: placing.conflicts,
        placement_refused: placing.refused,
    };
    let mut findings = format!(
        "{label}: {NOT_FOR_FABRICATION}\n\nThe findings of the placement on {} ({}):\n",
        exported.file,
        exported.findings()
    );
    for line in exported.lines() {
        findings.push_str(&format!("{line}\n"));
    }
    Package {
        pcb: marked_kicad_pcb(board, exported.findings()),
        pro: netsmith_pcb::kicad_pro(board),
        findings,
        exported,
    }
}

/// Write the placed `board` (named `label`) to `out` (a `.kicad_pcb`), with its `.kicad_pro` and
/// the list of the placement's findings (`<stem>.findings.txt`) beside it. The folder is made.
pub fn export(
    label: &str,
    board: &Board,
    placing: Placing,
    out: &Path,
) -> Result<Exported, String> {
    let package = package(label, board, placing, out);
    if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    for (file, text) in [
        (out.to_path_buf(), &package.pcb),
        (out.with_extension("kicad_pro"), &package.pro),
        (out.with_extension("findings.txt"), &package.findings),
    ] {
        std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(package.exported)
}

/// One footprint of a KiCad board as [`import`] reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Footprint {
    pub reference: String,
    pub library_id: String,
    /// The member path of its `netsmith_path` property, if it has one.
    pub path: Option<String>,
    /// Its origin, in the KiCad board's own frame (mm, y down).
    pub at: [f64; 2],
    /// Its orientation as the file states it.
    pub orientation: f64,
    pub side: Side,
}

impl Footprint {
    /// The turn netsmith gives a part that KiCad states so: a footprint on the back is written
    /// turned by a half more than the part (`netsmith_pcb::export`'s `file_orientation`), from 0
    /// up to 360.
    pub fn turn(&self) -> f64 {
        let turn = match self.side {
            Side::Back => self.orientation - 180.0,
            Side::Front => self.orientation,
        };
        normal_turn(turn)
    }
}

/// `turn` from 0 up to 360, to 0.0001 degree.
fn normal_turn(turn: f64) -> f64 {
    let t = (turn.rem_euclid(360.0) * 1e4).round() / 1e4;
    if t >= 360.0 { 0.0 } else { t + 0.0 }
}

/// The footprints of the `.kicad_pcb` text `pcb`.
pub fn footprints(pcb: &str) -> Result<Vec<Footprint>, String> {
    let tree = Sexp::parse(pcb).map_err(|e| format!("not a KiCad board: {e}"))?;
    if tree.head() != Some("kicad_pcb") {
        return Err("not a KiCad board: it does not start with `(kicad_pcb`".to_string());
    }
    let number = |s: &Sexp| s.text().and_then(|t| t.parse::<f64>().ok());
    let mut out = Vec::new();
    for fp in tree.find_all("footprint") {
        let library_id = fp
            .args()
            .first()
            .and_then(Sexp::text)
            .unwrap_or_default()
            .to_string();
        let property = |name: &str| {
            fp.find_all("property")
                .find(|p| p.args().first().and_then(Sexp::text) == Some(name))
                .and_then(|p| p.args().get(1))
                .and_then(Sexp::text)
                .map(str::to_string)
        };
        let reference = property("Reference").unwrap_or_default();
        let at = fp.find("at").map(Sexp::args).unwrap_or_default();
        let (Some(x), Some(y)) = (at.first().and_then(number), at.get(1).and_then(number)) else {
            return Err(format!(
                "footprint {reference} ({library_id}) has no place: not a KiCad board"
            ));
        };
        let side = match fp
            .find("layer")
            .and_then(|l| l.args().first())
            .and_then(Sexp::text)
        {
            Some("B.Cu") => Side::Back,
            _ => Side::Front,
        };
        out.push(Footprint {
            reference,
            library_id,
            path: property(netsmith_pcb::PATH_PROPERTY),
            at: [x, y],
            orientation: at.get(2).and_then(number).unwrap_or(0.0),
            side,
        });
    }
    Ok(out)
}

/// Where a part is, in the layout file's frame.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spot {
    pub at: [f64; 2],
    pub turn: f64,
    pub side: &'static str,
}

/// A part the KiCad board has elsewhere.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Move {
    pub path: String,
    pub refdes: String,
    /// Where the board places it now, and where the KiCad board has it.
    pub from: Spot,
    pub to: Spot,
    /// Whether the layout file had an entry for it before.
    pub existed: bool,
    pub lock: bool,
}

/// What [`import`] found and what it changes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Imported {
    pub board: String,
    pub file: String,
    pub moved: Vec<Move>,
    /// How many parts the KiCad board has where the board places them now.
    pub unchanged: usize,
    /// The parts of those that the placer placed, given an entry where they are (`source =
    /// "auto"`, unlocked) because a part moved; empty when none moved.
    pub held: Vec<String>,
    /// The parts of the board the KiCad board has no footprint for: their entries are kept.
    pub missing: Vec<String>,
    /// What was left out or is worth knowing, one sentence each.
    pub notes: Vec<String>,
}

impl Imported {
    /// What the command prints for a person.
    pub fn text(&self) -> String {
        let mut out = format!(
            "{}: {} part(s) moved in KiCad, {} where the board places them\n",
            self.board,
            self.moved.len(),
            self.unchanged
        );
        for m in &self.moved {
            out.push_str(&format!(
                "  {} ({}): {} -> {}{}\n",
                m.path,
                m.refdes,
                spot_text(&m.from),
                spot_text(&m.to),
                if m.lock { ", locked" } else { "" }
            ));
        }
        if !self.held.is_empty() {
            out.push_str(&format!(
                "note: {} part(s) the placer placed get an entry where they are (`source = \"auto\"`, unlocked), so that the board stays as it is on the KiCad board\n",
                self.held.len()
            ));
        }
        for path in &self.missing {
            out.push_str(&format!(
                "note: `{path}` has no footprint on the KiCad board: its entry is as it was\n"
            ));
        }
        for note in &self.notes {
            out.push_str(&format!("note: {note}\n"));
        }
        out
    }
}

fn spot_text(s: &Spot) -> String {
    format!("({}, {}) turned {} {}", s.at[0], s.at[1], s.turn, s.side)
}

fn face_word(side: Side) -> &'static str {
    match side {
        Side::Back => "bottom",
        Side::Front => "top",
    }
}

/// To 0.01 mm, as every entry of the layout file is; `+ 0.0` turns -0.0 into 0.0.
fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0 + 0.0
}

/// What the KiCad board `pcb` (its text; `name` for the messages) changes of the placed `board`,
/// and the layout file `layout` (`None`: there is none yet) with an entry for each part it moved.
/// A part counts as moved when its place differs by more than 0.005 mm in x or y, its turn by
/// more than 0.001 degree, or its face. `lock` locks every entry it writes; without it, an
/// entry keeps its lock and a new one is unlocked.
pub fn import(
    board: &Board,
    pcb: &str,
    name: &str,
    layout: Option<Layout>,
    file: &str,
    lock: bool,
) -> Result<(Imported, Layout), String> {
    let footprints = footprints(pcb).map_err(|e| format!("{name}: {e}"))?;
    let mut next = layout.unwrap_or_default();
    let table = next.boards.entry(board.name.clone()).or_default();
    let corner = board.outline.min;
    let mut imported = Imported {
        board: board.name.clone(),
        file: file.to_string(),
        moved: Vec::new(),
        unchanged: 0,
        held: Vec::new(),
        missing: Vec::new(),
        notes: Vec::new(),
    };
    let mut seen = std::collections::BTreeSet::new();
    // The parts where the board places them that have no entry: the placer's.
    let mut unplaced = Vec::new();
    for fp in &footprints {
        let Some(path) = &fp.path else {
            imported.notes.push(format!(
                "footprint {} ({}) has no `{}` property, so it is no part of the design: left out",
                fp.reference,
                fp.library_id,
                netsmith_pcb::PATH_PROPERTY
            ));
            continue;
        };
        let Some(part) = board.parts.iter().find(|p| &p.path == path) else {
            imported.notes.push(format!(
                "footprint {} names `{path}`, which the board has no part at: left out",
                fp.reference
            ));
            continue;
        };
        if !seen.insert(path.clone()) {
            imported.notes.push(format!(
                "footprint {} names `{path}` as an earlier footprint does: left out",
                fp.reference
            ));
            continue;
        }
        if fp.library_id != part.library_id {
            imported.notes.push(format!(
                "`{path}` is footprint `{}` on the KiCad board and `{}` in the design: only its place is brought back",
                fp.library_id, part.library_id
            ));
        }
        let now = part.at - corner;
        let from = Spot {
            at: [round(now.x), round(now.y)],
            turn: normal_turn(part.rotation),
            side: face_word(part.side),
        };
        let to = Spot {
            at: [round(fp.at[0] - corner.x), round(fp.at[1] - corner.y)],
            turn: fp.turn(),
            side: face_word(fp.side),
        };
        let turned = {
            let d = (to.turn - from.turn).rem_euclid(360.0);
            d.min(360.0 - d) > 1e-3
        };
        let shifted = (fp.at[0] - part.at.x).abs() > 0.005 || (fp.at[1] - part.at.y).abs() > 0.005;
        let side = match fp.side {
            Side::Back => Face::Bottom,
            Side::Front => Face::Top,
        };
        if !turned && !shifted && fp.side == part.side {
            imported.unchanged += 1;
            if !table.parts.contains_key(path) {
                unplaced.push((
                    path.clone(),
                    PartPlace {
                        at: to.at,
                        rotation: to.turn,
                        side,
                        text: part.reference_side.into(),
                        lock: false,
                        source: Some("auto".to_string()),
                    },
                ));
            }
            continue;
        }
        let old = table.parts.get(path);
        let place = match old {
            Some(old) => PartPlace {
                at: to.at,
                rotation: to.turn,
                side,
                lock: lock || old.lock,
                ..old.clone()
            },
            None => PartPlace {
                at: to.at,
                rotation: to.turn,
                side,
                text: Default::default(),
                lock,
                source: Some("hand".to_string()),
            },
        };
        imported.moved.push(Move {
            path: path.clone(),
            refdes: part.refdes.clone(),
            from,
            to,
            existed: old.is_some(),
            lock: place.lock,
        });
        table.parts.insert(path.clone(), place);
    }
    if !imported.moved.is_empty() {
        for (path, place) in unplaced {
            imported.held.push(path.clone());
            table.parts.insert(path, place);
        }
    }
    imported.missing = board
        .parts
        .iter()
        .filter(|p| !seen.contains(&p.path))
        .map(|p| p.path.clone())
        .collect();
    Ok((imported, next))
}

/// The layout file at `file`, if there is one.
pub fn read_layout(file: &Path) -> Result<Option<Layout>, String> {
    let name = file.display().to_string();
    match std::fs::read_to_string(file) {
        Ok(text) => Layout::parse(&text, &name)
            .map(Some)
            .map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = r#"(kicad_pcb (version 20241229)
      (footprint "Resistor_SMD:R_0603_1608Metric" (layer "F.Cu") (at 10 12 90)
        (property "Reference" "R1") (property "netsmith_path" "stage.r"))
      (footprint "Capacitor_SMD:C_0603_1608Metric" (layer "B.Cu") (at 4.5 6)
        (property "Reference" "C1") (property "netsmith_path" "stage.c"))
      (footprint "Logo" (layer "F.Cu") (at 1 1) (property "Reference" "G1")))"#;

    #[test]
    fn a_footprint_is_read_with_its_path_its_place_and_netsmiths_turn() {
        let fps = footprints(BOARD).unwrap();
        assert_eq!(fps.len(), 3);
        assert_eq!(fps[0].path.as_deref(), Some("stage.r"));
        assert_eq!((fps[0].at, fps[0].turn()), ([10.0, 12.0], 90.0));
        // A footprint on the back is written turned by a half more than the part.
        assert_eq!((fps[1].side, fps[1].turn()), (Side::Back, 180.0));
        assert_eq!(fps[2].path, None);
    }

    #[test]
    fn a_file_that_is_not_a_kicad_board_is_refused() {
        assert!(
            footprints("(kicad_sch)")
                .unwrap_err()
                .contains("not a KiCad board")
        );
        assert!(footprints("((").is_err());
    }

    #[test]
    fn a_turn_is_from_0_up_to_360() {
        assert_eq!(normal_turn(-90.0), 270.0);
        assert_eq!(normal_turn(360.0), 0.0);
        assert_eq!(normal_turn(-0.00001), 0.0);
    }
}
