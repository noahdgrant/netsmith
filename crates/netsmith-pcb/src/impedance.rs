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
//! - A microstrip under a solder mask (issue #1007): the mask is a slab of the mask's relative
//!   permittivity over the dielectric layer, and it raises the effective permittivity by the
//!   fraction of the field it takes the place of air in, times its permittivity less one (the
//!   air-replacement decomposition of P. Bahl and S. Stuchly, "Analysis of a Microstrip
//!   Covered with a Lossy Dielectric", IEEE MTT-28, 1980); the fraction is the filling factor
//!   of J. Svačina, "Analysis of multilayer microstrip lines by a conformal mapping method",
//!   IEEE MTT-40, 1992, pp. 769-772, as improved by C. Wan and A. Hoorfar, "Improved design
//!   equations for multilayer microstrip lines", IEEE Microwave and Guided Wave Letters 10,
//!   no. 6, 2000, pp. 223-224, which is taken between the dielectric layer alone and the
//!   dielectric layer under the mask. The impedance falls with the root of the ratio of the
//!   effective permittivity without the mask to the one with it. (The letter's narrow-strip
//!   equation prints `ln(0.125 w)`; the form here, `ln(8 / w)`, is the one that reproduces
//!   its table, as KiCad's `pcb_calculator` has it.) Neither paper gives an error for this
//!   use; JLCPCB's blog says a mask alone lowers a track by about 1 to 3 ohm
//!   (<https://jlcpcb.com/blog/pcb-impedance-testing>), which the test of this crate holds the
//!   figure to. The mask's thickness is the calculator's "coating above substrate" (the mask
//!   over the dielectric, beside the track); the thinner mask over the track's own copper and
//!   the mask between two tracks are not told apart.
//! - Two tracks side by side, an edge-coupled pair, worked out as the even and the odd mode,
//!   and the differential impedance as twice the odd mode's. On an outer layer: M. Kirschning
//!   and R. H. Jansen, "Accurate wide-range design equations for the frequency-dependent
//!   characteristic of parallel coupled microstrip lines", IEEE MTT-32, no. 1, 1984, pp. 83-90
//!   (with its 1985 correction) for the static modes, as the Qucs technical documentation
//!   gives them (<https://qucs.sourceforge.net/tech/node77.html>: static impedance better than
//!   0.6 %, for 0.1 <= w/h <= 10, 0.1 <= s/h <= 10, 1 <= εr <= 18). The strip thickness is
//!   taken as R. H. Jansen, "High-speed computation of single and coupled microstrip
//!   parameters including dispersion, high-order modes, loss and finite strip thickness",
//!   IEEE MTT-26, no. 2, 1978, pp. 75-82, couples it: a width for the even mode and a wider
//!   one for the odd mode, from the single track's width correction (the one `microstrip`
//!   uses, for a strip in this dielectric) and the space between the strips. That departs
//!   from Kirschning and Jansen, who also take the thickness off the filling factor and start
//!   from a single track of no thickness: a thick pair far apart came out 5 to 10 % off a
//!   thick single track, which it must become, so the thickness is counted once here and the
//!   single track each mode starts from has the width that mode sees. No source states the
//!   error of this for a thick pair; JLCPCB's calculator is the check (the tests). On an
//!   inner layer: S. B. Cohn, "Shielded Coupled-Strip Transmission
//!   Line", IRE Trans. MTT-3, no. 5, 1955, pp. 29-38: the exact even and odd impedances of two
//!   strips of no thickness between planes, from elliptic integrals, and his correction for
//!   the strips' thickness from the fringing capacitance (his equations 18, 20 and 22, in a
//!   medium of εr 1, where they are dimensionally consistent). Cohn gives the pair between
//!   planes with one dielectric; an off-centre pair between two different dielectric layers
//!   is split at the plane of the strips as a single track is, which has the same
//!   approximation and the same unstated error.
//!
//! Outside a formula's stated domain the figure is refused ([`OutOfRange`]), never
//! extrapolated.

use std::f64::consts::{E, PI};

use netsmith_core::OUNCE_OF_COPPER;
use netsmith_core::{DielectricLayer, InnerUse, SolderMask, Stackup};

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

/// Wan and Hoorfar's filling factor of the second layer of a three-layer microstrip, `u` the
/// strip's width over the first layer's thickness and `hbar` the top of the second layer over
/// the first layer's thickness (1 is no second layer).
fn wan_hoorfar_q2(u: f64, hbar: f64) -> Result<f64, OutOfRange> {
    let bad = || {
        OutOfRange(format!(
            "the solder mask's filling factor has no figure for a track {u:.3} times the dielectric's thickness wide"
        ))
    };
    let q2 = if u >= 1.0 {
        let wbar = u + 2.0 / PI * (17.08 * (0.5 * u + 0.92)).ln();
        let denom = wbar * PI - 4.0;
        if denom <= 0.0 {
            return Err(bad());
        }
        let vbar = 2.0 / PI * (2.0 * PI / denom * (hbar - 1.0)).atan();
        let half = 0.5 * PI * vbar;
        let q1 = 1.0 - (wbar * PI - 1.0).ln() / (2.0 * wbar);
        let inner = 2.0 * wbar * half.cos() / (2.0 * hbar - 1.0 + vbar) + half.sin();
        if inner <= 0.0 {
            return Err(bad());
        }
        1.0 - q1 - (1.0 - vbar) / (2.0 * wbar) * inner.ln()
    } else {
        let log = (8.0 / u).ln();
        let q1 = 0.5 + 0.9 / (PI * log);
        let bj = (hbar + 1.0) / (hbar + 0.25 * u - 1.0);
        if bj <= 0.0 {
            return Err(bad());
        }
        let arg = bj.sqrt() * (hbar - 1.0 + 0.125 * u) / hbar;
        1.0 - q1 - bj.ln() * arg.clamp(-1.0, 1.0).acos() / (4.0 * log)
    };
    if q2.is_finite() {
        Ok(q2.max(0.0))
    } else {
        Err(bad())
    }
}

/// What a solder mask changes of a microstrip's effective permittivity, `eps_eff` without it, `u`
/// the width over the dielectric's thickness `h`: the new effective permittivity.
fn coated_epsilon_eff(eps_eff: f64, u: f64, h: f64, mask: SolderMask) -> Result<f64, OutOfRange> {
    if mask.thickness <= 0.0 {
        return Ok(eps_eff);
    }
    if mask.epsilon <= 1.0 {
        return Err(OutOfRange(format!(
            "a solder mask needs a relative permittivity above 1, not {}",
            mask.epsilon
        )));
    }
    let dq = (wan_hoorfar_q2(u, 1.0 + mask.thickness / h)? - wan_hoorfar_q2(u, 1.0)?).max(0.0);
    Ok(eps_eff + dq * (mask.epsilon - 1.0))
}

/// A microstrip under a solder mask: [`microstrip`], with the mask's effect on the effective
/// permittivity and so on the impedance (see the module's header).
pub fn coated_microstrip(
    width: f64,
    thickness: f64,
    below: TrackDielectric,
    mask: SolderMask,
) -> Result<Line, OutOfRange> {
    let bare = microstrip(width, thickness, below)?;
    let eps = coated_epsilon_eff(
        bare.epsilon_eff,
        width / below.thickness,
        below.thickness,
        mask,
    )?;
    Ok(Line {
        impedance: bare.impedance * (bare.epsilon_eff / eps).sqrt(),
        epsilon_eff: eps,
    })
}

/// The two modes of a pair of tracks side by side, an edge-coupled pair.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pair {
    /// With both tracks at the same voltage.
    pub even: Line,
    /// With the tracks at opposite voltages.
    pub odd: Line,
}

impl Pair {
    /// The impedance between the two tracks when they carry opposite voltages: twice the odd
    /// mode's, in ohms.
    pub fn differential(&self) -> f64 {
        2.0 * self.odd.impedance
    }
}

/// The complete elliptic integral of the first kind of modulus `k`, by the arithmetic-geometric
/// mean: K(k) = π / (2 AGM(1, sqrt(1 - k²))).
fn elliptic_k(k: f64) -> f64 {
    let (mut a, mut b) = (1.0_f64, (1.0 - k * k).sqrt());
    for _ in 0..40 {
        (a, b) = ((a + b) / 2.0, (a * b).sqrt());
    }
    PI / (2.0 * a)
}

/// Kirschning and Jansen's static even and odd modes of two microstrips `width` wide and
/// `thickness` thick, `spacing` apart, on a dielectric layer under open air (see the module's
/// header), with no solder mask.
pub fn coupled_microstrip(
    width: f64,
    spacing: f64,
    thickness: f64,
    below: TrackDielectric,
) -> Result<Pair, OutOfRange> {
    let (h, er) = (below.thickness, below.epsilon);
    let (u, g) = (width / h, spacing / h);
    if !(1.0..=18.0).contains(&er) {
        return Err(OutOfRange(format!(
            "coupled microstrip needs a relative permittivity from 1 to 18, not {er}"
        )));
    }
    if !(0.1..=10.0).contains(&u) || !(0.1..=10.0).contains(&g) {
        return Err(OutOfRange(format!(
            "coupled microstrip needs a width and a spacing from 0.1 to 10 times the dielectric's thickness, not {u:.3} and {g:.3}"
        )));
    }
    if thickness < 0.0 || thickness >= width / 2.0 {
        return Err(OutOfRange(format!(
            "coupled microstrip's thickness correction needs copper thinner than half the track, not {thickness} mm on a {width} mm track"
        )));
    }
    let t_h = thickness / h;
    // The width each mode sees once the strip has a thickness: the single microstrip's
    // correction for a strip in this dielectric (Hammerstad and Jensen's, which `microstrip`
    // uses) as Jansen 1978 couples it to the other strip, `du * (1 - 0.5 exp(-0.69 du / dt))`
    // for the even mode and that plus `dt` for the odd mode.
    let (du_e, du_o) = if t_h > 0.0 {
        let coth = 1.0 / (6.517 * u).sqrt().tanh();
        let du1 = t_h / PI * (1.0 + 4.0 * E / (t_h * coth * coth)).ln();
        let du = du1 * (1.0 + 1.0 / (er - 1.0).sqrt().cosh()) / 2.0;
        let dt = t_h / (g * er);
        let due = du * (1.0 - 0.5 * (-0.69 * du / dt).exp());
        (due, due + dt)
    } else {
        (0.0, 0.0)
    };
    let (ue, uo) = (u + du_e, u + du_o);
    // The zero-thickness single line each mode's equations start from. Kirschning and Jansen
    // give it for the strip's own width, and take the thickness into account in the filling
    // factor besides (a term of `t / sqrt(u)`); that made a thick pair far apart 5 to 10 %
    // different from a thick single track, which the pair must become. Here the thickness is
    // counted once, in the width each mode sees (Jansen's `ue` and `uo`), and the single line
    // is the one of that width, which makes a pair far apart a single track to within 1 % (see
    // the tests). This departs from the papers, and no source states the error of what it
    // gives for a thick pair.
    let single = |u: f64| {
        let e = hj_epsilon_eff(u, er);
        (e, hj_z01(u) / e.sqrt())
    };
    let ((ers_e, z0s_e), (ers_o, z0s_o)) = (single(ue), single(uo));
    let mean = 0.5 * (er + 1.0);

    // Even mode.
    let v = ue * (20.0 + g * g) / (10.0 + g * g) + g * (-g).exp();
    let (v3, v4) = (v.powi(3), v.powi(4));
    let a_e = 1.0
        + ((v4 + v * v / 2704.0) / (v4 + 0.432)).ln() / 49.0
        + (1.0 + v3 / 5929.741).ln() / 18.7;
    let b_e = 0.564 * ((er - 0.9) / (er + 3.0)).powf(0.053);
    let q_e = (1.0 + 10.0 / v).powf(-a_e * b_e);
    let er_e = mean + 0.5 * (er - 1.0) * q_e;
    let q1 = 0.8695 * ue.powf(0.194);
    let q2 = 1.0 + 0.7519 * g + 0.189 * g.powf(2.31);
    let q3 = 0.1975
        + (16.6 + (8.4 / g).powi(6)).powf(-0.387)
        + (g.powi(10) / (1.0 + (g / 3.4).powi(10))).ln() / 241.0;
    let q4 = 2.0 * q1 / (q2 * ((-g).exp() * ue.powf(q3) + (2.0 - (-g).exp()) * ue.powf(-q3)));
    let z_e = z0s_e * (ers_e / er_e).sqrt() / (1.0 - ers_e.sqrt() * q4 * z0s_e / ETA0);

    // Odd mode.
    let b_o = 0.747 * er / (0.15 + er);
    let c_o = b_o - (b_o - 0.207) * (-0.414 * uo).exp();
    let d_o = 0.593 + 0.694 * (-0.562 * uo).exp();
    let q_o = (-c_o * g.powf(d_o)).exp();
    let a_o = 0.7287 * (ers_o - mean) * (1.0 - (-0.179 * uo).exp());
    let er_o = (mean + a_o - ers_o) * q_o + ers_o;
    let q5 = 1.794 + 1.14 * (1.0 + 0.638 / (g + 0.517 * g.powf(2.43))).ln();
    let q6 = 0.2305
        + (g.powi(10) / (1.0 + (g / 5.8).powi(10))).ln() / 281.3
        + (1.0 + 0.598 * g.powf(1.154)).ln() / 5.1;
    let q7 = (10.0 + 190.0 * g * g) / (1.0 + 82.3 * g.powi(3));
    let q8 = (-6.5 - 0.95 * g.ln() - (g / 0.15).powi(5)).exp();
    let q9 = q7.ln() * (q8 + 1.0 / 16.5);
    let q10 = (q2 * q4 - q5 * (uo.ln() * q6 * uo.powf(-q9)).exp()) / q2;
    let z_o = z0s_o * (ers_o / er_o).sqrt() / (1.0 - ers_o.sqrt() * q10 * z0s_o / ETA0);
    if ![er_e, er_o, z_e, z_o]
        .iter()
        .all(|x| x.is_finite() && *x > 0.0)
    {
        return Err(OutOfRange(
            "coupled microstrip has no figure for this geometry".to_string(),
        ));
    }
    Ok(Pair {
        even: Line {
            impedance: z_e,
            epsilon_eff: er_e,
        },
        odd: Line {
            impedance: z_o,
            epsilon_eff: er_o,
        },
    })
}

/// [`coupled_microstrip`] under a solder mask: the mask's effect on each mode's effective
/// permittivity and so on its impedance, as for [`coated_microstrip`] (the filling factor is a
/// single strip's, taken for each mode: no source gives its error for a pair).
pub fn coated_coupled_microstrip(
    width: f64,
    spacing: f64,
    thickness: f64,
    below: TrackDielectric,
    mask: SolderMask,
) -> Result<Pair, OutOfRange> {
    let bare = coupled_microstrip(width, spacing, thickness, below)?;
    let u = width / below.thickness;
    let coat = |m: Line| -> Result<Line, OutOfRange> {
        let eps = coated_epsilon_eff(m.epsilon_eff, u, below.thickness, mask)?;
        Ok(Line {
            impedance: m.impedance * (m.epsilon_eff / eps).sqrt(),
            epsilon_eff: eps,
        })
    };
    Ok(Pair {
        even: coat(bare.even)?,
        odd: coat(bare.odd)?,
    })
}

/// Cohn's even and odd impedances of two strips of no thickness, `w` wide and `s` apart, centred
/// between planes `b` apart, in a medium of εr 1.
fn cohn_coupled_thin(w: f64, s: f64, b: f64) -> (f64, f64) {
    let x = PI * w / (2.0 * b);
    let y = PI * (w + s) / (2.0 * b);
    let (ke, ko) = (x.tanh() * y.tanh(), x.tanh() / y.tanh());
    let z = |k: f64| ETA0 / 4.0 * elliptic_k((1.0 - k * k).sqrt()) / elliptic_k(k);
    (z(ke), z(ko))
}

/// Cohn's even and odd impedances of two strips `w` wide, `t` thick and `s` apart, centred
/// between planes `b` apart, in a medium of εr 1 (his equations 18, 20 and 22).
fn cohn_coupled(w: f64, s: f64, t: f64, b: f64) -> Result<(f64, f64), OutOfRange> {
    let (ze0, zo0) = cohn_coupled_thin(w, s, b);
    if t == 0.0 {
        return Ok((ze0, zo0));
    }
    let z_thick = wheeler_centred(w, t, b)?;
    let x = PI * w / (2.0 * b);
    let z_thin = ETA0 / 4.0 * elliptic_k(1.0 / x.cosh()) / elliptic_k(x.tanh());
    // The fringing capacitance of one edge, over ε, with thickness and without (Cohn 1954,
    // equations 2 and 13 of the 1955 paper).
    let r = 1.0 / (1.0 - t / b);
    let cf_t = ((2.0 * r) * (r + 1.0).ln() - (r - 1.0) * (r * r - 1.0).ln()) / PI;
    let cf_0 = 2.0 * 2.0_f64.ln() / PI;
    let ratio = cf_t / cf_0;
    let even = 1.0 / (1.0 / z_thick - ratio * (1.0 / z_thin - 1.0 / ze0));
    let odd = if s / t >= 5.0 {
        1.0 / (1.0 / z_thick + ratio * (1.0 / zo0 - 1.0 / z_thin))
    } else {
        1.0 / (1.0 / zo0 + (1.0 / z_thick - 1.0 / z_thin) - 2.0 / ETA0 * (cf_t - cf_0)
            + 2.0 * t / (ETA0 * s))
    };
    if even.is_finite() && odd.is_finite() && even > 0.0 && odd > 0.0 {
        Ok((even, odd))
    } else {
        Err(OutOfRange(
            "coupled stripline has no figure for this geometry".to_string(),
        ))
    }
}

/// Two strips `width` wide and `thickness` thick, `spacing` apart, between a dielectric layer to
/// the copper above and one to the copper below: Cohn's coupled stripline, split at the plane of
/// the strips as [`stripline`] is (see the module's header).
pub fn coupled_stripline(
    width: f64,
    spacing: f64,
    thickness: f64,
    above: TrackDielectric,
    below: TrackDielectric,
) -> Result<Pair, OutOfRange> {
    for d in [above, below] {
        if d.epsilon < 1.0 {
            return Err(OutOfRange(format!(
                "stripline needs a relative permittivity of 1 or more, not {}",
                d.epsilon
            )));
        }
    }
    if spacing <= 0.0 {
        return Err(OutOfRange(format!(
            "a pair needs a spacing above 0, not {spacing} mm"
        )));
    }
    let (ea, oa) = cohn_coupled(width, spacing, thickness, 2.0 * above.thickness + thickness)?;
    let (eb, ob) = cohn_coupled(width, spacing, thickness, 2.0 * below.thickness + thickness)?;
    let mode = |za: f64, zb: f64| {
        let air = (1.0 / za + 1.0 / zb) / 2.0;
        let filled = (above.epsilon / za + below.epsilon / zb) / 2.0;
        Line {
            impedance: 1.0 / (air * filled).sqrt(),
            epsilon_eff: filled / air,
        }
    };
    Ok(Pair {
        even: mode(ea, eb),
        odd: mode(oa, ob),
    })
}

/// The width (mm) at which a track has the impedance `target` (ohm), searched between `lo` and
/// `hi`, `impedance` being its impedance at a width.
/// An impedance falls as a track widens, so it halves the interval; a figure outside the
/// formula's domain at an end narrows the search to the domain.
pub fn width_for(
    target: f64,
    impedance: impl Fn(f64) -> Result<f64, OutOfRange>,
    lo: f64,
    hi: f64,
) -> Result<f64, OutOfRange> {
    // Move each end inside the domain, by steps of 2 %, at most 400 (a factor of 3000).
    let inside = |mut w: f64, step: f64| -> Result<(f64, f64), OutOfRange> {
        let mut last = None;
        for _ in 0..400 {
            match impedance(w) {
                Ok(z) => return Ok((w, z)),
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
        match impedance(mid) {
            Ok(z) if z > target => lo = mid,
            Ok(_) => hi = mid,
            Err(e) => return Err(e),
        }
    }
    Ok((lo * hi).sqrt())
}

/// One copper layer a track may be on, from a build-up.
struct TrackLayer {
    name: String,
    /// What the track is on this layer, with its reference layers and their dielectric layers.
    what: String,
    /// The reference layers that are not planes on this board.
    unplaned: Vec<String>,
    /// A track's thickness, in mm.
    copper: f64,
    /// The dielectric layer on an outer layer, below the track; above and below an inner one.
    outer: Option<TrackDielectric>,
    inner: Option<(TrackDielectric, TrackDielectric)>,
    /// The solder mask over an outer track, if the process states it.
    mask: Option<SolderMask>,
}

impl TrackLayer {
    /// A single track `width` wide.
    fn single(&self, width: f64) -> Result<Line, OutOfRange> {
        match (self.outer, self.inner, self.mask) {
            (Some(below), _, Some(mask)) => coated_microstrip(width, self.copper, below, mask),
            (Some(below), _, None) => microstrip(width, self.copper, below),
            (_, Some((above, below)), _) => stripline(width, self.copper, above, below),
            _ => unreachable!("a layer is an outer or an inner one"),
        }
    }

    /// An edge-coupled pair of tracks `width` wide, `spacing` apart.
    fn pair(&self, width: f64, spacing: f64) -> Result<Pair, OutOfRange> {
        match (self.outer, self.inner, self.mask) {
            (Some(below), _, Some(mask)) => {
                coated_coupled_microstrip(width, spacing, self.copper, below, mask)
            }
            (Some(below), _, None) => coupled_microstrip(width, spacing, self.copper, below),
            (_, Some((above, below)), _) => {
                coupled_stripline(width, spacing, self.copper, above, below)
            }
            _ => unreachable!("a layer is an outer or an inner one"),
        }
    }
}

/// The layers a track may be on (the outer layers and the signal layers, not the planes), from a
/// stackup with a build-up.
fn track_layers(stackup: &Stackup) -> Option<Vec<TrackLayer>> {
    let inner = stackup.inner.as_ref()?;
    let n = inner.dielectrics.len() + 1;
    if n < 3 || inner.layers.len() + 2 != n {
        return None;
    }
    let name = |i: usize| match i {
        0 => "F.Cu".to_string(),
        i if i == n - 1 => "B.Cu".to_string(),
        i => format!("In{i}.Cu"),
    };
    let planes: Vec<bool> = (0..n)
        .map(|i| i > 0 && i < n - 1 && matches!(inner.layers[i - 1], InnerUse::Plane(_)))
        .collect();
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
    let not_planes = |refs: &[usize]| -> Vec<String> {
        refs.iter()
            .filter(|&&r| !planes[r])
            .map(|&r| name(r))
            .collect()
    };
    let outer = inner
        .outer_copper
        .unwrap_or(stackup.copper_weight * OUNCE_OF_COPPER);
    let mut layers = Vec::new();
    for i in 0..n {
        if planes[i] {
            continue;
        }
        layers.push(if i == 0 || i == n - 1 {
            let (r, d) = if i == 0 { (1, 0) } else { (n - 2, n - 2) };
            TrackLayer {
                name: name(i),
                what: format!(
                    "microstrip over {} ({})",
                    name(r),
                    describe(&inner.dielectrics[d])
                ),
                unplaned: not_planes(&[r]),
                copper: outer,
                outer: Some(dielectric(d)),
                inner: None,
                mask: inner.mask,
            }
        } else {
            TrackLayer {
                name: name(i),
                what: format!(
                    "stripline between {} ({}) and {} ({})",
                    name(i - 1),
                    describe(&inner.dielectrics[i - 1]),
                    name(i + 1),
                    describe(&inner.dielectrics[i])
                ),
                unplaned: not_planes(&[i - 1, i + 1]),
                copper: inner.copper,
                outer: None,
                inner: Some((dielectric(i - 1), dielectric(i))),
                mask: None,
            }
        });
    }
    Some(layers)
}

/// The caveat of a track layer whose reference layers are not all planes.
fn unplaned(layer: &TrackLayer) -> String {
    if layer.unplaned.is_empty() {
        String::new()
    } else {
        format!(
            "; {} is not a plane on this board, so the figures hold only where a plane or a pour is there",
            layer.unplaned.join(" and ")
        )
    }
}

/// How far from a target JLCPCB's impedance test passes a board, as a fraction: a 50 ohm target
/// passes between 45 and 55 ohm and a 100 ohm differential pair between 90 and 110 ohm
/// (<https://jlcpcb.com/blog/pcb-impedance-testing>, read 2026-11-16).
pub const TEST_TOLERANCE: f64 = 0.10;

/// The `fab-notes.txt` lines of a board's track impedance: for each layer a track may be on,
/// the width of a `target`-ohm track and the impedance of each track width in use on it
/// (`in_use` gives a layer's widths in mm with how many tracks have each, issue #1589); a layer
/// with no track gets the impedance of the router's default `track_width` (mm). A two-layer board
/// records no build-up (ADR 0111), so it has none.
pub fn impedance_notes(
    stackup: &Stackup,
    in_use: &dyn Fn(&str) -> Vec<(f64, usize)>,
    track_width: f64,
    target: f64,
) -> Vec<String> {
    let Some(layers) = track_layers(stackup) else {
        return Vec::new();
    };
    let inner = stackup.inner.as_ref().expect("a build-up");
    let mask = match inner.mask {
        Some(m) => format!(
            "The solder mask over an outer track is counted ({} mm of er {}, JLCPCB's calculator parameters), as is the copper thickness of that calculator; an inner track between two different dielectric layers is an approximation of unstated error",
            m.thickness, m.epsilon
        ),
        None => "The solder mask is not counted and lowers an outer track's impedance; an inner track between two different dielectric layers is an approximation of unstated error".to_string(),
    };
    let mut lines = vec![format!(
        "track impedance: from build-up {}, the width of a {target} ohm track and the impedance of each track width in use on the layer (of the router's default {track_width} mm track where it has no track), by quasi-static closed forms (Hammerstad and Jensen 1980 for microstrip, Wheeler 1978 for stripline; netsmith_pcb::impedance). {mask}. The board is not ordered with impedance control: for a controlled board, check JLCPCB's impedance calculator and order it",
        inner.build
    )];
    for layer in &layers {
        let width = match width_for(
            target,
            |w| layer.single(w).map(|l| l.impedance),
            0.001,
            20.0,
        ) {
            Ok(w) => format!("a {target} ohm track is {:.3} mm wide", w),
            Err(e) => format!("no width for {target} ohm: {e}"),
        };
        let figure = |w: f64| match layer.single(w) {
            Ok(l) => format!("{:.1} ohm", l.impedance),
            Err(e) => format!("no figure: {e}"),
        };
        let used = in_use(&layer.name);
        let router = if used.is_empty() {
            format!(
                "no tracks on it; the router's default {track_width} mm track would be {}",
                figure(track_width)
            )
        } else {
            let each: Vec<String> = used
                .iter()
                .map(|&(w, n)| {
                    let tracks = if n == 1 {
                        "1 track"
                    } else {
                        &format!("{n} tracks")
                    };
                    format!("the {w} mm track ({tracks}) is {}", figure(w))
                })
                .collect();
            each.join("; ")
        };
        lines.push(format!(
            "  {}: {}: {width}; {router}{}",
            layer.name,
            layer.what,
            unplaned(layer)
        ));
    }
    lines
}

use netsmith_core::ImpedanceTarget;

/// The impedance of a track of `width` mm on the layer named `layer` (`F.Cu`, `In1.Cu`...) of a
/// board with a build-up: a single track's, or, given the `spacing` between the two tracks (mm,
/// edge to edge), the differential impedance of an edge-coupled pair. Refused where the layer
/// is not one a track may be on (a plane, or a board with no build-up) or the geometry is
/// outside a formula's domain.
pub fn impedance_at(
    stackup: &Stackup,
    layer: &str,
    width: f64,
    spacing: Option<f64>,
) -> Result<f64, OutOfRange> {
    let layers = track_layers(stackup).ok_or_else(|| {
        OutOfRange("the board records no build-up, so its tracks have no impedance".to_string())
    })?;
    let layer = layers.iter().find(|l| l.name == layer).ok_or_else(|| {
        OutOfRange(format!(
            "`{layer}` is not a layer a track may be on in this build-up"
        ))
    })?;
    match spacing {
        None => layer.single(width).map(|l| l.impedance),
        Some(s) => layer.pair(width, s).map(|p| p.differential()),
    }
}

/// The width a target asks of a track on one layer (issue #1025): a single track's that gives
/// its impedance, or the width of each track of a pair that gives its differential impedance at
/// its spacing. Refused outside a formula's domain.
fn solve(layer: &TrackLayer, target: &ImpedanceTarget) -> Result<f64, OutOfRange> {
    match target.spacing {
        None => width_for(
            target.ohms,
            |w| layer.single(w).map(|l| l.impedance),
            0.001,
            20.0,
        ),
        Some(s) => width_for(
            target.ohms,
            |w| layer.pair(w, s).map(|p| p.differential()),
            0.001,
            20.0,
        ),
    }
}

/// The width the router draws the tracks of a net with an impedance target at on one copper
/// layer (issue #1025), as [`target_widths`] solves it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TargetWidth {
    pub net: String,
    /// The layer, by its KiCad name (`F.Cu`, `In2.Cu`, `B.Cu`).
    pub layer: String,
    /// The width in mm: the one that gives the target, to the nearest 0.01 mm a width is
    /// written in.
    pub width: f64,
}

/// What an impedance target asks of the tracks on one layer: the width that gives it, or why
/// none does.
#[derive(Debug, Clone, PartialEq)]
pub struct TargetLayer {
    /// The layer, by its KiCad name.
    pub layer: String,
    /// The exact width that gives the target, in mm, or why no width does.
    pub width: Result<f64, OutOfRange>,
}

/// For each target, what it asks on each layer a track may be on (the outer layers and the
/// signal layers, not the planes); `None` for a board with no build-up (a two-layer board, ADR
/// 0111), where no width gives an impedance.
pub fn target_layers(
    stackup: &Stackup,
    targets: &[ImpedanceTarget],
) -> Option<Vec<Vec<TargetLayer>>> {
    let layers = track_layers(stackup)?;
    Some(
        targets
            .iter()
            .map(|t| {
                layers
                    .iter()
                    .map(|l| TargetLayer {
                        layer: l.name.clone(),
                        width: solve(l, t),
                    })
                    .collect()
            })
            .collect(),
    )
}

/// The widths the router draws the nets of the `targets` at (issue #1025): for each net of a
/// target and each layer a track may be on, the width that gives the target, rounded to the
/// nearest 0.01 mm. The first target that names a net, in the order of their paths, sizes it.
/// A layer where no width gives the target has no entry, so the net is drawn there at its own
/// width; [`target_layers`] says why, and the board's `impedance_findings` say so.
pub fn target_widths(stackup: &Stackup, targets: &[ImpedanceTarget]) -> Vec<TargetWidth> {
    let Some(solved) = target_layers(stackup, targets) else {
        return Vec::new();
    };
    let mut out: Vec<TargetWidth> = Vec::new();
    for (t, layers) in targets.iter().zip(solved) {
        for net in &t.nets {
            if out.iter().any(|w| &w.net == net) {
                continue;
            }
            for l in &layers {
                if let Ok(w) = l.width {
                    out.push(TargetWidth {
                        net: net.clone(),
                        layer: l.layer.clone(),
                        width: ((w * 100.0).round() / 100.0).max(0.01),
                    });
                }
            }
        }
    }
    out
}

/// `ok` or the figure's distance from a target, for a line: within JLCPCB's test tolerance or
/// not.
pub(crate) fn against(target: f64, got: f64) -> String {
    let off = (got - target) / target;
    if off.abs() <= TEST_TOLERANCE {
        format!(
            "{:+.0} %, within JLCPCB's test tolerance of 10 %",
            off * 100.0
        )
    } else {
        format!(
            "{:+.0} %, outside JLCPCB's test tolerance of 10 %",
            off * 100.0
        )
    }
}

/// The `fab-notes.txt` lines for the impedance a design states for some nets: for each target
/// and each layer a track may be on, the width that gives it and the impedance of the tracks
/// the router draws (`width_on(net, layer name)`, mm). A two-layer board records no build-up,
/// so it has none.
pub fn target_notes(
    stackup: &Stackup,
    targets: &[ImpedanceTarget],
    width_on: &dyn Fn(&str, &str) -> f64,
) -> Vec<String> {
    let Some(layers) = track_layers(stackup) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    for t in targets {
        let nets: Vec<String> = t.nets.iter().map(|n| format!("`{n}`")).collect();
        let header = match t.spacing {
            None => format!(
                "track impedance target: net {} ({}), {} ohm single-ended",
                nets.join(""),
                t.path,
                t.ohms
            ),
            Some(s) => format!(
                "track impedance target: nets {} ({}), {} ohm differential, the tracks {s} mm apart (edge to edge)",
                nets.join(" and "),
                t.path,
                t.ohms
            ),
        };
        lines.push(header);
        for layer in &layers {
            let drawn = width_on(&t.nets[0], &layer.name);
            let width = solve(layer, t);
            let router = match t.spacing {
                None => layer.single(drawn).map(|l| l.impedance),
                Some(s) => layer.pair(drawn, s).map(|p| p.differential()),
            };
            let width = match width {
                Ok(w) => format!("{:.3} mm wide tracks give {} ohm", w, t.ohms),
                Err(e) => format!("no width for {} ohm: {e}", t.ohms),
            };
            let router = match router {
                Ok(z) => format!(
                    "the {drawn} mm tracks the router draws give {z:.1} ohm ({})",
                    against(t.ohms, z)
                ),
                Err(e) => format!("the {drawn} mm tracks the router draws have no figure: {e}"),
            };
            lines.push(format!(
                "  {}: {}: {width}; {router}{}",
                layer.name,
                layer.what,
                unplaned(layer)
            ));
        }
    }
    lines
}

/// The conductivity of the copper of a track, in S/m: annealed copper, 58.0 MS/m (IEC 60028,
/// the International Annealed Copper Standard). A fabricator's electrodeposited copper and its
/// roughness are not counted: both raise the loss above the figure (issue #1405).
pub const COPPER_CONDUCTIVITY: f64 = 58.0e6;

/// The permeability of free space, in H/m (CODATA 2018, 1.256 637 062 12e-6).
const MU0: f64 = 1.256_637_062_12e-6;

/// The speed of light in vacuum, in m/s.
const C0: f64 = 299_792_458.0;

/// The loss of a track's copper and of its dielectric, each in dB per mm, at a frequency
/// ([`loss_per_mm`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrackLoss {
    /// The copper's, by the skin effect.
    pub conductor: f64,
    /// The dielectric's, by its loss tangent.
    pub dielectric: f64,
}

impl TrackLoss {
    /// The two together, in dB per mm.
    pub fn total(&self) -> f64 {
        self.conductor + self.dielectric
    }
}

/// The skin depth of copper at `frequency` (Hz), in mm.
pub fn skin_depth(frequency: f64) -> f64 {
    1e3 / (PI * frequency * MU0 * COPPER_CONDUCTIVITY).sqrt()
}

/// The loss per mm of a single track of `width` mm on the layer named `layer` of a board with a
/// build-up, at `frequency` Hz, with the build-up's loss tangent (issue #1405). Low-loss
/// closed forms, from D. M. Pozar, *Microwave Engineering*, 4th ed., Wiley 2012, section 3.7
/// and 3.8, with the line's impedance and effective permittivity from this module:
///
/// - On an outer layer, a microstrip: the copper loses `8.686 Rs / (Z0 W)` dB per length (Pozar
///   3.199, the strip and its reference plane together) and the dielectric `27.3 (er / sqrt(eeff))
///   ((eeff - 1) / (er - 1)) tan(d) / lambda0` (Pozar 3.198, there in nepers). The line is the
///   uncovered one: the solder mask's own loss is not counted.
/// - On an inner layer, a stripline: the copper loses Pozar's 3.199 for a stripline (from
///   Wheeler's incremental inductance rule, the two branches for `sqrt(er) Z0` below and above
///   120 ohm), with `b` the space between the planes, and the dielectric `27.3 sqrt(eeff) tan(d)
///   / lambda0` (Pozar 3.30). Pozar gives the strip centred in one dielectric; an off-centre
///   strip between two dielectric layers is taken with its effective permittivity, an
///   approximation of unstated error, as its impedance is.
///
/// `Rs = sqrt(pi f mu0 / sigma)` is the copper's surface resistance, which holds where the skin
/// depth is below the copper's thickness: a frequency where it is not is refused (a few MHz
/// for 1 oz copper, where any track's loss is far below a budget). Refused where the layer is
/// not one a track may be on, the build-up records no loss tangent, or the geometry is outside
/// a formula's domain.
pub fn loss_per_mm(
    stackup: &Stackup,
    layer: &str,
    width: f64,
    frequency: f64,
) -> Result<TrackLoss, OutOfRange> {
    let layers = track_layers(stackup).ok_or_else(|| {
        OutOfRange(
            "the board records no build-up (a two-layer board), so its tracks have no loss figure"
                .to_string(),
        )
    })?;
    let inner = stackup.inner.as_ref().expect("a build-up");
    let tan_d = inner.loss_tangent.ok_or_else(|| {
        OutOfRange(format!(
            "build-up {} records no loss tangent: state the laminate's, `.process.stackup.loss_tangent &= 0.02`, from its datasheet",
            inner.build
        ))
    })?;
    let layer = layers.iter().find(|l| l.name == layer).ok_or_else(|| {
        OutOfRange(format!(
            "`{layer}` is not a layer a track may be on in this build-up"
        ))
    })?;
    if frequency.is_nan() || frequency <= 0.0 {
        return Err(OutOfRange(format!(
            "a loss is worked out at a frequency above 0, not {frequency} Hz"
        )));
    }
    let delta = skin_depth(frequency);
    if delta >= layer.copper {
        return Err(OutOfRange(format!(
            "at {} the skin depth of copper is {delta:.4} mm, not below the track's {} mm, and the skin-effect formula needs it below",
            hertz(frequency),
            layer.copper
        )));
    }
    match (layer.outer, layer.inner) {
        (Some(below), _) => microstrip_loss(width, layer.copper, below, frequency, tan_d),
        (_, Some((above, below))) => {
            stripline_loss(width, layer.copper, above, below, frequency, tan_d)
        }
        _ => unreachable!("a layer is an outer or an inner one"),
    }
}

/// The surface resistance of copper at `frequency` (Hz), in ohms: `sqrt(pi f mu0 / sigma)`.
pub fn surface_resistance(frequency: f64) -> f64 {
    (PI * frequency * MU0 / COPPER_CONDUCTIVITY).sqrt()
}

/// Decibels in a neper.
const DB_PER_NEPER: f64 = 20.0 / std::f64::consts::LN_10;

/// The loss per mm of an uncovered microstrip `width` mm wide and `thickness` mm thick on the
/// dielectric `below`, at `frequency` Hz with the loss tangent `tan_d` ([`loss_per_mm`]).
pub fn microstrip_loss(
    width: f64,
    thickness: f64,
    below: TrackDielectric,
    frequency: f64,
    tan_d: f64,
) -> Result<TrackLoss, OutOfRange> {
    let line = microstrip(width, thickness, below)?;
    let (z0, eeff, er) = (line.impedance, line.epsilon_eff, below.epsilon);
    let lambda0 = C0 / frequency * 1e3;
    let fill = if er > 1.0 {
        (eeff - 1.0) / (er - 1.0)
    } else {
        1.0
    };
    Ok(TrackLoss {
        conductor: DB_PER_NEPER * surface_resistance(frequency) / (z0 * width * 1e-3) * 1e-3,
        dielectric: DB_PER_NEPER * PI * er / eeff.sqrt() * fill * tan_d / lambda0,
    })
}

/// The loss per mm of a stripline `width` mm wide and `thickness` mm thick between the
/// dielectric layers `above` and `below`, at `frequency` Hz with the loss tangent `tan_d`
/// ([`loss_per_mm`]).
pub fn stripline_loss(
    width: f64,
    thickness: f64,
    above: TrackDielectric,
    below: TrackDielectric,
    frequency: f64,
    tan_d: f64,
) -> Result<TrackLoss, OutOfRange> {
    let line = stripline(width, thickness, above, below)?;
    let (z0, eeff) = (line.impedance, line.epsilon_eff);
    let rs = surface_resistance(frequency);
    let lambda0 = C0 / frequency * 1e3;
    let (w, t) = (width * 1e-3, thickness * 1e-3);
    let b = (above.thickness + below.thickness) * 1e-3 + t;
    if t <= 0.0 {
        return Err(OutOfRange(
            "the stripline's copper loss needs a copper thickness above 0".to_string(),
        ));
    }
    let nepers = if eeff.sqrt() * z0 < 120.0 {
        let a = 1.0 + 2.0 * w / (b - t) + (b + t) / (b - t) / PI * ((2.0 * b - t) / t).ln();
        2.7e-3 * rs * eeff * z0 / (30.0 * PI * (b - t)) * a
    } else {
        let bb = 1.0
            + b / (0.5 * w + 0.7 * t)
                * (0.5 + 0.414 * t / w + (4.0 * PI * w / t).ln() / (2.0 * PI));
        0.16 * rs / (z0 * b) * bb
    };
    Ok(TrackLoss {
        conductor: DB_PER_NEPER * nepers * 1e-3,
        dielectric: DB_PER_NEPER * PI * eeff.sqrt() * tan_d / lambda0,
    })
}

/// A frequency for a message: `2.4 GHz`, `100 MHz`.
pub fn hertz(f: f64) -> String {
    let (v, unit) = if f >= 1e9 {
        (f / 1e9, "GHz")
    } else if f >= 1e6 {
        (f / 1e6, "MHz")
    } else if f >= 1e3 {
        (f / 1e3, "kHz")
    } else {
        (f, "Hz")
    };
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s} {unit}")
}
