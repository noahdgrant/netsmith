//! `netsmith board3d --board System`: the _System scene_ (ADR 0110, `netsmith_3d::system`) of a
//! system's boards placed against each other by their mates, as numbers. Each member is placed as
//! `board3d --board System/member` places it, and its report is that command's; the members'
//! motions, the mates with their mated heights and the findings across boards are added. The
//! fields are described in `docs/BOARD3D.md`; [`SCHEMA`] counts the versions of the shape.

use netsmith_3d::system::{Finding, MateInput, MemberInput, Place, SystemScene};
use netsmith_pcb::Board;
use serde::Serialize;

use crate::board3d::Report;
use crate::{Built, SolvedSystem};

/// The version of the system report's shape: a field removed or changed in meaning raises it;
/// a field added does not.
pub const SCHEMA: u32 = 1;

/// The frame, as the report states it.
pub const FRAME: &str = "the frame of the base, the first member the system declares: millimetres, right-handed, z up; the origin is the base outline's bottom-left corner on its top face (z = 0); each member's own frame is moved into it by its motion";

/// The system scene of `system`, from each member's board as placed (or routed), in the order
/// the system declares them.
pub fn scene(system: &SolvedSystem, members: &[(&Built, &Board)]) -> SystemScene {
    let inputs = members
        .iter()
        .map(|(built, board)| MemberInput {
            name: built.board.clone(),
            board,
            // A member's case model belongs to its own board's scene, not the system's.
            scene: crate::board3d::scene_without_case(built, board),
        })
        .collect();
    let mates = system
        .mates
        .iter()
        .map(|m| MateInput {
            a: m.a.clone(),
            b: m.b.clone(),
            a_member: m.a_board.clone(),
            b_member: m.b_board.clone(),
            a_seat: m.a_seat,
            b_seat: m.b_seat,
            mated_height: m.mated_height(),
            unknown: m.unknown(),
        })
        .collect();
    SystemScene::of(&system.name, inputs, mates)
}

/// The boards of `system` among `boards`, in the order the system declares them.
pub fn members<'a>(system: &SolvedSystem, boards: &'a [Built]) -> Vec<&'a Built> {
    system
        .boards
        .iter()
        .filter_map(|(name, _, _)| {
            boards
                .iter()
                .find(|b| b.system.as_deref() == Some(system.name.as_str()) && &b.board == name)
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct SystemReport {
    pub schema: u32,
    pub units: &'static str,
    pub frame: &'static str,
    pub system: String,
    /// The member whose frame the system scene is in.
    pub base: Option<String>,
    pub members: Vec<MemberReport>,
    pub mates: Vec<MateReport>,
    pub findings: Vec<FindingReport>,
}

#[derive(Debug, Serialize)]
pub struct MemberReport {
    pub member: String,
    /// `System/member`, as `--board` names it alone.
    pub board: String,
    pub motion: MotionReport,
    /// How it was placed: `{"base": true}`, or `{"mate": <index into mates>}`. Absent when apart.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placed: Option<PlacedReport>,
    /// Why no mate places it. Absent when placed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apart: Option<ApartReport>,
    /// The member's own report, in its own frame, as `--board System/member` prints it.
    pub report: Report,
}

#[derive(Debug, Serialize)]
pub struct MotionReport {
    /// By rows, acting on `(x, y, z, 1)` of the member's frame.
    pub matrix: [[f64; 4]; 4],
    /// Degrees about z, counterclockwise seen from above, after the flip.
    pub turn: f64,
    /// Turned over about the x axis, `(x, y, z) -> (x, -y, -z)`, before the turn.
    pub flip: bool,
    pub offset: [f64; 3],
}

#[derive(Debug, Serialize)]
pub struct PlacedReport {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub base: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mate: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ApartReport {
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct Range {
    pub min: f64,
    pub max: f64,
}

fn range(r: Option<(f64, f64)>) -> Option<Range> {
    r.map(|(min, max)| Range { min, max })
}

#[derive(Debug, Serialize)]
pub struct MateReport {
    pub a: String,
    pub b: String,
    pub a_member: Option<String>,
    pub b_member: Option<String>,
    /// The pads of one number on both connectors: the contacts that meet.
    pub contacts: usize,
    /// The distance between the two boards' faces: the seats added. `null` when a seat is
    /// unknown; the scene places at `min`.
    pub mated_height: Option<Range>,
    pub seats: SeatsReport,
    /// The largest distance in plan between a pad and its partner, once both members are placed.
    pub residual: Option<f64>,
    /// The distance between the two connectors' faces, once both members are placed.
    pub gap: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct SeatsReport {
    pub a: Option<Range>,
    pub b: Option<Range>,
}

#[derive(Debug, Serialize)]
pub struct FindingReport {
    pub code: &'static str,
    /// `error`, or `warning`.
    pub severity: &'static str,
    /// An error when constraints are strict (`--constraints error`), as `fab` runs it.
    pub error_when_strict: bool,
    pub message: String,
    pub members: Vec<String>,
    /// `member/path`.
    pub parts: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mate: Option<[String; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_bottom: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z_top: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
}

impl FindingReport {
    pub fn of(f: &Finding) -> FindingReport {
        FindingReport {
            code: f.code.word(),
            severity: f.code.severity(),
            error_when_strict: f.code.is_error(true),
            message: f.message.clone(),
            members: f.members.clone(),
            parts: f.parts.clone(),
            mate: f.mate.clone().map(|(a, b)| [a, b]),
            z_bottom: f.z.map(|z| z.0),
            z_top: f.z.map(|z| z.1),
            distance: f.distance,
        }
    }
}

/// The report of `scene`, with each member's own report in the system's order.
pub fn report(scene: &SystemScene, reports: Vec<Report>) -> SystemReport {
    SystemReport {
        schema: SCHEMA,
        units: "mm",
        frame: FRAME,
        system: scene.name.clone(),
        base: scene.members.first().map(|m| m.name.clone()),
        members: scene
            .members
            .iter()
            .zip(reports)
            .map(|(m, report)| MemberReport {
                member: m.name.clone(),
                board: format!("{}/{}", scene.name, m.name),
                motion: MotionReport {
                    matrix: m.motion.matrix(),
                    turn: m.motion.turn,
                    flip: m.motion.flip,
                    offset: m.motion.offset,
                },
                placed: match &m.place {
                    Place::Base => Some(PlacedReport {
                        base: true,
                        mate: None,
                    }),
                    Place::Mated { mate } => Some(PlacedReport {
                        base: false,
                        mate: Some(*mate),
                    }),
                    Place::Apart { .. } => None,
                },
                apart: match &m.place {
                    Place::Apart { reason } => Some(ApartReport {
                        reason: reason.clone(),
                    }),
                    _ => None,
                },
                report,
            })
            .collect(),
        mates: scene
            .mates
            .iter()
            .map(|m| MateReport {
                a: m.input.a.clone(),
                b: m.input.b.clone(),
                a_member: m.input.a_member.clone(),
                b_member: m.input.b_member.clone(),
                contacts: m.contacts,
                mated_height: range(m.input.mated_height),
                seats: SeatsReport {
                    a: range(m.input.a_seat),
                    b: range(m.input.b_seat),
                },
                residual: m.residual,
                gap: m.gap,
            })
            .collect(),
        findings: scene.findings.iter().map(FindingReport::of).collect(),
    }
}

impl SystemReport {
    /// The report as JSON, pretty-printed, with a final newline.
    pub fn json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("the report is plain data");
        text.push('\n');
        text
    }

    /// A short summary for a person.
    pub fn summary(&self) -> String {
        let mut text = format!(
            "{}: a system of {} boards, in the frame of `{}`\n",
            self.system,
            self.members.len(),
            self.base.as_deref().unwrap_or("")
        );
        for m in &self.members {
            let how = match (&m.placed, &m.apart) {
                (Some(p), _) if p.base => "the base".to_string(),
                (Some(p), _) => {
                    let mate = p.mate.and_then(|k| self.mates.get(k));
                    format!(
                        "placed by the mate of `{}` and `{}`: turned {} degrees{}, offset ({}, {}, {}) mm",
                        mate.map_or("", |x| x.a.as_str()),
                        mate.map_or("", |x| x.b.as_str()),
                        m.motion.turn,
                        if m.motion.flip { " and flipped" } else { "" },
                        m.motion.offset[0],
                        m.motion.offset[1],
                        m.motion.offset[2]
                    )
                }
                (None, Some(a)) => format!("apart: {}", a.reason),
                (None, None) => String::new(),
            };
            text += &format!("member `{}`: {how}\n", m.member);
        }
        for m in &self.mates {
            if let Some(h) = &m.mated_height {
                text += &format!(
                    "mate `{}` ~ `{}`: {} contacts, mated height {:.2} to {:.2} mm\n",
                    m.a, m.b, m.contacts, h.min, h.max
                );
            } else {
                text += &format!(
                    "mate `{}` ~ `{}`: no mated height (a seat is unknown)\n",
                    m.a, m.b
                );
            }
        }
        for f in &self.findings {
            text += &format!("{}: {}: {}\n", f.severity, f.code, f.message);
        }
        text
    }
}
