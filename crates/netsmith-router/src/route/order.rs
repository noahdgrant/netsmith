//! The coarse plan, the orders of the lines and the tries.

use super::*;

impl<'a> Router<'a> {
    /// The coarse plan of the board ([`crate::plan`]), with the lines in `order`: the tiles'
    /// capacities from what the fixed copper leaves open to the narrowest line's track at the
    /// room the first pass keeps, and each line's corridor.
    pub(super) fn make_plan(&self, order: &[usize]) -> Planned {
        let tile_cells = crate::plan::TILE_CELLS;
        let rules = &self.board.rules;
        let width = (0..self.lines.len())
            .filter(|&l| !self.lines[l].to_plane)
            .map(|l| self.width_of(l))
            .fold(f64::INFINITY, f64::min);
        let width = if width.is_finite() {
            width
        } else {
            rules.min_track_width
        };
        let reach = Reach {
            room: ROOM_LOOSE,
            width,
        };
        let key = (reach.room.to_bits(), reach.width.to_bits());
        let base = self
            .bases
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                std::rc::Rc::new(Base::new(self.board, &self.grid, &self.fixed, reach))
            })
            .clone();
        let counts = &base.counts;
        let open = |c: &Vec<u16>| c.iter().map(|&n| n == 0).collect::<Vec<bool>>();
        let (mut front, mut back, via) = (
            open(&counts.track[FRONT]),
            open(&counts.track[BACK]),
            open(&counts.via),
        );
        // A pad is a way across a tile's edge for the line of its net that ends in it: the
        // cells of a pad's copper count as open for tracks.
        for o in self.fixed.list.iter().filter(|o| o.net.is_some()) {
            let whole = Window::full(&self.grid);
            for (i, j) in self.grid.cells_near(&whole, &o.form.bounds(), 0.0) {
                if o.form.distance(self.grid.centre(i, j)) <= 1e-9 {
                    let at = j * self.grid.nx + i;
                    if o.layers[FRONT] {
                        front[at] = true;
                    }
                    if o.layers[BACK] {
                        back[at] = true;
                    }
                }
            }
        }
        let via_pitch = (rules.via_diameter + rules.clearance)
            .max(rules.via_drill + rules.via_hole_to_hole)
            / PITCH;
        let tiles = crate::plan::Tiles::measure(
            tile_cells,
            (self.grid.nx, self.grid.ny),
            [&front, &back],
            &via,
            (width + rules.clearance) / PITCH,
            via_pitch,
        );
        let at = |spots: &[Spot]| {
            let mut nodes: Vec<crate::plan::Node> = spots
                .iter()
                .map(|&(i, j, layer)| {
                    let (a, b) = tiles.of(i, j);
                    (a, b, layer)
                })
                .collect();
            nodes.sort_unstable();
            nodes.dedup();
            nodes
        };
        let ends: Vec<Option<crate::plan::Ends>> = (0..self.lines.len())
            .map(|l| {
                let (from, to) = &self.ends[l];
                (!self.lines[l].to_plane && !from.is_empty() && !to.is_empty()).then(|| {
                    crate::plan::Ends {
                        from: at(from),
                        to: at(to),
                        net: self.nets[l],
                    }
                })
            })
            .collect();
        let plan = crate::plan::negotiate(&tiles, &ends, order);
        let planned_vias: usize = plan
            .paths
            .iter()
            .flatten()
            .map(|p| p.windows(2).filter(|w| w[0].2 != w[1].2).count())
            .sum();
        self.report(true, || {
            format!(
                "coarse plan: {} by {} tiles of {} mm, {} rounds, {} tracks over capacity, {} \
                 crossings, {planned_vias} vias",
                tiles.nx,
                tiles.ny,
                tile_cells as f64 * PITCH,
                plan.rounds,
                plan.overuse,
                plan.crossings
            )
        });
        // The plan steers a line of the narrowest track only: the capacities are counted in it,
        // so a net its current widens has no corridor. Nor has a net of a pour, whose copper
        // the pour joins anyway, or a net of a diff pair, which runs beside its partner (S9)
        // and which any cost moves off it.
        let poured = |l: usize| self.board.pours.iter().any(|p| p.net == self.lines[l].net);
        let corridors: Vec<Option<[Vec<bool>; 2]>> = plan
            .paths
            .iter()
            .enumerate()
            .map(|(l, p)| {
                p.as_ref()
                    .filter(|_| self.width_of(l) <= width + 1e-9)
                    .filter(|_| !poured(l) && self.partners[self.nets[l]].is_none())
                    .map(|p| {
                        crate::plan::corridor(
                            &tiles,
                            p,
                            crate::plan::CORRIDOR_GROW,
                            crate::plan::CORRIDOR_ON_PLAN_LAYER,
                        )
                    })
            })
            .collect();
        let fill = plan.fill(&tiles);
        Planned {
            tiles,
            corridors,
            fill,
        }
    }

    /// The orders to try the lines in.
    pub(super) fn orders(&self, retry: bool) -> Vec<Vec<usize>> {
        let mut shortest: Vec<usize> = (0..self.routed_lines).collect();
        shortest.sort_by(|&a, &b| self.lines[a].length().total_cmp(&self.lines[b].length()));
        // Lines that end deep in a fine-pitch grid of pads first, the deepest first, and the
        // others shortest first. It differs from `shortest` only on a board that has such a
        // grid.
        let mut constrained = shortest.clone();
        constrained.sort_by(|&a, &b| self.depth[b].total_cmp(&self.depth[a]));
        let mut orders = vec![constrained];
        if !orders.contains(&shortest) {
            orders.push(shortest.clone());
        }
        if retry {
            let mut longest = shortest.clone();
            longest.reverse();
            let mut by_net = shortest.clone();
            by_net.sort_by_key(|&l| self.nets[l]);
            let mut reversed_nets = by_net.clone();
            reversed_nets.reverse();
            for o in [longest, by_net, reversed_nets] {
                if !orders.contains(&o) {
                    orders.push(o);
                }
            }
        }
        orders.truncate(MOST_TRIES);
        orders
    }

    /// The order of each try: [`Self::orders`], with the first order again second on a board
    /// with escape lanes or a later-try plan or classes, and the classes in turn from the try
    /// [`RouteOptions::classes`] says ([`Self::classed_order`]); the first try is never changed
    /// by the classes when they come later.
    pub(super) fn tries(&self, options: &RouteOptions) -> Vec<Vec<usize>> {
        let mut orders = self.orders(options.retry_orders);
        // On a board with escape lanes, the second try is the first order again with the lanes
        // (see the module's header), and every try after it has them too. A plan used from the
        // second try on is used there too.
        let second_again = self.lanes.is_some()
            || options.plan == CoarsePlan::FromSecondTry
            || options.classes == LineClasses::FromSecondTry;
        if second_again && options.retry_orders {
            orders.insert(1, orders[0].clone());
        }
        // The tries with classes route them in turn (issue #416), each in its own order.
        let first_classed = match options.classes {
            LineClasses::Off => orders.len(),
            LineClasses::EveryTry => 0,
            LineClasses::FromSecondTry => 1,
        };
        for order in orders.iter_mut().skip(first_classed) {
            *order = self.classed_order(order);
        }
        let mut seen: Vec<Vec<usize>> = Vec::new();
        let mut k = 0;
        orders.retain(|o| {
            k += 1;
            // The first try, and the first order again on the second, are kept as they are.
            let keep = k == 1 || (k == 2 && second_again) || !seen.contains(o);
            seen.push(o.clone());
            keep
        });
        orders
    }

    /// `order` with the classes in turn, the most important first, and its own order inside
    /// each ([`Class`], issue #416).
    pub(super) fn classed_order(&self, order: &[usize]) -> Vec<usize> {
        let mut order = order.to_vec();
        order.sort_by_key(|&l| self.class[l]);
        order
    }
}
