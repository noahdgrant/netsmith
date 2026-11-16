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
    let line = |w| microstrip(w, 0.035, d);
    let w = width_for(50.0, line, 0.001, 20.0).unwrap();
    assert!(within(line(w).unwrap().impedance, 50.0, 1e-9), "{w}");
    // No width of that dielectric makes 500 ohm inside the domain (w/h at least 0.01).
    assert!(width_for(500.0, line, 0.001, 20.0).is_err());
}
