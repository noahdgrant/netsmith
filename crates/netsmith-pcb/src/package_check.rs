//! A footprint measured against the known size of the package a catalog row names (issue #1658,
//! [`netsmith_core::package_sizes`]): its count of pads, the body it draws on `F.Fab`, and its
//! courtyard, which must hold the smallest body of the package.

use netsmith_core::package_sizes::{self, KnownPackage, Span};

use crate::footprint::Footprint;

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

/// Measure `footprint` (named `id`) against the known size of `package` (the row's package, as
/// written): the pads counted by number (an unnumbered pad is none), the body it draws on
/// `F.Fab` (short side and long side, either way round) within [`package_sizes::SLACK`] of the
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
        let (w, h) = sides(footprint.body.width(), footprint.body.height());
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

    #[test]
    fn a_package_netsmith_does_not_know_is_not_measured() {
        assert_eq!(
            check("SOT-223", "Package_TO_SOT_SMD:SOT-223"),
            PackageCheck::NotKnown
        );
    }
}
