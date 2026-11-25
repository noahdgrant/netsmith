//! `netsmith board check-footprint` (issue #1684): one footprint's pads against the rules of a
//! board's process, with nothing placed or routed.
//!
//! **What judges it.** The design rules check `fab` runs ([`crate::drc::of_board`]), on a board
//! of one part: the footprint alone on the board's process, its rules and the process's extras
//! (the space between surface-mount pads, `.min_mask_bridge`, the holes, the rings, the
//! silkscreen), in an outline far from its copper, with no other part, no copper, no pour, plane,
//! keep-out or waiver. Nothing here measures: the figures are those of the check's findings
//! ([`netsmith_geom::DrcDistance`]). The findings kept are those of the footprint's own items,
//! less the ones of connections (`unconnected_items`, `track_dangling`, `via_dangling`) and of
//! the outline, which a board of one part cannot say anything about.
//!
//! **The nets.** Of a part of the design, each pad has the net the design gives it, so two pads
//! of one net are not held apart. Of a footprint file, each pad number is a net of its own
//! (`pad-1`, ...), so every two pads of different numbers are held apart; pads with one number
//! are one net, as a board joins them, and a pad with no number has no net.

use netsmith_geom::{DrcFinding, DrcRule, DrcSeverity};
use netsmith_pcb::{Board, BoardNet, Footprint, PadRef, Placed, Pt, Rect, Side};

/// How far the outline of the one-part board is from the footprint's courtyard, mm: beyond any
/// rule's reach of the outline.
const ROOM: f64 = 10.0;

/// The designator of a footprint file on the one-part board.
const ALONE: &str = "FP1";

/// One finding of the footprint.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PadFault {
    /// `error`, `warning` or `note`.
    pub severity: String,
    /// The rule's id (`pad_to_pad`, `solder_mask_bridge`, `clearance`, ...).
    pub rule: String,
    /// The pad numbers it names, in order, once each.
    pub pads: Vec<String>,
    /// The items as the check names them (`SMD pad 1 [A] of U1 on F.Cu`).
    pub items: Vec<String>,
    /// The space the rule asks for and the space measured, mm, for a rule of a space.
    pub required: Option<f64>,
    pub measured: Option<f64>,
    /// `required - measured`, mm.
    pub shortfall: Option<f64>,
    /// Where, mm in the footprint's own frame (from its origin, y down, as the file has it).
    pub at: [f64; 2],
    pub layer: Option<String>,
    /// The check's message.
    pub message: String,
}

/// What `board check-footprint` found ([module](self)).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FootprintCheck {
    /// The board whose process judged it.
    pub board: String,
    /// The process's name.
    pub process: String,
    /// The part's designator, for a part of the design; `None` for a footprint file.
    pub part: Option<String>,
    /// The footprint: its library id, or the file's path.
    pub footprint: String,
    /// `part` (each pad has the part's net) or `pad` (each pad number a net of its own).
    pub nets: String,
    /// The footprint's terminal pads: its paste apertures are not counted (they are checked).
    pub pads: usize,
    /// No error.
    pub legal: bool,
    /// The errors, the largest shortfall first.
    pub errors: Vec<PadFault>,
    /// The warnings and notes, which do not make it illegal.
    pub warnings: Vec<PadFault>,
}

impl FootprintCheck {
    /// The text for a person.
    pub fn text(&self) -> String {
        let who = match &self.part {
            Some(p) => format!("{p} ({})", self.footprint),
            None => self.footprint.clone(),
        };
        let mut out = format!(
            "{who}: {} pad(s), {}, against the process {} of {}: {}\n",
            self.pads,
            if self.nets == "part" {
                "each pad on its part's net"
            } else {
                "each pad number on a net of its own"
            },
            self.process,
            self.board,
            if self.legal {
                "no rule broken".to_string()
            } else {
                format!("{} rule(s) broken", self.errors.len())
            }
        );
        for f in self.errors.iter().chain(&self.warnings) {
            let pads = match f.pads.as_slice() {
                [] => String::new(),
                [one] => format!(" pad {one}"),
                many => format!(" pads {}", many.join(" and ")),
            };
            let figures = match (f.required, f.measured, f.shortfall) {
                (Some(r), Some(m), Some(s)) => {
                    format!("needs {r:.4} mm, has {m:.4} mm, short by {s:.4} mm")
                }
                _ => f.message.clone(),
            };
            out.push_str(&format!(
                "  {} [{}]{pads}: {figures} at ({:.3}, {:.3}){}\n",
                f.severity,
                f.rule,
                f.at[0],
                f.at[1],
                f.layer
                    .as_ref()
                    .map(|l| format!(" on {l}"))
                    .unwrap_or_default()
            ));
        }
        out.push_str("places in mm from the footprint's origin, y down\n");
        out
    }
}

/// The board of `design` with its parts where [`Board::unplaced`] leaves them, nothing placed:
/// what [`of_part`] and [`of_footprint`] take the parts, the nets and the process from.
pub fn board_of(
    design: &netsmith_core::Design,
    libraries: &netsmith_pipeline::Libraries,
) -> Result<Board, String> {
    let options = netsmith_pcb::place_options(design, None);
    let unplaced = Board::unplaced(design, &libraries.mapping, &libraries.footprints, &options)
        .map_err(|e| e.to_string())?;
    Ok(unplaced.into_board(
        design,
        Rect::new(Pt::new(0.0, 0.0), Pt::new(100.0, 100.0)),
        0.0,
    ))
}

/// The part `reference` (a designator or a path of the design) of `board` (the design's board,
/// placed or not), checked alone with its nets.
pub fn of_part(board: &Board, reference: &str) -> Result<FootprintCheck, String> {
    let part = board
        .parts
        .iter()
        .find(|p| p.refdes == reference || p.path == reference)
        .ok_or_else(|| {
            format!(
                "the board {} has no part `{reference}` (a designator such as `U1`, or a path)",
                board.name
            )
        })?
        .clone();
    let nets = board
        .nets
        .iter()
        .filter_map(|n| {
            let pads: Vec<PadRef> = n
                .pads
                .iter()
                .filter(|p| p.refdes == part.refdes)
                .cloned()
                .collect();
            (!pads.is_empty()).then(|| BoardNet {
                name: n.name.clone(),
                pads,
            })
        })
        .collect();
    let footprint = part.library_id.clone();
    check(board, part, nets, Some(footprint), "part")
}

/// The footprint `footprint` (read from `shown`) checked alone on the process of `board`, each
/// pad number on a net of its own.
pub fn of_footprint(
    board: &Board,
    footprint: Footprint,
    shown: &str,
) -> Result<FootprintCheck, String> {
    let mut part = board
        .parts
        .first()
        .ok_or_else(|| {
            format!(
                "the board {} has no part to stand a footprint in for",
                board.name
            )
        })?
        .clone();
    let mut numbers: Vec<String> = Vec::new();
    for p in &footprint.pads {
        if !p.number.is_empty() && !numbers.contains(&p.number) {
            numbers.push(p.number.clone());
        }
    }
    let nets = numbers
        .iter()
        .map(|n| BoardNet {
            name: format!("pad-{n}"),
            pads: vec![PadRef {
                refdes: ALONE.to_string(),
                number: n.clone(),
            }],
        })
        .collect();
    part.refdes = ALONE.to_string();
    part.path = ALONE.to_string();
    part.value = footprint.name.clone();
    part.library_id = footprint.name.clone();
    part.footprint = footprint;
    part.power_pads.clear();
    part.designator = None;
    part.silk = Default::default();
    part.apart.clear();
    part.protrusion = None;
    part.layout.clear();
    part.contacts.clear();
    part.symbol = None;
    let mut checked = check(board, part, nets, None, "pad")?;
    checked.footprint = shown.to_string();
    Ok(checked)
}

/// `part` alone on a board of `template`'s process, with `nets`, checked.
fn check(
    template: &Board,
    mut part: Placed,
    nets: Vec<BoardNet>,
    footprint: Option<String>,
    nets_from: &str,
) -> Result<FootprintCheck, String> {
    part.rotation = 0.0;
    part.side = Side::Front;
    let reach = part
        .footprint
        .courtyard
        .union(&part.footprint.body)
        .expanded(ROOM);
    let size = reach.max - reach.min;
    let outline = Rect::new(Pt::new(0.0, 0.0), Pt::new(size.x, size.y));
    part.at = Pt::new(-reach.min.x, -reach.min.y);
    let origin = part.at;
    let refdes = part.refdes.clone();
    let pads = part
        .footprint
        .pads
        .iter()
        .filter(|p| p.is_terminal())
        .count();
    let mut board = template.clone();
    board.outline = outline;
    board.corner_radius = 0.0;
    board.polygon = None;
    board.cutouts.clear();
    board.pours.clear();
    board.keepouts.clear();
    board.planes.clear();
    board.tracks.clear();
    board.vias.clear();
    board.teardrops.clear();
    board.zones.clear();
    board.drc_waivers.clear();
    board.branch_currents.clear();
    board.pin_currents.clear();
    board.net_currents.clear();
    board.pad_pours.clear();
    board.overhang.clear();
    board.pinned.clear();
    board.nets = nets;
    board.parts = vec![part];
    let checked = crate::drc::of_board(&board, None, false)?;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for f in checked.report.findings.iter().filter(|f| own(f, &refdes)) {
        let fault = fault(f, origin);
        match f.severity {
            DrcSeverity::Error => errors.push(fault),
            _ => warnings.push(fault),
        }
    }
    let by_short = |a: &PadFault, b: &PadFault| {
        b.shortfall
            .unwrap_or(f64::INFINITY)
            .total_cmp(&a.shortfall.unwrap_or(f64::INFINITY))
            .then(a.rule.cmp(&b.rule))
            .then(a.pads.cmp(&b.pads))
    };
    errors.sort_by(by_short);
    warnings.sort_by(by_short);
    Ok(FootprintCheck {
        board: template.name.clone(),
        process: format!("{} {}", template.process.fabricator, template.process.name)
            .trim()
            .to_string(),
        part: footprint.as_ref().map(|_| refdes),
        footprint: footprint.unwrap_or_default(),
        nets: nets_from.to_string(),
        pads,
        legal: errors.is_empty(),
        errors,
        warnings,
    })
}

/// Whether `f` is a finding of the footprint of `refdes` that a board of that part alone can
/// make: one of its items, and not of a connection or the outline.
fn own(f: &DrcFinding, refdes: &str) -> bool {
    !matches!(
        f.rule,
        DrcRule::UnconnectedItems
            | DrcRule::TrackDangling
            | DrcRule::ViaDangling
            | DrcRule::CopperEdgeClearance
            | DrcRule::SilkEdgeClearance
            | DrcRule::HoleToEdge
            | DrcRule::VCutEdge
            | DrcRule::InvalidOutline
    ) && f.items.iter().any(|i| i.part.as_deref() == Some(refdes))
}

fn fault(f: &DrcFinding, origin: Pt) -> PadFault {
    let mut pads: Vec<String> = Vec::new();
    for p in f.items.iter().filter_map(|i| i.pad.clone()) {
        if !pads.contains(&p) {
            pads.push(p);
        }
    }
    let d = f.distance;
    PadFault {
        severity: f.severity.word().to_string(),
        rule: f.rule.id().to_string(),
        pads,
        items: f.items.iter().map(|i| i.what.clone()).collect(),
        required: d.map(|d| round(d.required)),
        measured: d.map(|d| round(d.measured)),
        shortfall: d.map(|d| round(d.shortfall())),
        at: [round(f.at.x - origin.x), round(f.at.y - origin.y)],
        layer: f.layer.clone(),
        message: f.message.clone(),
    }
}

/// To 0.0001 mm.
fn round(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
