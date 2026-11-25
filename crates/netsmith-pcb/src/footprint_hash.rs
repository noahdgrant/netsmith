//! The hash of a footprint's geometry, which the pick lock records for each part (issue #1683).
//!
//! It covers what the board is built from, read from the footprint's source (the `.kicad_mod`
//! file, the standard library's own file, or what a generator wrote), and nothing a person reads
//! only:
//!
//! - every pad: its number, type, shape, position and turn, size, drill and its offset, layers,
//!   corner ratio, custom primitives, chamfers, and its own clearance, solder mask and paste
//!   margins and ratio, and fill connection settings;
//! - the footprint's own `attr` (how it is mounted), clearance, mask and paste margins and
//!   ratio, connection settings and net-tie pad groups;
//! - the shapes drawn on the courtyard, solder mask, solder paste, copper and board edge
//!   layers (a courtyard line without its width, which nothing measures);
//! - its keepout zones.
//!
//! It leaves out the silkscreen and fabrication drawings, every text and property (reference,
//! value, description, tags), the 3D model, the uuids, the file's version and generator, and the
//! footprint's own name (the lock records the id beside the hash). Pads, shapes and zones are
//! hashed as a set, so a file that lists them in another order hashes the same. Numbers are
//! rounded to 0.1 µm, as netsmith writes them ([`netsmith_kicad::fmt_num`]), so a number written
//! `0.50` or `0.5` is one number, and a generated footprint hashes the same on every machine.

use netsmith_kicad::{Sexp, fmt_num};
use sha2::{Digest, Sha256};

use crate::footprint::Footprint;

/// The tag the canonical text starts with: a change to what the hash covers changes it, so that
/// every lock says so once.
const TAG: &str = "netsmith footprint geometry 1";

/// The heads of the footprint's own settings that the hash covers.
const SETTINGS: [&str; 10] = [
    "attr",
    "clearance",
    "solder_mask_margin",
    "solder_paste_margin",
    "solder_paste_margin_ratio",
    "solder_paste_ratio",
    "zone_connect",
    "thermal_width",
    "thermal_gap",
    "net_tie_pad_groups",
];

/// The heads of drawn shapes.
const SHAPES: [&str; 5] = ["fp_line", "fp_rect", "fp_circle", "fp_arc", "fp_poly"];

/// Children that say nothing of the geometry: left out wherever they are.
const NOT_GEOMETRY: [&str; 6] = ["uuid", "tstamp", "locked", "net", "pinfunction", "pintype"];

/// What a courtyard shape leaves out besides: the width of its line, which the placer does not
/// measure (on copper, mask and paste the line is the opening and stays).
const COURTYARD_LINE: [&str; 2] = ["stroke", "width"];

impl Footprint {
    /// The hash of the footprint's geometry (see the module's notes for what it covers): the
    /// first 16 hexadecimal digits of the SHA-256 of a canonical text, the same on every machine
    /// for the same pads, shapes and settings.
    pub fn geometry_hash(&self) -> String {
        let digest = Sha256::digest(canonical(&self.source).as_bytes());
        digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
    }
}

/// The canonical text of what the hash covers, one line for each item, sorted.
fn canonical(source: &Sexp) -> String {
    let mut lines: Vec<String> = Vec::new();
    for item in source.args() {
        let Some(head) = item.head() else { continue };
        let covered = match head {
            "pad" | "zone" => true,
            h if SETTINGS.contains(&h) => true,
            h if SHAPES.contains(&h) => layer_of(item).is_some_and(is_built_layer),
            _ => false,
        };
        if covered {
            let courtyard =
                SHAPES.contains(&head) && layer_of(item).is_some_and(|l| l.ends_with(".CrtYd"));
            let mut line = String::new();
            write(item, &mut line, courtyard);
            lines.push(line);
        }
    }
    lines.sort();
    let mut text = String::from(TAG);
    for line in lines {
        text.push('\n');
        text.push_str(&line);
    }
    text
}

/// The layer a shape is drawn on.
fn layer_of(item: &Sexp) -> Option<&str> {
    item.find("layer")?.args().first()?.text()
}

/// Whether a shape on `layer` is something the board is made from: the courtyard (which the
/// placer and the placement check measure), the mask and paste openings, copper, and the edge.
fn is_built_layer(layer: &str) -> bool {
    matches!(
        layer,
        "F.CrtYd" | "B.CrtYd" | "F.Mask" | "B.Mask" | "F.Paste" | "B.Paste" | "Edge.Cuts"
    ) || layer.ends_with(".Cu")
}

/// `item` as canonical text: numbers by [`fmt_num`], every other atom and string quoted, and the
/// children of [`NOT_GEOMETRY`] left out, with those of [`COURTYARD_LINE`] for a courtyard.
fn write(item: &Sexp, out: &mut String, courtyard: bool) {
    match item {
        Sexp::Atom(a) => match a.parse::<f64>() {
            Ok(n) if n.is_finite() => out.push_str(&fmt_num(n)),
            _ => push_quoted(a, out),
        },
        Sexp::Str(s) => push_quoted(s, out),
        Sexp::List(items) => {
            out.push('(');
            let mut first = true;
            for (i, child) in items.iter().enumerate() {
                let left_out = child.head().is_some_and(|h| {
                    NOT_GEOMETRY.contains(&h) || (courtyard && COURTYARD_LINE.contains(&h))
                });
                if i > 0 && left_out {
                    continue;
                }
                if !first {
                    out.push(' ');
                }
                first = false;
                // The head of a list is its keyword, never a number.
                if i == 0
                    && let Sexp::Atom(head) = child
                {
                    out.push_str(head);
                } else {
                    write(child, out, courtyard);
                }
            }
            out.push(')');
        }
    }
}

fn push_quoted(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"(footprint "R_0402" (version 20241229) (generator "pcbnew") (layer "F.Cu")
        (descr "a resistor") (tags "R")
        (property "Reference" "REF**" (at 0 -1 0) (layer "F.SilkS") (uuid "a1") (effects (font (size 1 1))))
        (property "Value" "R_0402" (at 0 1 0) (layer "F.Fab") (uuid "a2") (effects (font (size 1 1))))
        (attr smd)
        (fp_line (start -0.15 -0.26) (end 0.15 -0.26) (stroke (width 0.12) (type solid)) (layer "F.SilkS") (uuid "b1"))
        (fp_rect (start -0.93 -0.47) (end 0.93 0.47) (stroke (width 0.05) (type solid)) (fill no) (layer "F.CrtYd") (uuid "b2"))
        (fp_text user "${REFERENCE}" (at 0 0 0) (layer "F.Fab") (uuid "b3") (effects (font (size 0.26 0.26))))
        (pad "1" smd roundrect (at -0.51 0) (size 0.54 0.64) (layers "F.Cu" "F.Mask" "F.Paste") (roundrect_rratio 0.25) (uuid "c1"))
        (pad "2" smd roundrect (at 0.51 0) (size 0.54 0.64) (layers "F.Cu" "F.Mask" "F.Paste") (roundrect_rratio 0.25) (uuid "c2"))
        (model "${KICAD9_3DMODEL_DIR}/Resistor_SMD.3dshapes/R_0402.step" (offset (xyz 0 0 0)))
    )"#;

    fn hash(text: &str) -> String {
        Footprint::parse("Lib:R_0402", text)
            .unwrap()
            .geometry_hash()
    }

    #[test]
    fn the_hash_is_sixteen_hexadecimal_digits_and_the_same_each_time() {
        let h = hash(BASE);
        assert_eq!(h.len(), 16);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()), "{h}");
        assert_eq!(hash(BASE), h);
    }

    #[test]
    fn a_resized_pad_changes_the_hash() {
        let resized = BASE.replacen("(size 0.54 0.64)", "(size 0.60 0.64)", 1);
        assert_ne!(hash(&resized), hash(BASE));
        let moved = BASE.replacen("(at -0.51 0)", "(at -0.55 0)", 1);
        assert_ne!(hash(&moved), hash(BASE));
        let paste = BASE.replacen(
            "(roundrect_rratio 0.25) (uuid \"c1\")",
            "(roundrect_rratio 0.25) (solder_paste_margin -0.05) (uuid \"c1\")",
            1,
        );
        assert_ne!(hash(&paste), hash(BASE));
        let courtyard = BASE.replace("(end 0.93 0.47)", "(end 1.0 0.47)");
        assert_ne!(hash(&courtyard), hash(BASE));
    }

    #[test]
    fn silk_text_uuids_and_spelling_leave_the_hash_as_it_was() {
        let silk = BASE.replace(
            "(start -0.15 -0.26) (end 0.15 -0.26)",
            "(start -0.3 -0.3) (end 0.3 -0.3)",
        );
        assert_eq!(hash(&silk), hash(BASE));
        let text = BASE
            .replace("a resistor", "a chip resistor")
            .replace(
                "(at 0 -1 0) (layer \"F.SilkS\")",
                "(at 0 -1.2 0) (layer \"F.SilkS\")",
            )
            .replace("(size 0.26 0.26)", "(size 0.4 0.4)");
        assert_eq!(hash(&text), hash(BASE));
        let uuids = BASE.replace("\"c1\"", "\"d1\"").replace("\"b2\"", "\"d2\"");
        assert_eq!(hash(&uuids), hash(BASE));
        let spelled = BASE.replacen("(size 0.54 0.64)", "(size 0.540 0.6400)", 1);
        assert_eq!(hash(&spelled), hash(BASE));
        let stroke = BASE.replace("(width 0.05)", "(width 0.1)");
        assert_eq!(hash(&stroke), hash(BASE));
        let model = BASE.replace("R_0402.step", "R_0402_other.step");
        assert_eq!(hash(&model), hash(BASE));
        // The pads in another order are the same pads.
        let pad1 = "(pad \"1\" smd roundrect (at -0.51 0) (size 0.54 0.64) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") (roundrect_rratio 0.25) (uuid \"c1\"))";
        let swapped = BASE
            .replace(pad1, "")
            .replace("(model ", &format!("{pad1}\n        (model "));
        assert_eq!(hash(&swapped), hash(BASE));
    }
}
