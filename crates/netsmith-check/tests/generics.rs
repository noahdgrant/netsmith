//! A `generic` and the parts and modules that provide it (ADR 0068): what the checker makes of
//! a use, what a provider owes, and which provider is in scope and in what order. Every part,
//! number and name here is invented.

use netsmith_check::*;

const RF: &str = "\
/// A coaxial receptacle that carries an antenna lead off the board.
generic CoaxReceptacle {
    .designator &= \"J\"

    pins {
        .signal passive
        .shield passive
    }

    .impedance: Resistance
    .impedance &= 50ohm
    .max_frequency: Frequency
}
";

const ACME: &str = "\
import src::rf::CoaxReceptacle

part AcmeMicroCoax provides CoaxReceptacle {
    pins {
        .signal     passive  \"SIGNAL\"  \"1\"
        .shield     passive  \"GND\"     \"2\" \"3\"
    }
    .max_frequency &= 6GHz

    .package: Package
    catalog acme_micro_coax { .mpn, .package }
}
";

const BOARD: &str = "\
import src::rf::CoaxReceptacle
import src::acme::AcmeMicroCoax

board Feed {
    .rf: CoaxReceptacle { .max_frequency >= 1.61GHz }
}
";

fn loader(files: &[(&str, &str)]) -> Loader {
    let mut loader = Loader::std();
    for (module, text) in files {
        loader.insert(module, &format!("{}.nsm", module.replace("::", "/")), text);
    }
    loader
}

fn run(root: &str, files: &[(&str, &str)]) -> Checked {
    check("test.nsm", root, &loader(files))
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn ok(c: &Checked) {
    assert!(c.is_ok(), "{}", c.render());
}

fn uses(c: &Checked) -> &[GenericUse] {
    &c.board("Feed").unwrap().model.uses
}

/// A generic that says only "a part, with letters" has an empty `pins` table: a part provides it
/// with its own table (std's `Connector`), and a module cannot.
#[test]
fn a_generic_with_an_empty_pins_table_is_provided_by_a_part_with_its_own_pins() {
    let generic = "generic Marked {\n    .designator &= \"J\"\n    pins {\n    }\n}\n";
    let part = "import src::marked::Marked\npart Mine provides Marked {\n    pins {\n        .a passive \"A\" \"1\"\n    }\n}\n";
    let board = "import src::mine::Mine\nboard B {\n    .m: Mine\n}\n";
    let c = run(board, &[("src::marked", generic), ("src::mine", part)]);
    ok(&c);
    let module = "import src::marked::Marked\nmodule Wrapper provides Marked {\n}\n";
    let board =
        "import src::marked::Marked\nimport src::wrap::Wrapper\nboard B {\n    .m: Marked\n}\n";
    let c = run(board, &[("src::marked", generic), ("src::wrap", module)]);
    assert!(
        messages(&c)
            .iter()
            .any(|m| m.contains("`Wrapper` is a module, and `Marked` has a `pins` table")),
        "{:?}",
        messages(&c)
    );
}

#[test]
fn a_use_of_a_generic_is_made_from_the_provider_in_scope() {
    let c = run(BOARD, &[("src::rf", RF), ("src::acme", ACME)]);
    ok(&c);
    let model = &c.board("Feed").unwrap().model;
    let rf = model.inst_at("rf").unwrap();
    let inst = model.instance(rf);
    assert_eq!(inst.type_name, "AcmeMicroCoax");
    assert_eq!(inst.placed_as.as_deref(), Some("CoaxReceptacle"));
    assert_eq!(inst.provides, ["CoaxReceptacle"]);
    assert_eq!(inst.kind, InstKind::Part);
    let u = &uses(&c)[0];
    assert_eq!(u.path, "rf");
    assert_eq!(u.generic, "CoaxReceptacle");
    assert_eq!(u.chosen, 0);
    let ids: Vec<&str> = u.candidates.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, ["src::acme::AcmeMicroCoax"]);
}

#[test]
fn the_provider_has_the_generics_members_and_its_own_numbers() {
    let c = run(BOARD, &[("src::rf", RF), ("src::acme", ACME)]);
    ok(&c);
    let model = &c.board("Feed").unwrap().model;
    // The generic's narrowing and the provider's both hold.
    let range = |path: &str| match model.domain_at(path) {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("`{path}` is {other:?}"),
    };
    assert_eq!(range("rf.impedance"), (50.0, 50.0));
    assert_eq!(range("rf.max_frequency"), (6.0e9, 6.0e9));
    assert!(model.field_at("rf.mpn").is_some());
    // The generic's pins are the provider's, and its table gives them pads.
    let pin = model.inst_at("rf.shield").unwrap();
    assert_eq!(model.pin_facts(pin).unwrap().pads, ["2", "3"]);
}

#[test]
fn the_requirement_on_the_use_is_a_relation_of_the_provider() {
    let c = run(BOARD, &[("src::rf", RF), ("src::acme", ACME)]);
    ok(&c);
    let model = &c.board("Feed").unwrap().model;
    let shown: Vec<String> = model
        .relations()
        .iter()
        .map(|r| {
            format!(
                "{} {} {}",
                model.show_expr(&r.lhs),
                r.op.symbol(),
                model.show_expr(&r.rhs)
            )
        })
        .collect();
    assert!(
        shown.iter().any(|s| s.starts_with("rf.max_frequency >= ")),
        "{shown:?}"
    );
}

#[test]
fn a_required_pin_of_the_generic_is_required_of_the_providers_row() {
    let generic = RF.replace(".signal passive\n", ".signal passive required\n");
    let board = "\
import std::sources::Rail
import src::rf::CoaxReceptacle
import src::acme::AcmeMicroCoax

board Feed {
    .supply: Rail
    .rf: CoaxReceptacle
    .rf.signal ~ .supply.pos
}
";
    let c = run(board, &[("src::rf", &generic), ("src::acme", ACME)]);
    ok(&c);
    let model = &c.board("Feed").unwrap().model;
    let pin = model.inst_at("rf.signal").unwrap();
    assert_eq!(
        model.pin_facts(pin).unwrap().requirement,
        Some(Requirement::Required)
    );
    // Left unconnected, the pin is refused as any required pin is.
    let c = run(BOARD, &[("src::rf", &generic), ("src::acme", ACME)]);
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.starts_with("`rf.signal` is a passive pin that must be connected")),
        "{m:?}"
    );
}

#[test]
fn a_row_that_says_the_opposite_of_the_generics_requirement_is_refused() {
    let generic = RF.replace(".signal passive\n", ".signal passive required\n");
    let acme = ACME.replace("\"SIGNAL\"  \"1\"", "\"SIGNAL\"  \"1\" optional");
    let c = run(BOARD, &[("src::rf", &generic), ("src::acme", &acme)]);
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains("`optional` is for a power or input pin")
            || m.contains("`.signal` is `required` in `CoaxReceptacle`, which `AcmeMicroCoax` provides, but this row says `optional`")),
        "{m:?}"
    );
}

#[test]
fn naming_the_provider_as_the_type_places_it_plainly() {
    let board = "\
import src::acme::AcmeMicroCoax

board Feed {
    .rf: AcmeMicroCoax
    .rf.package &= \"ACME_MC_1\"
}
";
    let c = run(board, &[("src::rf", RF), ("src::acme", ACME)]);
    ok(&c);
    let model = &c.board("Feed").unwrap().model;
    let inst = model.instance(model.inst_at("rf").unwrap());
    assert_eq!(inst.type_name, "AcmeMicroCoax");
    assert_eq!(inst.placed_as, None);
    assert!(model.uses.is_empty());
    // Forced by type, a board may reach what only this provider has.
    assert!(model.field_at("rf.package").is_some());
}

#[test]
fn a_path_through_a_use_to_what_the_generic_does_not_declare_is_an_error() {
    let board = "\
import src::rf::CoaxReceptacle
import src::acme::AcmeMicroCoax

board Feed {
    .rf: CoaxReceptacle
    .rf.package &= \"ACME_MC_1\"
}
";
    let c = run(board, &[("src::rf", RF), ("src::acme", ACME)]);
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].contains("`package` is not declared by `CoaxReceptacle`"),
        "{}",
        m[0]
    );
    assert!(m[0].contains("holds for every provider"), "{}", m[0]);
    assert!(
        m[0].contains("name a provider, `AcmeMicroCoax`, as the type"),
        "{}",
        m[0]
    );
}

#[test]
fn what_every_part_has_is_reachable_through_a_use() {
    let board = "\
import src::rf::CoaxReceptacle
import src::acme::AcmeMicroCoax

board Feed {
    .rf: CoaxReceptacle
    .rf.height &= 5mm
    .rf.assembly &= AssemblyMode::Hand
}
";
    let c = run(board, &[("src::rf", RF), ("src::acme", ACME)]);
    ok(&c);
}

#[test]
fn a_provider_may_not_declare_a_member_of_the_generic_again() {
    let acme = ACME.replace(
        "    .max_frequency &= 6GHz\n",
        "    .max_frequency: Frequency\n    .max_frequency &= 6GHz\n",
    );
    let c = run(BOARD, &[("src::rf", RF), ("src::acme", &acme)]);
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert_eq!(
        m[0],
        "`max_frequency` is declared by `CoaxReceptacle`, which this part provides; do not declare it again: narrow it with `.max_frequency &= ...`, or state it with a relation"
    );
}

#[test]
fn a_provider_that_declares_an_interface_of_the_generic_again_is_told_to_use_statements() {
    let generic = "\
import std::sources::Rail
generic Supplied {
    .vin: Rail
}
";
    let provider = "\
import std::sources::Rail
import src::g::Supplied

module Supplier provides Supplied {
    .vin: Rail { .voltage &= 5V }
}
";
    let c = run(provider, &[("src::g", generic)]);
    // The provider is read when its own file is checked, or when something instantiates it.
    let board = "\
import src::g::Supplied
import src::p::Supplier

board Feed {
    .s: Supplied
}
";
    let c2 = run(board, &[("src::g", generic), ("src::p", provider)]);
    for c in [c, c2] {
        let m = messages(&c);
        assert!(
            m.iter().any(|m| m
                .contains("`vin` is declared by `Supplied`, which this module provides")
                && m.contains("state its values with statements, `.vin.<field> &= ...`")),
            "{m:?}"
        );
    }
}

#[test]
fn a_provider_with_the_wrong_pin_kind_is_refused_at_its_row() {
    let acme = ACME.replace(".signal     passive", ".signal     input");
    let c = run(BOARD, &[("src::rf", RF), ("src::acme", &acme)]);
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.starts_with("`.signal` is a passive pin in `CoaxReceptacle`, which `AcmeMicroCoax` provides, but this row says an input")),
        "{m:?}"
    );
}

#[test]
fn a_provider_that_leaves_a_pin_without_a_row_is_refused() {
    let acme = ACME.replace("        .shield     passive  \"GND\"     \"2\" \"3\"\n", "");
    let c = run(BOARD, &[("src::rf", RF), ("src::acme", &acme)]);
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains("`pins` of `AcmeMicroCoax` has no row for `.shield`, which `CoaxReceptacle` declares as a passive pin")),
        "{m:?}"
    );
}

#[test]
fn a_provider_with_no_pin_table_at_all_is_refused() {
    let acme = "\
import src::rf::CoaxReceptacle
part Bare provides CoaxReceptacle {
    .package: Package
}
";
    let board = "import src::rf::CoaxReceptacle\nimport src::acme::Bare\nboard Feed {\n .rf: CoaxReceptacle\n}\n";
    let c = run(board, &[("src::rf", RF), ("src::acme", acme)]);
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains("`Bare` provides `CoaxReceptacle`, which has the pin `.signal`, and this part has no `pins` table")),
        "{m:?}"
    );
}

#[test]
fn a_provider_may_name_the_generics_pins_and_values_with_aliases_and_equations() {
    let other = "\
import src::rf::CoaxReceptacle

part OtherCoax provides CoaxReceptacle {
    pins {
        .rf      passive  \"RF\"   \"1\"
        .shield  passive  \"GND\"  \"2\"
    }
    .signal ~ .rf
    .fmax: Frequency
    .fmax &= 3GHz
    .max_frequency == .fmax

    .package: Package
    catalog other_coax { .mpn, .package }
}
";
    let board = "import src::rf::CoaxReceptacle\nimport src::other::OtherCoax\nboard Feed {\n .rf: CoaxReceptacle\n}\n";
    let c = run(board, &[("src::rf", RF), ("src::other", other)]);
    ok(&c);
    let model = &c.board("Feed").unwrap().model;
    // The alias is joined to the pin that has the row.
    let signal = model.inst_at("rf.signal").unwrap();
    assert!(model.is_alias(signal));
}

#[test]
fn the_generics_narrowing_against_a_providers_contradicting_one_is_an_error() {
    let acme = ACME.replace(
        "    .max_frequency &= 6GHz\n",
        "    .max_frequency &= 6GHz\n    .impedance &= 75ohm\n",
    );
    let c = run(BOARD, &[("src::rf", RF), ("src::acme", &acme)]);
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.contains("contradiction") && m.contains("impedance")),
        "{m:?}"
    );
}

#[test]
fn the_providers_own_checks_apply_to_a_use() {
    let acme = ACME.replace(
        "    .max_frequency &= 6GHz\n",
        "    .max_frequency &= 6GHz\n    .nonsense &= 1\n",
    );
    let c = run(BOARD, &[("src::rf", RF), ("src::acme", &acme)]);
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.contains("`nonsense` is not a member of this AcmeMicroCoax")),
        "{m:?}"
    );
}

#[test]
fn the_generics_relations_hold_for_each_provider() {
    let generic = RF.replace(
        "    .max_frequency: Frequency\n",
        "    .max_frequency: Frequency\n    .max_frequency >= 1GHz\n",
    );
    let c = run(BOARD, &[("src::rf", &generic), ("src::acme", ACME)]);
    ok(&c);
    let model = &c.board("Feed").unwrap().model;
    let shown: Vec<String> = model
        .relations()
        .iter()
        .map(|r| {
            format!(
                "{} {} {}",
                model.show_expr(&r.lhs),
                r.op.symbol(),
                model.show_expr(&r.rhs)
            )
        })
        .collect();
    assert_eq!(
        shown
            .iter()
            .filter(|s| s.starts_with("rf.max_frequency >= "))
            .count(),
        2,
        "the generic's and the board's: {shown:?}"
    );
}

#[test]
fn no_provider_in_scope_is_an_error_at_the_use() {
    let board = "import src::rf::CoaxReceptacle\nboard Feed {\n    .rf: CoaxReceptacle\n}\n";
    let c = run(board, &[("src::rf", RF), ("src::acme", ACME)]);
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].starts_with("`rf` is a `CoaxReceptacle`, and no part or module in the files this file imports provides it"),
        "{}",
        m[0]
    );
    assert!(m[0].contains("import the file of a provider"), "{}", m[0]);
    let rendered = c.render();
    assert!(
        rendered.contains("test.nsm:3:5") || rendered.contains(".rf: CoaxReceptacle"),
        "{rendered}"
    );
}

#[test]
fn a_provider_in_a_file_the_board_does_not_import_is_not_a_candidate() {
    let second = "\
import src::rf::CoaxReceptacle

part BetaCoax provides CoaxReceptacle {
    pins {
        .signal  passive  \"S\"  \"1\"
        .shield  passive  \"G\"  \"2\"
    }
    .max_frequency &= 3GHz
}
";
    let files = [("src::rf", RF), ("src::acme", ACME), ("src::beta", second)];
    let one = run(BOARD, &files);
    ok(&one);
    assert_eq!(uses(&one)[0].candidates.len(), 1);
    // Importing the second file's definition adds its provider; the import closure is the list.
    let both = BOARD.replace(
        "import src::acme::AcmeMicroCoax",
        "import src::acme::AcmeMicroCoax\nimport src::beta::BetaCoax",
    );
    let two = run(&both, &files);
    ok(&two);
    let ids: Vec<&str> = uses(&two)[0]
        .candidates
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(ids, ["src::acme::AcmeMicroCoax", "src::beta::BetaCoax"]);
}

#[test]
fn a_provider_reached_through_another_file_is_a_candidate() {
    // `src::pack` imports both providers; the board imports only `src::pack`.
    let beta = "\
import src::rf::CoaxReceptacle
part BetaCoax provides CoaxReceptacle {
    pins {
        .signal  passive  \"S\"  \"1\"
        .shield  passive  \"G\"  \"2\"
    }
}
";
    let pack = "import src::acme::AcmeMicroCoax\nimport src::beta::BetaCoax\nmodule Pack {}\n";
    let board = "import src::rf::CoaxReceptacle\nimport src::pack::Pack\nboard Feed {\n .rf: CoaxReceptacle\n}\n";
    let c = run(
        board,
        &[
            ("src::rf", RF),
            ("src::acme", ACME),
            ("src::beta", beta),
            ("src::pack", pack),
        ],
    );
    ok(&c);
    assert_eq!(uses(&c)[0].candidates.len(), 2);
}

#[test]
fn the_order_is_by_module_then_name_and_not_by_where_a_definition_is_written() {
    let make =
        |first: &str, second: &str| format!("import src::rf::CoaxReceptacle\n{first}\n{second}\n");
    let part = |name: &str| {
        format!(
            "part {name} provides CoaxReceptacle {{\n    pins {{\n        .signal passive \"S\" \"1\"\n        .shield passive \"G\" \"2\"\n    }}\n}}\n"
        )
    };
    let board = "import src::rf::CoaxReceptacle\nimport src::two::Zed\nimport src::one::Mid\nboard Feed {\n .rf: CoaxReceptacle\n}\n";
    let a = run(
        board,
        &[
            ("src::rf", RF),
            ("src::one", &make(&part("Mid"), &part("Alpha"))),
            ("src::two", &make(&part("Zed"), &part("Beta"))),
        ],
    );
    let b = run(
        board,
        &[
            ("src::rf", RF),
            ("src::one", &make(&part("Alpha"), &part("Mid"))),
            ("src::two", &make(&part("Beta"), &part("Zed"))),
        ],
    );
    ok(&a);
    ok(&b);
    let ids = |c: &Checked| -> Vec<String> {
        uses(c)[0].candidates.iter().map(|p| p.id.clone()).collect()
    };
    assert_eq!(
        ids(&a),
        [
            "src::one::Alpha",
            "src::one::Mid",
            "src::two::Beta",
            "src::two::Zed"
        ]
    );
    assert_eq!(ids(&a), ids(&b));
}

#[test]
fn a_standard_provider_comes_before_a_projects() {
    // A project file named so that it sorts before `std::...`: the standard library still wins.
    let std_like = "\
import src::rf::CoaxReceptacle
part Mine provides CoaxReceptacle {
    pins {
        .signal passive \"S\" \"1\"
        .shield passive \"G\" \"2\"
    }
}
";
    let board = "import src::rf::CoaxReceptacle\nimport src::aaa::Mine\nboard Feed {\n .rf: CoaxReceptacle\n}\n";
    let mut l = loader(&[("src::rf", RF), ("src::aaa", std_like)]);
    // A library file that provides it too, loaded from the standard library's folder.
    l.insert(
        "std::zzz",
        "std/zzz.nsm",
        "import src::rf::CoaxReceptacle\npart Library provides CoaxReceptacle {\n    pins {\n        .signal passive \"S\" \"1\"\n        .shield passive \"G\" \"2\"\n    }\n}\n",
    );
    let board = format!("{board}import std::zzz::Library\n");
    let c = check("test.nsm", &board, &l);
    ok(&c);
    let ids: Vec<&str> = uses(&c)[0]
        .candidates
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(ids, ["std::zzz::Library", "src::aaa::Mine"]);
}

#[test]
fn an_assignment_puts_the_named_provider_in_the_uses_place() {
    let beta = "\
import src::rf::CoaxReceptacle
part BetaCoax provides CoaxReceptacle {
    pins {
        .signal  passive  \"S\"  \"1\"
        .shield  passive  \"G\"  \"2\"
    }
    .max_frequency &= 3GHz
}
";
    let board = "import src::rf::CoaxReceptacle\nimport src::acme::AcmeMicroCoax\nimport src::beta::BetaCoax\nboard Feed {\n .rf: CoaxReceptacle\n}\n";
    let l = loader(&[("src::rf", RF), ("src::acme", ACME), ("src::beta", beta)]);
    let mut assignment = Assignment::new();
    assignment
        .entry("Feed".into())
        .or_default()
        .insert("rf".into(), "src::beta::BetaCoax".into());
    let c = check_with("test.nsm", board, &l, &assignment);
    ok(&c);
    let model = &c.board("Feed").unwrap().model;
    assert_eq!(
        model.instance(model.inst_at("rf").unwrap()).type_name,
        "BetaCoax"
    );
    assert_eq!(model.uses[0].chosen, 1);
    // And without it, the first.
    let c = check("test.nsm", board, &l);
    let model = &c.board("Feed").unwrap().model;
    assert_eq!(
        model.instance(model.inst_at("rf").unwrap()).type_name,
        "AcmeMicroCoax"
    );
    assert_eq!(model.uses[0].chosen, 0);
}

#[test]
fn elaborating_the_same_board_twice_gives_the_same_model() {
    let l = loader(&[("src::rf", RF), ("src::acme", ACME)]);
    let first = check("test.nsm", BOARD, &l);
    let second = check("test.nsm", BOARD, &l);
    ok(&first);
    let dump = |c: &Checked| format!("{:?}", c.board("Feed").unwrap().model);
    assert_eq!(dump(&first), dump(&second));
    let again = check_with("test.nsm", BOARD, &l, &Assignment::new());
    assert_eq!(dump(&first), dump(&again));
    // Elaborating a board among others, in one run, does not leak into it.
    let two = format!("{BOARD}\nboard Other {{\n .rf: CoaxReceptacle\n}}\n");
    let both = check("test.nsm", &two, &l);
    ok(&both);
    assert_eq!(dump(&first), dump(&both));
}

#[test]
fn a_part_that_provides_two_generics_has_the_members_of_both() {
    let rf = "\
generic RfConnector {
    .gain_class: Number
    pins {
        .signal passive
    }
}
";
    let acme = "\
import src::rf::CoaxReceptacle
import src::rfc::RfConnector

part Both provides CoaxReceptacle, RfConnector {
    pins {
        .signal passive \"S\" \"1\"
        .shield passive \"G\" \"2\"
    }
    .gain_class &= 3
}
";
    let board = "\
import src::rf::CoaxReceptacle
import src::rfc::RfConnector
import src::acme::Both

board Feed {
    .rf: CoaxReceptacle
    .other: RfConnector { .gain_class >= 2 }
}
";
    let c = run(
        board,
        &[("src::rf", RF), ("src::rfc", rf), ("src::acme", acme)],
    );
    ok(&c);
    let model = &c.board("Feed").unwrap().model;
    assert_eq!(
        model.instance(model.inst_at("rf").unwrap()).provides,
        ["CoaxReceptacle", "RfConnector"]
    );
    // One use of each, and the part is a candidate for both.
    let paths: Vec<&str> = model.uses.iter().map(|u| u.path.as_str()).collect();
    assert_eq!(paths, ["rf", "other"]);
    assert!(model.uses.iter().all(|u| u.candidates.len() == 1));
}

const BUCK: &str = "\
import std::sources::Rail

generic BuckConverter {
    .vin: Rail
    .vout: Rail
    /// The highest voltage the input pin survives.
    .vin_abs_max: Voltage
    .vin.peak <= .vin_abs_max
}
";

const BUCK_PROVIDERS: &str = "\
import src::buck::BuckConverter

module Zbk2Buck provides BuckConverter {
    .vin_abs_max &= 7V
    .vin.neg ~ .vout.neg
}

module Zbk9Buck provides BuckConverter {
    .vin_abs_max &= 65V
    .vin.neg ~ .vout.neg
}
";

#[test]
fn a_module_can_provide_a_module_shaped_generic() {
    let board = "\
import src::buck::BuckConverter
import src::ics::Zbk9Buck
import std::sources::Rail

board Converter {
    .input: Rail { .voltage &= 12V +/- 5% }
    .reg: BuckConverter { .vin_abs_max >= 40V }
    .input ~ .reg.vin
}
";
    let c = run(board, &[("src::buck", BUCK), ("src::ics", BUCK_PROVIDERS)]);
    ok(&c);
    let model = &c.board("Converter").unwrap().model;
    let reg = model.instance(model.inst_at("reg").unwrap());
    assert_eq!(reg.kind, InstKind::Module);
    assert_eq!(reg.type_name, "Zbk2Buck");
    let u = &model.uses[0];
    let names: Vec<&str> = u.candidates.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Zbk2Buck", "Zbk9Buck"]);
    assert!(u.candidates.iter().all(|p| p.kind == InstKind::Module));
    // The generic's relation is on the module provider's rail.
    let shown: Vec<String> = model
        .relations()
        .iter()
        .map(|r| {
            format!(
                "{} {} {}",
                model.show_expr(&r.lhs),
                r.op.symbol(),
                model.show_expr(&r.rhs)
            )
        })
        .collect();
    assert!(
        shown.iter().any(|s| s == "reg.vin.peak <= reg.vin_abs_max"),
        "{shown:?}"
    );
}

#[test]
fn the_inside_of_a_module_provider_is_still_not_reachable() {
    let ics = "\
import src::buck::BuckConverter
import std::parts::passive::Resistor

module Zbk2Buck provides BuckConverter {
    .vin_abs_max &= 7V
    .sense: Resistor
}
";
    let board = "\
import src::buck::BuckConverter
import src::ics::Zbk2Buck

board Converter {
    .reg: BuckConverter
    .reg.sense.resistance &= 1ohm
}
";
    let c = run(board, &[("src::buck", BUCK), ("src::ics", ics)]);
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.contains("`sense` is not declared by `BuckConverter`")),
        "{m:?}"
    );
}

#[test]
fn providers_of_one_generic_are_all_parts_or_all_modules() {
    let generic = "generic Thing {\n    .size: Number\n}\n";
    let part = "import src::g::Thing\npart APart provides Thing {\n    .size &= 1\n}\n";
    let module = "import src::g::Thing\nmodule BModule provides Thing {\n    .size &= 2\n}\n";
    let board = "import src::g::Thing\nimport src::a::APart\nimport src::b::BModule\nboard B {\n .t: Thing\n}\n";
    let c = run(
        board,
        &[("src::g", generic), ("src::a", part), ("src::b", module)],
    );
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m
            .contains("`BModule` is a module, but `APart`, which provides `Thing` too, is a part")
            && m.contains(
                "all providers of a generic are parts, all are modules, or all are interfaces"
            )),
        "{m:?}"
    );
}

#[test]
fn a_module_does_not_provide_a_generic_that_has_a_pin_table() {
    let ics = "import src::rf::CoaxReceptacle\nmodule Wrapper provides CoaxReceptacle {\n    .max_frequency &= 6GHz\n}\n";
    let board = "import src::rf::CoaxReceptacle\nimport src::ics::Wrapper\nboard B {\n .rf: CoaxReceptacle\n}\n";
    let c = run(board, &[("src::rf", RF), ("src::ics", ics)]);
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains("`Wrapper` is a module, and `CoaxReceptacle` has a `pins` table, which only a part provides")),
        "{m:?}"
    );
}

#[test]
fn only_a_generic_is_provided() {
    // `Diode` is a part of the standard library that is not a generic (`Resistor` is one,
    // ADR 0068 stage 2).
    let acme =
        "import std::parts::diode::Diode\npart Fake provides Diode {\n    .package: Package\n}\n";
    let board = "import src::acme::Fake\nboard B {\n .f: Fake\n}\n";
    let c = run(board, &[("src::acme", acme)]);
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.starts_with("`Diode` is a part, and only a `generic` is provided")),
        "{m:?}"
    );
}

#[test]
fn an_interface_is_not_a_generic_to_provide() {
    let acme = "import std::sources::Rail\nmodule Fake provides Rail {\n}\n";
    let board = "import src::acme::Fake\nboard B {\n .f: Fake\n}\n";
    let c = run(board, &[("src::acme", acme)]);
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.starts_with("`Rail` is an interface, and only a `generic` is provided")),
        "{m:?}"
    );
}

#[test]
fn a_generic_holds_no_parts_and_no_modules() {
    let generic = "import std::parts::passive::Resistor\ngeneric Bad {\n    .r: Resistor\n}\n";
    // The generic is read when its own file is checked.
    let alone = check("g.nsm", generic, &Loader::std());
    let m = messages(&alone);
    assert!(
        m.iter().any(|m| m.starts_with("a generic holds values, pins and interfaces, and no parts or modules; `r` is a part (`Resistor`)")),
        "{m:?}"
    );
}

#[test]
fn a_generic_alone_is_checked_as_it_is() {
    let c = check("rf.nsm", RF, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    assert!(c.boards.is_empty());
}

#[test]
fn a_provider_alone_in_its_file_is_checked_for_what_it_owes() {
    // The root file is the provider's own, which imports the generic.
    let acme = ACME.replace(
        "    .max_frequency &= 6GHz\n",
        "    .max_frequency: Frequency\n",
    );
    let c = run(&acme, &[("src::rf", RF)]);
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.contains("`max_frequency` is declared by `CoaxReceptacle`")),
        "{m:?}"
    );
    // And one that conforms checks clean on its own.
    let c = run(ACME, &[("src::rf", RF)]);
    ok(&c);
}

#[test]
fn a_member_named_generic_or_provides_is_not_a_keyword() {
    let src = "\
part P {
    .generic: Number
    .provides: Number
    .generic &= 2
    .provides == .generic
}
board B {
    .p: P
}
";
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_use_inside_a_module_has_the_path_through_the_module() {
    let module = "\
import src::rf::CoaxReceptacle

module Antenna {
    .rf: CoaxReceptacle { .max_frequency >= 2GHz }
}
";
    let board = "\
import src::ant::Antenna
import src::acme::AcmeMicroCoax

board Feed {
    .ant: Antenna
}
";
    let c = run(
        board,
        &[("src::rf", RF), ("src::acme", ACME), ("src::ant", module)],
    );
    ok(&c);
    let u = &uses(&c)[0];
    assert_eq!(u.path, "ant.rf");
}

#[test]
fn a_cyclic_import_lists_a_provider_once() {
    // `src::acme` imports a file that imports it back: the provider is one candidate.
    let acme = format!("import src::back::Back\n{ACME}");
    let back = "import src::acme::AcmeMicroCoax\nmodule Back {}\n";
    let board = "import src::rf::CoaxReceptacle\nimport src::acme::AcmeMicroCoax\nimport src::back::Back\nboard Feed {\n .rf: CoaxReceptacle\n}\n";
    let c = run(
        board,
        &[("src::rf", RF), ("src::acme", &acme), ("src::back", back)],
    );
    ok(&c);
    let ids: Vec<&str> = uses(&c)[0]
        .candidates
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(ids, ["src::acme::AcmeMicroCoax"]);
}

#[test]
fn two_roots_of_one_project_have_candidates_of_their_own() {
    let first = run(BOARD, &[("src::rf", RF), ("src::acme", ACME)]);
    ok(&first);
    // Another file of the same project, which imports no provider.
    let second = "import src::rf::CoaxReceptacle\nboard Feed {\n .rf: CoaxReceptacle\n}\n";
    let c = run(second, &[("src::rf", RF), ("src::acme", ACME)]);
    assert!(
        messages(&c)[0].contains("no part or module in the files this file imports provides it")
    );
    assert_eq!(uses(&first)[0].candidates.len(), 1);
}

#[test]
fn a_generic_that_is_its_own_provider_comes_first() {
    // A generic with pads or a catalog stands for itself, as the standard passives will.
    let generic = "\
generic Washer {
    .designator &= \"W\"
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    .size: Length
    .package: Package
    catalog washer { .size, .package }
}
";
    let other = "\
import src::g::Washer
part ThickWasher provides Washer {
    .size &= 3mm
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
}
";
    let board = "import src::g::Washer\nimport src::o::ThickWasher\nboard B {\n .w: Washer\n}\n";
    let c = run(board, &[("src::g", generic), ("src::o", other)]);
    assert!(c.is_ok(), "{}", c.render());
    let u = &c.board("B").unwrap().model.uses[0];
    let candidates: Vec<(&str, bool)> = u
        .candidates
        .iter()
        .map(|p| (p.name.as_str(), p.own))
        .collect();
    assert_eq!(candidates, [("Washer", true), ("ThickWasher", false)]);
    let model = &c.board("B").unwrap().model;
    let w = model.instance(model.inst_at("w").unwrap());
    // Placed as itself: nothing is hidden from a board, as a standard passive's members are not.
    assert_eq!(w.type_name, "Washer");
    assert_eq!(w.placed_as, None);
    assert!(model.field_at("w.package").is_some());
}

// ---- the second review: cycles, the root file, aliases and names -------------------------------

const PLAIN_BUCK: &str = "\
import std::sources::Rail

/// What a buck converter gives its users. Invented.
generic BuckConverter {
    .vin: Rail
    .vout: Rail
}
";

const ZBK9_BUCK: &str = "\
import src::buck::BuckConverter

module Zbk9Buck provides BuckConverter {
}
";

/// A module that wraps the generic it provides: its inner use takes the first provider.
fn wrapper(name: &str) -> String {
    format!(
        "import src::buck::BuckConverter\n\nmodule {name} provides BuckConverter {{\n    .inner: BuckConverter\n    .inner.vin ~ .vin\n    .inner.vout ~ .vout\n}}\n"
    )
}

fn buck_board(imports: &[&str]) -> String {
    let mut text = String::from("import src::buck::BuckConverter\n");
    for i in imports {
        text.push_str(&format!("import {i}\n"));
    }
    text.push_str("board Feed {\n    .buck: BuckConverter\n}\n");
    text
}

#[test]
fn a_provider_that_uses_its_own_generic_is_refused_where_it_sorts_first() {
    // `AFilteredBuck` sorts before `Zbk9Buck`, so the use inside it would take itself, again
    // and again: the elaboration used to overflow the stack.
    let w = wrapper("AFilteredBuck");
    let board = buck_board(&["src::a_filtered::AFilteredBuck", "src::zbk9::Zbk9Buck"]);
    let c = run(
        &board,
        &[
            ("src::buck", PLAIN_BUCK),
            ("src::a_filtered", &w),
            ("src::zbk9", ZBK9_BUCK),
        ],
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].starts_with("`inner` is a `BuckConverter`, and `AFilteredBuck`, which is being made here, can be the provider picked for it, so `AFilteredBuck` would contain itself without end (the cycle: `AFilteredBuck` > `AFilteredBuck`)"),
        "{}",
        m[0]
    );
    assert!(m[0].contains("a provider cannot contain a use of the generic it provides"));
    // It is reported at the use inside the provider, in the provider's own file.
    assert_eq!(
        c.sources.get(c.diagnostics[0].source).name,
        "src/a_filtered.nsm"
    );
}

#[test]
fn a_provider_that_uses_its_own_generic_is_refused_whatever_its_name_sorts_as() {
    // `Zwrap` sorts after the real provider, so the use inside it used to take `Zbk9Buck`
    // and be accepted: whether it was a mistake depended on a name. Read as the root file's own
    // definition it is refused the same.
    let w = wrapper("Zwrap");
    let c = check(
        "src/z_wrap.nsm",
        &w,
        &loader(&[("src::buck", PLAIN_BUCK), ("src::zbk9", ZBK9_BUCK)]),
    );
    // The wrapper's file imports no real provider, so its only candidate is itself.
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.contains("`Zwrap` would contain itself without end")),
        "{m:?}"
    );
    // With the real provider in scope and sorting first, the wrapper is still refused: the
    // pick that could take it is what is refused, not the one that happens to be elsewhere.
    let with_real = format!("import src::zbk9::Zbk9Buck\n{w}");
    let c = check(
        "src/z_wrap.nsm",
        &with_real,
        &loader(&[("src::buck", PLAIN_BUCK), ("src::zbk9", ZBK9_BUCK)]),
    );
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.contains("`Zwrap` would contain itself without end")),
        "{m:?}"
    );
}

#[test]
fn a_cycle_through_two_generics_and_a_definition_in_itself_are_refused_and_named() {
    let files = [
        (
            "src::g",
            "generic First {\n    .a: Number\n}\ngeneric Second {\n    .b: Number\n}\n",
        ),
        (
            "src::p",
            "import src::g::First\nimport src::g::Second\nmodule P1 provides First {\n    .h: Second\n}\nmodule P2 provides Second {\n    .g: First\n}\n",
        ),
    ];
    let board = "import src::g::First\nimport src::p::P1\nboard Feed {\n    .x: First\n}\n";
    let c = run(board, &files);
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.contains("(the cycle: `P1` > `P2` > `P1`)")),
        "{m:?}"
    );
    // A definition that holds itself.
    let c = run("module Ring {\n    .inner: Ring\n}\n", &[]);
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(
        m[0].starts_with(
            "`inner` would make `Ring` inside itself without end (the cycle: `Ring` > `Ring`)"
        ),
        "{}",
        m[0]
    );
}

const MAIN_WITH_ZETA: &str = "\
import src::rf::CoaxReceptacle
import src::acme::AcmeMicroCoax

part ZetaCoax provides CoaxReceptacle {
    pins {
        .signal passive \"S\" \"1\"
        .shield passive \"G\" \"2\"
    }
}

board Feed {
    .rf: CoaxReceptacle
}
";

fn candidate_ids(c: &Checked) -> Vec<String> {
    uses(c)[0].candidates.iter().map(|p| p.id.clone()).collect()
}

#[test]
fn the_order_of_providers_does_not_depend_on_how_the_root_file_is_spelled() {
    let want = ["src::acme::AcmeMicroCoax", "src::main::ZetaCoax"];
    // The project holds the root file as a module, and a tool may be given its name any way.
    let with_main = loader(&[
        ("src::rf", RF),
        ("src::acme", ACME),
        ("src::main", MAIN_WITH_ZETA),
    ]);
    let without = loader(&[("src::rf", RF), ("src::acme", ACME)]);
    for name in [
        "src/main.nsm",
        "./src/main.nsm",
        "src/../src/main.nsm",
        ".//src/./main.nsm",
    ] {
        for l in [&with_main, &without] {
            let c = check(name, MAIN_WITH_ZETA, l);
            ok(&c);
            assert_eq!(candidate_ids(&c), want, "spelled {name}");
        }
    }
    // A name a tool knows to be that module (an absolute path in a project).
    let mut known = loader(&[("src::rf", RF), ("src::acme", ACME)]);
    known.attach_root_key("/work/demo/src/main.nsm", "src::main");
    let c = check("/work/demo/src/main.nsm", MAIN_WITH_ZETA, &known);
    ok(&c);
    assert_eq!(candidate_ids(&c), want);
}

#[test]
fn a_cycle_through_the_root_file_holds_the_root_once() {
    // `src/main.nsm` imports a file that imports the root back: the provider in that file
    // provides the root's own generic, not the generic of a second copy of the root.
    let main = "\
import src::prov::Prov

generic Gadget {
    pins {
        .a passive
    }
}

board Feed {
    .x: Gadget
}
";
    let prov = "\
import src::main::Gadget

part Prov provides Gadget {
    pins {
        .a passive \"A\" \"1\"
    }
}
";
    let l = loader(&[("src::main", main), ("src::prov", prov)]);
    for name in ["src/main.nsm", "./src/main.nsm"] {
        let c = check(name, main, &l);
        ok(&c);
        assert_eq!(candidate_ids(&c), ["src::prov::Prov"]);
        assert_eq!(uses(&c)[0].path, "x");
    }
}

#[test]
fn an_alias_for_a_generic_pin_has_the_generics_kind() {
    let other = |kind: &str| {
        format!(
            "import src::rf::CoaxReceptacle\n\npart OtherCoax provides CoaxReceptacle {{\n    pins {{\n        .rf      {kind}  \"RF\"   \"1\"\n        .shield  passive  \"GND\"  \"2\"\n    }}\n    .signal ~ .rf\n}}\n"
        )
    };
    let board = "import src::rf::CoaxReceptacle\nimport src::other::OtherCoax\nboard Feed {\n .rf: CoaxReceptacle\n}\n";
    // The row the alias is joined to says what the checker reads for the net: an output where
    // the generic says passive is refused, at that row.
    let c = run(board, &[("src::rf", RF), ("src::other", &other("output"))]);
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert_eq!(
        m[0],
        "`.signal` is a passive pin in `CoaxReceptacle`, which `OtherCoax` provides, but the row of `.rf`, which `.signal` is joined to, says an output; a provider's row has the generic's kind, because the kind decides what the checker asks of the board"
    );
    // The same kind is accepted.
    let c = run(board, &[("src::rf", RF), ("src::other", &other("passive"))]);
    ok(&c);
}

#[test]
fn two_providers_of_one_generic_cannot_share_a_name() {
    let vendor = "import src::rf::CoaxReceptacle\n\npart Coax provides CoaxReceptacle {\n    pins {\n        .signal passive \"S\" \"1\"\n        .shield passive \"G\" \"2\"\n    }\n}\n";
    let more = "import src::vendor_b::Coax\nmodule Anchor {\n}\n";
    let board = "import src::rf::CoaxReceptacle\nimport src::vendor_a::Coax\nimport src::more::Anchor\nboard Feed {\n .rf: CoaxReceptacle\n}\n";
    let c = run(
        board,
        &[
            ("src::rf", RF),
            ("src::vendor_a", vendor),
            ("src::vendor_b", vendor),
            ("src::more", more),
        ],
    );
    let m = messages(&c);
    assert_eq!(m.len(), 1, "{m:?}");
    assert_eq!(
        m[0],
        "`Coax` is a provider of `CoaxReceptacle` in `src::vendor_b` and is also one in `src::vendor_a::Coax`; two providers of one generic need different names, because a part is known by its name in the bill of materials, in `kicad.toml` and in the notes: rename one"
    );
    // At the second one's name, in its own file.
    assert_eq!(
        c.sources.get(c.diagnostics[0].source).name,
        "src/vendor_b.nsm"
    );
}

/// A manufacturing process as a generic (ADR 0113): values only, provided by interfaces. Every
/// number here is invented.
const FAB: &str = "\
generic Fab {
    .fabricator: String
    .fabricator &= \"Acme\"
    .min_track: Length
    .weight: Number
}
";

const FAB_PROVIDERS: &str = "\
import src::fab::Fab

interface FabOne provides Fab {
    .min_track &= 0.1mm..1m
    .weight &= 1
}

interface FabTwo provides Fab {
    .min_track &= 0.2mm..1m
    .weight &= 2
}
";

#[test]
fn an_interface_provides_a_generic_of_values_and_a_use_is_made_as_an_interface() {
    let board = "\
import src::fab::Fab
import src::fabs::FabOne

board B {
    .process: Fab {}
}
";
    let c = run(board, &[("src::fab", FAB), ("src::fabs", FAB_PROVIDERS)]);
    ok(&c);
    let model = &c.board("B").unwrap().model;
    let process = model.instance(model.inst_at("process").unwrap());
    assert_eq!(process.kind, InstKind::Interface);
    assert_eq!(process.type_name, "FabOne");
    assert_eq!(process.placed_as.as_deref(), Some("Fab"));
    let u = &model.uses[0];
    let names: Vec<&str> = u.candidates.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["FabOne", "FabTwo"]);
    assert!(u.candidates.iter().all(|p| p.kind == InstKind::Interface));
    // The generic's statement reaches the provider.
    let f = model.field_at("process.fabricator").unwrap();
    assert!(matches!(&model.field(f).domain, Domain::Text(t) if t == "Acme"));
    assert!(model.provider_conflicts.is_empty());
}

#[test]
fn an_interface_does_not_provide_a_generic_with_pins() {
    let generic = "generic Thing {\n    pins {\n        .a passive\n    }\n}\n";
    let c = run(
        "import src::g::Thing\ninterface IThing provides Thing {\n}\n",
        &[("src::g", generic)],
    );
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains(
            "`IThing` is an interface, and `Thing` has a `pins` table or a catalog, which only a part provides"
        )),
        "{m:?}"
    );
}

#[test]
fn providers_of_one_generic_are_not_interfaces_and_modules_together() {
    let generic = "generic Thing {\n    .size: Number\n}\n";
    let iface = "import src::g::Thing\ninterface AThing provides Thing {\n    .size &= 1\n}\n";
    let module = "import src::g::Thing\nmodule BModule provides Thing {\n    .size &= 2\n}\n";
    let board = "import src::g::Thing\nimport src::a::AThing\nimport src::b::BModule\nboard B {\n .t: Thing\n}\n";
    let c = run(
        board,
        &[("src::g", generic), ("src::a", iface), ("src::b", module)],
    );
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains(
            "`BModule` is a module, but `AThing`, which provides `Thing` too, is an interface"
        )),
        "{m:?}"
    );
}

#[test]
fn a_narrowing_of_a_use_that_its_first_provider_contradicts_is_a_provider_conflict() {
    let board = "\
import src::fab::Fab
import src::fabs::FabOne

board B {
    .process: Fab { .weight &= 2 }
}
";
    let c = run(board, &[("src::fab", FAB), ("src::fabs", FAB_PROVIDERS)]);
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.contains("contradiction on `process.weight`")),
        "{m:?}"
    );
    // The same diagnostic, kept apart: another provider may not have it, so the build tries
    // the others (ADR 0113).
    let model = &c.board("B").unwrap().model;
    assert_eq!(model.provider_conflicts.len(), 1, "{m:?}");
    assert_eq!(c.diagnostics.len(), 1, "{m:?}");
    // With the second provider, nothing contradicts.
    let mut assign = Assignment::new();
    assign.insert(
        "B".to_string(),
        [("process".to_string(), "src::fabs::FabTwo".to_string())]
            .into_iter()
            .collect(),
    );
    let c = check_with(
        "test.nsm",
        board,
        &loader(&[("src::fab", FAB), ("src::fabs", FAB_PROVIDERS)]),
        &assign,
    );
    ok(&c);
}

#[test]
fn a_contradiction_outside_a_use_with_several_providers_is_not_a_provider_conflict() {
    let board = "\
import src::fab::Fab
import src::fabs::FabOne

board B {
    .process: Fab {}
    .n: Number
    .n &= 1
    .n &= 2
}
";
    let c = run(board, &[("src::fab", FAB), ("src::fabs", FAB_PROVIDERS)]);
    assert!(!c.is_ok());
    assert!(c.board("B").unwrap().model.provider_conflicts.is_empty());
}

// ---- a generic's row limit is a contract every provider meets (ADR 0126) ----------------

/// A generic that is its own provider, with the limits of `.in` in its row.
const LIMITED: &str = "import std::sources::Rail
generic Limited {
    .designator &= \"U\"
    .vcc: Rail
    pins {
        .vcc.pos power \"VCC\" \"1\"
        .vcc.neg power \"GND\" \"2\"
        .in input \"IN\" \"3\" optional abs_min -0.5V abs_max 6V
    }
    .package: Package
    catalog limited { .package }
}
";

/// A provider of `Limited` with `tail` at the end of the row of `.in`.
fn limited_provider(tail: &str) -> String {
    format!(
        "import src::limited::Limited
part Prov provides Limited {{
    pins {{
        .vcc.pos power \"VCC\" \"1\"
        .vcc.neg power \"GND\" \"2\"
        .in input \"IN\" \"3\" optional{tail}
    }}
    catalog prov {{ .package }}
}}
"
    )
}

fn limited_board(generic: &str, provider: &str) -> Checked {
    run(
        "import src::prov::Prov\nimport std::sources::Rail\nboard B {\n    .m: Prov\n    .s: Rail { .voltage &= 3V\n .pos.net &= \"V\"\n .neg.net &= \"G\" }\n    .s ~ .m.vcc\n}\n",
        &[("src::limited", generic), ("src::prov", provider)],
    )
}

fn limit_of(c: &Checked, field: &str) -> Option<(f64, f64)> {
    let model = &c.board("B").unwrap().model;
    match model.domain_at(&format!("m.in.{field}")) {
        Some(Domain::Quantity(q)) => Some((q.lo(), q.hi())),
        _ => None,
    }
}

#[test]
fn a_provider_whose_row_states_no_limit_has_the_generics() {
    let c = limited_board(LIMITED, &limited_provider(""));
    ok(&c);
    assert_eq!(limit_of(&c, "abs_max"), Some((6.0, 6.0)));
    assert_eq!(limit_of(&c, "abs_min"), Some((-0.5, -0.5)));
    let model = &c.board("B").unwrap().model;
    let row = model.pin_row(model.inst_at("m.in").unwrap()).unwrap();
    assert_eq!(row.limits.as_ref().unwrap().text, "abs_min -0.5V abs_max 6V");
}

#[test]
fn a_provider_that_states_one_limit_inherits_the_other() {
    let c = limited_board(LIMITED, &limited_provider(" abs_max 5V"));
    ok(&c);
    assert_eq!(limit_of(&c, "abs_max"), Some((5.0, 5.0)));
    assert_eq!(limit_of(&c, "abs_min"), Some((-0.5, -0.5)));
}

#[test]
fn a_stricter_provider_keeps_its_own_limits() {
    let c = limited_board(LIMITED, &limited_provider(" abs_min 0V abs_max 5V"));
    ok(&c);
    assert_eq!(limit_of(&c, "abs_max"), Some((5.0, 5.0)));
    assert_eq!(limit_of(&c, "abs_min"), Some((0.0, 0.0)));
}

#[test]
fn a_looser_provider_is_refused_at_its_row_naming_the_generics_limit() {
    let c = limited_board(LIMITED, &limited_provider(" abs_max 7V"));
    assert_eq!(
        messages(&c),
        ["`.in` has `abs_max 7V` here, but `Limited`, which `Prov` provides, limits it with `abs_max 6V`: a provider's limit is at least as strict as its generic's, so `abs_max` is at most 6V"]
    );
    let c = limited_board(LIMITED, &limited_provider(" abs_min -1V"));
    assert_eq!(
        messages(&c),
        ["`.in` has `abs_min -1V` here, but `Limited`, which `Prov` provides, limits it with `abs_min -0.5V`: a provider's limit is at least as strict as its generic's, so `abs_min` is at least -500mV"]
    );
}

#[test]
fn a_generics_limit_that_is_a_member_is_the_providers_member() {
    let generic = LIMITED
        .replace(".package: Package", ".in_max: Voltage\n    .package: Package")
        .replace("abs_max 6V", "abs_max .in_max");
    let provider = limited_provider("").replace("catalog prov", ".in_max &= 4V\n    catalog prov");
    let c = limited_board(&generic, &provider);
    ok(&c);
    // The field holds the member as a relation, which the solver resolves.
    assert_eq!(limit_of(&c, "abs_max"), None);
    // A provider's own number against the member waits for the pick: the relation `<=` takes
    // the place of the generic's narrowing, which the provider's own limit replaces.
    let provider = limited_provider(" abs_max 7V").replace("catalog prov", ".in_max &= 4V\n    catalog prov");
    let c = limited_board(&generic, &provider);
    ok(&c);
    let own = c.board("B").unwrap().model.relations().len();
    let inherited = limited_board(&generic, &limited_provider("").replace("catalog prov", ".in_max &= 4V\n    catalog prov"));
    assert_eq!(own, inherited.board("B").unwrap().model.relations().len());
}

#[test]
fn a_generic_with_no_pads_may_state_a_limit_its_providers_meet() {
    let generic = "generic Pure {\n    .designator &= \"U\"\n    pins {\n        .in input abs_max 6V from .gnd\n        .gnd passive\n    }\n}\n";
    let provider = |tail: &str| format!("import src::pure::Pure\npart Prov provides Pure {{\n    pins {{\n        .in input \"IN\" \"1\" optional{tail}\n        .gnd passive \"GND\" \"2\"\n    }}\n}}\n");
    let board = "import src::prov::Prov\nboard B {\n    .m: Prov\n}\n";
    let run_with = |tail: &str| run(board, &[("src::pure", generic), ("src::prov", &provider(tail))]);
    let c = run_with("");
    ok(&c);
    let model = &c.board("B").unwrap().model;
    match model.domain_at("m.in.abs_max") {
        Some(Domain::Quantity(q)) => assert_eq!((q.lo(), q.hi()), (6.0, 6.0)),
        other => panic!("{other:?}"),
    }
    let c = run_with(" abs_max 8V from .gnd");
    assert!(
        messages(&c).iter().any(|m| m.starts_with("`.in` has `abs_max 8V` here, but `Pure`")),
        "{:?}",
        messages(&c)
    );
}
