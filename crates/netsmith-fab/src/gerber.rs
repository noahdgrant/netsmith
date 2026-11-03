//! The Gerber package: which layers a board gets, what the files are called, and the zip
//! JLCPCB takes. The files themselves are made by [`crate::plot`] and [`crate::drill`].
//!
//! The layout follows a package JLCPCB accepted (verified against that package, not in this repository): a top folder `gerber/` holding
//! `<board>-F_Cu.gbr`, `-B_Cu.gbr`, `-In1_Cu.gbr`..., `-F_Mask.gbr`, `-B_Mask.gbr`,
//! `-F_Paste.gbr`, `-F_Silkscreen.gbr`, `-Edge_Cuts.gbr`, `-PTH.drl` and `-NPTH.drl`.

use crate::kicad::Output;
use std::io::Write as _;

/// The folder inside the zip.
pub const FOLDER: &str = "gerber";

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

/// A zip with the folder entry `gerber/` and then each file inside it. Every entry has the
/// same fixed timestamp (1980-01-01) and permissions, so equal input gives equal bytes.
pub fn zip(files: &[Output]) -> Result<Vec<u8>, GerberError> {
    use zip::write::SimpleFileOptions;
    let err = |e: &dyn std::fmt::Display| GerberError::Zip(e.to_string());
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .add_directory(format!("{FOLDER}/"), options.unix_permissions(0o755))
        .map_err(|e| err(&e))?;
    for f in files {
        writer
            .start_file(
                format!("{FOLDER}/{}", f.name),
                options.unix_permissions(0o644),
            )
            .map_err(|e| err(&e))?;
        writer.write_all(&f.bytes).map_err(|e| err(&e))?;
    }
    Ok(writer.finish().map_err(|e| err(&e))?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read as _;

    fn out(name: &str, text: &str) -> Output {
        Output {
            name: name.into(),
            bytes: text.as_bytes().to_vec(),
        }
    }

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

    #[test]
    fn zip_has_a_gerber_folder_and_is_deterministic() {
        let files = vec![
            out("b-B_Cu.gbr", "%FSLAX46Y46*%\n"),
            out("b-PTH.drl", "M48\n"),
        ];
        let a = zip(&files).unwrap();
        assert_eq!(a, zip(&files).unwrap());
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(a)).unwrap();
        let listed: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();
        assert_eq!(listed, ["gerber/", "gerber/b-B_Cu.gbr", "gerber/b-PTH.drl"]);
        let mut text = String::new();
        archive
            .by_name("gerber/b-PTH.drl")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "M48\n");
        let stamp = archive
            .by_name("gerber/b-B_Cu.gbr")
            .unwrap()
            .last_modified();
        assert_eq!(stamp, Some(zip::DateTime::default()));
    }
}
