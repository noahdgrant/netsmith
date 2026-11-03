//! The gate in front of packaging: a board that fails KiCad's design rules check is not
//! sent to a fab. We run `kicad-cli pcb drc` for errors and read the counts from the
//! report's footer.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub violations: u32,
    pub unconnected: u32,
    pub footprint_errors: u32,
}

impl Counts {
    pub fn total(self) -> u32 {
        self.violations + self.unconnected + self.footprint_errors
    }
}

/// Counts from the text of a `kicad-cli pcb drc --format report` file, which has lines like
/// `** Found 30 DRC violations **`, `** Found 0 unconnected pads **` and
/// `** Found 0 Footprint errors **`. A report without them is not a report.
pub fn parse_report(report: &str) -> Option<Counts> {
    let count = |what: &str| -> Option<u32> {
        report.lines().find_map(|l| {
            let l = l.trim().strip_prefix("** Found ")?;
            l.strip_suffix(" **")?
                .strip_suffix(what)?
                .trim()
                .parse()
                .ok()
        })
    };
    Some(Counts {
        violations: count("DRC violations")?,
        unconnected: count("unconnected pads")?,
        footprint_errors: count("Footprint errors")?,
    })
}

/// What a DRC run found, or why it could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Clean,
    Failed {
        counts: Counts,
        report: String,
    },
    /// The report has no summary lines.
    Unreadable {
        report: String,
    },
}

pub fn check(report: &str) -> Verdict {
    match parse_report(report) {
        Some(c) if c.total() == 0 => Verdict::Clean,
        Some(counts) => Verdict::Failed {
            counts,
            report: report.to_string(),
        },
        None => Verdict::Unreadable {
            report: report.to_string(),
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub const CLEAN: &str = "** Drc report for b.kicad_pcb **\n\
        ** Created on 2026-10-31T23:27:39-0400 **\n\n\
        ** Found 0 DRC violations **\n\n\
        ** Found 0 unconnected pads **\n\n\
        ** Found 0 Footprint errors **\n\n\
        ** End of Report **\n";

    #[test]
    fn a_clean_report_passes() {
        assert_eq!(check(CLEAN), Verdict::Clean);
    }

    #[test]
    fn violations_unconnected_pads_and_footprint_errors_each_fail() {
        for (from, to, expected) in [
            (
                "Found 0 DRC",
                "Found 3 DRC",
                Counts {
                    violations: 3,
                    ..Counts::default()
                },
            ),
            (
                "Found 0 unconnected",
                "Found 2 unconnected",
                Counts {
                    unconnected: 2,
                    ..Counts::default()
                },
            ),
            (
                "Found 0 Footprint",
                "Found 1 Footprint",
                Counts {
                    footprint_errors: 1,
                    ..Counts::default()
                },
            ),
        ] {
            let report = CLEAN.replace(from, to);
            match check(&report) {
                Verdict::Failed { counts, report: r } => {
                    assert_eq!(counts, expected);
                    assert_eq!(r, report);
                }
                other => panic!("expected a failure, got {other:?}"),
            }
        }
    }

    #[test]
    fn a_report_without_a_summary_is_unreadable() {
        assert!(matches!(
            check("kicad-cli: something went wrong"),
            Verdict::Unreadable { .. }
        ));
    }
}
