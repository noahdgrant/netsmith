//! Type parameters of a module, type arguments of a member, and the `support` section of a part
//! (ADR 0115).

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const BUCK: &str = "\
module Buck<Ic: BuckIc> provides BuckConverter {
    .ic: Ic
    .ic.vin ~ .vin
}
";

fn only(src: &str) -> Definition {
    parse(src).unwrap().definitions.remove(0)
}

fn err(src: &str) -> String {
    parse(src).unwrap_err().message
}

fn decl(def: &Definition, at: usize) -> &Decl {
    match &def.items[at] {
        Item::Decl(d) => d,
        other => panic!("not a declaration: {other:?}"),
    }
}

#[test]
fn a_module_states_its_parameters_and_their_bounds() {
    let def = only(BUCK);
    assert_eq!(def.kind, DefKind::Module);
    assert_eq!(def.name.name, "Buck");
    let params: Vec<(&str, &str)> = def
        .params
        .iter()
        .map(|p| (p.name.name.as_str(), p.bound.name.as_str()))
        .collect();
    assert_eq!(params, [("Ic", "BuckIc")]);
    assert_eq!(def.provides[0].name, "BuckConverter");
    // The parameter is a type like any other inside the module.
    assert_eq!(decl(&def, 0).ty.segments[0].name, "Ic");
}

#[test]
fn several_parameters_are_separated_by_commas() {
    let def = only("module Pair<A: BuckIc, B: LinearRegulator> {\n}\n");
    let names: Vec<&str> = def.params.iter().map(|p| p.name.name.as_str()).collect();
    assert_eq!(names, ["A", "B"]);
    assert_eq!(def.params[1].bound.name, "LinearRegulator");
}

#[test]
fn a_member_names_the_arguments_of_its_type() {
    let def = only(
        "board B {\n    .reg: Buck<Lt8641> { .vout.voltage &= 3.3V }\n    .two: Pair<Zbk2, Zbk9>\n}\n",
    );
    let reg = decl(&def, 0);
    assert_eq!(reg.ty.segments[0].name, "Buck");
    let args: Vec<&str> = reg
        .ty
        .args
        .iter()
        .map(|a| a.segments[0].name.as_str())
        .collect();
    assert_eq!(args, ["Lt8641"]);
    assert!(reg.body.is_some());
    assert_eq!(decl(&def, 1).ty.args.len(), 2);
    // A type with no arguments has none, as before.
    let plain = only("board B {\n    .r: Rail\n}\n");
    assert!(decl(&plain, 0).ty.args.is_empty());
}

#[test]
fn a_comparison_is_still_two_characters() {
    // `<` and `>` are tokens of their own now; `<=` and `>=` are still the relations.
    let def = only("part P {\n    .a: Voltage\n    .a <= 3V\n    .a >= 1V\n}\n");
    let ops: Vec<CmpOp> = def
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Constraint(c) => Some(c.op),
            _ => None,
        })
        .collect();
    assert_eq!(ops, [CmpOp::Le, CmpOp::Ge]);
}

#[test]
fn only_a_module_takes_type_parameters() {
    assert_eq!(
        err("part Lt8641<Ic: BuckIc> {\n}\n"),
        "only a module takes type parameters; a part does not (`module Buck<Ic: BuckIc>`)"
    );
    assert_eq!(
        err("board B<Ic: BuckIc> {\n}\n"),
        "only a module takes type parameters; a board does not (`module Buck<Ic: BuckIc>`)"
    );
}

#[test]
fn a_parameter_has_a_bound() {
    assert!(
        err("module Buck<Ic> {\n}\n")
            .contains("`:` and the generic that bounds `Ic`, as in `Ic: BuckIc`"),
        "{}",
        err("module Buck<Ic> {\n}\n")
    );
    assert!(err("module Buck<Ic: BuckIc {\n}\n").contains("`>` or `,` after a type parameter"));
    assert!(
        err("board B {\n    .r: Buck<Lt8641\n}\n").contains("`>` or `,` after a type argument")
    );
}

#[test]
fn a_part_holds_a_support_section() {
    let def = only(
        "part Chip {\n    pins {\n        .bst passive \"BST\" \"1\"\n    }\n    support {\n        .boost: Capacitor { .capacitance &= 100nF }\n        .bst ~> .boost ~> .sw\n        .boost.max_voltage >= 6V\n    }\n}\n",
    );
    let Item::Support(support) = &def.items[1] else {
        panic!("{:?}", def.items[1]);
    };
    assert_eq!(support.items.len(), 3);
    assert!(matches!(support.items[0], Item::Decl(_)));
    assert!(matches!(support.items[1], Item::Connect(_)));
    assert!(matches!(support.items[2], Item::Constraint(_)));
    // `support` is a word only before `{`: a member may still be called `support`.
    let named = only("part Chip {\n    .support: Voltage\n}\n");
    assert!(matches!(named.items[0], Item::Decl(_)));
}

#[test]
fn the_printer_writes_parameters_arguments_and_support_back() {
    let src = "\
module Buck<Ic: BuckIc> provides BuckConverter {
    .ic: Ic
}

board B {
    .reg: Buck<Lt8641>
}

part Chip {
    support {
        .boost: Capacitor
        .bst ~> .boost ~> .sw
    }
}
";
    let printed = parse(src).unwrap().to_source();
    assert!(
        printed.contains("module Buck<Ic: BuckIc> provides BuckConverter {"),
        "{printed}"
    );
    assert!(printed.contains(".reg: Buck<Lt8641>"), "{printed}");
    assert!(printed.contains("support {"), "{printed}");
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}
