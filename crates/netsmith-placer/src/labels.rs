//! Where each designator the placer places goes (issue #1729): from its own neighbourhood only.
//!
//! The annealer moves a part's text from side to side as it moves the parts, so the side it
//! ends on depends on the whole run: removing a part anywhere on the board could move another
//! part's text, and onto a third part's pad. Once the parts are placed, [`settle_labels`]
//! places every text the placer owns again, from what lies round it alone:
//!
//! - Each of the four sides of [`ReferenceSide::ALL`] is measured, heaviest first: the pads
//!   (and drawn mask openings) of every part on the text's face, its own included, at the
//!   process's silk to pad space; the board edge and the cutouts, at the silk space; the
//!   silkscreen of the other parts and the texts already placed, at the silk space; then the
//!   annealer's wider margins round all of these; then the courtyards of the other parts, under
//!   whose bodies a text cannot be read. The text takes the clearest side, the first measure
//!   where two sides differ deciding, and of sides as clear the first of above (where libraries
//!   put it), below, left and right.
//! - The texts the placer does not own stand first (a part the layout file places keeps the
//!   side its entry gives, and a `designator` entry its place). Then the text with the fewest
//!   clear sides left goes next, again and again, so a text with one clear side has it before a
//!   neighbour with more takes it, and among as many the order of the parts' paths. A text is
//!   measured against the texts placed before it, and of its sides as clear as the clearest it
//!   takes the one in the way of the fewest clear sides of the texts still to place.
//!
//! What a text is measured against is only what lies under the four places it may take, and the
//! texts of its neighbours; where a part ends in the anneal does not enter. So
//! while a part and what lies round it do not change, its text keeps its place, whatever else
//! moves. A text that cannot keep off a pad on any side keeps the side that is nearest to
//! doing so, and is recorded as on a pad.
//!
//! The boxes and margins are the annealer's (`anneal::silk`, [`penetration`], [`text_box`]),
//! so a text the anneal made room for is found clear here too. Each text put off its own side,
//! or left on a pad, is recorded in [`Board::labels`] ([`LabelPlace`]) with what is in the way
//! of each side tried before it.

use netsmith_pcb::{Board, LabelPlace, Placed, Pt, Rect, ReferenceSide, Side};

use crate::anneal::silk::{self, Piece};
use crate::anneal::{SILK_CLEAR, TEXT_EDGE, penetration, text_box};

/// How far beyond a text's box a part is looked at, in mm: more than any gap measured.
const REACH: f64 = 1.0;

/// What a part puts in the way of a text, on the board.
struct Around {
    refdes: String,
    /// Each pad's box with its number and whether it has copper on the front and the back.
    pads: Vec<(Rect, String, bool, bool)>,
    /// The openings the footprint draws in the mask, on its own face.
    openings: Vec<Rect>,
    /// Its silkscreen, on its own face (absolute: the part is where it is).
    silk: Vec<Piece>,
    courts: Vec<Rect>,
    side: Side,
    /// Whether it takes room on both faces (a hole).
    through: bool,
    /// The box round all of it.
    reach: Rect,
}

impl Around {
    fn of(part: &Placed, rules: &netsmith_pcb::Rules) -> Around {
        let pads: Vec<(Rect, String, bool, bool)> = part
            .footprint
            .pads
            .iter()
            .filter_map(|p| {
                let (front, back) = part.pad_layers(p);
                (front || back).then(|| (part.pad_bounds(p), p.number.clone(), front, back))
            })
            .collect();
        let openings = part.mask_openings();
        let silk = silk::pieces(part, rules);
        let courts = part.courtyards();
        let reach = pads
            .iter()
            .map(|p| p.0)
            .chain(openings.iter().copied())
            .chain(silk.iter().map(Piece::bounds))
            .chain(courts.iter().copied())
            .fold(part.courtyard(), |a, r| a.union(&r));
        Around {
            refdes: part.refdes.clone(),
            pads,
            openings,
            silk,
            courts,
            side: part.side,
            through: part.has_hole(),
            reach,
        }
    }

    fn on(&self, face: Side) -> bool {
        self.side == face || self.through
    }
}

/// One measure of a place: how much is missing, and what is most in the way.
#[derive(Clone, Default)]
struct Worst {
    by: f64,
    what: Option<String>,
}

impl Worst {
    fn add(&mut self, by: f64, what: impl FnOnce() -> String) {
        if by > 1e-6 && by > self.by + 1e-9 {
            self.by = by;
            self.what = Some(what());
        }
    }
}

/// The measures of a place, heaviest first: what the design rules check would find (a pad, the
/// edge, other silkscreen, at the process's spaces), then the annealer's wider margins round
/// them, then other parts' courtyards (the text would be under a body).
const PAD: usize = 0;
const EDGE: usize = 1;
const SILK: usize = 2;
const MARGIN: usize = 3;
const COURT: usize = 4;

#[derive(Clone, Default)]
struct Measure([Worst; 5]);

impl Measure {
    /// Whether this place is clearer than `other`: the first measure where they differ decides.
    fn better_than(&self, other: &Measure) -> bool {
        for (a, b) in self.0.iter().zip(&other.0) {
            if (a.by - b.by).abs() > 1e-6 {
                return a.by < b.by;
            }
        }
        false
    }

    /// Whether nothing is in the way but, at most, a courtyard.
    fn clear(&self) -> bool {
        self.0[..COURT].iter().all(|w| w.what.is_none())
    }

    /// What is in the way, by the heaviest measure that finds something.
    fn reason(&self) -> Option<String> {
        self.0.iter().find_map(|w| w.what.clone())
    }
}

/// A text the placer places: its part, its face, and its four places with what lies under each.
struct Text {
    part: usize,
    face: Side,
    places: Vec<(ReferenceSide, Rect, Measure)>,
}

impl Text {
    /// How many of its places are clear.
    fn clear(&self) -> usize {
        self.places.iter().filter(|(_, _, m)| m.clear()).count()
    }
}

/// Place again the text of every part `own` says the placer owns, from its own neighbourhood
/// (the header), and record in [`Board::labels`] each one put off its own side or left on a
/// pad. A part that prints no text, artwork, and a part with a `designator` entry are left as
/// they are. The same parts where they are always give the same texts, whatever side each text
/// was on before.
pub fn settle_labels(board: &mut Board, own: impl Fn(&Placed) -> bool) {
    let rules = board.rules.clone();
    let edge = board.edge();
    let cutouts: Vec<Rect> = board
        .cutout_shapes()
        .into_iter()
        .map(|(_, s)| s.rect)
        .collect();
    let around: Vec<Around> = board.parts.iter().map(|p| Around::of(p, &rules)).collect();
    let margin_pad = rules.silk_to_pad.max(SILK_CLEAR);
    // What lies under each place of a text, the other texts left out: the same whatever order
    // the texts are placed in.
    let still = |i: usize, t: Rect| {
        let mut m = Measure::default();
        let face = board.parts[i].side;
        let near = t.expanded(REACH);
        for (j, a) in around.iter().enumerate() {
            if !a.on(face) || !a.reach.overlaps(&near) {
                continue;
            }
            let pad = |number: &str| format!("{} pad {number}", a.refdes);
            for (r, number, front, back) in &a.pads {
                if (face == Side::Front && *front) || (face == Side::Back && *back) {
                    m.0[PAD].add(penetration(&t, r, rules.silk_to_pad), || pad(number));
                    m.0[MARGIN].add(penetration(&t, r, margin_pad), || pad(number));
                }
            }
            if a.side == face {
                let opening = || format!("a mask opening of {}", a.refdes);
                for r in &a.openings {
                    m.0[PAD].add(penetration(&t, r, rules.silk_to_pad), opening);
                    m.0[MARGIN].add(penetration(&t, r, margin_pad), opening);
                }
            }
            if j == i {
                continue;
            }
            if a.side == face {
                let silk = || format!("{}'s silkscreen", a.refdes);
                for piece in &a.silk {
                    m.0[SILK].add(piece.depth(Pt::default(), &t, rules.silk_to_silk), silk);
                    m.0[MARGIN].add(piece.depth(Pt::default(), &t, SILK_CLEAR), silk);
                }
            }
            for c in &a.courts {
                m.0[COURT].add(penetration(&t, c, 0.0), || {
                    format!("{}'s courtyard", a.refdes)
                });
            }
        }
        let depth = edge.box_depth(&t);
        let board_edge = || "the board edge".to_string();
        m.0[EDGE].add(rules.silk_to_silk - depth, board_edge);
        m.0[MARGIN].add(TEXT_EDGE - depth, board_edge);
        for cut in &cutouts {
            m.0[EDGE].add(penetration(&t, cut, rules.silk_to_silk), || {
                "a cutout".to_string()
            });
        }
        m
    };
    // Each text the placer places, with its four places and what lies under each.
    let mut owned: Vec<Text> = (0..board.parts.len())
        .filter(|&i| {
            let p = &board.parts[i];
            own(p) && p.designator.is_none() && text_box(p, &rules).is_some()
        })
        .map(|i| {
            let mut probe = board.parts[i].clone();
            let places = ReferenceSide::ALL
                .into_iter()
                .filter_map(|side| {
                    probe.reference_side = side;
                    let t = text_box(&probe, &rules)?;
                    Some((side, t, still(i, t)))
                })
                .collect();
            Text {
                part: i,
                face: board.parts[i].side,
                places,
            }
        })
        .collect();
    let is_owned: std::collections::BTreeSet<usize> = owned.iter().map(|t| t.part).collect();
    // A text placed is in the way of the places of the texts not yet placed.
    let names: Vec<String> = board.parts.iter().map(|p| p.refdes.clone()).collect();
    let put = |owned: &mut [Text], j: usize, face: Side, r: Rect| {
        let refdes = &names[j];
        for text in owned.iter_mut().filter(|t| t.face == face && t.part != j) {
            for (_, t, m) in &mut text.places {
                let what = || format!("{refdes}'s designator");
                m.0[SILK].add(penetration(t, &r, rules.silk_to_silk), what);
                m.0[MARGIN].add(penetration(t, &r, SILK_CLEAR), what);
            }
        }
    };
    // The texts the placer does not own first: they stand where they are.
    for (j, p) in board.parts.iter().enumerate() {
        if !is_owned.contains(&j)
            && let Some(r) = text_box(p, &rules)
        {
            put(&mut owned, j, p.side, r);
        }
    }
    let mut labels = Vec::new();
    while !owned.is_empty() {
        // The text with the fewest clear places left, so that one with a single clear place has
        // it before a neighbour with more takes it; then by path, the same order every time.
        let next = (0..owned.len())
            .min_by(|&a, &b| {
                let key = |t: &Text| {
                    let p = &board.parts[t.part];
                    (t.clear(), p.path.clone(), p.refdes.clone())
                };
                key(&owned[a]).cmp(&key(&owned[b]))
            })
            .expect("a text is left");
        let text = owned.swap_remove(next);
        let Some(best) = (0..text.places.len()).reduce(|best, k| {
            if text.places[k].2.better_than(&text.places[best].2) {
                k
            } else {
                best
            }
        }) else {
            continue;
        };
        // Of the places as clear as the best, the one in the way of the fewest clear places of
        // the texts left; then the first.
        let blocks = |r: &Rect| -> Vec<usize> {
            owned
                .iter()
                .filter(|o| o.face == text.face)
                .flat_map(|o| o.places.iter().map(move |p| (o.part, p)))
                .filter(|(_, (_, t, m))| m.clear() && penetration(t, r, SILK_CLEAR) > 1e-6)
                .map(|(j, _)| j)
                .collect()
        };
        let in_the_way: Vec<Vec<usize>> = text.places.iter().map(|(_, r, _)| blocks(r)).collect();
        let best = (0..text.places.len())
            .filter(|&k| !text.places[best].2.better_than(&text.places[k].2))
            .min_by_key(|&k| (in_the_way[k].len(), k))
            .unwrap_or(best);
        let (side, t, m) = &text.places[best];
        let part = &mut board.parts[text.part];
        part.reference_side = *side;
        let on_pad = m.0[PAD].what.clone();
        if *side != ReferenceSide::Above || on_pad.is_some() {
            labels.push(LabelPlace {
                refdes: part.refdes.clone(),
                path: part.path.clone(),
                side: *side,
                blocked: text.places[..best]
                    .iter()
                    .zip(&in_the_way)
                    .map(|((s, _, m), way)| {
                        let why = m.reason().unwrap_or_else(|| {
                            let mut names: Vec<&str> =
                                way.iter().map(|&j| names[j].as_str()).collect();
                            names.dedup();
                            format!(
                                "in the way of a clear side of {}'s designator",
                                names.join(", ")
                            )
                        });
                        (*s, why)
                    })
                    .collect(),
                on_pad,
            });
        }
        put(&mut owned, text.part, text.face, *t);
    }
    labels.sort_by(|a, b| a.refdes.cmp(&b.refdes));
    board.labels = labels;
}

#[cfg(test)]
mod tests {
    use netsmith_kicad::Mapping;

    use super::*;
    use crate::testlib::{fixtures, led_design};

    fn placed() -> Board {
        crate::board_from_design(&led_design(), &Mapping::standard(), &fixtures()).unwrap()
    }

    fn sides(board: &Board) -> Vec<(String, ReferenceSide)> {
        board
            .parts
            .iter()
            .map(|p| (p.refdes.clone(), p.reference_side))
            .collect()
    }

    #[test]
    fn the_texts_do_not_depend_on_the_side_each_was_on_before() {
        let board = placed();
        for start in ReferenceSide::ALL {
            let mut again = board.clone();
            for p in &mut again.parts {
                p.reference_side = start;
            }
            settle_labels(&mut again, |_| true);
            assert_eq!(sides(&again), sides(&board), "starting {start:?}");
        }
    }

    /// Issue #1729: a part removed leaves the text of a part it was not near where it was, and
    /// a text it was in the way of goes back above its part, listed as no longer moved.
    #[test]
    fn removing_a_part_leaves_another_parts_text_where_it_was_or_lists_it_and_never_on_a_pad() {
        let board = placed();
        let r1 = board.part("R1").unwrap().clone();
        // Put the resistor right above the LED, where the LED's text would go: the text goes to
        // another side, and the record says the resistor is in the way.
        let d1 = board.part("D1").unwrap().clone();
        let rules = board.rules.clone();
        let mut above = d1.clone();
        above.reference_side = ReferenceSide::Above;
        let text = text_box(&above, &rules).unwrap();
        let r1_box = r1.courtyard();
        let shift = text.centre() - r1_box.centre();
        let mut crowded = board.clone();
        if let Some(r) = crowded.parts.iter_mut().find(|p| p.refdes == "R1") {
            r.at = r.at + shift;
        }
        settle_labels(&mut crowded, |_| true);
        let d1_text = crowded.part("D1").unwrap().reference_side;
        assert_ne!(d1_text, ReferenceSide::Above);
        let record = crowded
            .labels
            .iter()
            .find(|l| l.refdes == "D1")
            .expect("D1's text is listed as moved");
        assert!(record.on_pad.is_none(), "{record:?}");
        assert_eq!(record.blocked[0].0, ReferenceSide::Above);
        assert!(record.blocked[0].1.contains("R1"), "{record:?}");
        // Remove the resistor: a text moves only when the resistor was in its way (its record
        // named it), and none is on a pad.
        let mut removed = crowded.clone();
        removed.parts.retain(|p| p.refdes != "R1");
        settle_labels(&mut removed, |_| true);
        for part in &removed.parts {
            let before = crowded.part(&part.refdes).unwrap().reference_side;
            if part.reference_side != before {
                let record = crowded.labels.iter().find(|l| l.refdes == part.refdes);
                // R1 was in its way, or in the way of a neighbour it made room for.
                let names =
                    |l: &LabelPlace, who: &str| l.blocked.iter().any(|(_, why)| why.contains(who));
                let crowded_by_r1: Vec<&str> = crowded
                    .labels
                    .iter()
                    .filter(|l| names(l, "R1"))
                    .map(|l| l.refdes.as_str())
                    .collect();
                assert!(
                    record.is_some_and(
                        |l| names(l, "R1") || crowded_by_r1.iter().any(|n| names(l, n))
                    ),
                    "{} moved from {before:?} to {:?}, and R1 was not in its way: {record:?}",
                    part.refdes,
                    part.reference_side
                );
            }
        }
        assert!(
            removed.labels.iter().all(|l| l.on_pad.is_none()),
            "{:?}",
            removed.labels
        );
        // A part far from the others, removed, moves no other text.
        let mut far = board.clone();
        if let Some(r) = far.parts.iter_mut().find(|p| p.refdes == "R1") {
            r.at = r.at + Pt::new(50.0, 0.0);
        }
        settle_labels(&mut far, |_| true);
        let mut gone = far.clone();
        gone.parts.retain(|p| p.refdes != "R1");
        settle_labels(&mut gone, |_| true);
        for part in &gone.parts {
            assert_eq!(
                part.reference_side,
                far.part(&part.refdes).unwrap().reference_side,
                "{}",
                part.refdes
            );
        }
    }

    #[test]
    fn a_text_the_placer_does_not_own_keeps_its_side() {
        let mut board = placed();
        for p in &mut board.parts {
            p.reference_side = ReferenceSide::Right;
        }
        settle_labels(&mut board, |p| p.refdes != "J1");
        assert_eq!(
            board.part("J1").unwrap().reference_side,
            ReferenceSide::Right
        );
    }
}
