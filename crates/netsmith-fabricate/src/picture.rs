//! Pictures of the router's work as PNG files, for an agent that can open a picture and not an
//! SVG (`netsmith route --stages`, issue #431 S3b): one of each stage of the try that gave the
//! result ([`netsmith_router::Snapshot`]) and one of the crowding map ([`netsmith_router::Crowding`]).
//!
//! A stage's picture is the board's outline, its back copper (blue) and front copper (red) as
//! that stage left them, the pads and the vias, and on top of all the lines still left, drawn
//! bright (magenta, with a dot at each end), with a caption that names the stage and counts the
//! lines. The crowding picture is the board and its pads dimmed, with each 1 mm square that the
//! router's searches across other nets' copper passed through tinted from yellow (a few) to red
//! (the most). The long side of a picture is [`LONG_SIDE`] pixels; a board and its snapshots
//! give the same bytes every time. The fill is that of the Gerber rasteriser
//! ([`netsmith_fab::Canvas`]) and the caption's letters are fab's stroke font.

use netsmith_fab::{Canvas, RasterPoint, raster_disc};
use netsmith_geom::{ArtworkText, HAlign, VAlign};

use netsmith_pcb::{Board, CopperError, CopperLayer, Pt, Rect, copper_of};
use netsmith_router::{Crowding, Snapshot, SnapshotStage};

/// The pixels along the long side of a picture.
pub const LONG_SIDE: usize = 1600;

const BACKGROUND: [u8; 3] = [24, 24, 28];
const BOARD: [u8; 3] = [20, 56, 38];
const OUTLINE: [u8; 3] = [210, 210, 170];
const BACK_COPPER: [u8; 3] = [70, 120, 240];
const FRONT_COPPER: [u8; 3] = [225, 75, 55];
const PAD: [u8; 3] = [205, 175, 95];
const VIA: [u8; 3] = [200, 200, 200];
const LEFT: [u8; 3] = [255, 40, 255];
const CAPTION: [u8; 3] = [240, 240, 240];

/// The file a stage's picture is written to: `stage-1-first-pass.png` to `stage-4-final.png`,
/// numbered by the stage, so a try that had no negotiation has no `stage-3`.
pub fn stage_file(stage: SnapshotStage) -> &'static str {
    match stage {
        SnapshotStage::FirstPass => "stage-1-first-pass.png",
        SnapshotStage::RipUp => "stage-2-rip-up.png",
        SnapshotStage::Negotiation => "stage-3-negotiation.png",
        SnapshotStage::Final => "stage-4-final.png",
    }
}

/// Every name [`stage_file`] gives, and the crowding picture's.
pub const FILES: [&str; 5] = [
    "stage-1-first-pass.png",
    "stage-2-rip-up.png",
    "stage-3-negotiation.png",
    "stage-4-final.png",
    CROWDING_FILE,
];

/// The file the crowding map's picture is written to.
pub const CROWDING_FILE: &str = "crowding.png";

fn stage_words(stage: SnapshotStage) -> &'static str {
    match stage {
        SnapshotStage::FirstPass => "stage 1, after the first pass",
        SnapshotStage::RipUp => "stage 2, after the rip-ups",
        SnapshotStage::Negotiation => "stage 3, after the negotiation",
        SnapshotStage::Final => "stage 4, the end of the try",
    }
}

/// What every picture of one board shares: its frame, outline and pads, worked out once.
pub struct Painter {
    name: String,
    outline: Rect,
    cutouts: Vec<Rect>,
    /// The pads' copper on the two outer layers, as polygons.
    pads: Vec<Vec<RasterPoint>>,
    /// The drilled holes of the pads.
    holes: Vec<Vec<RasterPoint>>,
    origin: RasterPoint,
    px_per_mm: f64,
    size: (usize, usize),
    /// The caption's height and where its middle is, in mm.
    caption: (f64, RasterPoint),
}

fn pt(p: Pt) -> RasterPoint {
    (p.x, p.y)
}

impl Painter {
    /// The painter of `board`, placed: its tracks, vias and pours are not drawn (a stage
    /// brings its own copper).
    pub fn new(board: &Board) -> Result<Painter, CopperError> {
        let mut pads_only = board.clone();
        pads_only.tracks.clear();
        pads_only.vias.clear();
        pads_only.pours.clear();
        pads_only.planes.clear();
        let copper = copper_of(&pads_only)?;
        let mut pads = Vec::new();
        let outer = [copper.layers.first(), copper.layers.last()];
        for layer in outer.into_iter().flatten() {
            for pad in &layer.pads {
                for area in &pad.areas {
                    pads.push(area.outer.iter().copied().map(pt).collect());
                    pads.extend(
                        area.holes
                            .iter()
                            .map(|h| h.iter().copied().map(pt).collect()),
                    );
                }
            }
        }
        let holes = copper
            .holes
            .iter()
            .map(|h| match h.slot_to {
                Some(to) => netsmith_fab::raster_capsule(pt(h.at), pt(to), h.diameter / 2.0),
                None => raster_disc(pt(h.at), h.diameter / 2.0),
            })
            .collect();
        let outline = board.outline;
        let cutouts = board
            .cutouts
            .iter()
            .filter_map(|c| {
                let at = c.at?;
                Some(Rect::new(at, Pt::new(at.x + c.size.0, at.y + c.size.1)))
            })
            .collect();
        let long = outline.width().max(outline.height()).max(1.0);
        let margin = (long * 0.02).max(1.0);
        let text = long * 0.025;
        let band = text * 2.0;
        let (w, h) = (
            outline.width() + 2.0 * margin,
            outline.height() + 2.0 * margin + band,
        );
        let px_per_mm = LONG_SIDE as f64 / w.max(h);
        let size = (
            ((w * px_per_mm).round() as usize).max(1),
            ((h * px_per_mm).round() as usize).max(1),
        );
        let origin = (outline.min.x - margin, outline.min.y - margin - band);
        Ok(Painter {
            name: board.name.clone(),
            outline,
            cutouts,
            pads,
            holes,
            origin,
            px_per_mm,
            size,
            caption: (
                text,
                (origin.0 + margin, origin.1 + margin / 2.0 + band / 2.0),
            ),
        })
    }

    /// The pixels of every picture, width and height.
    pub fn size(&self) -> (usize, usize) {
        self.size
    }

    fn board(&self) -> Canvas {
        let mut c = Canvas::new(
            self.origin,
            self.px_per_mm,
            self.size.0,
            self.size.1,
            BACKGROUND,
        );
        let (min, max) = (pt(self.outline.min), pt(self.outline.max));
        c.rect(min, max, BOARD, 255);
        for cut in &self.cutouts {
            c.rect(pt(cut.min), pt(cut.max), BACKGROUND, 255);
        }
        let ring = vec![min, (max.0, min.1), max, (min.0, max.1), min];
        c.strokes(&[ring], self.line(1.5, 0.1), OUTLINE, 255);
        c
    }

    /// A line `px` pixels wide, or `mm` if that is wider.
    fn line(&self, px: f64, mm: f64) -> f64 {
        (px / self.px_per_mm).max(mm)
    }

    fn pads(&self, c: &mut Canvas, alpha: u8) {
        c.fill(&self.pads, PAD, alpha);
        c.fill(&self.holes, BOARD, alpha);
    }

    fn caption(&self, c: &mut Canvas, words: &str) {
        let (most, at) = self.caption;
        // A glyph advances about 0.95 of the text's size: a long caption is made smaller so
        // that it fits the picture's width.
        let room = self.size.0 as f64 / self.px_per_mm - 2.0 * (at.0 - self.origin.0);
        let size = most.min(room / (0.95 * words.chars().count().max(1) as f64));
        let text = ArtworkText {
            layer: String::new(),
            text: words.to_string(),
            at: netsmith_geom::Pt::new(at.0, at.1),
            angle: 0.0,
            size: (size, size),
            thickness: size * 0.12,
            mirror: false,
            italic: false,
            h: HAlign::Left,
            v: VAlign::Centre,
            owner: String::new(),
        };
        let (paths, _) = netsmith_geom::font_strokes(&text);
        let paths: Vec<Vec<RasterPoint>> = paths
            .iter()
            .map(|p| p.iter().map(|q| (q.x, q.y)).collect())
            .collect();
        c.strokes(&paths, size * 0.12, CAPTION, 255);
    }

    /// The PNG of a stage: its copper, the pads, and the lines it left drawn bright on top.
    pub fn stage(&self, stage: &Snapshot) -> Vec<u8> {
        let mut c = self.board();
        for (layer, colour) in [
            (CopperLayer::Back, BACK_COPPER),
            (CopperLayer::Front, FRONT_COPPER),
        ] {
            // One fill for each width, so that a layer's copper blends once where it overlaps.
            let mut by_width: Vec<(f64, Vec<Vec<RasterPoint>>)> = Vec::new();
            for t in stage.tracks.iter().filter(|t| t.layer == layer) {
                let polys = match by_width.iter_mut().find(|(w, _)| *w == t.width) {
                    Some((_, polys)) => polys,
                    None => {
                        by_width.push((t.width, Vec::new()));
                        &mut by_width.last_mut().expect("just pushed").1
                    }
                };
                polys.push(t.centre_line().into_iter().map(pt).collect());
            }
            for (width, paths) in &by_width {
                c.strokes(paths, *width, colour, 220);
            }
        }
        self.pads(&mut c, 255);
        let rings: Vec<Vec<RasterPoint>> = stage
            .vias
            .iter()
            .map(|v| raster_disc(pt(v.at), v.diameter / 2.0))
            .collect();
        c.fill(&rings, VIA, 255);
        let drills: Vec<Vec<RasterPoint>> = stage
            .vias
            .iter()
            .map(|v| raster_disc(pt(v.at), v.drill / 2.0))
            .collect();
        c.fill(&drills, BACKGROUND, 255);
        let lines: Vec<&netsmith_pcb::Ratline> =
            stage.left.iter().filter(|l| !l.to_plane).collect();
        let paths: Vec<Vec<RasterPoint>> = lines
            .iter()
            .map(|l| vec![pt(l.from_at), pt(l.to_at)])
            .collect();
        let width = self.line(4.0, 0.25);
        c.strokes(&paths, width, LEFT, 255);
        let ends: Vec<Vec<RasterPoint>> = stage
            .left
            .iter()
            .flat_map(|l| {
                [
                    raster_disc(pt(l.from_at), width * 1.6),
                    raster_disc(pt(l.to_at), width * 1.6),
                ]
            })
            .collect();
        c.fill(&ends, LEFT, 255);
        self.caption(
            &mut c,
            &format!(
                "{}: {}: {} left, {} tracks, {} vias",
                self.name,
                stage_words(stage.after),
                match stage.left.len() {
                    1 => "1 line".to_string(),
                    n => format!("{n} lines"),
                },
                stage.tracks.len(),
                stage.vias.len()
            ),
        );
        c.to_png()
    }

    /// The PNG of the crowding map over the board and its pads, dimmed.
    pub fn crowding(&self, crowding: &Crowding) -> Vec<u8> {
        let mut c = self.board();
        self.pads(&mut c, 120);
        let most = crowding.counts.iter().copied().max().unwrap_or(0);
        let first = crowding.counts.iter().position(|&n| n == most).unwrap_or(0);
        let at_most = (first % crowding.nx.max(1), first / crowding.nx.max(1));
        if most > 0 {
            // One fill for each count, so the squares of a count are drawn together.
            let mut counts: Vec<u32> = crowding.counts.iter().copied().filter(|&n| n > 0).collect();
            counts.sort_unstable();
            counts.dedup();
            for n in counts {
                let mut squares = Vec::new();
                for b in 0..crowding.ny {
                    for a in 0..crowding.nx {
                        if crowding.counts[b * crowding.nx + a] != n {
                            continue;
                        }
                        let x = crowding.origin.x + a as f64 * crowding.cell;
                        let y = crowding.origin.y + b as f64 * crowding.cell;
                        let s = crowding.cell;
                        squares.push(vec![(x, y), (x + s, y), (x + s, y + s), (x, y + s)]);
                    }
                }
                let t = f64::from(n) / f64::from(most);
                let colour = [255, (230.0 * (1.0 - t)).round() as u8, 0];
                let alpha = (70.0 + 185.0 * t).round() as u8;
                c.fill(&squares, colour, alpha);
            }
        }
        let words = if most == 0 {
            format!(
                "{}: crowding: no search crossed other nets' copper",
                self.name
            )
        } else {
            // From the outline's top left, as `routing-failure.txt` gives places.
            let x = at_most.0 as f64 * crowding.cell;
            let y = at_most.1 as f64 * crowding.cell;
            format!(
                "{}: crowding: at most {most} paths in a {} mm square, at x {x:.0}, y {y:.0} mm",
                self.name, crowding.cell
            )
        };
        self.caption(&mut c, &words);
        c.to_png()
    }
}
