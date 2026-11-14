//! What the attributes of a project's own definition do in the design (ADR 0125): a `#[bank]`
//! is one part for each of its count, and `#[kind(...)]` and `#[value(...)]` reach the
//! design's parts. The parts are invented for these tests.

use netsmith_check::{Loader, check};
use netsmith_core::PartKind;
use netsmith_design::{Designed, design_board};
use netsmith_solve::{Catalogs, solve};

fn designed(src: &str) -> Designed {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::empty());
    design_board(&c.boards[0], &s).unwrap_or_else(|e| panic!("{e:?}"))
}

const CELL: &str = "\
#[bank]
#[kind(capacitor)]
#[value(\"cell\")]
generic Cell {
    .designator &= \"C\"
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    #[role(count)]
    .cells: Number
}
";

#[test]
fn a_project_bank_is_one_part_for_each_of_its_count() {
    let src = format!(
        "{CELL}module Pack {{\n    .c: Cell {{ .cells &= 3 }}\n}}\nboard B {{\n    .p: Pack\n}}\n"
    );
    let l = designed(&src);
    let parts: Vec<(&str, &str)> = l
        .design
        .parts
        .iter()
        .map(|p| (p.refdes.as_str(), p.value.as_str()))
        .collect();
    assert_eq!(parts, [("C1", "cell"), ("C2", "cell"), ("C3", "cell")]);
    assert!(
        l.design
            .parts
            .iter()
            .all(|p| p.declared_kind == Some(PartKind::Capacitor))
    );
}

#[test]
fn the_same_generic_without_bank_is_one_part() {
    let src = format!(
        "{}module Pack {{\n    .c: Cell {{ .cells &= 3 }}\n}}\nboard B {{\n    .p: Pack\n}}\n",
        CELL.replace("#[bank]\n", "")
    );
    assert_eq!(designed(&src).design.parts.len(), 1);
}
