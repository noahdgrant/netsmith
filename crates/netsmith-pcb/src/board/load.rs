//! Building a board from a design: [`Board::load`] and [`Board::unplaced`].

use super::*;

impl Board {
    /// The part of [`Board::unplaced`] before the requirements are turned into
    /// places: the process and its rules, the options with the keepouts, cutouts and height
    /// limits of the design added, and the parts as footprints on the faces they were given.
    pub fn load(
        design: &Design,
        mapping: &Mapping,
        library: &FootprintLibrary,
        placement: &PlaceOptions,
    ) -> Result<Loaded, BoardError> {
        design.validate()?;
        if let Some(refdes) = placement
            .sides
            .keys()
            .find(|r| !design.parts.iter().any(|p| &p.refdes == *r))
        {
            return Err(BoardError::NoPart {
                refdes: refdes.clone(),
            });
        }
        let process = design.process.clone().ok_or(BoardError::NoProcess {
            board: design.name.clone(),
        })?;
        let rules = Rules::for_process(&process).map_err(BoardError::Process)?;
        if process.stackup.chosen_thickness().is_none() {
            let (lo, hi) = process.stackup.thickness;
            return Err(BoardError::Process(format!(
                "`{}` has not chosen a thickness: the process `{}` offers {lo} mm to {hi} mm, so say which with `.process.stackup.thickness &= 1.6mm`",
                design.name, process.name
            )));
        }

        // The placer keeps clear of the cutouts and the keepouts for parts that the source
        // places, whether or not the caller said so.
        let mut placement = placement.clone();
        placement.wall = Some(rules.edge_wall());
        for rect in crate::layout::clear_of(design, None) {
            if !placement.keepouts.contains(&rect) {
                placement.keepouts.push(rect);
            }
        }
        for rect in crate::layout::cutouts_of(design, None) {
            if !placement.cutouts.contains(&rect) {
                placement.cutouts.push(rect);
            }
        }
        for zone in crate::layout::height_zones(design, None) {
            if !placement.height_zones.contains(&zone) {
                placement.height_zones.push(zone);
            }
        }
        for zone in crate::layout::carried_zones(design) {
            if !placement.carried.contains(&zone) {
                placement.carried.push(zone);
            }
        }
        let placement = placement.clone();

        let mut loaded: HashMap<String, Footprint> = HashMap::new();
        let mut parts = Vec::with_capacity(design.parts.len());
        let links = netsmith_kicad::Hierarchy::of(design).links(design);
        for (part, symbol) in design.parts.iter().zip(links) {
            // A label is text, not a footprint of a library.
            if part.kind == "Label" {
                let height = design
                    .features
                    .labels
                    .iter()
                    .find(|l| l.path == part.path)
                    .and_then(|l| l.text_height)
                    .unwrap_or(rules.min_text_height);
                parts.push(Placed {
                    refdes: part.refdes.clone(),
                    path: part.path.clone(),
                    kind: part.kind.clone(),
                    provides: part.provides.clone(),
                    class: part.class,
                    value: part.value.clone(),
                    footprint: Footprint::label(&part.value, height, rules.min_text_thickness),
                    power_pads: Vec::new(),
                    library_id: "Label".to_string(),
                    at: Pt::default(),
                    rotation: 0.0,
                    side: placement
                        .sides
                        .get(&part.refdes)
                        .copied()
                        .unwrap_or_default(),
                    reference_side: ReferenceSide::default(),
                    height: None,
                    protrusion: None,
                    hand_fitted: true,
                    symbol,
                });
                continue;
            }
            let mapped = mapping
                .map(&part.kind, part.package.as_deref())
                .map_err(|error| BoardError::Map {
                    refdes: part.refdes.clone(),
                    error,
                })?;
            if !loaded.contains_key(&mapped.footprint) {
                let footprint =
                    library
                        .load(&mapped.footprint)
                        .map_err(|error| BoardError::Footprint {
                            refdes: part.refdes.clone(),
                            error,
                        })?;
                loaded.insert(mapped.footprint.clone(), footprint);
            }
            parts.push(Placed {
                refdes: part.refdes.clone(),
                path: part.path.clone(),
                kind: part.kind.clone(),
                provides: part.provides.clone(),
                class: part.class,
                value: part.value.clone(),
                footprint: loaded[&mapped.footprint].clone(),
                power_pads: part
                    .pins
                    .iter()
                    .filter(|pin| pin.kind == netsmith_core::PinKind::Power)
                    .flat_map(|pin| pin.pads.iter().cloned())
                    .collect(),
                library_id: mapped.footprint,
                at: Pt::default(),
                rotation: 0.0,
                side: placement
                    .sides
                    .get(&part.refdes)
                    .copied()
                    .unwrap_or_default(),
                reference_side: ReferenceSide::default(),
                height: design
                    .features
                    .heights
                    .iter()
                    .find(|h| h.path == part.path)
                    .map(|h| h.height),
                protrusion: design
                    .features
                    .protrusions
                    .iter()
                    .find(|t| t.path == part.path)
                    .map(|t| crate::protrusion::Protrusion {
                        length: t.length,
                        source: t.source.clone(),
                    }),
                hand_fitted: design.features.hand_fitted.contains(&part.path),
                symbol,
            });
        }
        Ok(Loaded {
            process,
            rules,
            placement,
            parts,
        })
    }

    /// The board of `design` before its parts are placed, with what is known about where they
    /// go (the face each is on, the size the board must be): the footprints on the faces they
    /// were given, the requirements of the physical file turned into what the placer is told
    /// (ADR 0045), the nets and the planes. What is wrong with the design is refused here,
    /// before any placing. The placer (`netsmith-placer`) places the parts and makes the
    /// board with [`Unplaced::into_board`].
    pub fn unplaced(
        design: &Design,
        mapping: &Mapping,
        library: &FootprintLibrary,
        placement: &PlaceOptions,
    ) -> Result<Unplaced, BoardError> {
        let Loaded {
            process,
            rules,
            placement,
            mut parts,
        } = Self::load(design, mapping, library, placement)?;

        // What the design requires of where the parts go (ADR 0045): pinned parts are fixed
        // like locked ones, the others get a want the placer keeps to, and every constrained
        // part keeps the face it was given.
        let base = placement.clone();
        let mut placement = placement.clone();
        let plan = crate::constraints::plan(design, &parts, &placement, rules.clearance)
            .map_err(BoardError::Constraints)?;
        // What the lines prove before any placing, in milliseconds: lines that cannot all
        // hold, a courtyard past an edge nobody stated, a part bigger than the outline. The
        // placer is not started for a board that cannot be placed.
        let pre = crate::explain::before_placing(
            design,
            &parts,
            &base,
            &plan,
            &rules,
            outline_box(design),
        );
        let pre = if base.lines_warn {
            pre.lines_as_warnings()
        } else {
            pre
        };
        if pre.is_error() {
            return Err(BoardError::Unplaceable(Box::new(pre)));
        }
        // A part may hang over an edge only where its own requirements say so; a lock in the
        // layout file does not (owner decision, 2026-11-03, ADR 0045 section 10).
        let mut overhang = placement.overhang.clone();
        overhang.extend(plan.overhang);
        placement.overhang = overhang.clone();
        let pinned: std::collections::BTreeSet<String> = plan.fixed.keys().cloned().collect();
        placement.fixed.extend(plan.fixed);
        placement.wants = plan.wants;
        placement.offsets = plan.offsets;
        for part in &mut parts {
            if let Some(&side) = plan.sides.get(&part.refdes) {
                part.side = side;
                placement.sides.insert(part.refdes.clone(), side);
            }
        }

        let mut nets = Vec::with_capacity(design.nets.len());
        for net in &design.nets {
            let mut pads = Vec::with_capacity(net.pins.len());
            for pin in &net.pins {
                let part = parts
                    .iter()
                    .find(|p| p.refdes == pin.refdes)
                    .expect("the design was validated, so every pin names a part");
                if part.footprint.pads_numbered(&pin.pin).next().is_none() {
                    return Err(BoardError::NoPad {
                        net: net.name.clone(),
                        refdes: pin.refdes.clone(),
                        pad: pin.pin.clone(),
                        footprint: part.library_id.clone(),
                        available: part
                            .footprint
                            .pad_numbers()
                            .into_iter()
                            .map(str::to_string)
                            .collect(),
                    });
                }
                pads.push(PadRef {
                    refdes: pin.refdes.clone(),
                    number: pin.pin.clone(),
                });
            }
            nets.push(BoardNet {
                name: net.name.clone(),
                pads,
            });
        }

        let planes: Vec<Plane> = process
            .stackup
            .inner
            .iter()
            .flat_map(|inner| {
                inner.planes().map(|(n, net)| Plane {
                    layer: format!("In{n}.Cu"),
                    net: net.to_string(),
                })
            })
            .collect();
        if let Some(plane) = planes
            .iter()
            .find(|p| !nets.iter().any(|n| n.name == p.net))
        {
            return Err(BoardError::Process(format!(
                "the inner layer {} is a plane on the net `{}`, but `{}` has no net of that name",
                plane.layer, plane.net, design.name
            )));
        }

        // A polygon outline (issue #57) is checked here, before any placing.
        let polygon = match &design.features.outline {
            Some(o) => crate::polygon::of_outline(o)
                .map_err(|e| BoardError::Outline(format!("{}: {e}", design.name)))?
                .map(|p| p.translated(ORIGIN)),
            None => None,
        };

        Ok(Unplaced {
            process,
            rules,
            base,
            placement,
            parts,
            nets,
            planes,
            overhang,
            pinned,
            polygon,
        })
    }
}
