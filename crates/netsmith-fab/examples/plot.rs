//! Writes netsmith's Gerber and drill files for a board file into a directory.
//!
//! ```text
//! cargo run -p netsmith-fab --example plot -- board.kicad_pcb --out dir
//! ```

use netsmith_geom::{Artwork, PlotRules, drill_files, plot_layer};
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (mut pcb, mut out) = (None, PathBuf::from("."));
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = PathBuf::from(args.next().ok_or("--out needs a directory")?),
            _ => pcb = Some(PathBuf::from(a)),
        }
    }
    let pcb = pcb.ok_or("usage: plot <board.kicad_pcb> --out <dir>")?;
    let src = std::fs::read_to_string(&pcb).map_err(|e| format!("{}: {e}", pcb.display()))?;
    let art = Artwork::parse(&src).map_err(|e| e.to_string())?;
    let name = pcb
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let plotted = plot_layer(&art, &name, &PlotRules::default()).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    for f in plotted
        .files
        .iter()
        .cloned()
        .chain(drill_files(&art, &name))
    {
        std::fs::write(out.join(&f.name), &f.bytes).map_err(|e| e.to_string())?;
    }
    for n in plotted.notes {
        eprintln!("note: {n}");
    }
    Ok(())
}
