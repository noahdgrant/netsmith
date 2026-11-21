//! `netsmith view pcb` (issue #1086): a 2D picture of a board's copper, as a PNG or an SVG. The
//! copper and the file are `netsmith_fabricate::pcb_picture`'s, the drawing
//! `netsmith_pcb::picture_svg`'s.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args as ClapArgs, ValueEnum};
use netsmith_fabricate::pcb_picture::{self, Format, Request, Route};
use netsmith_pcb::{CopperLayer, PictureLayers};

use super::common::{self, Ids, parse_ids};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`view pcb` draws a board's copper as the 2D PCB view of the editor does, with no editor, no browser, no GPU and no KiCad: the board placed (the layout file read, the placer run for the rest), the copper of the routes file beside the source, and the router's copper the editor keeps (`.netsmith/routes/`, what `board fab --reuse-routes` starts from) for every net whose pads have not moved since it was routed, with the teardrops the router draws on the finished board, as the Gerbers have them. It routes nothing unless `--route` is given, which runs the router first, from that copper, as `fab --reuse-routes` does; nothing is written but the picture. The open connections are what that copper leaves: the connections `routes show` lists as left to route, drawn as bright straight lines from pad to pad.\n\nWhat is drawn, from the bottom up: the board's shape, the keepouts of the layers drawn (dashed), each layer's pours and planes (translucent, as the Gerbers fill them), teardrop fills, pads and tracks, the layer furthest from the eye first; the vias and the holes; then, when asked, the courtyards and the designators; with `--stitching` the stitching vias the export will have that the routes file does not (the stitching `fab` runs, on the copper drawn), in a cooler grey than the other vias, and their count printed; with `--islands` the copper islands the fill of the pours took out (copper cut off from its net, as `board fill` lists them), hatched and outlined, dashed, in near white and labelled `I1`, `I2`, ...; the open connections; and with `--drc` the places of netsmith's design rules check (the check `board drc` and `fab` run) as numbered rings, red for an error and amber for a warning, with each finding printed after its number. A missing connection is not marked: it is an open connection. A pad the fill starves (fewer than two spokes and tracks join it to the pour) is the check's `starved_thermal` finding, so `--drc` marks it. Each pour is drawn as its fill, as the Gerbers have it: the clearances round other nets' copper, the thermal reliefs and their spokes. The top layer is orange, the bottom teal, each inner layer a tone of its own. Seen from below the board is mirrored left to right, as the editor's Side switch shows the back.\n\nThe PNG is drawn by netsmith's own rasterizer from the SVG (ADR 0142), so the two are the same drawing, and the same board and flags give the same bytes. The path written is printed, then each open connection (`open: net GND: D1.1 to J1.2`), each marker and each removed island (`removed island I1: bottom GND at (12.000, 3.500), 2.150 mm2`, the place a corner of it in the layout file's frame). The exit code is 0 when the picture is written, 1 when the board does not build or place, and 2 when the command cannot run: a layer or an id the board does not have (the error lists those it has), a picture of more than 16384 pixels a side, or a file that cannot be written."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board, by name, when the source has more than one.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// The copper layer to draw (`top`, `bottom`, `inner1`, ...), or `all` of them.
    ///
    /// `bottom` is seen from below unless `--side` says otherwise; the others from above.
    #[arg(long, value_name = "LAYER", default_value = "all", value_parser = parse_layer)]
    pub layer: PictureLayers,
    /// The side the board is seen from: `bottom` mirrors it left to right, as the editor's
    /// Side switch shows the back.
    #[arg(long, value_enum)]
    pub side: Option<SideName>,
    /// Only this region: `X,Y,W,H` in mm from the outline's top left corner, as the layout file
    /// and the routes file give places.
    ///
    /// The region is in the board's own frame seen from above, also when it is drawn from
    /// below. With none, the whole board with 1 mm round it.
    #[arg(long, value_name = "X,Y,W,H", value_parser = parse_region)]
    pub region: Option<[f64; 4]>,
    /// Pixels a millimetre.
    #[arg(long, value_name = "PX_PER_MM", default_value_t = pcb_picture::DEFAULT_SCALE, value_parser = parse_scale)]
    pub scale: f64,
    /// What to write: a PNG, or the SVG it is drawn from.
    #[arg(long, value_enum, default_value = "png")]
    pub format: PcbFormat,
    /// The file to write (default `<board>-pcb.<png|svg>`, `<board>-pcb-<layer>.<png|svg>` for
    /// one layer).
    #[arg(long, value_name = "FILE")]
    pub picture: Option<PathBuf>,
    /// Light a part (by designator or path), `net:<name>` or `module:<path>` and fade the rest
    /// (comma-separated or repeated).
    ///
    /// A part lights its pads, courtyard and designator; a net its copper and its open
    /// connections; a module (its path in the design, as `layout summary` names it) its parts,
    /// each part's box filled and outlined in the module's colour with a dashed box round them
    /// all, each module named in its own colour. An id the board does not have exits 2 with the
    /// list of the ones it has.
    #[arg(long, value_name = "ID[,ID...]", value_parser = parse_ids)]
    pub highlight: Vec<Ids>,
    /// Light the nets that have open connections and fade the rest.
    #[arg(long)]
    pub only_open: bool,
    /// Draw the parts' courtyards.
    #[arg(long)]
    pub courtyards: bool,
    /// Draw the parts' designators.
    #[arg(long)]
    pub designators: bool,
    /// Mark the places of the design rules check's findings, numbered, and print them.
    ///
    /// A pad the fill of a pour starves (joined by fewer than two spokes and tracks, as `board
    /// fill` lists it) is the check's `starved_thermal` finding, so it is marked with the rest.
    #[arg(long)]
    pub drc: bool,
    /// Hatch and outline the copper islands the fill of the pours took out, labelled `I1`, `I2`,
    /// ..., and print them: copper cut off from its net, which the Gerbers do not have.
    #[arg(long)]
    pub islands: bool,
    /// Draw the stitching vias the export will have that the routes file does not, in a tone of
    /// their own, and print how many.
    ///
    /// They are computed by the stitching `board fab` runs, on the copper drawn: on copper not
    /// routed yet (no `--route`) the router's tracks may still move a via a little, or take its
    /// place.
    #[arg(long)]
    pub stitching: bool,
    /// Route the board first, from the routes file's and the kept copper, as `board fab
    /// --reuse-routes` does.
    #[arg(long)]
    pub route: bool,
    /// Seconds the router may take with `--route`; 0 is no limit (default 300).
    #[arg(long, value_name = "SECS", requires = "route")]
    pub route_timeout: Option<u64>,
    #[command(flatten)]
    pub placing: common::Placing,
}

/// `view pcb --side`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SideName {
    Top,
    Bottom,
}

/// `view pcb --format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PcbFormat {
    Png,
    Svg,
}

fn parse_layer(given: &str) -> Result<PictureLayers, String> {
    let layer = match given {
        "all" => return Ok(PictureLayers::All),
        "top" => CopperLayer::Front,
        "bottom" => CopperLayer::Back,
        _ => match given
            .strip_prefix("inner")
            .and_then(|n| n.parse::<u8>().ok())
        {
            Some(n @ 1..=30) => CopperLayer::Inner(n),
            _ => {
                return Err(
                    "a copper layer: `top`, `bottom`, `inner1` to `inner30`, or `all`".to_string(),
                );
            }
        },
    };
    Ok(PictureLayers::One(layer))
}

fn parse_region(given: &str) -> Result<[f64; 4], String> {
    let numbers: Vec<f64> = given
        .split(',')
        .map(|v| v.trim().parse::<f64>())
        .collect::<Result<_, _>>()
        .map_err(|_| "four numbers in mm, `X,Y,W,H`".to_string())?;
    match numbers[..] {
        [x, y, w, h] if numbers.iter().all(|v| v.is_finite()) => Ok([x, y, w, h]),
        _ => Err("four numbers in mm, `X,Y,W,H`".to_string()),
    }
}

fn parse_scale(given: &str) -> Result<f64, String> {
    match given.parse::<f64>() {
        Ok(v) if v.is_finite() && v > 0.0 && v <= 1000.0 => Ok(v),
        _ => Err("pixels a millimetre, more than 0 and at most 1000".to_string()),
    }
}

impl Args {
    fn request(&self) -> Request {
        Request {
            layers: self.layer,
            from_below: self.side.map(|s| s == SideName::Bottom),
            region: self.region,
            scale: self.scale,
            courtyards: self.courtyards,
            designators: self.designators,
            highlight: self
                .highlight
                .iter()
                .flat_map(|ids| ids.0.clone())
                .collect(),
            only_open: self.only_open,
            drc: self.drc,
            islands: self.islands,
            stitching: self.stitching,
            format: match self.format {
                PcbFormat::Png => Format::Png,
                PcbFormat::Svg => Format::Svg,
            },
        }
    }

    /// `<board>-pcb.<ext>`, or `<board>-pcb-<layer>.<ext>`, with what a file name cannot hold
    /// as `-`.
    fn default_out(&self, label: &str) -> PathBuf {
        let clean: String = label
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let layer = match self.layer {
            PictureLayers::All => String::new(),
            PictureLayers::One(l) => format!(
                "-{}",
                netsmith_pcb::routes_layer_word(l.name()).unwrap_or_default()
            ),
        };
        let ext = match self.format {
            PcbFormat::Png => "png",
            PcbFormat::Svg => "svg",
        };
        PathBuf::from(format!("{clean}-pcb{layer}.{ext}"))
    }
}

/// Draws one board's copper and writes it. Exit 0 when it is written, 1 when the board does not
/// build or place, 2 when the request cannot be drawn or the file cannot be written.
pub fn run(args: Args) -> ExitCode {
    let request = args.request();
    let route = route_of(args.route, args.route_timeout);
    let drawn = common::with_stage_events(|| {
        common::with_placed_board(
            args.file.as_deref(),
            args.board.as_deref(),
            Some(&args.placing),
            |placed| {
                placed.with_board(|built, board| {
                    let label = built.label();
                    // A mistake in the flags is said before the copper is put on, which may route.
                    if let Err(e) = pcb_picture::check(board, &request) {
                        return Ok(Err(e));
                    }
                    let mut board = board.clone();
                    let say = |line: String| eprintln!("{label}: {line}");
                    pcb_picture::put_copper(
                        &mut board,
                        &placed.layout_of(built).file,
                        route.as_ref(),
                        &say,
                    )
                    .map_err(|e| format!("error: {label}: {e}\n"))?;
                    let constraints = args.drc.then(|| {
                        netsmith_pipeline::constraints::of_design(
                            &built.design,
                            placed.catalogs.constraints_warn(),
                        )
                    });
                    Ok(pcb_picture::draw(&board, &request, constraints.flatten())
                        .map(|d| (d, label.clone())))
                })?
            },
        )
    });
    let (drawn, label) = match drawn {
        Ok(Ok(made)) => made,
        Ok(Err(e)) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
        Err(code) => return code,
    };
    let out = args
        .picture
        .clone()
        .unwrap_or_else(|| args.default_out(&label));
    if let Err(e) = fs::write(&out, &drawn.bytes) {
        eprintln!("error: {}: {e}", out.display());
        return ExitCode::from(2);
    }
    println!("{}", out.display());
    for line in &drawn.open {
        println!("open: {line}");
    }
    for line in &drawn.markers {
        println!("marker {line}");
    }
    for line in &drawn.islands {
        println!("removed island {line}");
    }
    if args.stitching {
        println!(
            "stitching: {} via(s) the export will have that the routes file does not",
            drawn.stitching
        );
    }
    ExitCode::SUCCESS
}

/// What `--route` and `--route-timeout` ask of the router, as `board fab --reuse-routes` routes:
/// `None` without `--route`. `view pcb` and `board nets` take the two flags.
pub(super) fn route_of(route: bool, timeout: Option<u64>) -> Option<Route> {
    route.then(|| Route {
        weights: netsmith_fabricate::fab::project_weights(project().as_ref()),
        timeout: match timeout {
            Some(0) => None,
            Some(secs) => Some(Duration::from_secs(secs)),
            None => netsmith_fabricate::fab::FabOptions::default().route_timeout,
        },
        rip_rounds: netsmith_fabricate::fab::rip_rounds(project().as_ref()),
        track_margin: project()
            .and_then(|p| p.manifest.track_margin)
            .unwrap_or(netsmith_fabricate::fab::DEFAULT_TRACK_MARGIN),
    })
}

/// The project of the current folder, if there is one: its `rip_rounds`, `track_margin` and routing weights.
fn project() -> Option<netsmith_fabricate::project::Project> {
    netsmith_fabricate::project::Target::resolve(None)
        .ok()
        .and_then(|t| t.project)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::common::testing::{Leaf, kind, parse};
    use clap::error::ErrorKind;

    #[test]
    fn view_pcb_takes_a_layer_a_side_a_region_and_a_scale() {
        let Ok(Leaf::Pcb(a)) = parse(&["view", "pcb"]) else {
            panic!("view pcb should parse");
        };
        assert_eq!(a.layer, PictureLayers::All);
        assert_eq!((a.side, a.region, a.scale), (None, None, 20.0));
        assert_eq!(a.format, PcbFormat::Png);
        let Ok(Leaf::Pcb(a)) = parse(&[
            "view",
            "pcb",
            "a.nsm",
            "--layer",
            "inner2",
            "--side",
            "bottom",
            "--region",
            "1,2.5,10,8",
            "--scale",
            "40",
            "--format",
            "svg",
            "--highlight",
            "R1,net:GND",
            "--only-open",
            "--drc",
            "--islands",
            "--stitching",
            "--route",
            "--route-timeout",
            "30",
        ]) else {
            panic!("view pcb should parse");
        };
        assert_eq!(a.layer, PictureLayers::One(CopperLayer::Inner(2)));
        assert_eq!(a.side, Some(SideName::Bottom));
        assert_eq!(a.region, Some([1.0, 2.5, 10.0, 8.0]));
        assert_eq!(a.request().highlight, ["R1", "net:GND"]);
        assert_eq!(a.route_timeout, Some(30));
        assert!(a.request().drc && a.request().islands && a.request().stitching);
        assert_eq!(a.default_out("main"), PathBuf::from("main-pcb-inner2.svg"));
    }

    #[test]
    fn a_layer_a_region_or_a_scale_that_is_none_is_refused() {
        for args in [
            &["view", "pcb", "--layer", "middle"][..],
            &["view", "pcb", "--layer", "inner0"],
            &["view", "pcb", "--region", "1,2,3"],
            &["view", "pcb", "--region", "a,b,c,d"],
            &["view", "pcb", "--scale", "0"],
            &["view", "pcb", "--scale=-2"],
        ] {
            assert_eq!(kind(args), ErrorKind::ValueValidation, "{args:?}");
        }
        assert_eq!(
            kind(&["view", "pcb", "--route-timeout", "10"]),
            ErrorKind::MissingRequiredArgument
        );
    }
}
