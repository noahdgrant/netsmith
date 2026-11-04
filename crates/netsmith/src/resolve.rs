//! Resolving generic parts to orderable ones, when the bill of materials or `fab` is made
//! (ADR 0047, provisional).
//!
//! The solver picks a generic resistor by value alone (`netsmith_solve::Generic`). Here each
//! one becomes the row of a catalog that stands for it: Basic first, a row a provider says
//! has no stock passed over. A part no row stands for is **unresolved**, with the nearest
//! rows, and `fab` refuses it by name.

use netsmith_core::Part;
use netsmith_lower::Sourced;
use netsmith_solve::{CHANGE_THE_VALUE, Catalogs, Solution, alternatives, resolve};

/// The most times a build solves a board again to check alternatives. Each unresolved part
/// takes one solve for each of its (up to three) alternatives; parts past the limit are
/// listed without the check, as they were before it existed.
pub const MAX_RECHECKS: usize = 30;

/// The checks of one board run again with one alternative in place of an unresolved part
/// (ADR 0047).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recheck {
    pub refdes: String,
    /// The alternative row's name.
    pub alternative: String,
    /// `Ok` if the board solves with it; otherwise the first problem, in one line.
    pub outcome: Result<(), String>,
}

impl Recheck {
    fn phrase(&self) -> String {
        match &self.outcome {
            Ok(()) => format!("{} passes every check", self.alternative),
            Err(why) => format!("{} fails: {why}", self.alternative),
        }
    }
}

/// For each generic part of a solved board that no row stands for, solve the board again
/// with the part held to each of its nearest alternatives, and report whether the checks
/// still pass. `again` solves the same board with the catalogs it is given (one with the
/// part pinned); `parts` are the board's parts, to name each by its reference designator.
pub fn recheck_alternatives(
    solution: &Solution,
    parts: &[Part],
    catalogs: &Catalogs,
    again: &dyn Fn(&Catalogs) -> Solution,
) -> Vec<Recheck> {
    let stock = |lcsc: &str| catalogs.availability().and_then(|a| a.stock(lcsc));
    let mut out = Vec::new();
    let mut solves = 0;
    for pick in &solution.picks {
        let Some(generic) = pick.generic.as_ref().filter(|_| pick.lcsc.is_none()) else {
            continue;
        };
        if resolve(generic, catalogs, &stock).is_ok() {
            continue;
        }
        let path = solution.model.path_of(pick.part);
        let Some(part) = parts.iter().find(|p| p.path == path) else {
            continue;
        };
        for alt in alternatives(generic, catalogs) {
            if solves >= MAX_RECHECKS {
                return out;
            }
            solves += 1;
            let solved = again(&catalogs.with_pin(&path, &alt.name));
            let outcome = match solved.diagnostics.first() {
                None => Ok(()),
                Some(d) => {
                    let line = d.message.lines().next().unwrap_or_default();
                    // "no `x` candidate satisfies `p`: the closest ..., fails because <why>"
                    Err(line
                        .split_once(" fails because ")
                        .map_or(line, |(_, why)| why)
                        .to_string())
                }
            };
            out.push(Recheck {
                refdes: part.refdes.clone(),
                alternative: alt.name,
                outcome,
            });
        }
    }
    out
}

/// A generic part that no row stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    pub refdes: String,
    /// What is wanted, why nothing fits, and the nearest rows.
    pub message: String,
}

impl Unresolved {
    /// `R3: unresolved: ...`
    pub fn line(&self) -> String {
        format!("{}: {}", self.refdes, self.message)
    }
}

/// The sourcing with every generic part that a row stands for given that row's numbers, and
/// the generic parts left. A part that already has a number is untouched. The stock asked
/// about is the catalogs' `Availability`, if the project has one.
pub fn resolve_generics(
    sourcing: &[Sourced],
    rechecks: &[Recheck],
    catalogs: &Catalogs,
) -> (Vec<Sourced>, Vec<Unresolved>) {
    let stock = |lcsc: &str| catalogs.availability().and_then(|a| a.stock(lcsc));
    let mut unresolved = Vec::new();
    let resolved = sourcing
        .iter()
        .map(|s| {
            let mut s = s.clone();
            let Some(generic) = s.generic.clone().filter(|_| s.lcsc.is_none()) else {
                return s;
            };
            match resolve(&generic, catalogs, &stock) {
                Ok(row) => {
                    s.lcsc = row.lcsc;
                    s.mpn = row.mpn;
                    s.library = row.library;
                }
                Err(message) => unresolved.push(Unresolved {
                    refdes: s.refdes.clone(),
                    message: with_rechecks(message, &s.refdes, rechecks),
                }),
            }
            s
        })
        .collect();
    (resolved, unresolved)
}

/// The message of an unresolved part with the result of each alternative's re-check put
/// before the closing advice.
fn with_rechecks(message: String, refdes: &str, rechecks: &[Recheck]) -> String {
    let mine: Vec<String> = rechecks
        .iter()
        .filter(|r| r.refdes == refdes)
        .map(Recheck::phrase)
        .collect();
    if mine.is_empty() {
        return message;
    }
    let head = message
        .strip_suffix(CHANGE_THE_VALUE)
        .unwrap_or(&message)
        .to_string();
    format!(
        "{head} Checked again with each alternative in this part's place, its stated value left out: {}.{CHANGE_THE_VALUE}",
        mine.join("; ")
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use netsmith_lower::Assembly;
    use netsmith_solve::{Availability, Generic, Series, Spec};

    use super::*;

    const ROWS: &str = "lcsc,mpn,resistance,max_power,package,library,source\n\
        C900001,ZZR-A,249ohm +/- 1%,62.5mW,0402,Basic,https://example.invalid/a\n\
        C900002,ZZR-B,249ohm +/- 1%,62.5mW,0402,Extended,https://example.invalid/b\n";

    /// Stock by LCSC number; a number not listed is unknown.
    #[derive(Debug)]
    struct Stock(Vec<(&'static str, u64)>);

    impl Availability for Stock {
        fn stock(&self, lcsc: &str) -> Option<u64> {
            self.0.iter().find(|(n, _)| *n == lcsc).map(|(_, s)| *s)
        }
    }

    fn catalogs(stock: Option<Stock>) -> Catalogs {
        let mut c = Catalogs::empty();
        c.add_file("resistor", "catalogs/resistor.csv", ROWS)
            .unwrap();
        match stock {
            Some(s) => c.with_availability(Arc::new(s)),
            None => c,
        }
    }

    fn generic_part(refdes: &str) -> Sourced {
        Sourced {
            refdes: refdes.into(),
            kind: "Resistor".into(),
            value: "249".into(),
            package: Some("0402".into()),
            lcsc: None,
            mpn: None,
            library: None,
            assembly: Assembly::Fab,
            assembly_stated: false,
            generic: Some(Generic {
                catalog: "resistor".into(),
                package: "0402".into(),
                spec: Spec::Resistor {
                    series: Series::E96,
                    nominal: 249.0,
                    tolerance_percent: 1.0,
                    max_power: 0.0625,
                },
            }),
        }
    }

    #[test]
    fn basic_comes_first_and_a_part_that_already_has_a_number_is_untouched() {
        let mut numbered = generic_part("R2");
        numbered.lcsc = Some("C900099".into());
        let (resolved, unresolved) =
            resolve_generics(&[generic_part("R1"), numbered], &[], &catalogs(None));
        assert!(unresolved.is_empty());
        assert_eq!(resolved[0].lcsc.as_deref(), Some("C900001"));
        assert_eq!(resolved[0].mpn.as_deref(), Some("ZZR-A"));
        assert_eq!(resolved[1].lcsc.as_deref(), Some("C900099"));
        assert_eq!(resolved[1].mpn, None);
    }

    #[test]
    fn a_row_with_no_stock_is_passed_over_and_unknown_stock_is_not() {
        let sold_out = Stock(vec![("C900001", 0)]);
        let (resolved, _) = resolve_generics(&[generic_part("R1")], &[], &catalogs(Some(sold_out)));
        assert_eq!(resolved[0].lcsc.as_deref(), Some("C900002"));
        // The provider knows nothing of either row: Basic first, as with no provider.
        let other = Stock(vec![("C900077", 0)]);
        let (resolved, _) = resolve_generics(&[generic_part("R1")], &[], &catalogs(Some(other)));
        assert_eq!(resolved[0].lcsc.as_deref(), Some("C900001"));
    }

    #[test]
    fn when_every_fitting_row_is_sold_out_the_part_is_unresolved_and_named() {
        let all_out = Stock(vec![("C900001", 0), ("C900002", 0)]);
        let (resolved, unresolved) =
            resolve_generics(&[generic_part("R7")], &[], &catalogs(Some(all_out)));
        assert_eq!(resolved[0].lcsc, None);
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].refdes, "R7");
        let line = unresolved[0].line();
        assert!(
            line.starts_with("R7: unresolved: no row of the `resistor` catalog"),
            "{line}"
        );
        assert!(line.contains("Rows that fit were passed over"), "{line}");
    }
}
