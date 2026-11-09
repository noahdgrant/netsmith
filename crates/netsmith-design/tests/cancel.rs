//! The design stage can be stopped (ADR 0062), and with a token that is not cancelled it makes
//! the design it always did.

use netsmith_check::{Loader, check};
use netsmith_core::job::{self, Cancel, Cancelled};
use netsmith_design::design_board;
use netsmith_solve::{Catalogs, solve};

#[path = "../../netsmith-check/tests/common/stages.rs"]
mod stages;

#[test]
fn designing_with_a_cancelled_token_stops_and_with_another_gives_the_same_design() {
    let c = check("cancel.nsm", &stages::source(2), &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let solved = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(solved.diagnostics.is_empty(), "{:?}", solved.diagnostics);

    let plain = design_board(&c.boards[0], &solved).expect("designs");
    let token = Cancel::new();
    let watched = job::catch(|| job::scope(&token, || design_board(&c.boards[0], &solved)))
        .expect("not cancelled")
        .expect("designs");
    assert_eq!(format!("{plain:?}"), format!("{watched:?}"));

    token.cancel();
    let result = job::catch(|| job::scope(&token, || design_board(&c.boards[0], &solved)));
    assert!(matches!(result, Err(Cancelled)));
}
