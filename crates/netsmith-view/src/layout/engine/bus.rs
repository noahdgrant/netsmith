//! Interfaces as buses (GLOSSARY: Bus): the members of one interface instance that run
//! between the same two main parts of a sheet, or leave the sheet from one, are drawn as short
//! entries from their pins that meet one thick line, which runs to the other part or ends in
//! one flag.
//!
//! [`plan`] decides which nets are a bus before anything is placed, from the interface
//! instances the design carries ([`netsmith_core::Design::interfaces`]) and from where the
//! pins are, never from net names. A net is a candidate when it is not a rail, has a name that
//! someone chose, and has exactly one pin on each of two or more main parts (or, when it goes on
//! to another sheet, on one or more), and its pins on a part are all on one side. Per interface
//! instance (with the members of the interfaces nested in it, under dotted names), nets with the
//! same parts, the same flag and the same sides are one bus when they number [`MIN_MEMBERS`] or
//! more between two places, or [`MIN_MEMBERS_SHARED`] among three or more (parts, and the flag);
//! a net is in the bus of the instance that has most of them (then the shortest path, then the
//! path), so a module's port and the part's pins of the same interface are drawn once. The drawing
//! is then a pure function of the plan: the rows of the pins give the order of the entries, the
//! spine stands in the first column past everything else at that side of the part, the trunk
//! is routed on the grid like any wire, and each part past the second runs to the line already
//! drawn. A pin whose kind the part's pin table gives has an arrowhead on its entry. When a bus
//! cannot be drawn its nets are drawn as they were before (a flag or a name each, or a wire).
//! ADR 0049 and ADR 0067.

use super::*;

/// A bus has at least this many members on the sheet.
pub(super) const MIN_MEMBERS: usize = 3;
/// A bus that is shared by three or more places (parts, or two parts and the flag: an I2C bus
/// with several devices) has at least this many.
const MIN_MEMBERS_SHARED: usize = 2;
/// A bus has at most this many parts at its ends.
const MAX_ENDS: usize = 8;

/// How many members make a bus of `ends` places (the parts, and the flag when the members go
/// on to another sheet).
fn min_members(ends: usize) -> usize {
    if ends >= 3 {
        MIN_MEMBERS_SHARED
    } else {
        MIN_MEMBERS
    }
}
/// The spine is at least this many cells from the pins (two beyond the end of a stub, so that
/// it never touches the name at the stub of a pin between the members), and at most
/// [`SPINE_MAX`].
const SPINE_MIN: i32 = 4;
const SPINE_MAX: i32 = 48;
/// Room kept past what is written at a part's pins for the entries and the spine.
const SPINE_ROOM: f64 = 7.62;
/// The width of the tick and the count before the text of a bus.
const COUNT_ROOM: f64 = 3.6;
/// Letters of a bus's name are bold: wider than they are measured.
const WIDE: f64 = 1.18;
/// The list of members is cut at this many letters.
const LIST_CHARS: usize = 44;

/// One end of a bus: a main part and the pins of the members on it, in the order of the
/// side (top to bottom, or left to right).
pub(super) struct EndPlan {
    pub part: usize,
    pub side: Dir,
    /// `(index of the pin in the placed part, net)`.
    pub pins: Vec<(usize, usize)>,
}

pub(super) struct BusPlan {
    /// The path of the interface instance that names the bus.
    pub path: String,
    pub type_name: String,
    /// The instance's own name, the last segment of its path.
    pub name: String,
    /// The members, in the order of the first end's pins.
    pub nets: Vec<usize>,
    /// What the instance calls each member, in the same order.
    pub names: Vec<String>,
    /// The parts at its ends: two or more, or one for a bus that goes on to another sheet.
    pub ends: Vec<EndPlan>,
    /// Whether the members go on to another sheet: the thick line then ends in a flag.
    pub flag: bool,
    /// The interface at the other end: what it is called and what it calls the members. A bus
    /// of two ends has it when that differs from the first; a bus that goes on to another sheet
    /// always has it when the other sheet's instance is known.
    pub other: Option<OtherEnd>,
}

/// The interface instance at the other end of a bus, as that end names it.
pub(super) struct OtherEnd {
    /// The instance's path (`modem.uart`).
    pub path: String,
    /// What it calls the members, in the order of the bus's members.
    pub names: Vec<String>,
}

fn list_of(names: &[String]) -> String {
    let text = format!("{{{}}}", names.join(", "));
    if text.chars().count() <= LIST_CHARS {
        return text;
    }
    let head: String = text.chars().take(LIST_CHARS - 2).collect();
    format!("{head}…}}")
}

impl BusPlan {
    pub fn list(&self) -> String {
        list_of(&self.names)
    }

    pub fn other_text(&self) -> Option<String> {
        self.other.as_ref().map(|o| {
            if self.goes_on() {
                format!("\u{2192} {}: {}", o.path, list_of(&o.names))
            } else {
                format!("{}: {}", leaf(&o.path), list_of(&o.names))
            }
        })
    }

    /// How wide the first line of texts is, in millimetres.
    fn text_width(&self) -> f64 {
        let w = |t: &str| crate::layout::text_width(t);
        let mut width = (w(&self.name) * WIDE).max(w(&self.list()));
        if let Some(o) = self.other_text() {
            width = width.max(w(&o));
        }
        width
    }

    fn goes_on(&self) -> bool {
        self.flag
    }
}

/// Which way the signal goes at the pin `pad` of `part`, from the pin kind the part's pin
/// table gives it (the language says it there and not in the interface, ADR 0016): an output
/// gives, an input takes, a bidirectional pin does both. Any other kind, and a part that has
/// no pin table, say nothing.
fn member_flow(part: &netsmith_core::Part, pad: &str) -> Option<MemberFlow> {
    let pin = part.pins.iter().find(|q| q.pads.iter().any(|x| x == pad))?;
    match pin.kind {
        netsmith_core::PinKind::Output => Some(MemberFlow::Gives),
        netsmith_core::PinKind::Input => Some(MemberFlow::Takes),
        netsmith_core::PinKind::Bidirectional => Some(MemberFlow::Both),
        _ => None,
    }
}

/// The members of each interface instance, with those of the interfaces nested in it: an
/// instance that sits inside another listed instance (`mcu.link.data` inside `mcu.link`) gives
/// its members to it under its relative path (`data.sclk`), so one interface is one bus
/// whatever it is made of. A nested `Rail` is power and is not a member. Each is
/// `(name, net)`, the instance's own members first, in the order the design has them.
fn flat_members(design: &Design) -> Vec<Vec<(String, String)>> {
    let index: BTreeMap<&str, usize> = design
        .interfaces
        .iter()
        .enumerate()
        .map(|(i, inst)| (inst.path.as_str(), i))
        .collect();
    let mut flat: Vec<Vec<(String, String)>> = design
        .interfaces
        .iter()
        .map(|inst| {
            inst.members
                .iter()
                .map(|m| (m.name.clone(), m.net.clone()))
                .collect()
        })
        .collect();
    for inst in design.interfaces.iter() {
        if inst.type_name == "Rail" {
            continue;
        }
        let mut cut = inst.path.len();
        while let Some(at) = inst.path[..cut].rfind('.') {
            cut = at;
            if let Some(&i) = index.get(&inst.path[..at]) {
                let relative = &inst.path[at + 1..];
                for m in &inst.members {
                    flat[i].push((format!("{relative}.{}", m.name), m.net.clone()));
                }
            }
        }
    }
    flat
}

/// Whether the instance at `path` belongs to a part (or a module) that has a part on this
/// sheet: a part's own interface (`sensor1.spi`, part `sensor1.u`) or a module's port.
fn on_this_sheet(design: &Design, path: &str) -> bool {
    let owner = path.rsplit_once('.').map_or("", |(owner, _)| owner);
    design.parts.iter().any(|p| {
        !owner.is_empty() && p.path.starts_with(owner) && p.path[owner.len()..].starts_with('.')
    })
}

fn leaf(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

fn common_segments(a: &str, b: &str) -> usize {
    a.split('.')
        .zip(b.split('.'))
        .take_while(|(x, y)| x == y)
        .count()
}

/// How far a pin is along its side: down the page for a pin on the left or right, across it
/// for one on the top or bottom.
fn along_of(side: Dir, at: Pt) -> f64 {
    if side.is_vertical() { at.x } else { at.y }
}

/// The buses of a sheet, by the path of their interface, from the parts as they are placed
/// (at their own origins). `is_anchor` says which parts are main parts.
pub(super) fn plan(
    ctx: &Ctx,
    placed: &[PlacedPart],
    is_anchor: &dyn Fn(usize) -> bool,
) -> Vec<BusPlan> {
    let design = ctx.design;
    if design.interfaces.is_empty() {
        return Vec::new();
    }
    let net_ix: HashMap<&str, usize> = design
        .nets
        .iter()
        .enumerate()
        .map(|(i, n)| (n.name.as_str(), i))
        .collect();
    let part_ix: HashMap<&str, usize> = design
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| (p.refdes.as_str(), i))
        .collect();

    struct Elig {
        net: usize,
        /// The pin of each end, `(part, pin index, side)`, in the order of the parts' paths.
        ends: Vec<(usize, usize, Dir)>,
    }
    struct Cand {
        iface: usize,
        parts: Vec<usize>,
        flag: bool,
        members: Vec<Elig>,
    }
    let flat = flat_members(design);
    let mut cands: Vec<Cand> = Vec::new();
    for (ii, own) in flat.iter().enumerate() {
        netsmith_core::job::checkpoint();
        let mut buckets: BTreeMap<(Vec<usize>, bool), Vec<Elig>> = BTreeMap::new();
        for (_, member_net) in own {
            let Some(&net) = net_ix.get(member_net.as_str()) else {
                continue;
            };
            if ctx.power[net].is_some() || ctx.ground[net] || !is_named(&design.nets[net].name) {
                continue;
            }
            let mut ends = Vec::new();
            let mut ok = true;
            for pin in &design.nets[net].pins {
                let p = part_ix[pin.refdes.as_str()];
                match placed[p].pins.iter().position(|q| q.id == pin.pin) {
                    Some(i) if !ctx.glyphs[p].pins[i].hidden => {
                        ends.push((p, i, placed[p].pins[i].escape));
                    }
                    _ => ok = false,
                }
            }
            // Two or more parts on the sheet, or one when the net goes on to another sheet: a
            // net on more than two is a shared line (an I2C bus with several devices).
            let external = ctx.external[net];
            let at_least = if external { 1 } else { 2 };
            let mut parts: Vec<usize> = ends.iter().map(|e| e.0).collect();
            parts.sort_unstable();
            parts.dedup();
            if !ok
                || ends.len() < at_least
                || ends.len() > MAX_ENDS
                || parts.len() != ends.len()
                || !parts.iter().all(|&p| is_anchor(p))
            {
                continue;
            }
            ends.sort_by_key(|e| (design.parts[e.0].path.clone(), e.0));
            let key: Vec<usize> = ends.iter().map(|e| e.0).collect();
            buckets
                .entry((key, external))
                .or_default()
                .push(Elig { net, ends });
        }
        for ((parts, flag), members) in buckets {
            // The nets that have their pins on the same sides: the biggest such set.
            let mut by_sides: BTreeMap<Vec<u8>, Vec<Elig>> = BTreeMap::new();
            for e in members {
                let key: Vec<u8> = e.ends.iter().map(|x| x.2 as u8).collect();
                by_sides.entry(key).or_default().push(e);
            }
            let Some(best) = by_sides
                .into_iter()
                .max_by(|(ka, a), (kb, b)| a.len().cmp(&b.len()).then_with(|| kb.cmp(ka)))
                .map(|(_, v)| v)
            else {
                continue;
            };
            if best.len() >= min_members(parts.len() + usize::from(flag)) {
                cands.push(Cand {
                    iface: ii,
                    parts,
                    flag,
                    members: best,
                });
            }
        }
    }
    let depth = |c: &Cand| design.interfaces[c.iface].path.split('.').count();
    // An instance in the module of one of the parts names the bus before one of another module.
    let local = |c: &Cand| {
        c.parts
            .iter()
            .any(|&p| common_segments(&design.interfaces[c.iface].path, &design.parts[p].path) > 0)
    };
    cands.sort_by(|a, b| {
        b.members
            .len()
            .cmp(&a.members.len())
            .then_with(|| local(b).cmp(&local(a)))
            .then_with(|| depth(a).cmp(&depth(b)))
            .then_with(|| {
                design.interfaces[a.iface]
                    .path
                    .cmp(&design.interfaces[b.iface].path)
            })
    });

    let mut taken = vec![false; design.nets.len()];
    let mut out: Vec<BusPlan> = Vec::new();
    for c in cands {
        let members: Vec<Elig> = c.members.into_iter().filter(|e| !taken[e.net]).collect();
        if members.len() < min_members(c.parts.len() + usize::from(c.flag)) {
            continue;
        }
        let inst = &design.interfaces[c.iface];
        // The first end is the part that is in the instance's own module, else the first by
        // path; the others follow in the order of their paths.
        let mut order: Vec<usize> = (0..c.parts.len()).collect();
        order.sort_by_key(|&o| {
            std::cmp::Reverse(common_segments(&inst.path, &design.parts[c.parts[o]].path))
        });
        let pin_of = |e: &Elig, part: usize| {
            *e.ends
                .iter()
                .find(|x| x.0 == part)
                .expect("a net has a pin at each end")
        };
        let part_a = c.parts[order[0]];
        let side_a = pin_of(&members[0], part_a).2;
        let mut sorted: Vec<&Elig> = members.iter().collect();
        sorted.sort_by(|x, y| {
            let key = |e: &Elig| {
                let (p, i, _) = pin_of(e, part_a);
                along_of(side_a, placed[p].pins[i].at)
            };
            key(x).total_cmp(&key(y))
        });
        let nets: Vec<usize> = sorted.iter().map(|e| e.net).collect();
        let name_in = |i: usize, net: usize| {
            flat[i]
                .iter()
                .find(|(_, n)| net_ix.get(n.as_str()) == Some(&net))
                .map_or_else(String::new, |(name, _)| name.clone())
        };
        let names: Vec<String> = nets.iter().map(|&n| name_in(c.iface, n)).collect();
        let mut ends = Vec::new();
        for &o in &order {
            let part = c.parts[o];
            let side = pin_of(&members[0], part).2;
            let mut pins: Vec<(usize, usize)> =
                members.iter().map(|e| (pin_of(e, part).1, e.net)).collect();
            pins.sort_by(|a, b| {
                along_of(side, placed[part].pins[a.0].at)
                    .total_cmp(&along_of(side, placed[part].pins[b.0].at))
            });
            ends.push(EndPlan { part, side, pins });
        }
        // What the other end calls it: another instance, from another module, with most of
        // the same nets. Across a sheet's edge it is the instance on the sheet the bus goes to.
        let first_segment = inst.path.split('.').next().unwrap_or("");
        let other = design
            .interfaces
            .iter()
            .enumerate()
            .filter(|(_, j)| j.path != inst.path && j.path.split('.').next() != Some(first_segment))
            // Across a sheet's edge: not an instance of a part that is on this sheet.
            .filter(|(_, j)| !c.flag || !on_this_sheet(design, &j.path))
            .map(|(jx, j)| {
                let n = nets.iter().filter(|&&x| !name_in(jx, x).is_empty()).count();
                (n, jx, j)
            })
            .filter(|&(n, _, _)| n >= MIN_MEMBERS.min(nets.len()))
            .max_by(|(na, _, a), (nb, _, b)| {
                na.cmp(nb)
                    .then_with(|| b.path.split('.').count().cmp(&a.path.split('.').count()))
                    .then_with(|| b.path.cmp(&a.path))
            })
            .and_then(|(_, jx, j)| {
                let theirs: Vec<String> = nets.iter().map(|&n| name_in(jx, n)).collect();
                (c.flag || leaf(&j.path) != leaf(&inst.path) || theirs != names).then(|| OtherEnd {
                    path: j.path.clone(),
                    names: theirs,
                })
            });
        for &n in &nets {
            taken[n] = true;
        }
        out.push(BusPlan {
            path: inst.path.clone(),
            type_name: inst.type_name.clone(),
            name: leaf(&inst.path).to_string(),
            nets,
            names,
            ends,
            flag: c.flag,
            other,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// The room each bus needs at its parts, in the parts' own coordinates, so that a part is
/// placed with space for the entries, the spine and the writing at the end of its pins: from
/// the pins out past anything written at that side of the part.
pub(super) fn rooms(
    ctx: &Ctx,
    placed: &[PlacedPart],
    plans: &[BusPlan],
    tagged: &[bool],
) -> BTreeMap<usize, Vec<Rect>> {
    let mut out: BTreeMap<usize, Vec<Rect>> = BTreeMap::new();
    for plan in plans {
        let width = plan.text_width();
        let flag = port_length(Some(&plan.name));
        for (e, end) in plan.ends.iter().enumerate() {
            let a = &placed[end.part];
            let side = end.side;
            let tips: Vec<Pt> = end.pins.iter().map(|&(i, _)| a.pins[i].at).collect();
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            let mut base = if side.vector().0 + side.vector().1 > 0.0 {
                f64::MIN
            } else {
                f64::MAX
            };
            let out_of = |p: Pt| if side.is_vertical() { p.y } else { p.x };
            let outward = side.vector().0 + side.vector().1;
            for &t in &tips {
                lo = lo.min(along_of(side, t));
                hi = hi.max(along_of(side, t));
                base = if outward > 0.0 {
                    base.max(out_of(t))
                } else {
                    base.min(out_of(t))
                };
            }
            // What is written at the other pins at this side reaches this far out.
            let mut far = base;
            for i in 0..a.pins.len() {
                if a.pins[i].escape != side {
                    continue;
                }
                if let Some(r) = tag_rect_of(ctx, a, i, tagged) {
                    let edge = match side {
                        Dir::Right => r.max.x,
                        Dir::Left => r.min.x,
                        Dir::Down => r.max.y,
                        Dir::Up => r.min.y,
                    };
                    far = if outward > 0.0 {
                        far.max(edge)
                    } else {
                        far.min(edge)
                    };
                }
            }
            let extra = if e > 0 {
                0.0
            } else if plan.goes_on() {
                COUNT_ROOM + width + flag + 2.54
            } else {
                COUNT_ROOM + width
            };
            let reach = far + outward * (SPINE_ROOM + extra);
            let (a0, a1) = (lo - 1.9, hi + 1.9);
            let rect = if side.is_vertical() {
                Rect {
                    min: Pt::new(a0, base.min(reach)),
                    max: Pt::new(a1, base.max(reach)),
                }
            } else {
                Rect {
                    min: Pt::new(base.min(reach), a0),
                    max: Pt::new(base.max(reach), a1),
                }
            };
            out.entry(end.part).or_default().push(rect);
        }
    }
    out
}

/// Where an end of a bus is on the grid.
struct EndGeom {
    side: Dir,
    /// `(part, pin index, net, node of the pin)`.
    pins: Vec<(usize, usize, usize, Node)>,
    /// The first and last place along the spine.
    span: (i32, i32),
    /// The row (for a spine that runs across) or column of the spine.
    spine: i32,
}

impl EndGeom {
    fn sgn(&self) -> i32 {
        let (dx, dy) = self.side.vector();
        (dx + dy) as i32
    }

    fn along(&self, n: Node) -> i32 {
        if self.side.is_vertical() { n.0 } else { n.1 }
    }

    fn across(&self, n: Node) -> i32 {
        if self.side.is_vertical() { n.1 } else { n.0 }
    }

    /// The node of the spine at `along`.
    fn at(&self, along: i32) -> Node {
        if self.side.is_vertical() {
            (along, self.spine)
        } else {
            (self.spine, along)
        }
    }

    /// The node one step out of the spine at `along`.
    fn out(&self, along: i32) -> Node {
        let n = self.at(along);
        let (dx, dy) = self.side.vector();
        (n.0 + dx as i32, n.1 + dy as i32)
    }
}

/// What the writing of a bus is and where.
struct Writing {
    label: Text,
    list: Text,
    other: Option<Text>,
    count: Text,
    tick: (Pt, Pt),
    /// What the writing covers.
    rects: Vec<Rect>,
}

impl<'a> Scene<'a> {
    /// Draws every planned bus; the nets of one that cannot be drawn are drawn as before.
    pub(super) fn plan_buses(&mut self) {
        let plans = std::mem::take(&mut self.bus_plans);
        for (k, plan) in plans.iter().enumerate() {
            let saved = self.grid.clone();
            match self.try_bus(k, plan) {
                Some(bus) => {
                    for t in bus.texts() {
                        let _ = t;
                    }
                    self.buses.push(bus);
                }
                None => {
                    self.grid = saved;
                    for &net in &plan.nets {
                        self.bus_skip[net] = false;
                        self.tagged[net] = self.retag[net];
                    }
                    let pins: Vec<(usize, usize, usize, Node, Dir)> = self
                        .pins
                        .iter()
                        .filter(|p| plan.nets.contains(&p.net))
                        .map(|p| (p.part, p.idx, p.net, p.node, p.escape))
                        .collect();
                    for (part, idx, net, node, escape) in pins {
                        self.stub_pin(part, idx, net, node, escape);
                    }
                    self.plan_tags_of(plan.nets.clone());
                }
            }
        }
        self.bus_plans = plans;
    }

    fn end_geom(&self, end: &EndPlan) -> Option<EndGeom> {
        let mut pins = Vec::new();
        for &(i, net) in &end.pins {
            let node = self.grid.node(self.parts[end.part].pins[i].at);
            pins.push((end.part, i, net, node));
        }
        let mut g = EndGeom {
            side: end.side,
            pins,
            span: (0, 0),
            spine: 0,
        };
        let alongs: Vec<i32> = g.pins.iter().map(|p| g.along(p.3)).collect();
        g.span = (*alongs.iter().min()?, *alongs.iter().max()?);
        let sgn = g.sgn();
        let base = g.pins.iter().map(|p| g.across(p.3)).fold(
            if sgn > 0 { i32::MIN } else { i32::MAX },
            |m, v| {
                if sgn > 0 { m.max(v) } else { m.min(v) }
            },
        );
        let id = self.bus_id(0);
        let _ = id;
        for kk in SPINE_MIN..=SPINE_MAX {
            g.spine = base + sgn * kk;
            if self.spine_is_free(&g) {
                return Some(g);
            }
        }
        None
    }

    fn bus_id(&self, k: usize) -> u32 {
        (self.ctx.design.nets.len() + k) as u32
    }

    /// Whether the spine and the entries of `g` run over nodes with nothing on them.
    fn spine_is_free(&self, g: &EndGeom) -> bool {
        let step_along = if g.side.is_vertical() {
            Dir::Right
        } else {
            Dir::Down
        };
        for a in g.span.0..=g.span.1 {
            let n = g.at(a);
            if !self.grid.is_free(n)
                || (a > g.span.0 && self.grid.step_is_cut(g.at(a - 1), step_along))
            {
                return false;
            }
        }
        let sgn = g.sgn();
        for p in &g.pins {
            let mut c = g.across(p.3);
            let mut prev = p.3;
            while c != g.spine {
                c += sgn;
                let n = if g.side.is_vertical() {
                    (g.along(p.3), c)
                } else {
                    (c, g.along(p.3))
                };
                if !self.grid.is_free(n) || self.grid.step_is_cut(prev, g.side) {
                    return false;
                }
                prev = n;
            }
        }
        true
    }

    fn try_bus(&mut self, k: usize, plan: &BusPlan) -> Option<Bus> {
        let ctx = self.ctx;
        let id = self.bus_id(k);
        let mut ends: Vec<EndGeom> = Vec::new();
        for end in &plan.ends {
            let g = self.end_geom(end)?;
            // Take the spine now so that the other end does not use the same nodes.
            for a in g.span.0..=g.span.1 {
                let n = g.at(a);
                self.grid.claim_wire(n, n, id);
            }
            ends.push(g);
        }
        for g in &ends {
            for a in g.span.0..g.span.1 {
                self.grid.claim_wire(g.at(a), g.at(a + 1), id);
            }
        }
        // Where the thick line leaves each spine: level with the first end when it can (the
        // first end: with the second).
        let mid = |g: &EndGeom| (g.span.0 + g.span.1) / 2;
        let attach: Vec<i32> = (0..ends.len())
            .map(|e| {
                let g = &ends[e];
                match ends.get(if e == 0 { 1 } else { 0 }) {
                    Some(o) if o.side.is_vertical() == g.side.is_vertical() => {
                        let lo = g.span.0.max(o.span.0);
                        let hi = g.span.1.min(o.span.1);
                        if lo <= hi {
                            ((lo + hi) / 2).clamp(g.span.0, g.span.1)
                        } else {
                            mid(o).clamp(g.span.0, g.span.1)
                        }
                    }
                    _ => mid(g),
                }
            })
            .collect();

        let mut lines: Vec<Vec<Pt>> = Vec::new();
        for g in &ends {
            lines.push(vec![
                self.grid.pt(g.at(g.span.0)),
                self.grid.pt(g.at(g.span.1)),
            ]);
        }
        let a0 = ends[0].at(attach[0]);
        // The thick line beyond the spines, as runs of nodes: from the first end out to the
        // flag when the members go on, between the first two ends when they do not, and from
        // each further end to what is drawn of it so far (a bus shared by several parts is one
        // line with a branch to each).
        let mut trunks: Vec<Vec<Node>> = Vec::new();
        let mut network: HashSet<Node> = HashSet::new();
        let mut flag: Option<BusFlag> = None;
        let mut texts_run: Vec<(Pt, Pt)> = Vec::new();
        if plan.goes_on() {
            let g = &ends[0];
            let (dx, dy) = g.side.vector();
            let width = plan.text_width() + COUNT_ROOM;
            let min_cells = ((width / GRID).ceil() as i32).max(4);
            let fl = port_length(Some(&plan.name));
            let mut found: Option<Vec<Node>> = None;
            'len: for cells in min_cells..=min_cells + 20 {
                let mut nodes = vec![a0];
                for j in 1..=cells {
                    let n = (a0.0 + dx as i32 * j, a0.1 + dy as i32 * j);
                    if !self.grid.free_for(n, id) || self.grid.step_is_cut(*nodes.last()?, g.side) {
                        break 'len;
                    }
                    nodes.push(n);
                }
                let tip = self.grid.pt(*nodes.last()?);
                let probe = BusFlag {
                    at: tip,
                    dir: g.side,
                    length: fl,
                    label: port_label(&plan.name, tip, g.side),
                };
                if self.rect_is_free_for(inflate(probe.extent(), 0.3), id as usize) {
                    found = Some(nodes);
                    break;
                }
            }
            let trunk = found?;
            let tip = self.grid.pt(*trunk.last()?);
            self.grid.claim_wire(a0, *trunk.last()?, id);
            flag = Some(BusFlag {
                at: tip,
                dir: g.side,
                length: fl,
                label: port_label(&plan.name, tip, g.side),
            });
            texts_run.push((self.grid.pt(a0), tip));
            network.extend(trunk.iter().copied().skip(1));
            trunks.push(trunk);
        } else {
            let b0 = ends[1].at(attach[1]);
            let start = ends[0].out(attach[0]);
            let goal = ends[1].out(attach[1]);
            if !self.grid.free_for(start, id) || !self.grid.free_for(goal, id) {
                return None;
            }
            let targets: HashSet<Node> = std::iter::once(goal).collect();
            let path = self.grid.route(id, start, ends[0].side, &targets)?;
            self.grid.commit(&path, id);
            let mut all = vec![a0];
            all.extend(path.iter().copied());
            all.push(b0);
            self.grid.claim_wire(a0, start, id);
            self.grid.claim_wire(goal, b0, id);
            network.extend(path.iter().copied());
            let trunk = grid::corners(&all);
            for w in trunk.windows(2) {
                texts_run.push((self.grid.pt(w[0]), self.grid.pt(w[1])));
            }
            trunks.push(trunk);
        }
        // The further ends: each runs to the line, at the nearest place it can.
        let joiners = if plan.goes_on() { 1 } else { 2 };
        for e in joiners..ends.len() {
            netsmith_core::job::checkpoint();
            let (ae, oe) = (ends[e].at(attach[e]), ends[e].out(attach[e]));
            if !self.grid.free_for(oe, id) {
                return None;
            }
            let path = self.grid.route(id, oe, ends[e].side, &network)?;
            self.grid.commit(&path, id);
            self.grid.claim_wire(ae, oe, id);
            network.extend(path.iter().copied());
            let mut all = vec![ae];
            all.extend(path.iter().copied());
            let trunk = grid::corners(&all);
            for w in trunk.windows(2) {
                texts_run.push((self.grid.pt(w[0]), self.grid.pt(w[1])));
            }
            trunks.push(trunk);
        }
        for trunk in &trunks {
            lines.push(
                grid::corners(trunk)
                    .iter()
                    .map(|&n| self.grid.pt(n))
                    .collect(),
            );
        }

        let writing = self.place_writing(plan, &texts_run, id)?;
        for r in &writing.rects {
            self.grid.block_rect(inflate(*r, 0.6));
        }
        if let Some(f) = &flag {
            self.grid.block_rect(inflate(f.extent(), TAG_CLEAR));
            self.grid.unblock(self.grid.node(f.at));
        }

        // The entries: from each pin, straight to the spine.
        let mut entries = Vec::new();
        for (e, g) in ends.iter().enumerate() {
            for p in &g.pins {
                let net = p.2;
                let at = if g.side.is_vertical() {
                    (g.along(p.3), g.spine)
                } else {
                    (g.spine, g.along(p.3))
                };
                self.grid.unblock(p.3);
                self.grid.claim_wire(p.3, at, Self::net_id(net));
                self.push_wire(net, &[p.3, at]);
                let (from, to) = (self.grid.pt(p.3), self.grid.pt(at));
                entries.push(BusEntry {
                    net,
                    end: e,
                    at: to,
                    flow: member_flow(&ctx.design.parts[p.0], &self.parts[p.0].pins[p.1].id),
                    out: g.side,
                    length: (to.x - from.x).abs() + (to.y - from.y).abs(),
                });
            }
        }
        for r in &writing.rects {
            self.tag_boxes.push((plan.ends[0].part, *r));
        }
        for net in &plan.nets {
            self.bus_done[*net] = true;
        }
        let _ = ctx;
        Some(Bus {
            path: plan.path.clone(),
            type_name: plan.type_name.clone(),
            name: plan.name.clone(),
            members: plan
                .nets
                .iter()
                .zip(&plan.names)
                .map(|(&net, name)| BusMember {
                    net,
                    name: name.clone(),
                })
                .collect(),
            lines,
            entries,
            ends: plan.ends.iter().map(|e| e.part).collect(),
            flag,
            label: writing.label,
            list: writing.list,
            other_path: writing
                .other
                .is_some()
                .then(|| plan.other.as_ref().map(|o| o.path.clone()))
                .flatten(),
            other: writing.other,
            tick: writing.tick,
            count: writing.count,
        })
    }

    /// Writes the name, the members and the count beside the longest run of the thick line
    /// that has room: the name above and the members below, or both on one side. `None` when
    /// no run has room.
    fn place_writing(&self, plan: &BusPlan, runs: &[(Pt, Pt)], id: u32) -> Option<Writing> {
        let list = plan.list();
        let other = plan.other_text();
        let (wn, wl) = (
            crate::layout::text_width(&plan.name) * WIDE,
            crate::layout::text_width(&list) * 1.08,
        );
        let wo = other
            .as_ref()
            .map_or(0.0, |o| crate::layout::text_width(o) * 1.08);
        let wide = wn.max(wl);
        let mut order: Vec<&(Pt, Pt)> = runs.iter().collect();
        order.sort_by(|a, b| {
            let len = |r: &(Pt, Pt)| (r.0.x - r.1.x).abs() + (r.0.y - r.1.y).abs();
            len(b).total_cmp(&len(a))
        });
        let count = plan.nets.len().to_string();
        for &&(p, q) in &order {
            let vertical = (p.x - q.x).abs() < 1e-6;
            let (lo, hi) = if vertical {
                (p.y.min(q.y), p.y.max(q.y))
            } else {
                (p.x.min(q.x), p.x.max(q.x))
            };
            let fixed = if vertical { p.x } else { p.y };
            if hi - lo < COUNT_ROOM + wide.min(6.0) {
                continue;
            }
            // Along the run, from the middle outward in grid steps.
            let middle = (lo + hi) / 2.0;
            let mut shifts: Vec<f64> = vec![0.0];
            for s in 1..=((hi - lo) / GRID).ceil() as i32 {
                shifts.push(f64::from(s) * GRID);
                shifts.push(-f64::from(s) * GRID);
            }
            // The three ways to set the lines about the thick line, near ones first: name
            // above and members below, both above, both below.
            let ways: [(f64, f64, f64); 3] =
                [(-1.0, 2.5, -1.0), (-3.4, -1.0, -3.4), (2.5, 4.9, 2.5)];
            // With the other end's name first, and only when it has no room anywhere on the
            // run without it.
            for with_other in [true, false] {
                if with_other && other.is_none() {
                    continue;
                }
                for &shift in &shifts {
                    let start = if vertical {
                        snap(middle + shift + (wide - COUNT_ROOM) / 2.0, GRID)
                    } else {
                        snap(
                            middle + shift - (wide + COUNT_ROOM) / 2.0 + COUNT_ROOM,
                            GRID,
                        )
                    };
                    for &(name_off, list_off, count_off) in &ways {
                        let w = self.writing_at(
                            plan,
                            &list,
                            other.as_deref().filter(|_| with_other),
                            &count,
                            (wn, wl, wo),
                            vertical,
                            (fixed, start),
                            (name_off, list_off, count_off),
                        );
                        if w.rects
                            .iter()
                            .all(|r| self.rect_is_free_for(inflate(*r, 0.3), id as usize))
                        {
                            return Some(w);
                        }
                    }
                }
            }
        }
        None
    }

    /// The writing with its text starting `start` along a run that is `fixed` across.
    #[allow(clippy::too_many_arguments)]
    fn writing_at(
        &self,
        plan: &BusPlan,
        list: &str,
        other: Option<&str>,
        count: &str,
        (wn, wl, wo): (f64, f64, f64),
        vertical: bool,
        (fixed, start): (f64, f64),
        (name_off, list_off, count_off): (f64, f64, f64),
    ) -> Writing {
        // `along` runs down the page for a vertical run, and the text is turned.
        let place = |text: &str, across: f64, along_at: f64| -> Text {
            if vertical {
                // Written upward: it starts at the bottom of the block.
                Text::new(text, Pt::new(fixed + across, along_at), Anchor::Start).turned()
            } else {
                Text::new(text, Pt::new(along_at, fixed + across), Anchor::Start)
            }
        };
        // For a vertical run the block grows upward from `start` (a y), so the tick is below it.
        let (t0, t1, along_end) = if vertical {
            (
                start + COUNT_ROOM - 1.4,
                start + COUNT_ROOM - 1.4,
                start - 0.0,
            )
        } else {
            (start - COUNT_ROOM + 1.4, start - COUNT_ROOM + 1.4, start)
        };
        let _ = (t1, along_end);
        let tick_at = t0;
        let tick = if vertical {
            (
                Pt::new(fixed - 0.9, tick_at + 0.5),
                Pt::new(fixed + 0.9, tick_at - 0.5),
            )
        } else {
            (
                Pt::new(tick_at - 0.5, fixed + 0.9),
                Pt::new(tick_at + 0.5, fixed - 0.9),
            )
        };
        let name = place(&plan.name, name_off, start);
        let list_t = place(list, list_off, start);
        let other_off = if list_off > 0.0 {
            list_off + 2.4
        } else {
            name_off - 2.4
        };
        let other_t = other.map(|o| place(o, other_off, start));
        let count_t = if vertical {
            Text::new(
                count,
                Pt::new(
                    fixed + count_off * 0.0 + if name_off < 0.0 { -1.4 } else { 1.6 },
                    tick_at + 0.5,
                ),
                if name_off < 0.0 {
                    Anchor::End
                } else {
                    Anchor::Start
                },
            )
        } else {
            Text::new(
                count,
                Pt::new(tick_at + 0.8, fixed + count_off),
                Anchor::Start,
            )
        };
        let widen = |t: &Text, w: f64| {
            let mut r = t.extent();
            // The width the text will be written in, not the width it is measured at.
            if vertical {
                r.min.y = r.max.y - w.max(0.0);
            } else {
                r.max.x = r.min.x + w.max(0.0);
            }
            r
        };
        let mut rects = vec![widen(&name, wn), widen(&list_t, wl), count_t.extent()];
        rects.push(Rect {
            min: Pt::new(tick.0.x.min(tick.1.x), tick.0.y.min(tick.1.y)),
            max: Pt::new(tick.0.x.max(tick.1.x), tick.0.y.max(tick.1.y)),
        });
        if let Some(o) = &other_t {
            rects.push(widen(o, wo));
        }
        Writing {
            label: name,
            list: list_t,
            other: other_t,
            count: count_t,
            tick,
            rects,
        }
    }
}
