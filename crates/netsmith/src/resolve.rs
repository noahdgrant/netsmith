//! Resolving generic parts to orderable ones, when the bill of materials or `fab` is made
//! (ADR 0047, provisional).
//!
//! The solver picks a generic resistor by value alone (`netsmith_solve::Generic`). Here each
//! one becomes the row of a catalog that stands for it: Basic first, a row a provider says
//! has no stock passed over. A part no row stands for is **unresolved**, with the nearest
//! rows, and `fab` refuses it by name.
//!
//! **Checking the alternatives** (ADR 0047, issue #31, provisional). When the build finds an
//! unresolved generic part it plans a solve of the board with the part held to each of its
//! nearest alternatives (`Catalogs::with_pin`). The solves are made when a caller first asks to
//! show the result (the unresolved message of the bill of materials or `fab`), not by the build
//! itself, so a build that shows neither solves the board once. The numbers the design stated
//! for that part's catalog columns are what the alternative replaces, so they are dropped before
//! the board is solved; the package and colour statements and every relation stay. The message
//! then says, for each alternative, "passes every check" or "fails: <the relation and the
//! numbers>". The solves are capped at [`MAX_RECHECKS`] for a board; a part past the cap is
//! listed without the check. The editor's swap action, which would be a view of the same
//! values, is not built.

use netsmith_check::InstId;
use netsmith_core::Part;
use netsmith_design::Sourced;
use netsmith_design::guidelines::Guideline;
use netsmith_solve::{CHANGE_THE_VALUE, Catalogs, Solution, alternatives, resolve};
use std::sync::{Arc, OnceLock};

/// The most times the board is solved again to check alternatives. Each unresolved part
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

/// The alternatives of a board's unresolved generic parts, and the board's checks run again
/// with each (ADR 0047). What to re-solve is planned when the board is built, and costs no
/// solve; the solves are made when the first caller asks for the result (`get`), because the
/// only places that show it are the unresolved message of `fab` and of the bill of materials.
/// A build that shows neither (`netsmith build`, the editor's analysis) solves the board once.
#[derive(Clone, Default)]
pub struct Rechecks {
    plan: Option<Arc<Plan>>,
}

/// The alternatives to solve for, and the solve that does it.
struct Plan {
    /// One row for each alternative to check, in the order of the board's picks, with the
    /// reference designator and path of the part it stands in for.
    jobs: Vec<Job>,
    /// Solves the same board with the catalogs it is given.
    again: Box<dyn Fn(&Catalogs) -> Solution + Send + Sync>,
    done: OnceLock<Vec<Recheck>>,
}

struct Job {
    refdes: String,
    path: String,
    alternative: String,
}

impl std::fmt::Debug for Rechecks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rechecks")
            .field("planned", &self.planned())
            .field("solved", &self.is_solved())
            .finish()
    }
}

impl Rechecks {
    /// No part has an alternative to check.
    pub fn none() -> Self {
        Self::default()
    }

    /// For each generic part of a solved board that no row stands for, plan a solve of the
    /// board with the part held to each of its nearest alternatives. `parts` are the board's
    /// parts, to name each by its reference designator. `again` makes the function that
    /// solves the same board with the catalogs it is given (one with the part pinned); it is
    /// called only if some part is unresolved, so a board that has none is not copied.
    pub fn plan(
        solution: &Solution,
        parts: &[Part],
        catalogs: &Catalogs,
        again: impl FnOnce() -> Box<dyn Fn(&Catalogs) -> Solution + Send + Sync>,
    ) -> Self {
        let stock = |lcsc: &str| catalogs.availability().and_then(|a| a.stock(lcsc));
        let mut jobs = Vec::new();
        'picks: for pick in &solution.picks {
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
                if jobs.len() >= MAX_RECHECKS {
                    break 'picks;
                }
                jobs.push(Job {
                    refdes: part.refdes.clone(),
                    path: path.clone(),
                    alternative: alt.name,
                });
            }
        }
        if jobs.is_empty() {
            return Self::none();
        }
        Self {
            plan: Some(Arc::new(Plan {
                jobs,
                again: again(),
                done: OnceLock::new(),
            })),
        }
    }

    /// How many alternatives will be (or were) solved for.
    pub fn planned(&self) -> usize {
        self.plan.as_ref().map_or(0, |p| p.jobs.len())
    }

    /// Whether the solves have been made.
    pub fn is_solved(&self) -> bool {
        self.plan.as_ref().is_some_and(|p| p.done.get().is_some())
    }

    /// The result of each alternative's re-check: the first call makes the solves, each with
    /// one row of `catalogs` (the project's, as the build had them) pinned; a later call reads
    /// what it made.
    pub fn get(&self, catalogs: &Catalogs) -> &[Recheck] {
        match &self.plan {
            None => &[],
            Some(plan) => plan.done.get_or_init(|| plan.solve(catalogs)),
        }
    }
}

impl Plan {
    fn solve(&self, catalogs: &Catalogs) -> Vec<Recheck> {
        self.jobs
            .iter()
            .enumerate()
            .map(|(n, job)| {
                let solved = (self.again)(&catalogs.with_pin(&job.path, &job.alternative));
                netsmith_core::job::report("alternative parts re-solved", n as u64 + 1, None);
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
                Recheck {
                    refdes: job.refdes.clone(),
                    alternative: job.alternative.clone(),
                    outcome,
                }
            })
            .collect()
    }
}

/// A generic part that no row stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    pub refdes: String,
    /// What is wanted, why nothing fits, and the nearest rows.
    pub message: String,
}

impl Unresolved {
    /// `R3: unresolved: ...`, with the check of each alternative (this makes the solves of
    /// `rechecks` if they are not made yet).
    pub fn line(&self, rechecks: &Rechecks, catalogs: &Catalogs) -> String {
        format!("{}: {}", self.refdes, self.checked(rechecks, catalogs))
    }

    /// The message with the result of each alternative's re-check put before the closing
    /// advice. The part with no alternative to check has the message as it is, and the solves
    /// are not made for it.
    pub fn checked(&self, rechecks: &Rechecks, catalogs: &Catalogs) -> String {
        with_rechecks(&self.message, &self.refdes, rechecks, catalogs)
    }

    /// The message without its `unresolved: ` lead and the advice that closes it, to be said
    /// in a sentence of its own.
    fn detail(&self) -> &str {
        let text = self
            .message
            .strip_prefix("unresolved: ")
            .unwrap_or(&self.message);
        text.strip_suffix(CHANGE_THE_VALUE).unwrap_or(text)
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
            // The owner supplies a consigned part, so no row of a catalog stands for it.
            if s.assembly == netsmith_design::Assembly::Consigned {
                return s;
            }
            let Some(generic) = s.generic.clone().filter(|_| s.lcsc.is_none()) else {
                return s;
            };
            match resolve(&generic, catalogs, &stock) {
                Ok(row) => {
                    s.lcsc = row.lcsc;
                    s.mpn = row.mpn;
                    s.library = row.library;
                    s.catalog_height = row.height;
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

/// The guideline of a generic part that the design assembles and no catalog row can be
/// ordered for (issue #182): the solver kept it because no row meets the design, and `fab`
/// would refuse the board. It is the same check as `fab`'s (`resolve_generics`), made when the
/// board is built, for every assembled generic and not only on power paths; a part that is
/// fitted by hand or not at all is left out, as `fab` leaves it out. `origins` is the
/// instance of each part of `sourcing`, and `path_of` says the path of an instance.
pub(crate) fn unresolved_findings(
    sourcing: &[Sourced],
    origins: &[InstId],
    catalogs: &Catalogs,
    path_of: &dyn Fn(InstId) -> String,
) -> Vec<Guideline> {
    resolve_generics(sourcing, catalogs)
        .1
        .iter()
        .filter_map(|u| {
            let at = sourcing
                .iter()
                .position(|s| s.refdes == u.refdes && s.needs_part_number())?;
            let catalog = sourcing[at].generic.as_ref()?.catalog.as_str();
            let inst = origins.get(at).copied();
            let place = inst
                .map(path_of)
                .filter(|p| !p.is_empty())
                .map_or(String::new(), |p| format!(" ({p})"));
            let detail = u.detail();
            let stop = if detail.ends_with('.') { "" } else { "." };
            Some(Guideline {
                rule: "unresolved generic",
                message: format!(
                    "{}{place}: the solver kept a generic part because {detail}{stop} `fab` will refuse the board until a row meets it: add one to the project's `{catalog}` catalog (`catalogs/{catalog}.csv`), or change the design so that a row in the catalogs does.",
                    u.refdes
                ),
                at: inst,
            })
        })
        .collect()
}

/// The message of an unresolved part with the result of each alternative's re-check put
/// before the closing advice. The re-checks are made only if the part has an alternative.
fn with_rechecks(message: &str, refdes: &str, rechecks: &Rechecks, catalogs: &Catalogs) -> String {
    if rechecks.planned() == 0 {
        return message.to_string();
    }
    let mine: Vec<String> = rechecks
        .get(catalogs)
        .iter()
        .filter(|r| r.refdes == refdes)
        .map(Recheck::phrase)
        .collect();
    if mine.is_empty() {
        return message.to_string();
    }
    let head = message.strip_suffix(CHANGE_THE_VALUE).unwrap_or(message);
    format!(
        "{head} Checked again with each alternative in this part's place, its stated value left out: {}.{CHANGE_THE_VALUE}",
        mine.join("; ")
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use netsmith_design::Assembly;
    use netsmith_solve::{Availability, Generic, Series, Spec};

    use super::*;

    const ROWS: &str = "lcsc,mpn,resistance,max_power,package,library,height,source\n\
        C900001,ZZR-A,249ohm +/- 1%,62.5mW,0402,Basic,,https://example.invalid/a\n\
        C900002,ZZR-B,249ohm +/- 1%,62.5mW,0402,Extended,0.55mm,https://example.invalid/b\n";

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
            catalog_height: None,
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
            resolve_generics(&[generic_part("R1"), numbered], &catalogs(None));
        assert!(unresolved.is_empty());
        assert_eq!(resolved[0].lcsc.as_deref(), Some("C900001"));
        assert_eq!(resolved[0].mpn.as_deref(), Some("ZZR-A"));
        assert_eq!(resolved[1].lcsc.as_deref(), Some("C900099"));
        assert_eq!(resolved[1].mpn, None);
    }

    #[test]
    fn a_generic_part_has_the_height_of_the_row_it_resolves_to_and_none_when_the_row_has_none() {
        // The Basic row states no height, so a part that resolves to it has none; when that row
        // is sold out the part resolves to the Extended row and has its 0.55 mm (ADR 0094).
        let (resolved, _) = resolve_generics(&[generic_part("R1")], &catalogs(None));
        assert_eq!(resolved[0].mpn.as_deref(), Some("ZZR-A"));
        assert_eq!(resolved[0].catalog_height, None);
        let sold_out = Stock(vec![("C900001", 0)]);
        let (resolved, _) = resolve_generics(&[generic_part("R1")], &catalogs(Some(sold_out)));
        assert_eq!(resolved[0].mpn.as_deref(), Some("ZZR-B"));
        assert_eq!(resolved[0].catalog_height, Some(0.55));
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
        let catalogs = catalogs(Some(all_out));
        let (resolved, unresolved) = resolve_generics(&[generic_part("R7")], &catalogs);
        assert_eq!(resolved[0].lcsc, None);
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].refdes, "R7");
        let line = unresolved[0].line(&Rechecks::none(), &catalogs);
        assert!(
            line.starts_with("R7: unresolved: no row of the `resistor` catalog"),
            "{line}"
        );
        assert!(line.contains("Rows that fit were passed over"), "{line}");
    }

    fn findings(parts: &[Sourced], catalogs: &Catalogs) -> Vec<Guideline> {
        unresolved_findings(parts, &[], catalogs, &|_| String::new())
    }

    #[test]
    fn an_assembled_generic_that_no_row_stands_for_is_a_finding_with_the_need_and_the_advice() {
        let all_out = Stock(vec![("C900001", 0), ("C900002", 0)]);
        let found = findings(&[generic_part("R7")], &catalogs(Some(all_out)));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rule, "unresolved generic");
        let message = &found[0].message;
        assert!(
            message.starts_with("R7: the solver kept a generic part because no row of the `resistor` catalog can be ordered for 249 ohm 1% 0402 62.5 mW (E96)."),
            "{message}"
        );
        assert!(
            message.contains("Rows that fit were passed over"),
            "{message}"
        );
        assert!(
            message.ends_with("`fab` will refuse the board until a row meets it: add one to the project's `resistor` catalog (`catalogs/resistor.csv`), or change the design so that a row in the catalogs does."),
            "{message}"
        );
    }

    #[test]
    fn a_generic_that_a_row_stands_for_is_no_finding() {
        assert!(findings(&[generic_part("R1")], &catalogs(None)).is_empty());
    }

    #[test]
    fn a_part_fitted_by_hand_or_not_at_all_is_no_finding() {
        let all_out = Stock(vec![("C900001", 0), ("C900002", 0)]);
        let catalogs = catalogs(Some(all_out));
        let mut hand = generic_part("R1");
        hand.assembly = Assembly::Hand;
        let mut none = generic_part("R2");
        none.assembly = Assembly::None;
        assert!(findings(&[hand, none], &catalogs).is_empty());
        // The one the board house assembles is still named.
        let found = findings(&[generic_part("R3")], &catalogs);
        assert_eq!(found.len(), 1);
    }
}
