//! Resolving generic parts to orderable ones, when the bill of materials or `fab` is made
//! (ADR 0047, provisional).
//!
//! The solver picks a generic resistor by value alone (`netsmith_solve::Generic`). Here each
//! one becomes the row of a catalog that stands for it: Basic first, a row a provider says
//! has no stock passed over. A part no row stands for is **unresolved**, with the nearest
//! rows, and `fab` refuses it by name.

use netsmith_lower::Sourced;
use netsmith_solve::{Catalogs, resolve};

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
                    message,
                }),
            }
            s
        })
        .collect();
    (resolved, unresolved)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use netsmith_lower::Assembly;
    use netsmith_solve::{Availability, Generic, Series};

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
                series: Series::E96,
                nominal: 249.0,
                tolerance_percent: 1.0,
                package: "0402".into(),
                max_power: 0.0625,
            }),
        }
    }

    #[test]
    fn basic_comes_first_and_a_part_that_already_has_a_number_is_untouched() {
        let mut numbered = generic_part("R2");
        numbered.lcsc = Some("C900099".into());
        let (resolved, unresolved) =
            resolve_generics(&[generic_part("R1"), numbered], &catalogs(None));
        assert!(unresolved.is_empty());
        assert_eq!(resolved[0].lcsc.as_deref(), Some("C900001"));
        assert_eq!(resolved[0].mpn.as_deref(), Some("ZZR-A"));
        assert_eq!(resolved[1].lcsc.as_deref(), Some("C900099"));
        assert_eq!(resolved[1].mpn, None);
    }

    #[test]
    fn a_row_with_no_stock_is_passed_over_and_unknown_stock_is_not() {
        let sold_out = Stock(vec![("C900001", 0)]);
        let (resolved, _) = resolve_generics(&[generic_part("R1")], &catalogs(Some(sold_out)));
        assert_eq!(resolved[0].lcsc.as_deref(), Some("C900002"));
        // The provider knows nothing of either row: Basic first, as with no provider.
        let other = Stock(vec![("C900077", 0)]);
        let (resolved, _) = resolve_generics(&[generic_part("R1")], &catalogs(Some(other)));
        assert_eq!(resolved[0].lcsc.as_deref(), Some("C900001"));
    }

    #[test]
    fn when_every_fitting_row_is_sold_out_the_part_is_unresolved_and_named() {
        let all_out = Stock(vec![("C900001", 0), ("C900002", 0)]);
        let (resolved, unresolved) =
            resolve_generics(&[generic_part("R7")], &catalogs(Some(all_out)));
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
