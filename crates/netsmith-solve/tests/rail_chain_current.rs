//! A load behind a regulator is not worked out from the regulator's enclosed output current
//! (issue #741).
//!
//! Before the picks the solver encloses a regulator's output current from what the loads on its
//! net state (ADR 0076, amendment 11). That current is the sum of the loads, so the balance of
//! the net must not work out one load as the enclosure less the others: their ranges would be
//! counted twice. Here the store's port was the one load with no value before the picks, it was
//! given -45 mA to 45 mA, a balance of two terms in the store gave it to a balancing resistor,
//! and the resistor's power check failed at `-88 W .. 88 W` though the chain carries about 60 uA.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, LockedRow, PickLock, Solution, solve};

/// The board of the issue: a buck's 3.3 V rail with a load and a store whose three resistors are
/// one chain across the rail, with the chain's current written by hand.
const BOARD: &str = r#"import std::parts::passive::{Capacitor, Resistor}
import std::parts::regulator::{Buck, Lmr16006y}
import std::sources::Rail

module Store {
    .supply: Rail
    .mid: Pin
    .low: Pin
    .cell_top: Capacitor { .capacitance &= 10uF +/- 20% }
    .cell_bottom: Capacitor { .capacitance &= 10uF +/- 20% }
    .balance_top: Resistor { .resistance &= 43kohm +/- 1% }
    .balance_bottom: Resistor { .resistance &= 43kohm +/- 1% }
    .limit: Resistor

    .supply.pos ~> .cell_top ~> .mid
    .mid ~> .cell_bottom ~> .low
    .supply.pos ~> .balance_top ~> .mid
    .mid ~> .balance_bottom ~> .low
    .low ~> .limit ~> .supply.neg

    .balance_top.current == .supply.voltage / (.balance_top.resistance + .balance_bottom.resistance + .limit.resistance)
    .balance_bottom.current == .balance_top.current
    .limit.current == .balance_bottom.current
}

board Repro {
    .input: Rail {
        .voltage &= 9V..16V
        .peak &= 24V
        .pos.net &= "VIN"
    }
    .reg: Buck<Lmr16006y> {
        .vout.pos.net &= "V3V3"
        .vout.voltage &= 3.3V +/- 5%
    }
    .input ~ .reg.vin
    .load: Rail { .current &= 5mA..50mA }
    .reg.vout ~ .load
    .store: Store
    .reg.vout ~ .store.supply
}
"#;

fn run(catalogs: &Catalogs) -> Solution {
    let c = check("test.nsm", BOARD, &Loader::std());
    assert!(c.is_ok(), "the source should check clean:\n{}", c.render());
    solve(&c.boards[0], catalogs)
}

fn errors(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn q(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

/// The chain's current is what its own equation gives, tens of microamperes, and the store's
/// port carries it: neither is the rail's whole current.
fn assert_chain_current(s: &Solution) {
    for path in [
        "store.balance_top.current",
        "store.balance_bottom.current",
        "store.limit.current",
        "store.supply.current",
    ] {
        let i = q(s, path);
        assert!(
            i.lo() > 0.0 && i.hi() < 100e-6,
            "`{path}` is {i}, not the chain's tens of microamperes"
        );
    }
}

#[test]
fn a_chain_across_a_regulated_rail_is_judged_on_its_own_current() {
    let s = run(&Catalogs::jlcpcb());
    assert!(errors(&s).is_empty(), "{:?}", errors(&s));
    assert_chain_current(&s);
}

/// The second build holds each part to its row in the pick lock. A catalog of one row has a
/// narrow hull, so the regulator's enclosed load fits its rating and is written, which on the
/// issue's larger board made the locked build fail where the first passed.
#[test]
fn a_build_held_to_its_lock_judges_the_rows_as_the_first_did() {
    let first = run(&Catalogs::jlcpcb());
    assert!(errors(&first).is_empty(), "{:?}", errors(&first));
    let rows = first
        .picks
        .iter()
        .map(|p| LockedRow::of_pick(first.model.path_of(p.part), p))
        .collect();
    let mut lock = PickLock::new("test.lock.toml");
    lock.set("Repro", rows);
    let second = run(&Catalogs::jlcpcb().with_pick_lock(lock));
    assert!(errors(&second).is_empty(), "{:?}", errors(&second));
    assert_chain_current(&second);
    let names = |s: &Solution| -> Vec<String> {
        s.picks
            .iter()
            .map(|p| format!("{} = {}", s.model.path_of(p.part), p.candidate))
            .collect()
    };
    assert_eq!(names(&first), names(&second));
}
