//! The layers a board's process builds it from, top to bottom, with each one's thickness: the
//! one list that the KiCad board file's stackup (`export.rs`) and the 3D scene (`scene3d.rs`)
//! are both written from, so the two never disagree.
//!
//! Two copper layers are the process's outer copper round one core, which makes up the rest of
//! the thickness the board chose. More are the outer copper, then each dielectric layer the
//! process states with an inner copper layer between each two, then the outer copper (ADR
//! 0111); their sum is the process's own figures, so it may differ from the chosen thickness,
//! which is the nominal: JLCPCB's JLC04161H-7628 adds up to 1.5862 mm, 1.6062 mm with the mask,
//! for a 1.6 mm board, and JLC06161H-3313 to 1.5384 mm.
//! The fabricator makes a board within [`thickness_tolerance`] of the nominal, and the sum is
//! well within it. The
//! solder mask is on both faces. The silkscreen and the paste have no thickness here.

use netsmith_core::OUNCE_OF_COPPER;

use crate::board::Board;

/// The thickness of a solder mask layer, in mm, which JLCPCB's page gives as at least this.
pub const MASK: f64 = 0.01;

/// How far a finished board may be from its nominal thickness, in mm: JLCPCB's capabilities page
/// (jlcpcb.com/capabilities/pcb-capabilities, "Board Thickness Tolerance") gives ±10% for a
/// board of 1.0 mm or more and ±0.1 mm below. A stackup's layers add up to within this of the
/// thickness the board chose.
pub fn thickness_tolerance(nominal: f64) -> f64 {
    if nominal >= 1.0 { 0.1 * nominal } else { 0.1 }
}

/// What a layer of the stackup is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LayerKind {
    Mask,
    Copper,
    /// The rigid dielectric: the one dielectric of a two-layer board, or a core of a build-up.
    Core,
    /// The dielectric pressed between two cores, or between a core and the outer copper.
    Prepreg,
}

impl LayerKind {
    /// The word for it in a report: `mask`, `copper`, `core`, `prepreg`.
    pub fn word(self) -> &'static str {
        match self {
            LayerKind::Mask => "mask",
            LayerKind::Copper => "copper",
            LayerKind::Core => "core",
            LayerKind::Prepreg => "prepreg",
        }
    }
}

/// One layer of the stackup.
#[derive(Debug, Clone, PartialEq)]
pub struct StackLayer {
    /// KiCad's name for it: `F.Mask`, `F.Cu`, `dielectric 1`, `In1.Cu`, `B.Cu`, `B.Mask`.
    pub name: String,
    pub kind: LayerKind,
    /// In mm, rounded to a nanometre so sums of millimetre values print as written.
    pub thickness: f64,
    /// The material of a mask or dielectric layer, `None` for copper: a prepreg's glass style
    /// (`7628`), `FR4` for a core.
    pub material: Option<String>,
    /// The relative permittivity of a mask or dielectric layer, `None` for copper.
    pub epsilon: Option<f64>,
}

/// A length rounded to a nanometre, so sums of millimetre values print as written.
pub fn round(mm: f64) -> f64 {
    (mm * 1e6).round() / 1e6
}

/// The board's layers from the top face down (see the module's header). It panics if the board
/// has not chosen its thickness, as a board is only made once it has.
pub fn layers(board: &Board) -> Vec<StackLayer> {
    let stackup = &board.process.stackup;
    let copper_mm = round(stackup.copper_weight * OUNCE_OF_COPPER);
    let mask = |name: &str| StackLayer {
        name: name.to_string(),
        kind: LayerKind::Mask,
        thickness: MASK,
        material: Some("Dry Film".into()),
        epsilon: Some(3.3),
    };
    let copper = |name: &str, mm: f64| StackLayer {
        name: name.to_string(),
        kind: LayerKind::Copper,
        thickness: mm,
        material: None,
        epsilon: None,
    };
    let dielectric =
        |number: usize, kind: LayerKind, mm: f64, material: String, epsilon| StackLayer {
            name: format!("dielectric {number}"),
            kind,
            thickness: mm,
            material: Some(material),
            epsilon: Some(epsilon),
        };
    let mut layers = vec![mask("F.Mask"), copper("F.Cu", copper_mm)];
    match &stackup.inner {
        None => {
            let core_mm = round(
                stackup
                    .chosen_thickness()
                    .expect("a board is only made once its thickness is chosen")
                    - 2.0 * copper_mm
                    - 2.0 * MASK,
            );
            layers.push(dielectric(1, LayerKind::Core, core_mm, "FR4".into(), 4.5));
        }
        Some(inner) => {
            // Dielectric, inner copper, dielectric, ... from the top; the last dielectric is
            // followed by the bottom copper.
            for (i, d) in inner.dielectrics.iter().enumerate() {
                let (kind, material) = if d.core {
                    (LayerKind::Core, "FR4".to_string())
                } else {
                    (LayerKind::Prepreg, d.glass.clone())
                };
                layers.push(dielectric(i + 1, kind, d.thickness, material, d.epsilon));
                if i + 1 < inner.dielectrics.len() {
                    layers.push(copper(&format!("In{}.Cu", i + 1), inner.copper));
                }
            }
        }
    }
    layers.extend([copper("B.Cu", copper_mm), mask("B.Mask")]);
    layers
}
