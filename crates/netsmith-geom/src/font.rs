//! A stroke font for silkscreen text, drawn for this project.
//!
//! KiCad's own stroke font ("newstroke") is not used: its data is not ours to copy into this
//! repository, and a font of a few hundred line segments is easy to make. These glyphs are
//! original, plain, single-line shapes on a grid ten units tall (the height of a capital),
//! six wide for capitals and digits, with lowercase letters six tall and descenders three
//! below the line. They are scaled so that a capital is the text's height and a capital
//! `D` is about 0.6 of it wide, which is what KiCad's font measures for the same text size
//! (see the cross-check in `crosscheck.rs`). About 100 glyphs: capitals, digits, lowercase,
//! punctuation, the micro and ohm signs. Text keeps its size, thickness, justification, turn,
//! mirror (back layers) and italic.
//!
//! Each glyph is a list of strokes; a stroke is a path of points `x,y` (y up). A stroke of one
//! point, written twice, is a dot. Characters the font does not have are drawn as an open
//! box and reported, so a board is never plotted with text silently missing.

use crate::artwork::{HAlign, Text, VAlign};
use crate::geom::Pt;

/// The height of a capital, in glyph units.
const CAP: f64 = 10.0;
/// Space between lines, in text heights (KiCad uses 1.62).
const LINE_PITCH: f64 = 1.62;
/// The slant of italic text.
const ITALIC: f64 = 0.2;

/// Strokes and the advance (in glyph units) of a character.
fn glyph(c: char) -> Option<(&'static str, f64)> {
    Some(match c {
        ' ' => ("", 5.0),
        'A' => ("0,0 3,10 6,0; 1,3.5 5,3.5", 9.5),
        'B' => (
            "0,0 0,10 4,10 5.5,9 5.5,6 4,5 0,5; 4,5 6,4 6,1 4.5,0 0,0",
            9.5,
        ),
        'C' => ("6,8 5,10 1,10 0,8 0,2 1,0 5,0 6,2", 9.5),
        'D' => ("0,0 0,10 3.5,10 6,7.5 6,2.5 3.5,0 0,0", 9.5),
        'E' => ("6,10 0,10 0,0 6,0; 0,5 4,5", 9.5),
        'F' => ("6,10 0,10 0,0; 0,5 4,5", 9.5),
        'G' => ("6,8 5,10 1,10 0,8 0,2 1,0 5,0 6,2 6,5 3.5,5", 9.5),
        'H' => ("0,0 0,10; 6,0 6,10; 0,5 6,5", 9.5),
        'I' => ("1,0 5,0; 1,10 5,10; 3,0 3,10", 7.0),
        'J' => ("5,10 5,2 4,0 1.5,0 0.5,2 0.5,3", 9.0),
        'K' => ("0,0 0,10; 6,10 0,4.5; 2.2,6.2 6,0", 9.5),
        'L' => ("0,10 0,0 6,0", 9.0),
        'M' => ("0,0 0,10 3,4 6,10 6,0", 10.0),
        'N' => ("0,0 0,10 6,0 6,10", 9.5),
        'O' => ("1,0 0,2 0,8 1,10 5,10 6,8 6,2 5,0 1,0", 9.5),
        'P' => ("0,0 0,10 4.5,10 6,8.5 6,6.5 4.5,5 0,5", 9.5),
        'Q' => ("1,0 0,2 0,8 1,10 5,10 6,8 6,2 5,0 1,0; 3.5,3 6.5,-0.5", 9.5),
        'R' => ("0,0 0,10 4.5,10 6,8.5 6,6.5 4.5,5 0,5; 3.5,5 6,0", 9.5),
        'S' => (
            "6,8 5,10 1,10 0,8 0,6.5 1,5.2 5,4.8 6,3.5 6,2 5,0 1,0 0,2",
            9.5,
        ),
        'T' => ("0,10 6,10; 3,10 3,0", 9.0),
        'U' => ("0,10 0,2 1,0 5,0 6,2 6,10", 9.5),
        'V' => ("0,10 3,0 6,10", 9.5),
        'W' => ("0,10 1.5,0 3,6 4.5,0 6,10", 10.0),
        'X' => ("0,0 6,10; 0,10 6,0", 9.5),
        'Y' => ("0,10 3,5 6,10; 3,5 3,0", 9.5),
        'Z' => ("0,10 6,10 0,0 6,0", 9.5),
        '0' => ("1,0 0,2 0,8 1,10 5,10 6,8 6,2 5,0 1,0", 9.5),
        '1' => ("1,8 3,10 3,0", 9.5),
        '2' => ("0,8 1,10 5,10 6,8 6,6.5 0,0 6,0", 9.5),
        '3' => (
            "0,9 1,10 5,10 6,8.5 6,6.5 5,5.2 2.5,5.2; 5,5.2 6,3.8 6,1.5 5,0 1,0 0,1",
            9.5,
        ),
        '4' => ("4.5,0 4.5,10 0,3 6,3", 9.5),
        '5' => ("6,10 0.5,10 0,5.5 4,5.5 5.5,5 6,3.5 6,2 5,0 1,0 0,1.5", 9.5),
        '6' => ("5.5,10 2,10 0,7 0,2 1,0 5,0 6,2 6,4 5,5.5 1,5.5 0,4", 9.5),
        '7' => ("0,10 6,10 2.5,0", 9.5),
        '8' => (
            "1,5 0,6.5 0,8.5 1,10 5,10 6,8.5 6,6.5 5,5 1,5 0,3.5 0,1.5 1,0 5,0 6,1.5 6,3.5 5,5",
            9.5,
        ),
        '9' => ("0.5,0 4,0 6,3 6,8 5,10 1,10 0,8 0,6 1,4.5 5,4.5 6,6", 9.5),
        'a' => (
            "5,6 5,0; 5,4.8 3.5,6 1.5,6 0,4.8 0,1.2 1.5,0 3.5,0 5,1.2",
            8.0,
        ),
        'b' => (
            "0,10 0,0; 0,4.8 1.5,6 3.5,6 5,4.8 5,1.2 3.5,0 1.5,0 0,1.2",
            8.0,
        ),
        'c' => ("5,4.8 3.5,6 1.5,6 0,4.8 0,1.2 1.5,0 3.5,0 5,1.2", 8.0),
        'd' => (
            "5,10 5,0; 5,4.8 3.5,6 1.5,6 0,4.8 0,1.2 1.5,0 3.5,0 5,1.2",
            8.0,
        ),
        'e' => ("0,3 5,3 5,4.8 3.5,6 1.5,6 0,4.8 0,1.2 1.5,0 3.5,0 5,1", 8.0),
        'f' => ("5,9 4,10 3,10 2,9 2,0; 0,6 4,6", 6.5),
        'g' => (
            "5,6 5,-1.5 4,-3 1.5,-3 0.5,-2; 5,4.8 3.5,6 1.5,6 0,4.8 0,1.2 1.5,0 3.5,0 5,1.2",
            8.0,
        ),
        'h' => ("0,10 0,0; 0,4.5 1.5,6 3.5,6 5,4.5 5,0", 8.0),
        'i' => ("2.5,6 2.5,0; 2.5,9 2.5,9", 5.5),
        'j' => ("3,6 3,-1.5 2,-3 0.5,-3; 3,9 3,9", 5.5),
        'k' => ("0,10 0,0; 5,6 0,2.5; 1.8,3.7 5,0", 8.0),
        'l' => ("2.5,10 2.5,0", 5.5),
        'm' => (
            "0,6 0,0; 0,4.5 1.2,6 2.7,6 3.5,4.5 3.5,0; 3.5,4.5 4.7,6 6.2,6 7,4.5 7,0",
            10.0,
        ),
        'n' => ("0,6 0,0; 0,4.5 1.5,6 3.5,6 5,4.5 5,0", 8.0),
        'o' => ("1.5,0 0,1.2 0,4.8 1.5,6 3.5,6 5,4.8 5,1.2 3.5,0 1.5,0", 8.0),
        'p' => (
            "0,6 0,-3; 0,4.8 1.5,6 3.5,6 5,4.8 5,1.2 3.5,0 1.5,0 0,1.2",
            8.0,
        ),
        'q' => (
            "5,6 5,-3; 5,4.8 3.5,6 1.5,6 0,4.8 0,1.2 1.5,0 3.5,0 5,1.2",
            8.0,
        ),
        'r' => ("0,6 0,0; 0,4 1.5,6 3.5,6 4.5,5.5", 7.0),
        's' => ("5,5 4,6 1,6 0,5 0,4 1,3 4,3 5,2 5,1 4,0 1,0 0,1", 8.0),
        't' => ("2,9 2,1 3,0 4,0; 0,6 4.5,6", 6.5),
        'u' => ("0,6 0,1.5 1.5,0 3.5,0 5,1.5; 5,6 5,0", 8.0),
        'v' => ("0,6 2.5,0 5,6", 8.0),
        'w' => ("0,6 1.75,0 3.5,5 5.25,0 7,6", 10.0),
        'x' => ("0,6 5,0; 0,0 5,6", 8.0),
        'y' => ("0,6 2.5,0; 5,6 1,-3", 8.0),
        'z' => ("0,6 5,6 0,0 5,0", 8.0),
        '.' => ("1.5,0 1.5,0", 4.0),
        ',' => ("2,0 1,-2.5", 4.0),
        ':' => ("1.5,0.5 1.5,0.5; 1.5,6 1.5,6", 4.0),
        ';' => ("2,6 2,6; 2,0.5 1,-2", 4.0),
        '!' => ("3,10 3,3; 3,0 3,0", 6.0),
        '?' => ("0,8 1,10 5,10 6,8 6,6.5 3,4.5 3,3; 3,0 3,0", 9.0),
        '-' => ("1,5 5,5", 8.0),
        '+' => ("3,2 3,8; 0.5,5 5.5,5", 8.5),
        '=' => ("0.5,3.5 5.5,3.5; 0.5,6.5 5.5,6.5", 8.5),
        '_' => ("0,-1 6,-1", 9.0),
        '/' => ("0,0 6,10", 8.5),
        '\\' => ("0,10 6,0", 8.5),
        '(' => ("4,11 2,8 2,2 4,-1", 6.0),
        ')' => ("2,11 4,8 4,2 2,-1", 6.0),
        '[' => ("4,11 2,11 2,-1 4,-1", 6.0),
        ']' => ("2,11 4,11 4,-1 2,-1", 6.0),
        '{' => ("4,11 2,8 2,2 4,-1", 6.0),
        '}' => ("2,11 4,8 4,2 2,-1", 6.0),
        '<' => ("5,9 1,5 5,1", 8.0),
        '>' => ("1,9 5,5 1,1", 8.0),
        '#' => ("2,0 2.5,10; 4,0 4.5,10; 0,3.5 6,3.5; 0,6.5 6,6.5", 9.5),
        '%' => (
            "0.5,10 2.5,10 2.5,7.5 0.5,7.5 0.5,10; 3.5,2.5 5.5,2.5 5.5,0 3.5,0 3.5,2.5; 0,0 6,10",
            9.5,
        ),
        '*' => ("3,8 3,3; 0.5,6.5 5.5,4.5; 0.5,4.5 5.5,6.5", 8.0),
        '&' => (
            "6,0 1,6 1,8.5 2.5,10 4,8.5 4,7 0,3 0,1.5 1,0 3.5,0 6,4",
            9.5,
        ),
        '@' => (
            "6,4 6,8 5,10 1,10 0,8 0,2 1,0 5,0; 4.5,3 2,3 1.5,4 1.5,6 2,7 4,7 4.5,6 4.5,3 6,4",
            10.0,
        ),
        '~' => ("0,5 1.5,6.5 4.5,3.5 6,5", 9.0),
        '^' => ("1,6 3,10 5,6", 8.0),
        '|' => ("3,-1 3,11", 6.0),
        '$' => (
            "6,8 5,10 1,10 0,8 0,6.5 1,5.2 5,4.8 6,3.5 6,2 5,0 1,0 0,2; 3,-1 3,11",
            9.5,
        ),
        '\'' => ("3,10 3,7", 6.0),
        '"' => ("2,10 2,7; 4,10 4,7", 7.0),
        '`' => ("2,10 4,8", 6.0),
        '\u{b0}' => ("2,10 4,10 4,8 2,8 2,10", 6.5),
        '\u{b5}' => ("0,6 0,-3; 0,2 1,0 4,0 5,2; 5,6 5,0 6,0", 8.0),
        '\u{3a9}' | '\u{2126}' => (
            "1,0 2.5,0 0.5,3 0,5.5 0,7 1.5,10 4.5,10 6,7 6,5.5 5.5,3 3.5,0 5,0",
            9.5,
        ),
        '\u{b1}' => ("3,3 3,9; 0.5,6 5.5,6; 0.5,1 5.5,1", 8.5),
        _ => return None,
    })
}

/// The strokes of a text on the board, and the characters the font does not have. A stroke
/// is a path of points in board coordinates; a path of one repeated point is a dot.
pub fn strokes(text: &Text) -> (Vec<Vec<Pt>>, Vec<char>) {
    let (sx, sy) = (text.size.0 / CAP, text.size.1 / CAP);
    let mut missing = Vec::new();
    let lines: Vec<&str> = text.text.split('\n').collect();
    let mut out = Vec::new();
    // The block's height from the first baseline to the last, for vertical justification.
    let pitch = LINE_PITCH * text.size.1;
    let n = lines.len() as f64;
    let block_top = CAP * sy;
    let block_bottom = -(n - 1.0) * pitch;
    let y_shift = match text.v {
        VAlign::Top => -block_top,
        VAlign::Centre => -(block_top + block_bottom) / 2.0,
        VAlign::Bottom => -block_bottom,
    };
    for (row, line) in lines.iter().enumerate() {
        let width: f64 = line
            .chars()
            .map(|c| glyph(c).map_or(9.5, |(_, a)| a))
            .sum::<f64>()
            * sx;
        // The last glyph's advance has a gap after it that is not part of the text.
        let width = width - 3.5 * sx;
        let x_shift = match text.h {
            HAlign::Left => 0.0,
            HAlign::Centre => -width / 2.0,
            HAlign::Right => -width,
        };
        let baseline = -(row as f64) * pitch + y_shift;
        let mut pen = 0.0;
        for c in line.chars() {
            let (paths, advance) = match glyph(c) {
                Some(g) => g,
                None => {
                    if !missing.contains(&c) {
                        missing.push(c);
                    }
                    ("0,0 0,10 6,10 6,0 0,0", 9.5)
                }
            };
            for path in paths.split(';').map(str::trim).filter(|p| !p.is_empty()) {
                let pts: Vec<Pt> = path
                    .split_whitespace()
                    .filter_map(|xy| {
                        let (x, y) = xy.split_once(',')?;
                        let (x, y): (f64, f64) = (x.parse().ok()?, y.parse().ok()?);
                        // Text space is y up; the slant leans the top to the right.
                        let slant = if text.italic { ITALIC * y * sx } else { 0.0 };
                        let tx = x_shift + pen + x * sx + slant;
                        let ty = baseline + y * sy;
                        let tx = if text.mirror { -tx } else { tx };
                        // To the board: y down, turned by the text's angle about its anchor.
                        Some(text.at + Pt::new(tx, -ty).rotate(text.angle))
                    })
                    .collect();
                if !pts.is_empty() {
                    out.push(pts);
                }
            }
            pen += advance * sx;
        }
    }
    (out, missing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Text {
        Text {
            layer: "F.SilkS".into(),
            text: s.into(),
            at: Pt::new(10.0, 20.0),
            angle: 0.0,
            size: (1.0, 1.0),
            thickness: 0.15,
            mirror: false,
            italic: false,
            h: HAlign::Centre,
            v: VAlign::Centre,
            owner: "R1".into(),
        }
    }

    fn extent(strokes: &[Vec<Pt>]) -> (f64, f64, f64, f64) {
        let b = crate::geom::Bounds::of(strokes.iter().flatten().copied()).unwrap();
        (b.min.x, b.max.x, b.min.y, b.max.y)
    }

    #[test]
    fn a_capital_is_as_tall_as_the_text_and_centred_on_its_anchor() {
        let (s, missing) = strokes(&text("H"));
        assert!(missing.is_empty());
        let (x0, x1, y0, y1) = extent(&s);
        assert!((y1 - y0 - 1.0).abs() < 1e-9, "height {}", y1 - y0);
        assert!(((y0 + y1) / 2.0 - 20.0).abs() < 1e-9);
        assert!(((x0 + x1) / 2.0 - 10.0).abs() < 1e-9);
        assert!((x1 - x0 - 0.6).abs() < 1e-9, "width {}", x1 - x0);
    }

    #[test]
    fn justification_moves_the_text_and_mirroring_flips_it_about_the_anchor() {
        let mut t = text("R1");
        t.h = HAlign::Left;
        t.v = VAlign::Bottom;
        let (x0, _, _, y1) = extent(&strokes(&t).0);
        assert!((x0 - 10.0).abs() < 1e-9 && (y1 - 20.0).abs() < 1e-9);
        t.mirror = true;
        let (_, x1, _, _) = extent(&strokes(&t).0);
        assert!(
            (x1 - 10.0).abs() < 1e-9,
            "a mirrored text lies to the left of its anchor"
        );
    }

    #[test]
    fn a_turned_text_is_turned_counter_clockwise_on_screen() {
        let mut t = text("I");
        t.angle = 90.0;
        let (x0, x1, y0, y1) = extent(&strokes(&t).0);
        // Upright it is taller than wide; turned a quarter it is wider than tall.
        assert!(x1 - x0 > y1 - y0);
    }

    #[test]
    fn a_character_the_font_lacks_is_reported_and_drawn_as_a_box() {
        let (s, missing) = strokes(&text("A\u{e9}"));
        assert_eq!(missing, ['\u{e9}']);
        assert!(s.iter().any(|p| p.len() == 5));
    }

    #[test]
    fn every_ascii_printable_character_but_the_few_left_out_has_a_glyph() {
        let left_out = [];
        for c in ' '..='~' {
            if left_out.contains(&c) {
                continue;
            }
            assert!(glyph(c).is_some(), "no glyph for {c:?}");
        }
    }

    #[test]
    fn two_lines_are_spaced_by_the_line_pitch() {
        let one = extent(&strokes(&text("A")).0);
        let two = extent(&strokes(&text("A\nA")).0);
        let height = two.3 - two.2;
        assert!(
            (height - (one.3 - one.2) - LINE_PITCH).abs() < 1e-9,
            "{height}"
        );
    }
}
