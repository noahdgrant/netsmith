//! The generic parts of a board against its environment, once `fab` has found their rows
//! (ADR 0099, ADR 0047).
//!
//! The design stage judges every fitted part against the board's environment, but a generic
//! part (a `Resistor`, a `Capacitor`, an `Led` named by its value) has no catalog row yet: the row
//! that is ordered for it is found when `fab` resolves it. So `fab` judges the row it found, with
//! the one function the design stage uses (`netsmith_design::environment::judge`): a row inside
//! the environment is met and says nothing, a row outside it stops the run before anything is
//! placed (the row would have been ordered), and a row that states no range is named and counted
//! as unjudged. The counts of the two judgements are the lines of `fab-notes.txt` about the
//! environment.

use netsmith_check::InstId;
use netsmith_design::Sourced;
use netsmith_design::environment::{Fitted, judge};
use netsmith_solve::Pick;

use crate::Built;

/// What judging the resolved generic parts of one board gave.
#[derive(Debug, Default)]
pub(crate) struct Resolved {
    /// The lines of `fab-notes.txt` about the environment; empty for a board that has none.
    pub notes: Vec<String>,
    /// The generic parts this judgement spoke for: the design stage's findings about them are
    /// replaced by `findings`.
    pub replaced: Vec<InstId>,
    /// The findings about the rows `fab` chose, as the command line prints them.
    pub findings: Vec<String>,
}

/// Judge the generic parts of `built` against its environment, with the rows `sourcing` (the
/// board's sourcing after `resolve_generics`) gives them. The error is each refusal, one
/// message each; the board has no environment: nothing is judged and nothing is said.
pub(crate) fn judge_resolved(built: &Built, sourcing: &[Sourced]) -> Result<Resolved, Vec<String>> {
    let Some(designed) = &built.environment else {
        return Ok(Resolved::default());
    };
    // The row of each generic part the design stage could not judge, or none for a part that no
    // row stands for (one the owner consigns, or one that needs no number): the first sourced
    // line of a part stands for a bank.
    let mut picks: Vec<(InstId, usize, Option<Pick>)> = Vec::new();
    for &part in &designed.pending {
        let Some(at) = built.trace.origins.iter().position(|&o| o == part) else {
            continue;
        };
        let Some(s) = sourcing.get(at) else { continue };
        let pick = s.resolved_row.as_ref().map(|row| Pick {
            part,
            catalog: s
                .generic
                .as_ref()
                .map_or_else(String::new, |g| g.catalog.clone()),
            candidate: row.name.clone(),
            label: s.value.clone(),
            lcsc: s.lcsc.clone(),
            mpn: s.mpn.clone(),
            library: s.library,
            assembly: None,
            height: s.catalog_height,
            temp_min: row.temp_min,
            temp_max: row.temp_max,
            generic: None,
            rejected: 0,
            count: 1,
        });
        picks.push((part, at, pick));
    }
    let fitted: Vec<Fitted<'_>> = picks
        .iter()
        .map(|(part, at, pick)| Fitted {
            part: *part,
            kind: &sourcing[*at].kind,
            assembly: sourcing[*at].assembly,
            pick: pick.as_ref(),
        })
        .collect();
    let mut errors = Vec::new();
    let judged = judge(&built.trace.model, &fitted, &mut errors);
    if !errors.is_empty() {
        errors.sort();
        errors.dedup();
        return Err(errors
            .into_iter()
            .map(|e| {
                format!(
                    "{e} (the generic part is resolved to that row, the one that would be ordered, and nothing is placed or written until the row fits)"
                )
            })
            .collect());
    }
    let mut tally = designed.clone();
    if let Some(found) = &judged.tally {
        tally.add(found);
    }
    // Every part that was waiting is judged now: one that no row stands for was named as such.
    tally.pending.clear();
    let label = built.label();
    Ok(Resolved {
        notes: tally.notes(),
        replaced: designed.pending.clone(),
        findings: judged
            .findings
            .iter()
            .map(|g| format!("{label}: {} guideline: {}", g.rule, g.message))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve::resolve_generics;
    use crate::{outcome_with, render_problems};
    use netsmith_check::Loader;
    use netsmith_design::ResolvedRow;
    use netsmith_solve::Catalogs;

    /// One generic resistor of 31.6 kohm, a value that only the project's row below has. The
    /// row states -20 C to 70 C, so in an environment to 85 C the solver leaves it out, keeps
    /// the generic part, and `fab` finds the row.
    fn board(environment: &str) -> String {
        format!(
            "\
import std::env::Environment
import std::parts::passive::Resistor
import std::sources::Rail

board Widget {{
    .supply: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }}
    .r1: Resistor {{ .resistance &= 31.6kohm +/- 1% }}
    .supply.pos ~> .r1 ~> .supply.neg
{environment}}}
"
        )
    }

    const ROW: &str = "\
lcsc,mpn,resistance,max_power,package,temp_min,temp_max,library,source
C99001,ACME-R1,31.6kohm +/- 1%,62.5mW,0402,253.15K,343.15K,Basic,https://example.invalid/acme.pdf temp: page 1
";

    fn catalogs() -> Catalogs {
        let mut c = Catalogs::jlcpcb().with_series(Some(netsmith_solve::Series::E96));
        c.add_file("resistor", "catalogs/resistor.csv", ROW)
            .unwrap();
        c
    }

    fn built(src: &str) -> crate::Built {
        outcome_with("widget.nsm", src, &Loader::std(), &catalogs(), None)
            .map(|o| o.boards.into_iter().next().expect("one board"))
            .unwrap_or_else(|p| panic!("{}", render_problems(&p)))
    }

    const WARM: &str = "    .environment: Environment { .temperature &= -40degC..85degC }\n";

    /// The sourcing as `fab` has it, with the row of the generic part said by the test.
    fn sourcing(b: &crate::Built, row: Option<ResolvedRow>) -> Vec<netsmith_design::Sourced> {
        let (mut sourcing, unresolved) = resolve_generics(&b.sourcing, &catalogs());
        assert!(unresolved.is_empty(), "{unresolved:?}");
        if let Some(s) = sourcing.iter_mut().find(|s| s.kind == "Resistor") {
            s.resolved_row = row;
        }
        sourcing
    }

    fn row(min: Option<f64>, max: Option<f64>) -> Option<ResolvedRow> {
        Some(ResolvedRow {
            name: "ACME-X".into(),
            temp_min: min,
            temp_max: max,
        })
    }

    #[test]
    fn the_solver_keeps_a_generic_part_when_the_only_row_is_outside_the_environment() {
        let b = built(&board(WARM));
        let r1 = b.sourcing.iter().find(|s| s.kind == "Resistor").unwrap();
        assert!(r1.generic.is_some() && r1.lcsc.is_none(), "{r1:?}");
        let tally = b
            .environment
            .as_ref()
            .expect("the board has an environment");
        assert_eq!(tally.pending.len(), 1, "{tally:?}");
        assert_eq!((tally.met, tally.not_met), (0, 0));
    }

    #[test]
    fn a_row_inside_the_environment_is_met_and_says_nothing() {
        let b = built(&board(WARM));
        let sourcing = sourcing(&b, row(Some(233.15), Some(398.15)));
        let done = judge_resolved(&b, &sourcing).unwrap();
        assert!(done.findings.is_empty(), "{:?}", done.findings);
        assert_eq!(done.replaced.len(), 1);
        assert_eq!(
            done.notes[0],
            "environment: temperature of each fitted part against `environment.temperature` 233.15 K to 358.15 K (-40degC to 85degC): 1 met, 0 NOT MET, 0 UNJUDGED"
        );
    }

    #[test]
    fn a_row_outside_the_environment_is_refused_with_the_row_its_number_and_the_figures() {
        let b = built(&board(WARM));
        // The row `fab` really finds: ACME-R1, C99001, to 70 C.
        let (sourcing, unresolved) = resolve_generics(&b.sourcing, &catalogs());
        assert!(unresolved.is_empty());
        let errors = judge_resolved(&b, &sourcing).unwrap_err();
        assert_eq!(errors.len(), 2, "{errors:#?}");
        // The row stops at 70 C and starts at -20 C: both ends of the environment are refused.
        assert!(
            errors[0].starts_with("`r1` works down to 253.15 K (-20degC) only (the catalog row ACME-R1 (C99001) states `temp_min`), over the board's environment, which reaches 233.15 K (-40degC) (`environment.temperature` 233.15 K to 358.15 K (-40degC to 85degC)): choose a part rated for it or narrow the environment"),
            "{}",
            errors[0]
        );
        assert!(
            errors[1].starts_with("`r1` works up to 343.15 K (70degC) only (the catalog row ACME-R1 (C99001) states `temp_max`), under the board's environment, which reaches 358.15 K (85degC) (`environment.temperature` 233.15 K to 358.15 K (-40degC to 85degC)): choose a part rated for it or narrow the environment"),
            "{}",
            errors[1]
        );
    }

    #[test]
    fn a_row_with_no_range_is_named_and_counted_unjudged() {
        let b = built(&board(WARM));
        let sourcing = sourcing(&b, row(None, None));
        let done = judge_resolved(&b, &sourcing).unwrap();
        assert_eq!(done.findings.len(), 1, "{:?}", done.findings);
        assert!(
            done.findings[0].contains("`r1` (Resistor) was picked as ACME-X and its catalog row states no `temp_min` and no `temp_max`"),
            "{}",
            done.findings[0]
        );
        assert!(
            done.notes[0].ends_with(
                "0 met, 0 NOT MET, 1 UNJUDGED (1 with a range stated at one end only or at neither)"
            ),
            "{}",
            done.notes[0]
        );
    }

    #[test]
    fn a_generic_part_that_no_row_stands_for_is_counted_as_having_no_catalog_row() {
        let b = built(&board(WARM));
        let sourcing = sourcing(&b, None);
        let done = judge_resolved(&b, &sourcing).unwrap();
        assert!(
            done.notes[0].ends_with("0 met, 0 NOT MET, 1 UNJUDGED (1 with no catalog row)"),
            "{}",
            done.notes[0]
        );
    }

    #[test]
    fn a_part_that_states_its_own_range_is_judged_by_it_and_not_by_the_row() {
        let src = board(&format!(
            "{WARM}    .r1.temp_min &= -40degC\n    .r1.temp_max &= 100degC\n"
        ));
        let b = built(&src);
        let sourcing = sourcing(&b, row(Some(293.15), Some(303.15)));
        let done = judge_resolved(&b, &sourcing).unwrap();
        assert!(
            done.notes[0].ends_with("1 met, 0 NOT MET, 0 UNJUDGED"),
            "{}",
            done.notes[0]
        );
    }

    #[test]
    fn a_board_with_no_environment_says_nothing() {
        let b = built(&board(""));
        assert!(b.environment.is_none());
        let (sourcing, _) = resolve_generics(&b.sourcing, &catalogs());
        let done = judge_resolved(&b, &sourcing).unwrap();
        assert!(done.notes.is_empty() && done.findings.is_empty() && done.replaced.is_empty());
    }
}
