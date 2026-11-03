//! What fabrication needs to know about a board, read from its `.kicad_pcb` file: which
//! copper layers it has, and for every part its designator, footprint, side, how it is
//! soldered and its properties.

use crate::sexp::{self, Sexp};
use std::collections::BTreeMap;

#[derive(Debug)]
pub enum BoardError {
    Parse(sexp::ParseError),
    /// The file is not a board: its first list is not `kicad_pcb`.
    NotABoard,
}

impl std::fmt::Display for BoardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "cannot read the .kicad_pcb file: {e}"),
            Self::NotABoard => write!(
                f,
                "the file is not a KiCad board: it has no `(kicad_pcb` list"
            ),
        }
    }
}

impl std::error::Error for BoardError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
}

/// How a part gets onto the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Surface mount: JLCPCB places it.
    Assembled,
    /// Through hole: soldered by hand.
    HandSolder,
    /// Marked "do not populate".
    NotFitted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoardPart {
    pub designator: String,
    pub value: String,
    /// `Library:Name`, as the footprint is written in the file.
    pub footprint: String,
    pub side: Side,
    pub fit: Fit,
    pub properties: BTreeMap<String, String>,
    /// The footprint's origin on the board, in KiCad's frame (y down), before the drill/place
    /// origin is taken off.
    pub at: (f64, f64),
    /// The footprint's turn in degrees, as the file has it.
    pub rotation: f64,
}

impl BoardPart {
    /// The footprint's name without its library: `R_0603_1608Metric`.
    pub fn footprint_name(&self) -> &str {
        footprint_name(&self.footprint)
    }
}

/// `Resistor_SMD:R_0603_1608Metric` -> `R_0603_1608Metric`.
pub fn footprint_name(footprint: &str) -> &str {
    footprint.rsplit_once(':').map_or(footprint, |(_, n)| n)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    /// Copper layers in the file's order, such as `F.Cu`, `In1.Cu`, `B.Cu`.
    pub copper_layers: Vec<String>,
    /// The drill/place origin (`aux_axis_origin`) if the board sets one. Every output uses
    /// it; without one, coordinates are relative to the page corner.
    pub origin: Option<(f64, f64)>,
    /// Parts that appear in the bill of materials, in file order. Footprints that are not
    /// parts (fiducials, mounting holes, logos) are left out.
    pub parts: Vec<BoardPart>,
}

impl Board {
    pub fn parse(src: &str) -> Result<Self, BoardError> {
        let root = sexp::parse(src).map_err(BoardError::Parse)?;
        if root.head() != Some("kicad_pcb") {
            return Err(BoardError::NotABoard);
        }
        let copper_layers = root
            .child("layers")
            .map(|l| {
                l.items()
                    .iter()
                    .filter_map(|e| e.items().get(1)?.text())
                    .filter(|n| n.ends_with(".Cu"))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let origin = root
            .child("setup")
            .and_then(|s| s.child("aux_axis_origin"))
            .and_then(|o| {
                let x = o.items().get(1)?.text()?.parse().ok()?;
                let y = o.items().get(2)?.text()?.parse().ok()?;
                Some((x, y))
            });
        let parts = root.children("footprint").filter_map(read_part).collect();
        Ok(Self {
            copper_layers,
            origin,
            parts,
        })
    }
}

fn read_part(fp: &Sexp) -> Option<BoardPart> {
    let footprint = fp.items().get(1)?.text()?.to_string();
    let mut properties = BTreeMap::new();
    for p in fp.children("property") {
        if let (Some(k), Some(v)) = (
            p.items().get(1).and_then(Sexp::text),
            p.items().get(2).and_then(Sexp::text),
        ) {
            properties.insert(k.to_string(), v.to_string());
        }
    }
    let designator = properties.get("Reference")?.clone();
    let value = properties.get("Value").cloned().unwrap_or_default();
    let side = match fp.child("layer").and_then(|l| l.items().get(1)?.text()) {
        Some("B.Cu") => Side::Bottom,
        _ => Side::Top,
    };
    let attrs: Vec<&str> = fp
        .child("attr")
        .map(|a| a.items().iter().skip(1).filter_map(Sexp::text).collect())
        .unwrap_or_default();
    let has = |name: &str| attrs.contains(&name);
    if designator.is_empty() || has("exclude_from_bom") || has("board_only") {
        return None;
    }
    let through_hole_pads = fp
        .children("pad")
        .any(|p| p.items().get(2).and_then(Sexp::text) == Some("thru_hole"));
    let fit = if has("dnp") {
        Fit::NotFitted
    } else if has("through_hole") || (!has("smd") && through_hole_pads) {
        Fit::HandSolder
    } else {
        Fit::Assembled
    };
    let at_node = fp.child("at");
    let at_number = |i: usize| -> f64 {
        at_node
            .and_then(|a| a.items().get(i)?.text()?.parse().ok())
            .unwrap_or(0.0)
    };
    Some(BoardPart {
        at: (at_number(1), at_number(2)),
        rotation: at_number(3),
        designator,
        value,
        footprint,
        side,
        fit,
        properties,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A small board in the shape KiCad 9 writes: two resistors (one on the back), a pin
    /// header, a fiducial and a do-not-populate LED.
    pub const SAMPLE: &str = r#"(kicad_pcb
	(version 20241229)
	(layers
		(0 "F.Cu" signal)
		(2 "B.Cu" signal)
		(5 "F.SilkS" user "F.Silkscreen")
		(25 "Edge.Cuts" user)
	)
	(setup
		(aux_axis_origin 100 200)
	)
	(gr_rect (start 90 180) (end 150 210) (stroke (width 0.05) (type default)) (fill no) (layer "Edge.Cuts"))
	(footprint "Resistor_SMD:R_0603_1608Metric"
		(layer "F.Cu")
		(at 110 190 -90)
		(property "Reference" "R1" (at 0 0 0) (layer "F.SilkS"))
		(property "Value" "10k" (at 0 0 0) (layer "F.Fab"))
		(property "LCSC Part #" "C25804" (at 0 0 0) (layer "F.Fab"))
		(attr smd)
		(pad "1" smd roundrect (at -0.8 0) (size 1 1) (layers "F.Cu"))
	)
	(footprint "Resistor_SMD:R_0603_1608Metric"
		(layer "B.Cu")
		(at 120 190 90)
		(property "Reference" "R2" (at 0 0 0) (layer "B.SilkS"))
		(property "Value" "10k" (at 0 0 0) (layer "B.Fab"))
		(attr smd)
	)
	(footprint "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical"
		(layer "F.Cu")
		(at 130 190)
		(property "Reference" "J1" (at 0 0 0) (layer "F.SilkS"))
		(property "Value" "Conn_01x02" (at 0 0 0) (layer "F.Fab"))
		(attr through_hole)
		(pad "1" thru_hole rect (at 0 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask"))
	)
	(footprint "Fiducial:Fiducial_1mm_Mask2mm"
		(layer "F.Cu")
		(at 105 195)
		(property "Reference" "FID1" (at 0 0 0) (layer "F.SilkS"))
		(property "Value" "Fiducial" (at 0 0 0) (layer "F.Fab"))
		(attr smd exclude_from_bom)
	)
	(footprint "LED_SMD:LED_0603_1608Metric"
		(layer "F.Cu")
		(at 140 190)
		(property "Reference" "D1" (at 0 0 0) (layer "F.SilkS"))
		(property "Value" "red" (at 0 0 0) (layer "F.Fab"))
		(attr smd dnp)
	)
)"#;

    #[test]
    fn reads_layers_origin_and_parts() {
        let b = Board::parse(SAMPLE).unwrap();
        assert_eq!(b.copper_layers, ["F.Cu", "B.Cu"]);
        assert_eq!(b.origin, Some((100.0, 200.0)));
        let refs: Vec<_> = b.parts.iter().map(|p| p.designator.as_str()).collect();
        assert_eq!(refs, ["R1", "R2", "J1", "D1"]);
    }

    #[test]
    fn classifies_parts() {
        let b = Board::parse(SAMPLE).unwrap();
        let fit: Vec<_> = b.parts.iter().map(|p| (p.side, p.fit)).collect();
        assert_eq!(
            fit,
            [
                (Side::Top, Fit::Assembled),
                (Side::Bottom, Fit::Assembled),
                (Side::Top, Fit::HandSolder),
                (Side::Top, Fit::NotFitted),
            ]
        );
        assert_eq!(b.parts[0].footprint_name(), "R_0603_1608Metric");
        assert_eq!(b.parts[0].properties["LCSC Part #"], "C25804");
        assert_eq!(b.parts[0].value, "10k");
    }

    #[test]
    fn a_footprint_without_a_type_is_through_hole_if_it_has_a_through_hole_pad() {
        let src = r#"(kicad_pcb (footprint "L:X" (layer "F.Cu")
            (property "Reference" "J1") (pad "1" thru_hole circle (at 0 0))))"#;
        assert_eq!(Board::parse(src).unwrap().parts[0].fit, Fit::HandSolder);
    }

    #[test]
    fn rejects_other_files() {
        assert!(matches!(
            Board::parse("(kicad_sch)"),
            Err(BoardError::NotABoard)
        ));
        assert!(matches!(
            Board::parse("(kicad_pcb"),
            Err(BoardError::Parse(_))
        ));
    }
}
