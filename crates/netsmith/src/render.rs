//! `netsmith render`: the board's _Scene_ (ADR 0105) drawn to a PNG by netsmith's own
//! rasterizer ([`netsmith_3d::raster3d`]), for an agent that can read a picture, a golden test
//! and a person with no 3D viewer. The board is placed as `board3d`, `fab` and the editor place
//! it ([`crate::board3d::scene_and_label`]); nothing is routed, asked of KiCad or written but the
//! PNG. The same source and options give the same bytes. `docs/BOARD3D.md`, "Pictures for
//! agents", says what is drawn and in which frame.

use std::path::PathBuf;

use netsmith_3d::mesh::{self, Mesh};
use netsmith_3d::raster3d::{self, Axis, Lens, RenderOptions, Section, Theme, View};
use netsmith_3d::scene3d::Scene;
use netsmith_3d::system::SystemScene;

/// The picture's size unless `--size` says one.
pub const DEFAULT_SIZE: (usize, usize) = (1600, 1000);
/// The least and the most pixels along a side.
pub const SIZE_RANGE: (usize, usize) = (64, 8192);
/// The most pixels in a picture: the drawing keeps four samples of every pixel.
pub const MAX_PIXELS: usize = 16_000_000;
/// The view unless `--view` says one.
pub const DEFAULT_VIEW: View = View::Iso;
/// The vertical field of view of `--perspective`, in degrees.
pub const FOV_DEG: f64 = 30.0;

/// What `netsmith render` was asked to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub view: View,
    pub size: (usize, usize),
    pub section: Option<(Axis, f64)>,
    /// As given: part paths, designators, `net:<name>` and `limit:<name>`.
    pub highlight: Vec<String>,
    pub xray: bool,
    /// Fill the cut faces of a section ([`RenderOptions::caps`]); `--no-caps` leaves them open.
    pub caps: bool,
    pub dark: bool,
    pub perspective: bool,
}

impl Default for Request {
    fn default() -> Self {
        Request {
            view: DEFAULT_VIEW,
            size: DEFAULT_SIZE,
            section: None,
            highlight: Vec::new(),
            xray: false,
            caps: true,
            dark: false,
            perspective: false,
        }
    }
}

/// `1600x1000` as a size, within [`SIZE_RANGE`] and [`MAX_PIXELS`].
pub fn parse_size(text: &str) -> Result<(usize, usize), String> {
    let bad = || {
        format!(
            "--size needs <width>x<height> in pixels, such as 1600x1000 ({} to {} each, at most {} pixels), not `{text}`",
            SIZE_RANGE.0, SIZE_RANGE.1, MAX_PIXELS
        )
    };
    let (w, h) = text.split_once(['x', 'X']).ok_or_else(bad)?;
    let (w, h): (usize, usize) = (
        w.trim().parse().map_err(|_| bad())?,
        h.trim().parse().map_err(|_| bad())?,
    );
    if !(SIZE_RANGE.0..=SIZE_RANGE.1).contains(&w)
        || !(SIZE_RANGE.0..=SIZE_RANGE.1).contains(&h)
        || w * h > MAX_PIXELS
    {
        return Err(bad());
    }
    Ok((w, h))
}

/// `x=12.5` as an axis and a position in mm of the scene's frame.
pub fn parse_section(text: &str) -> Result<(Axis, f64), String> {
    let bad = || {
        format!(
            "--section needs one axis and a position in mm of the scene's frame, such as x=12.5, y=8 or z=-0.8, not `{text}`"
        )
    };
    let (axis, at) = text.split_once('=').ok_or_else(bad)?;
    let axis = match axis.trim() {
        "x" | "X" => Axis::X,
        "y" | "Y" => Axis::Y,
        "z" | "Z" => Axis::Z,
        _ => return Err(bad()),
    };
    let at: f64 = at.trim().parse().map_err(|_| bad())?;
    if !at.is_finite() {
        return Err(bad());
    }
    Ok((axis, at))
}

/// The file `render` writes unless `--out` says one: `<board>-<view>.png`, a board of a system
/// (`System/member`) as `System-member`.
pub fn default_out(label: &str, view: View) -> PathBuf {
    let board: String = label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    PathBuf::from(format!("{board}-{}.png", view.name()))
}

/// The ids of the meshes `requested` names, in the order given, each once. A part by path or
/// designator, `net:<name>` and `limit:<name>`; `board` is the board itself. The error names
/// every id that is none of them.
pub fn resolve_highlight(scene: &Scene, requested: &[String]) -> Result<Vec<String>, String> {
    let mut nets: Vec<&str> = Vec::new();
    if let Ok(copper) = &scene.copper {
        nets.extend(copper.layers.iter().flat_map(|l| l.nets()).flatten());
    }
    nets.extend(scene.board.holes.iter().filter_map(|h| match &h.owner {
        netsmith_3d::scene3d::HoleOwner::Via { net } => Some(net.as_str()),
        _ => None,
    }));
    nets.sort();
    nets.dedup();
    let mut ids: Vec<String> = Vec::new();
    let mut unknown: Vec<&str> = Vec::new();
    for want in requested {
        let found = if let Some(net) = want.strip_prefix("net:") {
            nets.contains(&net).then(|| net.to_string())
        } else if want.starts_with("limit:") {
            scene
                .limits
                .iter()
                .any(|l| format!("limit:{}", l.name) == *want)
                .then(|| want.clone())
        } else if let Some(b) = scene.body(want) {
            Some(b.path.clone())
        } else if let Some(b) = scene.bodies.iter().find(|b| b.refdes == *want) {
            Some(b.path.clone())
        } else {
            (want == "board").then(|| want.clone())
        };
        match found {
            Some(id) if !ids.contains(&id) => ids.push(id),
            Some(_) => {}
            None => unknown.push(want),
        }
    }
    if unknown.is_empty() {
        return Ok(ids);
    }
    let list = |v: Vec<String>| {
        if v.is_empty() {
            "(none)".to_string()
        } else {
            v.join(", ")
        }
    };
    Err(format!(
        "error: --highlight names {} that the board does not have: {}\nthe parts are: {}\nthe nets: {}\nthe limits: {}",
        if unknown.len() == 1 { "an id" } else { "ids" },
        unknown
            .iter()
            .map(|u| format!("`{u}`"))
            .collect::<Vec<_>>()
            .join(", "),
        list(
            scene
                .bodies
                .iter()
                .map(|b| format!("{} ({})", b.refdes, b.path))
                .collect()
        ),
        list(nets.iter().map(|n| format!("net:{n}")).collect()),
        list(
            scene
                .limits
                .iter()
                .map(|l| format!("limit:{}", l.name))
                .collect()
        ),
    ))
}

/// The side of the cut to keep: the one away from the camera, so that the cut face is the one
/// seen. A view along the cut keeps the lower side.
fn keep_below(view: View, axis: Axis) -> bool {
    let (towards_eye, _) = view.direction();
    let along = match axis {
        Axis::X => towards_eye[0],
        Axis::Y => towards_eye[1],
        Axis::Z => towards_eye[2],
    };
    along >= 0.0
}

/// The render options of `request`, with the highlight already resolved to mesh ids.
fn options(request: &Request, highlight: Vec<String>) -> RenderOptions {
    RenderOptions {
        width: request.size.0,
        height: request.size.1,
        palette: if request.dark {
            Theme::Dark
        } else {
            Theme::Light
        }
        .palette(),
        highlight,
        xray: request.xray,
        caps: request.caps,
        section: request.section.map(|(axis, at)| Section {
            axis,
            at,
            keep_below: keep_below(request.view, axis),
        }),
        ..RenderOptions::default()
    }
}

/// The meshes of `scene` drawn as `request` says, as the bytes of a PNG.
pub fn draw(scene: &Scene, request: &Request) -> Result<Vec<u8>, String> {
    let highlight = resolve_highlight(scene, &request.highlight)?;
    // A capped section draws the board as its stackup, a layer at a time.
    let meshes: Vec<Mesh> = match request.section {
        Some((axis, at)) if request.caps => mesh::of_scene_section(scene, axis, at),
        _ => mesh::of_scene(scene),
    };
    let options = options(request, highlight);
    let lens = if request.perspective {
        Lens::Perspective { fov_y_deg: FOV_DEG }
    } else {
        Lens::Orthographic { half_height: 1.0 }
    };
    let camera = raster3d::fit_view(&meshes, request.view, &options, lens);
    Ok(raster3d::render(&meshes, &camera, &options).to_png())
}

/// The ids of the meshes `requested` names in the system scene (ADR 0110): each is
/// `member/<id>`, the id as [`resolve_highlight`] reads it for that member's board (`led/J1`,
/// `led/net:GND`), or a member's name alone for the whole member. A net or a limit may also be
/// named with the member after its prefix, as the editor's 3D view names it (`net:led/GND`,
/// `limit:led/lid`), which is the same id.
pub fn resolve_system_highlight(
    system: &SystemScene,
    requested: &[String],
) -> Result<Vec<String>, String> {
    let mut ids: Vec<String> = Vec::new();
    let mut errors = Vec::new();
    for want in requested {
        let (member, id) = system_id(want);
        let (member, id) = (member.as_str(), id.as_str());
        let Some(m) = system.member(member) else {
            errors.push(format!(
                "error: --highlight names `{want}`, and the system `{}` has no member `{member}`; its members are: {} (name a part as `member/<designator>`)",
                system.name,
                system
                    .members
                    .iter()
                    .map(|m| m.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            continue;
        };
        let found = if id.is_empty() {
            // The whole member: every id of its meshes.
            mesh::of_scene(&m.scene)
                .into_iter()
                .map(|mesh| mesh.id)
                .collect()
        } else {
            match resolve_highlight(&m.scene, &[id.to_string()]) {
                Ok(found) => found,
                Err(e) => {
                    errors.push(format!("{e}\n(of the member `{member}`)"));
                    continue;
                }
            }
        };
        for id in found {
            let id = format!("{member}/{id}");
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    if errors.is_empty() {
        Ok(ids)
    } else {
        Err(errors.join("\n"))
    }
}

/// An id of the system scene as its member and the id in the member's board: `led/J1` and
/// `led/net:GND` as they are written, `net:led/GND` and `limit:led/lid` with the member taken out
/// of the name; a member alone has no id (`""`).
fn system_id(want: &str) -> (String, String) {
    for prefix in ["net:", "limit:"] {
        if let Some((member, name)) = want.strip_prefix(prefix).and_then(|r| r.split_once('/')) {
            return (member.to_string(), format!("{prefix}{name}"));
        }
    }
    let (member, id) = want.split_once('/').unwrap_or((want, ""));
    (member.to_string(), id.to_string())
}

/// The system scene drawn as `request` says, as the bytes of a PNG: every member's meshes moved
/// into the system's frame ([`netsmith_3d::system::meshes`]), framed as a whole. A section cuts in
/// the system's frame; its cut faces are filled, each board as one block (not its stackup).
pub fn draw_system(system: &SystemScene, request: &Request) -> Result<Vec<u8>, String> {
    let highlight = resolve_system_highlight(system, &request.highlight)?;
    let meshes = netsmith_3d::system::meshes(system);
    let options = options(request, highlight);
    let lens = if request.perspective {
        Lens::Perspective { fov_y_deg: FOV_DEG }
    } else {
        Lens::Orthographic { half_height: 1.0 }
    };
    let camera = raster3d::fit_view(&meshes, request.view, &options, lens);
    Ok(raster3d::render(&meshes, &camera, &options).to_png())
}
