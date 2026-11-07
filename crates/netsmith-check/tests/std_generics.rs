//! The standard passives, diodes, MOSFETs, level translator and linear regulator are generics that are their own
//! first provider (ADR 0068 stage 2): a design reads them as it always did, and a project may
//! add a provider of any of them. Every provider, part number and name here is invented.

use netsmith_check::*;
use netsmith_lang::ast::{DefKind, Item};
use netsmith_lang::parse;

/// The standard generics: the definition's name, its file, and the pins a provider's table
/// must give, with the kind of each.
/// A pin of a contract: its name and the word of its kind.
type Pins = &'static [(&'static str, &'static str)];

const GENERICS: [(&str, &str, Pins); 14] = [
    ("Resistor", "passive", AB),
    ("Capacitor", "passive", AB),
    ("Led", "passive", DIODE),
    ("Inductor", "passive", AB),
    ("FerriteBead", "passive", AB),
    ("Crystal", "passive", AB),
    ("Fuse", "passive", AB),
    ("Schottky", "diode", DIODE),
    ("Zener", "diode", DIODE),
    ("Tvs", "diode", DIODE),
    (
        "NMosfet",
        "transistor",
        &[
            ("gate", "input"),
            ("source", "passive"),
            ("drain", "passive"),
        ],
    ),
    (
        "PMosfet",
        "transistor",
        &[
            ("gate", "input"),
            ("source", "passive"),
            ("drain", "passive"),
        ],
    ),
    (
        "LevelTranslator",
        "logic",
        &[
            ("nc", "no_connect"),
            ("a", "input"),
            ("vcc.neg", "power"),
            ("y", "output"),
            ("vcc.pos", "power"),
        ],
    ),
    (
        "LinearRegulator",
        "regulator",
        &[
            ("vin.pos", "power"),
            ("vin.neg", "power"),
            ("vout.pos", "power"),
        ],
    ),
];

const AB: &[(&str, &str)] = &[("a", "passive"), ("b", "passive")];
const DIODE: &[(&str, &str)] = &[("anode", "passive"), ("cathode", "passive")];

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

/// Nothing is wrong but a pin that a test leaves open: a board of one part and no connections
/// has its required pins unconnected, which is not what these tests are about.
fn clean(c: &Checked) -> bool {
    messages(c)
        .iter()
        .all(|m| m.contains("that must be connected"))
}

/// A provider of `generic` in `src::acme`, with a pin table that gives every pin of the
/// contract a pad, and nothing else.
fn provider(name: &str, file: &str, generic: &str, pins: &[(&str, &str)]) -> String {
    let rows: String = pins
        .iter()
        .enumerate()
        .map(|(i, (pin, kind))| format!("        .{pin} {kind} \"P{}\" \"{}\"\n", i + 1, i + 1))
        .collect();
    format!(
        "import std::parts::{file}::{generic}\n\npart {name} provides {generic} {{\n    pins {{\n{rows}    }}\n}}\n"
    )
}

fn board_using(generic: &str, file: &str, extra_imports: &str, member: &str) -> String {
    format!(
        "import std::parts::{file}::{generic}\n{extra_imports}\nmodule M {{\n    .x: {member}\n}}\nboard B {{\n    .m: M\n}}\n"
    )
}

#[test]
fn the_standard_catalog_kinds_are_generics_and_the_rest_stay_parts() {
    let kind_of = |file: &str, name: &str| {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../std/parts")
                .join(format!("{file}.nsm")),
        )
        .unwrap();
        let parsed = parse(&text).unwrap();
        parsed
            .definitions
            .iter()
            .find(|d| d.name.name == name)
            .unwrap_or_else(|| panic!("no {name} in {file}"))
            .kind
    };
    for (name, file, _) in GENERICS {
        assert_eq!(kind_of(file, name), DefKind::Generic, "{name}");
    }
    // No catalog, so nothing to be the first provider of: they are not in the list of generics.
    for (name, file) in [("Diode", "diode"), ("Switch", "passive")] {
        assert_eq!(kind_of(file, name), DefKind::Part, "{name}");
    }
}

#[test]
fn a_use_of_a_standard_generic_is_the_generic_itself_as_a_part_of_its_own_name() {
    for (name, file, pins) in GENERICS {
        let c = run(&board_using(name, file, "", name), &[]);
        assert!(clean(&c), "{name}: {}", c.render());
        let model = &c.board("B").unwrap().model;
        let x = model.instance(model.inst_at("m.x").unwrap());
        assert_eq!(x.type_name, name);
        assert_eq!(x.kind, InstKind::Part, "{name}");
        assert_eq!(x.placed_as, None, "{name}");
        assert!(x.provides.is_empty(), "{name}");
        assert!(x.catalog.is_some(), "{name} keeps its catalog");
        assert_eq!(
            x.tables.iter().map(|t| t.rows.len()).max(),
            Some(pins.len())
        );
        // One candidate, itself: nothing is picked and nothing is said.
        let u = &model.uses[0];
        assert_eq!((u.generic.as_str(), u.chosen), (name, 0));
        assert_eq!(u.candidates.len(), 1, "{name}");
        assert!(u.candidates[0].own && u.candidates[0].std, "{name}");
    }
}

#[test]
fn a_provider_of_each_standard_generic_conforms_and_is_the_second_candidate() {
    for (name, file, pins) in GENERICS {
        let acme = provider("AcmeX", file, name, pins);
        let board = board_using(name, file, "import src::acme::AcmeX", name);
        let c = run(&board, &[("src::acme", &acme)]);
        assert!(clean(&c), "{name}: {}", c.render());
        let model = &c.board("B").unwrap().model;
        let u = &model.uses[0];
        let order: Vec<(&str, bool)> = u
            .candidates
            .iter()
            .map(|p| (p.name.as_str(), p.own))
            .collect();
        assert_eq!(order, [(name, true), ("AcmeX", false)], "{name}");
        // The generic is first, so the use is still the generic itself.
        assert_eq!(u.chosen, 0);

        // By its own type it is a plain part that says what it provides.
        let by_type = board_using(name, file, "import src::acme::AcmeX", "AcmeX");
        let c = run(&by_type, &[("src::acme", &acme)]);
        assert!(clean(&c), "{name}: {}", c.render());
        let model = &c.board("B").unwrap().model;
        let x = model.instance(model.inst_at("m.x").unwrap());
        assert_eq!(x.type_name, "AcmeX");
        assert_eq!(x.provides, [name], "{name}");
        assert_eq!(x.placed_as, None);
        assert!(model.uses.is_empty(), "{name}: nothing is picked");
    }
}

#[test]
fn a_provider_has_the_generics_members_and_statements_but_not_its_pads_or_catalog() {
    let acme = "\
import std::parts::passive::Resistor

part ThinFilm provides Resistor {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    catalog thin_film { .resistance, .max_power, .package }
}
";
    let board = "\
import std::parts::passive::Resistor
import src::acme::ThinFilm
module M {
    .x: ThinFilm
    .y: Resistor
}
board B {
    .m: M
}
";
    let c = run(board, &[("src::acme", acme)]);
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    let x = model.instance(model.inst_at("m.x").unwrap());
    let y = model.instance(model.inst_at("m.y").unwrap());
    // The generic's members and its `bridge` are the provider's.
    for member in [
        "resistance",
        "max_power",
        "temperature_coefficient",
        "current",
        "drop",
        "package",
        "designator",
    ] {
        assert!(
            model
                .member(model.inst_at("m.x").unwrap(), member)
                .is_some(),
            "{member}"
        );
    }
    assert!(
        x.bridge.is_some(),
        "the generic's `bridge .a ~> .b` holds for the provider"
    );
    // The generic's catalog and pads are its own: the provider has the one it wrote.
    assert_eq!(
        x.catalog.as_ref().map(|c| c.name.as_str()),
        Some("thin_film")
    );
    assert_eq!(
        y.catalog.as_ref().map(|c| c.name.as_str()),
        Some("resistor")
    );
    assert_eq!(x.tables.len(), 1);
    assert_eq!(x.tables[0].rows.len(), 2);
    assert!(x.tables[0].rows.iter().all(|r| r.pads.len() == 1));
    // The generic's relations are on the provider: Ohm's law and the power rating.
    assert_eq!(
        model.relations().len(),
        4,
        "Ohm's law and the rating, for each of the two resistors"
    );
    // The generic's statement, read into the provider.
    assert_eq!(
        model.domain_at("m.x.designator"),
        Some(&Domain::Text("R".into()))
    );
    assert_eq!(
        model.domain_at("m.y.designator"),
        Some(&Domain::Text("R".into()))
    );
}

#[test]
fn a_provider_that_gives_a_pin_the_wrong_kind_or_leaves_one_out_is_refused() {
    let wrong = "\
import std::parts::passive::Resistor
part Wrong provides Resistor {
    pins {
        .a power \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
}
";
    let c = run(
        "import src::acme::Wrong\nboard B {\n .x: Wrong\n}\n",
        &[("src::acme", wrong)],
    );
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains("`.a` is a passive pin in `Resistor`, which `Wrong` provides, but this row says a power pin")),
        "{m:?}"
    );

    let missing = "\
import std::parts::passive::Resistor
part Missing provides Resistor {
    pins {
        .a passive \"A\" \"1\"
    }
}
";
    let c = run(
        "import src::acme::Missing\nboard B {\n .x: Missing\n}\n",
        &[("src::acme", missing)],
    );
    let m = messages(&c);
    assert!(
        m.iter()
            .any(|m| m.contains("has no row for `.b`, which `Resistor` declares as a passive pin")),
        "{m:?}"
    );

    let none = "\
import std::parts::passive::Resistor
part Padless provides Resistor {
}
";
    let c = run(
        "import src::acme::Padless\nboard B {\n .x: Padless\n}\n",
        &[("src::acme", none)],
    );
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains("`Padless` provides `Resistor`, which has the pin `.a`, and this part has no `pins` table")),
        "{m:?}"
    );
}

#[test]
fn a_provider_does_not_declare_what_the_generic_declares_not_even_the_package() {
    // `.package: Package` is a member of every standard generic, so a hand-written part that
    // provides one drops its own line for it (it is written in a part otherwise).
    let acme = "\
import std::parts::passive::Resistor
part Again provides Resistor {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    .resistance: Resistance
    .package: Package
}
";
    let c = run(
        "import src::acme::Again\nboard B {\n .x: Again\n}\n",
        &[("src::acme", acme)],
    );
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.starts_with("`resistance` is declared by `Resistor`, which this part provides; do not declare it again")),
        "{m:?}"
    );
    assert!(
        m.iter().any(|m| m.starts_with(
            "`package` is declared by `Resistor`, which this part provides; do not declare it again"
        )),
        "{m:?}"
    );
}

#[test]
fn a_provider_cannot_change_the_letters_the_generic_states() {
    // Rule 4 of ADR 0068: the generic's narrowings hold for every provider, so a provider that
    // wants other designator letters is refused where it says so.
    let acme = "\
import std::parts::passive::Resistor
part Network provides Resistor {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    .designator &= \"RN\"
}
";
    let c = run(
        "import src::acme::Network\nboard B {\n .x: Network\n}\n",
        &[("src::acme", acme)],
    );
    let m = messages(&c);
    assert!(
        m.iter().any(|m| m.contains(
            "contradiction on `x.designator`: it is already \"R\", which does not overlap \"RN\""
        )),
        "{m:?}"
    );
}

#[test]
fn a_provider_may_name_a_pin_with_an_alias_and_a_value_with_an_equation() {
    let acme = "\
import std::parts::passive::Inductor
part Choke provides Inductor {
    pins {
        .p1 passive \"1\" \"1\"
        .b passive \"2\" \"2\"
    }
    .a ~ .p1
    .henries: Inductance
    .inductance == .henries
}
";
    let board = "import src::acme::Choke\nboard B {\n .x: Choke\n}\n";
    let c = run(board, &[("src::acme", acme)]);
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn a_linear_regulator_provider_gives_its_interface_pins_rows() {
    let acme = "\
import std::parts::regulator::LinearRegulator
part Ldo provides LinearRegulator {
    pins {
        .vin.pos  power \"IN\"  \"1\"
        .vin.neg  power \"GND\" \"2\"
        .vout.pos power \"OUT\" \"3\"
    }
}
";
    let board = "\
import std::parts::regulator::LinearRegulator
import src::acme::Ldo
module M {
    .u: LinearRegulator
}
board B {
    .m: M
}
";
    let c = run(board, &[("src::acme", acme)]);
    assert!(clean(&c), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    assert_eq!(
        model.uses[0]
            .candidates
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["LinearRegulator", "Ldo"]
    );
    // The generic's own pin tables are per package; a provider's contract is one table.
    let by_type = "import src::acme::Ldo\nmodule M {\n .u: Ldo\n}\nboard B {\n .m: M\n}\n";
    let c = run(by_type, &[("src::acme", acme)]);
    assert!(clean(&c), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    let u = model.instance(model.inst_at("m.u").unwrap());
    assert_eq!(u.tables.len(), 1);
    assert_eq!(u.provides, ["LinearRegulator"]);

    let short = acme.replace("        .vout.pos power \"OUT\" \"3\"\n", "");
    let c = run(by_type, &[("src::acme", &short)]);
    assert!(
        messages(&c).iter().any(|m| m
            .contains("has no row for `.pos`, which `LinearRegulator` declares as a power pin")),
        "{:?}",
        messages(&c)
    );
}

#[test]
fn a_module_cannot_provide_a_standard_generic() {
    let acme = "\
import std::parts::passive::Resistor
module Wrapper provides Resistor {
}
";
    let c = run(
        "import src::acme::Wrapper\nboard B {\n .x: Wrapper\n}\n",
        &[("src::acme", acme)],
    );
    assert!(
        messages(&c).iter().any(|m| m.contains(
            "`Wrapper` is a module, and `Resistor` has a `pins` table, which only a part provides"
        )),
        "{:?}",
        messages(&c)
    );
}

#[test]
fn providers_come_after_the_generic_by_module_path_and_name_not_by_import_order() {
    let make = |name: &str| provider(name, "passive", "Resistor", AB);
    let board = "\
import std::parts::passive::Resistor
import src::zed::Mid
import src::alpha::Zulu
import src::alpha::Able
board B {
    .x: Resistor
}
";
    let files = [
        ("src::zed", make("Mid")),
        ("src::alpha", make("Zulu").to_string()),
    ];
    // Two providers in one file, written in the opposite order of their names.
    let both = format!(
        "{}\npart Able provides Resistor {{\n    pins {{\n        .a passive \"A\" \"1\"\n        .b passive \"B\" \"2\"\n    }}\n}}\n",
        files[1].1
    );
    let c = run(board, &[("src::zed", &files[0].1), ("src::alpha", &both)]);
    assert!(c.is_ok(), "{}", c.render());
    let order: Vec<String> = c.board("B").unwrap().model.uses[0]
        .candidates
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(order, ["Resistor", "Able", "Zulu", "Mid"]);
}

#[test]
fn a_provider_in_a_file_that_is_not_imported_is_not_a_candidate() {
    let acme = provider("Unseen", "passive", "Resistor", AB);
    let board = "import std::parts::passive::Resistor\nboard B {\n .x: Resistor\n}\n";
    let c = run(board, &[("src::acme", &acme)]);
    assert!(c.is_ok(), "{}", c.render());
    assert_eq!(c.board("B").unwrap().model.uses[0].candidates.len(), 1);
}

#[test]
fn only_a_use_that_names_the_generics_members_works_with_every_provider() {
    // `.x.dc_resistance` is a member of `Inductor`'s contract but `.x.winding` is a member of
    // the provider only: a board that reaches it works with one provider and no other.
    let acme = "\
import std::parts::passive::Inductor
part Wound provides Inductor {
    pins {
        .a passive \"1\" \"1\"
        .b passive \"2\" \"2\"
    }
    .winding: Number
}
";
    let choose = |extra: &str| {
        let board = format!(
            "import std::parts::passive::Inductor\nimport src::acme::Wound\nboard B {{\n    .x: Inductor {{ {extra} }}\n}}\n"
        );
        run(&board, &[("src::acme", acme)])
    };
    let c = choose(".dc_resistance <= 1ohm");
    assert!(c.is_ok(), "{}", c.render());
    // The use is the generic itself here, so its own members are all it has.
    let c = choose(".winding &= 3");
    assert!(!c.is_ok());
    let m = messages(&c);
    assert!(m.iter().any(|m| m.contains("winding")), "{m:?}");
}

#[test]
fn a_provider_of_the_standard_capacitor_is_a_bank_like_it_and_a_projects_own_is_not() {
    let acme = "\
import std::parts::passive::Capacitor
part Poly provides Capacitor {
    pins {
        .a passive \"1\" \"1\"
        .b passive \"2\" \"2\"
    }
}
";
    let board = "\
import std::parts::passive::Capacitor
import src::acme::Poly
board B {
    .x: Poly { .count &= 2 }
    .y: Capacitor { .count &= 2 }
}
";
    let c = run(board, &[("src::acme", acme)]);
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    let x = model.inst_at("x").unwrap();
    let y = model.inst_at("y").unwrap();
    assert!(model.bank_count(x).is_some(), "the provider is a bank");
    assert!(model.bank_count(y).is_some());

    // A generic of the project that is merely called `Capacitor` is not the standard one.
    let own = "\
generic Capacitor {
    pins {
        .a passive
        .b passive
    }
    .count: Number
}
";
    let theirs = "\
import src::own::Capacitor
part Poly provides Capacitor {
    pins {
        .a passive \"1\" \"1\"
        .b passive \"2\" \"2\"
    }
}
";
    let board = "\
import src::theirs::Poly
board B {
    .x: Poly
}
";
    let c = run(board, &[("src::own", own), ("src::theirs", theirs)]);
    assert!(c.is_ok(), "{}", c.render());
    let model = &c.board("B").unwrap().model;
    assert_eq!(model.bank_count(model.inst_at("x").unwrap()), None);
}

#[test]
fn the_words_of_a_message_about_a_standard_generic_are_those_of_a_part() {
    // A standard generic is placed as a part, and messages that say what a thing is call it
    // one, as they did when it was written with `part`.
    let c = run(
        "import std::parts::passive::Resistor\nsystem S {\n .r: Resistor\n}\n",
        &[],
    );
    assert_eq!(
        messages(&c),
        ["a system holds boards, but `Resistor` is a part; put it on a board"]
    );
    // An invented generic that has neither pads nor a catalog is not a part.
    let g = "generic Thing {\n    .size: Length\n}\n";
    let c = run(
        "import src::g::Thing\nsystem S {\n .t: Thing\n}\n",
        &[("src::g", g)],
    );
    assert_eq!(
        messages(&c),
        ["a system holds boards, but `Thing` is a generic; put it on a board"]
    );
}

#[test]
fn every_item_of_a_standard_generic_is_still_in_its_file() {
    // The conversion changed the keyword and nothing else: the catalog, the pin tables and the
    // bridge are in the generic.
    for (name, file, _) in GENERICS {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../std/parts")
                .join(format!("{file}.nsm")),
        )
        .unwrap();
        let parsed = parse(&text).unwrap();
        let def = parsed
            .definitions
            .iter()
            .find(|d| d.name.name == name)
            .unwrap();
        assert!(
            def.items.iter().any(|i| matches!(i, Item::Catalog(_))),
            "{name}"
        );
        assert!(
            def.items.iter().any(|i| matches!(i, Item::Pins(_))),
            "{name}"
        );
        assert!(def.provides.is_empty(), "{name}");
    }
}
