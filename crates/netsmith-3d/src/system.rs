//! The _System scene_ (ADR 0110): the boards of a system placed against each other by their
//! mates, in one scene. Each member's [`Scene`] is built as it is for the board alone, in its own
//! frame ([`crate::scene3d`]), and put in place with one rigid [`Motion`]: a turn about z, with a
//! flip over when it faces the other way, and an offset.
//!
//! # How the members are placed
//!
//! - The base is the first member the system declares: the system scene is in its frame, so its
//!   motion is none and its numbers are those of `board3d --board System/base`.
//! - The mates are read in the order the system states them, again and again until none places
//!   another member: a mate with one end on a placed member places the member at its other end.
//!   Contact N meets contact N (pad N of a std header or socket), so the turn and the offset are
//!   the least-squares fit that puts each pad centre of the one connector on its partner's; the
//!   distance between the two boards' faces is the mate's _Mated height_, its least (the worst
//!   case for the checks). The member is flipped when its connector would otherwise face the
//!   same way as its partner (both on the top face, or both on the bottom).
//! - Only straight-through connectors on parallel boards are placed: every std header and socket
//!   is one, and nothing else has a `.mating` (ADR 0040).
//! - A mate whose two ends are both placed already (a card on two headers, a loop of three
//!   boards) is checked, not used: each of its pads must land on its partner within
//!   [`ALIGNMENT`], and its boards' faces must be its mated height apart, else
//!   `mate_misaligned`. The mate that placed a member is checked the same way, which finds a
//!   pinout that only fits mirrored.
//! - A member no mate places (a seat nobody states, a connector with no pads, a member joined by
//!   `~` alone) is drawn apart: beside the placed members along +x, [`APART_GAP`] past their
//!   bounds, in its own frame and unturned, with the reason. Nothing across boards is checked for
//!   it, and a `system_apart` finding says so. Never at a guessed height (ADR 0094).
//!
//! # What is checked across boards
//!
//! With parallel boards every body, board and pin protrusion is a prism: a convex polygon in plan
//! (a body's turned box, a board's outline with its rounded corners) and a z interval. A turn
//! about z and a flip keep that, so the checks are exact: a z overlap and a plan overlap measured
//! as the depth one polygon reaches into the other, with no triangles. Only prisms of different
//! members are compared, and the two connectors of a mate are exempt against each other, since
//! one goes into the other. A body that stands wholly inside a cutout of another board does not
//! meet that board.
//!
//! - `system_collision` (an error): a body, a pin protrusion or a board overlaps another
//!   member's by more than [`COLLISION`] in both plan and z, each of a stated or catalog height
//!   (or a board, or pins of a stated length).
//! - `system_unchecked` (a warning; an error with strict constraints): a collision in which a
//!   height is a package standard's maximum (a part in that package may be lower), or a part of
//!   unknown height on a face that faces another member's board over it.
//! - `mate_misaligned` (an error), above.
//! - `system_apart` (a warning; an error with strict constraints), above.
//!
//! Everything is in a fixed order: members as the system declares them, mates as it states them,
//! findings by code in the order above and then as they are found, member by member, body by
//! body. Lengths are rounded to a nanometre.

use netsmith_pcb::{Board, Side};

use crate::scene3d::{Body, HeightProvenance, Pins, Plan, Scene, pad_centres};

/// How far a pad of a mate that did not place its member may land from its partner, in mm (ADR
/// 0110).
pub(crate) const ALIGNMENT: f64 = 0.1;

/// How far two prisms of different members must reach into each other, in plan and in z, to
/// collide, in mm (ADR 0110).
pub(crate) const COLLISION: f64 = 0.01;

/// How far past the placed members' bounds, along +x, a member that cannot be placed is drawn,
/// in mm (ADR 0110).
pub(crate) const APART_GAP: f64 = 10.0;

/// A rigid motion of a member's scene into the system's: a point `p` of the member's frame is
/// `turn(flip(p)) + offset`, where the flip (when there is one) turns the board over about the x
/// axis, `(x, y, z) -> (x, -y, -z)`, and the turn is `turn` degrees about z, counterclockwise
/// seen from above.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    /// Degrees, in (-180, 180].
    pub turn: f64,
    pub flip: bool,
    pub offset: [f64; 3],
}

impl Motion {
    /// No motion: the base's.
    pub const NONE: Motion = Motion {
        turn: 0.0,
        flip: false,
        offset: [0.0, 0.0, 0.0],
    };

    /// The linear part, by rows: the turn after the flip.
    pub fn linear(&self) -> [[f64; 3]; 3] {
        let (s, c) = self.turn.to_radians().sin_cos();
        let (s, c) = (exact(s), exact(c));
        let f = if self.flip { -1.0 } else { 1.0 };
        [[c, -s * f, 0.0], [s, c * f, 0.0], [0.0, 0.0, f]].map(|row| row.map(exact))
    }

    /// The motion as a 4x4 matrix, by rows, acting on `(x, y, z, 1)`.
    pub fn matrix(&self) -> [[f64; 4]; 4] {
        let l = self.linear();
        let o = self.offset;
        [
            [l[0][0], l[0][1], l[0][2], o[0]],
            [l[1][0], l[1][1], l[1][2], o[1]],
            [l[2][0], l[2][1], l[2][2], o[2]],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }

    pub fn point(&self, p: [f64; 3]) -> [f64; 3] {
        let l = self.linear();
        [0, 1, 2].map(|r| clean(l[r][0] * p[0] + l[r][1] * p[1] + l[r][2] * p[2] + self.offset[r]))
    }

    pub fn plan(&self, p: Plan) -> Plan {
        let [x, y, _] = self.point([p.x, p.y, 0.0]);
        Plan { x, y }
    }

    /// A height in the member's frame in the system's (a motion keeps the boards level).
    pub fn z(&self, z: f64) -> f64 {
        clean(if self.flip { -z } else { z } + self.offset[2])
    }

    /// A z interval in the member's frame in the system's, low end first.
    pub fn z_range(&self, lo: f64, hi: f64) -> (f64, f64) {
        let (a, b) = (self.z(lo), self.z(hi));
        (a.min(b), a.max(b))
    }

    /// The way a face whose outward normal is `out` (+1 up, -1 down) looks in the system.
    pub fn out(&self, out: f64) -> f64 {
        if self.flip { -out } else { out }
    }
}

/// A sine or cosine of a whole number of quarter turns made exact, so that the matrices of the
/// common turns are clean.
fn exact(v: f64) -> f64 {
    let r = (v * 1e12).round() / 1e12;
    if r == 0.0 { 0.0 } else { r }
}

/// A length to a nanometre, with no negative zero.
fn clean(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

/// One member of the system, as the system scene is built from it.
pub struct MemberInput<'a> {
    /// The member's name in the system (`led`).
    pub name: String,
    /// The placed board, for the pad centres of its connectors.
    pub board: &'a Board,
    /// The board's scene, in its own frame.
    pub scene: Scene,
}

/// One mate of the system, as the system scene is built from it.
#[derive(Debug, Clone, PartialEq)]
pub struct MateInput {
    /// The two connectors by path in the system (`power.power_out`), as the statement names them.
    pub a: String,
    pub b: String,
    /// The member each is on, `None` when it is on none.
    pub a_member: Option<String>,
    pub b_member: Option<String>,
    /// Each connector's seat in mm, `(least, most)`, `None` when nothing states it.
    pub a_seat: Option<(f64, f64)>,
    pub b_seat: Option<(f64, f64)>,
    /// The distance between the two boards' faces, `(least, most)`: the seats added.
    pub mated_height: Option<(f64, f64)>,
    /// Why it has none.
    pub unknown: Option<String>,
}

impl MateInput {
    /// The connector's path in its member's board: `power.power_out` less `power.`.
    fn part(path: &str, member: &Option<String>) -> String {
        member
            .as_deref()
            .and_then(|m| path.strip_prefix(m).and_then(|p| p.strip_prefix('.')))
            .unwrap_or(path)
            .to_string()
    }
}

/// How a member is in the system scene.
#[derive(Debug, Clone, PartialEq)]
pub enum Place {
    /// The base: the first member, in whose frame the system is.
    Base,
    /// Placed by the mate at this index of [`SystemScene::mates`].
    Mated { mate: usize },
    /// Drawn apart, beside the others, for this reason.
    Apart { reason: String },
}

/// A member in the system scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub name: String,
    /// Its scene in its own frame: the motion puts it in the system's.
    pub scene: Scene,
    pub motion: Motion,
    pub place: Place,
}

impl Member {
    pub fn placed(&self) -> bool {
        !matches!(self.place, Place::Apart { .. })
    }
}

/// A mate in the system scene.
#[derive(Debug, Clone, PartialEq)]
pub struct Mate {
    pub input: MateInput,
    /// The contacts it pairs, pad by pad: as many as both connectors have pads of one number.
    pub contacts: usize,
    /// The largest distance in plan between a pad and its partner once both members are placed,
    /// in mm; `None` when one of them is not.
    pub residual: Option<f64>,
    /// The distance between the two connectors' faces once both are placed, in mm.
    pub gap: Option<f64>,
}

/// What a finding of the system scene is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Code {
    SystemCollision,
    SystemUnchecked,
    MateMisaligned,
    SystemApart,
}

impl Code {
    pub fn word(self) -> &'static str {
        match self {
            Code::SystemCollision => "system_collision",
            Code::SystemUnchecked => "system_unchecked",
            Code::MateMisaligned => "mate_misaligned",
            Code::SystemApart => "system_apart",
        }
    }

    /// Whether the finding is an error whatever the constraints say: a collision of known
    /// heights and a misaligned mate stop `fab`, and `--allow-drc-violations` never waives them.
    pub fn error(self) -> bool {
        matches!(self, Code::SystemCollision | Code::MateMisaligned)
    }

    /// Whether the finding is an error when constraints are strict (`--constraints error` or
    /// `constraints = "error"`); otherwise those that are not [`Code::error`] are warnings.
    pub fn is_error(self, strict: bool) -> bool {
        self.error() || strict
    }

    /// `error`, or `warning` when the finding is one unless constraints are strict.
    pub fn severity(self) -> &'static str {
        if self.error() { "error" } else { "warning" }
    }
}

/// A finding of the system scene, with the parts (`member/path`), the members and the numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub code: Code,
    pub message: String,
    pub members: Vec<String>,
    /// `member/path`; a board itself is `member/` followed by nothing.
    pub parts: Vec<String>,
    /// The mate, by its two connectors, for `mate_misaligned`.
    pub mate: Option<(String, String)>,
    /// The z interval two prisms share, for a collision.
    pub z: Option<(f64, f64)>,
    /// How far they reach into each other in plan, or how far a pad lands from its partner.
    pub distance: Option<f64>,
}

/// The system scene.
#[derive(Debug, Clone, PartialEq)]
pub struct SystemScene {
    pub name: String,
    /// As the system declares them; the first is the base.
    pub members: Vec<Member>,
    /// As the system states them.
    pub mates: Vec<Mate>,
    pub findings: Vec<Finding>,
}

impl SystemScene {
    /// The system scene of the system `name`, of `members` (the base first) joined by `mates`.
    pub fn of(name: &str, members: Vec<MemberInput>, mates: Vec<MateInput>) -> SystemScene {
        let index = |m: &Option<String>| {
            m.as_deref()
                .and_then(|m| members.iter().position(|x| x.name == m))
        };
        let mut motion: Vec<Option<Motion>> = vec![None; members.len()];
        let mut place: Vec<Option<Place>> = vec![None; members.len()];
        let mut why: Vec<Option<String>> = vec![None; members.len()];
        if !members.is_empty() {
            motion[0] = Some(Motion::NONE);
            place[0] = Some(Place::Base);
        }
        let ends: Vec<Option<(usize, usize)>> = mates
            .iter()
            .map(|m| Some((index(&m.a_member)?, index(&m.b_member)?)))
            .collect();
        // Place what the mates reach, in source order, until no mate places another member.
        loop {
            let mut moved = false;
            for (k, mate) in mates.iter().enumerate() {
                let Some((ia, ib)) = ends[k] else { continue };
                let (from, to, from_part, to_part, from_path, to_path) =
                    match (motion[ia].is_some(), motion[ib].is_some()) {
                        (true, false) => (
                            ia,
                            ib,
                            MateInput::part(&mate.a, &mate.a_member),
                            MateInput::part(&mate.b, &mate.b_member),
                            &mate.a,
                            &mate.b,
                        ),
                        (false, true) => (
                            ib,
                            ia,
                            MateInput::part(&mate.b, &mate.b_member),
                            MateInput::part(&mate.a, &mate.a_member),
                            &mate.b,
                            &mate.a,
                        ),
                        _ => continue,
                    };
                let fixed = motion[from].expect("placed");
                match fit(
                    &members[from],
                    &from_part,
                    from_path,
                    fixed,
                    &members[to],
                    &to_part,
                    to_path,
                    mate,
                ) {
                    Ok(m) => {
                        motion[to] = Some(m);
                        place[to] = Some(Place::Mated { mate: k });
                        moved = true;
                    }
                    Err(reason) => {
                        why[to].get_or_insert(reason);
                    }
                }
            }
            if !moved {
                break;
            }
        }
        // What cannot be placed goes apart, along +x past what is placed.
        let mut reach = members
            .iter()
            .zip(&motion)
            .filter_map(|(m, mo)| mo.and_then(|mo| bounds(&m.scene, mo)))
            .map(|(lo, hi)| (lo.x, hi.x))
            .fold(None::<f64>, |acc, (_, hi)| {
                Some(acc.map_or(hi, |a| a.max(hi)))
            });
        let mut findings = Vec::new();
        let mut out_members = Vec::new();
        let base_name = members.first().map(|m| m.name.clone()).unwrap_or_default();
        for (i, input) in members.into_iter().enumerate() {
            let (motion, place) = match (motion[i], place[i].take()) {
                (Some(m), Some(p)) => (m, p),
                _ => {
                    let reason = why[i].take().unwrap_or_else(|| {
                        format!("no mate joins it to `{base_name}` or to a member placed from it")
                    });
                    let (lo, hi) = bounds(&input.scene, Motion::NONE)
                        .unwrap_or((Plan { x: 0.0, y: 0.0 }, Plan { x: 0.0, y: 0.0 }));
                    let x = clean(reach.map_or(0.0, |r| r + APART_GAP) - lo.x);
                    reach = Some(x + hi.x);
                    findings.push(Finding {
                        code: Code::SystemApart,
                        message: format!(
                            "`{}` is drawn apart, and nothing across boards is checked for it: {reason}",
                            input.name
                        ),
                        members: vec![input.name.clone()],
                        parts: Vec::new(),
                        mate: None,
                        z: None,
                        distance: None,
                    });
                    (
                        Motion {
                            offset: [x, 0.0, 0.0],
                            ..Motion::NONE
                        },
                        Place::Apart { reason },
                    )
                }
            };
            out_members.push((
                Member {
                    name: input.name,
                    scene: input.scene,
                    motion,
                    place,
                },
                input.board,
            ));
        }
        // Every mate whose two ends are placed is measured.
        let mut out_mates = Vec::new();
        let mut misaligned = Vec::new();
        for (k, mate) in mates.into_iter().enumerate() {
            let pairs = ends[k]
                .map(|(ia, ib)| {
                    let (a, b) = (&out_members[ia], &out_members[ib]);
                    pairs(
                        a.1,
                        &MateInput::part(&mate.a, &mate.a_member),
                        b.1,
                        &MateInput::part(&mate.b, &mate.b_member),
                    )
                })
                .unwrap_or_default();
            let mut residual = None;
            let mut gap = None;
            if let Some((ia, ib)) = ends[k]
                && out_members[ia].0.placed()
                && out_members[ib].0.placed()
                && !pairs.is_empty()
            {
                let (a, b) = (&out_members[ia].0, &out_members[ib].0);
                let worst = pairs
                    .iter()
                    .map(|(n, pa, pb)| {
                        let (qa, qb) = (a.motion.plan(*pa), b.motion.plan(*pb));
                        (clean(dist(qa, qb)), n)
                    })
                    .max_by(|x, y| x.0.total_cmp(&y.0))
                    .expect("pairs");
                residual = Some(worst.0);
                let faces = (
                    face(a, &MateInput::part(&mate.a, &mate.a_member)),
                    face(b, &MateInput::part(&mate.b, &mate.b_member)),
                );
                let mut problem = None;
                if worst.0 > ALIGNMENT {
                    problem = Some(format!(
                        "contact {} of `{}` lands {:.3} mm from its partner on `{}`, more than {ALIGNMENT} mm",
                        worst.1, mate.a, worst.0, mate.b
                    ));
                }
                if let (Some((za, na)), Some((zb, nb))) = faces {
                    let g = clean((zb - za) * na);
                    gap = Some(g);
                    if problem.is_none() {
                        if na + nb != 0.0 {
                            problem = Some(format!(
                                "`{}` and `{}` face the same way, so they cannot meet",
                                mate.a, mate.b
                            ));
                        } else if let Some((lo, hi)) = mate.mated_height
                            && (g < lo - ALIGNMENT || g > hi + ALIGNMENT)
                        {
                            problem = Some(format!(
                                "the faces of `{}` and `{}` are {g:.3} mm apart, and the mate's mated height is {lo:.2} to {hi:.2} mm",
                                mate.a, mate.b
                            ));
                        }
                    }
                }
                if let Some(problem) = problem {
                    misaligned.push(Finding {
                        code: Code::MateMisaligned,
                        message: format!(
                            "the mate of `{}` and `{}` is misaligned: {problem}",
                            mate.a, mate.b
                        ),
                        members: vec![a.name.clone(), b.name.clone()],
                        parts: vec![
                            format!("{}/{}", a.name, MateInput::part(&mate.a, &mate.a_member)),
                            format!("{}/{}", b.name, MateInput::part(&mate.b, &mate.b_member)),
                        ],
                        mate: Some((mate.a.clone(), mate.b.clone())),
                        z: None,
                        distance: Some(worst.0),
                    });
                }
            }
            out_mates.push(Mate {
                contacts: pairs.len(),
                residual,
                gap,
                input: mate,
            });
        }
        let members: Vec<Member> = out_members.into_iter().map(|(m, _)| m).collect();
        let mut checked = checks(&members, &out_mates);
        checked.extend(misaligned);
        checked.append(&mut findings);
        checked.sort_by_key(|f| f.code);
        SystemScene {
            name: name.to_string(),
            members,
            mates: out_mates,
            findings: checked,
        }
    }

    pub fn member(&self, name: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.name == name)
    }
}

/// Every mesh of the system scene, in the system's frame: each member's meshes
/// ([`crate::mesh::of_scene`]) moved by its motion, each id starting with `member/` (`led/r1`,
/// `led/GND`), the members in order. What `render --board System` draws.
pub(crate) fn meshes(system: &SystemScene) -> Vec<crate::mesh::Mesh> {
    let mut out = Vec::new();
    for m in &system.members {
        for mut mesh in crate::mesh::of_scene(&m.scene) {
            mesh.transform(m.motion.linear(), m.motion.offset);
            mesh.id = format!("{}/{}", m.name, mesh.id);
            out.push(mesh);
        }
    }
    out
}

/// The pads of the two connectors that meet, by number: `(number, centre of a's, centre of b's)`
/// in each board's own plan, in the order of a's footprint. Pads with no number are not
/// contacts.
fn pairs(a: &Board, a_part: &str, b: &Board, b_part: &str) -> Vec<(String, Plan, Plan)> {
    let first = |pads: Vec<(String, Plan)>| {
        let mut seen: Vec<(String, Plan)> = Vec::new();
        for (n, p) in pads {
            if !n.is_empty() && !seen.iter().any(|(m, _)| *m == n) {
                seen.push((n, p));
            }
        }
        seen
    };
    let (pa, pb) = (first(pad_centres(a, a_part)), first(pad_centres(b, b_part)));
    pa.into_iter()
        .filter_map(|(n, p)| {
            let q = pb.iter().find(|(m, _)| *m == n)?.1;
            Some((n, p, q))
        })
        .collect()
}

/// The face a connector stands on, in the system: its z and its outward normal (+1 up, -1
/// down). `None` when the member's scene has no body for it.
fn face(member: &Member, part: &str) -> Option<(f64, f64)> {
    let body = member.scene.body(part)?;
    let out = if body.side == Side::Back { -1.0 } else { 1.0 };
    Some((member.motion.z(body.z_face), member.motion.out(out)))
}

fn dist(a: Plan, b: Plan) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

/// The motion that places `to`'s connector `to_part` on `from`'s `from_part` (placed by
/// `fixed`), or why there is none.
#[allow(clippy::too_many_arguments)]
fn fit(
    from: &MemberInput,
    from_part: &str,
    from_path: &str,
    fixed: Motion,
    to: &MemberInput,
    to_part: &str,
    to_path: &str,
    mate: &MateInput,
) -> Result<Motion, String> {
    let Some((height, _)) = mate.mated_height else {
        return Err(mate.unknown.clone().unwrap_or_else(|| {
            format!(
                "the mate of `{}` and `{}` has no mated height",
                mate.a, mate.b
            )
        }));
    };
    let (Some(fb), Some(tb)) = (from.scene.body(from_part), to.scene.body(to_part)) else {
        let missing = if from.scene.body(from_part).is_none() {
            from_path
        } else {
            to_path
        };
        return Err(format!("`{missing}` has no body on its board"));
    };
    let pairs = pairs(from.board, from_part, to.board, to_part);
    if pairs.len() < 2 {
        return Err(format!(
            "`{from_path}` and `{to_path}` have {} pad{} of one number, and two are needed to line them up",
            pairs.len(),
            if pairs.len() == 1 { "" } else { "s" }
        ));
    }
    // The faces: the fixed connector's in the system, and which way the other must face.
    let side = |b: &Body| if b.side == Side::Back { -1.0 } else { 1.0 };
    let n_from = fixed.out(side(fb));
    let z_from = fixed.z(fb.z_face);
    // The moving connector must face back at its partner: flipped when it would face the same
    // way.
    let flip = side(tb) == n_from;
    let f = if flip { -1.0 } else { 1.0 };
    let target: Vec<Plan> = pairs.iter().map(|(_, p, _)| fixed.plan(*p)).collect();
    let moving: Vec<Plan> = pairs
        .iter()
        .map(|(_, _, q)| Plan { x: q.x, y: f * q.y })
        .collect();
    let centre = |ps: &[Plan]| {
        let n = ps.len() as f64;
        Plan {
            x: ps.iter().map(|p| p.x).sum::<f64>() / n,
            y: ps.iter().map(|p| p.y).sum::<f64>() / n,
        }
    };
    let (ct, cm) = (centre(&target), centre(&moving));
    let (mut dot, mut cross) = (0.0, 0.0);
    for (m, t) in moving.iter().zip(&target) {
        let (mx, my, tx, ty) = (m.x - cm.x, m.y - cm.y, t.x - ct.x, t.y - ct.y);
        dot += mx * tx + my * ty;
        cross += mx * ty - my * tx;
    }
    if dot == 0.0 && cross == 0.0 {
        return Err(format!(
            "the pads of `{to_path}` are all at one point, so its turn is not known"
        ));
    }
    let mut turn = cross.atan2(dot).to_degrees();
    // Whole and half degrees come out clean.
    turn = (turn * 1e9).round() / 1e9;
    if turn <= -180.0 {
        turn += 360.0;
    }
    let turned = Motion {
        turn,
        flip,
        offset: [0.0, 0.0, 0.0],
    };
    // The centre of the moving pads, before the flip: the motion flips it itself.
    let at = turned.plan(Plan {
        x: cm.x,
        y: f * cm.y,
    });
    let z_target = z_from + n_from * height;
    Ok(Motion {
        turn: if turn == 0.0 { 0.0 } else { turn },
        flip,
        offset: [
            clean(ct.x - at.x),
            clean(ct.y - at.y),
            clean(z_target - f * tb.z_face),
        ],
    })
}

/// The bounds in plan of a member's board and bodies, moved by `motion`.
fn bounds(scene: &Scene, motion: Motion) -> Option<(Plan, Plan)> {
    let b = &scene.board;
    let mut points = vec![
        Plan { x: 0.0, y: 0.0 },
        Plan { x: b.width, y: 0.0 },
        Plan {
            x: b.width,
            y: b.height,
        },
        Plan {
            x: 0.0,
            y: b.height,
        },
    ];
    for body in &scene.bodies {
        points.extend(body.corners);
    }
    let moved: Vec<Plan> = points.into_iter().map(|p| motion.plan(p)).collect();
    let lo = moved.iter().copied().reduce(|a, b| Plan {
        x: a.x.min(b.x),
        y: a.y.min(b.y),
    })?;
    let hi = moved.iter().copied().reduce(|a, b| Plan {
        x: a.x.max(b.x),
        y: a.y.max(b.y),
    })?;
    Some((lo, hi))
}

/// How sure a prism's far end is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sure {
    /// A board, a stated or catalog height, pins of a stated length.
    Known,
    /// A package standard's maximum (ADR 0106).
    Standard,
}

/// A solid of a member in the system: a convex polygon in plan, grown by `radius` (a board's
/// rounded corners), between two heights.
struct Prism {
    member: usize,
    /// The part's path, `None` for the board.
    part: Option<String>,
    refdes: Option<String>,
    what: &'static str,
    sure: Sure,
    core: Vec<Plan>,
    radius: f64,
    z: (f64, f64),
    /// The board's cutouts, each a core and a radius: a body wholly inside one does not meet
    /// the board.
    cutouts: Vec<(Vec<Plan>, f64)>,
}

fn rect(min: Plan, max: Plan) -> Vec<Plan> {
    vec![
        min,
        Plan { x: max.x, y: min.y },
        max,
        Plan { x: min.x, y: max.y },
    ]
}

/// A rectangle with rounded corners as its core, shrunk by the radius, and the radius.
fn rounded(min: Plan, max: Plan, radius: f64) -> (Vec<Plan>, f64) {
    let r = radius
        .max(0.0)
        .min((max.x - min.x) / 2.0)
        .min((max.y - min.y) / 2.0);
    (
        rect(
            Plan {
                x: min.x + r,
                y: min.y + r,
            },
            Plan {
                x: max.x - r,
                y: max.y - r,
            },
        ),
        r,
    )
}

fn prisms(index: usize, member: &Member) -> Vec<Prism> {
    let m = member.motion;
    let s = &member.scene;
    let moved = |ps: Vec<Plan>| ps.into_iter().map(|p| m.plan(p)).collect::<Vec<_>>();
    let (core, radius) = rounded(
        Plan { x: 0.0, y: 0.0 },
        Plan {
            x: s.board.width,
            y: s.board.height,
        },
        s.board.corner_radius,
    );
    let mut out = vec![Prism {
        member: index,
        part: None,
        refdes: None,
        what: "board",
        sure: Sure::Known,
        core: moved(core),
        radius,
        z: m.z_range(-s.board.thickness, 0.0),
        cutouts: s
            .board
            .cutouts
            .iter()
            .map(|c| {
                let (core, r) = rounded(c.at.min, c.at.max, c.radius);
                (moved(core), r)
            })
            .collect(),
    }];
    for body in &s.bodies {
        if let (Some(_), (Some(lo), Some(hi))) = (body.height, body.z_range())
            && hi - lo > COLLISION
        {
            out.push(Prism {
                member: index,
                part: Some(body.path.clone()),
                refdes: Some(body.refdes.clone()),
                what: "body",
                sure: if body.provenance == HeightProvenance::Standard {
                    Sure::Standard
                } else {
                    Sure::Known
                },
                core: moved(body.corners.to_vec()),
                radius: 0.0,
                z: m.z_range(lo, hi),
                cutouts: Vec::new(),
            });
        }
        if let Pins::Known {
            at,
            z_top,
            z_bottom,
            ..
        } = &body.pins
        {
            out.push(Prism {
                member: index,
                part: Some(body.path.clone()),
                refdes: Some(body.refdes.clone()),
                what: "pins",
                sure: Sure::Known,
                core: moved(rect(at.min, at.max)),
                radius: 0.0,
                z: m.z_range(*z_bottom, *z_top),
                cutouts: Vec::new(),
            });
        }
    }
    out
}

/// The checks across boards: collisions, and parts of unknown height that face another member.
fn checks(members: &[Member], mates: &[Mate]) -> Vec<Finding> {
    // The connector pairs that are exempt against each other, as `(member, path)` pairs.
    let exempt: Vec<((String, String), (String, String))> = mates
        .iter()
        .filter_map(|m| {
            let i = &m.input;
            let (a, b) = (i.a_member.clone()?, i.b_member.clone()?);
            Some((
                (a, MateInput::part(&i.a, &i.a_member)),
                (b, MateInput::part(&i.b, &i.b_member)),
            ))
        })
        .collect();
    let is_exempt = |a: &Prism, b: &Prism| {
        let (Some(pa), Some(pb)) = (&a.part, &b.part) else {
            return false;
        };
        let ka = (members[a.member].name.clone(), pa.clone());
        let kb = (members[b.member].name.clone(), pb.clone());
        exempt
            .iter()
            .any(|(x, y)| (*x == ka && *y == kb) || (*x == kb && *y == ka))
    };
    let placed: Vec<(usize, Vec<Prism>)> = members
        .iter()
        .enumerate()
        .filter(|(_, m)| m.placed())
        .map(|(i, m)| (i, prisms(i, m)))
        .collect();
    let mut out = Vec::new();
    let name_of = |p: &Prism| match (&p.refdes, &p.part) {
        (Some(r), Some(path)) => format!(
            "{} {r} (`{}/{path}`)",
            if p.what == "pins" {
                "the pins of"
            } else {
                "the body of"
            },
            members[p.member].name
        ),
        _ => format!("the board `{}`", members[p.member].name),
    };
    for (k, (_, a_prisms)) in placed.iter().enumerate() {
        for (_, b_prisms) in &placed[k + 1..] {
            for a in a_prisms {
                for b in b_prisms {
                    if is_exempt(a, b) {
                        continue;
                    }
                    let lo = a.z.0.max(b.z.0);
                    let hi = a.z.1.min(b.z.1);
                    if hi - lo <= COLLISION {
                        continue;
                    }
                    if inside_a_cutout(a, b) || inside_a_cutout(b, a) {
                        continue;
                    }
                    let depth = penetration(&a.core, a.radius, &b.core, b.radius);
                    if depth <= COLLISION {
                        continue;
                    }
                    let known = a.sure == Sure::Known && b.sure == Sure::Known;
                    let (code, tail) = if known {
                        (Code::SystemCollision, String::new())
                    } else {
                        (
                            Code::SystemUnchecked,
                            "; a height in it is its package standard's maximum, and a part in that package may be lower".to_string(),
                        )
                    };
                    let part = |p: &Prism| {
                        format!(
                            "{}/{}",
                            members[p.member].name,
                            p.part.clone().unwrap_or_default()
                        )
                    };
                    out.push(Finding {
                        code,
                        message: format!(
                            "{} and {} fill the same space: {:.3} mm into each other in plan, z {:.3} to {:.3} mm{tail}",
                            name_of(a),
                            name_of(b),
                            clean(depth),
                            clean(lo),
                            clean(hi)
                        ),
                        members: vec![
                            members[a.member].name.clone(),
                            members[b.member].name.clone(),
                        ],
                        parts: vec![part(a), part(b)],
                        mate: None,
                        z: Some((clean(lo), clean(hi))),
                        distance: Some(clean(depth)),
                    });
                }
            }
        }
    }
    // A part of unknown height whose face looks at another member's board over it.
    for (i, _) in &placed {
        let m = &members[*i];
        for body in m.scene.bodies.iter().filter(|b| b.height.is_none()) {
            let out_n = m
                .motion
                .out(if body.side == Side::Back { -1.0 } else { 1.0 });
            let z_face = m.motion.z(body.z_face);
            let core: Vec<Plan> = body.corners.iter().map(|c| m.motion.plan(*c)).collect();
            let facing: Vec<String> = placed
                .iter()
                .filter(|(j, _)| j != i)
                .filter(|(_, ps)| {
                    let board = &ps[0];
                    let beyond = if out_n > 0.0 {
                        board.z.0 >= z_face - COLLISION
                    } else {
                        board.z.1 <= z_face + COLLISION
                    };
                    beyond && penetration(&core, 0.0, &board.core, board.radius) > COLLISION
                })
                .map(|(j, _)| members[*j].name.clone())
                .collect();
            if facing.is_empty() {
                continue;
            }
            let mut names = vec![m.name.clone()];
            names.extend(facing.iter().cloned());
            out.push(Finding {
                code: Code::SystemUnchecked,
                message: format!(
                    "{} (`{}/{}`) has no known height, and its face looks at `{}`, so it is not checked against it",
                    body.refdes,
                    m.name,
                    body.path,
                    facing.join("`, `")
                ),
                members: names,
                parts: vec![format!("{}/{}", m.name, body.path)],
                mate: None,
                z: None,
                distance: None,
            });
        }
    }
    out
}

/// Whether the body prism `body` stands wholly inside a cutout of the board prism `board`.
fn inside_a_cutout(body: &Prism, board: &Prism) -> bool {
    board.part.is_none()
        && body.part.is_some()
        && board.cutouts.iter().any(|(core, r)| {
            body.core
                .iter()
                .all(|p| distance_to_polygon(*p, core) <= *r + COLLISION)
        })
}

/// How far two convex polygons, each grown by its radius, reach into each other: the least
/// distance one must move to come apart; zero or less when they are apart.
pub(crate) fn penetration(a: &[Plan], ra: f64, b: &[Plan], rb: f64) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    match sat_depth(a, b) {
        Some(depth) => depth + ra + rb,
        None => ra + rb - polygon_distance(a, b),
    }
}

/// The least overlap of the projections of two convex polygons over the normals of their edges:
/// how deep they reach into each other, or `None` when an axis separates them.
fn sat_depth(a: &[Plan], b: &[Plan]) -> Option<f64> {
    let mut least = f64::MAX;
    for poly in [a, b] {
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let (nx, ny) = (q.y - p.y, p.x - q.x);
            let len = (nx * nx + ny * ny).sqrt();
            if len == 0.0 {
                continue;
            }
            let project = |s: &[Plan]| {
                s.iter()
                    .map(|c| (c.x * nx + c.y * ny) / len)
                    .fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)))
            };
            let ((alo, ahi), (blo, bhi)) = (project(a), project(b));
            let overlap = ahi.min(bhi) - alo.max(blo);
            if overlap < 0.0 {
                return None;
            }
            least = least.min(overlap);
        }
    }
    // Two points, or a polygon with no edge of length: the distance decides.
    if least == f64::MAX {
        return if polygon_distance(a, b) == 0.0 {
            Some(0.0)
        } else {
            None
        };
    }
    Some(least)
}

/// The distance between two convex polygons that do not overlap.
fn polygon_distance(a: &[Plan], b: &[Plan]) -> f64 {
    let mut least = f64::MAX;
    for (p, q) in [(a, b), (b, a)] {
        for v in p {
            least = least.min(distance_to_boundary(*v, q));
        }
    }
    least
}

/// The distance from a point to a convex polygon: zero inside it.
fn distance_to_polygon(p: Plan, poly: &[Plan]) -> f64 {
    if poly.len() >= 3 && inside(p, poly) {
        return 0.0;
    }
    distance_to_boundary(p, poly)
}

fn inside(p: Plan, poly: &[Plan]) -> bool {
    let mut sign = 0.0;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        let c = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        if c != 0.0 {
            if sign == 0.0 {
                sign = c.signum();
            } else if c.signum() != sign {
                return false;
            }
        }
    }
    true
}

fn distance_to_boundary(p: Plan, poly: &[Plan]) -> f64 {
    let mut least = f64::MAX;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        least = least.min(segment_distance(p, a, b));
    }
    least
}

fn segment_distance(p: Plan, a: Plan, b: Plan) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    dist(
        p,
        Plan {
            x: a.x + t * dx,
            y: a.y + t * dy,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, size: f64) -> Vec<Plan> {
        rect(
            Plan { x, y },
            Plan {
                x: x + size,
                y: y + size,
            },
        )
    }

    #[test]
    fn two_squares_reach_into_each_other_by_their_overlap() {
        let d = penetration(&square(0.0, 0.0, 2.0), 0.0, &square(1.5, 0.5, 2.0), 0.0);
        assert!((d - 0.5).abs() < 1e-12, "{d}");
        // Touching is not reaching in.
        assert_eq!(
            penetration(&square(0.0, 0.0, 1.0), 0.0, &square(1.0, 0.0, 1.0), 0.0),
            0.0
        );
    }

    #[test]
    fn a_rounded_corner_is_missed_by_a_square_in_the_corner() {
        // A 10 mm board with 3 mm corners, and a 1 mm square just off its corner, inside the
        // box of the outline but outside the arc.
        let (core, r) = rounded(Plan { x: 0.0, y: 0.0 }, Plan { x: 10.0, y: 10.0 }, 3.0);
        let corner = square(-0.2, -0.2, 1.0);
        assert!(penetration(&core, r, &corner, 0.0) < 0.0);
        // Along an edge it reaches in.
        let edge = square(5.0, -0.5, 1.0);
        assert!((penetration(&core, r, &edge, 0.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_half_turn_with_a_flip_is_a_clean_matrix() {
        let m = Motion {
            turn: 180.0,
            flip: true,
            offset: [50.0, 0.0, 5.0],
        };
        assert_eq!(
            m.matrix(),
            [
                [-1.0, 0.0, 0.0, 50.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, -1.0, 5.0],
                [0.0, 0.0, 0.0, 1.0],
            ]
        );
        assert_eq!(m.point([10.0, 15.0, 0.0]), [40.0, 15.0, 5.0]);
        assert_eq!(m.z_range(-1.6, 0.0), (5.0, 6.6));
    }
}
