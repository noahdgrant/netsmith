//! The places that the placer's sentences and the design rules check's lines say, moved into
//! another frame (`--frame`, issue #1613).
//!
//! A sentence is made once, in the frame its stage works in (the placer's in the outline's, the
//! check's in the board file's), and read by a person or put in a finding's `message`. Where a
//! command moves the places of its text into the frame asked for, the places of the data are
//! moved in the data; these sentences hold places that the data does not keep apart, so each
//! template that says one is recognised here, next to its words ([`BOARD_TAIL`] and the
//! others are what the generators in this crate use), and a test makes each generator say the
//! template.

/// What a placer sentence says after a place (or a box): the frame it is in.
pub const BOARD_TAIL: &str = "mm from the board's top left corner";

/// What the heading of the list of overlapping courtyards says of its positions.
pub const POSITIONS_HEAD: &str = "positions from the outline's top left corner";

/// A `(x, y)` in `text` at `start` (the index of its `(`): its end, its numbers and how many
/// decimals each has.
fn parse_pair(text: &str, start: usize) -> Option<(usize, f64, f64, usize, usize)> {
    let rest = text.get(start..)?.strip_prefix('(')?;
    let close = rest.find(')')?;
    let (a, b) = rest[..close].split_once(", ")?;
    let decimals = |s: &str| s.split_once('.').map_or(0, |(_, d)| d.len());
    let number = |s: &str| {
        let ok = !s.is_empty()
            && s.bytes()
                .all(|c| c.is_ascii_digit() || c == b'.' || c == b'-');
        if ok { s.parse::<f64>().ok() } else { None }
    };
    Some((
        start + 1 + close + 1,
        number(a)?,
        number(b)?,
        decimals(a),
        decimals(b),
    ))
}

/// To a micrometre, with no `-0`, as the data's places are.
fn tidy(v: f64) -> f64 {
    let r = (v * 1e6).round() / 1e6;
    if r == 0.0 { 0.0 } else { r }
}

/// The pairs `(x, y)` of `line` in `from..to` (indices), moved by `by`, each with the decimals
/// it had.
fn shift_pairs(line: &str, from: usize, to: usize, by: [f64; 2]) -> String {
    let mut out = String::new();
    let mut at = from;
    while let Some(i) = line[at..to].find('(').map(|i| at + i) {
        match parse_pair(line, i) {
            Some((end, x, y, dx, dy)) if end <= to => {
                out.push_str(&line[at..i]);
                out.push_str(&format!(
                    "({:.dx$}, {:.dy$})",
                    tidy(x + by[0]),
                    tidy(y + by[1]),
                    dx = dx,
                    dy = dy
                ));
                at = end;
            }
            _ => {
                out.push_str(&line[at..=i]);
                at = i + 1;
            }
        }
    }
    out.push_str(&line[at..to]);
    out
}

/// A via of a wall of `routes route`: `net X at (x, y)`, the place in the outline's frame. The
/// generator (`netsmith_fabricate::route_net`) says it by this, and [`via_moved`] reads it.
pub fn via_said(net: &str, at: [f64; 2]) -> String {
    format!("net {net} at ({}, {})", tidy(at[0]), tidy(at[1]))
}

/// `text`, a via as [`via_said`] says it, with its place moved by `by` (mm). With a move of
/// nothing, or a text that is not one, it is `text`.
pub fn via_moved(text: &str, by: [f64; 2]) -> String {
    if by == [0.0, 0.0] {
        return text.to_string();
    }
    let Some(i) = text.rfind(" at (").map(|i| i + " at ".len()) else {
        return text.to_string();
    };
    match parse_pair(text, i) {
        Some((end, ..)) if end == text.len() => {
            let mut out = text[..i].to_string();
            out.push_str(&shift_pairs(text, i, end, by));
            out
        }
        _ => text.to_string(),
    }
}

/// The sentences of the placer in `text`, whose places are in the outline's frame, with each
/// place moved by `by` (mm) and the words that name its frame replaced by `says` (what a text
/// says after `mm`, `Frame::says` of the command line): a part's courtyard or body "from the
/// board's top left corner", and the positions of the courtyards that overlap. With a move of
/// nothing it is `text`.
pub fn placer_moved(text: &str, by: [f64; 2], says: &str) -> String {
    if by == [0.0, 0.0] {
        return text.to_string();
    }
    let mut out = String::new();
    let mut positions = false;
    for line in text.split_inclusive('\n') {
        if let Some(tail) = line.find(BOARD_TAIL) {
            // The place or the box just before the words, and what the same sentence says
            // before them ("(1, 2) to (3, 4)").
            let head = &line[..tail];
            let mut from = tail;
            let mut chain = head.trim_end();
            while let Some(open) = chain.rfind('(').filter(|_| chain.ends_with(')')) {
                if parse_pair(chain, open).is_none() {
                    break;
                }
                from = open;
                chain = chain[..open].trim_end();
                let Some(before) = chain.strip_suffix(" to") else {
                    break;
                };
                chain = before;
            }
            out.push_str(&line[..from]);
            out.push_str(&shift_pairs(line, from, tail, by));
            out.push_str(says);
            out.push_str(&line[tail + BOARD_TAIL.len()..]);
            continue;
        }
        if let Some(i) = line.find(POSITIONS_HEAD) {
            positions = true;
            out.push_str(&line[..i]);
            out.push_str(&format!("positions in {says}"));
            out.push_str(&line[i + POSITIONS_HEAD.len()..]);
            continue;
        }
        // A line of the list: `... mm2 (w x h mm) at (x, y), d mm from the left edge`.
        if positions && line.starts_with("    ") && line.contains(" mm) at (") {
            let i = line.find(" mm) at (").expect("found") + " mm) at ".len();
            if let Some((end, ..)) = parse_pair(line, i) {
                out.push_str(&line[..i]);
                out.push_str(&shift_pairs(line, i, end, by));
                out.push_str(&line[end..]);
                continue;
            }
        }
        if !line.starts_with("    ") {
            positions = false;
        }
        out.push_str(line);
    }
    out
}

/// The lines of the design rules check in `text` (`severity [rule] message at (x, y) ...`,
/// every number to a thousandth of a millimetre), whose places are in the frame `by` away from
/// the one asked for, moved by `by` (mm).
pub fn drc_moved(text: &str, by: [f64; 2]) -> String {
    if by == [0.0, 0.0] {
        return text.to_string();
    }
    let mut out = String::new();
    for line in text.split_inclusive('\n') {
        let Some(rule) = line.find(" [").filter(|_| line.contains("] ")) else {
            out.push_str(line);
            continue;
        };
        // The last ` at (x, y)` with three decimals after the rule.
        let mut found = None;
        let mut from = rule;
        while let Some(i) = line[from..].find(" at (").map(|i| from + i + " at ".len()) {
            if let Some((end, _, _, 3, 3)) = parse_pair(line, i) {
                found = Some((i, end));
            }
            from = i;
        }
        match found {
            Some((i, end)) => {
                out.push_str(&line[..i]);
                out.push_str(&shift_pairs(line, i, end, by));
                out.push_str(&line[end..]);
            }
            None => out.push_str(line),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAYS: &str =
        "mm in the board file's frame, y down, the outline's top left corner at (20, 20)";

    #[test]
    fn a_place_before_the_frame_words_is_moved_and_the_words_say_the_new_frame() {
        let text =
            "R1 stands 2.00 mm tall, around (10.5, 12.0) mm from the board's top left corner\n";
        let moved = placer_moved(text, [20.0, 20.0], SAYS);
        assert_eq!(
            moved,
            format!("R1 stands 2.00 mm tall, around (30.5, 32.0) {SAYS}\n")
        );
        assert_eq!(placer_moved(text, [0.0, 0.0], SAYS), text);
        // And back.
        let back = placer_moved(
            &moved.replace(SAYS, BOARD_TAIL),
            [-20.0, -20.0],
            "mm from the outline's top left corner, y down",
        );
        assert!(
            back.contains("(10.5, 12.0) mm from the outline's"),
            "{back}"
        );
    }

    #[test]
    fn a_box_is_moved_by_both_corners() {
        let text = "x; the courtyard of R1 is (8.97, 8.97) to (11.03, 11.03) mm from the board's top left corner";
        let moved = placer_moved(text, [20.0, 20.0], "mm somewhere");
        assert_eq!(
            moved,
            "x; the courtyard of R1 is (28.97, 28.97) to (31.03, 31.03) mm somewhere"
        );
    }

    #[test]
    fn the_positions_of_overlapping_courtyards_are_moved() {
        let text = "refused\n  courtyards overlapping (the most overlapped first, positions from the outline's top left corner):\n    R1 and R2: 1.2 mm2 (1.00 x 1.20 mm) at (10.0, 10.5), 3.0 mm from the left edge\n    and 2 more: R3, R4\n  relation not held: x at (1.0, 2.0)\n";
        let moved = placer_moved(text, [20.0, 20.0], "mm there");
        assert!(moved.contains("positions in mm there):"), "{moved}");
        assert!(moved.contains("mm) at (30.0, 30.5), 3.0 mm"), "{moved}");
        // Only the list's lines.
        assert!(moved.contains("x at (1.0, 2.0)"), "{moved}");
    }

    #[test]
    fn a_via_of_a_wall_is_moved_at_its_place() {
        let said = via_said("VCC", [12.5, 3.0]);
        assert_eq!(said, "net VCC at (12.5, 3)");
        assert_eq!(via_moved(&said, [20.0, 20.0]), "net VCC at (32.5, 23)");
        assert_eq!(via_moved(&said, [0.0, 0.0]), said);
        // A net whose name says a place is not mistaken for one.
        let odd = via_said("A at (1, 2)", [1.5, 2.0]);
        assert_eq!(via_moved(&odd, [1.0, 1.0]), "net A at (1, 2) at (2.5, 3)");
        assert_eq!(via_moved("no place", [1.0, 1.0]), "no place");
    }

    #[test]
    fn a_line_of_the_design_rules_check_is_moved_at_its_place_only() {
        let text = "error [clearance] a 0.100 mm gap (0.200 mm wanted) at (25.000, 30.500) on F.Cu: track; pad\nplain (1.000, 2.000) line\n";
        let moved = drc_moved(text, [-20.0, -20.0]);
        assert_eq!(
            moved,
            "error [clearance] a 0.100 mm gap (0.200 mm wanted) at (5.000, 10.500) on F.Cu: track; pad\nplain (1.000, 2.000) line\n"
        );
    }

    #[test]
    fn the_sentences_the_placement_check_makes_are_moved() {
        use crate::{PlacementProblem, Side};
        let tall = PlacementProblem::TooTall {
            refdes: "C1".into(),
            path: "stage.c1".into(),
            keepout: "lid".into(),
            height: 3.0,
            limit: 2.0,
            growth: 0.0,
            side: Side::Front,
            at: (4.5, 6.0),
        };
        let text = tall.to_string();
        assert!(
            text.contains("around (4.5, 6.0) mm from the board's top left corner"),
            "{text}"
        );
        let moved = placer_moved(&text, [20.0, 20.0], SAYS);
        assert!(
            moved.contains(&format!("around (24.5, 26.0) {SAYS}")),
            "{moved}"
        );
        // The rest of the sentence is as it was.
        assert_eq!(
            moved
                .replace("(24.5, 26.0)", "(4.5, 6.0)")
                .replace(SAYS, BOARD_TAIL),
            text
        );

        let cut = PlacementProblem::OutsideOutline {
            refdes: "R1".into(),
            path: "r1".into(),
            past: Vec::new(),
            stated: Vec::new(),
            by: String::new(),
            cut: Some(0.4),
            at: Some((1.25, 2.0)),
            near: Some("top left corner"),
        };
        let text = cut.to_string();
        assert!(
            text.contains("(1.2, 2.0) mm from the board's top left corner"),
            "{text}"
        );
        let moved = placer_moved(&text, [20.0, 20.0], SAYS);
        assert!(moved.contains(&format!("(21.2, 22.0) {SAYS}")), "{moved}");
    }
}
