//! `alternate .u: ModuleB` (issue 1312): the checker records each alternate of a part, refuses
//! one that does not name a part beside it, and with an alternate fitted (`check_fitted`)
//! declares the member as the alternate.

use std::collections::BTreeMap;

use netsmith_check::{Assignment, Loader, check, check_fitted};

const PARTS: &str = r#"
part ModuleA {
    pins {
        .vcc passive "VCC" "1"
        .gnd passive "GND" "2"
    }
}

part ModuleB {
    pins {
        .vcc passive "VCC" "1"
        .gnd passive "GND" "2"
    }
}

part ModuleC {
    pins {
        .vcc passive "VCC" "1"
    }
}

module Radio {
    .u: ModuleA
    alternate .u: ModuleB
}
"#;

fn source(body: &str) -> String {
    format!("{PARTS}\nboard B {{\n{body}\n}}\n")
}

fn errors(body: &str) -> String {
    let checked = check("main.nsm", &source(body), &Loader::std());
    assert!(!checked.is_ok(), "{body} was accepted");
    checked.render()
}

#[test]
fn each_alternate_is_recorded_with_its_place() {
    let text = source(
        ".u: ModuleA\nalternate .u: ModuleB\nalternate .u: ModuleC\n.radio: Radio",
    );
    let checked = check("main.nsm", &text, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let b = checked.standalone_boards().next().unwrap();
    let got: Vec<(&str, usize, &str, &str)> = b
        .model
        .alternates
        .iter()
        .map(|a| (a.path.as_str(), a.index, a.kind.as_str(), a.primary.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("radio.u", 0, "ModuleB", "ModuleA"),
            ("u", 0, "ModuleB", "ModuleA"),
            ("u", 1, "ModuleC", "ModuleA"),
        ]
    );
    // The member is the part the board is drawn with.
    let u = b.model.inst_at("u").unwrap();
    assert_eq!(b.model.instance(u).type_name, "ModuleA");
}

#[test]
fn a_fitted_alternate_stands_in_the_place_of_the_member() {
    let text = source(".u: ModuleA { .assembly &= AssemblyMode::Hand }\nalternate .u: ModuleB\n.radio: Radio");
    let fitted = BTreeMap::from([("u".to_string(), 0), ("radio.u".to_string(), 0)]);
    let checked = check_fitted("main.nsm", &text, &Loader::std(), &Assignment::new(), "B", &fitted);
    assert!(checked.is_ok(), "{}", checked.render());
    let b = checked.board("B").unwrap();
    for path in ["u", "radio.u"] {
        let u = b.model.inst_at(path).unwrap();
        assert_eq!(b.model.instance(u).type_name, "ModuleB", "{path}");
    }
}

#[test]
fn a_pin_the_fitted_alternate_lacks_is_refused_where_it_is_used() {
    let text = source(
        ".u: ModuleA\nalternate .u: ModuleC\n.a: ModuleA\n.u.gnd ~ .a.gnd",
    );
    assert!(check("main.nsm", &text, &Loader::std()).is_ok());
    let fitted = BTreeMap::from([("u".to_string(), 0)]);
    let checked = check_fitted("main.nsm", &text, &Loader::std(), &Assignment::new(), "B", &fitted);
    assert!(!checked.is_ok());
    assert!(checked.render().contains("gnd"), "{}", checked.render());
}

#[test]
fn an_alternate_names_a_part_declared_beside_it() {
    let e = errors("alternate .u: ModuleB");
    assert!(
        e.contains("takes the place of `.u`, which this body does not declare"),
        "{e}"
    );
    let e = errors(".radio: Radio\nalternate .radio: Radio");
    assert!(e.contains("`.radio` is a module"), "{e}");
    let e = errors(".u: ModuleA\nalternate .u: Radio");
    assert!(e.contains("`Radio` is not a part"), "{e}");
    let e = errors(".u: ModuleA\nalternate .u: ModuleA");
    assert!(e.contains("`.u` is a `ModuleA` already"), "{e}");
    let e = errors(".u: 2 x ModuleA\nalternate .u: ModuleB");
    assert!(e.contains("`.u` is not one part"), "{e}");
}

#[test]
fn only_a_board_or_a_module_states_an_alternate() {
    let text = format!(
        "{PARTS}\npart P {{\n    .u: ModuleA\n    alternate .u: ModuleB\n    pins {{ .a passive \"A\" \"1\" }}\n}}\n"
    );
    let checked = check("main.nsm", &text, &Loader::std());
    assert!(
        checked.render().contains("only a board or a module states an alternate"),
        "{}",
        checked.render()
    );
}
