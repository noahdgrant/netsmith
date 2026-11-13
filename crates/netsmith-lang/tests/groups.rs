//! Groups of identical members, `N x Kind` (ADR 0119): the syntax, the printer, and the
//! expansion into indexed members that the checker reads.

use netsmith_lang::ast::*;
use netsmith_lang::{expand_groups, parse};

fn items(src: &str) -> Vec<Item> {
    parse(src).unwrap().definitions.remove(0).items
}

fn err(src: &str) -> String {
    parse(src).unwrap_err().message
}

/// The file after expansion, printed, and the expansion's errors.
fn expanded(src: &str) -> (String, Vec<String>) {
    let mut file = parse(src).unwrap();
    let errors = expand_groups(&mut file);
    (
        file.to_source(),
        errors.into_iter().map(|e| e.message).collect(),
    )
}

const NAMED: &str = "\
board B {
    /// The decoupling of the rail.
    .decoupling: 3 x Capacitor { .capacitance &= 100nF +/- 10% }
    .decoupling[1].package &= \"0603\"
    .v.pos ~> .decoupling ~> .v.neg
}
";

#[test]
fn a_named_group_has_a_count_and_its_members_are_named_by_index() {
    let items = items(NAMED);
    let Item::Decl(d) = &items[0] else {
        panic!("{:?}", items[0])
    };
    assert_eq!(d.count.map(|c| c.n), Some(3));
    assert_eq!(&NAMED[d.count.unwrap().span.start..d.count.unwrap().span.end], "3 x");
    assert_eq!(d.ty.to_source(), "Capacitor");
    let Item::Constraint(c) = &items[1] else {
        panic!("{:?}", items[1])
    };
    let Expr::Path(p) = &c.lhs else { panic!() };
    let names: Vec<&str> = p.segments.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["decoupling[1]", "package"]);
    // The index is part of the name, and its span covers the brackets.
    assert_eq!(&NAMED[p.segments[0].span.start..p.segments[0].span.end], "decoupling[1]");
}

#[test]
fn a_group_prints_back_to_its_source() {
    let printed = parse(NAMED).unwrap().to_source();
    assert!(printed.contains(".decoupling: 3 x Capacitor {"), "{printed}");
    assert!(printed.contains(".decoupling[1].package &= \"0603\""), "{printed}");
    assert_eq!(parse(&printed).unwrap().to_source(), printed);

    let inline = "board B {\n    .v.pos ~> 2 x Capacitor { .capacitance &= 10uF } ~> .v.neg\n}\n";
    let printed = parse(inline).unwrap().to_source();
    assert!(printed.contains(".v.pos ~> 2 x Capacitor {"), "{printed}");
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn a_named_group_becomes_its_members_in_index_order_and_a_chain_for_each() {
    let (printed, errors) = expanded(NAMED);
    assert!(errors.is_empty(), "{errors:?}");
    let decls: Vec<&str> = printed
        .lines()
        .filter(|l| l.contains(": Capacitor"))
        .map(str::trim)
        .collect();
    assert_eq!(
        decls,
        [
            ".decoupling[0]: Capacitor {",
            ".decoupling[1]: Capacitor {",
            ".decoupling[2]: Capacitor {"
        ]
    );
    // Each member has the group's body, and the first its documentation: one note.
    assert_eq!(printed.matches("/// The decoupling of the rail.").count(), 1);
    assert!(printed.contains("/// The decoupling of the rail.\n    .decoupling[0]"));
    assert_eq!(printed.matches(".capacitance &= 100nF +/- 10%").count(), 3);
    let chains: Vec<&str> = printed
        .lines()
        .filter(|l| l.contains("~>"))
        .map(str::trim)
        .collect();
    assert_eq!(
        chains,
        [
            ".v.pos ~> .decoupling[0] ~> .v.neg",
            ".v.pos ~> .decoupling[1] ~> .v.neg",
            ".v.pos ~> .decoupling[2] ~> .v.neg"
        ]
    );
}

#[test]
fn a_group_in_a_chain_is_named_from_its_kind_and_the_chains_ends() {
    let src = "board B {\n    .v3v3.pos ~> 2 x FerriteBead { .impedance &= 600ohm } ~> .load.vin\n}\n";
    let items = items(src);
    let Item::Connect(c) = &items[0] else {
        panic!()
    };
    assert_eq!(c.groups.len(), 1);
    assert_eq!(c.groups[0].count.n, 2);
    assert_eq!(c.rest[0].1.to_source(), ".ferrite_bead_v3v3_pos_load_vin");
    let (printed, errors) = expanded(src);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        printed.contains(".ferrite_bead_v3v3_pos_load_vin[1]: FerriteBead {"),
        "{printed}"
    );
    assert!(
        printed.contains(".v3v3.pos ~> .ferrite_bead_v3v3_pos_load_vin[1] ~> .load.vin"),
        "{printed}"
    );
}

#[test]
fn a_count_is_a_whole_number_of_at_least_two_written_with_spaces() {
    assert_eq!(
        err("board B {\n    .c: 1 x Capacitor\n}\n"),
        "a group has at least two members; `1 x` is not one, so declare the member alone (`.c: Capacitor`)"
    );
    assert_eq!(
        err("board B {\n    .c: 2.5 x Capacitor\n}\n"),
        "the count of a group is a whole number with no unit, such as `4 x`, not `2.5`"
    );
    assert_eq!(
        err("board B {\n    .c: 4x Capacitor\n}\n"),
        "write the count of a group with a space before and after the `x`: `4 x Capacitor`"
    );
    assert_eq!(
        err("board B {\n    .c: 5000 x Capacitor\n}\n"),
        "a group has at most 1000 members, and `5000` is more"
    );
    assert_eq!(
        err("board B {\n    .m.c: 2 x Capacitor\n}\n"),
        "a group is declared by one name of its own, such as `.c: 2 x Capacitor`, not by a path into a member"
    );
    assert_eq!(
        err("board B {\n    .c[x].a ~ .b\n}\n"),
        "expected the index of a member of a group, a whole number such as `[0]`, found `x`"
    );
}

#[test]
fn a_group_in_a_chain_stands_alone_between_two_ends() {
    let shape = "a group written in a chain stands alone between the chain's two ends, each member in parallel: `.v.pos ~> 2 x Capacitor { ... } ~> .v.neg`";
    assert_eq!(
        err("board B {\n    .v.pos ~> 2 x Capacitor ~> .r ~> .v.neg\n}\n"),
        shape
    );
    assert_eq!(
        err("board B {\n    .v.pos ~> 2 x Capacitor\n}\n"),
        shape
    );
    assert_eq!(
        err("board B {\n    .v.pos ~ 2 x Capacitor ~ .v.neg\n}\n"),
        shape
    );
}

#[test]
fn a_named_group_in_any_other_chain_is_refused_with_its_members() {
    let (_, errors) = expanded(
        "board B {\n    .caps: 2 x Capacitor\n    .v.pos ~> .caps ~> .r ~> .v.neg\n    .caps ~ .v.pos\n}\n",
    );
    let message = "`.caps` is a group of 2, and a group in a chain stands alone between the chain's two ends, each member in parallel (`.v.pos ~> .caps ~> .v.neg`); join one member by its index, `.caps[0]` to `.caps[1]`";
    assert_eq!(errors, [message, message]);
}

#[test]
fn two_groups_in_chains_with_one_name_are_refused() {
    let (printed, errors) = expanded(
        "board B {\n    .v.pos ~> 2 x Capacitor ~> .v.neg\n    .v.pos ~> 3 x Capacitor ~> .v.neg\n}\n",
    );
    assert_eq!(
        errors,
        ["a second group of `Capacitor` between `.v.pos` and `.v.neg`: a group in a chain is named from its kind and its ends (`capacitor_v_pos_v_neg`), so two would have one name; declare each group by a name of its own (`.bulk: 2 x Capacitor { ... }`, then `.v.pos ~> .bulk ~> .v.neg`)"]
    );
    // The refused group declares nothing, so no second error follows.
    assert!(!printed.contains("capacitor_v_pos_v_neg[2]"), "{printed}");
}
