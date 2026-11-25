//! `exposed_pad_paste` (issue #1686): an exposed pad with no solder paste. A part's thermal
//! resistance in its datasheet assumes its exposed pad is soldered to the board, and so does
//! the ground or the supply it carries; a pad with no paste opening is not soldered by the
//! stencil. The check does not judge it (a person may solder the pad by hand, or a datasheet
//! may say the pad needs no solder): each is a note, which fails nothing, with any strictness.
//!
//! **What is an exposed pad.** The board file says nothing of a part's heat, and no part of the
//! catalogs or the standard library states a thermal resistance, so the pad is told by the
//! footprint alone: a surface pad numbered `EP` or `EPAD`, or a surface pad of a part with at
//! least four other copper pads whose centre lies inside the box of their centres (on both
//! axes) and that is larger than each of them: the pad under a QFN, a DFN, a QFP or an SOIC
//! with a thermal pad. Pads of one number are one pad, the pieces of a split exposed pad.
//! **Not covered:** a tab on the edge of a part (a DPAK's, an SOT-223's), a thermal pad of a
//! part with fewer than four other pads (a power LED's), and a pad joined to a pin that a
//! part's thermal figures depend on, since no part states such figures; each is soldered or not
//! as its footprint says, unnoted.
//!
//! **What is a paste opening.** A piece of the pad is covered when its part's paste layer on
//! the pad's side (`F.Paste` for a pad on `F.Cu`, `B.Paste` on `B.Cu`) has an opening over its
//! copper: the pad's own, a paste-only pad of the part (an unnumbered pad on the paste layer
//! alone, the openings a library draws apart), or a drawing of the footprint on the paste
//! layer, each by its box. An opening is the pad's shape grown or shrunk by its paste margin
//! and ratio, or the board's ([`crate::pads::paste_opening`]), as the Gerber writer draws it; a
//! margin or a ratio that leaves nothing is no opening. A part that is not fitted (`dnp`) is not
//! soldered at all and is not noted.

use super::checks::{EPS, Out};
use super::model::Kind;
use super::{Finding, Rule, Severity};
use crate::artwork::{Artwork, Graphic, Pad, PadKind, Shape};
use crate::geom::Bounds;
use crate::pads;

/// The least number of other copper pads round an exposed pad.
const LEAST_OTHERS: usize = 4;

pub(crate) fn run(out: &mut Out<'_>) {
    let m = out.m;
    if m.rules.severity(Rule::ExposedPadPaste).is_none() {
        return;
    }
    let art = m.art;
    let mut parts: Vec<&str> = Vec::new();
    for p in &art.pads {
        if !parts.contains(&p.part.as_str()) {
            parts.push(&p.part);
        }
    }
    let mut notes: Vec<Finding> = Vec::new();
    for part in parts {
        if art
            .footprints
            .iter()
            .any(|f| f.part == part && f.not_fitted)
        {
            continue;
        }
        // The part's pads with copper, with their index in the board's pads.
        let copper: Vec<(usize, &Pad)> = art
            .pads
            .iter()
            .enumerate()
            .filter(|(_, p)| p.part == part && m.layers.iter().any(|l| art.pad_copper_on(p, l)))
            .collect();
        let mut numbers: Vec<&str> = Vec::new();
        for (_, p) in &copper {
            if p.kind == PadKind::Smd
                && !numbers.contains(&p.number.as_str())
                && is_exposed(p, &copper)
            {
                numbers.push(&p.number);
            }
        }
        for number in numbers {
            for (i, piece) in copper
                .iter()
                .filter(|(_, p)| p.number == number && p.kind == PadKind::Smd)
            {
                let Some((cu, paste)) = side(art, piece) else {
                    continue;
                };
                let Some(lacks) = no_paste(art, part, piece, paste) else {
                    continue;
                };
                let Some(item) = m
                    .items
                    .iter()
                    .find(|c| matches!(c.kind, Kind::Pad(k) if k == *i))
                    .map(|c| c.item.clone())
                else {
                    continue;
                };
                let net = piece
                    .net
                    .as_deref()
                    .map(|n| format!(", nor joins its net {n} through the pad"))
                    .unwrap_or_default();
                notes.push(Finding {
                    rule: Rule::ExposedPadPaste,
                    severity: Severity::Note,
                    message: format!(
                        "{}, an exposed pad, has no solder paste on {paste}: {lacks}; unsoldered, it does not carry the part's heat into the board (a datasheet's thermal resistance assumes the pad soldered){net}: add a paste opening, or keep it",
                        item.what
                    ),
                    at: piece.at,
                    layer: Some(cu.to_string()),
                    items: vec![item],
                    carrying: None,
                    source: None,
                    distance: None,
                });
            }
        }
    }
    out.findings.extend(notes);
}

/// Whether a surface pad is an exposed pad: numbered `EP` or `EPAD`, or inside the box of the
/// centres of at least four other copper pads of its part and larger than each of them.
fn is_exposed(pad: &Pad, copper: &[(usize, &Pad)]) -> bool {
    if ["EP", "EPAD"]
        .iter()
        .any(|n| pad.number.eq_ignore_ascii_case(n))
    {
        return true;
    }
    let others: Vec<&Pad> = copper
        .iter()
        .map(|(_, p)| *p)
        .filter(|p| p.number != pad.number)
        .collect();
    if others.len() < LEAST_OTHERS {
        return false;
    }
    let Some(round) = Bounds::of(others.iter().map(|p| p.at)) else {
        return false;
    };
    let inside = pad.at.x > round.min.x + EPS
        && pad.at.x < round.max.x - EPS
        && pad.at.y > round.min.y + EPS
        && pad.at.y < round.max.y - EPS;
    let area = |p: &Pad| p.size.0 * p.size.1;
    inside
        && others
            .iter()
            .all(|o| area(pad) > area(o) * (1.0 + 1e-6) + 1e-9)
}

/// The outer copper layer of a surface pad and the paste layer of its side.
fn side(art: &Artwork, pad: &Pad) -> Option<(&'static str, &'static str)> {
    [("F.Cu", "F.Paste"), ("B.Cu", "B.Paste")]
        .into_iter()
        .find(|(cu, _)| art.pad_copper_on(pad, cu))
}

/// Why a piece of an exposed pad has no paste over its copper, or `None` when it has some (or
/// its shape cannot be worked out, which says nothing).
fn no_paste(art: &Artwork, part: &str, piece: &Pad, paste: &str) -> Option<String> {
    let copper = pads::outline(piece).bounds().expanded(-EPS);
    let on_paste = |p: &Pad| p.layers.iter().any(|l| l == paste);
    for p in art.pads.iter().filter(|p| p.part == part && on_paste(p)) {
        match pads::paste_opening(p, art) {
            Ok(Some(o)) if o.bounds().overlaps(&copper) => return None,
            Err(_) if std::ptr::eq(p, piece) => return None,
            _ => {}
        }
    }
    if art
        .graphics
        .iter()
        .filter(|g| g.owner == part && g.layer == paste)
        .filter_map(drawing_box)
        .any(|b| b.overlaps(&copper))
    {
        return None;
    }
    Some(if on_paste(piece) {
        let margin = piece.paste_margin.unwrap_or(art.paste_margin);
        let ratio = piece.paste_ratio.unwrap_or(art.paste_ratio);
        format!(
            "its paste margin ({} mm) and ratio ({}) leave no opening, and no other paste opening of {part} covers it",
            num(margin),
            num(ratio)
        )
    } else {
        format!("the pad is not on {paste}, and no paste opening of {part} covers it")
    })
}

/// The box a drawing covers.
fn drawing_box(g: &Graphic) -> Option<Bounds> {
    let half = g.width.max(0.0) / 2.0;
    let b = match &g.shape {
        Shape::Line(a, b) => Bounds::of([*a, *b])?,
        Shape::Arc(a) => Bounds::of([a.centre, a.centre])?.expanded(a.radius),
        Shape::Circle { centre, radius } => Bounds::of([*centre])?.expanded(*radius),
        Shape::Poly(points) => Bounds::of(points.iter().copied())?,
    };
    Some(b.expanded(half))
}

/// A number with at most three decimals and no trailing zeros: `-0.5`, `0`.
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0" } else { s }.to_string()
}
