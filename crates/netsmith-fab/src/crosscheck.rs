//! The cross-check: plot the same board with KiCad and compare KiCad's files with
//! netsmith's own, to find mistakes in the writers. KiCad is an independent witness here,
//! not a source of files: nothing it writes goes into the package.
//!
//! The method, chosen and measured (ADR 0034):
//!
//! - **Gerber layers** are rendered to bitmaps by this crate's own rasteriser
//!   ([`crate::raster`]), both sets over the same view and scale, and compared as bitmaps.
//!   That ignores everything that does not change the picture (how a pad is written, the
//!   order of objects, an arc cut into lines or kept an arc) and finds everything that
//!   does. The picture of a layer is compared twice: pixel by pixel (the fraction of lit
//!   pixels on which the two disagree) and with a tolerance (a pixel only counts where the
//!   other picture has nothing within `tolerance` millimetres).
//! - **Silkscreen** is compared with a coarser tolerance: the text is drawn with
//!   netsmith's own stroke font, so its strokes differ from KiCad's by design, while where
//!   the text sits and how big it is must agree.
//! - **Copper pours** are compared like copper, with their own limit: both fills keep the
//!   same clearances, but KiCad also removes islands and thin slivers and cuts its spokes
//!   its own way.
//! - **Drill files** are parsed and compared hole by hole.
//! - **The placement file** is compared row by row with KiCad's position file.
//! - **The design rules check** is compared finding by finding with KiCad's (rule and place
//!   of every item; [`crate::drc::kicad`]).

use crate::cpl;
use crate::kicad::{CliError, KicadCli, Output};
use crate::raster::{self, Image, RasterError};
use std::path::Path;

/// What is compared and how closely. The defaults are measured on the examples and the
/// synthetic boards (ADR 0034): about three times the worst difference seen on files that
/// are right, and well under what a wrong pad or track makes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Pixels per millimetre of the bitmaps.
    pub px_per_mm: f64,
    /// The most pixels, as a fraction of the lit ones, on which the two pictures of a layer
    /// may disagree (the edges of shapes differ by a pixel where one writer cuts an arc
    /// into lines and the other keeps it).
    pub pixels: f64,
    /// The distance, in millimetres, within which a lit pixel of one picture needs a lit
    /// pixel of the other.
    pub tolerance: f64,
    /// The most pixels with nothing of the other picture within the tolerance, as a fraction
    /// of the lit ones, for copper, mask, paste and outline.
    pub beyond_tolerance: f64,
    /// The same for a layer with a pour: spokes that KiCad drops and islands it removes
    /// are copper the other picture does not have.
    pub pour_beyond_tolerance: f64,
    /// The silkscreen's tolerance in millimetres: its text is another font, so only where
    /// and how large the text is must agree.
    pub silk_tolerance: f64,
    /// The silkscreen's most pixels beyond that tolerance.
    pub silk_beyond_tolerance: f64,
    /// Holes and placements: how far apart, in millimetres, two may be.
    pub position: f64,
    /// Keep a picture of the difference for every layer, not only the ones that differ too
    /// much.
    pub pictures: bool,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            px_per_mm: 40.0,
            pixels: 0.01,
            tolerance: 0.05,
            beyond_tolerance: 0.001,
            pour_beyond_tolerance: 0.005,
            silk_tolerance: 0.6,
            silk_beyond_tolerance: 0.05,
            position: 0.001,
            pictures: false,
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(String),
    Cli(CliError),
    Raster { file: String, error: RasterError },
    Cpl(cpl::CplError),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(m) => write!(f, "{m}"),
            Self::Cli(e) => write!(f, "{e}"),
            Self::Raster { file, error } => write!(f, "cannot read {file}: {error}"),
            Self::Cpl(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<CliError> for Error {
    fn from(e: CliError) -> Self {
        Self::Cli(e)
    }
}

/// How one Gerber layer compares.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerResult {
    pub file: String,
    /// What the comparison is: `exact`, `pour`, `silkscreen` (see the module docs).
    pub kind: &'static str,
    pub lit_ours: usize,
    pub lit_theirs: usize,
    /// The fraction of pixels the two pictures disagree on.
    pub pixels: f64,
    /// The fraction of the pixels with nothing of the other picture within the tolerance.
    pub tolerant: f64,
    pub ok: bool,
    /// A picture of the difference (PNG), kept for a person to look at when it is not ok.
    pub diff_png: Option<Vec<u8>>,
}

/// The result of a cross-check.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub layers: Vec<LayerResult>,
    /// Differences in the drill files, in words (empty when they match).
    pub drill: Vec<String>,
    /// Differences in the placement file, in words.
    pub placement: Vec<String>,
    /// A file one side has and the other does not.
    pub missing: Vec<String>,
    /// netsmith's design rules check against KiCad's.
    pub drc: crate::drc::kicad::Comparison,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.layers.iter().all(|l| l.ok)
            && self.drill.is_empty()
            && self.placement.is_empty()
            && self.missing.is_empty()
            && self.drc.agrees()
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "cross-check against KiCad: {}",
            if self.ok() {
                "the files agree"
            } else {
                "DIFFERENCES"
            }
        )?;
        for l in &self.layers {
            writeln!(
                f,
                "  {:<28} {:<10} {} lit (KiCad {}), differ {:.2}% of pixels, {:.2}% beyond the tolerance{}",
                l.file,
                l.kind,
                l.lit_ours,
                l.lit_theirs,
                l.pixels * 100.0,
                l.tolerant * 100.0,
                if l.ok { "" } else { "  <- too different" }
            )?;
        }
        for m in &self.missing {
            writeln!(f, "  {m}")?;
        }
        for d in &self.drill {
            writeln!(f, "  drill: {d}")?;
        }
        for p in &self.placement {
            writeln!(f, "  placement: {p}")?;
        }
        write!(f, "  {}", self.drc)?;
        Ok(())
    }
}

/// How a layer file is compared: from its name.
fn kind_of(file: &str, has_pour: bool) -> &'static str {
    if file.ends_with("Silkscreen.gbr") {
        "silkscreen"
    } else if has_pour && file.contains("_Cu.gbr") {
        "pour"
    } else {
        "exact"
    }
}

/// Plots the board at `pcb` with netsmith's writers and with KiCad's, and compares the two.
///
/// The files netsmith compares are the ones [`crate::package`] puts in the zip: the same
/// calls on the same file.
pub fn check(pcb: &Path, kicad: &dyn KicadCli, limits: &Limits) -> Result<Report, Error> {
    let io = |what: &str, e: &dyn std::fmt::Display| Error::Io(format!("{what}: {e}"));
    let src = std::fs::read_to_string(pcb)
        .map_err(|e| io(&format!("cannot read {}", pcb.display()), &e))?;
    let board = crate::board::Board::parse(&src).map_err(|e| io("the board", &e))?;
    let mut art = crate::artwork::Artwork::parse(&src).map_err(|e| io("the board", &e))?;
    // A panel marks its V-cut lines with text on the outline layer, in the font of each
    // writer. The text is left out of the comparison on both sides, as the silkscreen's
    // font is not compared, so that the lines themselves are compared exactly.
    let marked = art.texts.iter().any(|t| t.layer == "Edge.Cuts");
    let cut_lines = crate::edge::cut_lines(&art);
    let unmarked = if marked {
        art.texts.retain(|t| t.layer != "Edge.Cuts");
        Some(Unmarked::make(pcb, &src).map_err(|e| io("a copy of the board", &e))?)
    } else {
        None
    };
    let kicad_pcb: &Path = unmarked.as_ref().map_or(pcb, |u| u.pcb.as_path());
    let name = pcb
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let plotted =
        crate::plot::plot(&art, &name, &crate::plot_rules(pcb)).map_err(|e| io("plotting", &e))?;
    let mut ours = plotted.files;
    ours.extend(crate::drill::files(&art, &name));
    let layers =
        crate::gerber::plot_layers(&board.copper_layers).map_err(|e| io("the layers", &e))?;
    let parts: Vec<&crate::board::BoardPart> = board.parts.iter().collect();
    let rows = cpl::build(board.origin, &parts).rows;
    let pours: Vec<String> = art
        .zones
        .iter()
        .filter(|z| z.keepout.is_none())
        .flat_map(|z| z.layers.iter().cloned())
        .collect();
    let mut report = compare(
        kicad_pcb, &layers, &ours, &rows, &parts, &pours, kicad, limits,
    )?;
    // The design rules, with the project file beside the board as `package` has it.
    let mut theirs = crate::drc::kicad::parse(&kicad.drc_json(kicad_pcb)?)
        .map_err(|e| io("KiCad's check", &e))?;
    // KiCad calls a V-cut line on `Edge.Cuts` a malformed outline. JLCPCB wants it there, so
    // that finding is expected of a panel and is not compared.
    theirs.retain(|r| {
        !(r.kind == "invalid_outline"
            && r.items.iter().any(|(_, at)| {
                cut_lines
                    .iter()
                    .any(|(a, b)| at.dist(*a) <= DRC_TOLERANCE || at.dist(*b) <= DRC_TOLERANCE)
            }))
    });
    let ours = crate::drc::check(
        &src,
        crate::drc::project_of(pcb).as_deref(),
        &Default::default(),
    )
    .map_err(|e| io("the design rules check", &e))?;
    report.drc = crate::drc::kicad::compare(&ours, &theirs, DRC_TOLERANCE);
    Ok(report)
}

/// A copy of a panel board in a folder of its own with the text on `Edge.Cuts` left out, for
/// KiCad to plot; the folder is removed when it is dropped.
struct Unmarked {
    dir: std::path::PathBuf,
    pcb: std::path::PathBuf,
}

impl Unmarked {
    fn make(pcb: &Path, src: &str) -> Result<Self, std::io::Error> {
        let root = crate::sexp::parse(src)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let mut text = String::from("(kicad_pcb");
        for e in root.items().iter().skip(1) {
            let mark = e.head() == Some("gr_text")
                && e.child("layer").and_then(|l| l.items().get(1)?.text()) == Some("Edge.Cuts");
            if !mark {
                text.push_str("\n\t");
                e.write(&mut text);
            }
        }
        text.push_str("\n)\n");
        let name = pcb.file_name().unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("netsmith-crosscheck-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let out = dir.join(name);
        std::fs::write(&out, text)?;
        let pro = pcb.with_extension("kicad_pro");
        if pro.is_file() {
            std::fs::copy(&pro, dir.join(pro.file_name().unwrap_or_default()))?;
        }
        Ok(Self { dir, pcb: out })
    }
}

impl Drop for Unmarked {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// How far apart (mm) the places of one finding may be in the two reports.
pub const DRC_TOLERANCE: f64 = 0.1;

/// [`check`], on files already made.
#[allow(clippy::too_many_arguments)]
fn compare(
    pcb: &Path,
    layers: &[String],
    ours: &[Output],
    ours_cpl: &[cpl::Placement],
    parts: &[&crate::board::BoardPart],
    pours: &[String],
    kicad: &dyn KicadCli,
    limits: &Limits,
) -> Result<Report, Error> {
    let theirs = kicad.gerbers(pcb, layers)?;
    let theirs_drill = kicad.drill(pcb)?;
    let pos = kicad.pos(pcb)?;

    let mut report = Report {
        layers: Vec::new(),
        drill: Vec::new(),
        placement: Vec::new(),
        missing: Vec::new(),
        drc: Default::default(),
    };
    let raster_err = |file: &str| {
        let file = file.to_string();
        move |error| Error::Raster {
            file: file.clone(),
            error,
        }
    };

    // One view for every layer: the outline's box (ours), a millimetre round it.
    let edge = ours.iter().find(|o| o.name.ends_with("-Edge_Cuts.gbr"));
    let view = match edge {
        Some(e) => raster::gerber_extent(&String::from_utf8_lossy(&e.bytes))
            .map_err(raster_err(&e.name))?
            .grow(1.0),
        None => {
            report
                .missing
                .push("netsmith wrote no Edge_Cuts file".into());
            return Ok(report);
        }
    };

    let render = |o: &Output| {
        raster::render_gerber(&String::from_utf8_lossy(&o.bytes), &view, limits.px_per_mm)
            .map_err(raster_err(&o.name))
    };
    for layer in layers {
        let suffix = crate::gerber::file_suffix(layer);
        let ours_file = ours
            .iter()
            .find(|o| o.name.ends_with(&format!("-{suffix}.gbr")));
        let theirs_file = theirs
            .iter()
            .find(|o| o.name.ends_with(&format!("-{suffix}.gbr")));
        match (ours_file, theirs_file) {
            (Some(a), Some(b)) => {
                let (ia, ib) = (render(a)?, render(b)?);
                let kind = kind_of(&a.name, pours.contains(layer));
                report
                    .layers
                    .push(compare_layer(&a.name, kind, &ia, &ib, limits)?);
            }
            (None, Some(b)) => {
                // Netsmith leaves out an empty back layer, and KiCad writes it empty.
                let lit = render(b)?.lit();
                if lit > 0 {
                    report.missing.push(format!(
                        "netsmith wrote no {suffix} file but KiCad's has {lit} lit pixels"
                    ));
                }
            }
            (Some(a), None) => {
                report
                    .missing
                    .push(format!("KiCad wrote no {suffix} file ({})", a.name));
            }
            (None, None) => {}
        }
    }

    drill(&mut report, ours, &theirs_drill, limits)?;
    placement(&mut report, ours_cpl, parts, &pos, limits).map_err(Error::Cpl)?;
    Ok(report)
}

fn compare_layer(
    file: &str,
    kind: &'static str,
    ours: &Image,
    theirs: &Image,
    limits: &Limits,
) -> Result<LayerResult, Error> {
    let err = |error| Error::Raster {
        file: file.to_string(),
        error,
    };
    let strict = raster::compare(ours, theirs).map_err(err)?;
    let tolerance_mm = if kind == "silkscreen" {
        limits.silk_tolerance
    } else {
        limits.tolerance
    };
    let tolerance_px = (tolerance_mm * limits.px_per_mm).ceil() as usize;
    let tolerant = raster::compare_tolerant(ours, theirs, tolerance_px).map_err(err)?;
    // Silkscreen text differs by design, so only the tolerant comparison counts there.
    let ok = match kind {
        "silkscreen" => tolerant.fraction() <= limits.silk_beyond_tolerance,
        "pour" => {
            strict.fraction() <= limits.pixels
                && tolerant.fraction() <= limits.pour_beyond_tolerance
        }
        _ => strict.fraction() <= limits.pixels && tolerant.fraction() <= limits.beyond_tolerance,
    };
    let diff_png = (!ok || limits.pictures).then(|| raster::diff_image(ours, theirs));
    Ok(LayerResult {
        file: file.to_string(),
        kind,
        lit_ours: strict.lit_a,
        lit_theirs: strict.lit_b,
        pixels: strict.fraction(),
        tolerant: tolerant.fraction(),
        ok,
        diff_png,
    })
}

fn drill(
    report: &mut Report,
    ours: &[Output],
    theirs: &[Output],
    limits: &Limits,
) -> Result<(), Error> {
    for kind in ["PTH", "NPTH"] {
        let find = |files: &[Output]| {
            files
                .iter()
                .find(|o| o.name.ends_with(&format!("-{kind}.drl")))
                .map(|o| {
                    (
                        o.name.clone(),
                        String::from_utf8_lossy(&o.bytes).into_owned(),
                    )
                })
        };
        let (Some((name, a)), Some((_, b))) = (find(ours), find(theirs)) else {
            report
                .missing
                .push(format!("a {kind} drill file is missing on one side"));
            continue;
        };
        let parse = |text: &str| {
            raster::parse_excellon(text).map_err(|error| Error::Raster {
                file: name.clone(),
                error,
            })
        };
        let (mut mine, mut kicads) = (parse(&a)?, parse(&b)?);
        let near = |p: (f64, f64), q: (f64, f64)| {
            (p.0 - q.0).abs() <= limits.position && (p.1 - q.1).abs() <= limits.position
        };
        // Pair each of ours with one of KiCad's of the same size and place.
        let mut only_ours = Vec::new();
        for h in mine.drain(..) {
            let hit = kicads.iter().position(|k| {
                (k.diameter - h.diameter).abs() <= limits.position
                    && near((k.x, k.y), (h.x, h.y))
                    && match (k.slot_to, h.slot_to) {
                        (None, None) => true,
                        (Some(a), Some(b)) => near(a, b),
                        _ => false,
                    }
            });
            match hit {
                Some(i) => {
                    kicads.remove(i);
                }
                None => only_ours.push(h),
            }
        }
        for h in only_ours {
            report.drill.push(format!(
                "{name}: netsmith has a {} mm hole at ({}, {}) that KiCad does not",
                h.diameter, h.x, h.y
            ));
        }
        for k in kicads {
            report.drill.push(format!(
                "{name}: KiCad has a {} mm hole at ({}, {}) that netsmith does not",
                k.diameter, k.x, k.y
            ));
        }
    }
    Ok(())
}

fn placement(
    report: &mut Report,
    ours: &[cpl::Placement],
    parts: &[&crate::board::BoardPart],
    pos: &str,
    limits: &Limits,
) -> Result<(), cpl::CplError> {
    let theirs = cpl::from_kicad_pos(pos, parts)?;
    for t in &theirs.rows {
        match ours.iter().find(|o| o.designator == t.designator) {
            None => report.placement.push(format!(
                "{} is in KiCad's position file but not in cpl.csv",
                t.designator
            )),
            Some(o) => {
                if (o.x - t.x).abs() > limits.position
                    || (o.y - t.y).abs() > limits.position
                    || o.side != t.side
                    || (o.rotation - t.rotation).abs() > 1e-6
                {
                    report.placement.push(format!(
                        "{}: netsmith has ({}, {}) {:?} at {} degrees, KiCad ({}, {}) {:?} at {} degrees",
                        t.designator, o.x, o.y, o.side, o.rotation, t.x, t.y, t.side, t.rotation
                    ));
                }
            }
        }
    }
    for o in ours {
        if !theirs.rows.iter().any(|t| t.designator == o.designator) {
            report.placement.push(format!(
                "{} is in cpl.csv but not in KiCad's position file",
                o.designator
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Side;

    fn gerber(x: f64) -> String {
        format!(
            "%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\n%ADD10C,1.000000*%\n%ADD11C,0.200000*%\nD10*\nX{}Y5000000D03*\nD11*\nX0Y0D02*\nX9000000Y0D01*\nM02*\n",
            (x * 1e6) as i64
        )
    }

    fn image(text: &str) -> Image {
        let view = raster::View {
            min: (-1.0, -1.0),
            max: (11.0, 8.0),
        };
        raster::render_gerber(text, &view, 40.0).unwrap()
    }

    #[test]
    fn the_same_picture_agrees_and_a_pad_a_third_of_a_millimetre_away_does_not() {
        let limits = Limits::default();
        let same = compare_layer(
            "a.gbr",
            "exact",
            &image(&gerber(5.0)),
            &image(&gerber(5.0)),
            &limits,
        )
        .unwrap();
        assert!(same.ok && same.pixels == 0.0 && same.diff_png.is_none());
        let off = compare_layer(
            "a.gbr",
            "exact",
            &image(&gerber(5.0)),
            &image(&gerber(5.3)),
            &limits,
        )
        .unwrap();
        assert!(!off.ok, "{off:?}");
        assert!(off.diff_png.is_some(), "a picture of what differs is kept");
        // A shift of a pixel or two is the same picture to within the tolerance.
        let near = compare_layer(
            "a.gbr",
            "exact",
            &image(&gerber(5.0)),
            &image(&gerber(5.01)),
            &limits,
        )
        .unwrap();
        assert!(near.ok, "{near:?}");
    }

    #[test]
    fn silkscreen_only_has_to_be_in_the_same_place() {
        let limits = Limits::default();
        // A pad moved by 0.3 mm is within the silkscreen's 0.6 mm and not within copper's.
        let silk = compare_layer(
            "a-Silkscreen.gbr",
            "silkscreen",
            &image(&gerber(5.0)),
            &image(&gerber(5.3)),
            &limits,
        )
        .unwrap();
        assert!(silk.ok, "{silk:?}");
        let far = compare_layer(
            "a-Silkscreen.gbr",
            "silkscreen",
            &image(&gerber(5.0)),
            &image(&gerber(8.0)),
            &limits,
        )
        .unwrap();
        assert!(!far.ok, "{far:?}");
    }

    fn drl(holes: &str) -> Output {
        Output {
            name: "b-PTH.drl".into(),
            bytes: format!("M48\nMETRIC\nT1C0.300\n%\nG90\nG05\nT1\n{holes}M30\n").into_bytes(),
        }
    }

    #[test]
    fn a_hole_one_side_lacks_is_named_with_its_size_and_place() {
        let mut report = Report {
            layers: Vec::new(),
            drill: Vec::new(),
            placement: Vec::new(),
            missing: Vec::new(),
            drc: Default::default(),
        };
        let ours = [
            drl("X1.0Y-2.0\nX3.0Y-4.0\n"),
            Output {
                name: "b-NPTH.drl".into(),
                bytes: b"M48\nMETRIC\n%\nG90\nM30\n".to_vec(),
            },
        ];
        let theirs = [drl("X1.0Y-2.0\nX3.0Y-4.5\n"), ours[1].clone()];
        drill(&mut report, &ours, &theirs, &Limits::default()).unwrap();
        assert_eq!(report.drill.len(), 2, "{:?}", report.drill);
        assert!(
            report.drill[0].contains("netsmith has a 0.3 mm hole at (3, -4) that KiCad does not"),
            "{:?}",
            report.drill
        );
        assert!(
            report.drill[1].contains("KiCad has a 0.3 mm hole at (3, -4.5) that netsmith does not"),
            "{:?}",
            report.drill
        );
        assert!(!report.ok());
    }

    #[test]
    fn a_part_in_another_place_or_on_the_other_side_is_named() {
        let board = crate::board::Board::parse(crate::board::tests::SAMPLE).unwrap();
        let parts: Vec<&crate::board::BoardPart> = board
            .parts
            .iter()
            .filter(|p| p.designator == "R1")
            .collect();
        let pos =
            "Ref,Val,Package,PosX,PosY,Rot,Side\nR1,10k,R_0603_1608Metric,10.0,10.0,-90.0,top\n";
        let row = |x: f64, side: Side| cpl::Placement {
            designator: "R1".into(),
            x,
            y: 10.0,
            side,
            rotation: 270.0,
        };
        let mut report = Report {
            layers: vec![],
            drill: vec![],
            placement: vec![],
            missing: vec![],
            drc: Default::default(),
        };
        placement(
            &mut report,
            &[row(10.0, Side::Top)],
            &parts,
            pos,
            &Limits::default(),
        )
        .unwrap();
        assert!(report.placement.is_empty(), "{:?}", report.placement);
        placement(
            &mut report,
            &[row(10.5, Side::Top)],
            &parts,
            pos,
            &Limits::default(),
        )
        .unwrap();
        assert!(
            report.placement[0].contains(
                "R1: netsmith has (10.5, 10) Top at 270 degrees, KiCad (10, 10) Top at 270 degrees"
            ),
            "{:?}",
            report.placement
        );
    }

    #[test]
    fn the_report_says_what_agrees_and_what_does_not() {
        let layer = |ok| LayerResult {
            file: "b-F_Cu.gbr".into(),
            kind: "exact",
            lit_ours: 10,
            lit_theirs: 12,
            pixels: 0.5,
            tolerant: 0.25,
            ok,
            diff_png: None,
        };
        let mut report = Report {
            layers: vec![layer(true)],
            drill: vec![],
            placement: vec![],
            missing: vec![],
            drc: Default::default(),
        };
        assert!(
            report
                .to_string()
                .starts_with("cross-check against KiCad: the files agree\n")
        );
        report.layers.push(layer(false));
        let text = report.to_string();
        assert!(
            text.contains("DIFFERENCES")
                && text.contains(
                    "differ 50.00% of pixels, 25.00% beyond the tolerance  <- too different"
                ),
            "{text}"
        );
    }
}
