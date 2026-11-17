//! Realising a path as tracks and vias.

use super::*;

impl<'a> Router<'a> {
    /// The pads of a net, each as the square a round pad of its size holds (where a corner is
    /// inside the pad's copper whatever its shape), with the layers it has copper on.
    pub(super) fn own_pad_squares(&self, net: usize) -> Vec<(Rect, Layers)> {
        self.fixed_by_net[net]
            .iter()
            .filter_map(|&k| {
                let o = &self.fixed.list[k];
                // An opening the footprint draws in the solder mask over a pad is the pad's
                // net's too, on both faces, but it is no copper: a corner in it is no corner
                // in a pad.
                let Form::Rect(r) = o.form else { return None };
                if !o.copper {
                    return None;
                }
                let inset =
                    r.width().min(r.height()) / 2.0 * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
                let r = r.expanded(-inset);
                (r.width() > 0.0 && r.height() > 0.0).then_some((r, o.layers))
            })
            .collect()
    }

    /// The via in the pad with only the layers it leads to where it would keep the rules'
    /// space from all other copper on its own layers (all of them for a through via), and its
    /// hole from other holes; `None` where it leads to none. The via is at a known place, so
    /// this is measured there and not on the grid.
    pub(super) fn via_fits(
        &self,
        pad: &InPad,
        net: usize,
        obstacles: &[&Obstacle],
        keepouts: &[(Form, f64)],
    ) -> Option<InPad> {
        let own = self.board.clearance(&self.board.nets[net].name) + MARGIN;
        let holes = keepouts.iter().all(|(form, reach)| match form {
            // The pad's own keep-out is what the via is allowed to stand in.
            Form::Rect(b) if *b == pad.bounds => true,
            _ => form.distance(pad.at) >= *reach,
        });
        if !holes {
            return None;
        }
        let fits = |l: usize| {
            let (on, offer) = self.stack.offer_between(self.board, pad.face, l);
            let radius = offer.diameter / 2.0;
            obstacles
                .iter()
                .filter(|o| o.net != Some(net) && (on == Layers::ALL || o.layers.meets(on)))
                .all(|o| o.form.distance(pad.at) >= o.space(own) - MARGIN + radius + 1e-3)
        };
        let layers = (0..32)
            .filter(|&l| pad.layers[l] && fits(l))
            .fold(Layers::outer(false, false), Layers::with);
        (!layers.is_empty()).then_some(InPad { layers, ..*pad })
    }

    /// The tracks and vias that follow a path. A path to a plane ends in a via, which is the
    /// join to the plane.
    pub(super) fn realize(
        &self,
        line: usize,
        path: &[Spot],
        in_from: Option<InPad>,
        in_to: Option<InPad>,
        masks: &Masks,
    ) -> Routed {
        let rules = &self.board.rules;
        // A track through a cell that is too tight for the full width is necked down: to the
        // width its current needs, or near its pads to the narrowest.
        let width_of = |neck: u8, layer: usize| match neck {
            0 => self.width_on(line, layer),
            1 => self.neck_width_on(line, layer),
            _ => rules.min_track_width,
        };
        let necked = |&(i, j, layer): &Spot| masks.neck(layer, i, j);
        let l = &self.lines[line];
        let mut tracks: Vec<Track> = Vec::new();
        let mut vias: Vec<Via> = Vec::new();
        let mut start = 0;
        for k in 1..=path.len() {
            if k < path.len() && path[k].2 == path[k - 1].2 {
                continue;
            }
            let layer = path[start].2;
            let cells: Vec<(usize, usize)> = path[start..k].iter().map(|s| (s.0, s.1)).collect();
            let neck: Vec<u8> = path[start..k].iter().map(necked).collect();
            for (a, b, narrow) in runs_with(&self.grid, &cells, &neck) {
                tracks.push(Track {
                    net: l.net.clone(),
                    layer: self.stack.layer(layer),
                    from: a,
                    to: b,
                    width: width_of(narrow, layer),
                    mid: None,
                });
            }
            if k < path.len() {
                // The via between the two layers: a through via, or on a board with blind and
                // buried vias the one the search took (issue #1015).
                let (diameter, drill, span) =
                    self.stack.via_between(self.board, path[k - 1].2, path[k].2);
                vias.push(Via {
                    net: l.net.clone(),
                    at: self.grid.centre(path[k].0, path[k].1),
                    diameter,
                    drill,
                    span,
                });
                // The others of its cluster, where the masks left room (issue #431).
                for at in self
                    .cluster_cells(line, masks, path[k].0, path[k].1)
                    .unwrap_or_default()
                {
                    vias.push(Via {
                        net: l.net.clone(),
                        at,
                        diameter: rules.via_diameter,
                        drill: rules.via_drill,
                        span: None,
                    });
                }
            }
            start = k;
        }
        let last = path[path.len() - 1];
        let mut pads = vec![(l.from_at, path[0])];
        if l.to_plane {
            vias.push(Via {
                net: l.net.clone(),
                at: self.grid.centre(last.0, last.1),
                diameter: rules.via_diameter,
                drill: rules.via_drill,
                span: None,
            });
        } else {
            pads.push((l.to_at, last));
        }
        // A path that begins or ends on another layer than its pad's goes through a via in the
        // pad, the one between the two layers (issue #1071).
        let ends = in_from
            .iter()
            .map(|p| (p, path[0].2))
            .chain(in_to.iter().map(|p| (p, last.2)));
        for (pad, layer) in ends {
            let (diameter, drill, span) = self.stack.via_between(self.board, pad.face, layer);
            vias.push(Via {
                net: l.net.clone(),
                at: pad.at,
                diameter,
                drill,
                span,
            });
        }
        // A pad's centre is rarely on the grid, so join it to the first and last cell.
        for (pad_at, spot) in pads {
            let cell = self.grid.centre(spot.0, spot.1);
            if pad_at != cell {
                tracks.push(Track {
                    net: l.net.clone(),
                    layer: self.stack.layer(spot.2),
                    from: pad_at,
                    to: cell,
                    width: width_of(necked(&spot), spot.2),
                    mid: None,
                });
            }
        }
        Routed { line, tracks, vias }
    }
}
