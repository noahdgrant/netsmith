//! A board whose parts are not placed yet.

use super::*;

/// A board whose parts are not placed yet: what [`Board::unplaced`] makes of a design, and what
/// the placer places.
#[derive(Debug, Clone)]
pub struct Unplaced {
    pub process: Process,
    pub rules: Rules,
    /// What the layout file and the caller said, before the requirements were turned into
    /// places.
    pub base: PlaceOptions,
    /// `base` with the requirements' fixed parts, wants, offsets, faces and overhangs: what the
    /// placer keeps to.
    pub placement: PlaceOptions,
    /// The footprints, on the faces they were given, where the placer is to place them.
    pub parts: Vec<Placed>,
    pub nets: Vec<BoardNet>,
    pub planes: Vec<Plane>,
    /// The parts that may hang over an edge, as [`Board::overhang`].
    pub overhang: BTreeMap<String, Overhang>,
    /// The parts the requirements pin, as [`Board::pinned`].
    pub pinned: std::collections::BTreeSet<String>,
    /// The design's polygon outline on the sheet, as [`Board::polygon`]: the placer keeps
    /// the parts inside it.
    pub polygon: Option<crate::polygon::Polygon>,
}

impl Unplaced {
    /// The board, with the parts where the placer left them in an outline of `outline`.
    pub fn into_board(self, design: &Design, outline: Rect, corner_radius: f64) -> Board {
        let Unplaced {
            process,
            rules,
            parts,
            nets,
            planes,
            overhang,
            pinned,
            polygon,
            ..
        } = self;
        let cutouts = design
            .features
            .cutouts
            .iter()
            .map(|c| BoardCutout {
                name: c.name.clone(),
                at: c.at.map(|(x, y)| Pt::new(x, y)),
                size: c.size,
                radius: c.corner_radius,
            })
            .collect();
        let pours = design
            .features
            .pours
            .iter()
            .map(|p| BoardPour {
                layer: if p.layer == "top" {
                    CopperLayer::Front
                } else {
                    CopperLayer::Back
                },
                net: p.net.clone(),
                stitch_pitch: p.stitch_pitch,
            })
            .collect();
        Board {
            name: design.name.clone(),
            outline,
            corner_radius,
            polygon,
            cutouts,
            pours,
            keepouts: design
                .features
                .keepouts
                .iter()
                .map(|k| BoardKeepout {
                    name: k.name.clone(),
                    part: k.part.clone(),
                    at: k.at.map(|(x, y)| Pt::new(x, y)),
                    size: k.size,
                    layers: k.layers.clone(),
                    tracks: k.tracks,
                    vias: k.vias,
                    copper: k.copper,
                    parts: k.parts,
                    max_height: k.max_height,
                    drawn: false,
                })
                .collect(),
            parts,
            nets,
            rules,
            process,
            planes,
            tracks: Vec::new(),
            vias: Vec::new(),
            teardrops: Vec::new(),
            net_widths: design.features.net_widths.clone(),
            net_currents: design.features.net_currents.clone(),
            overhang,
            pinned,
        }
    }
}
