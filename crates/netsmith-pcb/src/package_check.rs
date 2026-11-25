//! A footprint measured against the known size of the package a catalog row names (issue #1658,
//! [`netsmith_core::package_sizes`]): its count of pads, the body it draws on `F.Fab`, and its
//! courtyard, which must hold the smallest body of the package.
//!
//! The body is the largest closed outline drawn on `F.Fab` (issue #1662): a footprint that also
//! draws each lead there (a rectangle of its own beside the body) is measured by its body, not
//! by the span over its leads.

use std::collections::HashMap;

use netsmith_core::package_sizes::{self, KnownPackage, Span};

use crate::footprint::{Footprint, Layer, Shape};
use crate::geometry::{Pt, Rect};

/// What [`check_package`] found.
#[derive(Debug, Clone, PartialEq)]
pub enum PackageCheck {
    /// The package is not one netsmith knows the size of: nothing was measured.
    NotKnown,
    /// The footprint is the package.
    Agrees(&'static KnownPackage),
    /// The footprint is not the package: each message gives both numbers.
    Disagrees(&'static KnownPackage, Vec<String>),
}

/// Round to a micrometre, for a message.
fn um(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// The two sides of a box, the shorter first.
fn sides(w: f64, h: f64) -> (f64, f64) {
    if w <= h { (w, h) } else { (h, w) }
}

fn size(short: Span, long: Span) -> String {
    format!("{short} x {long} mm")
}

/// The body a footprint draws on `F.Fab`, as the package check measures it: the box of the
/// largest closed outline there. A rectangle, a circle and a polygon are each closed; lines and
/// arcs are joined end to end (to 1 µm), and a chain of them is closed when every end meets
/// another. Leads drawn as outlines of their own, a pin-one mark or a cathode bar that are not
/// part of the body's outline are left out. A drawing with no closed outline is measured as the
/// box round all of it ([`Footprint::body`]).
pub fn package_body(footprint: &Footprint) -> Rect {
    let fab: Vec<&Shape> = footprint
        .graphics
        .iter()
        .filter(|g| g.layer == Layer::Fab)
        .map(|g| &g.shape)
        .collect();
    let key = |p: &Pt| ((p.x * 1000.0).round() as i64, (p.y * 1000.0).round() as i64);
    // The ends of the open pieces, joined where they meet.
    let mut ends: HashMap<(i64, i64), usize> = HashMap::new();
    let mut parent: Vec<usize> = Vec::new();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut end = |p: &Pt, parent: &mut Vec<usize>| -> usize {
        *ends.entry(key(p)).or_insert_with(|| {
            parent.push(parent.len());
            parent.len() - 1
        })
    };
    let mut degree: Vec<usize> = Vec::new();
    let mut pieces: Vec<(usize, &Shape)> = Vec::new();
    let mut outlines: Vec<Rect> = Vec::new();
    for shape in &fab {
        match shape {
            Shape::Line { a, b }
            | Shape::Arc {
                start: a, end: b, ..
            } => {
                let (i, j) = (end(a, &mut parent), end(b, &mut parent));
                degree.resize(parent.len(), 0);
                degree[i] += 1;
                degree[j] += 1;
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                parent[ri] = rj;
                pieces.push((i, shape));
            }
            closed => outlines.extend(Rect::around(closed.exact_extent())),
        }
    }
    let mut chains: HashMap<usize, (bool, Vec<Pt>)> = HashMap::new();
    for (i, d) in degree.iter().enumerate() {
        let r = root(&mut parent, i);
        chains.entry(r).or_insert((true, Vec::new())).0 &= *d >= 2;
    }
    for (i, shape) in &pieces {
        let r = root(&mut parent, *i);
        if let Some(chain) = chains.get_mut(&r) {
            chain.1.extend(shape.exact_extent());
        }
    }
    outlines.extend(
        chains
            .into_values()
            .filter(|(closed, _)| *closed)
            .filter_map(|(_, points)| Rect::around(points)),
    );
    outlines
        .into_iter()
        .max_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
        .unwrap_or(footprint.body)
}

/// Measure `footprint` (named `id`) against the known size of `package` (the row's package, as
/// written): the pads counted by number (an unnumbered pad is none), the body it draws on
/// `F.Fab` ([`package_body`]: short side and long side, either way round) within [`package_sizes::SLACK`] of the
/// package's, and a courtyard at least the package's smallest body. A footprint that draws no
/// body has only its pads and courtyard measured. Each message names the footprint, the package,
/// both numbers and the source of the package's.
pub fn check_package(package: &str, id: &str, footprint: &Footprint) -> PackageCheck {
    let Some(known) = package_sizes::known(package) else {
        return PackageCheck::NotKnown;
    };
    let mut problems = Vec::new();
    let says = |what: String| {
        format!(
            "a `{}` (the row's package `{package}`) has {what}; source: {}",
            known.name, known.source
        )
    };
    let pads = footprint
        .pad_numbers()
        .into_iter()
        .filter(|n| !n.is_empty())
        .count();
    if pads != known.pads {
        problems.push(format!(
            "the footprint `{id}` has {pads} pads, but {}",
            says(format!("{} pads", known.pads))
        ));
    }
    let (short, long) = known.body;
    if !footprint.body_is_courtyard {
        let body = package_body(footprint);
        let (w, h) = sides(body.width(), body.height());
        if !short.holds(w) || !long.holds(h) {
            problems.push(format!(
                "the footprint `{id}` draws a body {} x {} mm on `F.Fab` (short side, long side), but {}",
                um(w),
                um(h),
                says(format!("a body {}", size(short, long)))
            ));
        }
    }
    let (w, h) = sides(footprint.courtyard.width(), footprint.courtyard.height());
    if w + package_sizes::SLACK < short.min || h + package_sizes::SLACK < long.min {
        problems.push(format!(
            "the footprint `{id}` has a courtyard {} x {} mm, smaller than the body, but {}",
            um(w),
            um(h),
            says(format!("a body {}", size(short, long)))
        ));
    }
    if problems.is_empty() {
        PackageCheck::Agrees(known)
    } else {
        PackageCheck::Disagrees(known, problems)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::generate;

    fn check(package: &str, id: &str) -> PackageCheck {
        let footprint = generate(id).unwrap_or_else(|| panic!("{id} is generated"));
        check_package(package, id, &footprint)
    }

    #[test]
    fn a_sot_23_row_on_the_sot_323_footprint_is_reported_with_both_sizes_and_the_source() {
        let PackageCheck::Disagrees(known, problems) =
            check("SOT-23", "Package_TO_SOT_SMD:SOT-323_SC-70")
        else {
            panic!("a SOT-323 footprint is not a SOT-23");
        };
        assert_eq!(known.name, "SOT-23");
        let text = problems.join("\n");
        assert!(text.contains("1.35 x 2.2 mm"), "{text}");
        assert!(text.contains("1.2-1.4 x 2.8-3 mm"), "{text}");
        assert!(text.contains("JSCJ 2N7002"), "{text}");
    }

    #[test]
    fn a_footprint_of_the_package_agrees() {
        for (package, id) in [
            ("SOT-23", "Package_TO_SOT_SMD:SOT-23"),
            ("SOT-323", "Package_TO_SOT_SMD:SOT-323_SC-70"),
            ("SOT-23-5", "Package_TO_SOT_SMD:SOT-23-5"),
            ("0402", "Resistor_SMD:R_0402_1005Metric"),
            ("0805", "Capacitor_SMD:C_0805_2012Metric"),
            ("SMA", "Diode_SMD:D_SMA"),
        ] {
            assert!(
                matches!(check(package, id), PackageCheck::Agrees(_)),
                "{package} {id}: {:?}",
                check(package, id)
            );
        }
    }

    #[test]
    fn a_footprint_of_another_pad_count_or_size_disagrees() {
        for (package, id) in [
            ("SOT-23", "Package_TO_SOT_SMD:SOT-23-5"),
            ("0402", "Resistor_SMD:R_0603_1608Metric"),
            ("SMA", "Diode_SMD:D_SMB"),
            ("SOD-123", "Diode_SMD:D_SOD-323"),
        ] {
            assert!(
                matches!(check(package, id), PackageCheck::Disagrees(..)),
                "{package} {id}"
            );
        }
    }

    /// A footprint of a project's own library, as the example has it.
    fn example(path: &str) -> Footprint {
        let path = format!("{}/../../examples/{path}", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        Footprint::parse("example", &text).unwrap()
    }

    #[test]
    fn a_footprint_that_draws_its_leads_on_f_fab_is_measured_by_its_body() {
        // The body's outline is 4.013 x 4.978 mm; with the eight leads it draws beside it, the
        // drawing spans 6.198 mm.
        let footprint = example("orbit/libs/orbit_original.pretty/SOIC127P600X175-8N.kicad_mod");
        assert!(
            (footprint.body.width() - 6.1976).abs() < 1e-3,
            "{:?}",
            footprint.body
        );
        let body = package_body(&footprint);
        assert!((body.width() - 4.0132).abs() < 1e-3, "{body:?}");
        assert!((body.height() - 4.9784).abs() < 1e-3, "{body:?}");
        assert!(
            matches!(
                check_package("SOIC-8", "SOIC127P600X175-8N", &footprint),
                PackageCheck::Agrees(_)
            ),
            "{:?}",
            check_package("SOIC-8", "SOIC127P600X175-8N", &footprint)
        );
    }

    #[test]
    fn a_body_wider_than_the_package_is_still_reported() {
        // KiCad's SOT-23W draws Allegro's wide body, 1.91 x 2.98 mm, a closed outline with no
        // leads: a part in the plain SOT-23 does not have that body.
        let footprint =
            example("easyduino_esp32s3/libs/easyduino_original.pretty/SOT-23W.kicad_mod");
        let body = package_body(&footprint);
        assert!((body.width() - 1.91).abs() < 1e-3, "{body:?}");
        let PackageCheck::Disagrees(_, problems) = check_package("SOT-23", "SOT-23W", &footprint)
        else {
            panic!("a SOT-23W body is not a SOT-23's");
        };
        assert!(
            problems.join("\n").contains("1.91 x 2.98 mm"),
            "{problems:?}"
        );
    }

    #[test]
    fn leads_drawn_beside_a_generated_body_do_not_count() {
        // The generator's SOT-23 with a lead drawn on `F.Fab` outside its body, a rectangle of
        // its own: the body is measured as before.
        let mut footprint = generate("Package_TO_SOT_SMD:SOT-23").unwrap();
        let before = package_body(&footprint);
        footprint.graphics.push(crate::footprint::Graphic {
            layer: Layer::Fab,
            shape: Shape::Rect {
                a: Pt::new(-2.5, -1.2),
                b: Pt::new(-0.7, -0.7),
            },
            stroke: 0.1,
            filled: false,
        });
        assert_eq!(package_body(&footprint), before);
        assert!(matches!(
            check_package("SOT-23", "Package_TO_SOT_SMD:SOT-23", &footprint),
            PackageCheck::Agrees(_)
        ));
    }

    #[test]
    fn a_package_netsmith_does_not_know_is_not_measured() {
        assert_eq!(
            check("SOT-223", "Package_TO_SOT_SMD:SOT-223"),
            PackageCheck::NotKnown
        );
    }
}
