//! The Gerber layers: which layers a board gets and what the files are called. The files
//! themselves are made by [`crate::plot`] and [`crate::drill`]; the zip JLCPCB takes is
//! `netsmith_fab::gerber_zip`.
//!
//! The layout follows a package JLCPCB accepted (verified against that package, not in this repository): a top folder `gerber/` holding
//! `<board>-F_Cu.gbr`, `-B_Cu.gbr`, `-In1_Cu.gbr`..., `-F_Mask.gbr`, `-B_Mask.gbr`,
//! `-F_Paste.gbr`, `-F_Silkscreen.gbr`, `-Edge_Cuts.gbr`, `-PTH.drl` and `-NPTH.drl`.

#[derive(Debug, PartialEq, Eq)]
pub enum GerberError {
    /// The board's `(layers` list has no copper layers.
    NoCopper,
    /// The board outline is missing or not closed.
    Outline(String),
    /// A zone cannot be filled.
    Fill(String),
    /// A pad's margin cannot be applied to its shape.
    Pad {
        pad: String,
        layer: String,
        what: String,
    },
    Zip(String),
}

impl std::fmt::Display for GerberError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCopper => write!(f, "the board lists no copper layers (F.Cu, B.Cu, ...)"),
            Self::Outline(m) | Self::Fill(m) => write!(f, "{m}"),
            Self::Pad { pad, layer, what } => write!(f, "pad {pad} on {layer}: {what}"),
            Self::Zip(m) => write!(f, "cannot write the zip: {m}"),
        }
    }
}

impl std::error::Error for GerberError {}

/// `F.Cu` -> `F_Cu`, `F.SilkS` -> `F_Silkscreen`: the name KiCad gives the file.
pub fn file_suffix(layer: &str) -> String {
    match layer {
        "F.SilkS" => "F_Silkscreen".to_string(),
        "B.SilkS" => "B_Silkscreen".to_string(),
        other => other.replace('.', "_"),
    }
}

/// Layers to ask KiCad to plot: the board's copper layers, both masks, front paste and
/// silkscreen, the outline, and the back paste and silkscreen (kept only if not empty).
pub fn plot_layers(copper: &[String]) -> Result<Vec<String>, GerberError> {
    if copper.is_empty() {
        return Err(GerberError::NoCopper);
    }
    let mut layers: Vec<String> = copper.to_vec();
    layers.extend(
        [
            "F.Paste",
            "B.Paste",
            "F.Mask",
            "B.Mask",
            "F.SilkS",
            "B.SilkS",
            "Edge.Cuts",
        ]
        .map(str::to_string),
    );
    Ok(layers)
}

/// Layers that stay in the package even when nothing is drawn on them.
pub(crate) fn required(layer: &str) -> bool {
    !matches!(layer, "B.Paste" | "B.SilkS")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layers(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn suffixes_match_the_accepted_package() {
        assert_eq!(file_suffix("F.Cu"), "F_Cu");
        assert_eq!(file_suffix("In1.Cu"), "In1_Cu");
        assert_eq!(file_suffix("F.SilkS"), "F_Silkscreen");
        assert_eq!(file_suffix("Edge.Cuts"), "Edge_Cuts");
        assert_eq!(file_suffix("B.Mask"), "B_Mask");
    }

    #[test]
    fn plots_only_copper_the_board_has() {
        let two = plot_layers(&layers(&["F.Cu", "B.Cu"])).unwrap();
        assert!(!two.iter().any(|l| l.starts_with("In")));
        let four = plot_layers(&layers(&["F.Cu", "In1.Cu", "In2.Cu", "B.Cu"])).unwrap();
        assert!(four.contains(&"In2.Cu".to_string()));
        assert_eq!(plot_layers(&[]), Err(GerberError::NoCopper));
    }

    #[test]
    fn only_the_back_paste_and_silkscreen_may_be_left_out() {
        assert!(required("F.Paste") && required("B.Mask") && required("Edge.Cuts"));
        assert!(!required("B.Paste") && !required("B.SilkS"));
    }
}
