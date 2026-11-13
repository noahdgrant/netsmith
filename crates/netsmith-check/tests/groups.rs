//! Groups of identical members, `N x Kind` (ADR 0119): what the checker says about a path that
//! names a whole group or an index past its end, and a group's members in a physical file.

use netsmith_check::*;

const HEAD: &str = "\
import std::parts::passive::Capacitor
import std::sources::Rail

";

const BOARD: &str = "\
board B {
    .v: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
    }
    .decoupling: 4 x Capacitor { .capacitance &= 100nF +/- 10% }
    .v.pos ~> .decoupling ~> .v.neg
";

fn run(extra: &str, physical: &str) -> Checked {
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", physical);
    check("t.nsm", &format!("{HEAD}{BOARD}{extra}}}\n"), &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn a_group_is_four_members_and_each_may_be_narrowed() {
    let c = run("    .decoupling[1].package &= \"0603\"\n", "");
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.boards[0].model;
    assert_eq!(
        m.domain_at("decoupling[1].package"),
        Some(&Domain::Text("0603".into()))
    );
    assert!(m.field_at("decoupling[3].capacitance").is_some());
    assert!(m.field_at("decoupling[4].capacitance").is_none());
}

#[test]
fn a_path_to_the_whole_group_names_its_members() {
    let c = run("    .decoupling.package &= \"0603\"\n", "");
    assert_eq!(
        messages(&c),
        ["`.decoupling` is a group of 4, not one member: name one, `.decoupling[0]` to `.decoupling[3]`; a statement in the group's block holds for every member"]
    );
}

#[test]
fn an_index_past_the_last_member_says_how_many_there_are() {
    let c = run("    .decoupling[4].package &= \"0603\"\n", "");
    assert_eq!(
        messages(&c),
        ["`.decoupling` is a group of 4, `.decoupling[0]` to `.decoupling[3]`, and has no member `[4]`"]
    );
}

#[test]
fn a_physical_file_places_one_member_by_its_index() {
    let c = run(
        "",
        "physical for B {\n    .decoupling[2].place.side &= \"bottom\"\n}\n",
    );
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(
        c.boards[0].model.domain_at("decoupling[2].place.side"),
        Some(&Domain::Text("bottom".into()))
    );
    let c = run("", "physical for B {\n    .decoupling.place.side &= \"bottom\"\n}\n");
    assert_eq!(
        messages(&c),
        ["`.decoupling` is a group of 4, not one member: name one, `.decoupling[0]` to `.decoupling[3]`; a statement in the group's block holds for every member"]
    );
}

#[test]
fn an_error_of_the_expansion_is_reported_at_its_statement() {
    let c = run("    .decoupling ~ .v.pos\n", "");
    assert_eq!(
        messages(&c),
        ["`.decoupling` is a group of 4, and a group in a chain stands alone between the chain's two ends, each member in parallel (`.v.pos ~> .decoupling ~> .v.neg`); join one member by its index, `.decoupling[0]` to `.decoupling[3]`"]
    );
    let d = &c.diagnostics[0];
    assert_eq!(&c.sources.get(d.source).text[d.span.start..d.span.end], ".decoupling");
}
