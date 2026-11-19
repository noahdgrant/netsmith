//! The loss of a track (issue #1405) against figures that do not come from the closed forms:
//! the skin depth of copper, the exact dielectric loss of a line in one dielectric, and the
//! conductor loss by Wheeler's incremental inductance rule worked out from the impedance alone.

use netsmith_core::{DielectricLayer, InnerLayers, InnerUse, Stackup};
use netsmith_pcb::{
    TrackDielectric, loss_per_mm, microstrip, microstrip_loss, skin_depth, stripline,
    stripline_loss, surface_resistance,
};

/// The impedance of free space, μ0·c, in ohms (CODATA 2018).
const ETA0: f64 = 376.730_313_412;

fn within(got: f64, want: f64, fraction: f64) -> bool {
    ((got - want) / want).abs() <= fraction
}

fn dielectric(thickness: f64, epsilon: f64) -> TrackDielectric {
    TrackDielectric { thickness, epsilon }
}

/// The dielectric loss of a TEM line all in one dielectric, in dB per mm: `27.3 sqrt(er) tan(d)
/// / lambda0`, which holds for any shape of the line.
fn homogeneous(er: f64, tan_d: f64, f: f64) -> f64 {
    let lambda0 = 299_792_458.0 / f * 1e3;
    20.0 / std::f64::consts::LN_10 * std::f64::consts::PI * er.sqrt() * tan_d / lambda0
}

#[test]
fn the_skin_depth_and_surface_resistance_of_copper_are_the_textbook_figures() {
    // Copper at 1 GHz: a skin depth of 2.09 um and a surface resistance of 8.25 mohm (sigma
    // 58 MS/m; Pozar, Microwave Engineering, 4th ed., appendix F, gives 5.813e7 S/m).
    assert!(
        within(skin_depth(1e9), 2.09e-3, 0.005),
        "{}",
        skin_depth(1e9)
    );
    assert!(within(surface_resistance(1e9), 8.25e-3, 0.005));
    // Both go with the root of the frequency.
    assert!(within(
        surface_resistance(4e9),
        2.0 * surface_resistance(1e9),
        1e-12
    ));
}

#[test]
fn a_stripline_in_one_dielectric_loses_the_exact_dielectric_loss() {
    let d = dielectric(0.2, 4.4);
    for f in [1e9, 2.4e9, 10e9] {
        let l = stripline_loss(0.15, 0.0152, d, d, f, 0.02).unwrap();
        assert!(
            within(l.dielectric, homogeneous(4.4, 0.02, f), 1e-9),
            "{l:?}"
        );
    }
}

#[test]
fn a_microstrip_loses_less_in_its_dielectric_than_a_line_all_in_it_and_nears_it_as_it_widens() {
    let d = dielectric(0.2, 4.4);
    let f = 2.4e9;
    let full = homogeneous(4.4, 0.02, f);
    let narrow = microstrip_loss(0.1, 0.035, d, f, 0.02).unwrap().dielectric;
    let wide = microstrip_loss(15.0, 0.035, d, f, 0.02).unwrap().dielectric;
    assert!(narrow < wide && wide < full, "{narrow} {wide} {full}");
    assert!(within(wide, full, 0.05), "{wide} {full}");
    // It goes with the loss tangent and the frequency.
    let twice = microstrip_loss(0.1, 0.035, d, 2.0 * f, 0.04).unwrap().dielectric;
    assert!(within(twice, 4.0 * narrow, 1e-9));
}

#[test]
fn a_microstrip_loses_rs_over_z0_w_in_its_copper_as_a_parallel_plate_line_does() {
    // A parallel-plate line loses Rs / (Z0 W) in nepers per length, exactly (both plates).
    let d = dielectric(0.2, 4.4);
    let (w, f) = (0.35, 2.4e9);
    let line = microstrip(w, 0.035, d).unwrap();
    let want = 20.0 / std::f64::consts::LN_10 * surface_resistance(f) / (line.impedance * w);
    let got = microstrip_loss(w, 0.035, d, f, 0.0).unwrap();
    assert!(within(got.conductor, want, 1e-9), "{got:?}");
    assert_eq!(got.dielectric, 0.0);
}

/// Wheeler's incremental inductance rule: a line's conductor loss, in nepers per mm, is `Rs /
/// (2 eta Z0) dZ0/dn`, with `n` the depth every copper wall recedes by (the strip narrows and
/// thins by 2n, the planes move apart by 2n), worked out here by a finite difference of the
/// stripline's impedance alone.
fn wheeler_rule(w: f64, t: f64, h: f64, er: f64, f: f64) -> f64 {
    let n = 1e-5;
    let z = |w: f64, t: f64, h: f64| stripline(w, t, dielectric(h, er), dielectric(h, er)).unwrap().impedance;
    // Each wall recedes by n: the strip is 2n narrower and thinner, and each dielectric 2n
    // thicker, as the strip's faces and the planes move apart.
    let at = |n: f64| z(w - 2.0 * n, t - 2.0 * n, h + 2.0 * n);
    let dz = (at(n) - at(-n)) / (2.0 * n);
    let eta = ETA0 / er.sqrt();
    surface_resistance(f) / (2.0 * eta * z(w, t, h)) * dz
}

#[test]
fn pozars_stripline_copper_loss_is_near_wheelers_incremental_inductance_rule() {
    let f = 2.4e9;
    for (w, t, h) in [(0.15, 0.0152, 0.2), (0.3, 0.035, 0.3), (0.1, 0.0152, 0.12)] {
        let got = stripline_loss(w, t, dielectric(h, 4.4), dielectric(h, 4.4), f, 0.0)
            .unwrap()
            .conductor;
        let want = 20.0 / std::f64::consts::LN_10 * wheeler_rule(w, t, h, 4.4, f);
        assert!(
            within(got, want, 0.10),
            "w {w} t {t} h {h}: Pozar {got:.5} dB/mm, Wheeler's rule {want:.5} dB/mm"
        );
    }
}

/// JLCPCB's default four-layer build-up of 1.6 mm, `JLC04161H-7628`, with a loss tangent.
fn four_layer(tan_d: Option<f64>) -> Stackup {
    let layer = |core: bool, glass: &str, thickness: f64, epsilon: f64| DielectricLayer {
        core,
        glass: glass.into(),
        thickness,
        epsilon,
        plies: Vec::new(),
    };
    Stackup {
        layers: 4,
        thickness: (1.6, 1.6),
        copper_weight: 1.0,
        inner: Some(InnerLayers {
            layers: vec![InnerUse::Plane("GND".into()), InnerUse::Signal],
            copper: 0.0152,
            dielectrics: vec![
                layer(false, "7628", 0.2104, 4.4),
                layer(true, "", 1.065, 4.6),
                layer(false, "7628", 0.2104, 4.4),
            ],
            build: "JLC04161H-7628".into(),
            hole_clearance: 0.3,
            mask: None,
            outer_copper: None,
            loss_tangent: tan_d,
        }),
        mask_colour: None,
    }
}

#[test]
fn a_board_works_out_the_loss_of_each_layer_from_its_build_up() {
    let s = four_layer(Some(0.02));
    let outer = loss_per_mm(&s, "F.Cu", 0.35, 2.4e9).unwrap();
    let want = microstrip_loss(0.35, 0.035, dielectric(0.2104, 4.4), 2.4e9, 0.02).unwrap();
    assert_eq!(outer, want);
    // FR-4 at 2.4 GHz: about a decibel in 7 to 8 cm.
    println!("F.Cu 0.35 mm at 2.4 GHz: {:.4} dB/mm", outer.total());
    assert!((0.005..0.03).contains(&outer.total()), "{outer:?}");
    let inner = loss_per_mm(&s, "In2.Cu", 0.15, 2.4e9).unwrap();
    assert!(inner.dielectric > 0.0 && inner.conductor > 0.0, "{inner:?}");
}

#[test]
fn a_build_up_with_no_loss_tangent_a_two_layer_board_and_a_low_frequency_have_no_figure() {
    let e = loss_per_mm(&four_layer(None), "F.Cu", 0.35, 2.4e9).unwrap_err();
    assert!(
        e.0.contains("build-up JLC04161H-7628 records no loss tangent")
            && e.0.contains("`.process.stackup.loss_tangent &= 0.02`"),
        "{e}"
    );
    let mut two = four_layer(Some(0.02));
    two.layers = 2;
    two.inner = None;
    let e = loss_per_mm(&two, "F.Cu", 0.35, 2.4e9).unwrap_err();
    assert!(e.0.contains("records no build-up"), "{e}");
    let e = loss_per_mm(&four_layer(Some(0.02)), "In2.Cu", 0.15, 1e6).unwrap_err();
    assert!(
        e.0.contains("at 1 MHz the skin depth of copper is") && e.0.contains("0.0152 mm"),
        "{e}"
    );
    let e = loss_per_mm(&four_layer(Some(0.02)), "In1.Cu", 0.15, 2.4e9).unwrap_err();
    assert!(e.0.contains("`In1.Cu` is not a layer a track may be on"), "{e}");
}
