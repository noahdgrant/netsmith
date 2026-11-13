//! The drawing itself: lighting, clipping, projection, coverage, the z-buffer, blending,
//! outlines and the downsampling (see the module's header for the rules).

use std::collections::BTreeSet;

use super::cap::cap;
use super::{
    Basis, CREASE_DEG, Camera, Image, Lens, RenderOptions, Rgba, Shading, XRAY_ALPHA, cross, dot,
    normalise, sub,
};
use crate::mesh::{Mesh, bounds_of, trig};

/// Fixed-point steps per sample in the coverage test.
const SUB: i64 = 256;

/// A corner this close (mm) to the section plane is in it.
const PLANE_EPSILON: f64 = 1e-9;

/// How much deeper than what is drawn already (as a share of its depth key) a triangle in the
/// section plane may be and still be drawn over it.
const LEVEL_TOLERANCE: f32 = 2e-6;

const AMBIENT: f64 = 0.35;
const KEY: f64 = 0.55;
const FILL: f64 = 0.22;

/// How far a highlighted picture dims what is not lit up: the share of the background mixed in.
const DIM: f64 = 0.6;

/// How much of the outline colour covers a sample on an outline.
const OUTLINE_COVER: f64 = 0.85;

/// A corner on its way through the pipeline: the scene's position (or the camera's, or the
/// picture's, as the stage says) and the lit colour, 0 to 1.
#[derive(Debug, Clone, Copy)]
struct V {
    p: [f64; 3],
    c: [f64; 3],
}

fn lerp_v(a: &V, b: &V, t: f64) -> V {
    V {
        p: [0, 1, 2].map(|k| a.p[k] + (b.p[k] - a.p[k]) * t),
        c: [0, 1, 2].map(|k| a.c[k] + (b.c[k] - a.c[k]) * t),
    }
}

/// Keeps the part of the polygon where `d(v) >= 0` (Sutherland-Hodgman), into `out`.
fn clip(poly: &[V], out: &mut Vec<V>, d: impl Fn(&V) -> f64) {
    out.clear();
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (&poly[i], &poly[(i + 1) % n]);
        let (da, db) = (d(a), d(b));
        if da >= 0.0 {
            out.push(*a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            out.push(lerp_v(a, b, da / (da - db)));
        }
    }
}

/// A triangle in the picture: x and y in samples, z the depth key (smaller is nearer, and
/// linear across the picture), with its colour and what it is.
#[derive(Debug, Clone, Copy)]
struct Tri {
    v: [V; 3],
    alpha: f64,
    mesh: u32,
    /// The face's normal, turned to face the camera, for the creases.
    normal: [i8; 3],
}

struct Buffers {
    w: usize,
    h: usize,
    colour: Vec<[u8; 3]>,
    depth: Vec<f32>,
    /// The mesh nearest at each sample, plus one; 0 is the background.
    id: Vec<u32>,
    normal: Vec<[i8; 3]>,
    /// Whether the nearest surface is a cap (or lies in the section plane).
    cap: Vec<bool>,
}

fn quantise(c: f64) -> u8 {
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

fn floor_div(a: i64, b: i64) -> i64 {
    a.div_euclid(b)
}

fn ceil_div(a: i64, b: i64) -> i64 {
    -(-a).div_euclid(b)
}

/// Calls `f(sample index, weights of the three corners)` for every sample whose centre the
/// triangle covers, by a top-left rule, so that triangles sharing an edge cover each sample once.
fn cover(v: &[V; 3], w: usize, h: usize, mut f: impl FnMut(usize, [f64; 3])) {
    let fx = |a: f64| (a * SUB as f64).round() as i64;
    let mut p = [0, 1, 2].map(|k| (fx(v[k].p[0]), fx(v[k].p[1])));
    let mut order = [0usize, 1, 2];
    let e = |a: (i64, i64), b: (i64, i64), q: (i64, i64)| {
        (b.0 - a.0) * (q.1 - a.1) - (b.1 - a.1) * (q.0 - a.0)
    };
    let mut area = e(p[0], p[1], p[2]);
    if area == 0 {
        return;
    }
    if area < 0 {
        p.swap(1, 2);
        order.swap(1, 2);
        area = -area;
    }
    // The edges opposite each corner: 1 -> 2 for corner 0, 2 -> 0 for 1, 0 -> 1 for 2.
    let edges = [(p[1], p[2]), (p[2], p[0]), (p[0], p[1])];
    // A sample on an edge is in when the edge is a top or left one: E >= t with t = 0, else 1.
    let t = edges.map(|(a, b)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        i64::from(!(dy > 0 || (dy == 0 && dx < 0)))
    });
    let (ymin, ymax) = (
        p[0].1.min(p[1].1).min(p[2].1),
        p[0].1.max(p[1].1).max(p[2].1),
    );
    let (xmin, xmax) = (
        p[0].0.min(p[1].0).min(p[2].0),
        p[0].0.max(p[1].0).max(p[2].0),
    );
    let half = SUB / 2;
    let j0 = ceil_div(ymin - half, SUB).max(0);
    let j1 = floor_div(ymax - half, SUB).min(h as i64 - 1);
    let i_lo = ceil_div(xmin - half, SUB).max(0);
    let i_hi = floor_div(xmax - half, SUB).min(w as i64 - 1);
    let inv = 1.0 / area as f64;
    for j in j0..=j1 {
        let qy = j * SUB + half;
        let (mut lo, mut hi) = (i_lo, i_hi);
        for (k, &(a, b)) in edges.iter().enumerate() {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let c = dx * (qy - a.1) - t[k];
            // E(qx) = c + t - dy * (qx - a.x) >= t.
            if dy > 0 {
                let qx_max = a.0 + floor_div(c, dy);
                hi = hi.min(floor_div(qx_max - half, SUB));
            } else if dy < 0 {
                let qx_min = a.0 + ceil_div(-c, -dy);
                lo = lo.max(ceil_div(qx_min - half, SUB));
            } else if c < 0 {
                hi = lo - 1;
            }
        }
        if lo > hi {
            continue;
        }
        let row = j as usize * w;
        let q0 = (lo * SUB + half, qy);
        let mut e1 = e(edges[1].0, edges[1].1, q0);
        let mut e2 = e(edges[2].0, edges[2].1, q0);
        let s1 = -(edges[1].1.1 - edges[1].0.1) * SUB;
        let s2 = -(edges[2].1.1 - edges[2].0.1) * SUB;
        for i in lo..=hi {
            let b1 = e1 as f64 * inv;
            let b2 = e2 as f64 * inv;
            let mut b = [0.0; 3];
            b[order[0]] = 1.0 - b1 - b2;
            b[order[1]] = b1;
            b[order[2]] = b2;
            f(row + i as usize, b);
            e1 += s1;
            e2 += s2;
        }
    }
}

fn at(v: &[V; 3], b: [f64; 3]) -> (f64, [f64; 3]) {
    let z = b[0] * v[0].p[2] + b[1] * v[1].p[2] + b[2] * v[2].p[2];
    let c = [0, 1, 2].map(|k| b[0] * v[0].c[k] + b[1] * v[1].c[k] + b[2] * v[2].c[k]);
    (z, c)
}

/// The camera, the lights and the picture's size: what turns a scene's corner into a sample.
struct Stage {
    eye: [f64; 3],
    basis: Basis,
    lens: Lens,
    /// Samples across and down.
    w: usize,
    h: usize,
    /// The tangent of half the vertical field of view, for a perspective lens.
    tan_half: f64,
    near: f64,
    key: [f64; 3],
    fill: [f64; 3],
    guard: f64,
}

impl Stage {
    fn light(&self, n: [f64; 3], base: [f64; 3]) -> [f64; 3] {
        let l = AMBIENT + KEY * dot(n, self.key).max(0.0) + FILL * dot(n, self.fill).max(0.0);
        base.map(|c| (c * l).min(1.0))
    }

    /// The corner in the camera's frame: x right, y up, z ahead.
    fn view(&self, v: &V) -> V {
        let d = sub(v.p, self.eye);
        V {
            p: [
                dot(d, self.basis.r),
                dot(d, self.basis.u),
                dot(d, self.basis.f),
            ],
            c: v.c,
        }
    }

    /// A corner in the camera's frame to the picture: x and y in samples, z the depth key.
    fn project(&self, v: &V) -> V {
        let (w, h) = (self.w as f64, self.h as f64);
        let aspect = w / h;
        let [x, y, z] = v.p;
        let p = match self.lens {
            Lens::Orthographic { half_height } => {
                let hw = half_height * aspect;
                [
                    (x / hw + 1.0) * 0.5 * w,
                    (1.0 - y / half_height) * 0.5 * h,
                    z,
                ]
            }
            Lens::Perspective { .. } => {
                let s = z * self.tan_half;
                [
                    (x / (s * aspect) + 1.0) * 0.5 * w,
                    (1.0 - y / s) * 0.5 * h,
                    -1.0 / z,
                ]
            }
        };
        V { p, c: v.c }
    }

    /// The depth key back to the distance ahead of the camera.
    fn distance(&self, key: f32) -> f64 {
        match self.lens {
            Lens::Orthographic { .. } => f64::from(key),
            Lens::Perspective { .. } => -1.0 / f64::from(key),
        }
    }
}

pub(super) fn render(meshes: &[Mesh], camera: &Camera, options: &RenderOptions) -> Image {
    let ss = options.supersample.clamp(1, 4);
    let (w, h) = (options.width.max(1) * ss, options.height.max(1) * ss);
    let basis = camera.basis();
    let tan_half = match camera.lens {
        Lens::Perspective { fov_y_deg } => {
            trig::tan(trig::radians(0.5 * fov_y_deg.clamp(1.0, 170.0)))
        }
        Lens::Orthographic { .. } => 1.0,
    };
    let reach = dot(
        sub(camera.target, camera.eye),
        sub(camera.target, camera.eye),
    )
    .sqrt();
    let combine = |a: f64, b: f64, c: f64| {
        normalise([0, 1, 2].map(|k| a * basis.r[k] + b * basis.u[k] - c * basis.f[k]))
    };
    let stage = Stage {
        eye: camera.eye,
        basis,
        lens: camera.lens,
        w,
        h,
        tan_half,
        near: (reach * 1e-4).max(1e-6),
        key: combine(-0.35, 0.55, 0.75),
        fill: combine(0.6, -0.25, 0.75),
        guard: w.max(h) as f64,
    };
    let palette = &options.palette;
    let bg = palette.background;
    let to_f = |c: Rgba| [c[0], c[1], c[2]].map(|x| f64::from(x) / 255.0);
    let bg_f = to_f(bg);
    let lit: BTreeSet<&str> = options.highlight.iter().map(String::as_str).collect();

    let mut opaque: Vec<Tri> = Vec::new();
    let mut flat: Vec<Tri> = Vec::new();
    let mut clear: Vec<Tri> = Vec::new();
    let mut poly = Vec::with_capacity(8);
    let mut tmp = Vec::with_capacity(8);
    for (mi, mesh) in meshes.iter().enumerate() {
        let colour = match mesh.colour {
            Some([r, g, b]) => [r, g, b, 255],
            None => palette.colour(mesh.material),
        };
        let highlighted = lit.contains(mesh.id.as_str());
        let mut base = to_f(colour);
        let mut alpha = f64::from(colour[3]) / 255.0;
        if !lit.is_empty() {
            if highlighted {
                base = to_f(palette.highlight);
            } else {
                base = [0, 1, 2].map(|k| base[k] + (bg_f[k] - base[k]) * DIM);
                alpha *= 0.5;
            }
        }
        let see_through = mesh.material.is_translucent() || (options.xray && !highlighted);
        if options.xray && !mesh.material.is_translucent() && !highlighted {
            alpha = XRAY_ALPHA;
        }
        // The cap of a closed mesh; when the section does not close up the mesh is drawn open.
        let section = options.section;
        let axis = section.map(|s| s.axis.index());
        let mut capped = false;
        let mut cap_tris: Vec<[[f64; 3]; 3]> = Vec::new();
        if let (Some(s), Some(k), true, true) = (section, axis, options.caps, mesh.closed)
            && let Some(t) = cap(mesh, k, s.at)
        {
            capped = true;
            cap_tris = t;
        }
        let cull = mesh.closed && (section.is_none() || capped);
        let normals = match options.shading {
            Shading::Smooth => mesh
                .normals
                .as_ref()
                .filter(|n| n.len() == mesh.positions.len()),
            Shading::Flat => None,
        };
        let own = mesh.indices.as_chunks::<3>().0.iter().filter_map(|t| {
            let ix = [t[0] as usize, t[1] as usize, t[2] as usize];
            if ix.iter().any(|&i| i >= mesh.positions.len()) {
                return None;
            }
            Some((ix.map(|i| mesh.positions[i]), Some(ix), false))
        });
        let caps = cap_tris.iter().map(|&p| (p, None, true));
        for (p, ix, is_cap) in own.chain(caps) {
            let face = cross(sub(p[1], p[0]), sub(p[2], p[0]));
            if dot(face, face) == 0.0 || !face.iter().all(|c| c.is_finite()) {
                continue;
            }
            let face = normalise(face);
            let towards = match camera.lens {
                Lens::Orthographic { .. } => [0, 1, 2].map(|k| -basis.f[k]),
                Lens::Perspective { .. } => sub(camera.eye, p[0]),
            };
            let front = dot(face, towards) > 0.0;
            if !front && cull && !is_cap {
                continue;
            }
            let sign = if front { 1.0 } else { -1.0 };
            let corner_normal = |k: usize| match (normals, ix) {
                (Some(ns), Some(ix)) => ns[ix[k]].map(|c| c * sign),
                _ => face.map(|c| c * sign),
            };
            // In the section plane: a cap, or a flat piece the scene lays in it.
            let in_plane = is_cap
                || section
                    .zip(axis)
                    .is_some_and(|(s, k)| p.iter().all(|c| (c[k] - s.at).abs() <= PLANE_EPSILON));
            poly.clear();
            for (k, &corner) in p.iter().enumerate() {
                poly.push(V {
                    p: corner,
                    c: stage.light(corner_normal(k), base),
                });
            }
            if let (Some(s), Some(k)) = (section, axis) {
                let keep = if s.keep_below { -1.0 } else { 1.0 };
                clip(&poly, &mut tmp, |v| keep * (v.p[k] - s.at));
                std::mem::swap(&mut poly, &mut tmp);
            }
            for v in &mut poly {
                *v = stage.view(v);
            }
            if matches!(camera.lens, Lens::Perspective { .. }) {
                clip(&poly, &mut tmp, |v| v.p[2] - stage.near);
                std::mem::swap(&mut poly, &mut tmp);
            }
            if poly.len() < 3 {
                continue;
            }
            for v in &mut poly {
                *v = stage.project(v);
            }
            // Keep the fixed-point coverage far from overflow: clip to a band round the picture.
            let g = stage.guard;
            let far = poly.iter().any(|v| {
                v.p[0] < -g || v.p[1] < -g || v.p[0] > w as f64 + g || v.p[1] > h as f64 + g
            });
            if far {
                for (k, lim, s) in [
                    (0, -g, 1.0),
                    (1, -g, 1.0),
                    (0, w as f64 + g, -1.0),
                    (1, h as f64 + g, -1.0),
                ] {
                    clip(&poly, &mut tmp, |v| s * (v.p[k] - lim));
                    std::mem::swap(&mut poly, &mut tmp);
                }
                if poly.len() < 3 {
                    continue;
                }
            }
            let n = face.map(|c| (c * sign * 127.0).round() as i8);
            for k in 1..poly.len() - 1 {
                let tri = Tri {
                    v: [poly[0], poly[k], poly[k + 1]],
                    alpha,
                    mesh: mi as u32 + 1,
                    normal: n,
                };
                if see_through {
                    clear.push(tri);
                } else if in_plane {
                    flat.push(tri);
                } else {
                    opaque.push(tri);
                }
            }
        }
    }

    let n = w * h;
    let mut buf = Buffers {
        w,
        h,
        colour: vec![[bg[0], bg[1], bg[2]]; n],
        depth: vec![f32::INFINITY; n],
        id: vec![0; n],
        normal: vec![[0; 3]; n],
        cap: vec![false; n],
    };
    // `level`: a triangle in the section plane wins a tie in depth with what is already drawn
    // there (the one drawn later is on top).
    let draw_opaque = |buf: &mut Buffers, tris: &[Tri], write_colour: bool, level: bool| {
        for t in tris {
            let Buffers {
                colour,
                depth,
                id,
                normal,
                cap,
                ..
            } = buf;
            cover(&t.v, w, h, |i, b| {
                let (z, c) = at(&t.v, b);
                let z = z as f32;
                let near = z < depth[i]
                    || (level && z <= depth[i] + LEVEL_TOLERANCE * depth[i].abs().max(1e-3));
                if near {
                    depth[i] = depth[i].min(z);
                    id[i] = t.mesh;
                    normal[i] = t.normal;
                    cap[i] = level;
                    if write_colour {
                        colour[i] = c.map(quantise);
                    }
                }
            });
        }
    };
    draw_opaque(&mut buf, &opaque, true, false);
    draw_opaque(&mut buf, &flat, true, true);
    let depth_jump = bounds_of(meshes)
        .map(|b| {
            let s = b.size();
            dot(s, s).sqrt()
        })
        .unwrap_or(1.0)
        * 0.02;
    let mut lines = None;
    if options.outlines {
        if options.xray {
            // The outlines of the nearest surfaces, as if all were opaque, drawn over the
            // see-through picture at the end.
            let mut all: Vec<Tri> = opaque.clone();
            all.extend_from_slice(&clear);
            let mut g = Buffers {
                w,
                h,
                colour: Vec::new(),
                depth: vec![f32::INFINITY; n],
                id: vec![0; n],
                normal: vec![[0; 3]; n],
                cap: vec![false; n],
            };
            draw_opaque(&mut g, &all, false, false);
            draw_opaque(&mut g, &flat, false, true);
            lines = Some(outlines(&g, &stage, depth_jump, ss));
        } else {
            let mask = outlines(&buf, &stage, depth_jump, ss);
            paint(&mut buf.colour, &mask, palette.outline);
        }
    }

    // See-through triangles, far to near; ties keep the order they came in.
    let key = |t: &Tri| t.v[0].p[2] + t.v[1].p[2] + t.v[2].p[2];
    clear.sort_by(|a, b| key(b).total_cmp(&key(a)));
    for t in &clear {
        let Buffers { colour, depth, .. } = &mut buf;
        cover(&t.v, w, h, |i, b| {
            let (z, c) = at(&t.v, b);
            if (z as f32) < depth[i] {
                let d = colour[i];
                colour[i] = [0, 1, 2].map(|k| {
                    let dst = f64::from(d[k]) / 255.0;
                    quantise(dst + (c[k] - dst) * t.alpha)
                });
            }
        });
    }
    if let Some(mask) = lines {
        paint(&mut buf.colour, &mask, palette.outline);
    }

    downsample(&buf, ss, options.width.max(1), options.height.max(1))
}

/// The samples on an outline: where the nearest mesh changes, its depth jumps by more than
/// `depth_jump` mm, or its faces turn by more than the crease angle, about one pixel wide.
fn outlines(g: &Buffers, stage: &Stage, depth_jump: f64, ss: usize) -> Vec<bool> {
    let (w, h) = (g.w, g.h);
    let crease = trig::sin_cos(trig::radians(CREASE_DEG)).1 * 127.0 * 127.0;
    let differs = |a: usize, b: usize| {
        if g.id[a] != g.id[b] {
            // The bands of a section meet without a line, so thin layers stay readable.
            return !(g.cap[a] && g.cap[b]);
        }
        if g.id[a] == 0 {
            return false;
        }
        let (na, nb) = (g.normal[a], g.normal[b]);
        let d: i32 = (0..3).map(|k| i32::from(na[k]) * i32::from(nb[k])).sum();
        if f64::from(d) < crease {
            return true;
        }
        (stage.distance(g.depth[a]) - stage.distance(g.depth[b])).abs() > depth_jump
    };
    let mut mask = vec![false; w * h];
    let reach = (ss / 2).max(1);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            for k in 1..=reach {
                if x + k < w && differs(i, i + k) {
                    mask[i] = true;
                    if ss > 1 {
                        mask[i + k] = true;
                    }
                }
                if y + k < h && differs(i, i + k * w) {
                    mask[i] = true;
                    if ss > 1 {
                        mask[i + k * w] = true;
                    }
                }
            }
        }
    }
    mask
}

fn paint(colour: &mut [[u8; 3]], mask: &[bool], outline: Rgba) {
    for (c, &m) in colour.iter_mut().zip(mask) {
        if m {
            *c = [0, 1, 2].map(|k| {
                let d = f64::from(c[k]);
                (d + (f64::from(outline[k]) - d) * OUTLINE_COVER).round() as u8
            });
        }
    }
}

fn downsample(buf: &Buffers, ss: usize, width: usize, height: usize) -> Image {
    let mut rgba = Vec::with_capacity(width * height * 4);
    let n = (ss * ss) as u32;
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0u32; 3];
            for sy in 0..ss {
                let row = (y * ss + sy) * buf.w;
                for sx in 0..ss {
                    let c = buf.colour[row + x * ss + sx];
                    for k in 0..3 {
                        sum[k] += u32::from(c[k]);
                    }
                }
            }
            for s in sum {
                rgba.push(((s + n / 2) / n) as u8);
            }
            rgba.push(255);
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64) -> V {
        V {
            p: [x, y, 0.0],
            c: [0.0; 3],
        }
    }

    #[test]
    fn two_triangles_sharing_an_edge_cover_each_sample_once() {
        let (w, h) = (37, 29);
        let mut count = vec![0u8; w * h];
        // A quad cut along an odd diagonal, and a fan of thin triangles about a centre.
        let quad = [v(1.3, 2.7), v(33.1, 4.2), v(30.9, 27.5), v(3.3, 25.1)];
        for tri in [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]] {
            cover(&tri, w, h, |i, _| count[i] += 1);
        }
        assert!(count.iter().all(|&c| c <= 1));
        let mut fan = vec![0u8; w * h];
        let c = v(18.5, 14.5);
        let pts = crate::mesh::circle([18.5, 14.5], 12.0, 13);
        for k in 0..pts.len() {
            let (a, b) = (pts[k], pts[(k + 1) % pts.len()]);
            cover(&[c, v(a[0], a[1]), v(b[0], b[1])], w, h, |i, _| fan[i] += 1);
        }
        assert!(fan.iter().all(|&c| c <= 1));
        // The sample at the centre is in exactly one of the thirteen.
        assert_eq!(fan[14 * w + 18], 1);
    }

    #[test]
    fn weights_sum_to_one_and_find_the_corners() {
        let tri = [v(0.0, 0.0), v(10.0, 0.0), v(0.0, 10.0)];
        let mut seen = 0;
        cover(&tri, 16, 16, |i, b| {
            seen += 1;
            assert!((b[0] + b[1] + b[2] - 1.0).abs() < 1e-12);
            if i == 0 {
                assert!(b[0] > 0.85);
            }
        });
        // Samples with x + y < 9 (centres at +0.5): 45, and the diagonal x + y = 9 by the rule.
        assert!((45..=55).contains(&seen), "{seen}");
    }
}
