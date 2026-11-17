//! What walls in a line that even a path crossing other nets' routed copper cannot reach (issue
//! #1088): the pads, the holes, the vias and the edge round the end it cannot leave, named for
//! the router's report ([`LineReport`]).
//!
//! The report names the nets a soft path crosses ([`Router::contested`]); when there is no such
//! path, nothing the router laid is in the way, and before this the report named nothing. Here
//! the cells a track of the line may use, with every net's routed copper taken away but a
//! protected line's (the masks of a soft search at the least room, [`ROOM_TIGHT`]), are flooded
//! from each end, layer to layer where a via may stand. The end whose cells do not reach the
//! other end (the smaller of the two, or the one end of a line to a plane) is walled in, and each
//! closed cell next to what it reaches is put down to the obstacle that is nearest to closing
//! it: another net's pad or its hole (a pad of the line's own part too), a via or a track the
//! routes file fixed (or a protected line's), or the edge. A cell no obstacle closes (a
//! keepout, a mask opening that is not a pad's) names nothing.

use super::*;

/// What walls in an end of a line ([`Router::walls`]).
#[derive(Default)]
pub(super) struct Walls {
    pub(super) pads: Vec<netsmith_pcb::PadRef>,
    pub(super) vias: Vec<WallVia>,
    /// The nets of the routes file's tracks (or a protected line's) in the wall.
    pub(super) nets: Vec<usize>,
    pub(super) edge: bool,
}

impl<'a> Router<'a> {
    /// The module's header, for a line left in `state`.
    pub(super) fn walls(&self, line: usize, state: &State) -> Walls {
        let net = self.nets[line];
        let protected: Vec<&Routed> = {
            let lines = self.protected(line, state);
            state
                .routed
                .iter()
                .filter(|r| lines.contains(&r.line) && self.nets[r.line] != net)
                .collect()
        };
        let (hard, hard_keepouts) = self.copper_of(&protected);
        let hard_list: Vec<&Obstacle> = hard.iter().collect();
        let window = Window::full(&self.grid);
        let masks = self.window_masks(
            line,
            window,
            ROOM_TIGHT,
            (&hard_list, &hard_keepouts),
            (&[], &[]),
        );
        let (from, to) = &self.ends[line];
        let ends: Vec<Spot> = from.iter().chain(to).copied().collect();
        let passable = |(i, j, layer): Spot| {
            !masks.track[layer][masks.at(i, j)] || ends.contains(&(i, j, layer))
        };
        let a = flood(&masks, from, &passable);
        let walled = if self.lines[line].to_plane {
            // A line to a plane ends at any cell where a via may stand.
            if a.iter().any(|&(i, j, _)| !masks.via[masks.at(i, j)]) {
                return Walls::default();
            }
            a
        } else {
            if to.iter().any(|s| a.contains(s)) {
                return Walls::default();
            }
            let b = flood(&masks, to, &passable);
            if b.len() < a.len() { b } else { a }
        };
        // The closed cells beside the walled-in cells, on their layer.
        let layers = masks.layers();
        let cells = window.cells();
        let mut inside = vec![false; cells * layers];
        for &(i, j, l) in &walled {
            inside[l * cells + window.index(i, j)] = true;
        }
        let mut wall = Vec::new();
        let mut seen = vec![false; cells * layers];
        for &(i, j, l) in &walled {
            for (di, dj) in MOVES {
                let (ni, nj) = (i as isize + di, j as isize + dj);
                if ni < 0 || nj < 0 || !window.contains(ni as usize, nj as usize) {
                    continue;
                }
                let n = (ni as usize, nj as usize, l);
                let k = l * cells + window.index(n.0, n.1);
                if !inside[k] && !seen[k] && !passable(n) {
                    seen[k] = true;
                    wall.push(n);
                }
            }
        }
        self.name_walls(line, &wall, &hard)
    }

    /// The obstacle nearest to closing each cell of `wall`, by name: the fixed obstacles of
    /// other nets and `hard`, the copper of the protected lines.
    fn name_walls(&self, line: usize, wall: &[Spot], hard: &[Obstacle]) -> Walls {
        let mut walls = Walls::default();
        let Some(bounds) = Rect::around(wall.iter().map(|&(i, j, _)| self.grid.centre(i, j)))
        else {
            return walls;
        };
        let net = self.nets[line];
        let clearance = self.clearance_of(line);
        let reach = |o: &Obstacle, layer: usize| {
            o.space(clearance) + self.width_on(line, layer) / 2.0 + ROOM_TIGHT
        };
        let far = self.print_reach;
        let candidates: Vec<(&Obstacle, bool)> = self
            .fixed
            .list
            .iter()
            .map(|o| (o, false))
            .chain(hard.iter().map(|o| (o, true)))
            .filter(|(o, _)| o.net != Some(net) || !o.copper)
            .filter(|(o, _)| o.form.bounds().expanded(far).overlaps(&bounds))
            .collect();
        let outline = &self.board.outline;
        let polygon = self.board.polygon.as_ref();
        let mut named: Vec<usize> = Vec::new();
        for &(i, j, layer) in wall {
            let p = self.grid.centre(i, j);
            if !outline.contains(&Rect::new(p, p)) || polygon.is_some_and(|s| !s.contains(p)) {
                walls.edge = true;
                continue;
            }
            let nearest = candidates
                .iter()
                .enumerate()
                .filter(|(_, (o, _))| o.layers[layer])
                .map(|(k, (o, _))| (k, o.form.distance(p) - reach(o, layer)))
                .filter(|&(_, slack)| slack < 0.0)
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            if let Some((k, _)) = nearest
                && !named.contains(&k)
            {
                named.push(k);
            }
        }
        named.sort_unstable();
        for k in named {
            let (o, routed) = candidates[k];
            self.name_obstacle(o, routed, &mut walls);
        }
        walls.pads.sort_by(|a, b| {
            (a.refdes.as_str(), a.number.as_str()).cmp(&(b.refdes.as_str(), b.number.as_str()))
        });
        walls.pads.dedup();
        walls.vias.sort_by(|a, b| {
            (a.net.as_str(), a.at.x, a.at.y)
                .partial_cmp(&(b.net.as_str(), b.at.x, b.at.y))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        walls.vias.dedup();
        walls.nets.sort_unstable();
        walls.nets.dedup();
        walls
    }

    /// Put one obstacle of the wall into `walls`: a pad or its hole by the part and pad it is,
    /// a via by its net and place, a track by its net, an edge as the edge.
    fn name_obstacle(&self, o: &Obstacle, routed: bool, walls: &mut Walls) {
        let net_name =
            |n: Option<usize>| n.map_or(String::new(), |n| self.board.nets[n].name.clone());
        match (o.form, o.copper) {
            (Form::Circle(at, _), true) => walls.vias.push(WallVia {
                net: net_name(o.net),
                at,
            }),
            (Form::Segment(_, _, _), true) => {
                if let Some(n) = o.net {
                    walls.nets.push(n);
                }
            }
            (Form::Segment(_, _, half), false) if half == 0.0 && !routed => walls.edge = true,
            (form, _) => {
                if let Some(pad) = self.pad_of(&form) {
                    walls.pads.push(pad);
                }
            }
        }
    }

    /// The pad whose copper, or whose hole, is `form`.
    fn pad_of(&self, form: &Form) -> Option<netsmith_pcb::PadRef> {
        self.board.parts.iter().find_map(|part| {
            part.footprint.pads.iter().find_map(|pad| {
                let hit = match *form {
                    Form::Rect(r) => part.pad_bounds(pad) == r,
                    Form::Segment(a, b, radius) => part.hole(pad) == Some((a, b, radius)),
                    Form::Circle(..) => false,
                };
                hit.then(|| netsmith_pcb::PadRef {
                    refdes: part.refdes.clone(),
                    number: pad.number.clone(),
                })
            })
        })
    }
}

/// The cells a track may reach from `start`, layer to layer where a via may stand.
fn flood(masks: &Masks, start: &[Spot], passable: &dyn Fn(Spot) -> bool) -> Vec<Spot> {
    let window = masks.window;
    let cells = window.cells();
    let layers = masks.layers();
    let mut seen = vec![false; cells * layers];
    let mut queue = std::collections::VecDeque::new();
    let mut out = Vec::new();
    for &s in start.iter().filter(|s| window.contains(s.0, s.1)) {
        let k = s.2 * cells + window.index(s.0, s.1);
        if !seen[k] {
            seen[k] = true;
            queue.push_back(s);
        }
    }
    while let Some((i, j, layer)) = queue.pop_front() {
        out.push((i, j, layer));
        let mut next = |n: Spot, queue: &mut std::collections::VecDeque<Spot>| {
            let k = n.2 * cells + window.index(n.0, n.1);
            if !seen[k] && passable(n) {
                seen[k] = true;
                queue.push_back(n);
            }
        };
        for (di, dj) in MOVES {
            let (ni, nj) = (i as isize + di, j as isize + dj);
            if ni < 0 || nj < 0 || !window.contains(ni as usize, nj as usize) {
                continue;
            }
            next((ni as usize, nj as usize, layer), &mut queue);
        }
        for other in (0..layers).filter(|&l| l != layer) {
            if masks.via_open(i, j, layer, other) {
                next((i, j, other), &mut queue);
            }
        }
    }
    out
}
