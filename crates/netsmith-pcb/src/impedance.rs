//! The characteristic impedance of a track, worked out from the board's build-up (issue #759),
//! and the width a track needs for an impedance: the _Track impedance_ lines of
//! `fab-notes.txt`.
//!
//! Every figure is quasi-static (no dispersion, so it holds where the track is short beside a
//! wavelength in it) and lossless, from closed forms whose source is open to read:
//!
//! - A track on an outer layer is a microstrip over the next copper layer, by E. Hammerstad and
//!   Ø. Jensen, "Accurate Models for Microstrip Computer-Aided Design", IEEE MTT-S
//!   International Microwave Symposium Digest, 1980, pp. 407-409, as the Qucs technical
//!   documentation gives it (<https://qucs.sourceforge.net/tech/node75.html>) and Qucs's
//!   `msline.cpp` computes it (its `Hammerstad` model, with the strip thickness correction by
//!   Wheeler's method). Its stated accuracy: the impedance of the line in one medium better
//!   than 0.01 % for w/h up to 1 and 0.03 % up to 1000, the effective permittivity better than
//!   0.2 % for εr below 128 and 0.01 <= w/h <= 100. The thickness correction asks for t << h
//!   and t < w/2; t < w/2 is held, and t << h, which no source puts a number on, is not (1 oz
//!   outer copper is a third of JLCPCB's thinnest prepreg, 0.0994 mm). The solder mask over the track is not counted: a covered microstrip has a
//!   lower impedance than the figure.
//! - A track on an inner layer is a stripline between the copper layers above and below, by
//!   H. A. Wheeler, "Transmission-Line Properties of a Strip Line Between Parallel Planes",
//!   IEEE Trans. MTT-26, no. 11, 1978, pp. 866-876, as Wikipedia's "Stripline" (from R. W.
//!   Rhea, *HF Filter Design and Computer Simulation*, 1995, pp. 106-107) and K. Ahrens et al.,
//!   arXiv:1309.5331, appendix A, give it. Its stated accuracy: 0.5 % for a strip centred
//!   between the planes with W'/(b - t) < 10, W' the width with the thickness correction and b
//!   the space between the planes.
//!
//! A build-up's inner track is not centred and has a different dielectric layer above and
//! below (a prepreg on one side, a core on the other). Its capacitance is split at the plane
//! of the strip into the part to the plane above and the part to the plane below, each half
//! the capacitance of a centred stripline of twice that height, in its own dielectric (for one
//! dielectric this is KiCad's offset stripline, `pcb_calculator`'s `stripline.cpp`). The split
//! is exact for a centred strip in one dielectric and an approximation otherwise; no source
//! states its error, and the report says so.
//!
//! Outside a formula's stated domain the figure is refused ([`OutOfRange`]), never
//! extrapolated.

use std::f64::consts::{E, PI};

use netsmith_core::OUNCE_OF_COPPER;
use netsmith_core::{DielectricLayer, InnerUse, Stackup};

/// The impedance of free space, μ0·c, in ohms (CODATA 2018).
const ETA0: f64 = 376.730_313_412;

/// A line's characteristic impedance and its effective relative permittivity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line {
    /// In ohms.
    pub impedance: f64,
    /// The relative permittivity a wave on the line sees: its speed is c / sqrt(this).
    pub epsilon_eff: f64,
}

/// Why a figure is refused: the geometry is outside the formula's stated domain.
#[derive(Debug, Clone, PartialEq)]
pub struct OutOfRange(pub String);

impl std::fmt::Display for OutOfRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One dielectric layer beside a track: its thickness in mm (from the track's copper to the
/// reference copper) and its relative permittivity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackDielectric {
    pub thickness: f64,
    pub epsilon: f64,
}

/// Hammerstad and Jensen's impedance of a microstrip of normalized width `u` in one medium of
/// εr 1.
fn hj_z01(u: f64) -> f64 {
    let f = 6.0 + (2.0 * PI - 6.0) * (-(30.666 / u).powf(0.7528)).exp();
    ETA0 / (2.0 * PI) * (f / u + (1.0 + 4.0 / (u * u)).sqrt()).ln()
}

/// Hammerstad and Jensen's effective permittivity of a microstrip of normalized width `u`.
fn hj_epsilon_eff(u: f64, er: f64) -> f64 {
    let u4 = u.powi(4);
    let a = 1.0
        + ((u4 + (u / 52.0).powi(2)) / (u4 + 0.432)).ln() / 49.0
        + (1.0 + (u / 18.1).powi(3)).ln() / 18.7;
    let b = 0.564 * ((er - 0.9) / (er + 3.0)).powf(0.053);
    (er + 1.0) / 2.0 + (er - 1.0) / 2.0 * (1.0 + 10.0 / u).powf(-a * b)
}

/// A microstrip: a track `width` wide and `thickness` thick (mm) on a dielectric layer over its
/// reference copper, uncovered (see the module's header).
pub fn microstrip(width: f64, thickness: f64, below: TrackDielectric) -> Result<Line, OutOfRange> {
    let (h, er) = (below.thickness, below.epsilon);
    let u = width / h;
    if !(1.0..=128.0).contains(&er) {
        return Err(OutOfRange(format!(
            "microstrip needs a relative permittivity from 1 to 128, not {er}"
        )));
    }
    if !(0.01..=100.0).contains(&u) {
        return Err(OutOfRange(format!(
            "microstrip needs a width from 0.01 to 100 times the dielectric's thickness, not {u:.3}"
        )));
    }
    if thickness < 0.0 || thickness >= width / 2.0 {
        return Err(OutOfRange(format!(
            "microstrip's thickness correction needs copper thinner than half the track, not {thickness} mm on a {width} mm track"
        )));
    }
    let t = thickness / h;
    // Wheeler's thickness correction as Hammerstad and Jensen give it: the width a strip of no
    // thickness would need, in air (du1) and in the dielectric (dur).
    let du1 = if t > 0.0 {
        let coth = 1.0 / (6.517 * u).sqrt().tanh();
        t / PI * (1.0 + 4.0 * E / (t * coth * coth)).ln()
    } else {
        0.0
    };
    let dur = du1 * (1.0 + 1.0 / (er - 1.0).sqrt().cosh()) / 2.0;
    let (u1, ur) = (u + du1, u + dur);
    let (z1, zr) = (hj_z01(u1), hj_z01(ur));
    let e = hj_epsilon_eff(ur, er);
    Ok(Line {
        impedance: zr / e.sqrt(),
        epsilon_eff: e * (z1 / zr).powi(2),
    })
}

/// Wheeler's impedance of a strip `width` wide and `thickness` thick centred between two planes
/// `spacing` apart, in a medium of εr 1.
fn wheeler_centred(width: f64, thickness: f64, spacing: f64) -> Result<f64, OutOfRange> {
    let (w, t, b) = (width, thickness, spacing);
    if t < 0.0 || t >= b {
        return Err(OutOfRange(format!(
            "stripline needs copper thinner than the space between the planes, not {t} mm in {b} mm"
        )));
    }
    let x = t / b;
    let dw = if t > 0.0 {
        let m = 2.0 / (1.0 + 2.0 / 3.0 * x / (1.0 - x));
        let ln = ((x / (2.0 - x)).powi(2) + (0.0796 * x / (w / b + 1.1 * x)).powf(m)).ln();
        t / PI * (1.0 - 0.5 * ln)
    } else {
        0.0
    };
    let ratio = (w + dw) / (b - t);
    if ratio >= 10.0 {
        return Err(OutOfRange(format!(
            "stripline needs a width under 10 times the space between the copper and its planes, not {ratio:.2}"
        )));
    }
    let c = 8.0 / PI / ratio;
    Ok(ETA0 / (4.0 * PI) * (1.0 + c / 2.0 * (c + (c * c + 6.27).sqrt())).ln())
}

/// A stripline: a track `width` wide and `thickness` thick (mm) between a dielectric layer to
/// the copper above and one to the copper below (see the module's header for how an off-centre
/// strip between two dielectrics is worked out).
pub fn stripline(
    width: f64,
    thickness: f64,
    above: TrackDielectric,
    below: TrackDielectric,
) -> Result<Line, OutOfRange> {
    for d in [above, below] {
        if d.epsilon < 1.0 {
            return Err(OutOfRange(format!(
                "stripline needs a relative permittivity of 1 or more, not {}",
                d.epsilon
            )));
        }
    }
    // Each side's capacitance, per unit length and times c, is half that of a centred line of
    // twice its height: 1 / Z of that line in air, halved, times its εr.
    let z_above = wheeler_centred(width, thickness, 2.0 * above.thickness + thickness)?;
    let z_below = wheeler_centred(width, thickness, 2.0 * below.thickness + thickness)?;
    let air = (1.0 / z_above + 1.0 / z_below) / 2.0;
    let filled = (above.epsilon / z_above + below.epsilon / z_below) / 2.0;
    Ok(Line {
        impedance: 1.0 / (air * filled).sqrt(),
        epsilon_eff: filled / air,
    })
}

/// The width (mm) at which `line` has the impedance `target`, searched between `lo` and `hi`.
/// An impedance falls as a track widens, so it halves the interval; a figure outside the
/// formula's domain at an end narrows the search to the domain.
pub fn width_for(
    target: f64,
    line: impl Fn(f64) -> Result<Line, OutOfRange>,
    lo: f64,
    hi: f64,
) -> Result<f64, OutOfRange> {
    // Move each end inside the domain, by steps of 2 %, at most 400 (a factor of 3000).
    let inside = |mut w: f64, step: f64| -> Result<(f64, f64), OutOfRange> {
        let mut last = None;
        for _ in 0..400 {
            match line(w) {
                Ok(l) => return Ok((w, l.impedance)),
                Err(e) => last = Some(e),
            }
            w *= step;
        }
        Err(last.expect("400 tries"))
    };
    let (mut lo, z_lo) = inside(lo, 1.02)?;
    let (mut hi, z_hi) = inside(hi, 1.0 / 1.02)?;
    if target > z_lo || target < z_hi {
        return Err(OutOfRange(format!(
            "{target} ohm is outside what the formula gives in its domain, {z_hi:.1} to {z_lo:.1} ohm"
        )));
    }
    // 60 halvings of the logarithm leave the width exact to far below a nanometre.
    for _ in 0..60 {
        let mid = (lo * hi).sqrt();
        match line(mid) {
            Ok(l) if l.impedance > target => lo = mid,
            Ok(_) => hi = mid,
            Err(e) => return Err(e),
        }
    }
    Ok((lo * hi).sqrt())
}

/// A line's figures for a track of a width (mm).
type LineOfWidth = dyn Fn(f64) -> Result<Line, OutOfRange>;

/// The `fab-notes.txt` lines of a board's track impedance: for each layer a track may be on,
/// the width of a `target`-ohm track and the impedance of the router's `track_width` (mm). A
/// two-layer board records no build-up (ADR 0111), so it has none.
pub fn impedance_notes(stackup: &Stackup, track_width: f64, target: f64) -> Vec<String> {
    let Some(inner) = &stackup.inner else {
        return Vec::new();
    };
    let n = inner.dielectrics.len() + 1;
    if n < 3 || inner.layers.len() + 2 != n {
        return Vec::new();
    }
    let name = |i: usize| match i {
        0 => "F.Cu".to_string(),
        i if i == n - 1 => "B.Cu".to_string(),
        i => format!("In{i}.Cu"),
    };
    let is_plane =
        |i: usize| i > 0 && i < n - 1 && matches!(inner.layers[i - 1], InnerUse::Plane(_));
    let describe = |d: &DielectricLayer| {
        if !d.plies.is_empty() {
            let plies: Vec<String> = d
                .plies
                .iter()
                .map(|p| {
                    if p.core {
                        "core".to_string()
                    } else {
                        p.glass.clone()
                    }
                })
                .collect();
            return format!(
                "{} mm of {} plies ({}), er {:.2} in series",
                d.thickness,
                d.plies.len(),
                plies.join(", "),
                d.epsilon
            );
        }
        let what = if d.core {
            "core".to_string()
        } else {
            format!("{} prepreg", d.glass)
        };
        format!("{} mm of {what}, er {}", d.thickness, d.epsilon)
    };
    let dielectric = |i: usize| TrackDielectric {
        thickness: inner.dielectrics[i].thickness,
        epsilon: inner.dielectrics[i].epsilon,
    };
    let outer = stackup.copper_weight * OUNCE_OF_COPPER;
    let mut lines = vec![format!(
        "track impedance: from build-up {}, the width of a {target} ohm track and the impedance of the router's {track_width} mm track, by quasi-static closed forms (Hammerstad and Jensen 1980 for microstrip, Wheeler 1978 for stripline; netsmith_pcb::impedance). The solder mask is not counted and lowers an outer track's impedance; an inner track between two different dielectric layers is an approximation of unstated error. The board is not ordered with impedance control: for a controlled board, check JLCPCB's impedance calculator and order it",
        inner.build
    )];
    for i in 0..n {
        if is_plane(i) {
            continue;
        }
        let (what, refs, line): (String, Vec<usize>, Box<LineOfWidth>) = if i == 0 || i == n - 1 {
            let (r, d) = if i == 0 { (1, 0) } else { (n - 2, n - 2) };
            let below = dielectric(d);
            (
                format!(
                    "microstrip over {} ({})",
                    name(r),
                    describe(&inner.dielectrics[d])
                ),
                vec![r],
                Box::new(move |w| microstrip(w, outer, below)),
            )
        } else {
            let (above, below) = (dielectric(i - 1), dielectric(i));
            let t = inner.copper;
            (
                format!(
                    "stripline between {} ({}) and {} ({})",
                    name(i - 1),
                    describe(&inner.dielectrics[i - 1]),
                    name(i + 1),
                    describe(&inner.dielectrics[i])
                ),
                vec![i - 1, i + 1],
                Box::new(move |w| stripline(w, t, above, below)),
            )
        };
        let width = match width_for(target, &line, 0.001, 20.0) {
            Ok(w) => format!("a {target} ohm track is {:.3} mm wide", w),
            Err(e) => format!("no width for {target} ohm: {e}"),
        };
        let router = match line(track_width) {
            Ok(l) => format!(
                "the router's {track_width} mm track is {:.1} ohm",
                l.impedance
            ),
            Err(e) => format!("the router's {track_width} mm track has no figure: {e}"),
        };
        let unplaned: Vec<String> = refs
            .iter()
            .filter(|&&r| !is_plane(r))
            .map(|&r| name(r))
            .collect();
        let caveat = if unplaned.is_empty() {
            String::new()
        } else {
            format!(
                "; {} is not a plane on this board, so the figures hold only where a plane or a pour is there",
                unplaned.join(" and ")
            )
        };
        lines.push(format!("  {}: {what}: {width}; {router}{caveat}", name(i)));
    }
    lines
}
