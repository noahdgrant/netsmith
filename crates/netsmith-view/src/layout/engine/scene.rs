//! The scene: the grid, the wires and the symbols of one sheet, and the routing that fills it.

use super::*;

/// A pin of a placed part: the part and the index of the pin.
pub(super) type PinKey = (usize, usize);

/// The rail pins of satellites whose stubs end on one line: the net and the line, and the pins
/// along it with where each stub ends.
pub(super) type RailRow = ((usize, i32), Vec<(i32, PinKey)>);

pub(super) struct PinInfo {
    pub(super) part: usize,
    pub(super) idx: usize,
    pub(super) net: usize,
    pub(super) node: Node,
    pub(super) escape: Dir,
}

pub(super) struct Stub {
    pub(super) pin: Node,
    pub(super) end: Node,
    pub(super) heading: Dir,
}

pub(super) struct Scene<'a> {
    pub(super) ctx: &'a Ctx<'a>,
    pub(super) parts: Vec<PlacedPart>,
    pub(super) grid: Grid,
    pub(super) wires: Vec<Wire>,
    pub(super) symbols: Vec<PowerSymbol>,
    pub(super) ports: Vec<Port>,
    pub(super) pins: Vec<PinInfo>,
    pub(super) stubs: BTreeMap<(usize, usize), Stub>,
    /// Nodes each net's wires already cover, for the nets a stack joined.
    pub(super) trees: BTreeMap<usize, BTreeSet<Node>>,
    pub(super) joined: BTreeSet<(usize, usize)>,
    pub(super) local: Vec<usize>,
    pub(super) marks: &'a Marks,
    pub(super) units: &'a [Unit],
    /// The parts that are drawn together: the row of blocks, then each row of loose parts.
    /// A rail is one wire across the group, with one symbol.
    pub(super) groups: Vec<Vec<usize>>,
    /// The parts that stand beside a pin of another part.
    pub(super) satellites: BTreeSet<usize>,
    /// The nets drawn as a name at the end of a stub (flags and labels), not as one wire.
    pub(super) tagged: Vec<bool>,
    /// The block each part is in: its main part, or the first part of a unit that stands
    /// alone.
    pub(super) block_of: Vec<usize>,
    /// Whether each part is in a block with a main part (and not one of the parts that stand
    /// alone).
    pub(super) anchored: Vec<bool>,
    /// How many blocks with a main part each net has pins in.
    pub(super) clusters: Vec<usize>,
    /// The pins of each tagged net, by block.
    pub(super) tag_clusters: BTreeMap<usize, Vec<Vec<(usize, usize)>>>,
    /// What the names and flags cover, with the part each is at.
    pub(super) tag_boxes: Vec<(usize, Rect)>,
    /// The pins whose stub ends in a name or a flag: a wire to them joins the stub half way,
    /// not at its end, where the name is.
    pub(super) tag_keys: BTreeSet<(usize, usize)>,
    pub(super) labels: Vec<NetLabel>,
    /// The buses planned, until they are drawn.
    pub(super) bus_plans: Vec<bus::BusPlan>,
    /// Whether each net is one of a bus that is planned or drawn: it has no stub of its own
    /// and no wire to route.
    pub(super) bus_skip: Vec<bool>,
    /// Whether each net is one of a bus that was drawn.
    pub(super) bus_done: Vec<bool>,
    /// What each net's tag was before the buses took their nets' tags away.
    pub(super) retag: Vec<bool>,
    pub(super) buses: Vec<Bus>,
    /// The pair satellites placed beside two pins: their wires are drawn first.
    pub(super) pair_routes: Vec<pair::PairRoute>,
    /// The pins of those satellites: a wire leaves the tip in any direction, with no stub.
    pub(super) bare: BTreeSet<(usize, usize)>,
    /// The nets of pair satellites, for the policy that names nets.
    pub(super) pair_nets: Vec<bool>,
    /// The rail pins of satellites that have a symbol of their own, or share one with the
    /// satellites beside them: the rail of the group does not reach them.
    pub(super) local_done: BTreeSet<(usize, usize)>,
    /// The nets of the spines and the way the symbol of each goes (-1 left, 1 right).
    pub(super) spine_pref: Vec<(usize, i32)>,
    /// The supply nets drawn as wires with their name on a stub, because their pins are close: no symbol.
    pub(super) close_nets: BTreeSet<usize>,
}

impl<'a> Scene<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        ctx: &'a Ctx<'a>,
        parts: Vec<PlacedPart>,
        units: &'a [Unit],
        marks: &'a Marks,
        groups: Vec<Vec<usize>>,
        satellites: BTreeSet<usize>,
        tagged: Vec<bool>,
        block_of: Vec<usize>,
        anchored: Vec<bool>,
        bus_plans: Vec<bus::BusPlan>,
        retag: Vec<bool>,
        pair_routes: Vec<pair::PairRoute>,
        pair_nets: Vec<bool>,
        spine_pref: Vec<(usize, i32)>,
    ) -> Self {
        let mut bounds = parts[0..]
            .first()
            .map_or(Rect::around(Pt::new(0.0, 0.0)), |p| p.extent);
        for p in &parts {
            bounds.union(p.extent);
        }
        // Room for the rails, their symbols and the names at the pins at the edge.
        // A bus has its spine, its trunk and its writing out beyond the parts as well.
        let bounds = inflate(bounds, if bus_plans.is_empty() { 40.0 } else { 70.0 });
        let grid = Grid::new(bounds, GRID);
        let mut scene = Scene {
            ctx,
            parts,
            grid,
            wires: Vec::new(),
            symbols: Vec::new(),
            ports: Vec::new(),
            pins: Vec::new(),
            stubs: BTreeMap::new(),
            trees: BTreeMap::new(),
            joined: BTreeSet::new(),
            local: vec![0; ctx.design.nets.len()],
            marks,
            units,
            groups,
            satellites,
            clusters: vec![0; ctx.design.nets.len()],
            tagged,
            block_of,
            anchored,
            tag_clusters: BTreeMap::new(),
            tag_boxes: Vec::new(),
            tag_keys: BTreeSet::new(),
            labels: Vec::new(),
            bus_skip: {
                let mut skip = vec![false; ctx.design.nets.len()];
                for plan in &bus_plans {
                    for &net in &plan.nets {
                        skip[net] = true;
                    }
                }
                skip
            },
            bus_done: vec![false; ctx.design.nets.len()],
            bus_plans,
            retag,
            buses: Vec::new(),
            bare: pair_routes
                .iter()
                .flat_map(|r| r.links.iter().map(|&(pin, _)| (r.part, pin)))
                .collect(),
            pair_routes,
            pair_nets,
            local_done: BTreeSet::new(),
            spine_pref,
            close_nets: BTreeSet::new(),
        };
        scene.block_parts();
        scene.collect_pins();
        scene.count_clusters();
        scene.block_open_pins();
        scene
    }

    /// An X is at the end of every pin with nothing on it, and nothing else is near it.
    pub(super) fn block_open_pins(&mut self) {
        let mut rects = Vec::new();
        for (p, part) in self.parts.iter().enumerate() {
            for (pin, g) in part.pins.iter().zip(&part.glyph.pins) {
                if g.hidden {
                    continue;
                }
                let open = match pin.net {
                    None => true,
                    Some(n) => !self.active(n),
                };
                let _ = p;
                if open {
                    rects.push(inflate(Rect::around(pin.at), 1.5));
                }
            }
        }
        for r in rects {
            self.grid.block_rect(r);
        }
    }

    pub(super) fn net_id(net: usize) -> u32 {
        net as u32
    }

    /// Parts, the lines of their pins and all their texts are in the way of wires.
    pub(super) fn block_parts(&mut self) {
        for part in &self.parts {
            block_part(&mut self.grid, part);
        }
    }

    pub(super) fn collect_pins(&mut self) {
        let mut seen: BTreeSet<Node> = BTreeSet::new();
        for (p, part) in self.parts.iter().enumerate() {
            for (i, pin) in part.pins.iter().enumerate() {
                if part.glyph.pins[i].hidden {
                    continue;
                }
                let node = self.grid.node(pin.at);
                if let Some(net) = pin.net
                    && seen.insert(node)
                {
                    self.pins.push(PinInfo {
                        part: p,
                        idx: i,
                        net,
                        node,
                        escape: pin.escape,
                    });
                    self.local[net] += 1;
                }
            }
        }
    }

    /// How many blocks with a main part each net has pins in: parts that stand alone are
    /// reached by a bus wire and do not make a net one to name.
    pub(super) fn count_clusters(&mut self) {
        for net in 0..self.clusters.len() {
            let blocks: BTreeSet<usize> = self
                .pins
                .iter()
                .filter(|p| p.net == net && self.anchored[p.part])
                .map(|p| self.block_of[p.part])
                .collect();
            self.clusters[net] = blocks.len();
        }
    }

    /// Whether the net has to be drawn here: two of its pins, a rail, or a way on.
    pub(super) fn active(&self, net: usize) -> bool {
        self.local[net] >= 2 || self.ctx.external[net] || self.ctx.power[net].is_some()
    }

    pub(super) fn poly(&self, nodes: &[Node]) -> Vec<Pt> {
        grid::corners(nodes)
            .into_iter()
            .map(|n| self.grid.pt(n))
            .collect()
    }

    pub(super) fn push_wire(&mut self, net: usize, nodes: &[Node]) {
        if nodes.len() >= 2 {
            let points = self.poly(nodes);
            self.wires.push(Wire { net, points });
        }
    }

    /// The stack's own wires, and the straight stubs out of the pins that are not in one.
    pub(super) fn join_and_stub(&mut self) {
        let units = self.units;
        for unit in units.iter().filter(|u| u.parts.len() > 1) {
            for w in 1..unit.parts.len() {
                let (prev, next) = (unit.parts[w - 1], unit.parts[w]);
                let down = self.parts[prev]
                    .pins
                    .iter()
                    .position(|q| q.id != unit.up_pads[w - 1])
                    .expect("two pins");
                let up = self.parts[next]
                    .pins
                    .iter()
                    .position(|q| q.id == unit.up_pads[w])
                    .expect("two pins");
                let (a, b) = (
                    self.grid.node(self.parts[prev].pins[down].at),
                    self.grid.node(self.parts[next].pins[up].at),
                );
                let Some(net) = self.parts[prev].pins[down].net else {
                    continue;
                };
                self.grid.claim_wire(a, b, Self::net_id(net));
                let mut nodes = vec![a];
                let (dx, dy) = ((b.0 - a.0).signum(), (b.1 - a.1).signum());
                let mut k = a;
                while k != b {
                    k = (k.0 + dx, k.1 + dy);
                    nodes.push(k);
                }
                self.push_wire(net, &[a, b]);
                self.trees.entry(net).or_default().extend(nodes);
                self.joined.insert((prev, down));
                self.joined.insert((next, up));
            }
        }
        let pins: Vec<(usize, usize, usize, Node, Dir)> = self
            .pins
            .iter()
            .map(|p| (p.part, p.idx, p.net, p.node, p.escape))
            .collect();
        for (part, idx, net, node, escape) in pins {
            if self.joined.contains(&(part, idx)) || !self.active(net) || self.bus_skip[net] {
                continue;
            }
            self.stub_pin(part, idx, net, node, escape);
        }
    }

    /// The straight stub out of a pin, as far as it goes free, up to [`STUB_CELLS`].
    pub(super) fn stub_pin(
        &mut self,
        part: usize,
        idx: usize,
        net: usize,
        node: Node,
        escape: Dir,
    ) {
        let end = if self.bare.contains(&(part, idx)) {
            // A pair satellite's pin has no stub: the wire leaves the tip as it likes.
            self.grid.unblock(node);
            self.grid.claim_wire(node, node, Self::net_id(net));
            node
        } else {
            lay_stub(&mut self.grid, node, escape, Self::net_id(net))
        };
        if end != node {
            self.push_wire(net, &[node, end]);
        }
        self.stubs.insert(
            (part, idx),
            Stub {
                pin: node,
                end,
                heading: escape,
            },
        );
    }

    pub(super) fn manhattan(a: Node, b: Node) -> i32 {
        (a.0 - b.0).abs() + (a.1 - b.1).abs()
    }

    /// Joins the stubs of `pins` (and what the net already has) into one tree of wires, and
    /// returns the nodes it covers.
    pub(super) fn route_tree(
        &mut self,
        net: usize,
        pins: &[(usize, usize)],
        mut tree: BTreeSet<Node>,
    ) -> BTreeSet<Node> {
        let id = Self::net_id(net);
        let mut todo: Vec<(usize, usize)> = pins
            .iter()
            .copied()
            .filter(|k| self.stubs.contains_key(k))
            .collect();
        todo.sort_by_key(|k| (self.stubs[k].pin.0, self.stubs[k].pin.1));
        let add_stub = |tree: &mut BTreeSet<Node>, s: &Stub| {
            let (dx, dy) = ((s.end.0 - s.pin.0).signum(), (s.end.1 - s.pin.1).signum());
            let mut k = s.pin;
            tree.insert(k);
            while k != s.end {
                k = (k.0 + dx, k.1 + dy);
                tree.insert(k);
            }
        };
        if tree.is_empty() && !todo.is_empty() {
            let first = todo.remove(0);
            add_stub(&mut tree, &self.stubs[&first]);
        }
        while !todo.is_empty() {
            // The pin nearest the tree goes next.
            let (at, _) = todo
                .iter()
                .enumerate()
                .map(|(i, k)| {
                    let end = self.stubs[k].end;
                    let d = tree
                        .iter()
                        .map(|&t| Self::manhattan(end, t))
                        .min()
                        .unwrap_or(0);
                    (i, (d, k.0, k.1))
                })
                .min_by_key(|&(_, key)| key)
                .expect("a pin");
            let key = todo.remove(at);
            let (end, heading, pin) = {
                let s = &self.stubs[&key];
                (s.end, s.heading, s.pin)
            };
            // A stub that ends in a name is joined half way along: the name is in the way
            // at its end.
            let start = if self.tag_keys.contains(&key) && Self::manhattan(end, pin) >= 2 {
                let (dx, dy) = heading.vector();
                (end.0 - dx as i32, end.1 - dy as i32)
            } else {
                end
            };
            let targets: HashSet<Node> = tree.iter().copied().collect();
            let routed = if self.bare.contains(&key) {
                self.grid
                    .route_free(id, start, &targets)
                    .map(|(_, path)| path)
            } else {
                self.grid.route(id, start, heading, &targets)
            };
            match routed {
                Some(path) => {
                    self.grid.commit(&path, id);
                    self.push_wire(net, &path);
                    tree.extend(path.iter().copied());
                }
                None => {
                    // Nothing clear: the pin is left unjoined, and `check` says so. A wire
                    // laid through whatever is in the way would join it by drawing a short
                    // or a wire along another net's.
                    continue;
                }
            }
            add_stub(&mut tree, &self.stubs[&key]);
            tree.insert(pin);
        }
        tree
    }

    pub(super) fn pins_of(&self, net: usize) -> Vec<(usize, usize)> {
        self.pins
            .iter()
            .filter(|p| p.net == net && !self.joined.contains(&(p.part, p.idx)))
            .map(|p| (p.part, p.idx))
            .collect()
    }

    /// The wires of the pair satellites, before anything else is routed: each joins the stub of
    /// the main part's pin on its net, in the order the placement found. What else is on the
    /// net is joined to them afterwards.
    pub(super) fn route_pairs(&mut self) {
        for route in self.pair_routes.clone() {
            for &k in &route.order {
                let (sat_pin, anchor_pin) = route.links[k];
                let Some(net) = self.parts[route.part].pins[sat_pin].net else {
                    continue;
                };
                let Some(stub) = self.stubs.get(&(route.anchor, anchor_pin)) else {
                    continue;
                };
                let mut tree: BTreeSet<Node> = BTreeSet::new();
                let (dx, dy) = (
                    (stub.end.0 - stub.pin.0).signum(),
                    (stub.end.1 - stub.pin.1).signum(),
                );
                let mut at = stub.pin;
                tree.insert(at);
                while at != stub.end {
                    at = (at.0 + dx, at.1 + dy);
                    tree.insert(at);
                }
                // The row of a spine through the net is joined where the wire meets it.
                tree.extend(self.trees.get(&net).cloned().unwrap_or_default());
                let mut tree = self.route_tree(net, &[(route.part, sat_pin)], tree);
                tree.extend(self.trees.remove(&net).unwrap_or_default());
                self.trees.insert(net, tree);
            }
        }
    }

    /// The nets of the spines first (`spine`), then every other signal.
    pub(super) fn route_signals(&mut self, spine: bool) {
        let mut nets: Vec<(i32, usize)> = (0..self.ctx.design.nets.len())
            .filter(|&n| {
                (self.ctx.power[n].is_none() || self.ctx.spine_net[n])
                    && self.ctx.spine_net[n] == spine
                    && self.active(n)
                    && !self.bus_skip[n]
            })
            .map(|n| {
                let nodes: Vec<Node> = self
                    .pins
                    .iter()
                    .filter(|p| p.net == n)
                    .map(|p| p.node)
                    .collect();
                let (lo, hi) = nodes
                    .iter()
                    .fold(((i32::MAX, i32::MAX), (i32::MIN, i32::MIN)), |(l, h), q| {
                        ((l.0.min(q.0), l.1.min(q.1)), (h.0.max(q.0), h.1.max(q.1)))
                    });
                (
                    if nodes.is_empty() {
                        0
                    } else {
                        (hi.0 - lo.0) + (hi.1 - lo.1)
                    },
                    n,
                )
            })
            .collect();
        nets.sort();
        for (_, net) in nets {
            if let Some(clusters) = self.tag_clusters.get(&net).cloned() {
                // A net with a name at each of its blocks: each block's pins are wired to
                // that block's name, and nothing joins the blocks.
                let mut stacked = self.trees.remove(&net).unwrap_or_default();
                for cluster in clusters {
                    let has_joined = cluster.iter().any(|k| self.joined.contains(k));
                    let pins: Vec<(usize, usize)> = cluster
                        .iter()
                        .copied()
                        .filter(|k| !self.joined.contains(k))
                        .collect();
                    let tree = if has_joined {
                        std::mem::take(&mut stacked)
                    } else {
                        BTreeSet::new()
                    };
                    self.route_tree(net, &pins, tree);
                }
                continue;
            }
            let mut pins = self.pins_of(net);
            if spine {
                // The pair satellites' wires are drawn next and join the row.
                pins.retain(|k| !self.bare.contains(k));
            }
            let tree = self.trees.remove(&net).unwrap_or_default();
            let tree = self.route_tree(net, &pins, tree);
            self.trees.insert(net, tree);
        }
    }

    pub(super) fn rect_is_free(&self, r: Rect) -> bool {
        let r = inflate(r, 0.2);
        let lo = self.grid.node(Pt::new(r.min.x, r.min.y));
        let hi = self.grid.node(Pt::new(r.max.x, r.max.y));
        for j in lo.1..=hi.1 {
            for i in lo.0..=hi.0 {
                if !self.grid.is_free((i, j)) {
                    return false;
                }
            }
        }
        true
    }

    /// The nodes of the stub ends of a part's pins on one side, with what is between.
    pub(super) fn along(node: Node, escape: Dir) -> i32 {
        if escape.is_vertical() { node.0 } else { node.1 }
    }

    /// Each rail is one wire across its group, at the top (a supply) or the bottom (a ground)
    /// beyond every part, with one symbol; the pins hang from it by straight wires. Pins next
    /// to each other on one side share a bus first.
    /// A supply whose pins on the sheet are on two or more parts that stand close together, none of them a
    /// part to ground, is wired and not drawn as a symbol: the symbol would join what a short wire joins
    /// already (a connector's pin and a fuse's), and read as a port to somewhere. The wire has the net's name on a short stub. A net whose pins are far
    /// apart (a capacitor across the sheet) keeps its symbol, which is what joins them, and so
    /// does one that does not wire cleanly or has no room for the name. Grounds are as they were.
    pub(super) fn route_close_rails(&mut self) {
        for net in 0..self.ctx.design.nets.len() {
            if self.ctx.power[net] != Some(PowerKind::Supply)
                || self.ctx.spine_net[net]
                || self.ctx.external[net]
                || self.bus_skip[net]
                || self.tag_clusters.contains_key(&net)
            {
                continue;
            }
            let keys: Vec<(usize, usize)> = self
                .pins_of(net)
                .into_iter()
                .filter(|k| {
                    self.stubs.contains_key(k)
                        && !self.bare.contains(k)
                        && !self.local_done.contains(k)
                })
                .collect();
            let parts: BTreeSet<usize> = keys.iter().map(|k| k.0).collect();
            if parts.len() < 2 || keys.len() > CLOSE_MAX_PINS {
                continue;
            }
            // A small part from the rail to a ground (a decoupling or bulk capacitor, a TVS diode) is what a supply
            // symbol is for: the rail has a symbol, and so does every rail that has one.
            let shunted = parts.iter().any(|&p| {
                crate::layout::is_stackable(self.ctx, p)
                    && self.ctx.part_nets[p].iter().any(|&n| self.ctx.ground[n])
            });
            if shunted {
                continue;
            }
            let ends: Vec<Node> = keys.iter().map(|k| self.stubs[k].end).collect();
            let (lo, hi) = ends
                .iter()
                .fold(((i32::MAX, i32::MAX), (i32::MIN, i32::MIN)), |(l, h), q| {
                    ((l.0.min(q.0), l.1.min(q.1)), (h.0.max(q.0), h.1.max(q.1)))
                });
            let span = (hi.0 - lo.0) + (hi.1 - lo.1);
            if span > CLOSE_CELLS {
                continue;
            }
            // Wired on a copy of the grid first: kept only if every pin is joined without a detour and the name fits.
            let saved = (
                self.grid.clone(),
                self.wires.len(),
                self.trees.get(&net).cloned(),
            );
            let tree = self.route_tree(
                net,
                &keys,
                self.trees.get(&net).cloned().unwrap_or_default(),
            );
            let joined = keys.iter().all(|k| {
                let s = &self.stubs[k];
                tree.contains(&s.pin) && tree.contains(&s.end)
            });
            let tidy = tree.len() as i32 <= 2 * span + 2 * keys.len() as i32 + 6;
            if joined && tidy && self.name_wire(net, &tree) {
                self.trees.insert(net, tree);
                self.close_nets.insert(net);
                continue;
            }
            self.grid = saved.0;
            self.wires.truncate(saved.1);
            match saved.2 {
                Some(t) => {
                    self.trees.insert(net, t);
                }
                None => {
                    self.trees.remove(&net);
                }
            }
        }
    }

    /// Puts the name of `net` on a short stem up (else down) from the wire `tree`, as far as it goes free: the label of a
    /// net that is wired and has no symbol. False when no node has room.
    pub(super) fn name_wire(&mut self, net: usize, tree: &BTreeSet<Node>) -> bool {
        let id = Self::net_id(net);
        let name = ellipsize(&self.ctx.design.nets[net].name);
        let mid = (
            tree.iter().map(|n| n.0).sum::<i32>() / tree.len().max(1) as i32,
            tree.iter().map(|n| n.1).sum::<i32>() / tree.len().max(1) as i32,
        );
        let mut nodes: Vec<Node> = tree.iter().copied().collect();
        nodes.sort_by_key(|n| (Self::manhattan(*n, mid), n.1, n.0));
        for q in nodes {
            for dir in [Dir::Up, Dir::Down] {
                let (dx, dy) = dir.vector();
                let end = (q.0 + dx as i32 * STEM_CELLS, q.1 + dy as i32 * STEM_CELLS);
                let free = (1..=STEM_CELLS + 2).all(|k| {
                    self.grid
                        .is_free((q.0 + dx as i32 * k, q.1 + dy as i32 * k))
                });
                if !free || self.grid.is_blocked(q) {
                    continue;
                }
                let at = self.grid.pt(end);
                let text = name_text(&name, at, dir);
                let mut rect = text.extent();
                let more = 0.15 * text.text.chars().count() as f64 * TEXT_SIZE * CHAR_WIDTH;
                match dir {
                    Dir::Up => rect.min.y -= more,
                    _ => rect.max.y += more,
                }
                if !self.rect_is_free(inflate(rect, 0.3)) {
                    continue;
                }
                self.grid.claim_wire(q, end, id);
                self.push_wire(net, &[q, end]);
                self.labels.push(NetLabel { net, text, at, dir });
                return true;
            }
        }
        false
    }

    pub(super) fn route_rails(&mut self) {
        self.local_rails();
        for g in 0..self.groups.len() {
            let mut rect: Option<Rect> = None;
            for &p in &self.groups[g] {
                union_into(&mut rect, self.parts[p].extent);
            }
            // What is written at the pins is part of the group: a rail goes beyond it.
            for (p, r) in &self.tag_boxes {
                if self.groups[g].contains(p) {
                    union_into(&mut rect, *r);
                }
            }
            let Some(rect) = rect else {
                continue;
            };
            let in_group: BTreeSet<usize> = self.groups[g].iter().copied().collect();
            let mut nets: Vec<usize> = self
                .pins
                .iter()
                .filter(|p| {
                    in_group.contains(&p.part)
                        && self.ctx.power[p.net].is_some()
                        && !self.ctx.spine_net[p.net]
                        && !self.close_nets.contains(&p.net)
                        && self.stubs.contains_key(&(p.part, p.idx))
                        && !self.bare.contains(&(p.part, p.idx))
                        && !self.local_done.contains(&(p.part, p.idx))
                })
                .map(|p| p.net)
                .collect();
            nets.sort_unstable();
            nets.dedup();
            let (mut supplies, mut grounds) = (0, 0);
            for net in nets {
                let kind = self.ctx.power[net].expect("a rail");
                let k = if kind == PowerKind::Supply {
                    supplies += 1;
                    supplies - 1
                } else {
                    grounds += 1;
                    grounds - 1
                };
                self.rail(net, kind, k, rect, &in_group);
            }
        }
    }

    pub(super) fn rail(
        &mut self,
        net: usize,
        kind: PowerKind,
        k: i32,
        rect: Rect,
        in_group: &BTreeSet<usize>,
    ) {
        let id = Self::net_id(net);
        let natural = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let mut keys: Vec<(usize, usize)> = self
            .pins
            .iter()
            .filter(|p| p.net == net && in_group.contains(&p.part))
            .map(|p| (p.part, p.idx))
            .filter(|key| {
                self.stubs.contains_key(key)
                    && !self.bare.contains(key)
                    && !self.local_done.contains(key)
            })
            .collect();
        keys.sort_by_key(|key| {
            let s = &self.stubs[key];
            (key.0, s.heading as u8, Self::along(s.end, s.heading))
        });
        // The rail's wire is beyond the parts that have a pin on it (and what is written at their
        // pins), not beyond every part of the group: the supply of a unit that stands apart from
        // a tall one has its symbol just above it.
        let rect = {
            let mut own: Option<Rect> = None;
            for key in &keys {
                union_into(&mut own, self.parts[key.0].extent);
            }
            for (p, r) in &self.tag_boxes {
                if keys.iter().any(|k| k.0 == *p) {
                    union_into(&mut own, *r);
                }
            }
            own.unwrap_or(rect)
        };
        // Runs: neighbours on one side of one part, with no other pin between them.
        let mut runs: Vec<Vec<(usize, usize)>> = Vec::new();
        for key in keys {
            let s = &self.stubs[&key];
            let (end, heading) = (s.end, s.heading);
            let joins = runs.last().is_some_and(|run| {
                let last = &self.stubs[run.last().expect("a run has a pin")];
                let (a, b) = (Self::along(last.end, heading), Self::along(end, heading));
                last.heading == heading
                    && run[0].0 == key.0
                    && b - a <= 2
                    && !self.parts[key.0].pins.iter().any(|q| {
                        let node = self.grid.node(q.at);
                        let c = Self::along(node, heading);
                        q.escape == heading && c > a && c < b
                    })
            });
            if joins {
                runs.last_mut().expect("a run to join").push(key);
            } else {
                runs.push(vec![key]);
            }
        }
        struct Drop {
            node: Node,
            heading: Dir,
            part: usize,
        }
        let mut drops: Vec<Drop> = Vec::new();
        for run in &runs {
            let ends: Vec<Node> = run.iter().map(|key| self.stubs[key].end).collect();
            let heading = self.stubs[&run[0]].heading;
            if ends.len() > 1 {
                let (first, last) = (ends[0], ends[ends.len() - 1]);
                self.grid.claim_wire(first, last, id);
                self.push_wire(net, &[first, last]);
            }
            let (node, h) = if heading.is_vertical() {
                let mean = ends.iter().map(|n| n.0).sum::<i32>() as f64 / ends.len() as f64;
                let hub = *ends
                    .iter()
                    .min_by(|a, b| {
                        (f64::from(a.0) - mean)
                            .abs()
                            .total_cmp(&(f64::from(b.0) - mean).abs())
                    })
                    .expect("a pin");
                (hub, heading)
            } else if ends.len() > 1 {
                let pick = if kind == PowerKind::Supply {
                    ends.iter().min_by_key(|n| n.1)
                } else {
                    ends.iter().max_by_key(|n| n.1)
                };
                (*pick.expect("a pin"), natural)
            } else {
                (ends[0], heading)
            };
            drops.push(Drop {
                node,
                heading: h,
                part: run[0].0,
            });
        }
        // The rail's row, and what it spans.
        let top = self.grid.node(Pt::new(0.0, rect.min.y)).1;
        let bottom = self.grid.node(Pt::new(0.0, rect.max.y)).1;
        let row = if kind == PowerKind::Supply {
            top - (4 + 3 * k)
        } else {
            bottom + (4 + 3 * k)
        };
        let xs: Vec<i32> = drops.iter().map(|d| d.node.0).collect();
        let (mut lo, mut hi) = (
            *xs.iter().min().expect("a drop"),
            *xs.iter().max().expect("a drop"),
        );
        // The rail goes further out while a text or a part is on its row.
        let outward = if kind == PowerKind::Supply { -1 } else { 1 };
        let mut row = row;
        for _ in 0..40 {
            if (lo..=hi).all(|x| self.grid.free_for((x, row), id)) {
                break;
            }
            row += outward;
        }
        // Where the symbol hangs: in line with the outermost satellite on the rail, the part's
        // own centre when there is one part, else the left end.
        let centre = self
            .grid
            .node(Pt::new((rect.min.x + rect.max.x) / 2.0, 0.0))
            .0;
        let parts_on: BTreeSet<usize> = drops.iter().map(|d| d.part).collect();
        let sat_xs: Vec<i32> = drops
            .iter()
            .filter(|d| self.satellites.contains(&d.part))
            .map(|d| d.node.0)
            .collect();
        let preferred = if let Some(&x) = sat_xs.iter().max_by_key(|&&x| ((x - centre).abs(), -x)) {
            x
        } else if parts_on.len() == 1 {
            (lo + hi) / 2
        } else {
            lo
        };
        // The symbol's place decides how far the rail goes: if it is outside the drops, the
        // rail reaches it.
        let mut order: Vec<i32> = vec![preferred];
        order.extend(xs.iter().copied());
        for d in 1..=14 {
            order.push(lo - d);
            order.push(hi + d);
        }
        order.dedup();
        let mut chosen: Option<(i32, Option<Text>)> = None;
        for &x in &order {
            if let Some(label) = self.symbol_fits(net, kind, (x, row)) {
                chosen = Some((x, label));
                break;
            }
        }
        let sym_x = chosen.as_ref().map_or(preferred, |c| c.0);
        lo = lo.min(sym_x);
        hi = hi.max(sym_x);
        self.grid.claim_wire((lo, row), (hi, row), id);
        if hi > lo {
            self.push_wire(net, &[(lo, row), (hi, row)]);
        }
        let targets: HashSet<Node> = (lo..=hi).map(|x| (x, row)).collect();
        for d in &drops {
            // Nothing clear: the pin is left unjoined, and `check` says so.
            if let Some(path) = self.grid.route(id, d.node, d.heading, &targets) {
                self.grid.commit(&path, id);
                self.push_wire(net, &path);
            }
        }
        // The symbol, at the end of a stem from the rail.
        self.place_symbol(net, kind, (sym_x, row), chosen.map(|c| c.1));
    }

    /// The symbol of rail `net` at the end of a stem from `q`, a node of the rail's wire.
    /// `label` is the place its name was found for by [`symbol_fits`](Self::symbol_fits); none
    /// when no place was found, and the name goes beside it as it is.
    pub(super) fn place_symbol(
        &mut self,
        net: usize,
        kind: PowerKind,
        q: Node,
        label: Option<Option<Text>>,
    ) {
        let id = Self::net_id(net);
        let natural = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let (dx, dy) = natural.vector();
        let end = (q.0 + dx as i32 * STEM_CELLS, q.1 + dy as i32 * STEM_CELLS);
        self.grid.claim_wire(q, end, id);
        self.push_wire(net, &[q, end]);
        let at = self.grid.pt(end);
        let name = ellipsize(&self.ctx.design.nets[net].name);
        let named = kind == PowerKind::Supply || self.ctx.label_grounds;
        let label = match label {
            Some(l) => l,
            None => named.then(|| label_beside(at, natural, &name, Side::Right)),
        };
        let symbol = PowerSymbol {
            net,
            kind,
            at,
            dir: natural,
            label,
        };
        self.grid.block_rect(inflate(symbol.extent(), 0.9));
        self.symbols.push(symbol);
    }

    /// A satellite whose pin is on a rail has a stub and a symbol of its own, short and local,
    /// instead of a drop to the one wire across the group (a decoupling capacitor's supply
    /// and ground would run the length of the sheet and across other wires): the stub of its
    /// pin goes on, a few cells at most, to a symbol that points away from the part (a supply
    /// up, a ground down). Satellites standing side by side in a row, whose pins end on one
    /// line, share a wire and a symbol when the wire is short and nothing is on its way
    /// ([`policy::shares_rail`]). A pin that has no room for a symbol, or whose stub does not
    /// point away from the part, stays with the group's rail, as does every pin of a main
    /// part.
    pub(super) fn local_rails(&mut self) {
        // The rail pins, by net and by the line their stubs end on.
        let mut rows: BTreeMap<(usize, i32), Vec<(i32, PinKey)>> = BTreeMap::new();
        for p in &self.pins {
            let Some(kind) = self.ctx.power[p.net] else {
                continue;
            };
            if self.ctx.spine_net[p.net] || self.close_nets.contains(&p.net) {
                continue;
            }
            let key = (p.part, p.idx);
            if self.bare.contains(&key) {
                continue;
            }
            let Some(stub) = self.stubs.get(&key) else {
                continue;
            };
            let natural = if kind == PowerKind::Supply {
                Dir::Up
            } else {
                Dir::Down
            };
            if self.satellites.contains(&p.part) {
                if stub.heading != natural {
                    continue;
                }
            } else {
                // A main part's few pins on a rail, when the sheet is drawn so: out of the
                // part, not through it (a stub that goes the other way from the symbol's).
                let on_net = self
                    .pins
                    .iter()
                    .filter(|q| q.part == p.part && q.net == p.net);
                if on_net.count() > Tuning::LOCAL_MAIN_PINS || stub.heading == natural.opposite() {
                    continue;
                }
                // The pin is one that `Tuning::local_main` gives a symbol: told to the context
                // (`Ctx::touch`), whether it does or not.
                self.ctx.touch(Tuning::LOCAL_MAIN_BIT);
                if !self.ctx.tuning.local_main {
                    continue;
                }
            }
            rows.entry((p.net, stub.end.1))
                .or_default()
                .push((stub.end.0, key));
        }
        // The wires that carry a symbol so far, by net: a satellite near one of them is joined to it
        // instead of having a symbol of its own (see `bridge`). A supply's rows are taken from the
        // top down and a ground's from the bottom up, so that a symbol is always the farthest out.
        let mut hubs: Vec<(usize, Vec<Node>)> = Vec::new();
        let mut rows: Vec<RailRow> = rows.into_iter().collect();
        rows.sort_by_key(|&((net, row), _)| {
            let ground = self.ctx.power[net] == Some(PowerKind::Ground);
            (net, if ground { -row } else { row })
        });
        for ((net, row), mut pins) in rows {
            pins.sort();
            let kind = self.ctx.power[net].expect("a rail");
            let id = Self::net_id(net);
            // Runs of pins whose stubs end near each other on the line.
            let mut runs: Vec<Vec<(i32, PinKey)>> = Vec::new();
            for pin in pins {
                match runs.last_mut() {
                    Some(run)
                        if f64::from(pin.0 - run[run.len() - 1].0)
                            <= policy::RAIL_ROW_GAP_STEPS =>
                    {
                        run.push(pin)
                    }
                    _ => runs.push(vec![pin]),
                }
            }
            for run in runs {
                let (first, last) = (run[0].0, run[run.len() - 1].0);
                let mut shared = false;
                if run.len() >= 2
                    && policy::shares_rail(f64::from(last - first))
                    && (first..=last).all(|x| self.grid.free_for((x, row), id))
                {
                    // The symbol hangs from the end of the row, or else from another stub.
                    let mut order: Vec<i32> = run.iter().map(|p| p.0).collect();
                    order.reverse();
                    let found = order.into_iter().find_map(|x| {
                        self.symbol_fits(net, kind, (x, row))
                            .map(|label| (x, label))
                    });
                    if let Some((x, label)) = found {
                        self.grid.claim_wire((first, row), (last, row), id);
                        self.push_wire(net, &[(first, row), (last, row)]);
                        self.place_symbol(net, kind, (x, row), Some(label));
                        self.local_done.extend(run.iter().map(|p| p.1));
                        hubs.push((net, (first..=last).map(|x| (x, row)).collect()));
                        shared = true;
                    }
                }
                if shared {
                    continue;
                }
                for &(x, key) in &run {
                    if self.bridge(net, kind, (x, row), key, &hubs) {
                        continue;
                    }
                    self.local_symbol(net, kind, (x, row), key);
                    if self.local_done.contains(&key) {
                        hubs.push((net, vec![(x, row)]));
                    }
                }
            }
        }
    }

    /// A satellite pin whose stub ends at `end` joins a wire of its rail that carries a symbol
    /// (`hubs`), where that is near and the way is free: pull-ups side by side, standing a row
    /// apart, share one wire and one symbol, as a hand drawing has them, and not a symbol
    /// each. The wire goes out along the stub's way to the row of the hub's wire and across to
    /// it, through free nodes only (so it crosses nothing, and does not run along anything).
    /// Whether it was drawn.
    pub(super) fn bridge(
        &mut self,
        net: usize,
        kind: PowerKind,
        end: Node,
        key: (usize, usize),
        hubs: &[(usize, Vec<Node>)],
    ) -> bool {
        let id = Self::net_id(net);
        let natural = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let (_, dy) = natural.vector();
        let dy = dy as i32;
        // The nearest node of a hub of this net that is level with the stub or beyond it.
        let mut best: Option<(i32, Node)> = None;
        for (n, nodes) in hubs {
            if *n != net {
                continue;
            }
            for &t in nodes {
                let out = (t.1 - end.1) * dy;
                let cost = (t.0 - end.0).abs() + out;
                if out >= 0
                    && cost <= policy::BRIDGE_CELLS
                    && best.is_none_or(|(c, b)| (cost, t) < (c, b))
                {
                    best = Some((cost, t));
                }
            }
        }
        let Some((_, to)) = best else {
            return false;
        };
        // Out along the stub's way, then across.
        let mut path: Vec<Node> = vec![end];
        let mut at = end;
        while at.1 != to.1 {
            let next = (at.0, at.1 + dy);
            if !self.grid.free_for(next, id) || self.grid.step_is_cut(at, natural) {
                return false;
            }
            path.push(next);
            at = next;
        }
        let across = if to.0 > at.0 { Dir::Right } else { Dir::Left };
        while at.0 != to.0 {
            let next = (at.0 + across.vector().0 as i32, at.1);
            if !self.grid.free_for(next, id) || self.grid.step_is_cut(at, across) {
                return false;
            }
            path.push(next);
            at = next;
        }
        if path.len() < 2 {
            return false;
        }
        let corner = (end.0, to.1);
        self.grid.claim_wire(end, corner, id);
        self.grid.claim_wire(corner, to, id);
        self.push_wire(net, &path);
        self.local_done.insert(key);
        true
    }

    /// The symbol of one satellite pin whose stub ends at `end`: at the end of the stub, or a
    /// few cells beyond it if the symbol or its name has no room there.
    pub(super) fn local_symbol(
        &mut self,
        net: usize,
        kind: PowerKind,
        end: Node,
        key: (usize, usize),
    ) {
        let id = Self::net_id(net);
        let natural = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let (dx, dy) = natural.vector();
        for extra in 0..=policy::LOCAL_STEM_EXTRA {
            let q = (end.0 + dx as i32 * extra, end.1 + dy as i32 * extra);
            if extra > 0 {
                let before = (q.0 - dx as i32, q.1 - dy as i32);
                if !self.grid.free_for(q, id) || self.grid.step_is_cut(before, natural) {
                    return;
                }
            }
            let Some(label) = self.symbol_fits(net, kind, q) else {
                continue;
            };
            if extra > 0 {
                self.grid.claim_wire(end, q, id);
                self.push_wire(net, &[end, q]);
            }
            self.place_symbol(net, kind, q, Some(label));
            self.local_done.insert(key);
            return;
        }
    }

    /// Whether a symbol can hang at `q` of a rail: the stem and the drawing are clear of
    /// everything, and its name has room. Gives the name's place.
    pub(super) fn symbol_fits(&self, net: usize, kind: PowerKind, q: Node) -> Option<Option<Text>> {
        let dir = if kind == PowerKind::Supply {
            Dir::Up
        } else {
            Dir::Down
        };
        let (dx, dy) = dir.vector();
        let end = (q.0 + dx as i32 * STEM_CELLS, q.1 + dy as i32 * STEM_CELLS);
        if !self.grid.run_is_free(q, dir, STEM_CELLS + 2) && !self.grid.is_free(q) {
            // The rail's own row is free before it is claimed; only the stem is tested.
        }
        let stem_free = (1..=STEM_CELLS + 2).all(|k| {
            self.grid
                .is_free((q.0 + dx as i32 * k, q.1 + dy as i32 * k))
        });
        if !stem_free || self.grid.is_blocked(q) {
            return None;
        }
        let at = self.grid.pt(end);
        let glyph = PowerSymbol {
            net,
            kind,
            at,
            dir,
            label: None,
        }
        .glyph_extent();
        if !self.rect_is_free(inflate(glyph, 0.3)) {
            return None;
        }
        let named = kind == PowerKind::Supply || self.ctx.label_grounds;
        if !named {
            return Some(None);
        }
        let name = ellipsize(&self.ctx.design.nets[net].name);
        [Side::Right, Side::Left, Side::Beyond]
            .into_iter()
            .map(|s| label_beside(at, dir, &name, s))
            .find(|t| self.rect_is_free(inflate(t.extent(), 0.3)))
            .map(Some)
    }

    /// Whether every node of the boxes `r` (and a little round it) is free to a wire of `net`:
    /// not in the way of a part or text, and not another net's.
    pub(super) fn rect_is_free_for(&self, r: Rect, net: usize) -> bool {
        let r = inflate(r, 0.2);
        let lo = self.grid.node(Pt::new(r.min.x, r.min.y));
        let hi = self.grid.node(Pt::new(r.max.x, r.max.y));
        // Only the nodes that lie in the box, not those a rounding of its corners reaches.
        (lo.1 - 1..=hi.1 + 1).all(|j| {
            (lo.0 - 1..=hi.0 + 1).all(|i| {
                let p = self.grid.pt((i, j));
                let inside = p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y;
                !inside || self.grid.free_for((i, j), Self::net_id(net))
            })
        })
    }

    /// A name at the end of the stub of every block's pins of each net that is tagged: a flag
    /// for a net that goes on to another sheet, a label for a net the policy named. The pin is
    /// the one of the part with the most pins (the main part), in the order of the parts'
    /// paths, and its stub is lengthened until the name has room. A net whose names cannot be
    /// placed is left as wires.
    pub(super) fn plan_tags(&mut self) {
        self.plan_tags_of((0..self.ctx.design.nets.len()).collect());
    }

    /// [`plan_tags`](Self::plan_tags) for the nets of `nets`.
    pub(super) fn plan_tags_of(&mut self, nets: Vec<usize>) {
        let ctx = self.ctx;
        for net in nets {
            if !self.tagged[net] || self.local[net] == 0 {
                continue;
            }
            let mut by_block: BTreeMap<usize, Vec<(usize, usize)>> = BTreeMap::new();
            for p in self.pins.iter().filter(|p| p.net == net) {
                by_block
                    .entry(self.block_of[p.part])
                    .or_default()
                    .push((p.part, p.idx));
            }
            let mut clusters: Vec<Vec<(usize, usize)>> = by_block.into_values().collect();
            // A block with no pin that has a stub (all in a stack) has nowhere for a name.
            let has_stub = |c: &Vec<(usize, usize)>| c.iter().any(|k| self.stubs.contains_key(k));
            if clusters.iter().any(|c| !has_stub(c)) {
                self.tagged[net] = false;
                continue;
            }
            let path = |k: &(usize, usize)| ctx.design.parts[k.0].path.clone();
            for c in &mut clusters {
                c.sort_by_key(|k| (std::cmp::Reverse(self.parts[k.0].pins.len()), path(k), k.1));
            }
            clusters.sort_by_key(|c| (path(&c[0]), c[0].1));
            for cluster in &clusters {
                let keys: Vec<(usize, usize)> = cluster
                    .iter()
                    .copied()
                    .filter(|k| self.stubs.contains_key(k))
                    .collect();
                let placed = keys.iter().any(|&key| {
                    let past = self.cells_past(key, cluster);
                    self.try_tag(net, key, past, true)
                });
                let placed = placed || keys.iter().any(|&key| self.try_spur(net, key));
                if !placed {
                    // Nowhere with room: the name goes at the first stub as it is.
                    self.try_tag(net, keys[0], 0, false);
                }
            }
            self.tag_clusters.insert(net, clusters);
        }
    }

    /// Puts the name of `net` at the end of the stub of `key`, lengthening the stub while the
    /// name does not fit and until it is past `past` cells (the parts beside the pin join the
    /// wire before the name, as a person draws it); with `must_fit` false it is put at the end
    /// of the stub as it is whatever is there. Whether it was put.
    pub(super) fn try_tag(
        &mut self,
        net: usize,
        key: (usize, usize),
        past: i32,
        must_fit: bool,
    ) -> bool {
        let id = Self::net_id(net);
        let stub = &self.stubs[&key];
        let (start, heading) = (stub.end, stub.heading);
        let (dx, dy) = heading.vector();
        let reach = if must_fit {
            STUB_EXTRA_CELLS.max(past + 8)
        } else {
            0
        };
        let mut end = start;
        for k in 0..=reach {
            if k > 0 {
                // The stub goes on over free nodes only.
                let before = end;
                end = (start.0 + dx as i32 * k, start.1 + dy as i32 * k);
                if !self.grid.free_for(end, id) || self.grid.step_is_cut(before, heading) {
                    return false;
                }
            }
            if k < past {
                continue;
            }
            let tag = tag_at(self.ctx, net, self.grid.pt(end), heading);
            if must_fit && !self.rect_is_free_for(tag.rect, net) {
                continue;
            }
            if k > 0 {
                self.grid.claim_wire(start, end, id);
                self.push_wire(net, &[start, end]);
                if let Some(stub) = self.stubs.get_mut(&key) {
                    stub.end = end;
                }
            }
            self.grid.block_rect(inflate(tag.rect, TAG_CLEAR));
            self.grid.unblock(end);
            self.tag_keys.insert(key);
            self.tag_boxes.push((key.0, tag.rect));
            self.ports.extend(tag.port);
            self.labels.extend(tag.label);
            return true;
        }
        false
    }

    /// Puts the name of `net` at the end of a short spur that leaves the stub of `key` at right
    /// angles, half way along it, where nothing is room for it in line (another pin of its
    /// block is in the way). Whether it was put.
    pub(super) fn try_spur(&mut self, net: usize, key: (usize, usize)) -> bool {
        let id = Self::net_id(net);
        let stub = &self.stubs[&key];
        let (end, heading, pin) = (stub.end, stub.heading, stub.pin);
        let (hx, hy) = heading.vector();
        let mid = if Self::manhattan(end, pin) >= 2 {
            (end.0 - hx as i32, end.1 - hy as i32)
        } else {
            end
        };
        let sides = if heading.is_vertical() {
            [Dir::Left, Dir::Right]
        } else {
            [Dir::Up, Dir::Down]
        };
        for side in sides {
            let (dx, dy) = side.vector();
            for len in 2..=4 {
                let tip = (mid.0 + dx as i32 * len, mid.1 + dy as i32 * len);
                let clear = (1..=len).all(|k| {
                    let n = (mid.0 + dx as i32 * k, mid.1 + dy as i32 * k);
                    self.grid.free_for(n, id)
                        && !self
                            .grid
                            .step_is_cut((n.0 - dx as i32, n.1 - dy as i32), side)
                });
                if !clear {
                    break;
                }
                let tag = tag_at(self.ctx, net, self.grid.pt(tip), side);
                if !self.rect_is_free_for(tag.rect, net) {
                    continue;
                }
                self.grid.claim_wire(mid, tip, id);
                self.push_wire(net, &[mid, tip]);
                self.grid.block_rect(inflate(tag.rect, TAG_CLEAR));
                self.grid.unblock(tip);
                self.tag_boxes.push((key.0, tag.rect));
                self.ports.extend(tag.port);
                self.labels.extend(tag.label);
                return true;
            }
        }
        false
    }

    /// How many cells past the end of the stub of `key` the other pins of its block are, along
    /// the stub: the name goes beyond them.
    pub(super) fn cells_past(&self, key: (usize, usize), cluster: &[(usize, usize)]) -> i32 {
        let stub = &self.stubs[&key];
        let (dx, dy) = stub.heading.vector();
        cluster
            .iter()
            .filter(|&&k| k != key)
            .map(|k| {
                let n = self.stubs.get(k).map_or_else(
                    || {
                        self.pins
                            .iter()
                            .find(|p| (p.part, p.idx) == *k)
                            .map_or(stub.end, |p| p.node)
                    },
                    |s| s.end,
                );
                (n.0 - stub.end.0) * dx as i32 + (n.1 - stub.end.1) * dy as i32
            })
            .filter(|&d| d > 0)
            .map(|d| d + 2)
            .max()
            .unwrap_or(0)
    }

    /// A rail on a spine is wired along the line like any net and has one symbol, at the outer
    /// end of its wire: up from the node that is farthest the way the net's stretch of the line
    /// goes (the left end of the input side, the right end of the output), as a person draws the
    /// supply where the power comes in or goes out.
    pub(super) fn spine_symbols(&mut self) {
        for (net, pref) in self.spine_pref.clone() {
            // No symbol on this net (the source end of a link).
            if pref == 0 {
                continue;
            }
            let Some(kind) = self.ctx.power[net] else {
                continue;
            };
            let Some(tree) = self.trees.get(&net) else {
                continue;
            };
            let mut nodes: Vec<Node> = tree.iter().copied().collect();
            nodes.sort_by_key(|n| (-pref * n.0, n.1));
            let first = nodes.first().copied();
            let found = nodes
                .into_iter()
                .take(80)
                .find_map(|q| self.symbol_fits(net, kind, q).map(|label| (q, label)));
            if let Some((q, label)) = found {
                self.place_symbol(net, kind, q, Some(label));
                continue;
            }
            // No room by the wire (a long name above the part): the wire goes on outward until
            // a symbol has.
            let Some(q0) = first else {
                continue;
            };
            let id = Self::net_id(net);
            for k in 1..=40 {
                let q = (q0.0 + pref * k, q0.1);
                let before = (q.0 - pref, q.1);
                let dir = if pref > 0 { Dir::Right } else { Dir::Left };
                if !self.grid.free_for(q, id) || self.grid.step_is_cut(before, dir) {
                    break;
                }
                if let Some(label) = self.symbol_fits(net, kind, q) {
                    self.grid.claim_wire(q0, q, id);
                    self.push_wire(net, &[q0, q]);
                    self.place_symbol(net, kind, q, Some(label));
                    break;
                }
            }
        }
    }

    pub(super) fn pin_marks(&self) -> Vec<PinMark> {
        let mut out = Vec::new();
        for (p, part) in self.parts.iter().enumerate() {
            let refdes = &self.ctx.design.parts[p].refdes;
            for (i, (pin, g)) in part.pins.iter().zip(&part.glyph.pins).enumerate() {
                let _ = i;
                if g.hidden {
                    continue;
                }
                let must = self.marks.must.get(&(refdes.clone(), g.id.clone()));
                let open = match pin.net {
                    None => true,
                    Some(n) => {
                        self.local[n] <= 1 && !self.ctx.external[n] && self.ctx.power[n].is_none()
                    }
                };
                if !open && must.is_none() {
                    continue;
                }
                out.push(PinMark {
                    part: p,
                    pad: g.id.clone(),
                    at: pin.at,
                    kind: match must {
                        Some(m) => MarkKind::Must(m.clone()),
                        None => MarkKind::Open,
                    },
                    pin_name: g.name.clone(),
                    pin_kind: g.kind.to_string(),
                });
            }
        }
        out
    }

    /// Where a wire of a net may end: at one of its pins, power symbols, flags or labels.
    pub(super) fn anchors(&self) -> BTreeSet<(usize, Node)> {
        let mut out = BTreeSet::new();
        for part in &self.parts {
            for pin in &part.pins {
                if let Some(net) = pin.net {
                    out.insert((net, self.grid.node(pin.at)));
                }
            }
        }
        for s in &self.symbols {
            out.insert((s.net, self.grid.node(s.at)));
        }
        for p in &self.ports {
            out.insert((p.net, self.grid.node(p.at)));
        }
        for l in &self.labels {
            out.insert((l.net, self.grid.node(l.at)));
        }
        for b in &self.buses {
            for e in &b.entries {
                out.insert((e.net, self.grid.node(e.at)));
            }
        }
        out
    }

    pub(super) fn finish(mut self) -> Drawn {
        self.join_and_stub();
        self.plan_tags();
        self.plan_buses();
        // The rows of the spines first: the wires of the pair satellites join them where they
        // meet, and nothing else is wired along them.
        self.route_signals(true);
        self.route_pairs();
        self.route_close_rails();
        self.route_rails();
        self.route_signals(false);
        self.spine_symbols();
        let marks = self.pin_marks();
        let anchors = self.anchors();
        // Take out what no pin needs, and put a dot where three arms meet.
        let (wires, junctions) =
            prune::prune(&self.grid, std::mem::take(&mut self.wires), &anchors);
        self.wires = wires;

        let mut bounds = self
            .parts
            .first()
            .map_or(Rect::around(Pt::new(0.0, 0.0)), |p| p.extent);
        for p in &self.parts {
            bounds.union(p.extent);
        }
        for w in &self.wires {
            for &pt in &w.points {
                bounds.include(pt);
            }
        }
        for s in &self.symbols {
            bounds.union(s.extent());
        }
        for p in &self.ports {
            bounds.union(p.extent());
            if let Some(l) = &p.label {
                bounds.union(l.extent());
            }
        }
        for l in &self.labels {
            bounds.union(l.text.extent());
        }
        for b in &self.buses {
            for line in &b.lines {
                for &pt in line {
                    bounds.include(pt);
                }
            }
            for t in b.texts() {
                bounds.union(t.extent());
            }
            if let Some(f) = &b.flag {
                bounds.union(f.extent());
            }
        }
        bounds.min = Pt::new(
            snap(bounds.min.x - BORDER, GRID),
            snap(bounds.min.y - BORDER, GRID),
        );
        bounds.max = Pt::new(
            snap(bounds.max.x + BORDER, GRID),
            snap(bounds.max.y + BORDER, GRID),
        );
        Drawn {
            layout: Layout {
                parts: self.parts,
                symbols: self.symbols,
                ports: self.ports,
                marks,
                sheets: Vec::new(),
                wires: self.wires,
                junctions,
                labels: self.labels,
                buses: self.buses,
                spines: Vec::new(),
                notes: Vec::new(),
                rows: 0,
                bounds,
            },
            clusters: self.clusters,
            bus_nets: self.bus_done,
            pair_nets: self.pair_nets,
            shifts: Vec::new(),
            findings: Vec::new(),
        }
    }
}
