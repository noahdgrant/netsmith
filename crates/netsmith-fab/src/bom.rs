//! The bill of materials in the form JLCPCB's assembly upload reads.
//!
//! Columns are `Comment,Designator,Footprint,LCSC Part #`, the names in JLCPCB's own BOM
//! guide for KiCad. The Fabrication Toolkit writes `Designator,Footprint,Quantity,Value,
//! LCSC Part #` instead, and JLCPCB accepted that too (seen in an accepted assembly order, not in this repository); we use the guide's
//! names because they are the documented ones. `Comment` carries the part's value.

use crate::board::Board;
use crate::csv::{self, CsvError};
use std::cmp::Ordering;

pub(crate) const HEADER: [&str; 4] = ["Comment", "Designator", "Footprint", "LCSC Part #"];

/// One kind of part and where it goes. Identical parts may arrive as one entry per
/// designator or already grouped; [`group`] puts them on one row either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BomEntry {
    pub designators: Vec<String>,
    /// The value shown to the buyer: `10k`, `100n`, `red`.
    pub value: String,
    /// The footprint's name, such as `R_0603_1608Metric`.
    pub footprint: String,
    /// JLCPCB's part number, such as `C25804`.
    pub lcsc: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BomError {
    /// A designator is listed twice, in one entry or two.
    DuplicateDesignator(String),
    /// An entry lists no designators.
    NoDesignators {
        value: String,
        footprint: String,
    },
    /// A part number that is not `C` followed by digits.
    BadLcsc {
        designators: String,
        lcsc: String,
    },
    Csv(CsvError),
}

impl std::fmt::Display for BomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateDesignator(d) => {
                write!(f, "designator {d} appears more than once in the BOM")
            }
            Self::NoDesignators { value, footprint } => {
                write!(f, "the BOM entry {value} ({footprint}) has no designators")
            }
            Self::BadLcsc { designators, lcsc } => write!(
                f,
                "{designators}: `{lcsc}` is not an LCSC part number (expected `C` and digits, such as C25804)"
            ),
            Self::Csv(e) => write!(f, "cannot read the BOM file: {e}"),
        }
    }
}

impl std::error::Error for BomError {}

impl From<CsvError> for BomError {
    fn from(e: CsvError) -> Self {
        Self::Csv(e)
    }
}

/// Orders designators the way people read them: `C2` before `C10`, `C` before `R`.
pub(crate) fn designator_cmp(a: &str, b: &str) -> Ordering {
    fn split(s: &str) -> (&str, u64, &str) {
        let letters = s.find(|c: char| c.is_ascii_digit()).unwrap_or(s.len());
        let digits = s[letters..]
            .find(|c: char| !c.is_ascii_digit())
            .map_or(s.len(), |d| letters + d);
        (
            &s[..letters],
            s[letters..digits].parse().unwrap_or(0),
            &s[digits..],
        )
    }
    split(a).cmp(&split(b)).then_with(|| a.cmp(b))
}

impl BomEntry {
    /// The part number, if there is a non-blank one.
    pub fn lcsc(&self) -> Option<&str> {
        self.lcsc
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }
}

fn valid_lcsc(s: &str) -> bool {
    s.strip_prefix('C')
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

/// Puts identical parts (same value, footprint and part number) on one entry, sorts the
/// designators within it, and sorts the entries by their first designator.
pub(crate) fn group(entries: &[BomEntry]) -> Result<Vec<BomEntry>, BomError> {
    let mut seen = std::collections::BTreeSet::new();
    let mut grouped: Vec<BomEntry> = Vec::new();
    for e in entries {
        let value = e.value.trim().to_string();
        let footprint = e.footprint.trim().to_string();
        let lcsc = e.lcsc().map(str::to_string);
        if e.designators.is_empty() {
            return Err(BomError::NoDesignators { value, footprint });
        }
        for d in &e.designators {
            if !seen.insert(d.trim().to_string()) {
                return Err(BomError::DuplicateDesignator(d.trim().to_string()));
            }
        }
        if let Some(l) = &lcsc
            && !valid_lcsc(l)
        {
            return Err(BomError::BadLcsc {
                designators: e.designators.join(", "),
                lcsc: l.clone(),
            });
        }
        let designators = e.designators.iter().map(|d| d.trim().to_string());
        match grouped
            .iter_mut()
            .find(|g| g.value == value && g.footprint == footprint && g.lcsc == lcsc)
        {
            Some(g) => g.designators.extend(designators),
            None => grouped.push(BomEntry {
                designators: designators.collect(),
                value,
                footprint,
                lcsc,
            }),
        }
    }
    for g in &mut grouped {
        g.designators.sort_by(|a, b| designator_cmp(a, b));
    }
    grouped.sort_by(|a, b| designator_cmp(&a.designators[0], &b.designators[0]));
    Ok(grouped)
}

/// The text of `bom.csv` for entries that are already grouped.
pub(crate) fn render(entries: &[BomEntry]) -> String {
    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|e| {
            vec![
                e.value.clone(),
                e.designators.join(","),
                e.footprint.clone(),
                e.lcsc().unwrap_or("").to_string(),
            ]
        })
        .collect();
    csv::file(&HEADER, &rows)
}

/// One entry per part on the board, taking the part number from the footprint property
/// named `property` (for example `LCSC Part #`). A missing or blank property gives `None`.
pub fn from_board(board: &Board, property: &str) -> Vec<BomEntry> {
    board
        .parts
        .iter()
        .map(|p| BomEntry {
            designators: vec![p.designator.clone()],
            value: p.value.clone(),
            footprint: p.footprint_name().to_string(),
            lcsc: p
                .properties
                .get(property)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
        })
        .collect()
}

/// Reads a BOM file with the same columns [`render`] writes. Designators in one cell are
/// separated by commas or spaces.
pub fn parse_csv(src: &str) -> Result<Vec<BomEntry>, BomError> {
    let rows = csv::parse(src)?;
    let Some((header, body)) = rows.split_first() else {
        return Ok(Vec::new());
    };
    let cols = csv::columns(header, &HEADER)?;
    let cell = |row: &[String], i: usize| row.get(cols[i]).map_or("", |s| s.trim()).to_string();
    Ok(body
        .iter()
        .map(|row| BomEntry {
            designators: cell(row, 1)
                .split([',', ' '])
                .filter(|d| !d.is_empty())
                .map(str::to_string)
                .collect(),
            value: cell(row, 0),
            footprint: cell(row, 2),
            lcsc: Some(cell(row, 3)).filter(|s| !s.is_empty()),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(d: &[&str], value: &str, fp: &str, lcsc: Option<&str>) -> BomEntry {
        BomEntry {
            designators: d.iter().map(|s| s.to_string()).collect(),
            value: value.into(),
            footprint: fp.into(),
            lcsc: lcsc.map(str::to_string),
        }
    }

    #[test]
    fn designators_sort_by_letters_then_number() {
        let mut v = vec!["R10", "C2", "R2", "C10", "D1", "R1"];
        v.sort_by(|a, b| designator_cmp(a, b));
        assert_eq!(v, ["C2", "C10", "D1", "R1", "R2", "R10"]);
    }

    #[test]
    fn identical_parts_share_a_row() {
        let g = group(&[
            e(&["R10"], "10k", "R_0603_1608Metric", Some("C25804")),
            e(&["C1"], "100n", "C_0603_1608Metric", Some("C14663")),
            e(&["R2"], "10k", "R_0603_1608Metric", Some("C25804")),
            e(&["R1", "R3"], "10k", "R_0603_1608Metric", Some("C25804")),
        ])
        .unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].designators, ["C1"]);
        assert_eq!(g[1].designators, ["R1", "R2", "R3", "R10"]);
    }

    #[test]
    fn a_different_value_footprint_or_part_number_is_a_different_row() {
        let g = group(&[
            e(&["R1"], "10k", "R_0603_1608Metric", Some("C25804")),
            e(&["R2"], "10k", "R_0402_1005Metric", Some("C25744")),
            e(&["R3"], "10k", "R_0603_1608Metric", Some("C99999")),
            e(&["R4"], "1k", "R_0603_1608Metric", Some("C21190")),
        ])
        .unwrap();
        assert_eq!(g.len(), 4);
    }

    #[test]
    fn blank_part_numbers_count_as_none_and_group_together() {
        let g = group(&[
            e(&["R1"], "10k", "R_0603_1608Metric", Some(" ")),
            e(&["R2"], "10k", "R_0603_1608Metric", None),
        ])
        .unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].lcsc, None);
    }

    #[test]
    fn rejects_duplicate_designators_and_bad_part_numbers() {
        assert_eq!(
            group(&[
                e(&["R1"], "10k", "a", Some("C1")),
                e(&["R1"], "1k", "a", Some("C2"))
            ]),
            Err(BomError::DuplicateDesignator("R1".into()))
        );
        assert!(matches!(
            group(&[e(&["R1"], "10k", "a", Some("25804"))]),
            Err(BomError::BadLcsc { .. })
        ));
        assert!(matches!(
            group(&[e(&[], "10k", "a", None)]),
            Err(BomError::NoDesignators { .. })
        ));
    }

    #[test]
    fn renders_header_and_quotes_designator_lists() {
        let g = group(&[
            e(&["R1", "R2"], "10k", "R_0603_1608Metric", Some("C25804")),
            e(&["D1"], "LED, red", "LED_0603_1608Metric", Some("C2286")),
        ])
        .unwrap();
        assert_eq!(
            render(&g),
            "Comment,Designator,Footprint,LCSC Part #\n\
             \"LED, red\",D1,LED_0603_1608Metric,C2286\n\
             10k,\"R1,R2\",R_0603_1608Metric,C25804\n"
        );
    }

    #[test]
    fn what_render_writes_parse_csv_reads() {
        let g = group(&[
            e(&["R1", "R2"], "10k", "R_0603_1608Metric", Some("C25804")),
            e(&["U1"], "AMS1117-3.3", "SOT-223-3_TabPin2", None),
        ])
        .unwrap();
        assert_eq!(group(&parse_csv(&render(&g)).unwrap()).unwrap(), g);
    }

    #[test]
    fn from_board_reads_the_named_property() {
        let board = Board::parse(crate::board::tests::SAMPLE).unwrap();
        let entries = from_board(&board, "LCSC Part #");
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].designators, ["R1"]);
        assert_eq!(entries[0].footprint, "R_0603_1608Metric");
        assert_eq!(entries[0].lcsc.as_deref(), Some("C25804"));
        assert_eq!(entries[1].lcsc, None);
    }

    #[test]
    fn parse_csv_names_a_missing_column() {
        assert!(matches!(
            parse_csv("Comment,Designator\n10k,R1\n"),
            Err(BomError::Csv(CsvError::MissingColumn(c))) if c == "Footprint"
        ));
    }
}
