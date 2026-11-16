//! `netsmith case import`: the outline and cutouts of a board, and its mounting holes, from the
//! STEP model of its case, as source for the board's physical file (ADR 0109 "Left open").
//!
//! The model is posed as a `CaseModel` poses it (`.rx`, `.ry`, `.rz`, then `.x`, `.y`, `.z`)
//! and cut by a plane parallel to the board (`at`, in the 3D scene's frame, where the board's
//! top face is z = 0 and its bottom face z = -thickness: the default, -0.8 mm, is the middle of
//! a 1.6 mm board). The cut draws closed curves on the case's solids ([`netsmith_3d::CaseRing`]),
//! and how many of the others enclose each is its depth:
//!
//! - depth 0, the outside of a wall: not used;
//! - depth 1, a pocket in it: **the board's outline**, the pocket the board stands in (the
//!   largest, if there are several; the others are said and left out). Where the case's wall
//!   has a rib, a notch or a rounded corner, the outline has it too, so it is a `polygon`
//!   (corners, `arc through` for arcs) unless it is a rectangle with equal rounded corners,
//!   exactly as `dxf import` prints one;
//! - depth 2, inside the pocket: a post, a pin or a block standing where the board is: **a
//!   `Cutout`** (a rounded rectangle, a slot or a circle; any other shape is refused, as it is
//!   by `dxf import`);
//! - deeper: not used.
//!
//! A second plane, `holes_at` (a standoff's height, below the board), gives the **mounting
//! holes**: each circle at depth 3, a hole in a post inside the pocket, is a `MountingHole` at
//! its centre, stated by its diameter (`.diameter`, as `dxf import` states a hole); a drill the
//! generator cannot draw refuses the import, as it does there.
//!
//! With `limits`, the **height limits**: each underside of the case above the board's top
//! face (a lid, a rib or a boss hanging from it) is a `Keepout` that states `.max_height`
//! over the region of the underside, on the top face (`.layers &= "top"`), found by
//! [`netsmith_3d::CaseCeilings`]; the same for what the case holds below the board's bottom
//! face (a floor, a rib or a boss standing under it, `thickness` mm below the top face), on
//! the bottom face (`.layers &= "bottom"`), each height from that face. A part of the board
//! that is taller meets the case there. What stands from the board up to the case is a
//! cutout, not a limit. A region is covered by rectangles, each a `Keepout`: one for a
//! rectangle, one for each part of an L, slabs of at most `limit_step` mm for a curve (a round
//! underside is limited a little more than it covers, not over its box). A sloped underside
//! (a dome, a slope, a chamfer) is followed in steps of `limit_step` mm, each limited to the
//! height of its lower end.
//!
//! The section of a model is of its triangles (0.01 mm of chord), so a curve of arcs has a
//! corner for each chord, and some of them inside the circle by up to that chord;
//! [`polygon_of_ring`] finds the arcs again, within the reader's chord, and joins the collinear
//! runs, and the lengths are printed to a micrometre. A shape that is not made of lines and
//! circular arcs stays chords.
//! The result is the same kind of text as `dxf import`'s and is reviewed the same way; nothing
//! is derived at build time (ADR 0109).

use netsmith_3d::{CaseRing, PlacedCase, place_case, read_step};
use netsmith_core::CaseFigures;
use netsmith_dxf::{Hole, Polygon, Vertex, outline_of_loops};

use std::fmt::Write as _;

use crate::dxf::{Imported, imports_text, translate};

/// What `case import` is asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// The model's `.rx`, `.ry`, `.rz`, in degrees.
    pub rotation: [f64; 3],
    /// The model's `.x`, `.y`, `.z`, in mm.
    pub offset: [f64; 3],
    /// The height of the plane of the outline and the cutouts, in mm, in the posed frame.
    pub at: f64,
    /// The height of the plane of the mounting holes, if the model has any to read.
    pub holes_at: Option<f64>,
    /// Whether to print the height limits the case puts over and under the board (`Keepout`s).
    pub limits: bool,
    /// The board's thickness, in mm: its bottom face is z = -thickness (for the limits of the
    /// bottom face).
    pub thickness: f64,
    /// How finely a sloped or curved underside is followed, in mm: the height of a step of a
    /// slope, the height of a slab of a curve.
    pub limit_step: f64,
}

impl Default for Request {
    fn default() -> Request {
        Request {
            rotation: [0.0; 3],
            offset: [0.0; 3],
            at: -0.8,
            holes_at: None,
            limits: false,
            thickness: 1.6,
            limit_step: 0.5,
        }
    }
}

type P = (f64, f64);

fn number(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn pt(p: [f64; 2]) -> P {
    (p[0], p[1])
}

fn area(p: &[[f64; 2]]) -> f64 {
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

/// Whether `p` is inside `ring` (the even-odd rule).
fn inside(p: [f64; 2], ring: &[[f64; 2]]) -> bool {
    let n = ring.len();
    let mut crossings = 0;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
            if x > p[0] {
                crossings += 1;
            }
        }
    }
    crossings % 2 == 1
}

fn describe(ring: &CaseRing) -> String {
    let (lo, hi) = bounds(&ring.points);
    format!(
        "`{}`, {} x {} mm at ({}, {})",
        ring.solid,
        number(hi[0] - lo[0]),
        number(hi[1] - lo[1]),
        number(lo[0]),
        number(lo[1])
    )
}

fn bounds(points: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in points {
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    (lo, hi)
}

/// Read the STEP model in `bytes` (`name` is its name for messages) and translate its section.
pub fn import(bytes: &[u8], name: &str, request: &Request) -> Result<Imported, String> {
    if request.limits && !(request.limit_step >= 0.05 && request.limit_step.is_finite()) {
        return Err(format!(
            "{name}: the step of a limit is {} mm, and it must be at least 0.05 mm",
            number(request.limit_step)
        ));
    }
    if request.limits && !(request.thickness > 0.0 && request.thickness.is_finite()) {
        return Err(format!(
            "{name}: the board's thickness is {} mm, and it must be more than 0",
            number(request.thickness)
        ));
    }
    let mesh = read_step(bytes).map_err(|e| format!("{name}: {e}"))?;
    let case = place_case(
        "case",
        name,
        "",
        CaseFigures::default(),
        &mesh,
        request.rotation,
        request.offset,
    );
    let at = request.at;
    let rings = case.section(at).map_err(|e| format!("{name}: {e}"))?;
    let mut warnings: Vec<String> = Vec::new();

    let mut pockets: Vec<&CaseRing> = rings.iter().filter(|r| r.depth == 1).collect();
    pockets.sort_by(|a, b| area(&b.points).total_cmp(&area(&a.points)));
    let Some(pocket) = pockets.first().copied() else {
        let seen: Vec<String> = rings
            .iter()
            .map(|r| format!("depth {}: {}", r.depth, describe(r)))
            .collect();
        return Err(format!(
            "{name}: the plane z = {} mm cuts no pocket in the model's walls, so there is no outline to read ({}). Choose the height of the board with --at (the board's top face is z = 0, with the model posed as `CaseModel` poses it)",
            number(at),
            if seen.is_empty() {
                "it cuts nothing".to_string()
            } else {
                seen.join("; ")
            }
        ));
    };
    for other in &pockets[1..] {
        warnings.push(format!(
            "another pocket at z = {} mm, {}, was left out: the outline is the largest, {}",
            number(at),
            describe(other),
            describe(pocket)
        ));
    }

    let outer = polygon_of_ring(&pocket.points);
    let mut islands: Vec<&CaseRing> = Vec::new();
    for r in rings.iter().filter(|r| r.depth == 2) {
        if inside(r.points[0], &pocket.points) {
            islands.push(r);
        }
    }
    let cutouts: Vec<Polygon> = islands.iter().map(|r| polygon_of_ring(&r.points)).collect();
    // A cutout that has no shape the language states is listed with the rest by `translate`.

    let mut holes: Vec<Hole> = Vec::new();
    let mut holes_report = None;
    if let Some(hz) = request.holes_at {
        let low = case.section(hz).map_err(|e| format!("{name}: {e}"))?;
        let mut found = 0;
        for r in low.iter().filter(|r| r.depth == 3) {
            if !inside(r.points[0], &pocket.points) {
                continue;
            }
            found += 1;
            match polygon_of_ring(&r.points).as_circle() {
                Some((centre, diameter)) => holes.push(Hole { centre, diameter }),
                None => warnings.push(format!(
                    "a hole at z = {} mm, {}, is not a circle and was left out",
                    number(hz),
                    describe(r)
                )),
            }
        }
        holes_report = Some(format!(
            "holes: {found} hole{} in posts inside the outline at z = {} mm",
            if found == 1 { "" } else { "s" },
            number(hz)
        ));
    }

    let mut outline = outline_of_loops(&outer, &cutouts, &holes);
    outline.warnings = warnings;
    let mut imported = translate(&outline, name, "case import")?;

    let (lo, _) = bounds(&pocket.points);
    let walls: Vec<String> = rings
        .iter()
        .filter(|r| r.depth == 0)
        .map(|r| {
            let (l, h) = bounds(&r.points);
            format!("{} x {} mm", number(h[0] - l[0]), number(h[1] - l[1]))
        })
        .collect();
    let mut head = vec![
        format!(
            "model: {name}, posed .rx {} .ry {} .rz {} (degrees), .x {} .y {} .z {} (mm)",
            number(request.rotation[0]),
            number(request.rotation[1]),
            number(request.rotation[2]),
            number(request.offset[0]),
            number(request.offset[1]),
            number(request.offset[2]),
        ),
        format!(
            "section: z = {} mm; outside of the walls {}; the pocket is the outline",
            number(at),
            if walls.is_empty() {
                "none".to_string()
            } else {
                walls.join(", ")
            }
        ),
        format!(
            "pose: with this pose the pocket's bottom left corner is at ({}, {}) of the 3D frame, whose origin is the outline's bottom left corner on the top face; a model that stands as the board does has .x &= {}mm and .y &= {}mm",
            number(lo[0]),
            number(lo[1]),
            number(request.offset[0] - lo[0]),
            number(request.offset[1] - lo[1]),
        ),
    ];
    head.extend(holes_report);
    imported.families = vec!["cutout"];
    if request.holes_at.is_some() {
        imported.families.push("hole");
    }
    if request.limits {
        imported.families.push("limit");
        head.extend(limits(&case, pocket, request, &mut imported));
    }
    imported.report.splice(0..0, head);
    Ok(imported)
}

/// The `Keepout`s of the case's height limits over and under `pocket`, added to `imported`, and
/// the lines that say what was found. A plane that could not be cut is a warning.
fn limits(
    case: &PlacedCase,
    pocket: &CaseRing,
    request: &Request,
    imported: &mut Imported,
) -> Vec<String> {
    let step = request.limit_step;
    let faces = [
        ("top", case.ceilings(&pocket.points, step)),
        (
            "bottom",
            case.ceilings_below(&pocket.points, request.thickness, step),
        ),
    ];
    for (face, found) in &faces {
        for e in &found.skipped {
            imported.warnings.push(format!(
                "{e}: the height limits that plane shows for the {face} face are not read"
            ));
        }
    }
    if faces.iter().all(|(_, f)| f.zones.is_empty()) {
        return vec![
            "limits: no underside of the case above the board's top face and no top of it below the board's bottom face limits a part (no lid, rib, boss or floor over or under the pocket, or none that is not a wall)"
                .to_string(),
        ];
    }
    let (lo, hi) = bounds(&pocket.points);
    // The board's frame: the outline's top left corner, y down.
    let mm = |v: f64| format!("{}mm", number(um(v)));
    let total: usize = faces.iter().map(|(_, f)| f.zones.len()).sum();
    let mut report = vec![format!(
        "limits: {total} height limit{} over and under the board, each over a rectangle of an underside of the case (a round underside is covered by slabs of at most {} mm, a slope followed in steps of {} mm), for the parts on the face it names",
        if total == 1 { "" } else { "s" },
        number(step),
        number(step),
    )];
    imported.members.push('\n');
    let mut n = 0;
    for (face, found) in &faces {
        for z in &found.zones {
            n += 1;
            let id = format!("limit{n}");
            let (x, y) = (z.lo[0] - lo[0], hi[1] - z.hi[1]);
            let (width, height) = (z.hi[0] - z.lo[0], z.hi[1] - z.lo[1]);
            let _ = writeln!(imported.members, ".{id}: Keepout {{");
            let _ = writeln!(imported.members, "    .x &= {}", mm(x));
            let _ = writeln!(imported.members, "    .y &= {}", mm(y));
            let _ = writeln!(imported.members, "    .width &= {}", mm(width));
            let _ = writeln!(imported.members, "    .height &= {}", mm(height));
            let _ = writeln!(imported.members, "    .layers &= \"{face}\"");
            let _ = writeln!(imported.members, "    .max_height &= {}", mm(z.height));
            let _ = writeln!(imported.members, "}}");
            imported.names.push(id.clone());
            report.push(format!(
                "{id}: on the {face} face at most {} mm over {} x {} mm with its top left corner at ({}, {}) on the board: {} of `{}`{}",
                number(z.height),
                number(width),
                number(height),
                number(x),
                number(y),
                if *face == "top" {
                    "the underside"
                } else {
                    "the top"
                },
                z.solid,
                if z.sloped {
                    " (a step of a slope: the underside is no lower)"
                } else {
                    ""
                }
            ));
        }
    }
    imported.needed.push(("std::pcb", "Keepout"));
    imported.imports = imports_text(&imported.needed);
    report
}

// ---------------------------------------------------------------------------------------
// From chords to lines and arcs.

/// How far a corner may be from the line or the circle it is taken to lie on, in mm (the
/// tolerance `dxf import` recognises a rounded rectangle by).
const FIT: f64 = 2e-3;

/// The most a chord of an arc may be from its arc, in mm: the reader's chord tolerance is
/// 0.01, so a polygon's own corners (an octagon's sides) are not taken for an arc's chords.
const SAGITTA: f64 = 0.02;

/// How far inside its circle a corner of an arc's section may be, in mm: the reader's chord
/// tolerance (0.01) and room for where the circle of those corners is found.
const BAND: f64 = 0.015;

/// How far inside a circle a corner may be and still count when its centre is found again, in mm.
const SELECT: f64 = 0.004;

/// How many corners in a row that do not make an arc end the search for a longer one.
const PATIENCE: usize = 6;

/// How far a corner may be from the line between its neighbours to be dropped, in mm.
const COLLINEAR: f64 = 1e-5;

/// A length to a micrometre: the section is of triangles within 10 micrometres of the model, so
/// no more is said of it, and a circle's diameter and radius agree.
fn um(v: f64) -> f64 {
    let r = (v * 1e3).round() / 1e3;
    if r == 0.0 { 0.0 } else { r }
}

fn dist(a: P, b: P) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn cross(o: P, a: P, b: P) -> f64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

/// How far `p` is from the line through `a` and `b`.
fn line_distance(a: P, b: P, p: P) -> f64 {
    let d = dist(a, b);
    if d == 0.0 {
        dist(a, p)
    } else {
        (cross(a, b, p) / d).abs()
    }
}

/// The angle turned from `a` to `b` round `centre`, in (-pi, pi].
fn angle_between(centre: P, a: P, b: P) -> f64 {
    let (u, v) = (
        (a.0 - centre.0, a.1 - centre.1),
        (b.0 - centre.0, b.1 - centre.1),
    );
    (u.0 * v.1 - u.1 * v.0).atan2(u.0 * v.0 + u.1 * v.1)
}

/// The circle the points are the corners of the chords of: its centre and radius, `None` when
/// they are in a line or a circle of more than a kilometre.
///
/// A plane that cuts the side of a cylinder meets the triangles' edges between the points of
/// the circle, on chords, so the section has corners on the circle (where it meets an edge
/// that stands on it) and corners inside it by up to the reader's chord tolerance. The centre
/// is the least squares one, and the radius reaches the outermost corners: the true ones.
fn circle_of(pts: &[P]) -> Option<(P, f64)> {
    let (mut centre, mut r) = least_squares_circle(pts)?;
    // The corners inside the circle pull the centre toward them: fit again to the corners that
    // are near the circle found, which are the true ones and the least bent of the others.
    for _ in 0..4 {
        let near: Vec<P> = pts
            .iter()
            .copied()
            .filter(|&p| dist(p, centre) - r >= -SELECT)
            .collect();
        if near.len() < 3 || near.len() == pts.len() {
            break;
        }
        match least_squares_circle(&near) {
            Some((c, rr)) => (centre, r) = (c, rr),
            None => break,
        }
    }
    let r = pts.iter().map(|&p| dist(p, centre)).fold(r, f64::max);
    (r.is_finite() && r < 1e3).then_some((centre, r))
}

/// The least squares circle of `pts` (a radius that is the mean one), `None` when they are in a
/// line.
fn least_squares_circle(pts: &[P]) -> Option<(P, f64)> {
    let n = pts.len() as f64;
    let (mx, my) = (
        pts.iter().map(|p| p.0).sum::<f64>() / n,
        pts.iter().map(|p| p.1).sum::<f64>() / n,
    );
    // x^2 + y^2 + D x + E y + F = 0 by least squares, about the mean.
    let (mut sxx, mut sxy, mut syy, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut bx, mut by, mut bs) = (0.0, 0.0, 0.0);
    for p in pts {
        let (x, y) = (p.0 - mx, p.1 - my);
        let q = x * x + y * y;
        sxx += x * x;
        sxy += x * y;
        syy += y * y;
        sx += x;
        sy += y;
        bx += x * q;
        by += y * q;
        bs += q;
    }
    // The normal equations: [sxx sxy sx; sxy syy sy; sx sy n] [D E F] = -[bx by bs].
    let m = [[sxx, sxy, sx], [sxy, syy, sy], [sx, sy, n]];
    let rhs = [-bx, -by, -bs];
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(m);
    if d.abs() < 1e-12 {
        return None;
    }
    let solve = |col: usize| {
        let mut t = m;
        for r in 0..3 {
            t[r][col] = rhs[r];
        }
        det(t) / d
    };
    let centre = (mx - solve(0) / 2.0, my - solve(1) / 2.0);
    let r = pts.iter().map(|&p| dist(p, centre)).sum::<f64>() / n;
    Some((centre, r))
}

/// The arc the corners `pts` are the chords of: its sweep (radians, positive counterclockwise)
/// and the circle, or `None` if they are not within the chord tolerance of one circle, turn both
/// ways round it, or are the corners of a coarser shape.
fn arc_of(pts: &[P]) -> Option<(f64, P, f64)> {
    let (centre, r) = circle_of(pts)?;
    if pts
        .iter()
        .any(|&p| !(-BAND..=FIT).contains(&(dist(p, centre) - r)))
    {
        return None;
    }
    let mut total = 0.0;
    let mut sign = 0.0;
    for w in pts.windows(2) {
        let d = dist(w[0], w[1]);
        if r - (r * r - d * d / 4.0).max(0.0).sqrt() > SAGITTA {
            return None;
        }
        let step = angle_between(centre, w[0], w[1]);
        if sign * step < 0.0 {
            return None;
        }
        if step != 0.0 {
            sign = step;
        }
        total += step;
    }
    Some((total, centre, r))
}

/// The first edge from `pts[i]`: the index it ends at (`pts` ends with its first point again, at
/// `n`) and its bulge.
fn next_edge(pts: &[P], i: usize, n: usize) -> (usize, f64) {
    let mut line_end = i + 1;
    for j in i + 2..=n {
        if (i + 1..j).all(|m| line_distance(pts[i], pts[j], pts[m]) <= FIT) {
            line_end = j;
        } else {
            break;
        }
    }
    let mut arc: Option<(usize, f64)> = None;
    // The circle of a few corners is not yet sure (a corner inside it by the chord tolerance
    // is a large part of it), so a failure is not the end until several more corners have failed.
    let mut failed = 0;
    for k in i + 3..=n {
        match arc_of(&pts[i..=k]) {
            Some((sweep, ..)) => {
                arc = Some((k, sweep));
                failed = 0;
            }
            None => {
                failed += 1;
                if failed > PATIENCE {
                    break;
                }
            }
        }
    }
    match arc {
        Some((k, sweep)) if k > line_end => (k, (sweep / 4.0).tan()),
        _ => (line_end, 0.0),
    }
}

/// The edges of the closed curve `pts` from `start`, as the vertex each begins at and its bulge.
fn edges_from(ring: &[P], start: usize) -> Vec<(usize, f64)> {
    let n = ring.len();
    let mut pts: Vec<P> = (0..=n).map(|k| ring[(start + k) % n]).collect();
    pts[n] = pts[0];
    let mut edges = Vec::new();
    let mut i = 0;
    while i < n {
        let (end, bulge) = next_edge(&pts, i, n);
        edges.push(((start + i) % n, bulge));
        i = end;
    }
    edges
}

/// A closed curve of the section (counterclockwise) as a polygon of lines and arcs: its
/// corners, the collinear ones dropped, and each run of chords that lie on one circle one arc.
pub(crate) fn polygon_of_ring(ring: &[[f64; 2]]) -> Polygon {
    let mut pts: Vec<P> = Vec::with_capacity(ring.len());
    for &p in ring {
        if pts.last().is_none_or(|&q| dist(q, pt(p)) > 1e-9) {
            pts.push(pt(p));
        }
    }
    while pts.len() > 1 && dist(pts[0], pts[pts.len() - 1]) <= 1e-9 {
        pts.pop();
    }
    // A corner on the line between its neighbours says nothing: a plane that cuts the quads of
    // a cylinder's side meets their diagonals between the points of the circle, on its chords.
    let mut kept: Vec<P> = Vec::with_capacity(pts.len());
    for k in 0..pts.len() {
        let next = pts[(k + 1) % pts.len()];
        match kept.last() {
            Some(&prev) if line_distance(prev, next, pts[k]) <= COLLINEAR => {}
            _ => kept.push(pts[k]),
        }
    }
    let pts = kept;
    let n = pts.len();
    // The whole curve on one circle: two half circles.
    if n >= 8
        && let Some((total, centre, r)) = arc_of(&[pts.as_slice(), &pts[..1]].concat())
        && (total.abs() - std::f64::consts::TAU).abs() < 1e-6
    {
        let sign = total.signum();
        let (centre, r) = ((um(centre.0), um(centre.1)), um(r));
        return Polygon {
            vertices: vec![
                Vertex {
                    x: centre.0 + r,
                    y: centre.1,
                    bulge: sign,
                },
                Vertex {
                    x: centre.0 - r,
                    y: centre.1,
                    bulge: sign,
                },
            ],
        };
    }
    if n < 4 {
        return lines(&pts);
    }
    // Start where an edge starts: the beginning of the longest chord, which is a straight
    // edge when there is one. Then once more from the last edge's start, so that the seam is
    // not inside an edge.
    let first = (0..n)
        .max_by(|&a, &b| {
            dist(pts[a], pts[(a + 1) % n])
                .total_cmp(&dist(pts[b], pts[(b + 1) % n]))
                .then(b.cmp(&a))
        })
        .unwrap_or(0);
    let pass = edges_from(&pts, first);
    let start = pass.last().map_or(first, |e| e.0);
    let edges = if start == first {
        pass
    } else {
        edges_from(&pts, start)
    };
    Polygon {
        vertices: edges
            .into_iter()
            .map(|(k, bulge)| Vertex {
                x: um(pts[k].0),
                y: um(pts[k].1),
                bulge,
            })
            .collect(),
    }
}

fn lines(pts: &[P]) -> Polygon {
    Polygon {
        vertices: pts
            .iter()
            .map(|p| Vertex {
                x: p.0,
                y: p.1,
                bulge: 0.0,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests;
