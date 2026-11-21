//! `netsmith view pcb` (issue #1086): a 2D picture of a board's copper, a PNG or an SVG, with no
//! browser, no GPU and no KiCad. The drawing is [`netsmith_pcb::picture_svg`]'s (what is drawn
//! and how is in its header); this module puts the copper on the board and writes the file.
//!
//! **The copper drawn** is what `board fab --reuse-routes` starts from: the board as placed (the
//! layout file read, the placer run for the rest), the routes file's copper on it
//! ([`crate::routes::apply_beside`]), and the router's copper the editor keeps on disk
//! ([`crate::kept_routes`], `.netsmith/routes/`) for every net whose pads are where they were when
//! it was routed, carried as a route carries it ([`netsmith_router::carry::carry_copper`]) and
//! with no router. A kept file that is stale or cannot be read is not used, and the reason is
//! said. The teardrops are drawn on that copper as the router's last pass draws them on the
//! board `fab` writes ([`netsmith_router::teardrop::add`]): no file keeps them. With a
//! [`Route`] (`--route`) the router runs first, from that copper, as `fab --reuse-routes` runs
//! it, and the picture is of the board it leaves; nothing is written but the picture, the kept
//! file neither. The open connections are what the copper leaves
//! ([`netsmith_pcb::Ratsnest`], the connections `netsmith routes show` lists as left to route).
//!
//! **The markers** (`--drc`) are the places of netsmith's design rules check of the board drawn
//! ([`crate::drc::of_board`], the check `fab` and `board drc` run), numbered in the order the
//! check lists its findings. A missing connection is not marked: it is drawn as an open
//! connection. A pad the fill of a pour starves (joined to it by fewer than two spokes and
//! tracks) is the check's `starved_thermal` finding, so `--drc` marks it: the check judges the
//! fill the picture draws, and its starved pads are those of `board fill`
//! ([`netsmith_pcb::FillReport`]), so a flag of its own would ring the same pad twice.
//!
//! **The removed islands** (`--islands`, issue #1188) are the copper islands the fill of each
//! pour took out ([`netsmith_pcb::FillReport`]'s `removed_islands`, what `board fill` lists):
//! copper the pour would have had that touched no copper of its net, so the Gerbers have none
//! there. Each is hatched and outlined on its layer and labelled `I1`, `I2`, ... in the order of
//! the report, apart from the numbers of the markers.
//!
//! **The stitching** (`--stitching`, issue #1489) is the grid of stitching vias the export will
//! have and the routes file does not ([`netsmith_router::stitching_vias`], the function `fab`
//! stitches with): the stitching vias the router already laid on the copper drawn, then the
//! vias the stitching would add to it. They are put on the board drawn, so the fill clears round
//! them and `--drc` checks them, and drawn in a tone of their own. On copper that is not routed
//! yet (no `--route`) the router's tracks may still move a via within a third of the pitch, or
//! take its point.
//!
//! **The PNG** is the SVG filled by `netsmith-draw` (ADR 0142) at the scale asked, so the two
//! are the same drawing, and the same board gives the same bytes.

use std::path::Path;
use std::time::Duration;

use netsmith_geom::Constraints;
use netsmith_pcb::{
    Board, FillReport, Lit, Marker, PictureIsland, PictureLayers, PictureOptions, Pt, Ratline,
    Ratsnest, Rect, picture_svg,
};
use netsmith_router::RouteOptions;

use crate::kept_routes::KeptRoutes;

/// Pixels a millimetre of `view pcb` when `--scale` is not given, and of the pictures of a
/// package made for review (`board fab --for-review`, issue #1213).
pub const DEFAULT_SCALE: f64 = 20.0;

/// What file to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Png,
    Svg,
}

/// The router run before the picture (`--route`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Route {
    /// `None` is no limit.
    pub timeout: Option<Duration>,
    pub rip_rounds: usize,
    pub track_margin: f64,
}

/// What `netsmith view pcb` asks for.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub layers: PictureLayers,
    /// Seen from below; `None` is from below for the bottom layer alone and from above
    /// otherwise.
    pub from_below: Option<bool>,
    /// `x`, `y`, `width`, `height` in mm, from the outline's top left corner (the frame of the
    /// layout file and the routes file); `None` is the whole board.
    pub region: Option<[f64; 4]>,
    /// Pixels a millimetre.
    pub scale: f64,
    pub courtyards: bool,
    pub designators: bool,
    /// The ids of `--highlight`: a part by designator or path, `net:<name>`, `module:<path>`.
    pub highlight: Vec<String>,
    /// Light the nets with open connections.
    pub only_open: bool,
    /// Mark the places of the design rules check's findings.
    pub drc: bool,
    /// Hatch and number the copper islands the fill took out.
    pub islands: bool,
    /// Draw the stitching vias the export will have that the routes file does not, in a tone of
    /// their own (issue #1489).
    pub stitching: bool,
    pub format: Format,
}

/// A picture made.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    pub bytes: Vec<u8>,
    /// Its size in pixels.
    pub width: usize,
    pub height: usize,
    /// The open connections, as `routes show` words them (`net GND: D1.1 to J1.2`).
    pub open: Vec<String>,
    /// The findings marked, each with its number (`1: error [clearance] ...`), in the order
    /// drawn.
    pub markers: Vec<String>,
    /// The removed islands drawn, each with its label (`I1: bottom GND at (12.000, 3.500), 2.150
    /// mm2`), places in the frame of the layout file.
    pub islands: Vec<String>,
    /// The stitching vias drawn (`--stitching`): those the export will have that the routes
    /// file does not.
    pub stitching: usize,
}

/// Put on `board`, placed and with no copper, the copper the picture draws (the module's
/// header): the routes file beside `layout` and the copper kept for it, or with `route` what
/// the router leaves. What is worth saying (a record not used, a kept file not used, how much
/// was carried, the router's end) goes to `say`.
pub fn put_copper(
    board: &mut Board,
    layout: &Path,
    route: Option<&Route>,
    say: &dyn Fn(String),
) -> Result<(), String> {
    for note in crate::routes::apply_beside(board, Some(layout))? {
        say(format!("warning: {note}"));
    }
    let kept = KeptRoutes::beside_layout(layout);
    let from = kept.start_from(board).unwrap_or_else(|why| {
        say(format!("warning: {why}"));
        None
    });
    let Some(route) = route else {
        match from {
            Some(from) => {
                let carried = netsmith_router::carry::carry_copper(&from.before, board, &from.rip);
                say(format!(
                    "the router's copper of {} net(s) from {}{}",
                    carried.kept.len(),
                    from.file.display(),
                    if carried.ripped.is_empty() {
                        String::new()
                    } else {
                        format!(
                            "; not drawn, as their pads or the routes file changed: {}",
                            carried.ripped.join(", ")
                        )
                    }
                ));
            }
            None => say(format!(
                "no router's copper kept for this board ({}): the routes file's copper alone; `--route` routes the rest",
                kept.file(&board.name).display()
            )),
        }
        // The teardrops the router draws on the finished board, which no file keeps: the
        // Gerbers `fab` writes have them (issue #1142).
        netsmith_router::teardrop::add(board);
        return Ok(());
    };
    let options = RouteOptions {
        rip_rounds: route.rip_rounds,
        ..RouteOptions::default()
    };
    let routing = match &from {
        Some(from) => {
            crate::fab::reroute_with_margin(
                board,
                &from.before,
                &from.rip,
                route.timeout,
                options,
                route.track_margin,
                say,
            )
            .routing
        }
        None => {
            crate::fab::route_with_margin(board, route.timeout, options, route.track_margin, say)
        }
    };
    say(match (routing.left.len(), routing.stopped) {
        (0, _) => "routed every connection".to_string(),
        (left, true) => format!(
            "the router stopped at the route timeout with {left} connection(s) open; raise it with --route-timeout"
        ),
        (left, false) => format!("the router left {left} connection(s) open"),
    });
    Ok(())
}

/// The rectangle of the board's frame that `region` (from the outline's top left corner) is.
pub fn region_on(board: &Board, region: [f64; 4]) -> Rect {
    let [x, y, w, h] = region;
    let at = board.outline.min + Pt::new(x, y);
    Rect::new(at, at + Pt::new(w, h))
}

/// What of `request` can be told wrong from the board as placed, before any copper is put on it
/// (so before a route of minutes): a layer, an id of `--highlight` the board does not have, a
/// region of no size, a picture too big. Gives the parts and nets lit and the region, in the
/// board's frame.
pub fn check(board: &Board, request: &Request) -> Result<(Option<Lit>, Rect), String> {
    if let PictureLayers::One(layer) = request.layers {
        let words = netsmith_pcb::copper_layers(board);
        let word = netsmith_pcb::routes_layer_word(layer.name()).unwrap_or_default();
        if !words.contains(&word) {
            return Err(format!(
                "the board has no copper layer {word}: its layers are {}, or `all`",
                words.join(", ")
            ));
        }
    }
    let lit = if request.highlight.is_empty() {
        None
    } else {
        Some(Lit::of(board, &request.highlight)?)
    };
    let region = match request.region {
        Some(r) => {
            if !(r[2] > 0.0 && r[3] > 0.0) {
                return Err(format!(
                    "--region {},{},{},{}: the width and the height must be more than 0",
                    r[0], r[1], r[2], r[3]
                ));
            }
            region_on(board, r)
        }
        None => netsmith_pcb::picture_whole(board),
    };
    let (width, height) = pixels(region, request.scale);
    let most = netsmith_draw::raster::MAX_SIDE;
    if width > most || height > most {
        let fits = most as f64 / region.width().max(region.height());
        return Err(format!(
            "the picture would be {width} by {height} pixels, more than {most} a side: give a --scale of at most {:.2} or a smaller --region",
            (fits * 100.0).floor() / 100.0
        ));
    }
    Ok((lit, region))
}

/// The size in pixels of `region` at `scale`, as the SVG says it and the PNG is.
fn pixels(region: Rect, scale: f64) -> (usize, usize) {
    let side = |len: f64| (len * scale).ceil().max(1.0) as usize;
    (side(region.width()), side(region.height()))
}

/// Draw `board`, with its copper on, as `request` says. `constraints` are the design's, for the
/// check of `--drc`. The error is a mistake in the request ([`check`]) or why the board cannot
/// be drawn.
pub fn draw(
    board: &Board,
    request: &Request,
    constraints: Option<Constraints>,
) -> Result<Drawn, String> {
    let (mut lit, region) = check(board, request)?;
    // The stitching the export will have (issue #1489), on the board drawn: the fill clears
    // round the vias as the Gerbers' does, and the check sees them.
    let stitching = if request.stitching {
        netsmith_router::stitching_vias(board)
    } else {
        Vec::new()
    };
    let with_stitching;
    let board = if stitching.iter().any(|v| !board.vias.contains(v)) {
        let mut b = board.clone();
        let laid: Vec<_> = stitching
            .iter()
            .filter(|v| !board.vias.contains(v))
            .cloned()
            .collect();
        b.vias.extend(laid);
        with_stitching = b;
        &with_stitching
    } else {
        board
    };
    let (width, height) = pixels(region, request.scale);
    let open: Vec<Ratline> = Ratsnest::of(board).lines;
    if request.only_open {
        lit = Some(lit.unwrap_or_default().with_nets_of(&open));
    }
    let (markers, listed) = if request.drc {
        markers_of(board, constraints, request.layers)?
    } else {
        (Vec::new(), Vec::new())
    };
    let (islands, island_lines) = if request.islands {
        removed_islands(board, request.layers)?
    } else {
        (Vec::new(), Vec::new())
    };
    let options = PictureOptions {
        layers: request.layers,
        from_below: request
            .from_below
            .unwrap_or(request.layers == PictureLayers::One(netsmith_pcb::CopperLayer::Back)),
        region: Some(region),
        scale: request.scale,
        courtyards: request.courtyards,
        designators: request.designators,
        lit,
        stitching: stitching.iter().map(|v| v.at).collect(),
    };
    let svg = picture_svg(board, &open, &islands, &markers, &options)?;
    let bytes = match request.format {
        Format::Svg => svg.into_bytes(),
        Format::Png => {
            let picture = netsmith_draw::svg::read(&svg).map_err(|e| e.to_string())?;
            let [r, g, b] = netsmith_pcb::PICTURE_BACKGROUND;
            let image = netsmith_draw::raster::render(
                &picture,
                picture.view,
                request.scale,
                netsmith_draw::Rgba::opaque(r, g, b),
            );
            netsmith_draw::raster::png(&image)
        }
    };
    Ok(Drawn {
        bytes,
        width,
        height,
        open: open
            .iter()
            .filter(|l| !l.to_plane)
            .map(ToString::to_string)
            .collect(),
        markers: listed,
        islands: island_lines,
        stitching: stitching.len(),
    })
}

/// The copper islands the fill of the board's pours took out, on a copper layer drawn, and their
/// lines, numbered.
fn removed_islands(
    board: &Board,
    layers: PictureLayers,
) -> Result<(Vec<PictureIsland>, Vec<String>), String> {
    let report = FillReport::of(board).map_err(|e| e.to_string())?;
    let mut islands = Vec::new();
    let mut lines = Vec::new();
    for fill in &report.fills {
        if let PictureLayers::One(l) = layers
            && l.name() != fill.layer_name
        {
            continue;
        }
        for island in &fill.removed_islands {
            // `I1`, `I2`: not the numbers of the markers drawn beside them.
            let label = format!("I{}", islands.len() + 1);
            let at = island.at - report.outline_at;
            lines.push(format!(
                "{label}: {} {} at ({:.3}, {:.3}), {:.3} mm2",
                fill.layer, fill.net, at.x, at.y, island.area
            ));
            islands.push(PictureIsland {
                layer: fill.layer_name.clone(),
                rings: island.outline.clone(),
                label,
            });
        }
    }
    Ok((islands, lines))
}

/// The places of the check's findings, but a missing connection and one on a copper layer that
/// is not drawn, and their lines, numbered.
fn markers_of(
    board: &Board,
    constraints: Option<Constraints>,
    layers: PictureLayers,
) -> Result<(Vec<Marker>, Vec<String>), String> {
    let checked = crate::drc::of_board(board, constraints, false)?;
    let on = |layer: Option<&str>| match (layers, layer) {
        (PictureLayers::One(l), Some(at)) if at.ends_with(".Cu") => l.name() == at,
        _ => true,
    };
    let mut markers = Vec::new();
    let mut lines = Vec::new();
    for f in
        checked.report.findings.iter().filter(|f| {
            f.rule != netsmith_geom::DrcRule::UnconnectedItems && on(f.layer.as_deref())
        })
    {
        let label = (markers.len() + 1).to_string();
        lines.push(format!("{label}: {f}"));
        markers.push(Marker {
            at: Pt::new(f.at.x, f.at.y),
            layer: f.layer.clone(),
            error: f.severity == netsmith_geom::DrcSeverity::Error,
            label,
        });
    }
    Ok((markers, lines))
}
