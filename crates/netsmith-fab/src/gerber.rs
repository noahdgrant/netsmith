//! The Gerber package: the zip JLCPCB takes. Which layers a board gets and what the files are
//! called is `netsmith-geom`'s ([`netsmith_geom::gerber_plot_layers`]); the files themselves are
//! made by [`netsmith_geom::plot_layer`] and [`netsmith_geom::drill_files`].
//!
//! The layout follows a package JLCPCB accepted (verified against that package, not in this repository): a top folder `gerber/` holding
//! `<board>-F_Cu.gbr`, `-B_Cu.gbr`, `-In1_Cu.gbr`..., `-F_Mask.gbr`, `-B_Mask.gbr`,
//! `-F_Paste.gbr`, `-F_Silkscreen.gbr`, `-Edge_Cuts.gbr`, `-PTH.drl` and `-NPTH.drl`.

use crate::kicad::Output;
use netsmith_geom::GerberError;
use std::io::Write as _;

/// The folder inside the zip.
pub(crate) const FOLDER: &str = "gerber";

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
