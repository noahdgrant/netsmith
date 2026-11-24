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
    /// The least space between two pieces of silkscreen (issue #1186), which KiCad's check also
    /// holds silkscreen to the board edge to. 0 in a design file written before it was read,
    /// and then the space from a pad stands for it.
    #[serde(default)]
    pub min_silk_to_silk: f64,
    /// The least solder mask left between two pads.
    pub min_mask_bridge: f64,
    /// How many fiducials each side that has assembled parts has, at least (a guideline,
    /// issue #761): a count, not a length.
    pub min_fiducials: f64,
    /// The diameter of a fiducial's copper that the process recommends, and how many times
    /// that the solder mask opening round it is at least (guidelines).
    pub fiducial_diameter: f64,
    pub fiducial_mask_ratio: f64,
    /// The least space from a fiducial's copper to the board edge (a limit).
    pub min_fiducial_edge: f64,
    /// What the process advises of a test point (guidelines): the least width of its pad, the
    /// least distance between the centres of two on a side, the least space from its copper
    /// to the body of a part and to the board edge.
    pub min_test_point_pad: f64,
    pub min_test_point_pitch: f64,
    pub min_test_point_body: f64,
    pub min_test_point_edge: f64,
    /// The test point rules that depend on the net or on the part (guidelines): the least width
    /// of the pad of a test point on a power or ground net, the height above which a part is
    /// tall, and the least space from a test point's copper to the body of a tall part.
    pub min_power_test_point_pad: f64,
    pub tall_part_height: f64,
    pub min_test_point_tall_body: f64,
    /// The angle in degrees between neighbouring fiducials about the centre of a round board
    /// (the blog's "approximately 90 degrees apart", a guideline), and the pitch in mm at or
    /// under which a part is "ultra-fine-pitch" to the surface finish advice (the blog's "0.4mm
    /// pitch BGAs", a guideline).
    pub fiducial_round_angle: f64,
    /// How far from a rule area that forbids copper or parts (a module's antenna keep-out) the
    /// design rules check notes the parts, tracks, vias and pours near it, in mm
    /// (`.process.rule_area_reach`, issue #1591; `near_rule_area`, a note). Every process offers
    /// 3 mm, a default of this tool's and not a fabricator's figure; a design file written before
    /// it is read with that.
    #[serde(default = "default_rule_area_reach")]
    pub rule_area_reach: f64,
    pub fine_pitch: f64,
    /// The largest via the fabricator fills and caps, so that it may sit in a pad. This is
    /// an upper bound: 0 where the board does not want vias in pads or the process cannot
    /// make them.
    pub via_in_pad: f64,
    /// The most a loaded track may heat above the board, in kelvin, if the board states it
    /// (`.process.max_temperature_rise`). `None` is the IPC-2221 check's default
    /// (ADR 0036).
    pub max_temperature_rise: Option<f64>,
    /// The least angle, in degrees, between a track's centre line and the tangent of a round
    /// copper edge where the track leaves it, if the board states it
    /// (`.process.min_round_track_angle`). `None`, what every process leaves it, is that round
    /// copper (a round pad or via, an oval's end, a rounded corner, a custom pad's circle or
    /// arc) is not judged for acid traps. No fabricator publishes a figure; netsmith's own
    /// rule (`acid_trap`, a warning). Not written to the design file when unstated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_round_track_angle: Option<f64>,
    /// The most space, in mm, between the copper of two vias of one net at which they stand
    /// side by side as one group, if the board states it (`.process.via_group_gap`, issue
    /// #1505). `None`, what every process leaves it, is the process's `min_clearance`. Not
    /// written to the design file when unstated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_group_gap: Option<f64>,
    /// The least side, in mm, of a single board that the assembly house places parts on one
    /// side of, if the process states it (`.process.min_single_sided_assembly_size`, issue
    /// 1285). `None`, what a process leaves it where its house publishes no figure, is that the
    /// board's size is not judged. Not written to the design file when unstated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_single_sided_assembly_size: Option<f64>,
    /// The same for a board with assembled parts on both faces
    /// (`.process.min_double_sided_assembly_size`): a board smaller than this on either side
    /// is ordered as a panel. `None` is that it is not judged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_double_sided_assembly_size: Option<f64>,
    /// Where the two figures above come from, in words (`.process.assembly_size_source`, issue
    /// 1393): the page, the service and the day it was read. The `assembly_size` finding quotes
    /// it, so that a person can check the number. `None` is that the process states none (a
    /// board that states a figure of its own may state its source). Not written to the design
    /// file when unstated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly_size_source: Option<String>,
    /// How much narrower than drawn a finished track may be, as a fraction (0.2 for ±20 %): the
    /// track width check sizes for the narrowest.
    pub track_width_tolerance: f64,
    /// The copper plated on a hole's wall, in mm, which carries a via's current.
    pub hole_plating: f64,
    /// The surface finish, one of the finishes the process offers (`HASL lead-free`, `ENIG`...):
    /// the process's own word, as the board chose it or the process's default (issue #842).
    pub finish: String,
    pub stackup: Stackup,
    /// What an HDI process makes besides through vias (issue #1015): `None` for every other
    /// process, and then not written to the design file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdi: Option<Hdi>,
}

/// The blind and buried vias of an HDI process (issue #1015), as its `.hdi_order`,
/// `.buried_vias` and laser fields resolved them. Lengths are in mm.
///
/// A board of `n` layers and order `k` has `k` build-up layers on each face: laser-drilled blind
/// vias join each pair of neighbouring layers among the top `k + 1` (`L1`-`L2` ... `Lk`-`Lk+1`)
/// and the bottom `k + 1`, each through one dielectric layer; with buried vias, a mechanically
/// drilled via joins the two outer layers of the core, `Lk+1`-`Ln-k`. A through via joins every
/// layer, as on any process.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hdi {
    /// The build-up layers on each face: 1, 2 or 3 (the fabricator's "1st-order" to "3rd-order").
    pub order: u32,
    /// Whether the board has buried vias through its core.
    pub buried: bool,
    /// The blind via the router draws: its laser-drilled hole and its copper.
    pub laser_drill: f64,
    pub laser_via_diameter: f64,
    /// The smallest and the largest laser-drilled hole.
    pub min_laser_drill: f64,
    pub max_laser_drill: f64,
    /// The least copper round the hole of a blind or a buried via.
    pub min_ring: f64,
    /// The most a blind via's dielectric may be thick for its hole's diameter (1 for 1:1), and
    /// a mechanically drilled hole's depth for its diameter (16 for 16:1).
    pub max_laser_aspect: f64,
    pub max_drill_aspect: f64,
    /// The thinnest and the thickest dielectric layer a blind via is drilled through.
    pub min_laser_dielectric: f64,
    pub max_laser_dielectric: f64,
    /// The thinnest core a buried via is drilled through, and its largest hole.
    pub min_buried_core: f64,
    pub max_buried_drill: f64,
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
    /// The colour of the solder mask, as the board's `.stackup.mask_colour` states it (issue
    /// #771): the name of a `std::parts::passive::Color` variant (`Black`). `None` when the board
    /// says nothing, which every drawing and file takes as it did before the member existed (so
    /// a design file that states none is unchanged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask_colour: Option<String>,
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
    /// The solder mask the fabricator's impedance calculator puts over an outer track, if the
    /// process states it (issue #1007).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<SolderMask>,
    /// The thickness the fabricator's impedance calculator gives an outer track's copper, in
    /// mm, if the process states it (the finished copper, with the plating): the track
    /// impedance is worked out with it where there is one, else with the copper weight's
    /// nominal thickness (issue #1007).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer_copper: Option<f64>,
    /// The loss tangent (dissipation factor, Df) of the build-up's dielectric, every layer of
    /// it, as `.stackup.loss_tangent` states it (issue #1405): the loss of a `TrackPath` is
    /// worked out with it at the path's `.frequency`. `None` when nothing states one, which no
    /// build-up of the standard library does (no source gives one per build-up), and then no
    /// loss is worked out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loss_tangent: Option<f64>,
}

/// A solder mask over a microstrip, as a fabricator's impedance calculator takes it (issue
/// #1007). Lengths are in mm.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SolderMask {
    /// The mask over the dielectric layer, beside the track: JLCPCB's "coating above
    /// substrate".
    pub thickness: f64,
    /// The mask's relative permittivity.
    pub epsilon: f64,
}

/// What an inner copper layer is.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum InnerUse {
    /// One sheet of copper on the net.
    Plane(String),
    /// A layer for tracks, which the router draws on as on the outer layers (issue #833).
    Signal,
}

/// One dielectric layer of a build-up: the dielectric between two copper layers. A layer pressed
/// from several plies has them in `plies`, and its own fields are what they make together
/// ([`DielectricLayer::of_plies`]); a layer of one prepreg or core has no plies.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DielectricLayer {
    /// A core, or else a prepreg. A layer of several plies is a prepreg unless every ply is a
    /// core.
    pub core: bool,
    /// The glass style of a prepreg (`7628`), empty for a core. A layer of several plies has
    /// the glass of each prepreg ply, from the top, joined by `+` (`7628+2116`).
    pub glass: String,
    pub thickness: f64,
    /// The relative permittivity.
    pub epsilon: f64,
    /// The plies of a layer pressed from several, from the top; empty for one prepreg or core.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plies: Vec<DielectricPly>,
}

/// One ply of a dielectric layer pressed from several: a sheet of prepreg, or a core with no
/// copper on it. Lengths are in mm.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DielectricPly {
    /// A core, or else a prepreg.
    pub core: bool,
    /// The glass style of a prepreg (`7628`), empty for a core.
    pub glass: String,
    pub thickness: f64,
    /// The relative permittivity.
    pub epsilon: f64,
}

impl DielectricLayer {
    /// A layer of one prepreg or core.
    pub fn single(core: bool, glass: String, thickness: f64, epsilon: f64) -> Self {
        DielectricLayer {
            core,
            glass,
            thickness,
            epsilon,
            plies: Vec::new(),
        }
    }

    /// A layer pressed from `plies`, from the top. Its thickness is their sum. Its permittivity
    /// is theirs in series, `h / sum(h_i / e_i)`: the plies lie one over the other across the
    /// field between two copper layers, as capacitors in series do. That holds exactly for a
    /// field straight across the layer (a plate capacitor) and approximately for a track's,
    /// whose impedance (`netsmith_pcb::impedance`) reads the layer as one dielectric. One ply is
    /// a layer of one.
    pub fn of_plies(plies: Vec<DielectricPly>) -> Self {
        if let [ply] = plies.as_slice() {
            return DielectricLayer::single(
                ply.core,
                ply.glass.clone(),
                ply.thickness,
                ply.epsilon,
            );
        }
        // Rounded to a nanometre of the millimetre, as the design stage rounds a length, so that
        // the sum of `0.218` and `0.2104` is `0.4284`.
        let thickness = (plies.iter().map(|p| p.thickness).sum::<f64>() * 1e6).round() / 1e6;
        let across: f64 = plies.iter().map(|p| p.thickness / p.epsilon).sum();
        let glass: Vec<&str> = plies
            .iter()
            .filter(|p| !p.core)
            .map(|p| p.glass.as_str())
            .collect();
        DielectricLayer {
            core: plies.iter().all(|p| p.core),
            glass: glass.join("+"),
            thickness,
            epsilon: if across > 0.0 {
                thickness / across
            } else {
                0.0
            },
            plies,
        }
    }
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

/// The reach of `near_rule_area` a design file written before issue #1591 is read with: what
/// every process of the standard library offers (`.rule_area_reach ?= 3mm`).
fn default_rule_area_reach() -> f64 {
    3.0
}
