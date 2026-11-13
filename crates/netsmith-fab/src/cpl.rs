//! The placement (centroid) file: where each assembled part goes, in JLCPCB's columns.
//!
//! Positions come from the board itself: a footprint's origin, relative to the same
//! drill/place origin as the Gerbers (so the placement lands on the copper), with y turned
//! up as KiCad's position file has it. Rotation gets the offset from [`crate::rotation`].
//! `Layer` is written `top` or `bottom`, which is what the Fabrication Toolkit writes and
//! what JLCPCB accepted in assembly orders (verified against JLCPCB-accepted assemblies, not in this repository).
//! [`from_kicad_pos`] reads the same rows from `kicad-cli pcb export pos` for the
//! cross-check.

use crate::board::{BoardPart, Side};
use crate::bom::designator_cmp;
use crate::csv::{self, CsvError};
use crate::rotation::{self, Correction};
use std::collections::BTreeMap;

pub(crate) const HEADER: [&str; 5] = ["Designator", "Mid X", "Mid Y", "Layer", "Rotation"];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Placement {
    pub designator: String,
    pub x: f64,
    pub y: f64,
    pub side: Side,
    /// Degrees in `[0, 360)`, after the JLCPCB correction.
    pub rotation: f64,
}

#[derive(Debug, PartialEq)]
pub enum CplError {
    Csv(CsvError),
    /// A row has a number that is not one.
    BadNumber {
        designator: String,
        column: &'static str,
        text: String,
    },
    BadSide {
        designator: String,
        text: String,
    },
    /// KiCad's position file has no row for a part on the board.
    NoPosition(String),
}

impl std::fmt::Display for CplError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Csv(e) => write!(f, "cannot read KiCad's position file: {e}"),
            Self::BadNumber {
                designator,
                column,
                text,
            } => write!(
                f,
                "KiCad's position file gives {designator} a {column} of `{text}`, which is not a number"
            ),
            Self::BadSide { designator, text } => write!(
                f,
                "KiCad's position file puts {designator} on side `{text}`, expected top or bottom"
            ),
            Self::NoPosition(d) => write!(
                f,
                "KiCad's position file has no row for {d}; is the footprint excluded from position files?"
            ),
        }
    }
}

impl std::error::Error for CplError {}

impl From<CsvError> for CplError {
    fn from(e: CsvError) -> Self {
        Self::Csv(e)
    }
}

/// The placement rows plus what could not be checked about them.
#[derive(Debug, PartialEq)]
pub(crate) struct Cpl {
    pub rows: Vec<Placement>,
    /// Footprint name -> designators whose rotation offset has not been measured.
    pub unverified: BTreeMap<String, Vec<String>>,
    /// Designators on the bottom side, whose rotation and mirroring have not been measured.
    pub bottom: Vec<String>,
}

/// Placements for `parts`: where each footprint's origin is, from the drill/place `origin`
/// (the page corner if the board sets none), with y up, and its turn as the file has it.
pub(crate) fn build(origin: Option<(f64, f64)>, parts: &[&BoardPart]) -> Cpl {
    let (ox, oy) = origin.unwrap_or((0.0, 0.0));
    finish(
        parts
            .iter()
            .map(|p| (*p, p.at.0 - ox, -(p.at.1 - oy), p.side, p.rotation))
            .collect(),
    )
}

/// Placements for `parts` from the text of KiCad's CSV position file (millimetres), which
/// `kicad-cli pcb export pos --use-drill-file-origin` writes. Only the cross-check reads it.
pub(crate) fn from_kicad_pos(pos_csv: &str, parts: &[&BoardPart]) -> Result<Cpl, CplError> {
    let table = csv::parse(pos_csv)?;
    let Some((header, body)) = table.split_first() else {
        return Err(CplError::Csv(CsvError::MissingColumn("Ref".into())));
    };
    let c = csv::columns(header, &["Ref", "PosX", "PosY", "Rot", "Side"])?;
    let by_ref: BTreeMap<&str, &Vec<String>> = body
        .iter()
        .filter_map(|r| Some((r.get(c[0])?.as_str(), r)))
        .collect();

    let mut raw = Vec::new();
    for part in parts {
        let d = &part.designator;
        let row = by_ref
            .get(d.as_str())
            .ok_or_else(|| CplError::NoPosition(d.clone()))?;
        let num = |i: usize, column: &'static str| -> Result<f64, CplError> {
            let text = row.get(c[i]).map_or("", |s| s.trim());
            text.parse().map_err(|_| CplError::BadNumber {
                designator: d.clone(),
                column,
                text: text.to_string(),
            })
        };
        let (x, y, rot) = (num(1, "PosX")?, num(2, "PosY")?, num(3, "Rot")?);
        let side = match row.get(c[4]).map_or("", |s| s.trim()) {
            "top" => Side::Top,
            "bottom" => Side::Bottom,
            other => {
                return Err(CplError::BadSide {
                    designator: d.clone(),
                    text: other.to_string(),
                });
            }
        };
        raw.push((*part, x, y, side, rot));
    }
    Ok(finish(raw))
}

/// Applies the rotation offsets and orders the rows.
fn finish(raw: Vec<(&BoardPart, f64, f64, Side, f64)>) -> Cpl {
    let mut rows = Vec::new();
    let mut unverified: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut bottom = Vec::new();
    for (part, x, y, side, rot) in raw {
        let d = &part.designator;
        let offset = if side == Side::Bottom {
            bottom.push(d.clone());
            0.0
        } else {
            match rotation::lookup(part.footprint_name()) {
                Correction::Offset(o) => o,
                Correction::Unverified => {
                    unverified
                        .entry(part.footprint_name().to_string())
                        .or_default()
                        .push(d.clone());
                    0.0
                }
            }
        };
        rows.push(Placement {
            designator: d.clone(),
            x,
            y,
            side,
            rotation: rotation::normalize(rot + offset),
        });
    }
    rows.sort_by(|a, b| designator_cmp(&a.designator, &b.designator));
    for ds in unverified.values_mut() {
        ds.sort_by(|a, b| designator_cmp(a, b));
    }
    bottom.sort_by(|a, b| designator_cmp(a, b));
    Cpl {
        rows,
        unverified,
        bottom,
    }
}

/// A coordinate with at most six decimals and no trailing zeros: `19.2`, `270`, `0`.
pub(crate) fn number(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

/// The text of `cpl.csv`.
pub(crate) fn render(rows: &[Placement]) -> String {
    let rows: Vec<Vec<String>> = rows
        .iter()
        .map(|p| {
            vec![
                p.designator.clone(),
                number(p.x),
                number(p.y),
                match p.side {
                    Side::Top => "top",
                    Side::Bottom => "bottom",
                }
                .to_string(),
                number(p.rotation),
            ]
        })
        .collect();
    csv::file(&HEADER, &rows)
}

/// Reads a placement file with the columns [`render`] writes, or the Fabrication Toolkit's
/// `positions.csv` (`Designator,Mid X,Mid Y,Rotation,Layer`): columns are found by name.
#[cfg(test)]
pub(crate) fn parse(src: &str) -> Result<Vec<Placement>, CplError> {
    let table = csv::parse(src)?;
    let Some((header, body)) = table.split_first() else {
        return Ok(Vec::new());
    };
    let c = csv::columns(header, &HEADER)?;
    body.iter()
        .map(|r| {
            let d = r[c[0]].trim().to_string();
            let num = |i: usize, column: &'static str| -> Result<f64, CplError> {
                let text = r.get(c[i]).map_or("", |s| s.trim());
                text.parse().map_err(|_| CplError::BadNumber {
                    designator: d.clone(),
                    column,
                    text: text.to_string(),
                })
            };
            let side = match r.get(c[3]).map_or("", |s| s.trim()).to_lowercase().as_str() {
                "top" => Side::Top,
                "bottom" => Side::Bottom,
                other => {
                    return Err(CplError::BadSide {
                        designator: d.clone(),
                        text: other.to_string(),
                    });
                }
            };
            Ok(Placement {
                x: num(1, "Mid X")?,
                y: num(2, "Mid Y")?,
                rotation: num(4, "Rotation")?,
                side,
                designator: d,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{Board, tests::SAMPLE};

    const POS: &str = "Ref,Val,Package,PosX,PosY,Rot,Side\n\
        R1,10k,R_0603_1608Metric,10.0,10.0,-90.000000,top\n\
        R2,10k,R_0603_1608Metric,20.0,10.5,90.0,bottom\n\
        J1,Conn,PinHeader,30.0,10.0,0.0,top\n\
        D1,red,LED_0603_1608Metric,40.0,10.0,180.0,top\n";

    fn parts(board: &Board, refs: &[&str]) -> Vec<BoardPart> {
        board
            .parts
            .iter()
            .filter(|p| refs.contains(&p.designator.as_str()))
            .cloned()
            .collect()
    }

    #[test]
    fn applies_offsets_and_normalizes() {
        let board = Board::parse(SAMPLE).unwrap();
        let ps = parts(&board, &["R1"]);
        let cpl = from_kicad_pos(POS, &ps.iter().collect::<Vec<_>>()).unwrap();
        assert_eq!(cpl.rows.len(), 1);
        assert_eq!(cpl.rows[0].rotation, 270.0); // -90 + 0
        assert!(cpl.unverified.is_empty());
        assert_eq!(
            render(&cpl.rows),
            "Designator,Mid X,Mid Y,Layer,Rotation\nR1,10,10,top,270\n"
        );
    }

    #[test]
    fn positions_come_from_the_board_relative_to_its_origin_with_y_up() {
        let board = Board::parse(SAMPLE).unwrap();
        let ps = parts(&board, &["R1", "R2"]);
        let cpl = build(board.origin, &ps.iter().collect::<Vec<_>>());
        // R1 is at (110, 190) turned -90, R2 on the back at (120, 190) turned 90; the origin
        // is (100, 200), so y = -(190 - 200) = 10.
        assert_eq!(
            render(&cpl.rows),
            "Designator,Mid X,Mid Y,Layer,Rotation\nR1,10,10,top,270\nR2,20,10,bottom,90\n"
        );
        // Without an origin the page corner is the origin.
        let cpl = build(None, &ps.iter().collect::<Vec<_>>());
        assert_eq!((cpl.rows[0].x, cpl.rows[0].y), (110.0, -190.0));
    }

    #[test]
    fn reports_unverified_footprints_and_bottom_parts() {
        let board = Board::parse(SAMPLE).unwrap();
        let ps = parts(&board, &["R1", "R2", "D1"]);
        let cpl = from_kicad_pos(POS, &ps.iter().collect::<Vec<_>>()).unwrap();
        assert_eq!(
            cpl.unverified,
            BTreeMap::from([("LED_0603_1608Metric".to_string(), vec!["D1".to_string()])])
        );
        assert_eq!(cpl.bottom, ["R2"]);
        // Unverified and bottom parts keep KiCad's rotation.
        let rot: Vec<_> = cpl
            .rows
            .iter()
            .map(|r| (r.designator.as_str(), r.rotation))
            .collect();
        assert_eq!(rot, [("D1", 180.0), ("R1", 270.0), ("R2", 90.0)]);
    }

    #[test]
    fn a_part_missing_from_kicads_file_is_an_error() {
        let board = Board::parse(SAMPLE).unwrap();
        let ps = parts(&board, &["R1"]);
        let only_header = "Ref,Val,Package,PosX,PosY,Rot,Side\n";
        assert_eq!(
            from_kicad_pos(only_header, &ps.iter().collect::<Vec<_>>()),
            Err(CplError::NoPosition("R1".into()))
        );
    }

    #[test]
    fn numbers_are_short_and_never_negative_zero() {
        assert_eq!(number(19.2), "19.2");
        assert_eq!(number(270.0), "270");
        assert_eq!(number(-0.0), "0");
        assert_eq!(number(-0.0000001), "0");
        assert_eq!(number(6.906_666_67), "6.906667");
        assert_eq!(number(-110.29), "-110.29");
    }

    #[test]
    fn what_render_writes_parse_reads() {
        let rows = vec![Placement {
            designator: "R1".into(),
            x: 1.5,
            y: -2.25,
            side: Side::Bottom,
            rotation: 90.0,
        }];
        assert_eq!(parse(&render(&rows)).unwrap(), rows);
    }

    #[test]
    fn parse_reads_the_fabrication_toolkits_column_order() {
        let p = parse("\u{feff}Designator,Mid X,Mid Y,Rotation,Layer\nC1,19.2,26.85,270.0,top\n")
            .unwrap();
        assert_eq!(p[0].rotation, 270.0);
        assert_eq!(p[0].side, Side::Top);
    }
}
