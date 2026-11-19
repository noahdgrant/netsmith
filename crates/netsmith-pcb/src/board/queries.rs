//! Reading a board: its parts, outline, keepouts, height zones and distances to the edge.

use super::*;

impl Board {
    /// Whether a routes file fixed a zone on `layer` (KiCad's name, `F.Cu`), of any net.
    pub fn zoned(&self, layer: &str) -> bool {
        self.zones
            .iter()
            .any(|z| z.layers.iter().any(|l| l == layer))
    }

    /// Whether a zone of the routes file of `net` is on `layer`: then the routes file restates
    /// the design's pour or plane of `net` there, which is not drawn, nor counted as joining
    /// its net. A zone of another net does not replace it: it has the area it covers, and the
    /// pour fills round it (issue #1314; [`Board::zone_lift`]).
    pub fn replaced(&self, layer: &str, net: &str) -> bool {
        self.zones
            .iter()
            .any(|z| z.net == net && z.layers.iter().any(|l| l == layer))
    }

    /// How far the zones of the routes file are raised above their own priorities in the board
    /// file: 1 when a pour or a plane of the process is drawn on a layer that has one (of
    /// another net, [`Board::replaced`]), so that each has the area it covers and the pour,
    /// written at priority 0, fills round it as KiCad's fill does (issue #1314); 0 otherwise.
    /// Every zone is raised alike, so their order among themselves is the routes file's.
    /// `routes import` takes the 1 off again.
    pub fn zone_lift(&self) -> u32 {
        let under = self
            .pours
            .iter()
            .map(|p| (p.layer.name(), p.net.as_str()))
            .chain(
                self.planes
                    .iter()
                    .map(|p| (p.layer.as_str(), p.net.as_str())),
            )
            .any(|(layer, net)| self.zoned(layer) && !self.replaced(layer, net));
        u32::from(under)
    }

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

    /// The keepout zones the parts' library footprints carry ([`crate::FootprintKeepout`]), on
    /// the board: each is the box round its outline where the part is, turned and moved with the
    /// part and on the other face for a part on the back, as the board file has it (issue
    /// #1237). A zone that is not a box with sides along the axes, or a part turned by other than
    /// a right angle, gives the box that holds it, which keeps copper out of a little more than
    /// the zone. Its layers are the layout file's words (`top`, `inner1`, `bottom`); `copper`
    /// is the zone's `copperpour` and `parts` its `footprints`. The carrier is the part.
    ///
    /// They are not keepouts of the design and are not among [`Board::keepout_areas`] (the
    /// board file has them in the footprint already, and the placer does not read them): what
    /// adds copper reads [`Board::copper_keepouts`] (issue #1291).
    pub fn footprint_keepouts(&self) -> Vec<KeepoutArea> {
        let words = crate::layout::copper_layers(self);
        let mut out = Vec::new();
        for part in &self.parts {
            for (n, k) in part.footprint.keepouts().into_iter().enumerate() {
                let Some(rect) = Rect::around(k.polygon.iter().map(|p| part.to_board(*p))) else {
                    continue;
                };
                let mut layers: Vec<String> = Vec::new();
                for name in &k.layers {
                    let named: Vec<String> = if name == "*.Cu" {
                        words.clone()
                    } else {
                        // A part on the back has the library's front copper on the back, as the
                        // board file has it.
                        let name = match (part.side, name.as_str()) {
                            (Side::Back, "F.Cu") => "B.Cu",
                            (Side::Back, "B.Cu") => "F.Cu",
                            (_, other) => other,
                        };
                        CopperLayer::of_name(name)
                            .map(|l| l.word().to_string())
                            .filter(|w| words.contains(w))
                            .into_iter()
                            .collect()
                    };
                    for w in named {
                        if !layers.contains(&w) {
                            layers.push(w);
                        }
                    }
                }
                if layers.is_empty() {
                    continue;
                }
                out.push(KeepoutArea {
                    name: format!("{}'s footprint keepout {}", part.refdes, n + 1),
                    carrier: Some(part.path.clone()),
                    rect,
                    layers,
                    tracks: k.tracks,
                    vias: k.vias,
                    copper: k.copper,
                    parts: k.footprints,
                    max_height: None,
                });
            }
        }
        out
    }

    /// The keepouts that copper added to the board keeps out of: the design's
    /// ([`Board::keepout_areas`]) and those of the parts' footprints
    /// ([`Board::footprint_keepouts`]). The router, the stitching of a pour and the vias in pads
    /// read these (issue #1291).
    pub fn copper_keepouts(&self) -> Vec<KeepoutArea> {
        let mut areas = self.keepout_areas();
        areas.extend(self.footprint_keepouts());
        areas
    }

    /// The areas under the bodies that stand off the board (`.standoff`, issue #775), each
    /// limiting the parts on the same face to the standoff, on the sheet. They are not keepouts of
    /// the board and are not among [`Board::keepout_areas`]; the checks that compare a part's
    /// height with a limit read both.
    pub fn standoff_areas(&self) -> Vec<KeepoutArea> {
        self.parts
            .iter()
            .filter_map(|p| {
                let zone = p.standoff_zone()?;
                Some(KeepoutArea {
                    name: zone.name,
                    carrier: Some(p.path.clone()),
                    rect: p.body(),
                    layers: vec![
                        if p.side == Side::Back {
                            "bottom"
                        } else {
                            "top"
                        }
                        .into(),
                    ],
                    tracks: false,
                    vias: false,
                    copper: false,
                    parts: false,
                    max_height: zone.limit,
                })
            })
            .collect()
    }

    /// The keepouts that have a place and the standoff areas: what limits the height of the
    /// parts over a place.
    pub fn height_areas(&self) -> Vec<KeepoutArea> {
        let mut areas = self.keepout_areas();
        areas.extend(self.standoff_areas());
        areas
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

    /// The keepouts of the board that forbid parts on one face only (issue #767), for the
    /// placer; those that forbid them on both are among its `keepouts`.
    pub fn face_keepouts(&self) -> Vec<crate::placement::FaceKeepout> {
        self.keepout_areas()
            .into_iter()
            .filter(|k| k.carrier.is_none())
            .filter_map(|k| {
                let (front, back) = (
                    k.forbids_parts_on(Side::Front),
                    k.forbids_parts_on(Side::Back),
                );
                (front != back).then_some(crate::placement::FaceKeepout {
                    name: k.name,
                    rect: k.rect,
                    front,
                    back,
                })
            })
            .collect()
    }

    /// The keepouts that parts carry, each in its part's own frame, for the placer.
    pub fn carried_zones(&self) -> Vec<crate::placement::CarriedZone> {
        let standoffs = self.parts.iter().filter_map(Placed::standoff_zone);
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
                    on_carrier_face: false,
                })
            })
            .chain(standoffs)
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
