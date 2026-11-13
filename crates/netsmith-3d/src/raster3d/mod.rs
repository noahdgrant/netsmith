//! netsmith's own software rasterizer: a PNG picture of [`Mesh`]es in 3D, with no GPU, no
//! browser and no KiCad (ADR 0105, "Headless pictures from netsmith's own rasterizer").
//!
//! An agent that can look, a golden test and a person reading a report get the same pixels
//! from the same scene on every machine. [`render`] draws meshes seen by a [`Camera`] with
//! [`RenderOptions`] into an [`Image`], and [`Image::to_png`] writes it
//! ([`netsmith_core::png::rgba`]). [`Camera::fit`] frames a named [`View`] (top, bottom, front,
//! back, left, right, iso, iso-bottom) round the meshes' box; a camera can also be given by its
//! eye, target, up and lens.
//!
//! # How it draws
//!
//! - **Frame.** The meshes are in the scene's frame ([`crate::mesh`]: mm, right-handed, z up from
//!   the board's top face). The top view has x to the right and y up the picture; the bottom
//!   view looks up from below, so x runs to the left, as on the board turned over left to right.
//! - **Pipeline.** Each triangle is lit at its corners, clipped by the section plane (if any)
//!   and, for a perspective lens, by a near plane, projected, and filled by edge functions in
//!   fixed point (1/256 of a sample) with a top-left rule, so two triangles that share an edge
//!   cover each sample once. A z-buffer keeps the nearest. A closed mesh's back faces are not
//!   drawn (unless a section opens it); an open one is drawn from both sides, lit as the side
//!   that faces the camera.
//! - **Light.** A fixed key light (above left of the camera), a weaker fill (right, below) and
//!   ambient light, all fixed to the camera, diffuse only. [`Shading::Smooth`] interpolates the
//!   light of the corners (Gouraud) from the meshes' normals; [`Shading::Flat`] lights each
//!   triangle by its own normal.
//! - **Caps.** With a section, the face where the plane cuts a closed mesh is filled
//!   (`cap.rs`): the loops of the plane's cut of the mesh, triangulated and drawn flat in the
//!   plane in the mesh's colour, lit as a face turned to the camera; a mesh whose cut does not
//!   close up is left open and drawn from inside as before. Anything lying in the plane (the
//!   caps, a flat piece the scene puts there) is drawn after the rest, in mesh order, each over
//!   the last where they are at one depth; no line is drawn between two caps, so the bands of a
//!   stackup read by their colours. [`crate::mesh::of_scene_section`] gives the meshes of a
//!   board as its stackup for this.
//! - **See-through.** The limits' materials ([`Material::is_translucent`]) are drawn after
//!   everything opaque, their triangles sorted far to near, blended over what is behind,
//!   without writing depth. X-ray ([`RenderOptions::xray`]) draws every mesh that way.
//! - **Outlines.** For a CAD look, a line is drawn where the nearest surface changes from one
//!   mesh to another or to the background, where its depth jumps, and at a crease (faces at
//!   more than [`CREASE_DEG`]).
//! - **Highlight.** The meshes whose `id` is in [`RenderOptions::highlight`] are drawn in the
//!   palette's highlight colour and the rest are dimmed toward the background.
//! - **Anti-aliasing.** Each pixel is the mean of `supersample` x `supersample` samples.
//!
//! # Determinism
//!
//! The same meshes, camera and options give the same bytes on every machine: all geometry is
//! `f64` arithmetic in a fixed order (Rust never fuses a multiply and an add on its own, and
//! `sqrt` is correctly rounded everywhere); no function of the platform's maths library is
//! called (the field of view's tangent and the circles' points come from [`crate::mesh::trig`]);
//! coverage is decided in integers; see-through triangles are sorted by a total order (depth,
//! then the order they were given in); there are no threads and no hash-map iteration. The
//! golden test in this module holds the bytes of a small scene's PNG.
//!
//! # Palette
//!
//! [`Theme::Light`] and [`Theme::Dark`] each give a [`Palette`]: a background, a colour per
//! [`Material`] (green mask, a lighter green where copper is under it, tan laminate, gold copper
//! and a brighter gold finish where the mask is open, dark grey bodies, an orange flat outline
//! for a part of unknown height, light grey metal, white marks and silkscreen, a blue see-through
//! limit, a red see-through broken limit), the highlight colour and the outline colour.

mod cap;
mod draw;
#[cfg(test)]
mod tests;

use crate::mesh::{Aabb, Material, Mesh, bounds_of, trig};

pub(crate) use crate::mesh::Axis;

/// Two faces of one mesh that meet at more than this many degrees get an outline between them.
pub(crate) const CREASE_DEG: f64 = 40.0;

/// The opacity of every mesh in x-ray.
pub(crate) const XRAY_ALPHA: f64 = 0.3;

/// How a camera projects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lens {
    /// Parallel projection: `half_height` mm from the middle of the picture to its top edge.
    Orthographic { half_height: f64 },
    /// A pinhole with a vertical field of view of `fov_y_deg` degrees.
    Perspective { fov_y_deg: f64 },
}

/// Where a picture is seen from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub eye: [f64; 3],
    pub target: [f64; 3],
    /// Which way is up the picture; need not be at right angles to the line of sight.
    pub up: [f64; 3],
    pub lens: Lens,
}

/// A named direction to look from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
    /// From the front, right and top at once (from +x, -y, +z).
    Iso,
    /// From the front, right and below (from +x, -y, -z).
    IsoBottom,
}

impl View {
    pub const ALL: [View; 8] = [
        View::Top,
        View::Bottom,
        View::Front,
        View::Back,
        View::Left,
        View::Right,
        View::Iso,
        View::IsoBottom,
    ];

    /// The name a command line gives: `top`, `iso-bottom`.
    pub fn name(self) -> &'static str {
        match self {
            View::Top => "top",
            View::Bottom => "bottom",
            View::Front => "front",
            View::Back => "back",
            View::Left => "left",
            View::Right => "right",
            View::Iso => "iso",
            View::IsoBottom => "iso-bottom",
        }
    }

    pub fn from_name(name: &str) -> Option<View> {
        View::ALL.into_iter().find(|v| v.name() == name)
    }

    /// The unit direction from the target to the eye, and the up of the picture.
    pub fn direction(self) -> ([f64; 3], [f64; 3]) {
        let z = [0.0, 0.0, 1.0];
        let y = [0.0, 1.0, 0.0];
        let k = 1.0 / 3f64.sqrt();
        match self {
            View::Top => ([0.0, 0.0, 1.0], y),
            View::Bottom => ([0.0, 0.0, -1.0], y),
            View::Front => ([0.0, -1.0, 0.0], z),
            View::Back => ([0.0, 1.0, 0.0], z),
            View::Left => ([-1.0, 0.0, 0.0], z),
            View::Right => ([1.0, 0.0, 0.0], z),
            View::Iso => ([k, -k, k], z),
            View::IsoBottom => ([k, -k, -k], z),
        }
    }
}

/// The camera's own axes: right, up and forward (from the eye to the target), unit length.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Basis {
    pub r: [f64; 3],
    pub u: [f64; 3],
    pub f: [f64; 3],
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn normalise(v: [f64; 3]) -> [f64; 3] {
    let l = dot(v, v).sqrt();
    if l == 0.0 { v } else { v.map(|c| c / l) }
}

impl Camera {
    pub(crate) fn basis(&self) -> Basis {
        let f = normalise(sub(self.target, self.eye));
        let mut r = normalise(cross(f, self.up));
        if dot(r, r) == 0.0 {
            // Up along the line of sight: any right angle will do, the same one every time.
            r = normalise(cross(f, [0.0, 1.0, 0.0]));
            if dot(r, r) == 0.0 {
                r = normalise(cross(f, [1.0, 0.0, 0.0]));
            }
        }
        let u = cross(r, f);
        Basis { r, u, f }
    }

    /// A camera looking from `view` at the box `bounds`, framed so that the box fills the
    /// picture of width / height `aspect` with `margin` (a fraction of the picture, such as 0.05)
    /// free on the tighter side. `lens` decides the projection; for an orthographic lens its
    /// `half_height` is computed and the one given is not read.
    pub fn fit(view: View, bounds: Aabb, aspect: f64, margin: f64, lens: Lens) -> Camera {
        let (back, up) = view.direction();
        let centre = bounds.centre();
        let size = bounds.size();
        let diagonal = dot(size, size).sqrt().max(1e-6);
        let aspect = if aspect > 0.0 { aspect } else { 1.0 };
        let scale = 1.0 + 2.0 * margin.max(0.0);
        let probe = Camera {
            eye: [
                centre[0] + back[0],
                centre[1] + back[1],
                centre[2] + back[2],
            ],
            target: centre,
            up,
            lens,
        };
        let b = probe.basis();
        match lens {
            Lens::Orthographic { .. } => {
                // Centre the box's projection, then make it fit both ways.
                let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
                for c in bounds.corners() {
                    let d = sub(c, centre);
                    let (x, y) = (dot(d, b.r), dot(d, b.u));
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
                let (mx, my) = (0.5 * (x0 + x1), 0.5 * (y0 + y1));
                let target = [0, 1, 2].map(|k| centre[k] + mx * b.r[k] + my * b.u[k]);
                let half = (0.5 * (y1 - y0)).max(0.5 * (x1 - x0) / aspect).max(1e-6) * scale;
                Camera {
                    eye: [0, 1, 2].map(|k| target[k] + 2.0 * diagonal * back[k]),
                    target,
                    up,
                    lens: Lens::Orthographic { half_height: half },
                }
            }
            Lens::Perspective { fov_y_deg } => {
                let ty = trig::tan(trig::radians(0.5 * fov_y_deg)) / scale;
                let tx = ty * aspect;
                // The nearest eye, at a sideways offset (ox, oy) from the box's centre, from
                // which every corner is inside the picture; then the offset that centres the
                // corners' extent, a few times over (each step changes the distance a little).
                let corners = bounds.corners().map(|c| {
                    let d = sub(c, centre);
                    (dot(d, b.r), dot(d, b.u), dot(d, back))
                });
                let fit = |ox: f64, oy: f64| {
                    let mut distance: f64 = 1e-3;
                    for &(x, y, toward) in &corners {
                        let need = toward + ((x - ox).abs() / tx).max((y - oy).abs() / ty);
                        distance = distance.max(need);
                    }
                    distance
                };
                let (mut ox, mut oy) = (0.0, 0.0);
                for _ in 0..6 {
                    let distance = fit(ox, oy);
                    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
                    for &(x, y, toward) in &corners {
                        let depth = distance - toward;
                        x0 = x0.min((x - ox) / depth);
                        x1 = x1.max((x - ox) / depth);
                        y0 = y0.min((y - oy) / depth);
                        y1 = y1.max((y - oy) / depth);
                    }
                    ox += 0.5 * (x0 + x1) * distance;
                    oy += 0.5 * (y0 + y1) * distance;
                }
                let distance = fit(ox, oy);
                let target = [0, 1, 2].map(|k| centre[k] + ox * b.r[k] + oy * b.u[k]);
                Camera {
                    eye: [0, 1, 2].map(|k| target[k] + distance * back[k]),
                    target,
                    up,
                    lens,
                }
            }
        }
    }
}

/// An sRGB colour with alpha, 0 to 255.
pub type Rgba = [u8; 4];

/// The colours of a picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub background: Rgba,
    /// One per material, in the order of [`Material::ALL`]; the alpha of a see-through one is
    /// its opacity.
    pub materials: [Rgba; 14],
    pub highlight: Rgba,
    pub outline: Rgba,
}

impl Palette {
    pub fn colour(&self, m: Material) -> Rgba {
        let i = Material::ALL.iter().position(|&x| x == m).unwrap_or(0);
        self.materials[i]
    }
}

/// A light or a dark picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

impl Theme {
    pub fn palette(self) -> Palette {
        match self {
            Theme::Light => Palette {
                background: [244, 245, 247, 255],
                materials: [
                    [196, 176, 128, 255], // substrate
                    [38, 120, 72, 255],   // mask-top
                    [38, 110, 68, 255],   // mask-bottom
                    [212, 170, 84, 255],  // copper (gold finish)
                    [58, 60, 66, 255],    // body
                    [236, 140, 40, 255],  // body-unknown
                    [190, 192, 198, 255], // metal
                    [250, 250, 250, 255], // mark
                    [60, 130, 240, 64],   // limit
                    [230, 60, 50, 96],    // limit-violated
                    [222, 184, 96, 255],  // finish (ENIG gold, where the mask is open)
                    [74, 158, 98, 255],   // mask-over-copper
                    [226, 208, 150, 255], // prepreg
                    [120, 132, 150, 52],  // case (see-through)
                ],
                highlight: [255, 190, 0, 255],
                outline: [24, 26, 30, 255],
            },
            Theme::Dark => Palette {
                background: [28, 30, 34, 255],
                materials: [
                    [170, 152, 108, 255],
                    [30, 104, 62, 255],
                    [30, 96, 58, 255],
                    [200, 160, 78, 255],
                    [92, 95, 104, 255],
                    [236, 140, 40, 255],
                    [170, 172, 178, 255],
                    [232, 232, 232, 255],
                    [90, 150, 255, 64],
                    [255, 80, 70, 96],
                    [206, 170, 88, 255],
                    [58, 138, 84, 255],
                    [196, 178, 128, 255],
                    [176, 188, 206, 44],
                ],
                highlight: [255, 200, 40, 255],
                outline: [8, 9, 11, 255],
            },
        }
    }
}

/// How a triangle's light is worked out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shading {
    /// One light per triangle, from its own normal.
    Flat,
    /// The light of each corner from the mesh's normals, blended across the triangle; a mesh
    /// with no normals is shaded flat.
    #[default]
    Smooth,
}

/// A cut through the scene at right angles to an axis: what is on one side is not drawn. The cut
/// faces of closed meshes are filled with caps ([`RenderOptions::caps`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section {
    pub axis: Axis,
    /// Where the plane is, in mm.
    pub at: f64,
    /// Keep what is below `at` (true) or above it.
    pub keep_below: bool,
}

/// What to draw and how.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    /// The picture's size in pixels.
    pub width: usize,
    pub height: usize,
    /// Samples a pixel along each side (1 to 4); 2 or 3 gives clean edges.
    pub supersample: usize,
    pub palette: Palette,
    pub shading: Shading,
    /// Lines at the edges of meshes and at creases.
    pub outlines: bool,
    /// The ids of the meshes to light up; when empty nothing is dimmed.
    pub highlight: Vec<String>,
    /// Every mesh see-through.
    pub xray: bool,
    /// Draw a case model's solids ([`Material::Case`]) solid, so they hide the board inside:
    /// opaque, in the case's colour. A case solid that is lit by the highlight stays see-through
    /// (in the highlight colour), so what is inside shows.
    pub case_solid: bool,
    pub section: Option<Section>,
    /// With a section: fill the cut faces of closed meshes, in the colour of the mesh's
    /// material, so that the cut reads as a solid section; false leaves them open.
    pub caps: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            width: 800,
            height: 600,
            supersample: 2,
            palette: Theme::Light.palette(),
            shading: Shading::Smooth,
            outlines: true,
            highlight: Vec::new(),
            xray: false,
            case_solid: false,
            section: None,
            caps: true,
        }
    }
}

/// A picture: `width` x `height` pixels, four bytes each (red, green, blue, alpha), row by row
/// from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl Image {
    /// The pixel at column `x`, row `y` from the top.
    pub fn pixel(&self, x: usize, y: usize) -> Rgba {
        let i = 4 * (y * self.width + x);
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    pub fn to_png(&self) -> Vec<u8> {
        netsmith_core::png::rgba(self.width, self.height, &self.rgba)
    }
}

/// Draws `meshes` seen by `camera`.
pub fn render(meshes: &[Mesh], camera: &Camera, options: &RenderOptions) -> Image {
    draw::render(meshes, camera, options)
}

/// Draws `meshes` from `view` with an orthographic lens, framed to fit with a 5 % margin.
pub fn render_view(meshes: &[Mesh], view: View, options: &RenderOptions) -> Image {
    let camera = fit_view(
        meshes,
        view,
        options,
        Lens::Orthographic { half_height: 1.0 },
    );
    render(meshes, &camera, options)
}

/// The camera [`render_view`] uses, with any lens.
pub(crate) fn fit_view(meshes: &[Mesh], view: View, options: &RenderOptions, lens: Lens) -> Camera {
    let bounds = bounds_of(meshes).unwrap_or(Aabb {
        min: [-1.0; 3],
        max: [1.0; 3],
    });
    let aspect = options.width.max(1) as f64 / options.height.max(1) as f64;
    Camera::fit(view, bounds, aspect, 0.05, lens)
}
