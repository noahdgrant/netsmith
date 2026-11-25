//! `exposed_pad_paste` (issue #1686): an exposed pad with no solder paste over its copper is a
//! note, which fails nothing; one with paste, its own or the footprint's paste-only openings,
//! is not noted.

use crate::common;

use common::*;
use netsmith_geom::{DrcExtras, DrcReport, DrcRule, DrcSeverity, drc_check};

fn check(b: &mut Pcb, strict: bool) -> DrcReport {
    let (pcb, pro) = (b.pcb(), b.pro());
    let extras = DrcExtras {
        strict_guidelines: strict,
        ..DrcExtras::default()
    };
    drc_check(&pcb, Some(&pro), &extras).unwrap_or_else(|e| panic!("{e}"))
}

fn noted(r: &DrcReport) -> Vec<String> {
    r.findings
        .iter()
        .filter(|f| f.rule == DrcRule::ExposedPadPaste)
        .map(ToString::to_string)
        .collect()
}

/// A board with one QFN-like part U1 at (20, 15): four 0.6 mm leads 2 mm from its centre and a
/// 2 mm exposed pad numbered 5 on GND, written with `ep` after its size (its layers and any
/// setting of its own), `extra` among the footprint's items and `attr` as its attributes.
fn board(ep: &str, extra: &str, attr: &str) -> Pcb {
    let mut b = Pcb::new(2);
    let sig = b.net_ref(Some("SIG"));
    let gnd = b.net_ref(Some("GND"));
    let lead = |n: u32, x: f64, y: f64| {
        format!(
            "\t\t(pad \"{n}\" smd rect (at {x} {y}) (size 0.6 0.6) (layers \"F.Cu\" \"F.Mask\" \"F.Paste\") {sig})\n"
        )
    };
    let mut s = format!(
        "(footprint \"Test:QFN\" (layer \"F.Cu\") (at 20 15 0)\n\t\t(property \"Reference\" \"U1\" (at 0 -3 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))\n\t\t(property \"Value\" \"v\" (at 0 3 0) (layer \"F.Fab\") (hide yes) (effects (font (size 1 1) (thickness 0.15))))\n\t\t(attr {attr})\n\t\t(fp_rect (start -2.6 -2.6) (end 2.6 2.6) (stroke (width 0.05) (type solid)) (fill no) (layer \"F.CrtYd\"))\n"
    );
    for (n, (x, y)) in [(-2.0, 0.0), (0.0, 2.0), (2.0, 0.0), (0.0, -2.0)]
        .into_iter()
        .enumerate()
    {
        s.push_str(&lead(n as u32 + 1, x, y));
    }
    s.push_str(&format!(
        "\t\t(pad \"5\" smd rect (at 0 0) (size 2 2) {ep} {gnd})\n{extra}\t)"
    ));
    b.raw(&s);
    b
}

const NO_PASTE: &str = "(layers \"F.Cu\" \"F.Mask\")";
const PASTE: &str = "(layers \"F.Cu\" \"F.Mask\" \"F.Paste\")";

#[test]
fn an_exposed_pad_with_no_paste_layer_is_a_note_with_any_strictness() {
    for strict in [false, true] {
        let r = check(&mut board(NO_PASTE, "", "smd"), strict);
        let notes = noted(&r);
        assert_eq!(notes.len(), 1, "{r}");
        assert!(
            notes[0].starts_with("note [exposed_pad_paste] SMD pad 5 [GND] of U1, an exposed pad, has no solder paste on F.Paste: the pad is not on F.Paste, and no paste opening of U1 covers it; unsoldered, it does not carry the part's heat into the board (a datasheet's thermal resistance assumes the pad soldered), nor joins its net GND through the pad"),
            "{}",
            notes[0]
        );
        let f = r
            .findings
            .iter()
            .find(|f| f.rule == DrcRule::ExposedPadPaste)
            .unwrap();
        assert_eq!(f.severity, DrcSeverity::Note);
        assert_eq!(f.layer.as_deref(), Some("F.Cu"));
        assert!(r.is_clean() || r.errors().all(|e| e.rule != DrcRule::ExposedPadPaste));
    }
}

#[test]
fn an_exposed_pad_with_paste_is_not_noted() {
    let r = check(&mut board(PASTE, "", "smd"), true);
    assert!(noted(&r).is_empty(), "{r}");
}

#[test]
fn paste_only_pads_over_an_exposed_pad_are_its_paste() {
    // The openings a library draws apart: unnumbered pads on the paste layer alone.
    let mut openings = String::new();
    for (x, y) in [(-0.5, -0.5), (0.5, -0.5), (-0.5, 0.5), (0.5, 0.5)] {
        openings.push_str(&format!(
            "\t\t(pad \"\" smd rect (at {x} {y}) (size 0.8 0.8) (layers \"F.Paste\"))\n"
        ));
    }
    let r = check(&mut board(NO_PASTE, &openings, "smd"), false);
    assert!(noted(&r).is_empty(), "{r}");
    // A drawing on the paste layer is an opening too.
    let poly = "\t\t(fp_poly (pts (xy -0.8 -0.8) (xy 0.8 -0.8) (xy 0.8 0.8) (xy -0.8 0.8)) (stroke (width 0) (type solid)) (fill solid) (layer \"F.Paste\"))\n";
    let r = check(&mut board(NO_PASTE, poly, "smd"), false);
    assert!(noted(&r).is_empty(), "{r}");
}

#[test]
fn a_paste_ratio_that_leaves_no_opening_is_a_note() {
    // KiCad's ratio is of each side: -50 % takes the whole pad away (a ratio of 0 shrinks
    // nothing).
    let own = format!("{PASTE} (solder_paste_margin_ratio -0.5)");
    let r = check(&mut board(&own, "", "smd"), false);
    let notes = noted(&r);
    assert_eq!(notes.len(), 1, "{r}");
    assert!(
        notes[0].contains(
            "its paste margin (0 mm) and ratio (-0.5) leave no opening, and no other paste opening of U1 covers it"
        ),
        "{}",
        notes[0]
    );
    let r = check(
        &mut board(&format!("{PASTE} (solder_paste_margin_ratio 0)"), "", "smd"),
        false,
    );
    assert!(noted(&r).is_empty(), "{r}");
    // A margin that eats the pad is no opening either.
    let r = check(
        &mut board(&format!("{PASTE} (solder_paste_margin -1.1)"), "", "smd"),
        false,
    );
    assert_eq!(noted(&r).len(), 1, "{r}");
}

#[test]
fn a_part_that_is_not_fitted_or_the_rule_turned_off_notes_nothing() {
    let r = check(&mut board(NO_PASTE, "", "smd dnp"), false);
    assert!(noted(&r).is_empty(), "{r}");
    let mut b = board(NO_PASTE, "", "smd");
    let (pcb, pro) = (b.pcb(), b.pro());
    let pro = pro.replacen(
        "\"rule_severities\": {",
        "\"rule_severities\": {\"exposed_pad_paste\": \"ignore\", ",
        1,
    );
    assert!(pro.contains("exposed_pad_paste"), "{pro}");
    let r = drc_check(&pcb, Some(&pro), &DrcExtras::default()).unwrap();
    assert!(noted(&r).is_empty(), "{r}");
}
