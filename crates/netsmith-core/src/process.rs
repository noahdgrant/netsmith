//! The manufacturing process, the panel and the stackup.

use crate::design_file;

/// The manufacturing process a board is made with, as the board's `process` member
/// resolved it: the least the process can make, after the board has narrowed it, and the
/// stackup. Lengths are in millimetres. Nothing here is specific to one manufacturer; a
/// different house is a different definition with the same fields.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Process {
    /// The definition it came from: `TwoLayer`.
    pub name: String,
    /// Who makes it: `JLCPCB`.
    pub fabricator: String,
    /// Where the numbers are published.
    pub source: String,
    /// The narrowest track.
    pub min_track: f64,
    /// The least space between copper of different nets.
    pub min_clearance: f64,
    /// The smallest drilled hole, which is a via's drill.
    pub min_drill: f64,
    /// The smallest copper disc of a via.
    pub min_via_diameter: f64,
    /// The least copper left round a plated pad's hole.
    pub min_annular_ring: f64,
    /// The least copper left round a via's hole: half of how much wider the via is than its
    /// hole (issue #431).
    pub min_via_ring: f64,
    /// The least space from the edge of a hole to copper of another net (a via's hole, a
    /// non-plated hole), and from a plated pad's hole to a track.
    pub min_hole_clearance: f64,
    pub min_plated_hole_clearance: f64,
    /// The least space between surface-mount pads of different nets.
    pub min_pad_to_pad: f64,
    /// The least space between a solder mask opening and a track of another net.
    pub min_mask_to_track: f64,
    /// The smallest non-plated hole, and the narrowest plated and non-plated slot.
    pub min_npth_drill: f64,
    pub min_plated_slot: f64,
    pub min_npth_slot: f64,
    /// The via the router draws: its hole and its copper (a default of the process the board
    /// may replace, issue #431).
    pub via_drill: f64,
    pub via_diameter: f64,
    /// The least space between the edges of two holes when one is a pad's, and between two
    /// vias' holes.
    pub min_hole_to_hole: f64,
    pub min_via_hole_to_hole: f64,
    /// The least space between copper and the board edge.
    pub min_edge_clearance: f64,
    /// The thinnest silkscreen stroke.
    pub min_silk_width: f64,
    /// The smallest silkscreen text.
    pub min_text_height: f64,
    /// The least space between silkscreen and a pad.
    pub min_silk_to_pad: f64,
    /// The least solder mask left between two pads.
    pub min_mask_bridge: f64,
    /// The largest via the fabricator fills and caps, so that it may sit in a pad. This is
    /// an upper bound: 0 where the board does not want vias in pads or the process cannot
    /// make them.
    pub via_in_pad: f64,
    /// The most a loaded track may heat above the board, in kelvin, if the board states it
    /// (`.process.max_temperature_rise`). `None` is the IPC-2221 check's default
    /// (ADR 0036).
    pub max_temperature_rise: Option<f64>,
    /// How much narrower than drawn a finished track may be, as a fraction (0.2 for ±20 %): the
    /// track width check sizes for the narrowest.
    pub track_width_tolerance: f64,
    /// The copper plated on a hole's wall, in mm, which carries a via's current.
    pub hole_plating: f64,
    pub stackup: Stackup,
}

/// The fabricators std names that do not exist: `std::mfg::acme`'s ACME (ADR 0112). A board on
/// one of their processes is made into a package but cannot be ordered.
pub const FICTIONAL_FABRICATORS: &[&str] = &["ACME"];

impl Process {
    /// Whether no real fabricator makes it: its numbers were chosen, not sourced, and a board on
    /// it is not for ordering (ADR 0112).
    pub fn is_fictional(&self) -> bool {
        FICTIONAL_FABRICATORS.contains(&self.fabricator.as_str())
    }
}

/// A panel (ADR 0040): one board repeated in a grid, cut apart by V-cuts, as a `panel`
/// definition states it. Lengths are in millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelSpec {
    /// The panel's name.
    pub name: String,
    /// The board it repeats, by the name of its definition.
    pub cell: String,
    /// Boards across the panel, and down it.
    pub columns: u32,
    pub rows: u32,
    /// The handling rail on all four sides, 0 for none.
    pub rail: f64,
    pub rules: PanelRules,
}

/// What the board house needs of a V-cut panel, as its `fab` member resolved it. Nothing here
/// is specific to one house; a different house is a different definition with the same
/// fields.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelRules {
    /// The definition it came from: `VCutPanel`.
    pub name: String,
    pub fabricator: String,
    pub source: String,
    pub min_rail: f64,
    pub min_size: f64,
    pub max_size: f64,
    pub max_assembled_size: f64,
    pub max_cuts: u32,
    pub min_cut_spacing: f64,
    pub advised_cut_spacing: f64,
    pub min_edge_clearance: f64,
    pub advised_edge_clearance: f64,
    pub min_thickness: f64,
    pub tooling_hole: f64,
    pub fiducial: f64,
    pub fiducial_offset: f64,
}

/// The thickness of one ounce of copper per square foot, in mm. It is a unit conversion,
/// not a figure from any manufacturer.
pub const OUNCE_OF_COPPER: f64 = 0.035;

/// What a board is built from.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Stackup {
    /// Copper layers.
    pub layers: u32,
    /// The finished board's thickness in mm: the range the board still allows. It is one
    /// value once the board has chosen.
    #[serde(with = "design_file::range")]
    pub thickness: (f64, f64),
    /// Finished outer copper, in ounces per square foot.
    pub copper_weight: f64,
    /// What the inner layers are, if the board has any (more than two copper layers).
    pub inner: Option<InnerLayers>,
}

/// The inner copper layers of a board of more than two layers and how the board is built up
/// round them (ADR 0111). Lengths are in mm.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InnerLayers {
    /// What each inner layer is, from the top: the first is `In1.Cu`.
    pub layers: Vec<InnerUse>,
    /// The copper of an inner layer.
    pub copper: f64,
    /// The dielectric layers between the copper layers, from the top: one fewer than the
    /// copper layers.
    pub dielectrics: Vec<DielectricLayer>,
    /// The name the fabricator gives the build-up, such as `JLC04161H-7628`.
    pub build: String,
    /// The least space between a plane and the hole of a via or pad that is not on its net.
    pub hole_clearance: f64,
}

/// What an inner copper layer is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InnerUse {
    /// One sheet of copper on the net.
    Plane(String),
    /// A layer for tracks. The router does not draw on inner layers yet, so it is empty.
    Signal,
}

/// One dielectric layer of a build-up.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DielectricLayer {
    /// A core, or else a prepreg.
    pub core: bool,
    /// The glass style of a prepreg (`7628`), empty for a core.
    pub glass: String,
    pub thickness: f64,
    /// The relative permittivity.
    pub epsilon: f64,
}

impl InnerLayers {
    /// The planes, each as its layer's number from the top (1 for `In1.Cu`) and its net.
    pub fn planes(&self) -> impl Iterator<Item = (usize, &str)> {
        self.layers.iter().enumerate().filter_map(|(i, u)| match u {
            InnerUse::Plane(net) => Some((i + 1, net.as_str())),
            InnerUse::Signal => None,
        })
    }
}

impl Stackup {
    /// The thickness, if the board has chosen one.
    pub fn chosen_thickness(&self) -> Option<f64> {
        (self.thickness.0 == self.thickness.1).then_some(self.thickness.0)
    }
}
