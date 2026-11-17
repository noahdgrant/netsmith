//! The widths of the PDF's standard fonts, so that text anchored at its middle or its end is
//! placed where the PDF viewer draws it. The widths are those of Adobe's font metrics for the
//! core fonts (Helvetica and Helvetica-Bold, in thousandths of the em; every Courier letter is
//! 600), which a PDF reader uses for these fonts without the file carrying them.
//!
//! The metrics of the printable ASCII letters are exact; a letter of Latin-1 above them is taken
//! as wide as the letter it is made from (`é` as `e`), and any other as 556 (a digit's width).

use crate::Face;

/// Helvetica, from the space (32) to the tilde (126).
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // 32-47
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // 48-63
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // 64-79
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // 80-95
    333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // 96-111
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, // 112-126
];

/// Helvetica-Bold, the same letters.
const HELVETICA_BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, // 32-47
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, // 48-63
    975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778, // 64-79
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556, // 80-95
    333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, // 96-111
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584, // 112-126
];

/// The width of `c` in thousandths of the em.
pub(crate) fn width(face: Face, c: char) -> u16 {
    if face.mono() {
        return 600;
    }
    let table = if face.bold() {
        &HELVETICA_BOLD
    } else {
        &HELVETICA
    };
    let c = base_letter(c);
    match c as u32 {
        32..=126 => table[c as usize - 32],
        _ => 556,
    }
}

/// The ASCII letter an accented Latin-1 letter is made from, or the letter itself.
fn base_letter(c: char) -> char {
    match c {
        'À'..='Å' => 'A',
        'Ç' => 'C',
        'È'..='Ë' => 'E',
        'Ì'..='Ï' => 'I',
        'Ñ' => 'N',
        'Ò'..='Ö' | 'Ø' => 'O',
        'Ù'..='Ü' => 'U',
        'Ý' => 'Y',
        'à'..='å' => 'a',
        'ç' => 'c',
        'è'..='ë' => 'e',
        'ì'..='ï' => 'i',
        'ñ' => 'n',
        'ò'..='ö' | 'ø' => 'o',
        'ù'..='ü' => 'u',
        'ý' | 'ÿ' => 'y',
        '\u{a0}' => ' ',
        _ => c,
    }
}

/// How far a line of `text` in `face` at `size` (the em, in the picture's units) runs along its
/// baseline, as a PDF reader draws it.
pub fn advance(face: Face, text: &str, size: f64) -> f64 {
    let total: u32 = text.chars().map(|c| u32::from(width(face, c))).sum();
    f64::from(total) * size / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_widths() {
        assert_eq!(width(Face::Sans, 'A'), 667);
        assert_eq!(width(Face::Sans, 'i'), 222);
        assert_eq!(width(Face::SansBold, 'i'), 278);
        assert_eq!(width(Face::Sans, '~'), 584);
        assert_eq!(width(Face::Mono, 'W'), 600);
        assert_eq!(width(Face::Sans, 'é'), width(Face::Sans, 'e'));
        assert!((advance(Face::Sans, "R1", 2.0) - 2.0 * (722.0 + 556.0) / 1000.0).abs() < 1e-12);
    }
}
