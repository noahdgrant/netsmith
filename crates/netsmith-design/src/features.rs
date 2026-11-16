//! What a board says about its shape and copper beyond its parts and nets (ADR 0020, ADR
//! 0029): rail track widths, the outline and its cutouts, outer pours and keepouts.
//!
//! Everything is read from the checked model by the type of the member, so a module can
//! bring a rail with a width along, and a part can bring a keepout along. Each message names
//! the member and the number involved.

use netsmith_check::{Domain, InstId, InstKind, Member, Model, SourceId};
use netsmith_core::{
    CaseModel, Cutout, Features, ImpedanceTarget, Keepout, Label, Logo, LogoLayer, Offset,
    OffsetEnd, Outline, PartHeight, PartKind, PartSilk, PartStandoff, PinProtrusion, PlaceBound,
    PlaceField, Placement, Pour, SilkPrint, StepModel,
};
use netsmith_lang::Span;
use std::collections::{BTreeMap, BTreeSet};

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
            // A range with an open end names the end it has.
            let end = if q.lo().is_finite() { q.lo() } else { q.hi() };
            errors.push(format!(
                "`{}.{name}` is {q}, a range, and a position, a size or a height is one value: say it with `&= {}mm`",
                model.path_of(inst),
                mm(end)
            ));
            None
        }
        _ => None,
    }
}

/// A length that may be a range: its bottom and its top, in mm. An end that is open
/// (`1mm..`, `..5mm`) is infinite: a caller that needs a number there asks `is_finite`.
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

/// Every copper layer of a board, as a keepout names them: `top`, `inner1`... and `bottom`.
fn copper_layer_names(copper_layers: u32) -> Vec<String> {
    let mut names = vec!["top".to_string()];
    names.extend((1..=copper_layers.saturating_sub(2)).map(|n| format!("inner{n}")));
    names.push("bottom".to_string());
    names
}

/// The copper layers a keepout's `.layers` names, as `top`, `bottom`, `inner1`...
fn layers_of(
    model: &Model,
    inst: InstId,
    copper_layers: u32,
    errors: &mut Vec<String>,
) -> Vec<String> {
    let available = copper_layer_names(copper_layers);
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
    by_net_of_rails(model, "min_track")
}

/// The clearance the rails put on each net: the largest bottom of any `min_clearance` of a
/// rail with a pin on it (issue #758).
fn net_clearances(model: &Model) -> Vec<(String, f64)> {
    by_net_of_rails(model, "min_clearance")
}

/// The largest bottom of the range `field` of the rails with a pin on each net, by net name,
/// sorted by name: both pins of a rail get it, and a net takes the largest of its rails.
fn by_net_of_rails(model: &Model, field: &str) -> Vec<(String, f64)> {
    let mut widths: Vec<(String, f64)> = Vec::new();
    for (id, inst) in model.instances() {
        if inst.kind != InstKind::Interface {
            continue;
        }
        let Some((lo, _)) = range(model, id, field).filter(|(lo, _)| lo.is_finite()) else {
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

/// A quantity that must be one value, in its base unit (ohms for a resistance, metres for a
/// length): `None` if the design did not state it.
fn point(model: &Model, inst: InstId, name: &str, errors: &mut Vec<String>) -> Option<f64> {
    match domain(model, inst, name)? {
        Domain::Quantity(q) if q.is_point() || (q.hi() - q.lo()).abs() < 1e-12 => Some(q.lo()),
        Domain::Quantity(q) => {
            errors.push(format!(
                "`{}.{name}` is {q}, a range, and a target impedance is one value",
                model.path_of(inst)
            ));
            None
        }
        _ => None,
    }
}

/// The impedance the design states for the tracks of nets (issue #1007): each `TrackImpedance`
/// and `PairImpedance`, with the nets its pins are on, in the order of their paths.
fn impedance_targets(model: &Model, errors: &mut Vec<String>) -> Vec<ImpedanceTarget> {
    let mut found = Vec::new();
    let mut members: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| {
            i.kind == InstKind::Interface
                && (i.type_name == "TrackImpedance" || i.type_name == "PairImpedance")
        })
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    members.sort();
    for (path, id) in members {
        let pair = model.instance(id).type_name == "PairImpedance";
        let pins: &[&str] = if pair { &["p", "n"] } else { &["line"] };
        let mut nets = Vec::new();
        for pin in pins {
            let on = match model.member(id, pin) {
                Some(Member::Inst(p)) => model
                    .nets()
                    .iter()
                    .find(|n| n.pins.contains(&p))
                    .map(|n| n.name.clone()),
                _ => None,
            };
            match on {
                Some(net) => nets.push(net),
                None => errors.push(format!(
                    "`{path}.{pin}` is on no net: join it with `~` to the pin or the net the tracks are on"
                )),
            }
        }
        let ohms = point(model, id, "impedance", errors);
        if ohms.is_none()
            && domain(model, id, "impedance").is_none_or(|d| matches!(d, Domain::Unset))
        {
            errors.push(format!(
                "`{path}.impedance` is not stated: say it with `&= 50ohm`"
            ));
        }
        let spacing = if pair {
            let s = point(model, id, "spacing", errors).map(mm);
            if s.is_none()
                && domain(model, id, "spacing").is_none_or(|d| matches!(d, Domain::Unset))
            {
                errors.push(format!(
                    "`{path}.spacing` is not stated: say the space between the two tracks, edge to edge, with `&= 0.15mm`"
                ));
            }
            s
        } else {
            None
        };
        let (Some(ohms), true) = (
            ohms,
            nets.len() == pins.len() && (!pair || spacing.is_some()),
        ) else {
            continue;
        };
        if ohms <= 0.0 {
            errors.push(format!(
                "`{path}.impedance` is {ohms} ohm: a target is above 0"
            ));
            continue;
        }
        if let Some(s) = spacing
            && s <= 0.0
        {
            errors.push(format!(
                "`{path}.spacing` is {s} mm: a pair's tracks are apart"
            ));
            continue;
        }
        if pair && nets[0] == nets[1] {
            errors.push(format!(
                "`{path}` joins both of its pins to the net `{}`: a pair is two nets",
                nets[0]
            ));
            continue;
        }
        found.push(ImpedanceTarget {
            path,
            nets,
            ohms,
            spacing,
        });
    }
    found
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

/// The heights the design states for its parts (`.height`), by path. A part that does
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
        netsmith_core::job::checkpoint();
        let Some(height) = exact(model, id, "height", errors) else {
            continue;
        };
        if height < 0.0 {
            errors.push(format!(
                "`{path}.height` is {height} mm, and a part does not stand below the board"
            ));
            continue;
        }
        found.push(PartHeight { path, height });
    }
    found
}

/// The standoffs the design states for its parts (`.standoff`, issue #775), by path: how high
/// the underside of the body stands above the board. A part that states none stands on the
/// board and is not listed. A negative standoff is an error, as is one that is not below the
/// part's own stated height: the body would have no thickness.
fn standoffs(model: &Model, errors: &mut Vec<String>) -> Vec<PartStandoff> {
    let mut parts: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part)
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    parts.sort();
    let mut found = Vec::new();
    for (path, id) in parts {
        netsmith_core::job::checkpoint();
        let Some(standoff) = exact(model, id, "standoff", errors) else {
            continue;
        };
        if standoff < 0.0 {
            errors.push(format!(
                "`{path}.standoff` is {standoff} mm, and a body does not stand below the board"
            ));
            continue;
        }
        // The height is read quietly: `heights` has said what is wrong with it.
        if let Some(Domain::Quantity(q)) = domain(model, id, "height")
            && (q.hi() - q.lo()).abs() < 1e-12
            && standoff >= mm(q.lo()) - 1e-9
        {
            errors.push(format!(
                "`{path}.standoff` is {standoff} mm and `{path}.height` is {} mm: the body stands from its standoff up to its height, so the height is the top of the body, above the standoff",
                mm(q.lo())
            ));
            continue;
        }
        found.push(PartStandoff { path, standoff });
    }
    found
}

/// The package of a hole: the one it states (`.package`), or, for a hole that states a
/// `.diameter`, the package that names that drill (`netsmith_core::drilled_package`), which the
/// mapping answers with a generated footprint. A hole that states both is an error: the stock
/// package already says the drill. A diameter that is a range or not above nothing is an error.
pub(crate) fn hole_package(
    model: &Model,
    inst: InstId,
    stated: Option<String>,
    errors: &mut Vec<String>,
) -> Option<String> {
    if !matches!(
        model.instance(inst).marks.declared_kind,
        Some(PartKind::MountingHole | PartKind::PlatedHole)
    ) {
        return stated;
    }
    let path = model.path_of(inst);
    let Some(diameter) = exact(model, inst, "diameter", errors) else {
        return stated;
    };
    if diameter <= 0.0 {
        errors.push(format!(
            "`{path}.diameter` is {diameter} mm, and a hole is wider than nothing: state the drill, for example `&= 3.2mm`"
        ));
        return stated;
    }
    match stated {
        Some(package) => {
            errors.push(format!(
                "`{path}` states `.package` \"{package}\" and `.diameter` {diameter} mm: the package already says the drill, so state one of them"
            ));
            Some(package)
        }
        None => Some(netsmith_core::drilled_package(diameter)),
    }
}

/// The room each mounting hole that states a `.head_diameter` keeps for its screw head
/// (ADR 0045, C3): a keepout the hole carries, centred on the hole, that forbids tracks, vias,
/// copper and parts on every copper layer (`netsmith_core::HEAD_ZONE` names it). It is the
/// square that holds the head's circle: the geometry of every check is rectangular, and a
/// square is never smaller than the circle. A head that is not one positive value is an error.
fn heads(model: &Model, copper_layers: u32, errors: &mut Vec<String>) -> Vec<Keepout> {
    let mut holes: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part && i.type_name == "MountingHole")
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    holes.sort();
    let mut found = Vec::new();
    for (path, id) in holes {
        netsmith_core::job::checkpoint();
        let Some(head) = exact(model, id, "head_diameter", errors) else {
            continue;
        };
        if head <= 0.0 {
            errors.push(format!(
                "`{path}.head_diameter` is {head} mm, and a screw head is wider than nothing: leave it out for a hole with no head"
            ));
            continue;
        }
        found.push(Keepout {
            name: format!("{}{path}", netsmith_core::HEAD_ZONE),
            part: Some(path),
            at: Some((-head / 2.0, -head / 2.0)),
            size: (head, head),
            layers: copper_layer_names(copper_layers),
            tracks: true,
            vias: true,
            copper: true,
            parts: true,
            max_height: None,
        });
    }
    found
}

/// What a field of text holds when it is one value (`"hide"`), if the design states it.
fn text(model: &Model, inst: InstId, name: &str) -> Option<String> {
    match domain(model, inst, name)? {
        Domain::Text(t) => Some(t),
        _ => None,
    }
}

/// What the silkscreen prints for the parts that do not print their designator, by path
/// (`.silk`, `.silk_text`, stated in the physical file). A part prints a text of its own
/// (`.silk_text`) when it states one; otherwise it takes the first `.silk` that says "show" or
/// "hide" of these, the most specific first: its own (a statement, or the default of its
/// kind), the module it is in, the module that module is in and so on, and the board. A part
/// that none of them says anything of prints its designator and is not listed.
fn silks(model: &Model, errors: &mut Vec<String>) -> Vec<PartSilk> {
    // A `.silk` that is neither "show" nor "hide" is an error wherever it is stated.
    let mut stated: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| matches!(i.kind, InstKind::Part | InstKind::Module | InstKind::Board))
        .map(|(id, i)| {
            let path = model.path_of(id);
            let name = if i.kind == InstKind::Board {
                "the board".to_string()
            } else {
                format!("`{path}`")
            };
            (name, id)
        })
        .collect();
    stated.sort();
    for (name, id) in &stated {
        if let Some(t) = text(model, *id, "silk")
            && t != "show"
            && t != "hide"
        {
            errors.push(format!(
                "{name} has `.silk` \"{t}\", which is not \"show\" or \"hide\": say whether the silkscreen prints designators"
            ));
        }
    }
    let mut parts: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part)
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    parts.sort();
    let mut found = Vec::new();
    for (path, id) in parts {
        netsmith_core::job::checkpoint();
        if let Some(t) = text(model, id, "silk_text") {
            if t.trim().is_empty() {
                errors.push(format!(
                    "`{path}.silk_text` is empty: give the text the silkscreen prints, or hide the designator with `.{path}.silk &= \"hide\"`"
                ));
                continue;
            }
            if text(model, id, "silk").as_deref() == Some("hide") {
                errors.push(format!(
                    "`{path}` hides its silkscreen text with `.silk` and gives it the text \"{t}\" with `.silk_text`: keep one of the two"
                ));
                continue;
            }
            found.push(PartSilk {
                path,
                print: SilkPrint::Text(t),
            });
            continue;
        }
        let mut cur = Some(id);
        while let Some(at) = cur {
            match text(model, at, "silk").as_deref() {
                Some("hide") => {
                    found.push(PartSilk {
                        path,
                        print: SilkPrint::Nothing,
                    });
                    break;
                }
                Some("show") => break,
                _ => cur = model.instance(at).parent,
            }
        }
    }
    found
}

/// How far the pins of the design's parts protrude (`.pin_protrusion`, ADR 0045 section 15),
/// by path, and the statements that state each. A protrusion is one value: a part whose
/// protrusion is only bounded (its kind says its pins protrude at most so far, and the board
/// does not say) has an unknown protrusion and is not listed. A negative one is an error.
pub(crate) fn protrusions(
    model: &Model,
    errors: &mut Vec<String>,
) -> (Vec<PinProtrusion>, Vec<Vec<(SourceId, Span)>>) {
    let mut parts: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part)
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    parts.sort();
    let (mut found, mut sites) = (Vec::new(), Vec::new());
    for (path, id) in parts {
        netsmith_core::job::checkpoint();
        let Some(Member::Field(field)) = model.member(id, "pin_protrusion") else {
            continue;
        };
        let w = netsmith_solve::window(model, field);
        if w.is_open() || !w.lo.is_finite() || !w.hi.is_finite() || (w.hi - w.lo).abs() >= 1e-12 {
            continue;
        }
        let length = mm(w.lo);
        if length < 0.0 {
            errors.push(format!(
                "`{path}.pin_protrusion` is {length} mm, and a pin does not protrude less than nothing"
            ));
            continue;
        }
        found.push(PinProtrusion {
            path,
            length,
            source: String::new(),
        });
        sites.push(w.sites);
    }
    (found, sites)
}

/// The STEP models the design's parts are drawn from (`.model`, ADR 0108), by path, and the
/// statements that state each file. The offset and the rotation are each one value, zero when
/// unstated. A part that states an offset or a rotation and no file is an error, as is an
/// empty file name: there is nothing to place.
pub(crate) fn models(
    model: &Model,
    errors: &mut Vec<String>,
) -> (Vec<StepModel>, Vec<Vec<(SourceId, Span)>>) {
    let mut parts: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part)
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    parts.sort();
    let (mut found, mut sites) = (Vec::new(), Vec::new());
    for (path, id) in parts {
        netsmith_core::job::checkpoint();
        let Some(Member::Inst(m)) = model.member(id, "model") else {
            continue;
        };
        let Some(Member::Field(file_field)) = model.member(m, "file") else {
            continue;
        };
        const PLACEMENT: [&str; 6] = ["x", "y", "z", "rx", "ry", "rz"];
        let file = match &model.field(file_field).domain {
            Domain::Text(file) => file.clone(),
            _ => {
                let stated: Vec<String> = PLACEMENT
                    .iter()
                    .filter(|n| !matches!(domain(model, m, n), None | Some(Domain::Unset)))
                    .map(|n| format!("`{path}.model.{n}`"))
                    .collect();
                if !stated.is_empty() {
                    errors.push(format!(
                        "{} {} stated, and `{path}.model.file` is not, so there is no STEP model to place: name the file with `.{path}.model.file &= \"models/part.step\"`, or remove the placement",
                        stated.join(", "),
                        if stated.len() == 1 { "is" } else { "are" }
                    ));
                }
                continue;
            }
        };
        // An empty file says the part has no model: it is drawn with its generated shape,
        // and the project's model table gives it none (issue #763).
        if file.trim().is_empty() {
            let stated: Vec<String> = PLACEMENT
                .iter()
                .filter(|n| !matches!(domain(model, m, n), None | Some(Domain::Unset)))
                .map(|n| format!("`{path}.model.{n}`"))
                .collect();
            if !stated.is_empty() {
                errors.push(format!(
                    "`{path}.model.file` is empty, which says `{path}` has no model, and {} {} stated, so there is nothing to place: name the STEP file, or remove the placement",
                    stated.join(", "),
                    if stated.len() == 1 { "is" } else { "are" }
                ));
                continue;
            }
        }
        let (offset, rotation) = if file.trim().is_empty() {
            ([0.0; 3], [0.0; 3])
        } else {
            placement_of(model, m, &format!("{path}.model"), errors)
        };
        found.push(StepModel {
            path,
            file: if file.trim().is_empty() {
                String::new()
            } else {
                file
            },
            offset,
            rotation,
            source: String::new(),
            stated_in: Vec::new(),
            resolved: None,
            footprint: None,
        });
        sites.push(model.field(file_field).narrowed_at.clone());
    }
    (found, sites)
}

/// The offset in mm (`.x`, `.y`, `.z`) and the rotation in degrees (`.rx`, `.ry`, `.rz`) of a
/// `StepModel` or a `CaseModel` (ADR 0108, ADR 0109): each one value, zero when unstated.
/// `path` names the instance in a message.
fn placement_of(
    model: &Model,
    inst: InstId,
    path: &str,
    errors: &mut Vec<String>,
) -> ([f64; 3], [f64; 3]) {
    let mut offset = [0.0; 3];
    for (slot, name) in offset.iter_mut().zip(["x", "y", "z"]) {
        *slot = exact(model, inst, name, errors).unwrap_or(0.0);
    }
    let mut rotation = [0.0; 3];
    for (slot, name) in rotation.iter_mut().zip(["rx", "ry", "rz"]) {
        *slot = match domain(model, inst, name) {
            Some(Domain::Quantity(q)) if (q.hi() - q.lo()).abs() < 1e-12 => q.lo(),
            Some(Domain::Quantity(q)) => {
                errors.push(format!(
                    "`{path}.{name}` is {q}, a range, and a rotation is one value in degrees: say it with `&= {}`",
                    q.lo()
                ));
                0.0
            }
            _ => 0.0,
        };
    }
    (offset, rotation)
}

/// The STEP files of the board's case (`CaseModel` members of the `case { }` section, ADR
/// 0109), by name, and the statements that state each file. A case model is a member of the
/// board; one that names no file, or an empty one, is an error, as there is nothing to place.
pub(crate) fn case_models(
    model: &Model,
    errors: &mut Vec<String>,
) -> (Vec<CaseModel>, Vec<Vec<(SourceId, Span)>>) {
    let mut cases: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Interface && i.type_name == "CaseModel")
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    cases.sort();
    let (mut found, mut sites) = (Vec::new(), Vec::new());
    for (path, id) in cases {
        netsmith_core::job::checkpoint();
        if model.instance(id).parent != Some(model.root) {
            errors.push(format!(
                "`{path}` is a `CaseModel`, and a case is the board's: declare it in the `case {{ }}` section of the board's physical file"
            ));
            continue;
        }
        let Some(Member::Field(file_field)) = model.member(id, "file") else {
            continue;
        };
        let file = match &model.field(file_field).domain {
            Domain::Text(file) if !file.trim().is_empty() => file.clone(),
            Domain::Text(_) => {
                errors.push(format!(
                    "`{path}.file` is empty: name the case's STEP file, relative to the physical file that states it"
                ));
                continue;
            }
            _ => {
                errors.push(format!(
                    "`{path}` is a `CaseModel` and names no file, so there is no case to place: name it with `.file &= \"case/box.step\"`, relative to the physical file"
                ));
                continue;
            }
        };
        let (offset, rotation) = placement_of(model, id, &path, errors);
        let mut figures = netsmith_core::CaseFigures::default();
        for (slot, name) in [
            (&mut figures.clearance, "clearance"),
            (&mut figures.reach, "reach"),
            (&mut figures.boss_gap, "boss_gap"),
        ] {
            match exact(model, id, name, errors) {
                Some(v) if v < 0.0 => errors.push(format!(
                    "`{path}.{name}` is {v}mm, and a gap is not negative: state it with `&= 0mm` or more"
                )),
                Some(v) => *slot = v,
                None => {}
            }
        }
        let checked = match domain(model, id, "check") {
            Some(Domain::Text(t)) if t == "fit" => true,
            Some(Domain::Text(t)) if t == "none" => false,
            Some(Domain::Text(t)) => {
                errors.push(format!(
                    "`{path}.check` is \"{t}\", which is not \"fit\" or \"none\": \"fit\" measures the board against the model, \"none\" only draws it"
                ));
                true
            }
            _ => true,
        };
        found.push(CaseModel {
            name: path,
            file,
            offset,
            rotation,
            figures,
            checked,
            source: String::new(),
            stated_in: Vec::new(),
            resolved: None,
        });
        sites.push(model.field(file_field).narrowed_at.clone());
    }
    (found, sites)
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
        // The most space between stitching vias; with no upper limit (`5mm..`) it is no limit.
        let stitch_pitch = range(model, pour, "stitch_pitch")
            .map(|(_, hi)| hi)
            .filter(|hi| hi.is_finite());
        if let Some(pitch) = stitch_pitch
            && pitch <= 0.0
        {
            errors.push(format!(
                "`process.{layer}.stitch_pitch` has a top of {pitch} mm, and the most space between stitching vias must be positive"
            ));
        }
        // The gap and the spokes of a thermal relief: the least the process allows, the bottom
        // of the range; with no lower limit (`..1mm`) it is not stated.
        let mut least = |name: &str, what: &str| {
            let low = range(model, pour, name)
                .map(|(lo, _)| lo)
                .filter(|lo| lo.is_finite());
            if let Some(v) = low
                && v <= 0.0
            {
                errors.push(format!(
                    "`process.{layer}.{name}` has a bottom of {v} mm, and the {what} of a thermal relief must be positive"
                ));
            }
            low
        };
        let thermal_gap = least("thermal_gap", "gap");
        let spoke_width = least("spoke_width", "spoke width");
        found.push(Pour {
            layer: layer.to_string(),
            net,
            stitch_pitch,
            thermal_gap,
            spoke_width,
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
    if let Some(polygon) = model.polygon_of(inst) {
        return polygon_outline(model, inst, polygon, errors);
    }
    // A size has no negative values, so `..100mm` is from nothing up to 100 mm.
    let size = |name: &str| {
        range(model, inst, name)
            .map(|(lo, hi)| (lo.max(0.0), hi))
            .unwrap_or(unbounded)
    };
    let (width, height) = (size("width"), size("height"));
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
        polygon: None,
    }
}

/// An outline stated as a `polygon` (issue #57): its corners with the fillets made arcs, and
/// its box as the width and height. The polygon says the size and the corners, so `.width`,
/// `.height` and `.corner_radius` stated beside it are refused, each naming the polygon.
fn polygon_outline(
    model: &Model,
    inst: InstId,
    polygon: &netsmith_check::OutlinePolygon,
    errors: &mut Vec<String>,
) -> Outline {
    let path = model.path_of(inst);
    for name in ["width", "height", "corner_radius"] {
        if matches!(domain(model, inst, name), Some(Domain::Quantity(_))) {
            errors.push(format!(
                "`{path}.{name}` is stated beside the `polygon` of `{path}`: the polygon is the whole edge, its corners give the size and the roundings, so leave `.{name}` out"
            ));
        }
    }
    let vertices = match crate::polygon::vertices(&path, &polygon.corners) {
        Ok(v) => v,
        Err(found) => {
            errors.extend(found);
            Vec::new()
        }
    };
    let (w, h) = if vertices.is_empty() {
        (0.0, 0.0)
    } else {
        crate::polygon::size(&vertices)
    };
    Outline {
        width: (w, w),
        height: (h, h),
        corner_radius: 0.0,
        polygon: (!vertices.is_empty()).then_some(vertices),
    }
}

/// The `Label` parts of the model: their text and text height.
fn labels(model: &Model, least_text: f64, errors: &mut Vec<String>) -> Vec<Label> {
    let mut parts: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part && i.type_name == "Label")
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    parts.sort();
    let mut found = Vec::new();
    for (path, id) in parts {
        netsmith_core::job::checkpoint();
        let text = match domain(model, id, "text") {
            Some(Domain::Text(t)) if !t.is_empty() => t,
            _ => {
                errors.push(format!(
                    "`{path}` is a `Label` and has no text: say what it prints, for example `.text &= \"REV A\"`"
                ));
                continue;
            }
        };
        let height = range(model, id, "text_height")
            .map(|(lo, _)| lo)
            .filter(|lo| lo.is_finite());
        if let Some(h) = height
            && h < least_text - 1e-9
        {
            errors.push(format!(
                "`{path}.text_height` is {h} mm, under the process's least text height of {least_text} mm"
            ));
        }
        found.push(Label {
            path,
            text,
            text_height: height,
        });
    }
    found
}

/// The `Logo` parts of the model: their file, size and layer, and the lines that state the
/// file (in the order of the logos, for the sources the caller names). `least_text` is the
/// process's smallest text, the least size on the silkscreen, and `least_track` its narrowest
/// track, the least size in copper.
pub(crate) fn logos(
    model: &Model,
    least_text: f64,
    least_track: f64,
    errors: &mut Vec<String>,
) -> (Vec<Logo>, Vec<Vec<(SourceId, Span)>>) {
    let mut parts: Vec<(String, InstId)> = model
        .instances()
        .filter(|(_, i)| i.kind == InstKind::Part && i.type_name == "Logo")
        .map(|(id, _)| (model.path_of(id), id))
        .collect();
    parts.sort();
    let (mut found, mut sites) = (Vec::new(), Vec::new());
    for (path, id) in parts {
        netsmith_core::job::checkpoint();
        let Some(Member::Field(file_field)) = model.member(id, "file") else {
            continue;
        };
        let file = match &model.field(file_field).domain {
            Domain::Text(file) if !file.trim().is_empty() => file.clone(),
            Domain::Text(_) => {
                errors.push(format!(
                    "`{path}.file` is empty: name the logo's SVG or DXF file, relative to the file that states it"
                ));
                continue;
            }
            _ => {
                errors.push(format!(
                    "`{path}` is a `Logo` and names no file, so there is nothing to print: name it with `.file &= \"logo.svg\"`, relative to the file that states it"
                ));
                continue;
            }
        };
        let layer = match domain(model, id, "layer") {
            Some(Domain::Text(t)) => match LogoLayer::from_word(&t) {
                Some(layer) => layer,
                None => {
                    errors.push(format!(
                        "`{path}.layer` is \"{t}\", which is not \"silkscreen\" or \"copper\": say where the logo is drawn"
                    ));
                    continue;
                }
            },
            _ => LogoLayer::Silkscreen,
        };
        let (least, what) = match layer {
            LogoLayer::Silkscreen => (least_text, "text height"),
            LogoLayer::Copper => (least_track, "track"),
        };
        let size = match range(model, id, "size").map(|(lo, _)| lo) {
            Some(lo) if lo.is_finite() => lo,
            _ => {
                errors.push(format!(
                    "`{path}` is a `Logo` and states no size: say how long the longer side of the drawing is, for example `.size &= 10mm`"
                ));
                continue;
            }
        };
        if size < least - 1e-9 {
            errors.push(format!(
                "`{path}.size` is {size} mm, under the process's least {what} of {least} mm, which is the smallest {} that is legible",
                match layer {
                    LogoLayer::Silkscreen => "mark on the silkscreen",
                    LogoLayer::Copper => "mark in copper",
                }
            ));
        }
        found.push(Logo {
            path,
            file,
            size,
            layer,
            source: String::new(),
            stated_in: Vec::new(),
            outlines: Vec::new(),
        });
        sites.push(model.field(file_field).narrowed_at.clone());
    }
    (found, sites)
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
        net_clearances: net_clearances(model),
        impedance_targets: impedance_targets(model, errors),
        pours: pours(model, nets, errors),
        labels: labels(model, least_text, errors),
        heights: heights(model, errors),
        standoffs: standoffs(model, errors),
        silks: silks(model, errors),
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
                if matches!(domain(model, id, "parts"), Some(Domain::Text(t)) if t == "forbid")
                    && !layer_list.iter().any(|l| l == "top" || l == "bottom")
                {
                    errors.push(format!(
                        "`{path}.parts` is \"forbid\" and its `.layers` name neither \"top\" nor \"bottom\", the faces a part stands on, so it keeps no part out"
                    ));
                }
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
    features.keepouts.extend(heads(model, layers, errors));
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
            bounds.push(PlaceBound {
                field,
                lo,
                hi,
                source: String::new(),
            });
            spots.push((field.word().to_string(), w.sites));
        }
        let mut side = None;
        // A module's face is stated on each of its parts (ADR 0048): read it there.
        if !module
            && let Some(Member::Field(id)) = model.member(place, "side")
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

/// The paths of the parts a bank stands for (ADR 0052): `path[0]`, `path[1]`, ... for a bank
/// of two or more, written as the members of `N x Kind` are (ADR 0119). A part that is not a
/// bank, or a bank of one, is just `path`.
fn members(path: &str, banks: &BTreeMap<String, usize>) -> Vec<String> {
    let n = banks.get(path).copied().unwrap_or(1);
    (0..n)
        .map(|k| crate::bank_member_path(path, k, n))
        .collect()
}

/// Make every requirement the design states on the path of a bank hold for each capacitor of
/// it (ADR 0052). The design stage makes one part of the design for each, with a path of its own
/// (`stage.output_cap[1]`), and a requirement that named only the first would leave the others
/// to sit anywhere with nothing said. So a placement (`.place`), a height and a protrusion
/// become one for each capacitor, and a relation between two places becomes one for each pair
/// of capacitors of its two ends. The sites that say where each is stated are copied with it.
/// A note says what was done, once for each statement.
///
/// The choice is every capacitor, not the first (a choice of an agent, for the owner to
/// change). A distance from "the output capacitor" to the inductor is a statement about the
/// bank as a whole, and a bank with only its first capacitor near the inductor would meet the
/// sentence in the file and not its purpose. The cost is that a bank of eight with a tight
/// window may be unplaceable where the first alone was (not tried: the placer reports an unmet
/// requirement for each part it concerns). Rejected: a note that the later capacitors are not
/// covered, which would leave the defect in place with a warning.
pub(crate) fn expand_banks(
    features: &mut Features,
    place_sites: &mut Sites,
    offset_sites: &mut Vec<(SourceId, Span)>,
    protrusion_sites: &mut Vec<Vec<(SourceId, Span)>>,
    model_sites: &mut Vec<Vec<(SourceId, Span)>>,
    banks: &BTreeMap<String, usize>,
    notes: &mut Vec<String>,
) {
    if banks.is_empty() {
        return;
    }
    let mut said: BTreeSet<String> = BTreeSet::new();
    let mut say = |notes: &mut Vec<String>, what: String| {
        if said.insert(what.clone()) {
            notes.push(what);
        }
    };

    let old = std::mem::take(&mut features.constraints);
    let old_sites = std::mem::take(place_sites);
    for (i, p) in old.into_iter().enumerate() {
        let own: Vec<_> = old_sites.iter().filter(|s| s.0 == i).collect();
        let paths = if p.module {
            vec![p.path.clone()]
        } else {
            members(&p.path, banks)
        };
        if paths.len() > 1 {
            say(
                notes,
                format!(
                    "`{}` is a bank of {} capacitors: what `{}.place` requires holds for each of them (`{}`)",
                    p.path,
                    paths.len(),
                    p.path,
                    paths.join("`, `")
                ),
            );
        }
        for path in paths {
            let index = features.constraints.len();
            place_sites.extend(own.iter().map(|(_, f, s)| (index, f.clone(), s.clone())));
            features.constraints.push(Placement { path, ..p.clone() });
        }
    }

    let (old, old_sites) = (
        std::mem::take(&mut features.offsets),
        std::mem::take(offset_sites),
    );
    for (o, site) in old.into_iter().zip(old_sites) {
        let (a_paths, b_paths) = (members(&o.a.path, banks), members(&o.b.path, banks));
        if a_paths.len() * b_paths.len() > 1 {
            say(
                notes,
                format!(
                    "`{}.place.{}` and `{}.place.{}` are related, and {} a bank (ADR 0052): the relation holds for each pair of capacitors ({} pairs)",
                    o.b.path,
                    o.b.field.word(),
                    o.a.path,
                    o.a.field.word(),
                    match (a_paths.len() > 1, b_paths.len() > 1) {
                        (true, true) => "both are",
                        (true, false) => "`a` is",
                        _ => "`b` is",
                    },
                    a_paths.len() * b_paths.len()
                ),
            );
        }
        for b in &b_paths {
            for a in &a_paths {
                features.offsets.push(Offset {
                    a: OffsetEnd {
                        path: a.clone(),
                        ..o.a.clone()
                    },
                    b: OffsetEnd {
                        path: b.clone(),
                        ..o.b.clone()
                    },
                    ..o.clone()
                });
                offset_sites.push(site);
            }
        }
    }

    let old = std::mem::take(&mut features.heights);
    for h in old {
        for path in members(&h.path, banks) {
            features.heights.push(PartHeight { path, ..h.clone() });
        }
    }
    let old = std::mem::take(&mut features.standoffs);
    for s in old {
        for path in members(&s.path, banks) {
            features.standoffs.push(PartStandoff { path, ..s.clone() });
        }
    }
    let old = std::mem::take(&mut features.silks);
    for s in old {
        for path in members(&s.path, banks) {
            features.silks.push(PartSilk { path, ..s.clone() });
        }
    }
    let (old, old_sites) = (
        std::mem::take(&mut features.protrusions),
        std::mem::take(protrusion_sites),
    );
    for (p, site) in old.into_iter().zip(old_sites) {
        for path in members(&p.path, banks) {
            features
                .protrusions
                .push(PinProtrusion { path, ..p.clone() });
            protrusion_sites.push(site.clone());
        }
    }
    let (old, old_sites) = (
        std::mem::take(&mut features.models),
        std::mem::take(model_sites),
    );
    for (m, site) in old.into_iter().zip(old_sites) {
        for path in members(&m.path, banks) {
            features.models.push(StepModel { path, ..m.clone() });
            model_sites.push(site.clone());
        }
    }
}

/// The relations between the places of two parts (ADR 0045, section 3), and where the design
/// states each: `.b.place.x - .a.place.x == 17.5mm +/- 0.05mm` is an offset of `b` from `a`
/// along x. Which relations those are is [`netsmith_solve::offsets`]'s; this puts the parts
/// by path and the window in millimetres. A window no position can meet is an error.
pub(crate) fn offsets(
    model: &Model,
    errors: &mut Vec<String>,
) -> (Vec<Offset>, Vec<(SourceId, Span)>) {
    let mut found = Vec::new();
    let mut sites = Vec::new();
    for o in netsmith_solve::offsets(model) {
        let end = |f: netsmith_check::FieldId| {
            let place = model.field(f).owner;
            let part = model.instance(place).parent?;
            Some(OffsetEnd {
                path: model.path_of(part),
                field: PlaceField::from_word(&model.field(f).name)?,
            })
        };
        let (Some(a), Some(b)) = (end(o.a), end(o.b)) else {
            continue;
        };
        let to_mm = |v: f64| if v.is_finite() { mm(v) } else { v };
        let (lo, hi) = (to_mm(o.lo), to_mm(o.hi));
        let r = &model.relations()[o.relation];
        if lo > hi + 1e-9 {
            errors.push(format!(
                "`{}.place.{}` minus `{}.place.{}` cannot be held: `{} {} {}` leaves it at least {} mm and at most {} mm",
                b.path,
                b.field.word(),
                a.path,
                a.field.word(),
                model.show_expr(&r.lhs),
                r.op.symbol(),
                model.show_expr(&r.rhs),
                show(lo),
                show(hi)
            ));
            continue;
        }
        found.push(Offset {
            a,
            b,
            lo,
            hi,
            source: String::new(),
        });
        sites.push((r.source, r.span));
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

/// The `near` guidelines of the parts' `layout` blocks (ADR 0030, issue #829), by the path of
/// the part, with its pins named as its pin table names them (`vin.pos`), sorted by part and
/// name.
pub(crate) fn layout(model: &Model) -> Vec<netsmith_core::LayoutNear> {
    let mut out: Vec<netsmith_core::LayoutNear> = model
        .layout
        .iter()
        .map(|l| {
            let pin = |p| model.path_below(p, l.part);
            netsmith_core::LayoutNear {
                part: model.path_of(l.part),
                name: l.name.clone(),
                source: l.source.clone(),
                site: l.site.clone(),
                select: match l.select {
                    netsmith_check::NearSelect::Bridging(a, b) => {
                        netsmith_core::NearSelect::Bridging {
                            a: pin(a),
                            b: pin(b),
                        }
                    }
                    netsmith_check::NearSelect::On { pin: p, kind } => {
                        netsmith_core::NearSelect::On { pin: pin(p), kind }
                    }
                },
                to: pin(l.to),
                within: l.within,
            }
        })
        .collect();
    out.sort_by(|a, b| (&a.part, &a.name).cmp(&(&b.part, &b.name)));
    out
}
