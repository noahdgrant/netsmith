//! The verdict on a draw does not depend on how its range is written (ADR 0066, section 3):
//! `0A..750nA` and `0A .. .max_draw` (with `.max_draw &= 750nA`) are one range and judged
//! alike against the same supply, wherever the statement stands. A part's draw is the part's
//! claim, so a supply that is allowed less than the claim is refused with both forms. A design's
//! range wider than the part's own is intersected with it, as `&=` does everywhere. Every part
//! and number here is invented.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

fn run(src: &str) -> Solution {
    let checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    solve(board, &Catalogs::jlcpcb())
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

/// A gate whose supply pin draws up to the amount stated in its body (`draw`, a line of the
/// part) and a board whose supply allows `allowed`, with `gate` as the gate's statements.
fn source(part_lines: &str, gate: &str, allowed: &str) -> String {
    format!(
        "import std::sources::Rail\n\npart Gate {{\n    .designator &= \"U\"\n    .vcc: Rail\n    \
         .max_draw: Current\n{part_lines}    pins {{\n        .vcc.pos power \"VCC\" \"1\"\n        \
         .vcc.neg power \"GND\" \"2\"\n    }}\n}}\n\nboard Alone {{\n    .supply: Rail {{\n        \
         .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n        \
         .current &= {allowed}\n    }}\n    .gate: Gate {{\n{gate}    }}\n    .supply ~ .gate.vcc\n}}\n"
    )
}

const LITERAL: &str = "    .vcc.current &= 0A..750nA\n";
const MEMBER: &str = "    .vcc.current &= 0A .. .max_draw\n";
const MAX: &str = "        .max_draw &= 750nA\n";

fn first(s: &Solution) -> String {
    messages(s).into_iter().next().unwrap_or_default()
}

#[test]
fn a_part_that_writes_its_draw_with_numbers_or_with_a_member_end_is_judged_alike() {
    let literal = run(&source(LITERAL, MAX, "-500nA..0A"));
    let member = run(&source(MEMBER, MAX, "-500nA..0A"));
    let message = "the currents on net `3V3` do not add up: `supply.current` must stay within -500nA..0A, but the others on it work out to -750nA..0A";
    assert!(
        first(&literal).starts_with(message),
        "{:?}",
        messages(&literal)
    );
    assert!(
        first(&member).starts_with(message),
        "{:?}",
        messages(&member)
    );
    // And a supply that can feed the part passes with both.
    for part in [LITERAL, MEMBER] {
        let s = run(&source(part, MAX, "-1uA..0A"));
        assert!(messages(&s).is_empty(), "{:?}", messages(&s));
        assert_eq!(range(&s, "gate.vcc.current").hi(), 750e-9);
    }
}

#[test]
fn a_design_that_states_the_draw_on_the_part_is_judged_alike_in_both_forms() {
    // The board states the gate's draw; the part states none.
    let literal = run(&source(
        "",
        "        .vcc.current &= 0A..750nA\n",
        "-500nA..0A",
    ));
    let member = run(&source(
        "",
        "        .max_draw &= 750nA\n        .vcc.current &= 0A .. .max_draw\n",
        "-500nA..0A",
    ));
    assert_eq!(first(&literal), first(&member));
    assert!(
        first(&literal).contains("work out to -750nA..0A"),
        "{:?}",
        messages(&literal)
    );
}

#[test]
fn a_design_range_wider_than_the_parts_is_intersected_with_it() {
    // `&=` intersects, as atopile's does: the part's 0A..750nA and the design's 0A..3mA leave
    // 0A..750nA, in the form with numbers and in the form with a member end.
    for part in [LITERAL, MEMBER] {
        let s = run(&source(
            part,
            &format!("{MAX}        .vcc.current &= 0A..3mA\n"),
            "-1uA..0A",
        ));
        assert!(messages(&s).is_empty(), "{:?}", messages(&s));
        assert_eq!(range(&s, "gate.vcc.current").hi(), 750e-9);
    }
}
