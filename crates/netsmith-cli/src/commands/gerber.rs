use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args as ClapArgs, ValueEnum};
use netsmith_fab::viewer::{self, Package};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "`view gerber` reads the RS-274X and Excellon files of a fab package (the zip, or the folder; `board fab`'s output folder opens as its `<board>-gerbers.zip`, and a package another tool made is read the same way) with netsmith's own reader, no KiCad, and draws them as the board house will see them, or sums them up (issue #1105). A file's function comes from its X2 attribute or else its name (KiCad's `-F_Cu`, Protel's `.GTL`). A file with a construct the reader does not support (a step and repeat, an arc routed in a drill file) is not drawn at all: it is named with the line and the command, and the exit code is 1. The frame is the layout and routes files': mm from the outline's top left corner, y down. The PNG is filled by netsmith's own rasterizer from the SVG, so the two are one drawing, and the same package gives the same bytes. Exit 0 when every file was read, 1 when one could not be (the rest is drawn or summed up), 2 when the command cannot run (no such package, a layer the package has no file for or does not know, a picture too large)."
)]
pub struct Args {
    /// The package: a `.zip`, a folder of Gerber and drill files, or `board fab`'s output folder of a board.
    #[arg(value_name = "PACKAGE")]
    pub package: PathBuf,
    /// Repeatable: `top`, `bottom`, `inner<n>`, `all`, `outline`, `drill`, or
    /// `<top|bottom>-<copper|mask|paste|silk>`.
    ///
    /// What to draw, repeated to stack layers: `top` or `bottom` (that side's copper, mask
    /// openings, paste and silkscreen, the outline and the holes, the bottom mirrored as a person
    /// sees it), `inner<n>` (that copper layer), `all`, or one function alone: `top-copper`,
    /// `top-mask`, `top-paste`, `top-silk`, `bottom-copper`, `bottom-mask`, `bottom-paste`,
    /// `bottom-silk`, `outline` or `drill`.
    ///
    /// A word the package has no file for is refused with the ones it has (`--format json` lists them as `layer_names`). A stack of things of the bottom alone is drawn mirrored, as a person sees the bottom; with anything of the top it is drawn from the top.
    #[arg(
        long,
        value_name = "top|bottom|inner<n>|all|top-copper|top-mask|top-paste|top-silk|bottom-copper|bottom-mask|bottom-paste|bottom-silk|outline|drill",
        default_value = "top"
    )]
    pub layer: Vec<String>,
    /// Only this region: `X,Y,W,H` in mm from the outline's top left corner (the frame of the layout and routes files).
    ///
    /// With none, the box of the outline's centrelines and of what the layer's other files draw,
    /// with a millimetre round it: for a board netsmith made, the region `view pcb` draws.
    #[arg(long, value_name = "X,Y,W,H", value_parser = parse_region)]
    pub region: Option<[f64; 4]>,
    /// The PNG's pixels a millimetre (default 20, fewer for a picture longer than 8192 pixels).
    #[arg(long, value_name = "PX_PER_MM", value_parser = parse_scale)]
    pub scale: Option<f64>,
    /// `png` or `svg` writes the picture; `json` prints a summary of every file.
    ///
    /// The summary has each file's layers, extents, apertures, objects, holes by size, and the
    /// files not read.
    #[arg(long, value_enum, default_value = "png")]
    pub format: GerberFormat,
    /// The file to write (default `<package>-<layer>.<png|svg>` in the current folder).
    #[arg(long, value_name = "FILE")]
    pub picture: Option<PathBuf>,
}

/// `view gerber --format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum GerberFormat {
    Png,
    Svg,
    Json,
}

fn parse_region(s: &str) -> Result<[f64; 4], String> {
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    let bad = || format!("`{s}` is not X,Y,W,H in mm (four numbers, the width and height above 0)");
    let [x, y, w, h] = parts.as_slice() else {
        return Err(bad());
    };
    let n = |t: &str| t.parse::<f64>().ok().filter(|v| v.is_finite());
    match (n(x), n(y), n(w), n(h)) {
        (Some(x), Some(y), Some(w), Some(h)) if w > 0.0 && h > 0.0 => Ok([x, y, w, h]),
        _ => Err(bad()),
    }
}

fn parse_scale(s: &str) -> Result<f64, String> {
    s.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
        .ok_or_else(|| format!("`{s}` is not a number of pixels a millimetre above 0"))
}

/// The package's name without `.zip`, for the default file name.
fn stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "package".to_string());
    match name.strip_suffix(".zip") {
        Some(s) => s.to_string(),
        None => name,
    }
}

/// Says the files that could not be read; true when there is one.
fn say_unread(pkg: &Package) -> bool {
    let mut any = false;
    for (file, why) in pkg.unread() {
        eprintln!("error: {}: {why} (not drawn)", file.name);
        any = true;
    }
    any
}

pub fn run(args: Args) -> ExitCode {
    let pkg = match viewer::open(&args.package) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    if pkg.files.is_empty() {
        eprintln!(
            "error: {} has no Gerber or drill file",
            args.package.display()
        );
        return ExitCode::from(2);
    }
    if args.format == GerberFormat::Json {
        let summary = pkg.summary();
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).expect("the summary is JSON")
        );
        return if say_unread(&pkg) {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }
    let mut shows = Vec::new();
    for word in &args.layer {
        match pkg.parse_layer(word) {
            Ok(show) => shows.push(show),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        }
    }
    let drawing = match viewer::draw_stack(&pkg, &shows, args.region) {
        Ok(d) => d,
        Err(e) => {
            say_unread(&pkg);
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let (bytes, ext) = match args.format {
        GerberFormat::Svg => (drawing.svg.clone().into_bytes(), "svg"),
        _ => match viewer::png(&drawing, args.scale) {
            Ok(b) => (b, "png"),
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        },
    };
    let layers = shows
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("+");
    let out = args
        .picture
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("{}-{}.{ext}", stem(&args.package), layers)));
    if let Err(e) = std::fs::write(&out, bytes) {
        eprintln!("error: {}: {e}", out.display());
        return ExitCode::from(2);
    }
    println!("{}", out.display());
    if say_unread(&pkg) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_is_four_numbers_with_a_size() {
        assert_eq!(parse_region("1,2.5, 3,4"), Ok([1.0, 2.5, 3.0, 4.0]));
        assert!(parse_region("1,2,0,4").is_err());
        assert!(parse_region("1,2,3").is_err());
        assert!(parse_region("a,2,3,4").is_err());
    }

    #[test]
    fn the_default_file_is_named_for_the_package_and_layer() {
        assert_eq!(stem(Path::new("fab/led/led-gerbers.zip")), "led-gerbers");
        assert_eq!(stem(Path::new("fab/led")), "led");
    }
}
