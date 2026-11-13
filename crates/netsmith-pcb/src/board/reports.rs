//! What a board says about its placement: hangs, trims, excused courtyards and the height checks.

use super::*;

impl Board {
    /// The requirements of `design`'s physical file that the parts are not at, and the lines
    /// that cannot all hold where that is proved (ADR 0045): what to say beside
    /// the faults [`Board::check_placement`] finds when the placer could not meet them.
    /// `options` is what the layout file and the caller said ([`place_options`]). `None` when
    /// the design states no requirement or all are met.
    pub fn explain_requirements(
        &self,
        design: &Design,
        options: &PlaceOptions,
    ) -> Option<crate::explain::RequirementReport> {
        crate::explain::explain(design, &self.parts, self.outline, options, &self.rules)
    }

    /// The courtyards that are past an edge of the outline, where the design says the part may
    /// hang over it, with how far: what [`Board::check_placement`] leaves out for a stated
    /// overhang, so that a person or a fabrication note can still read it.
    pub fn hangs(&self) -> Vec<Hang> {
        let mut found = Vec::new();
        for part in &self.parts {
            let Some(stated) = self.overhang.get(&part.refdes) else {
                continue;
            };
            let may = stated.edges();
            for (edge, by) in overhang::past(&part.courtyard(), &self.outline) {
                if may.has(edge) {
                    found.push(Hang {
                        refdes: part.refdes.clone(),
                        path: part.path.clone(),
                        edge,
                        by,
                        stated: stated.by(),
                    });
                }
            }
        }
        found
    }

    /// The through-hole parts whose pin protrusion the design states
    /// (ADR 0045 section 15), each with the parts that lie over them. The
    /// manufacturing step they stand for is the person's to do.
    pub fn trims(&self) -> Vec<Trim> {
        let zones = self.height_zones();
        let mut found = Vec::new();
        for t in &self.parts {
            let Some(protrusion) = &t.protrusion else {
                continue;
            };
            if !t.has_hole() {
                continue;
            }
            let reach: Vec<Rect> = t
                .footprint
                .pads
                .iter()
                .map(|p| t.pad_bounds(p).expanded(self.rules.clearance))
                .collect();
            let mut covers = Vec::new();
            let mut unlimited = Vec::new();
            for c in &self.parts {
                if crate::protrusion::pins_under(t, c).is_none() {
                    continue;
                }
                let court = c.courtyard();
                if !reach.iter().any(|r| r.overlaps(&court)) {
                    continue;
                }
                covers.push((c.refdes.clone(), c.path.clone()));
                if let (_, crate::protrusion::Verdict::Excused { limit: None }) = (
                    (),
                    crate::protrusion::judge(
                        protrusion.length,
                        &crate::protrusion::cover_of(c),
                        &zones,
                    ),
                ) {
                    unlimited.push((c.refdes.clone(), c.path.clone()));
                }
            }
            found.push(Trim {
                refdes: t.refdes.clone(),
                path: t.path.clone(),
                length: protrusion.length,
                source: protrusion.source.clone(),
                face: if t.side == Side::Front {
                    Side::Back
                } else {
                    Side::Front
                },
                covers,
                unlimited,
            });
        }
        found
    }

    /// The pairs of parts, by designator, whose courtyards overlap only because one stands
    /// over the other's protruding pins and the rule of [`crate::protrusion`] excuses it, for the
    /// design rules check's courtyard rule.
    pub fn excused_courtyards(&self) -> Vec<(String, String)> {
        let zones = self.height_zones();
        let mut pairs = Vec::new();
        for trim in self.trims() {
            let Some(t) = self.part(&trim.refdes) else {
                continue;
            };
            for (cover, _) in &trim.covers {
                let Some(c) = self.part(cover) else { continue };
                if crate::protrusion::excused(t, c, &zones).is_some() {
                    pairs.push((trim.refdes.clone(), cover.clone()));
                }
            }
        }
        pairs
    }

    /// The parts that stand under a height limit and state no height, so that whether they
    /// fit is not known (ADR 0045, "Height limits"). Not a pass: each is named,
    /// with the keepout and its limit. A label has no body and is left out.
    ///
    /// A part whose footprint has a _Standard height_ (ADR 0106) is unchecked only where the
    /// standard's maximum is over the limit; where it is not, the part is a [`StandardPass`]
    /// ([`Board::standard_passes`]).
    pub fn heights_unchecked(&self) -> Vec<HeightUnchecked> {
        self.under_limits_without_height()
            .into_iter()
            .filter(|(_, limit, standard)| {
                standard.is_none_or(|s| s.max > limit + HEIGHT_TOLERANCE)
            })
            .map(|(u, limit, standard)| HeightUnchecked {
                refdes: u.refdes.clone(),
                path: u.path.clone(),
                keepout: u.keepout.clone(),
                limit,
                side: u.side,
                standard: standard.copied(),
            })
            .collect()
    }

    /// The parts under a height limit that state no height and fit it by their package
    /// standard's maximum (ADR 0106). Each is a pass to be reported as one, not a silent pass.
    pub fn standard_passes(&self) -> Vec<StandardPass> {
        self.under_limits_without_height()
            .into_iter()
            .filter_map(|(u, limit, standard)| {
                let s = standard.filter(|s| s.max <= limit + HEIGHT_TOLERANCE)?;
                Some(StandardPass {
                    refdes: u.refdes.clone(),
                    path: u.path.clone(),
                    keepout: u.keepout.clone(),
                    limit,
                    side: u.side,
                    standard: *s,
                })
            })
            .collect()
    }

    /// Each pair of a part with no height of its own and a height limit over its body on its
    /// face, with the limit and the part's standard height, if any. A label is left out.
    pub(super) fn under_limits_without_height(
        &self,
    ) -> Vec<(
        UnderLimit,
        f64,
        Option<&'static crate::standard_height::StandardHeight>,
    )> {
        let keepouts = self.keepout_areas();
        let mut found = Vec::new();
        for a in self
            .parts
            .iter()
            .filter(|a| a.height.is_none() && a.kind != "Label")
        {
            let body = a.body();
            for k in keepouts
                .iter()
                .filter(|k| k.carrier.as_deref() != Some(a.path.as_str()))
            {
                if let Some(limit) = k.max_height
                    && k.limits_height_on(a.side)
                    && body.overlaps(&k.rect)
                {
                    found.push((
                        UnderLimit {
                            refdes: a.refdes.clone(),
                            path: a.path.clone(),
                            keepout: k.name.clone(),
                            side: a.side,
                        },
                        limit,
                        a.standard_height(),
                    ));
                }
            }
        }
        found
    }
}
