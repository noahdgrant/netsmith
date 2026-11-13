//! Flags and net names at the end of stubs: their text and the room they take.

use super::*;

/// The name inside a flag whose point is at `tip` and that points `dir`: across for a flag
/// that points left or right, upward for one that points up or down.
pub(super) fn port_label(name: &str, tip: Pt, dir: Dir) -> Text {
    let start = tip.step(dir, 1.1);
    match dir {
        Dir::Left => Text::new(name, Pt::new(start.x, start.y + 0.3), Anchor::End),
        Dir::Right => Text::new(name, Pt::new(start.x, start.y + 0.3), Anchor::Start),
        Dir::Up => Text::new(name, Pt::new(start.x + 0.3, start.y), Anchor::Start).turned(),
        Dir::Down => Text::new(name, Pt::new(start.x + 0.3, start.y), Anchor::End).turned(),
    }
}

/// How long a flag is for `name`, from its point.
pub(super) fn port_length(name: Option<&str>) -> f64 {
    name.map_or(3.0, |n| {
        n.chars().count() as f64 * Port::TEXT * FLAG_CHAR_WIDTH + 2.2
    })
}

/// The name of a net written at `e`, the end of a stub that points `dir`, in line with the
/// stub: across for a stub that points left or right, upward for one that points up or down.
pub(super) fn name_text(name: &str, e: Pt, dir: Dir) -> Text {
    match dir {
        Dir::Right => Text::new(name, Pt::new(e.x + NAME_GAP, e.y + 0.45), Anchor::Start),
        Dir::Left => Text::new(name, Pt::new(e.x - NAME_GAP, e.y + 0.45), Anchor::End),
        Dir::Up => Text::new(name, Pt::new(e.x + 0.45, e.y - NAME_GAP), Anchor::Start).turned(),
        Dir::Down => Text::new(name, Pt::new(e.x + 0.45, e.y + NAME_GAP), Anchor::End).turned(),
    }
}

/// What is drawn at the end of a stub: a flag for a net that leaves the sheet, else the net's
/// name. `rect` is what it covers.
pub(super) struct Tag {
    pub(super) port: Option<Port>,
    pub(super) label: Option<NetLabel>,
    pub(super) rect: Rect,
}

/// What a label says for `net`: its name, or for a net with a generated name the first pin
/// of the part with the most pins (`J2.3`), as KiCad names such a net by its pin.
pub(super) fn shown_name(ctx: &Ctx, net: usize) -> String {
    let name = &ctx.design.nets[net].name;
    if is_named(name) {
        return name.clone();
    }
    let part = |refdes: &str| ctx.design.parts.iter().position(|p| p.refdes == refdes);
    let main = ctx.design.nets[net].pins.iter().min_by_key(|pin| {
        let p = part(&pin.refdes);
        (
            std::cmp::Reverse(p.map_or(0, |p| ctx.glyphs[p].pins.len())),
            p.map(|p| ctx.design.parts[p].path.clone()),
            pin.pin.clone(),
        )
    });
    main.map_or_else(|| name.clone(), |pin| format!("{}.{}", pin.refdes, pin.pin))
}

pub(super) fn tag_at(ctx: &Ctx, net: usize, e: Pt, dir: Dir) -> Tag {
    let name = &ctx.design.nets[net].name;
    if ctx.external[net] {
        let shown = is_named(name).then(|| ellipsize(name));
        let port = Port {
            net,
            at: e,
            dir,
            length: port_length(shown.as_deref()),
            label: shown.as_deref().map(|l| port_label(l, e, dir)),
        };
        let mut rect = port.extent();
        if let Some(t) = &port.label {
            rect.union(t.extent_at(Port::TEXT));
        }
        Tag {
            port: Some(port),
            label: None,
            rect,
        }
    } else {
        let text = name_text(&ellipsize(&shown_name(ctx, net)), e, dir);
        // Bold letters run wider than they are measured: keep a little room past the end.
        let mut rect = text.extent();
        let more = 0.15 * text.text.chars().count() as f64 * TEXT_SIZE * CHAR_WIDTH;
        match dir {
            Dir::Right => rect.max.x += more,
            Dir::Left => rect.min.x -= more,
            Dir::Up => rect.min.y -= more,
            Dir::Down => rect.max.y += more,
        }
        Tag {
            port: None,
            label: Some(NetLabel {
                net,
                text,
                at: e,
                dir,
            }),
            rect,
        }
    }
}

/// The box the name or flag at pin `i` of a part will cover, stub included, if the pin ends in
/// one: room to keep when the part is placed. The part may be at the origin or placed.
pub(super) fn tag_rect_of(ctx: &Ctx, part: &PlacedPart, i: usize, tagged: &[bool]) -> Option<Rect> {
    let (pin, g) = (&part.pins[i], &part.glyph.pins[i]);
    let net = pin.net?;
    if g.hidden || !tagged[net] {
        return None;
    }
    let e = pin.at.step(pin.escape, STUB);
    let mut rect = tag_at(ctx, net, e, pin.escape).rect;
    rect.include(pin.at);
    Some(rect)
}

/// The boxes of every name and flag at a part's pins.
pub(super) fn tag_rects(ctx: &Ctx, part: &PlacedPart, tagged: &[bool]) -> Vec<Rect> {
    (0..part.pins.len())
        .filter_map(|i| tag_rect_of(ctx, part, i, tagged))
        .collect()
}

pub(super) fn unit_index(units: &[Unit], part: usize) -> usize {
    units
        .iter()
        .position(|u| u.parts.contains(&part))
        .expect("every part is in a unit")
}
