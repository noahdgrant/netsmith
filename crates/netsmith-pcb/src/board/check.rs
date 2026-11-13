//! The placement check: [`Board::check_placement`].

use super::*;

impl Board {
    /// Everything wrong with where the parts are: courtyards that overlap, courtyards that
    /// stick out of the outline, pads nearer the edge than `rules.copper_to_edge`, and holes
    /// nearer the holes and copper of other parts than the process allows. Empty when the
    /// placement is valid.
    ///
    /// A part the design says may hang over an edge ([`Board::overhang`]) is not reported
    /// past that edge, and takes only the room its courtyard has inside the outline: that is
    /// the part of it that overlaps other courtyards, keepouts and cutouts. Its pads are
    /// checked as ever.
    pub fn check_placement(&self) -> Vec<PlacementProblem> {
        let courtyards: Vec<Rect> = self.parts.iter().map(Placed::courtyard).collect();
        // Where a part's room is: its courtyard, clipped to the outline if it may hang over.
        let may: Vec<crate::overhang::Edges> = self
            .parts
            .iter()
            .map(|p| {
                self.overhang
                    .get(&p.refdes)
                    .map(Overhang::edges)
                    .unwrap_or_default()
            })
            .collect();
        let rooms: Vec<Rect> = courtyards
            .iter()
            .zip(&may)
            .map(|(c, m)| {
                if m.any() {
                    overhang::clip(c, &self.outline)
                } else {
                    *c
                }
            })
            .collect();
        let cutouts = self.cutout_shapes();
        let zones = self.height_zones();
        let mut problems = Vec::new();
        for c in self.cutouts.iter().filter(|c| c.at.is_none()) {
            problems.push(PlacementProblem::NoPosition {
                what: "cutout",
                name: c.name.clone(),
            });
        }
        for k in self.keepouts.iter().filter(|k| k.at.is_none()) {
            problems.push(PlacementProblem::NoPosition {
                what: "keepout",
                name: k.name.clone(),
            });
        }
        let keepouts = self.keepout_areas();
        let edge = self.edge();
        for (cutout, hole) in &cutouts {
            if edge.box_depth(&hole.rect) < -1e-6 {
                problems.push(PlacementProblem::CutoutOutsideOutline {
                    cutout: cutout.name.clone(),
                });
            }
        }
        for (i, a) in self.parts.iter().enumerate() {
            let declared = may[i];
            let past: Vec<(Edge, f64)> = overhang::past(&courtyards[i], &self.outline)
                .into_iter()
                .filter(|&(edge, _)| !declared.has(edge))
                .collect();
            // The corners of the room, but not those on an edge the part may hang over, which
            // are the board's edge and not its rounded corners.
            let o = &self.outline;
            let (in_corner, cut) = match &self.polygon {
                // A polygon's edge is measured along all of it, not only at the room's
                // corners, which may all be inside an L while its inner corner is in the room.
                // Past a side of the box the design allows, the room is clipped to the box.
                Some(polygon) => {
                    let depth = polygon.box_depth(&rooms[i]);
                    (depth < -1e-6, (depth < -1e-6).then_some(-depth))
                }
                None => {
                    let shape = self.outline_shape();
                    let in_corner = rooms[i].corners().iter().any(|&c| {
                        let on_edge = (declared.left && c.x <= o.min.x + 1e-6)
                            || (declared.right && c.x >= o.max.x - 1e-6)
                            || (declared.top && c.y <= o.min.y + 1e-6)
                            || (declared.bottom && c.y >= o.max.y - 1e-6);
                        !on_edge && !shape.contains(c)
                    });
                    (in_corner, None)
                }
            };
            if !past.is_empty() || in_corner {
                let stated = self.overhang.get(&a.refdes);
                problems.push(PlacementProblem::OutsideOutline {
                    refdes: a.refdes.clone(),
                    path: a.path.clone(),
                    past,
                    stated: declared.list(),
                    by: stated.map(Overhang::by).unwrap_or_default(),
                    cut,
                });
            }
            for k in keepouts
                .iter()
                .filter(|k| k.carrier.as_deref() != Some(a.path.as_str()))
            {
                if k.parts && rooms[i].overlaps(&k.rect) {
                    problems.push(PlacementProblem::InKeepout {
                        refdes: a.refdes.clone(),
                        path: a.path.clone(),
                        keepout: k.name.clone(),
                    });
                }
                let trapped = k.tracks
                    && a.footprint.pads.iter().any(|p| {
                        let (front, back) = p.copper_layers();
                        ((front && k.on(CopperLayer::Front)) || (back && k.on(CopperLayer::Back)))
                            && a.pad_bounds(p).overlaps(&k.rect)
                    });
                if trapped {
                    problems.push(PlacementProblem::PadInKeepout {
                        refdes: a.refdes.clone(),
                        path: a.path.clone(),
                        keepout: k.name.clone(),
                    });
                }
            }
            // A part taller than the limit of a keepout it stands under, on its face. The body
            // is what stands there (the footprint's fabrication outline, else its courtyard).
            if let Some(height) = a.height {
                let body = a.body();
                for k in keepouts
                    .iter()
                    .filter(|k| k.carrier.as_deref() != Some(a.path.as_str()))
                {
                    let Some(limit) = k.max_height else { continue };
                    if k.limits_height_on(a.side)
                        && body.overlaps(&k.rect)
                        && height > limit + HEIGHT_TOLERANCE
                    {
                        let centre = body.centre();
                        problems.push(PlacementProblem::TooTall {
                            refdes: a.refdes.clone(),
                            path: a.path.clone(),
                            keepout: k.name.clone(),
                            height,
                            limit,
                            side: a.side,
                            at: (centre.x - self.outline.min.x, centre.y - self.outline.min.y),
                        });
                    }
                }
            }
            for (cutout, hole) in &cutouts {
                if rooms[i].overlaps(&hole.rect) {
                    problems.push(PlacementProblem::InCutout {
                        refdes: a.refdes.clone(),
                        path: a.path.clone(),
                        cutout: cutout.name.clone(),
                    });
                }
                let nearest = a
                    .footprint
                    .pads
                    .iter()
                    .filter(|p| p.copper_layers() != (false, false))
                    .map(|p| rect_distance(&a.pad_bounds(p), &hole.rect))
                    .fold(f64::MAX, f64::min);
                if nearest < self.rules.copper_to_edge - 1e-9 && !rooms[i].overlaps(&hole.rect) {
                    problems.push(PlacementProblem::PadNearCutout {
                        refdes: a.refdes.clone(),
                        path: a.path.clone(),
                        cutout: cutout.name.clone(),
                        distance: nearest,
                    });
                }
            }
            let nearest = a
                .footprint
                .pads
                .iter()
                .filter(|p| a.pad_layers(p) != (false, false))
                .map(|p| self.pad_edge_distance(a, p))
                .fold(f64::MAX, f64::min);
            if nearest < self.rules.copper_to_edge - 1e-9 {
                problems.push(PlacementProblem::PadNearEdge {
                    refdes: a.refdes.clone(),
                    path: a.path.clone(),
                    distance: nearest,
                });
            }
            for (j, b) in self.parts.iter().enumerate().skip(i + 1) {
                // Parts on opposite faces may share the board's area, unless one has a hole
                // through the board that takes room there (`Placed::room_on`).
                if a.rooms_overlap(rooms[i], b, rooms[j], self.rules.clearance, &zones) {
                    problems.push(PlacementProblem::overlap_of(
                        a,
                        b,
                        self.rules.clearance,
                        &zones,
                    ));
                }
                self.check_holes(a, b, true, &mut problems);
                self.check_holes(b, a, false, &mut problems);
            }
        }
        problems
    }
}
