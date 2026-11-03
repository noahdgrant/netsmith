//! What a board says about its shape and copper beyond its parts and nets (ADR 0020, ADR
//! 0029): rail track widths, the outline and its cutouts, outer pours and keepouts.
//!
//! Everything is read from the checked model by the type of the member, so a module can
//! bring a rail with a width along, and a part can bring a keepout along. Each message names
//! the member and the number involved.

use netsmith_check::{Domain, InstId, InstKind, Member, Model, SourceId};
use netsmith_core::{
    Cutout, Features, Keepout, Label, Outline, PartHeight, PlaceBound, PlaceField, Placement, Pour,
};
use netsmith_lang::Span;

/// A length in mm, rounded to a nanometre of the millimetre so `0.1mm` is `0.1`.
fn mm(metres: f64) -> f64 {
    (metres * 1e12).round() / 1e9
}

/// What a field of an instance holds, if it has the field.
fn domain(model: &Model, inst: InstId, name: &str) -> Option<Domain> {
    match model.member(inst, name)? {
        Member::Field(f) => Some(model.field(f).domain.clone()),
        _ => None,
    }
}

/// A length that must be one value: `None` if the design did not state it.
fn exact(model: &Model, inst: InstId, name: &str, errors: &mut Vec<String>) -> Option<f64> {
    match domain(model, inst, name)? {
        Domain::Quantity(q) if q.is_point() || (q.hi() - q.lo()).abs() < 1e-12 => Some(mm(q.lo())),
        Domain::Quantity(q) => {
            errors.push(format!(
                "`{}.{name}` is {q}, a range, and a position, a size or a height is one value: say it with `&= {}mm`",
                model.path_of(inst),
                mm(q.lo())
            ));
            None
        }
        _ => None,
    }
}

/// A length that may be a range: its bottom and its top, in mm.
fn range(model: &Model, inst: InstId, name: &str) -> Option<(f64, f64)> {
    match domain(model, inst, name)? {
        Domain::Quantity(q) => Some((mm(q.lo()), mm(q.hi()))),
        _ => None,
    }
}

/// `"forbid"` or `"allow"`: whether a keepout forbids what the field names. Unstated is
/// `unstated`: forbidden, except for a keepout that states a height limit, which forbids
/// nothing it does not say.
fn forbids(
    model: &Model,
    inst: InstId,
    name: &str,
    unstated: bool,
    errors: &mut Vec<String>,
) -> bool {
    match domain(model, inst, name) {
        Some(Domain::Text(t)) if t == "forbid" => true,
        Some(Domain::Text(t)) if t == "allow" => false,
        Some(Domain::Text(t)) => {
            errors.push(format!(
                "`{}.{name}` is \"{t}\", which is not \"forbid\" or \"allow\"",
                model.path_of(inst)
            ));
            true
        }
        _ => unstated,
    }
}

/// The copper layers a keepout's `.layers` names, as `top`, `bottom`, `inner1`...
fn layers_of(
    model: &Model,
    inst: InstId,
    copper_layers: u32,
    errors: &mut Vec<String>,
) -> Vec<String> {
    let mut available = vec!["top".to_string()];
    available.extend((1..=copper_layers.saturating_sub(2)).map(|n| format!("inner{n}")));
    available.push("bottom".to_string());
    let Some(Domain::Text(text)) = domain(model, inst, "layers") else {
        return available;
    };
    let mut found = Vec::new();
    for word in text.split(',').map(str::trim) {
        if word == "all" {
            if text.trim() != "all" {
                errors.push(format!(
                    "`{}.layers` is \"{text}\": \"all\" stands alone",
                    model.path_of(inst)
                ));
            }
            return available;
        }
        if available.iter().any(|a| a == word) {
            if !found.iter().any(|f| f == word) {
                found.push(word.to_string());
            }
        } else {
            errors.push(format!(
                "`{}.layers` names \"{word}\", and this {copper_layers}-layer board has {}",
                model.path_of(inst),
                available
                    .iter()
                    .map(|a| format!("\"{a}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    // In board order, whatever order the design listed them in.
    found.sort_by_key(|f| available.iter().position(|a| a == f));
    found
}

/// The width the rails put on each net: the widest bottom of any `min_track` of a rail with
/// a pin on it.
fn net_widths(model: &Model) -> Vec<(String, f64)> {
    let mut widths: Vec<(String, f64)> = Vec::new();
    for (id, inst) in model.instances() {
        if inst.kind != InstKind::Interface {
            continue;
        }
        let Some((lo, _)) = range(model, id, "min_track") else {
            continue;
        };
        let pins: Vec<InstId> = ["pos", "neg"]
            .iter()
            .filter_map(|n| match model.member(id, n) {
                Some(Member::Inst(p)) => Some(p),
                _ => None,
            })
            .collect();
        for net in model.nets() {
            if !net.pins.iter().any(|p| pins.contains(p)) {
                continue;
            }
            match widths.iter_mut().find(|(n, _)| *n == net.name) {
                Some((_, w)) => *w = w.max(lo),
                None => widths.push((net.name.clone(), lo)),
            }
        }
    }
    widths.sort_by(|a, b| a.0.cmp(&b.0));
    widths
}

/// The height limit a keepout states, in mm: one value above zero, or none.
fn max_height_of(model: &Model, inst: InstId, path: &str, errors: &mut Vec<String>) -> Option<f64> {
    let limit = exact(model, inst, "max_height", errors)?;
    if limit <= 0.0 {
        errors.push(format!(
            "`{path}.max_height` is {limit} mm, and a part cannot stand under a limit of nothing: forbid parts there with `.parts &= \"forbid\"` instead"
        ));
        return None;
    }
    Some(limit)
}

/// The heights the design states for its parts (`.body_height`), by path. A part that does
/// not state one is not listed: its height is unknown (ADR 0045, "Height limits").
fn heights(model: &Model, errors: &mut Vec<String>) -> Vec<PartHeight> {
    let mut parts: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part)
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    parts.sort();
    let mut found = Vec::new();
    for (path, id) in parts {
        let Some(height) = exact(model, id, "body_height", errors) else {
            continue;
        };
        if height < 0.0 {
            errors.push(format!(
                "`{path}.body_height` is {height} mm, and a part does not stand below the board"
            ));
            continue;
        }
        found.push(PartHeight { path, height });
    }
    found
}

/// The outer pours of the process: the layers whose pour has a net.
fn pours(model: &Model, nets: &[String], errors: &mut Vec<String>) -> Vec<Pour> {
    let mut found = Vec::new();
    for layer in ["top", "bottom"] {
        let Some(Member::Inst(pour)) = model.member(model.root, "process").and_then(|m| match m {
            Member::Inst(p) => model.member(p, layer),
            _ => None,
        }) else {
            continue;
        };
        let Some(Domain::Text(net)) = domain(model, pour, "net") else {
            continue;
        };
        if !nets.contains(&net) {
            errors.push(format!(
                "`process.{layer}` is a pour on the net `{net}`, but the board has no net of that name (its nets: {})",
                nets.iter()
                    .map(|n| format!("`{n}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            continue;
        }
        let stitch_pitch = range(model, pour, "stitch_pitch").map(|(_, hi)| hi);
        if let Some(pitch) = stitch_pitch
            && pitch <= 0.0
        {
            errors.push(format!(
                "`process.{layer}.stitch_pitch` has a top of {pitch} mm, and the most space between stitching vias must be positive"
            ));
        }
        found.push(Pour {
            layer: layer.to_string(),
            net,
            stitch_pitch,
        });
    }
    found
}

/// The size of a shape: both `.width` and `.height`, stated.
fn size_of(model: &Model, inst: InstId, errors: &mut Vec<String>) -> Option<(f64, f64)> {
    let stated = |name: &str| matches!(domain(model, inst, name), Some(Domain::Quantity(_)));
    let missing: Vec<&str> = ["width", "height"]
        .into_iter()
        .filter(|n| !stated(n))
        .collect();
    if !missing.is_empty() {
        errors.push(format!(
            "`{}` is a `{}` and has no {}: say how big it is, for example `.{} &= 10mm`",
            model.path_of(inst),
            model.instance(inst).type_name,
            missing.join(" or "),
            missing[0]
        ));
        return None;
    }
    let (w, h) = (
        exact(model, inst, "width", errors)?,
        exact(model, inst, "height", errors)?,
    );
    if w <= 0.0 || h <= 0.0 {
        errors.push(format!(
            "`{}` is {w} by {h} mm, and a shape has a size above zero in both",
            model.path_of(inst)
        ));
        return None;
    }
    Some((w, h))
}

/// The position of a shape: both `.x` and `.y`, or neither (the layout file gives it).
fn position_of(model: &Model, inst: InstId, errors: &mut Vec<String>) -> Option<(f64, f64)> {
    let (x, y) = (
        exact(model, inst, "x", errors),
        exact(model, inst, "y", errors),
    );
    match (x, y) {
        (Some(x), Some(y)) => Some((x, y)),
        (None, None) => None,
        _ => {
            errors.push(format!(
                "`{}` states only one of `.x` and `.y`: state both, or neither to take its place from the layout file",
                model.path_of(inst)
            ));
            None
        }
    }
}

/// The corner radius of a shape, 0 if unstated.
fn corner_radius(
    model: &Model,
    inst: InstId,
    size: Option<(f64, f64)>,
    errors: &mut Vec<String>,
) -> f64 {
    let Some(r) = exact(model, inst, "corner_radius", errors) else {
        return 0.0;
    };
    if r < 0.0 {
        errors.push(format!(
            "`{}.corner_radius` is {r} mm, which is below zero",
            model.path_of(inst)
        ));
    }
    if let Some((w, h)) = size
        && r > w.min(h) / 2.0 + 1e-9
    {
        errors.push(format!(
            "`{}.corner_radius` is {r} mm, more than half of the shape's shorter side ({} mm)",
            model.path_of(inst),
            w.min(h)
        ));
    }
    r
}

fn outline_of(model: &Model, inst: InstId, errors: &mut Vec<String>) -> Outline {
    let unbounded = (0.0, f64::INFINITY);
    let width = range(model, inst, "width").unwrap_or(unbounded);
    let height = range(model, inst, "height").unwrap_or(unbounded);
    let tight = (width.0.is_finite() && width.1.is_finite())
        .then_some(())
        .and((height.0.is_finite() && height.1.is_finite()).then_some((width.1, height.1)));
    let corner_radius = corner_radius(model, inst, tight, errors);
    // A rounded corner has to fit in the smallest the outline may be. (When both sizes are
    // limited, `corner_radius` has already compared the radius to the shorter side.)
    for (name, (_, hi)) in [("width", width), ("height", height)] {
        if tight.is_none() && hi < 2.0 * corner_radius - 1e-9 {
            errors.push(format!(
                "`outline.{name}` allows at most {hi} mm and the corner radius {corner_radius} mm needs {} mm",
                2.0 * corner_radius
            ));
        }
    }
    Outline {
        width,
        height,
        corner_radius,
    }
}

/// The `Label` parts of the model: their text and height.
fn labels(model: &Model, least_text: f64, errors: &mut Vec<String>) -> Vec<Label> {
    let mut parts: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part && i.type_name == "Label")
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    parts.sort();
    let mut found = Vec::new();
    for (path, id) in parts {
        let text = match domain(model, id, "text") {
            Some(Domain::Text(t)) if !t.is_empty() => t,
            _ => {
                errors.push(format!(
                    "`{path}` is a `Label` and has no text: say what it prints, for example `.text &= \"REV A\"`"
                ));
                continue;
            }
        };
        let height = range(model, id, "height").map(|(lo, _)| lo);
        if let Some(h) = height
            && h < least_text - 1e-9
        {
            errors.push(format!(
                "`{path}.height` is {h} mm, under the process's least text height of {least_text} mm"
            ));
        }
        found.push(Label { path, text, height });
    }
    found
}

/// Read every board feature of the model. `nets` are the board's net names, `layers` the
/// copper layers of its process (2 when it has none) and `least_text` its smallest text.
pub(crate) fn features_of(
    model: &Model,
    nets: &[String],
    layers: u32,
    least_text: f64,
    errors: &mut Vec<String>,
) -> Features {
    let mut features = Features {
        net_widths: net_widths(model),
        pours: pours(model, nets, errors),
        labels: labels(model, least_text, errors),
        heights: heights(model, errors),
        ..Features::default()
    };

    let mut instances: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Interface)
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    instances.sort();
    for (path, id) in instances {
        let parent = model.instance(id).parent;
        let on_board = parent == Some(model.root);
        let owner = model.owner_of(id);
        match model.instance(id).type_name.as_str() {
            "Outline" => {
                if !(on_board && path == "outline") {
                    errors.push(format!(
                        "`{path}` is an `Outline`, and the board's outline is its member `.outline`"
                    ));
                } else {
                    features.outline = Some(outline_of(model, id, errors));
                }
            }
            "Cutout" => {
                if !on_board {
                    errors.push(format!("`{path}` is a `Cutout`, and only a board cuts holes in itself: make it a member of the board"));
                    continue;
                }
                let size = size_of(model, id, errors);
                let at = position_of(model, id, errors);
                let radius = corner_radius(model, id, size, errors);
                if let Some(size) = size {
                    features.cutouts.push(Cutout {
                        name: path,
                        at,
                        size,
                        corner_radius: radius,
                    });
                }
            }
            "Keepout" => {
                let part = match owner {
                    Some(o) if o == model.root => None,
                    Some(o) if model.instance(o).kind == InstKind::Part => Some(model.path_of(o)),
                    _ => {
                        errors.push(format!("`{path}` is a `Keepout`, and it belongs to a board or to a part, which are the things that have a place"));
                        continue;
                    }
                };
                let size = size_of(model, id, errors);
                let at = position_of(model, id, errors);
                if part.is_some() && at.is_none() && size.is_some() {
                    errors.push(format!(
                        "`{path}` is a keepout of a part and has no `.x` and `.y`: say where it is in the part's own frame"
                    ));
                }
                let layer_list = layers_of(model, id, layers, errors);
                let max_height = max_height_of(model, id, &path, errors);
                if max_height.is_some() && !layer_list.iter().any(|l| l == "top" || l == "bottom") {
                    errors.push(format!(
                        "`{path}` states `.max_height` and its `.layers` name neither \"top\" nor \"bottom\", the faces a part stands on, so it limits nothing"
                    ));
                }
                let unstated = max_height.is_none();
                let flags = [
                    forbids(model, id, "tracks", unstated, errors),
                    forbids(model, id, "vias", unstated, errors),
                    forbids(model, id, "copper", unstated, errors),
                    forbids(model, id, "parts", unstated, errors),
                ];
                if flags == [false; 4] && max_height.is_none() {
                    errors.push(format!(
                        "`{path}` allows tracks, vias, copper and parts, and forbids nothing"
                    ));
                }
                if let Some(size) = size {
                    features.keepouts.push(Keepout {
                        name: path,
                        part,
                        at,
                        size,
                        layers: layer_list,
                        tracks: flags[0],
                        vias: flags[1],
                        copper: flags[2],
                        parts: flags[3],
                        max_height,
                    });
                }
            }
            _ => {}
        }
    }
    features
}

/// Where each requirement of a [`Placement`] is stated: the placement's index in
/// [`Features::constraints`], the name of the field (`x`, `rotation`, `side`...), and the
/// statements that narrow it.
pub type Sites = Vec<(usize, String, Vec<(SourceId, Span)>)>;

/// The requirements the design puts on where its parts and modules go (ADR 0045): the
/// narrowed fields of every `.place`, by the path of the part or module that has it.
pub(crate) fn placements(model: &Model, errors: &mut Vec<String>) -> (Vec<Placement>, Sites) {
    let mut owners: Vec<(String, InstId, InstId)> = model
        .instances()
        .filter(|(id, i)| {
            i.kind == InstKind::Interface
                && i.type_name == "Place"
                && model.instance(*id).parent.is_some()
        })
        .filter_map(|(id, _)| {
            let owner = model.instance(id).parent?;
            matches!(
                model.instance(owner).kind,
                InstKind::Part | InstKind::Module
            )
            .then(|| (model.path_of(owner), owner, id))
        })
        .collect();
    owners.sort();
    let mut found = Vec::new();
    let mut sites: Sites = Vec::new();
    for (path, owner, place) in owners {
        let module = model.instance(owner).kind == InstKind::Module;
        let mut bounds = Vec::new();
        let mut spots: Vec<(String, Vec<(SourceId, Span)>)> = Vec::new();
        for field in PlaceField::ALL {
            let Some(Member::Field(id)) = model.member(place, field.word()) else {
                continue;
            };
            let w = netsmith_solve::window(model, id);
            if !w.open_outline.is_empty() {
                errors.push(format!(
                    "`{path}.place.{}` is measured from the outline's top left corner, and {} does not state one size, so the board has no fixed outline; give `{}` a single value (`&= 30mm`), or state the position without it",
                    field.word(),
                    w.open_outline
                        .iter()
                        .map(|n| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(" and "),
                    w.open_outline.join("` and `"),
                ));
                continue;
            }
            if w.is_open() {
                continue;
            }
            // Lengths are metres in the model and millimetres here.
            let scale = if field == PlaceField::Rotation {
                1.0
            } else {
                1e3
            };
            let mm = |v: f64| {
                if v.is_finite() {
                    (v * scale * 1e9).round() / 1e9
                } else {
                    v
                }
            };
            let (lo, hi) = (mm(w.lo), mm(w.hi));
            if field == PlaceField::Rotation && !has_quarter_turn(lo, hi) {
                errors.push(format!(
                    "`{path}.place.rotation` allows {} to {} degrees, and a part is turned by multiples of 90",
                    show(lo),
                    show(hi)
                ));
            }
            bounds.push(PlaceBound {
                field,
                lo,
                hi,
                source: String::new(),
            });
            spots.push((field.word().to_string(), w.sites));
        }
        let mut side = None;
        if let Some(Member::Field(id)) = model.member(place, "side")
            && let Domain::Text(t) = &model.field(id).domain
        {
            if t != "top" && t != "bottom" {
                errors.push(format!(
                    "`{path}.place.side` is \"{t}\", which is not \"top\" or \"bottom\""
                ));
            }
            side = Some(t.clone());
            spots.push(("side".to_string(), model.field(id).narrowed_at.clone()));
        }
        if bounds.is_empty() && side.is_none() {
            continue;
        }
        let index = found.len();
        found.push(Placement {
            path,
            module,
            bounds,
            side,
            side_source: String::new(),
        });
        sites.extend(spots.into_iter().map(|(f, s)| (index, f, s)));
    }
    (found, sites)
}

fn show(v: f64) -> String {
    if v.is_finite() {
        format!("{v}")
    } else if v < 0.0 {
        "no least".to_string()
    } else {
        "no most".to_string()
    }
}

/// Whether the range holds a multiple of 90 degrees (turned by any number of whole turns).
fn has_quarter_turn(lo: f64, hi: f64) -> bool {
    if !lo.is_finite() || !hi.is_finite() || hi - lo >= 360.0 {
        return true;
    }
    let first = (lo / 90.0 - 1e-9).ceil() * 90.0;
    first <= hi + 1e-9
}
