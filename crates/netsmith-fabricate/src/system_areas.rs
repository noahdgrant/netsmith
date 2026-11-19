//! The area another member of a system covers over a board, as a keepout-by-height of the
//! board (issue 1331, decided by the owner: derived from the system's mates, nothing stated by
//! hand).
//!
//! A mate holds the two boards rigidly to each other by its two connectors, so the other board
//! is fixed in the frame of this board's connector: its outline lies over a known area of this
//! board, the mated height above this board's face. This module turns each mate of a system into
//! one keepout on each of its two members, carried by the member's own connector of the mate:
//!
//! - its area is the box round the other member's outline, in the connector's frame (the
//!   footprint's, as on the top face), found as the system scene finds the other member's place
//!   (`netsmith_3d::SystemScene`): contact N on contact N, the turn and the offset the
//!   least-squares fit of the pads' contacts (their centres unless the part's `pads` block states
//!   `.contact_offset`, issue #1321), the other board turned over when its connector is on
//!   its top face;
//! - its height limit is the mate's least mated height, the distance between the two boards'
//!   faces: a part of this board taller than that stands in the other board;
//! - it is on the face the connector is on, wherever the placer puts the connector
//!   ([`netsmith_core::Keepout::on_carrier_face`]), and it forbids nothing else;
//! - it is named `System/member over connector`: `Kit/card over slot`.
//!
//! The placer keeps the parts too tall for it out from under the other board, and `layout
//! check` and `fab`'s placement check say a part that stands in it, as for any keepout with a
//! `.max_height`. The connector that carries it is not measured against it.
//!
//! The other member's connector must have a place before either board is placed: a locked entry
//! of its layout file or requirements of its physical file that pin it, and the other member's
//! outline a size (the largest its range allows). When it has none the area is not known, no
//! keepout is made, and a note says what would give it one. The system scene checks the boards
//! after placing in any case (`system_collision`), and it alone measures what stands on the
//! other board's facing face and the pins through it: the keepout is the other board's face,
//! not what hangs below it, and its box may be larger than an outline that is not a rectangle.

use std::path::Path;

use netsmith_core::Keepout;
use netsmith_pcb::{Board, Pt, Side, Unplaced};
use netsmith_pipeline::{Built, Libraries, SolvedSystem};

/// The area the other board of a mate covers over `host`'s connector, in that connector's
/// footprint frame, or why it is not known.
///
/// `host` is the connector's pads' contacts in its own frame; `partner` the other connector's
/// contacts on its board where its place puts it, `partner_side` the face of its board it is on and
/// `outline` the corners of its board's box, in the same frame as `partner`. `pairs` gives
/// the pad numbers that meet, `(host's, partner's)`.
pub fn covered_area(
    host: &[(String, Pt)],
    partner: &[(String, Pt)],
    partner_side: Side,
    outline: [Pt; 4],
    pairs: &[(String, String)],
) -> Result<(Pt, Pt), String> {
    // The other board faces back at this one: turned over when its connector is on its top
    // face, as the system scene turns it.
    let flip = partner_side == Side::Front;
    let mirror = |p: Pt| if flip { Pt::new(-p.x, p.y) } else { p };
    let first = |pads: &[(String, Pt)], n: &str| pads.iter().find(|(m, _)| m == n).map(|p| p.1);
    let matched: Vec<(Pt, Pt)> = pairs
        .iter()
        .filter_map(|(h, p)| Some((first(host, h)?, mirror(first(partner, p)?))))
        .collect();
    if matched.len() < 2 {
        return Err(format!(
            "the connectors have {} pad{} that meet, and two are needed to line them up",
            matched.len(),
            if matched.len() == 1 { "" } else { "s" }
        ));
    }
    let n = matched.len() as f64;
    let centre = |pick: fn(&(Pt, Pt)) -> Pt| {
        let sum = matched
            .iter()
            .map(pick)
            .fold(Pt::new(0.0, 0.0), |a, b| Pt::new(a.x + b.x, a.y + b.y));
        Pt::new(sum.x / n, sum.y / n)
    };
    let (ct, cm) = (centre(|m| m.0), centre(|m| m.1));
    let (mut dot, mut cross) = (0.0, 0.0);
    for (t, m) in &matched {
        let (mx, my, tx, ty) = (m.x - cm.x, m.y - cm.y, t.x - ct.x, t.y - ct.y);
        dot += mx * tx + my * ty;
        cross += mx * ty - my * tx;
    }
    if dot == 0.0 && cross == 0.0 {
        return Err("the other connector's pads are all at one point".to_string());
    }
    let angle = cross.atan2(dot);
    let (s, c) = angle.sin_cos();
    let moved = |p: Pt| {
        let p = mirror(p);
        let (x, y) = (p.x - cm.x, p.y - cm.y);
        Pt::new(ct.x + c * x - s * y, ct.y + s * x + c * y)
    };
    let corners = outline.map(moved);
    let lo = corners
        .iter()
        .fold(Pt::new(f64::INFINITY, f64::INFINITY), |a, p| {
            Pt::new(a.x.min(p.x), a.y.min(p.y))
        });
    let hi = corners
        .iter()
        .fold(Pt::new(f64::NEG_INFINITY, f64::NEG_INFINITY), |a, p| {
            Pt::new(a.x.max(p.x), a.y.max(p.y))
        });
    Ok((clean(lo), clean(hi)))
}

/// A point to a nanometre, with no negative zero.
fn clean(p: Pt) -> Pt {
    let r = |v: f64| {
        let r = (v * 1e6).round() / 1e6;
        if r == 0.0 { 0.0 } else { r }
    };
    Pt::new(r(p.x), r(p.y))
}

/// One member of a system as this module reads it: its index in the boards, its name in the
/// system, and the board before placing.
struct Member {
    index: usize,
    name: String,
    unplaced: Unplaced,
}

/// Add to each board of a system the areas the other members cover over its mated connectors
/// (the module's header), reading each member's layout file as `fab` does (`layout`, the one
/// beside the source; `None` when there is none). Gives the notes of the areas that are not
/// known, each with the label of the board it is about. A board that does not load is left as
/// it is: what is wrong with it is said where it is made.
pub fn add(
    boards: &mut [Built],
    systems: &[SolvedSystem],
    loader: &netsmith_check::Loader,
    layout: Option<&Path>,
    libraries: &Libraries,
) -> Vec<(String, String)> {
    let names: Vec<String> = boards.iter().map(|b| b.design.name.clone()).collect();
    let mut notes = Vec::new();
    let mut found: Vec<(usize, Keepout)> = Vec::new();
    for system in systems {
        if system.mates.iter().all(|m| m.mated_height().is_none()) {
            continue;
        }
        let members: Vec<Member> = system
            .boards
            .iter()
            .filter_map(|(name, _, _)| {
                let index = boards.iter().position(|b| {
                    b.system.as_deref() == Some(system.name.as_str()) && &b.board == name
                })?;
                let built = &boards[index];
                let file = layout
                    .and_then(|l| crate::layout::read_board(built, loader, l, &names).ok())
                    .and_then(|(read, _)| read);
                let options =
                    netsmith_pcb::place_options(&built.design, file.as_ref().map(|f| &f.0));
                let unplaced = Board::unplaced(
                    &built.design,
                    &libraries.mapping,
                    &libraries.footprints,
                    &options,
                )
                .ok()?;
                Some(Member {
                    index,
                    name: name.clone(),
                    unplaced,
                })
            })
            .collect();
        let member = |name: &Option<String>| {
            let name = name.as_deref()?;
            members.iter().find(|m| m.name == name)
        };
        for mate in &system.mates {
            let Some((height, _)) = mate.mated_height() else {
                continue;
            };
            let (Some(a), Some(b)) = (member(&mate.a_board), member(&mate.b_board)) else {
                continue;
            };
            if a.index == b.index {
                continue;
            }
            let window = mate.window;
            let ends = [
                (a, b, &mate.a, &mate.b, window),
                (b, a, &mate.b, &mate.a, window.map(|(x, y, n)| (y, x, n))),
            ];
            for (host, partner, host_path, partner_path, window) in ends {
                let host_part = part_path(host_path, &host.name);
                let partner_part = part_path(partner_path, &partner.name);
                let label = boards[host.index].label();
                match area_of(
                    host,
                    &host_part,
                    partner,
                    &partner_part,
                    &boards[partner.index],
                    window,
                ) {
                    Ok(Some((lo, hi))) => found.push((
                        host.index,
                        Keepout {
                            name: format!("{}/{} over {host_part}", system.name, partner.name),
                            part: Some(host_part),
                            at: Some((lo.x, lo.y)),
                            size: (clean_len(hi.x - lo.x), clean_len(hi.y - lo.y)),
                            layers: vec!["top".to_string()],
                            tracks: false,
                            vias: false,
                            copper: false,
                            parts: false,
                            max_height: Some(height),
                            on_carrier_face: true,
                        },
                    )),
                    Ok(None) => {}
                    Err(why) => notes.push((
                        label,
                        format!(
                            "the area `{}` covers over `{host_part}` is not known before placing, so the placer of `{}` does not keep the parts taller than the mated height ({height} mm) out from under it: {why}; the system scene `{}` checks the boards after placing",
                            partner.name, host.name, system.name
                        ),
                    )),
                }
            }
        }
    }
    for (index, keepout) in found {
        let keepouts = &mut boards[index].design.features.keepouts;
        if !keepouts.iter().any(|k| k.name == keepout.name) {
            keepouts.push(keepout);
        }
    }
    notes
}

/// A length to a nanometre.
fn clean_len(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// A connector's path in its member's board: `card.j1` less `card.`.
fn part_path(path: &str, member: &str) -> String {
    path.strip_prefix(member)
        .and_then(|p| p.strip_prefix('.'))
        .unwrap_or(path)
        .to_string()
}

/// The area `partner` covers over `host`'s connector `host_part`, in its frame: `None` when a
/// connector is not on its board (a mate the scene reports), an error when the place of the
/// partner's connector or the size of its outline is not known before placing.
fn area_of(
    host: &Member,
    host_part: &str,
    partner: &Member,
    partner_part: &str,
    partner_built: &Built,
    window: Option<(usize, usize, usize)>,
) -> Result<Option<(Pt, Pt)>, String> {
    let (Some(h), Some(p)) = (
        host.unplaced.parts.iter().find(|x| x.path == host_part),
        partner
            .unplaced
            .parts
            .iter()
            .find(|x| x.path == partner_part),
    ) else {
        return Ok(None);
    };
    let Some(fixed) = partner.unplaced.placement.fixed.get(&p.refdes) else {
        return Err(format!(
            "`{}.{partner_part}` has no place until `{}` is placed; lock it in the layout file of `{}` (`netsmith layout set {partner_part} --at X,Y --lock --board <system>/{}`) or pin it with requirements of its physical file",
            partner.name, partner.name, partner.name, partner.name
        ));
    };
    let (outline, bounded, _) = netsmith_pcb::outline_box(&partner_built.design);
    if !bounded {
        return Err(format!(
            "the outline of `{}` has no size before it is placed; state its width and height",
            partner.name
        ));
    }
    let mut at = p.clone();
    at.at = netsmith_pcb::ORIGIN + fixed.at;
    at.rotation = fixed.rotation;
    at.side = fixed.side;
    let local = |placed: &netsmith_pcb::Placed| -> Vec<(String, Pt)> {
        placed
            .footprint
            .pads
            .iter()
            .map(|pad| (pad.number.clone(), placed.contact(pad)))
            .collect()
    };
    let host_pads = local(h);
    let partner_pads: Vec<(String, Pt)> = local(&at)
        .into_iter()
        .map(|(n, q)| (n, at.to_board(q)))
        .collect();
    let pairs: Vec<(String, String)> = host_pads
        .iter()
        .filter(|(n, _)| !n.is_empty())
        .filter_map(|(n, _)| match window {
            None => Some((n.clone(), n.clone())),
            Some((fh, fp, count)) => {
                let i = n.parse::<usize>().ok()?.checked_sub(fh)?;
                (i < count).then(|| (n.clone(), (fp + i).to_string()))
            }
        })
        .collect();
    let corners = [
        outline.min,
        Pt::new(outline.max.x, outline.min.y),
        outline.max,
        Pt::new(outline.min.x, outline.max.y),
    ];
    covered_area(&host_pads, &partner_pads, fixed.side, corners, &pairs).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pads(points: &[(f64, f64)]) -> Vec<(String, Pt)> {
        points
            .iter()
            .enumerate()
            .map(|(i, (x, y))| ((i + 1).to_string(), Pt::new(*x, *y)))
            .collect()
    }

    fn one_for_one(n: usize) -> Vec<(String, String)> {
        (1..=n).map(|i| (i.to_string(), i.to_string())).collect()
    }

    fn corners(w: f64, h: f64) -> [Pt; 4] {
        [
            Pt::new(0.0, 0.0),
            Pt::new(w, 0.0),
            Pt::new(w, h),
            Pt::new(0.0, h),
        ]
    }

    #[test]
    fn a_board_whose_connector_is_on_its_back_lies_over_the_host_as_it_is_drawn() {
        // Pads 1 and 2 at the host connector's origin, 2.54 mm apart down y; the other
        // connector at (5, 3) of a 20 x 10 board, on its back, so the board is not turned over.
        let host = pads(&[(0.0, 0.0), (0.0, 2.54)]);
        let partner = pads(&[(5.0, 3.0), (5.0, 5.54)]);
        let (lo, hi) = covered_area(
            &host,
            &partner,
            Side::Back,
            corners(20.0, 10.0),
            &one_for_one(2),
        )
        .unwrap();
        assert_eq!((lo.x, lo.y, hi.x, hi.y), (-5.0, -3.0, 15.0, 7.0));
    }

    #[test]
    fn a_board_whose_connector_is_on_its_top_is_turned_over_onto_the_host() {
        // Turned over, the board's x runs the other way: what was 15 mm right of the connector
        // is 15 mm left of it.
        let host = pads(&[(0.0, 0.0), (0.0, 2.54)]);
        let partner = pads(&[(5.0, 3.0), (5.0, 5.54)]);
        let (lo, hi) = covered_area(
            &host,
            &partner,
            Side::Front,
            corners(20.0, 10.0),
            &one_for_one(2),
        )
        .unwrap();
        assert_eq!((lo.x, lo.y, hi.x, hi.y), (-15.0, -3.0, 5.0, 7.0));
    }

    #[test]
    fn a_connector_turned_on_its_board_turns_the_board_over_the_host() {
        // The other connector runs along x on its board (turned a quarter): the board lies
        // turned a quarter over the host's connector, which runs along y.
        let host = pads(&[(0.0, 0.0), (0.0, 2.54)]);
        let partner = pads(&[(5.0, 3.0), (7.54, 3.0)]);
        let (lo, hi) = covered_area(
            &host,
            &partner,
            Side::Back,
            corners(20.0, 10.0),
            &one_for_one(2),
        )
        .unwrap();
        // The board is 10 wide and 20 tall over the host once turned.
        assert!(((hi.x - lo.x) - 10.0).abs() < 1e-9, "{lo:?} {hi:?}");
        assert!(((hi.y - lo.y) - 20.0).abs() < 1e-9, "{lo:?} {hi:?}");
    }

    #[test]
    fn one_pad_that_meets_is_not_enough_to_line_the_boards_up() {
        let host = pads(&[(0.0, 0.0), (0.0, 2.54)]);
        let partner = pads(&[(5.0, 3.0)]);
        let why = covered_area(
            &host,
            &partner,
            Side::Back,
            corners(20.0, 10.0),
            &one_for_one(2),
        )
        .unwrap_err();
        assert!(why.contains("1 pad that meet"), "{why}");
    }
}
