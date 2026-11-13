//! Realising a path as tracks and vias.

use super::*;

impl<'a> Router<'a> {
    /// The pads of a net, each as the square a round pad of its size holds (where a corner is
    /// inside the pad's copper whatever its shape), with the layers it has copper on.
    pub(super) fn own_pad_squares(&self, net: usize) -> Vec<(Rect, [bool; 2])> {
        self.fixed_by_net[net]
            .iter()
            .filter_map(|&k| {
                let o = &self.fixed.list[k];
                let Form::Rect(r) = o.form else { return None };
                let inset =
                    r.width().min(r.height()) / 2.0 * (1.0 - std::f64::consts::FRAC_1_SQRT_2);
                let r = r.expanded(-inset);
                (r.width() > 0.0 && r.height() > 0.0).then_some((r, o.layers))
            })
            .collect()
    }

    /// Whether a via in the pad would keep the rules' space from all other copper, and its
    /// hole from other holes. The via is at a known place, so this is measured there and
    /// not on the grid.
    pub(super) fn via_fits(
        &self,
        pad: &InPad,
        net: usize,
        obstacles: &[&Obstacle],
        keepouts: &[(Form, f64)],
    ) -> bool {
        let radius = self.board.rules.via_diameter / 2.0;
        obstacles
            .iter()
            .filter(|o| o.net != Some(net))
            .all(|o| o.form.distance(pad.at) >= o.clearance - MARGIN + radius + 1e-3)
            && keepouts.iter().all(|(form, reach)| match form {
                // The pad's own keep-out is what the via is allowed to stand in.
                Form::Rect(b) if *b == pad.bounds => true,
                _ => form.distance(pad.at) >= *reach,
            })
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
        let width_of = |neck: u8| match neck {
            0 => self.width_of(line),
            1 => self.neck_width(line),
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
                    layer: layer_of(layer),
                    from: a,
                    to: b,
                    width: width_of(narrow),
                    mid: None,
                });
            }
            if k < path.len() {
                vias.push(Via {
                    net: l.net.clone(),
                    at: self.grid.centre(path[k].0, path[k].1),
                    diameter: rules.via_diameter,
                    drill: rules.via_drill,
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
            });
        } else {
            pads.push((l.to_at, last));
        }
        // A path that begins or ends on the far layer of a pad goes through a via in the pad.
        for pad in in_from.iter().chain(&in_to) {
            vias.push(Via {
                net: l.net.clone(),
                at: pad.at,
                diameter: rules.via_diameter,
                drill: rules.via_drill,
            });
        }
        // A pad's centre is rarely on the grid, so join it to the first and last cell.
        for (pad_at, spot) in pads {
            let cell = self.grid.centre(spot.0, spot.1);
            if pad_at != cell {
                tracks.push(Track {
                    net: l.net.clone(),
                    layer: layer_of(spot.2),
                    from: pad_at,
                    to: cell,
                    width: width_of(necked(&spot)),
                    mid: None,
                });
            }
        }
        Routed { line, tracks, vias }
    }
}
