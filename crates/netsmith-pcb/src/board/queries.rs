//! Reading a board: its parts, outline, keepouts, height zones and distances to the edge.

use super::*;

impl Board {
    pub fn part(&self, refdes: &str) -> Option<&Placed> {
        self.parts.iter().find(|p| p.refdes == refdes)
    }

    /// The outline's box with its rounded corners: the board edge, unless the board has a
    /// polygon outline, whose bounding box it then is. What measures the edge uses
    /// [`Board::edge`].
    pub fn outline_shape(&self) -> RoundRect {
        RoundRect::new(self.outline, self.corner_radius)
    }

    /// The board edge, whichever shape it is.
    pub fn edge(&self) -> crate::polygon::BoardEdge {
        match &self.polygon {
            Some(p) => crate::polygon::BoardEdge::Polygon(p.clone()),
            None => crate::polygon::BoardEdge::Round(self.outline_shape()),
        }
    }

    /// The part of the outline's box that is outside a polygon outline, as boxes the placer
    /// keeps courtyards out of. Empty for a rounded rectangle.
    pub fn outside_boxes(&self) -> Vec<Rect> {
        self.polygon
            .as_ref()
            .map(crate::polygon::Polygon::outside_boxes)
            .unwrap_or_default()
    }

    /// The cutouts that have a place, as shapes on the sheet.
    pub fn cutout_shapes(&self) -> Vec<(&BoardCutout, RoundRect)> {
        self.cutouts
            .iter()
            .filter_map(|c| {
                let min = self.outline.min + c.at?;
                Some((
                    c,
                    RoundRect::new(
                        Rect::new(min, Pt::new(min.x + c.size.0, min.y + c.size.1)),
                        c.radius,
                    ),
                ))
            })
            .collect()
    }

    /// The keepouts that have a place, on the sheet. One a part carries moves and turns with
    /// the part (a turn that is not a right angle gives the box that holds it).
    pub fn keepout_areas(&self) -> Vec<KeepoutArea> {
        self.keepouts
            .iter()
            .filter_map(|k| {
                let at = k.at?;
                let local = Rect::new(at, Pt::new(at.x + k.size.0, at.y + k.size.1));
                let rect = match &k.part {
                    None => local.translated(self.outline.min),
                    Some(path) => {
                        let part = self.parts.iter().find(|p| &p.path == path)?;
                        Rect::around(local.corners().map(|c| part.to_board(c)))?
                    }
                };
                Some(KeepoutArea {
                    name: k.name.clone(),
                    carrier: k.part.clone(),
                    rect,
                    layers: k.layers.clone(),
                    tracks: k.tracks,
                    vias: k.vias,
                    copper: k.copper,
                    parts: k.parts,
                    max_height: k.max_height,
                })
            })
            .collect()
    }

    /// The areas that limit how tall a part may stand, for the placer.
    pub fn height_zones(&self) -> Vec<crate::placement::HeightZone> {
        self.keepout_areas()
            .into_iter()
            .filter(|k| k.carrier.is_none())
            .filter_map(|k| {
                Some(crate::placement::HeightZone {
                    front: k.limits_height_on(Side::Front),
                    back: k.limits_height_on(Side::Back),
                    limit: k.max_height?,
                    name: k.name,
                    rect: k.rect,
                })
            })
            .collect()
    }

    /// The keepouts that parts carry, each in its part's own frame, for the placer.
    pub fn carried_zones(&self) -> Vec<crate::placement::CarriedZone> {
        self.keepouts
            .iter()
            .filter_map(|k| {
                let at = k.at?;
                let on = |layer: &str| k.layers.iter().any(|l| l == layer);
                Some(crate::placement::CarriedZone {
                    name: k.name.clone(),
                    part: k.part.clone()?,
                    local: Rect::new(at, Pt::new(at.x + k.size.0, at.y + k.size.1)),
                    parts: k.parts,
                    front: on("top"),
                    back: on("bottom"),
                    limit: k.max_height,
                })
            })
            .collect()
    }

    /// How far a box is from the board edge: negative if any of it is outside.
    pub fn edge_distance(&self, r: &Rect) -> f64 {
        if let Some(p) = &self.polygon {
            return p.box_depth(r);
        }
        let o = &self.outline;
        let straight = (r.min.x - o.min.x)
            .min(r.min.y - o.min.y)
            .min(o.max.x - r.max.x)
            .min(o.max.y - r.max.y);
        if self.corner_radius == 0.0 {
            return straight;
        }
        // A box inside a convex shape is nearest the edge at one of its corners.
        let shape = self.outline_shape();
        r.corners()
            .iter()
            .map(|&c| shape.depth(c))
            .fold(straight, f64::min)
    }

    /// How far a pad's copper is from the board edge: negative if any of it is outside. A round
    /// pad is measured as the circle it is, so that a plated mounting hole near a rounded
    /// corner is not taken for a pad whose box corner nearly touches the edge; any other pad is
    /// measured by its box.
    pub(super) fn pad_edge_distance(&self, a: &Placed, pad: &Pad) -> f64 {
        let round = pad.shape == crate::footprint::PadShape::Circle
            && pad.extent.is_none()
            && (pad.width - pad.height).abs() < 1e-9;
        if round {
            return self.edge().depth(a.pad_centre(pad)) - pad.width / 2.0;
        }
        self.edge_distance(&a.pad_bounds(pad))
    }

    /// The net a pad is on, if any.
    pub fn net_of(&self, refdes: &str, number: &str) -> Option<&BoardNet> {
        self.nets.iter().find(|n| {
            n.pads
                .iter()
                .any(|p| p.refdes == refdes && p.number == number)
        })
    }

    /// The holes of `a` that are nearer a hole or the copper of `b` than the rules allow.
    ///
    /// `holes` says whether the distance between two holes is measured: it is the same from
    /// either part, so a pair is checked from one of them only; the distance from a hole to
    /// the other part's copper is not the same both ways.
    pub(super) fn check_holes(
        &self,
        a: &Placed,
        b: &Placed,
        holes: bool,
        problems: &mut Vec<PlacementProblem>,
    ) {
        for pad in &a.footprint.pads {
            let Some((p, q, radius)) = a.hole(pad) else {
                continue;
            };
            let mut report = |to, other_pad: &str, distance: f64, least: f64| {
                if distance < least - 1e-9 {
                    problems.push(PlacementProblem::HoleTooNear {
                        refdes: a.refdes.clone(),
                        path: a.path.clone(),
                        other_path: b.path.clone(),
                        pad: pad.number.clone(),
                        other: b.refdes.clone(),
                        other_pad: other_pad.to_string(),
                        to,
                        distance,
                        least,
                    });
                }
            };
            for other in &b.footprint.pads {
                if let (true, Some((r, s, other_radius))) = (holes, b.hole(other)) {
                    let apart = segment_distance((p, q), (r, s)) - radius - other_radius;
                    report(
                        HoleNeighbour::Hole,
                        &other.number,
                        apart,
                        self.rules.hole_to_hole,
                    );
                }
                if b.pad_layers(other) != (false, false) {
                    let apart = segment_to_rect((p, q), &b.pad_bounds(other)) - radius;
                    report(
                        HoleNeighbour::Copper,
                        &other.number,
                        apart,
                        self.rules.clearance,
                    );
                }
            }
        }
    }
}
