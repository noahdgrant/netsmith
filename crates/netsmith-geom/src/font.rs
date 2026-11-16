//! A stroke font for silkscreen text, drawn for this project.
//!
//! KiCad's own stroke font ("newstroke") is not used: its data is not ours to copy into this
//! repository, and a font of a few hundred line segments is easy to make. These glyphs are
//! original, plain, single-line shapes on a grid ten units tall (the height of a capital),
//! six wide for capitals and digits, with lowercase letters six tall and descenders three
//! below the line. They are scaled so that a capital is the text's height and a capital
//! `D` is about 0.6 of it wide, which is what KiCad's font measures for the same text size
//! (see the cross-check in `crosscheck.rs`). About 100 glyphs for ASCII, the micro and ohm
//! signs, and, for issue #839, the Latin-1 letters with accents (a plain letter and a mark
//! drawn over it), the Greek letters a board uses, the superscripts, fractions and currency
//! and maths signs of Latin-1 and a few more (arrows, `≤ ≥ ≈ ≠ ∞ √`, dashes, quotes). Text
//! keeps its size, thickness, justification, turn, mirror (back layers) and italic.
//!
//! **Source of the glyphs.** Every glyph here, the ones added for issue #839 included, is
//! drawn for this project (the added ones in the style of the first ones, and an accented
//! letter is its plain letter with a mark over it). No third-party font data is in this file,
//! so it has the repository's own licence and there is nothing to record beyond that. The
//! Hershey fonts, which KiCad's font is derived from, were looked at for no outline.
//!
//! Each glyph is a list of strokes; a stroke is a path of points `x,y` (y up). A stroke of one
//! point, written twice, is a dot. Characters the font does not have are drawn as an open
//! box and reported, so a board is never plotted with text silently missing.

use std::borrow::Cow;

use crate::artwork::{HAlign, Text, VAlign};
use crate::geom::Pt;

/// The height of a capital, in glyph units.
const CAP: f64 = 10.0;
/// Space between lines, in text heights (KiCad uses 1.62).
const LINE_PITCH: f64 = 1.62;
/// The slant of italic text.
const ITALIC: f64 = 0.2;

/// Strokes and the advance (in glyph units) of a character: the drawn glyphs of [`drawn`], then
/// the ones made from them ([`composed`]).
fn glyph(c: char) -> Option<(Cow<'static, str>, f64)> {
    match drawn(c) {
        Some((paths, advance)) => Some((Cow::Borrowed(paths), advance)),
        None => composed(c).map(|(paths, advance)| (Cow::Owned(paths), advance)),
    }
}

/// The glyphs drawn by hand: strokes and the advance (in glyph units).
fn drawn(c: char) -> Option<(&'static str, f64)> {
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
        // Symbols and letters beyond ASCII, drawn for this project in the style of the glyphs
        // above (issue #839). The accented Latin letters, the superscripts, the fractions and
        // the trade mark are made from these by `composed`.
        '\u{a1}' => ("3,0 3,7; 3,10 3,10", 6.0),
        '\u{bf}' => ("6,2 5,0 1,0 0,2 0,3.5 3,5.5 3,7; 3,10 3,10", 9.0),
        '\u{a2}' => ("5.5,8 4.5,9 1.5,9 0,7 0,3 1.5,1 4.5,1 5.5,2; 3,0 3,10", 9.5),
        '\u{a3}' => ("5.5,8.5 4.5,10 2,10 1,8.5 1,2 0,0 6,0; 0,5 4,5", 9.5),
        '\u{a5}' => ("0,10 3,5 6,10; 3,5 3,0; 0.5,3.5 5.5,3.5; 0.5,6 5.5,6", 9.5),
        '\u{20ac}' => (
            "6,8 5,10 2,10 0.5,8 0.5,2 2,0 5,0 6,2; -0.5,4 4.5,4; -0.5,6 4.5,6",
            9.5,
        ),
        '\u{b7}' | '\u{2022}' => (
            "3,3.5 4.5,5 3,6.5 1.5,5 3,3.5; 3,3.5 3,6.5; 1.5,5 4.5,5",
            6.5,
        ),
        '\u{d7}' => ("1,2 5,8; 1,8 5,2", 8.0),
        '\u{f7}' => ("0.5,5 5.5,5; 3,8 3,8; 3,2 3,2", 8.5),
        '\u{2013}' => ("0,5 6,5", 8.5),
        '\u{2014}' => ("0,5 10,5", 12.5),
        '\u{2018}' => ("3,7 2,10", 5.5),
        '\u{2019}' => ("3,10 2,7", 5.5),
        '\u{201c}' => ("3,7 2,10; 6,7 5,10", 8.5),
        '\u{201d}' => ("3,10 2,7; 6,10 5,7", 8.5),
        '\u{ab}' => ("5,8 1,5 5,2; 8,8 4,5 8,2", 11.0),
        '\u{bb}' => ("1,8 5,5 1,2; 4,8 8,5 4,2", 11.0),
        '\u{2264}' => ("5,9 1,6 5,3; 1,1 5,1", 8.0),
        '\u{2265}' => ("1,9 5,6 1,3; 1,1 5,1", 8.0),
        '\u{2248}' => ("0,7 1.5,8 4.5,6 6,7; 0,3.5 1.5,4.5 4.5,2.5 6,3.5", 9.0),
        '\u{2260}' => ("0.5,3.5 5.5,3.5; 0.5,6.5 5.5,6.5; 1.5,1 4.5,9", 8.5),
        '\u{221e}' => (
            "5,5 6.5,6.5 8,6.5 9,5 8,3.5 6.5,3.5 5,5 3.5,6.5 2,6.5 1,5 2,3.5 3.5,3.5 5,5",
            12.5,
        ),
        '\u{221a}' => ("0,5 1,4.5 2.5,0 5,10 9,10", 11.0),
        '\u{2192}' => ("0,5 7,5; 4,8 7,5 4,2", 10.0),
        '\u{2190}' => ("7,5 0,5; 3,8 0,5 3,2", 10.0),
        '\u{2191}' => ("3,0 3,10; 0,7 3,10 6,7", 9.0),
        '\u{2193}' => ("3,10 3,0; 0,3 3,0 6,3", 9.0),
        // Greek capitals (the ohm sign is above) and the lowercase letters a board uses (the
        // micro sign is above; `composed` makes the Greek mu from it).
        '\u{393}' => ("0,0 0,10 6,10", 9.0),
        '\u{394}' => ("0,0 3,10 6,0 0,0", 9.5),
        '\u{398}' => ("1,0 0,2 0,8 1,10 5,10 6,8 6,2 5,0 1,0; 2,5 4,5", 9.5),
        '\u{39b}' => ("0,0 3,10 6,0", 9.5),
        '\u{39e}' => ("0,10 6,10; 1,5 5,5; 0,0 6,0", 9.5),
        '\u{3a0}' => ("0,0 0,10; 6,0 6,10; -0.5,10 6.5,10", 9.5),
        '\u{3a3}' => ("6,10 0,10 3,5 0,0 6,0", 9.5),
        '\u{3a6}' => ("3,0 3,10; 3,2 1,2.5 0,5 1,7.5 3,8 5,7.5 6,5 5,2.5 3,2", 9.5),
        '\u{3a8}' => ("3,0 3,10; 0,10 0,6 1,4 3,3 5,4 6,6 6,10", 9.5),
        '\u{3b1}' => (
            "5,6 5,0.5 5.5,0; 5,4.8 3.5,6 1.5,6 0,4.8 0,1.2 1.5,0 3.5,0 5,1.2",
            8.5,
        ),
        '\u{3b2}' => (
            "0,-3 0,8 1,10 3.5,10 4.5,8.5 4.5,6.5 3.5,5.5 0,5.5; 3.5,5.5 5,4.5 5,1.5 3.5,0 1.5,0 0,1.5",
            8.0,
        ),
        '\u{3b3}' => ("0,6 2.5,1 5,6; 2.5,1 2.5,-3", 8.0),
        '\u{3b5}' => (
            "5,5 4,6 1.5,6 0.5,5 0.5,4 1.5,3 3,3; 1.5,3 0.5,2 0.5,1 1.5,0 4,0 5,1",
            8.0,
        ),
        '\u{3b8}' => (
            "1.5,0 0,2 0,8 1.5,10 3.5,10 5,8 5,2 3.5,0 1.5,0; 0,5 5,5",
            8.0,
        ),
        '\u{3bb}' => ("0,10 1,10 5,0; 2.5,5 0,0", 8.0),
        '\u{3c0}' => ("0,6 5.5,6; 1.5,6 1.5,0; 4,6 4,0.5 4.5,0", 8.5),
        '\u{3c3}' => (
            "5.5,6 2.5,6; 2.5,6 1,6 0,4.8 0,1.2 1.5,0 3.5,0 5,1.2 5,4.8 3.5,6",
            8.5,
        ),
        '\u{3c4}' => ("0,6 5,6; 2.5,6 2.5,0.5 3,0 4,0", 8.0),
        '\u{3c6}' => (
            "2.5,-3 2.5,9; 2.5,6 1,5.5 0,4 0,2 1,0.5 2.5,0 4,0.5 5,2 5,4 4,5.5 2.5,6",
            8.0,
        ),
        '\u{3c9}' => (
            "0,6 0,1.5 1,0 2.5,0 3.5,1.5 3.5,4; 3.5,1.5 4.5,0 6,0 7,1.5 7,6",
            10.0,
        ),
        '\u{df}' => (
            "0,0 0,8 1,10 4,10 5,8.5 5,7 3.5,5.5 2,5.5; 3.5,5.5 5.5,4.5 5.5,1.5 4,0 1,0",
            9.0,
        ),
        '\u{d8}' => (
            "1,0 0,2 0,8 1,10 5,10 6,8 6,2 5,0 1,0; -0.5,-0.5 6.5,10.5",
            9.5,
        ),
        '\u{f8}' => (
            "1.5,0 0,1.2 0,4.8 1.5,6 3.5,6 5,4.8 5,1.2 3.5,0 1.5,0; -0.5,-0.5 5.5,6.5",
            8.0,
        ),
        _ => return None,
    })
}

/// The cedilla, the one mark that hangs below the baseline.
const CEDILLA: &str = "3,0 3,-1.5 2,-2.5 1,-2";

/// The accent of a letter with a mark over (or, for the cedilla, under) a plain one: the plain
/// letter and the mark. The mark is drawn for a capital, centred at x = 3 above a height of 10;
/// over a lowercase letter [`composed`] lowers it to the height of the letters.
fn accented(c: char) -> Option<(char, &'static str)> {
    const ACUTE: &str = "2.5,11 4,13";
    const GRAVE: &str = "2,13 3.5,11";
    const CIRCUMFLEX: &str = "1.5,11 3,13 4.5,11";
    const DIAERESIS: &str = "1.5,12 1.5,12; 4.5,12 4.5,12";
    const TILDE: &str = "1,11.5 2,12.5 4,11.5 5,12.5";
    const RING: &str = "2.5,11 3.5,11 3.5,12 2.5,12 2.5,11";
    Some(match c {
        '\u{c0}' => ('A', GRAVE),
        '\u{c1}' => ('A', ACUTE),
        '\u{c2}' => ('A', CIRCUMFLEX),
        '\u{c3}' => ('A', TILDE),
        '\u{c4}' => ('A', DIAERESIS),
        '\u{c5}' => ('A', RING),
        '\u{c7}' => ('C', CEDILLA),
        '\u{c8}' => ('E', GRAVE),
        '\u{c9}' => ('E', ACUTE),
        '\u{ca}' => ('E', CIRCUMFLEX),
        '\u{cb}' => ('E', DIAERESIS),
        '\u{cc}' => ('I', GRAVE),
        '\u{cd}' => ('I', ACUTE),
        '\u{ce}' => ('I', CIRCUMFLEX),
        '\u{cf}' => ('I', DIAERESIS),
        '\u{d1}' => ('N', TILDE),
        '\u{d2}' => ('O', GRAVE),
        '\u{d3}' => ('O', ACUTE),
        '\u{d4}' => ('O', CIRCUMFLEX),
        '\u{d5}' => ('O', TILDE),
        '\u{d6}' => ('O', DIAERESIS),
        '\u{d9}' => ('U', GRAVE),
        '\u{da}' => ('U', ACUTE),
        '\u{db}' => ('U', CIRCUMFLEX),
        '\u{dc}' => ('U', DIAERESIS),
        '\u{dd}' => ('Y', ACUTE),
        '\u{e0}' => ('a', GRAVE),
        '\u{e1}' => ('a', ACUTE),
        '\u{e2}' => ('a', CIRCUMFLEX),
        '\u{e3}' => ('a', TILDE),
        '\u{e4}' => ('a', DIAERESIS),
        '\u{e5}' => ('a', RING),
        '\u{e7}' => ('c', CEDILLA),
        '\u{e8}' => ('e', GRAVE),
        '\u{e9}' => ('e', ACUTE),
        '\u{ea}' => ('e', CIRCUMFLEX),
        '\u{eb}' => ('e', DIAERESIS),
        '\u{ec}' => ('\u{131}', GRAVE),
        '\u{ed}' => ('\u{131}', ACUTE),
        '\u{ee}' => ('\u{131}', CIRCUMFLEX),
        '\u{ef}' => ('\u{131}', DIAERESIS),
        '\u{f1}' => ('n', TILDE),
        '\u{f2}' => ('o', GRAVE),
        '\u{f3}' => ('o', ACUTE),
        '\u{f4}' => ('o', CIRCUMFLEX),
        '\u{f5}' => ('o', TILDE),
        '\u{f6}' => ('o', DIAERESIS),
        '\u{f9}' => ('u', GRAVE),
        '\u{fa}' => ('u', ACUTE),
        '\u{fb}' => ('u', CIRCUMFLEX),
        '\u{fc}' => ('u', DIAERESIS),
        '\u{fd}' => ('y', ACUTE),
        '\u{ff}' => ('y', DIAERESIS),
        _ => return None,
    })
}

/// The paths of `paths` scaled by `(kx, ky)` and moved by `(dx, dy)`.
fn placed(paths: &str, kx: f64, ky: f64, dx: f64, dy: f64) -> String {
    paths
        .split(';')
        .map(|path| {
            path.split_whitespace()
                .filter_map(|xy| {
                    let (x, y) = xy.split_once(',')?;
                    let (x, y): (f64, f64) = (x.parse().ok()?, y.parse().ok()?);
                    Some(format!("{},{}", x * kx + dx, y * ky + dy))
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The characters `chars` as one glyph, each drawn at `scale` and raised by `lift`, one after
/// the other starting at `from`: the strokes and the x where the next would start.
fn run(chars: &str, scale: f64, from: f64, lift: f64) -> (String, f64) {
    let mut pen = from;
    let mut paths = Vec::new();
    for c in chars.chars() {
        if let Some((p, advance)) = drawn(c) {
            paths.push(placed(p, scale, scale, pen, lift));
            pen += advance * scale;
        }
    }
    (paths.join("; "), pen)
}

/// The glyphs made from the drawn ones: a letter with its accent (a dotless `ı` for `ì í î ï`),
/// the superscript digits, the vulgar fractions, the trade mark, the copyright and registered
/// signs, and the Greek mu (the micro sign).
fn composed(c: char) -> Option<(String, f64)> {
    if let Some((base, mark)) = accented(c) {
        // `ı` is the stem of `i` alone, without the dot the accent replaces.
        let (strokes, advance, upper) = match base {
            '\u{131}' => ("2.5,6 2.5,0", 5.5, false),
            _ => {
                let (p, a) = drawn(base)?;
                (p, a, base.is_uppercase())
            }
        };
        // The mark is drawn centred at x = 3 over a capital; a lowercase letter is five wide,
        // and the mark comes down to just above its height of six. The cedilla hangs from
        // the baseline and moves sideways only.
        let dx = if upper { 0.0 } else { -0.5 };
        let dy = if upper || mark == CEDILLA { 0.0 } else { -4.0 };
        let mark = placed(mark, 1.0, 1.0, dx, dy);
        return Some((format!("{strokes}; {mark}"), advance));
    }
    match c {
        '\u{b2}' => {
            let (p, end) = run("2", 0.55, 0.0, 4.5);
            Some((p, end + 0.5))
        }
        '\u{b3}' => {
            let (p, end) = run("3", 0.55, 0.0, 4.5);
            Some((p, end + 0.5))
        }
        '\u{b9}' => {
            let (p, end) = run("1", 0.55, 0.0, 4.5);
            Some((p, end + 0.5))
        }
        '\u{bc}' | '\u{bd}' | '\u{be}' => {
            let (top, bottom) = match c {
                '\u{bc}' => ('1', '4'),
                '\u{bd}' => ('1', '2'),
                _ => ('3', '4'),
            };
            let (n, _) = run(&top.to_string(), 0.5, 0.0, 5.0);
            let (d, end) = run(&bottom.to_string(), 0.5, 6.5, 0.0);
            Some((format!("{n}; 3.5,0 7.5,10; {d}"), end + 1.0))
        }
        '\u{2122}' => {
            let (p, end) = run("TM", 0.5, 0.0, 5.0);
            Some((p, end))
        }
        '\u{a9}' | '\u{ae}' => {
            // A ring of sixteen sides around the letter, 12 wide and 12 tall about (6, 5).
            let ring: Vec<String> = (0..=16)
                .map(|i| {
                    let a = std::f64::consts::TAU * f64::from(i) / 16.0;
                    format!("{:.3},{:.3}", 6.0 + 6.0 * a.cos(), 5.0 + 6.0 * a.sin())
                })
                .collect();
            let inner = if c == '\u{a9}' { "C" } else { "R" };
            let (p, _) = run(inner, 0.55, 3.0, 2.2);
            Some((format!("{}; {p}", ring.join(" ")), 15.0))
        }
        '\u{3bc}' => drawn('\u{b5}').map(|(p, a)| (p.to_string(), a)),
        _ => None,
    }
}

/// The width of one line of text, where `sx` is the text's character width divided by
/// [`CAP`] (a glyph unit's length), without the gap after the last glyph.
fn line_width(line: &str, sx: f64) -> f64 {
    let advances: f64 = line.chars().map(|c| glyph(c).map_or(9.5, |(_, a)| a)).sum();
    // The last glyph's advance has a gap after it that is not part of the text.
    (advances - 3.5) * sx
}

/// `text` with its lines broken at the spaces so that none is wider than `width` (the text's
/// character width is `size_x`), as a text box wraps. A line that has no space to break at
/// stays whole, and the lines the text already has are kept.
pub(crate) fn wrap(text: &str, size_x: f64, width: f64) -> String {
    let sx = size_x / CAP;
    let mut out: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let mut current = String::new();
        for word in line.split(' ') {
            let joined = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if current.is_empty() || line_width(&joined, sx) <= width {
                current = joined;
            } else {
                out.push(std::mem::take(&mut current));
                current = word.to_string();
            }
        }
        out.push(current);
    }
    out.join("\n")
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
        let width = line_width(line, sx);
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
                    (Cow::Borrowed("0,0 0,10 6,10 6,0 0,0"), 9.5)
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
        let (s, missing) = strokes(&text("A\u{4e2d}"));
        assert_eq!(missing, ['\u{4e2d}']);
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
    fn the_latin_1_letters_greek_letters_and_signs_a_board_uses_have_glyphs() {
        let wanted = "\u{c0}\u{c1}\u{c2}\u{c3}\u{c4}\u{c5}\u{c7}\u{c8}\u{c9}\u{ca}\u{cb}\
            \u{cc}\u{cd}\u{ce}\u{cf}\u{d1}\u{d2}\u{d3}\u{d4}\u{d5}\u{d6}\u{d8}\u{d9}\u{da}\u{db}\u{dc}\u{dd}\u{df}\
            \u{e0}\u{e1}\u{e2}\u{e3}\u{e4}\u{e5}\u{e7}\u{e8}\u{e9}\u{ea}\u{eb}\u{ec}\u{ed}\u{ee}\u{ef}\u{f1}\
            \u{f2}\u{f3}\u{f4}\u{f5}\u{f6}\u{f8}\u{f9}\u{fa}\u{fb}\u{fc}\u{fd}\u{ff}\
            \u{a1}\u{a2}\u{a3}\u{a5}\u{a9}\u{ab}\u{ae}\u{b2}\u{b3}\u{b7}\u{b9}\u{bb}\u{bc}\u{bd}\u{be}\u{bf}\
            \u{d7}\u{f7}\u{2013}\u{2014}\u{2018}\u{2019}\u{201c}\u{201d}\u{2022}\u{20ac}\u{2122}\
            \u{2190}\u{2191}\u{2192}\u{2193}\u{2248}\u{2260}\u{2264}\u{2265}\u{221a}\u{221e}\
            \u{393}\u{394}\u{398}\u{39b}\u{39e}\u{3a0}\u{3a3}\u{3a6}\u{3a8}\u{3a9}\
            \u{3b1}\u{3b2}\u{3b3}\u{3b5}\u{3b8}\u{3bb}\u{3bc}\u{3c0}\u{3c3}\u{3c4}\u{3c6}\u{3c9}";
        let (paths, missing) = strokes(&text(wanted));
        assert!(missing.is_empty(), "{missing:?}");
        assert!(!paths.is_empty());
        for c in wanted.chars() {
            let (strokes, advance) = glyph(c).unwrap_or_else(|| panic!("no glyph for {c:?}"));
            assert!(advance > 0.0, "{c:?}");
            // Every point of a glyph parses, so none is dropped.
            let points = strokes.split_whitespace().count();
            let parsed = strokes
                .split(';')
                .flat_map(str::split_whitespace)
                .filter(|xy| {
                    xy.split_once(',')
                        .is_some_and(|(x, y)| x.parse::<f64>().is_ok() && y.parse::<f64>().is_ok())
                })
                .count();
            assert_eq!(points, parsed, "{c:?}: {strokes}");
        }
    }

    /// How far a text's ink rises above its baseline, and how far it hangs below it, in text
    /// heights (a capital rises 1.0), and how wide it is.
    fn ink(s: &str) -> (f64, f64, f64) {
        let mut t = text(s);
        t.v = VAlign::Bottom;
        let (x0, x1, y0, y1) = extent(&strokes(&t).0);
        (20.0 - y0, y1 - 20.0, x1 - x0)
    }

    #[test]
    fn an_accent_is_a_mark_over_the_plain_letter() {
        // Over a capital the mark rises above the capital's height; over a lowercase letter it
        // stands above the letter (0.6), below a capital's mark.
        assert!(ink("\u{c9}").0 > ink("E").0 + 0.2);
        assert!(ink("\u{e9}").0 > ink("e").0 + 0.2);
        assert!(ink("\u{e9}").0 < ink("\u{c9}").0);
        // The plain letter is kept, and the advance is the plain letter's.
        let (e, ea) = glyph('e').unwrap();
        let (accented, aa) = glyph('\u{e9}').unwrap();
        assert!(accented.starts_with(e.as_ref()));
        assert_eq!(ea, aa);
        // The cedilla hangs under the baseline; the plain `c` does not.
        assert!(ink("\u{e7}").1 > ink("c").1 + 0.1);
        // `ï` has no dot of its own: the diaeresis is over a bare stem, as high as `i`'s dot.
        assert!(ink("\u{ef}").0 < ink("\u{cf}").0);
    }

    #[test]
    fn a_superscript_is_a_small_digit_above_the_line_and_a_fraction_has_two() {
        let (rise, drop, width) = ink("\u{b2}");
        // All of it is above the baseline (a negative drop), at half the digit's size.
        assert!(rise <= ink("2").0 + 1e-9 && drop < -0.4 && width < 0.4);
        let (fraction_rise, fraction_drop, _) = ink("\u{bd}");
        assert!(fraction_rise > 0.95 && fraction_drop < 0.05);
    }

    #[test]
    fn a_text_is_wrapped_at_its_spaces_to_a_width_and_keeps_its_own_lines() {
        let sx = 1.0 / CAP;
        let one = line_width("AB CD", sx);
        // Exactly as wide as the line: it fits; a little narrower: it breaks at the space.
        assert_eq!(wrap("AB CD", 1.0, one), "AB CD");
        assert_eq!(wrap("AB CD", 1.0, one - 0.01), "AB\nCD");
        // A word wider than the width stays whole, and an own line break is kept.
        assert_eq!(wrap("ABCDEF GH\nI", 1.0, 0.5), "ABCDEF\nGH\nI");
        assert_eq!(wrap("", 1.0, 5.0), "");
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
