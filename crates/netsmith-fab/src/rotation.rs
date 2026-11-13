//! Rotation corrections between KiCad's footprint orientation and JLCPCB's pick-and-place
//! convention.
//!
//! The offsets are measured, not assumed. The Fabrication Toolkit corrects rotations before
//! writing `positions.csv`; comparing that file with `kicad-cli pcb export pos` for the
//! same board, part by part, gives the offset per footprint. That was done for two
//! boards whose assemblies JLCPCB accepted; their files are not in this repository, so
//! the table is verified against them but cannot be re-checked here.
//!
//! Every family listed below was seen at least once; the count is in the comment. A
//! footprint matching no pattern has no known offset: it is left as KiCad has it and
//! reported as unverified, because guessing a correction rotates the part on the board.
//!
//! Not covered by any evidence, so reported when present: SOT-23, SOT-223, SOIC and QFN
//! families, LED sizes other than 0402, resistor and capacitor sizes not listed, and every
//! part on the bottom side (no board in the evidence has one).

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Pattern {
    /// The whole footprint name.
    Exact(&'static str),
    /// The start of the footprint name.
    Prefix(&'static str),
}

impl Pattern {
    fn matches(self, name: &str) -> bool {
        match self {
            Self::Exact(e) => name == e,
            Self::Prefix(p) => name.starts_with(p),
        }
    }
}

/// Degrees added to KiCad's rotation to get JLCPCB's.
pub(crate) const TABLE: &[(Pattern, f64)] = &[
    (Pattern::Prefix("R_0402_"), 0.0),              // 18 parts
    (Pattern::Prefix("R_0603_"), 0.0),              // 3
    (Pattern::Prefix("R_0805_"), 0.0),              // 1
    (Pattern::Prefix("C_0402_"), 0.0),              // 10
    (Pattern::Prefix("C_0603_"), 0.0),              // 8
    (Pattern::Prefix("C_1206_"), 0.0),              // 1
    (Pattern::Prefix("LED_0402_"), 0.0),            // 3
    (Pattern::Prefix("Fuse_0603_"), 0.0),           // 2
    (Pattern::Exact("D_SMA"), 0.0),                 // 2
    (Pattern::Exact("Crystal_SMD_HC49-SD"), 0.0),   // 1
    (Pattern::Prefix("TSSOP-20_4.4x6.5mm"), 270.0), // 1
    (Pattern::Prefix("DFN-8-1EP_3x3mm"), 270.0),    // 1
];

/// What is known about a footprint's rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Correction {
    Offset(f64),
    Unverified,
}

/// `name` is the footprint's name without its library, such as `R_0603_1608Metric`.
pub(crate) fn lookup(name: &str) -> Correction {
    TABLE
        .iter()
        .find(|(p, _)| p.matches(name))
        .map_or(Correction::Unverified, |&(_, o)| Correction::Offset(o))
}

/// `degrees` in `[0, 360)`.
pub(crate) fn normalize(degrees: f64) -> f64 {
    let r = degrees.rem_euclid(360.0);
    // Rounds away float noise such as 359.99999999999994 and avoids a negative zero.
    let r = (r * 1e6).round() / 1e6;
    if r >= 360.0 { 0.0 } else { r + 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_families_have_offsets() {
        assert_eq!(lookup("R_0402_1005Metric"), Correction::Offset(0.0));
        assert_eq!(lookup("D_SMA"), Correction::Offset(0.0));
        assert_eq!(
            lookup("TSSOP-20_4.4x6.5mm_P0.65mm"),
            Correction::Offset(270.0)
        );
        assert_eq!(
            lookup("DFN-8-1EP_3x3mm_P0.65mm_EP1.55x2.4mm"),
            Correction::Offset(270.0)
        );
    }

    #[test]
    fn unmeasured_families_are_unverified() {
        for name in [
            "SOT-223-3_TabPin2",
            "SOT-23-6",
            "LED_0603_1608Metric",
            "R_2512_6332Metric",
            "D_SMA_Handsoldering",
            "PinHeader_1x02_P2.54mm_Vertical",
        ] {
            assert_eq!(lookup(name), Correction::Unverified, "{name}");
        }
    }

    #[test]
    fn no_pattern_hides_another() {
        for (i, (a, _)) in TABLE.iter().enumerate() {
            for (j, (b, _)) in TABLE.iter().enumerate() {
                if i == j {
                    continue;
                }
                let sample = match b {
                    Pattern::Exact(s) | Pattern::Prefix(s) => *s,
                };
                assert!(!a.matches(sample), "{a:?} also matches {b:?}");
            }
        }
    }

    #[test]
    fn normalizes_into_zero_to_360() {
        assert_eq!(normalize(-90.0), 270.0);
        assert_eq!(normalize(450.0), 90.0);
        assert_eq!(normalize(360.0), 0.0);
        assert_eq!(normalize(-0.0), 0.0);
        assert!(normalize(-0.0).is_sign_positive());
        assert_eq!(normalize(-1e-9), 0.0);
    }
}
