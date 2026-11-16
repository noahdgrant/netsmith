//! The track impedance calculator (issue #759) against values that do not come from it: the
//! exact impedance of a thin centred stripline and the worked examples of a textbook.

use netsmith_pcb::{OutOfRange, TrackDielectric, microstrip, stripline, width_for};

/// The impedance of free space, μ0·c, in ohms (CODATA 2018).
const ETA0: f64 = 376.730_313_412;

/// The complete elliptic integral of the first kind of modulus `k`, by the arithmetic-geometric
/// mean: K(k) = π / (2 AGM(1, sqrt(1 - k²))).
fn elliptic_k(k: f64) -> f64 {
    let (mut a, mut b) = (1.0_f64, (1.0 - k * k).sqrt());
    for _ in 0..40 {
        (a, b) = ((a + b) / 2.0, (a * b).sqrt());
    }
    std::f64::consts::PI / (2.0 * a)
}

/// Cohn's exact impedance of a strip of no thickness, `w` wide, centred between planes `b`
/// apart in a medium of εr 1 (S. B. Cohn, "Characteristic Impedance of the Shielded-Strip
/// Transmission Line", IRE Trans. MTT-2, 1954, pp. 52-57, by conformal mapping):
/// Z = (η0 / 4) K(k) / K(k'), with k = sech(πw / 2b) and k' = tanh(πw / 2b).
fn cohn_exact(w: f64, b: f64) -> f64 {
    let x = std::f64::consts::PI * w / (2.0 * b);
    ETA0 / 4.0 * elliptic_k(1.0 / x.cosh()) / elliptic_k(x.tanh())
}

fn air(thickness: f64) -> TrackDielectric {
    TrackDielectric {
        thickness,
        epsilon: 1.0,
    }
}

fn within(got: f64, want: f64, fraction: f64) -> bool {
    ((got - want) / want).abs() <= fraction
}

#[test]
fn a_thin_centred_stripline_is_within_wheelers_half_percent_of_the_exact_impedance() {
    // Wheeler 1978 states 0.5 % for W'/(b - t) < 10. From w/b = 0.05 to 8.
    let b = 1.0;
    for w in [0.05, 0.1, 0.2, 0.35, 0.5, 0.8, 1.0, 2.0, 4.0, 8.0] {
        let got = stripline(w, 0.0, air(b / 2.0), air(b / 2.0)).unwrap().impedance;
        let exact = cohn_exact(w, b);
        assert!(
            within(got, exact, 0.005),
            "w/b {w}: Wheeler {got:.3} ohm, exact {exact:.3} ohm"
        );
    }
}

#[test]
fn a_stripline_in_one_dielectric_is_the_air_line_over_the_root_of_its_permittivity() {
    let er = 4.4;
    let filled = TrackDielectric {
        thickness: 0.2,
        epsilon: er,
    };
    let line = stripline(0.15, 0.0152, filled, filled).unwrap();
    let in_air = stripline(0.15, 0.0152, air(0.2), air(0.2)).unwrap();
    assert!(within(line.impedance, in_air.impedance / er.sqrt(), 1e-12));
    assert!(within(line.epsilon_eff, er, 1e-12));
}

#[test]
fn pozars_example_of_a_50_ohm_stripline_is_50_ohm() {
    // D. M. Pozar, Microwave Engineering, 4th ed., Example 3.5: a 50 ohm stripline with
    // b = 0.32 cm and εr = 2.20 is W = 0.266 cm wide (his formula is good to about 1 %).
    let d = TrackDielectric {
        thickness: 1.6,
        epsilon: 2.2,
    };
    let z = stripline(2.66, 0.0, d, d).unwrap().impedance;
    assert!(within(z, 50.0, 0.01), "{z:.3} ohm");
}

#[test]
fn pozars_example_of_a_50_ohm_microstrip_is_50_ohm() {
    // D. M. Pozar, Microwave Engineering, 4th ed., Example 3.7: on 0.5 mm of alumina
    // (εr = 9.9) a 50 ohm microstrip has W/d = 0.9654 (W = 0.483 mm) and εe = 6.665 (his
    // formulas are good to about 1 %).
    let line = microstrip(
        0.9654 * 0.5,
        0.0,
        TrackDielectric {
            thickness: 0.5,
            epsilon: 9.9,
        },
    )
    .unwrap();
    assert!(within(line.impedance, 50.0, 0.01), "{line:?}");
    assert!(within(line.epsilon_eff, 6.665, 0.01), "{line:?}");
}

#[test]
fn thicker_copper_lowers_the_impedance() {
    let d = TrackDielectric {
        thickness: 0.2,
        epsilon: 4.4,
    };
    let thin = microstrip(0.3, 0.0, d).unwrap().impedance;
    let thick = microstrip(0.3, 0.035, d).unwrap().impedance;
    assert!(thick < thin, "{thick} {thin}");
    let thin = stripline(0.15, 0.0, d, d).unwrap().impedance;
    let thick = stripline(0.15, 0.0152, d, d).unwrap().impedance;
    assert!(thick < thin, "{thick} {thin}");
}

#[test]
fn a_track_outside_a_formulas_domain_has_no_figure() {
    let d = TrackDielectric {
        thickness: 0.1,
        epsilon: 4.4,
    };
    // Wider than 100 times its dielectric.
    assert!(matches!(microstrip(11.0, 0.0, d), Err(OutOfRange(_))));
    // Copper as thick as half the track.
    assert!(matches!(microstrip(0.07, 0.035, d), Err(OutOfRange(_))));
    // A stripline 10 times wider than its space to a plane.
    assert!(matches!(stripline(2.5, 0.0, d, d), Err(OutOfRange(_))));
}

#[test]
fn the_width_for_an_impedance_gives_that_impedance() {
    let d = TrackDielectric {
        thickness: 0.2104,
        epsilon: 4.4,
    };
    let line = |w| microstrip(w, 0.035, d).map(|l| l.impedance);
    let w = width_for(50.0, line, 0.001, 20.0).unwrap();
    assert!(within(line(w).unwrap(), 50.0, 1e-9), "{w}");
    // No width of that dielectric makes 500 ohm inside the domain (w/h at least 0.01).
    assert!(width_for(500.0, line, 0.001, 20.0).is_err());
}

// ---------------------------------------------------------------------------------------
// Issue #1007: a solder mask over a microstrip, and two tracks side by side.

use netsmith_core::SolderMask;
use netsmith_pcb::{
    coated_coupled_microstrip, coated_microstrip, coupled_microstrip, coupled_stripline,
};

/// JLCPCB's calculator parameters for its multilayer boards: the mask above the dielectric is 1.2
/// mil and its relative permittivity 3.8 ("Calculation Parameters Used" of its user guide).
const MASK: SolderMask = SolderMask {
    thickness: 0.03048,
    epsilon: 3.8,
};

fn prepreg(thickness: f64, epsilon: f64) -> TrackDielectric {
    TrackDielectric { thickness, epsilon }
}

#[test]
fn a_solder_mask_lowers_a_50_ohm_track_by_the_1_to_3_ohm_jlcpcb_states() {
    // https://jlcpcb.com/blog/pcb-impedance-testing: "Solder mask alone can reduce impedance by
    // about 1-3 ohm." The outer prepreg of the six-layer build-up, and the 7628 of the
    // four-layer one, each at the width that is 50 ohm without the mask.
    for d in [prepreg(0.0994, 4.1), prepreg(0.2104, 4.4)] {
        let bare = |w| microstrip(w, 0.04064, d).map(|l| l.impedance);
        let w = width_for(50.0, bare, 0.001, 20.0).unwrap();
        let coated = coated_microstrip(w, 0.04064, d, MASK).unwrap().impedance;
        let drop = 50.0 - coated;
        assert!(
            (1.0..=3.0).contains(&drop),
            "{} mm of er {}: a 50 ohm track of {w:.3} mm is {coated:.2} ohm under the mask",
            d.thickness,
            d.epsilon
        );
    }
}

#[test]
fn a_mask_of_the_dielectrics_permittivity_a_long_way_thick_makes_the_line_homogeneous() {
    // A strip in one medium that fills the space above it too has the effective permittivity
    // of the medium. The mask 50 dielectric thicknesses thick, of the dielectric's own er.
    let d = prepreg(0.1, 4.4);
    let mask = SolderMask {
        thickness: 5.0,
        epsilon: 4.4,
    };
    // Hammerstad and Jensen's filling factor under the strip and Wan and Hoorfar's over it
    // are two fits, which are within 6 % of each other where they meet: the size of the error
    // that nothing states for the mask's figure.
    for w in [0.05, 0.1, 0.2, 0.5, 1.0] {
        let line = coated_microstrip(w, 0.0, d, mask).unwrap();
        assert!(
            within(line.epsilon_eff, 4.4, 0.06),
            "w {w}: effective permittivity {:.3}",
            line.epsilon_eff
        );
    }
}

#[test]
fn a_mask_that_is_not_there_changes_nothing_and_a_mask_below_air_is_refused() {
    let d = prepreg(0.1088, 4.16);
    let bare = microstrip(0.15, 0.035, d).unwrap();
    let none = SolderMask {
        thickness: 0.0,
        epsilon: 3.8,
    };
    assert_eq!(coated_microstrip(0.15, 0.035, d, none).unwrap(), bare);
    let air = SolderMask {
        thickness: 0.03,
        epsilon: 1.0,
    };
    assert!(matches!(
        coated_microstrip(0.15, 0.035, d, air),
        Err(OutOfRange(_))
    ));
    // A thicker mask lowers the impedance more.
    let thin = coated_microstrip(0.15, 0.035, d, MASK).unwrap().impedance;
    let thick = SolderMask {
        thickness: 0.06,
        ..MASK
    };
    let thicker = coated_microstrip(0.15, 0.035, d, thick).unwrap().impedance;
    assert!(thicker < thin && thin < bare.impedance, "{thicker} {thin}");
}

#[test]
fn two_microstrips_far_apart_are_two_single_tracks() {
    for (h, er) in [(0.0994, 4.1), (0.2104, 4.4)] {
        let d = prepreg(h, er);
        for w in [0.1, 0.2, 0.4] {
            let single = microstrip(w, 0.04064, d).unwrap().impedance;
            let pair = coupled_microstrip(w, 9.0 * h, 0.04064, d).unwrap();
            assert!(
                within(pair.even.impedance, single, 0.015)
                    && within(pair.odd.impedance, single, 0.025),
                "h {h} w {w}: single {single:.2}, even {:.2}, odd {:.2}",
                pair.even.impedance,
                pair.odd.impedance
            );
        }
    }
}

#[test]
fn closer_tracks_couple_more_and_the_even_mode_is_the_higher() {
    let d = prepreg(0.2104, 4.4);
    let mut last = None;
    for s in [0.5, 0.3, 0.2, 0.1, 0.05] {
        let p = coupled_microstrip(0.2, s, 0.035, d).unwrap();
        assert!(p.even.impedance > p.odd.impedance, "s {s}");
        assert!(
            p.even.epsilon_eff > p.odd.epsilon_eff,
            "the even mode has more of its field in the dielectric"
        );
        if let Some(prev) = last {
            assert!(p.odd.impedance < prev, "the odd mode falls as the gap closes");
        }
        last = Some(p.odd.impedance);
    }
}

#[test]
fn a_microstrip_pair_is_near_the_ipc_2141_estimate() {
    // IPC-2141's estimate of the differential impedance of an edge-coupled microstrip pair,
    // Zdiff = 2 Z0 (1 - 0.48 exp(-0.96 s / h)), as it is widely quoted. The standard states a
    // wide error, so this holds the figure to 8 % over the geometries it is quoted for.
    let h = 0.2104;
    let d = prepreg(h, 4.4);
    for w in [0.15, 0.2104, 0.3] {
        let z0 = microstrip(w, 0.0, d).unwrap().impedance;
        for s in [0.2, 0.2104, 0.4, 0.6] {
            let want = 2.0 * z0 * (1.0 - 0.48 * (-0.96 * s / h).exp());
            let got = coupled_microstrip(w, s, 0.0, d).unwrap().differential();
            assert!(
                within(got, want, 0.08),
                "w {w} s {s}: {got:.2} ohm, IPC-2141 {want:.2} ohm"
            );
        }
    }
}

#[test]
fn a_mask_lowers_both_modes_of_a_pair() {
    let d = prepreg(0.2104, 4.4);
    let bare = coupled_microstrip(0.2, 0.15, 0.04064, d).unwrap();
    let coated = coated_coupled_microstrip(0.2, 0.15, 0.04064, d, MASK).unwrap();
    assert!(coated.even.impedance < bare.even.impedance);
    assert!(coated.odd.impedance < bare.odd.impedance);
    assert!(coated.differential() < bare.differential());
    // By about as many ohms as a single track of that width is lowered.
    let drop = bare.differential() - coated.differential();
    assert!((2.0..=12.0).contains(&drop), "{drop}");
}

#[test]
fn the_pair_formulas_refuse_what_is_outside_their_domain() {
    let d = prepreg(0.2104, 4.4);
    // Kirschning and Jansen: w/h and s/h from 0.1 to 10, er from 1 to 18.
    assert!(matches!(
        coupled_microstrip(0.01, 0.2, 0.0, d),
        Err(OutOfRange(_))
    ));
    assert!(matches!(
        coupled_microstrip(0.2, 3.0, 0.0, d),
        Err(OutOfRange(_))
    ));
    assert!(matches!(
        coupled_microstrip(0.2, 0.2, 0.0, prepreg(0.2, 30.0)),
        Err(OutOfRange(_))
    ));
    // Copper as thick as half the track.
    assert!(matches!(
        coupled_microstrip(0.07, 0.2, 0.035, d),
        Err(OutOfRange(_))
    ));
    // A stripline pair with no space between its tracks, and one too wide for its planes.
    assert!(matches!(
        coupled_stripline(0.15, 0.0, 0.0152, d, d),
        Err(OutOfRange(_))
    ));
    assert!(matches!(
        coupled_stripline(5.0, 0.2, 0.0152, d, d),
        Err(OutOfRange(_))
    ));
}

#[test]
fn a_stripline_pair_far_apart_is_a_single_stripline_and_in_one_dielectric_a_scaled_air_one() {
    let (above, below) = (prepreg(0.55, 4.6), prepreg(0.1088, 4.16));
    for w in [0.1, 0.15, 0.25] {
        let single = stripline(w, 0.0152, above, below).unwrap().impedance;
        let pair = coupled_stripline(w, 50.0, 0.0152, above, below).unwrap();
        assert!(
            within(pair.even.impedance, single, 1e-3) && within(pair.odd.impedance, single, 1e-3),
            "w {w}: single {single:.3}, even {:.3}, odd {:.3}",
            pair.even.impedance,
            pair.odd.impedance
        );
    }
    // One dielectric: both modes are the air line's over the root of er, and the effective
    // permittivity is er.
    let er = 4.4;
    let filled = prepreg(0.2, er);
    let air_pair = coupled_stripline(0.15, 0.15, 0.0152, air(0.2), air(0.2)).unwrap();
    let pair = coupled_stripline(0.15, 0.15, 0.0152, filled, filled).unwrap();
    assert!(within(pair.even.impedance, air_pair.even.impedance / er.sqrt(), 1e-9));
    assert!(within(pair.odd.impedance, air_pair.odd.impedance / er.sqrt(), 1e-9));
    assert!(within(pair.even.epsilon_eff, er, 1e-9));
}

/// The even and odd impedances of two strips of no thickness `w` wide and `s` apart, centred
/// between two planes `b` apart in air, by finite differences on Laplace's equation: the two
/// symmetries of the section (the plane of the strips and the plane between them) leave a
/// quarter of it, and the section is solved on grids of `b / 64` and `b / 128` across the space
/// between the planes and extrapolated to a grid of nothing, since the strip's edge is a
/// singularity that the error of one grid follows by its step. Independent of Cohn's formulas, which it checks.
fn stripline_pair_by_finite_differences(w: f64, s: f64, b: f64, even: bool) -> f64 {
    let solve = |n: usize| -> f64 {
        // n cells across half the space between the planes (b / 2), so the grid is b / 2n; the
        // reach is 3 b: the field
        // between planes falls as exp(-pi x / b), 1e-4 there.
        let h = b / (2.0 * n as f64);
        let nx = (3.0 * b / h).round() as usize;
        let ny = n;
        let at = |x: f64| (x / h).round() as usize;
        let (x0, x1) = (at(s / 2.0), at(s / 2.0 + w));
        // v[i][j]: x = i h, y = j h with y = b / 2 at j = ny (the plane of the strips) and 0 at
        // j = 0 (a plane at 0 V).
        let mut v = vec![vec![0.0_f64; ny + 1]; nx + 1];
        for row in v.iter_mut().take(x1 + 1).skip(x0) {
            row[ny] = 1.0;
        }
        let fixed = |i: usize, j: usize| j == 0 || i == nx || (j == ny && (x0..=x1).contains(&i));
        // Successive over-relaxation, red-black not needed at this size.
        let rho = ((std::f64::consts::PI / nx as f64).cos() + (std::f64::consts::PI / ny as f64).cos()) / 2.0;
        let omega = 2.0 / (1.0 + (1.0 - rho * rho).sqrt());
        for _ in 0..40_000 {
            let mut change = 0.0_f64;
            for i in 0..=nx {
                for j in 0..=ny {
                    if fixed(i, j) {
                        continue;
                    }
                    // Mirror the neighbours that lie across a symmetry plane: the plane of the
                    // strips (j = ny) and, for the even mode, the plane between the strips
                    // (i = 0); in the odd mode that plane is at 0 V.
                    let left = if i == 0 {
                        if even { v[1][j] } else { 0.0 }
                    } else {
                        v[i - 1][j]
                    };
                    let up = if j == ny { v[i][ny - 1] } else { v[i][j + 1] };
                    let new = (left + v[i + 1][j] + up + v[i][j - 1]) / 4.0;
                    let d = omega * (new - v[i][j]);
                    v[i][j] += d;
                    change = change.max(d.abs());
                }
            }
            if change < 1e-10 {
                break;
            }
        }
        // The energy of the quarter, sum of the squared steps over the edges; an edge on a
        // symmetry plane counts half, and so does the one between a strip's nodes (none).
        let mut sum = 0.0;
        for i in 0..=nx {
            for j in 0..=ny {
                if i < nx {
                    let weight = if j == ny || j == 0 { 0.5 } else { 1.0 };
                    sum += weight * (v[i + 1][j] - v[i][j]).powi(2);
                }
                if j < ny {
                    let weight = if i == 0 || i == nx { 0.5 } else { 1.0 };
                    sum += weight * (v[i][j + 1] - v[i][j]).powi(2);
                }
            }
        }
        // C / ε0 of one strip's line in the mode: four quarters hold both strips, so one strip
        // has twice a quarter's energy sum.
        ETA0 / (2.0 * sum)
    };
    let (coarse, fine) = (solve(32), solve(64));
    2.0 * fine - coarse
}

#[test]
fn cohns_stripline_pair_matches_a_finite_difference_solution() {
    // Cohn's even and odd impedances of two strips of no thickness, from the 1955 paper's
    // elliptic integrals, against Laplace's equation solved on a grid.
    let b = 1.0;
    for (w, s) in [(0.25, 0.25), (0.5, 0.375), (0.375, 0.75)] {
        let pair = coupled_stripline(w, s, 0.0, air(b / 2.0), air(b / 2.0)).unwrap();
        for (even, formula) in [(true, pair.even.impedance), (false, pair.odd.impedance)] {
            let numeric = stripline_pair_by_finite_differences(w, s, b, even);
            assert!(
                within(formula, numeric, 0.01),
                "w/b {w}, s/b {s}, {}: Cohn {formula:.2} ohm, grid {numeric:.2} ohm",
                if even { "even" } else { "odd" }
            );
        }
    }
}

#[test]
fn a_thick_stripline_pair_lies_between_its_thin_pair_and_its_thick_single_track() {
    // Copper takes room between the strips and the planes, so each mode's impedance is
    // lower than the thin pair's; and a thick pair far apart is the thick single track.
    let d = prepreg(0.2, 4.4);
    let thin = coupled_stripline(0.15, 0.15, 0.0, d, d).unwrap();
    let thick = coupled_stripline(0.15, 0.15, 0.0152, d, d).unwrap();
    assert!(thick.even.impedance < thin.even.impedance);
    assert!(thick.odd.impedance < thin.odd.impedance);
    // Cohn's equation 22 (s / t under 5) and 20 (over) agree where they meet.
    let t = 0.0152;
    let just_under = coupled_stripline(0.15, 4.999 * t, t, d, d).unwrap();
    let just_over = coupled_stripline(0.15, 5.001 * t, t, d, d).unwrap();
    assert!(
        within(just_under.odd.impedance, just_over.odd.impedance, 0.06),
        "{} {}",
        just_under.odd.impedance,
        just_over.odd.impedance
    );
}

#[test]
fn the_width_for_a_differential_impedance_gives_it() {
    let d = prepreg(0.2104, 4.4);
    let line = |w| coupled_microstrip(w, 0.15, 0.04064, d).map(|p| p.differential());
    let w = width_for(100.0, line, 0.001, 20.0).unwrap();
    assert!(within(line(w).unwrap(), 100.0, 1e-9), "{w}");
}
