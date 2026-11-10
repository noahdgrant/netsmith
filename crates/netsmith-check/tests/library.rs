//! The standard library is source, so it has to check clean.

use netsmith_check::*;

#[test]
fn every_standard_library_file_checks_clean() {
    let loader = Loader::std();
    for (name, text) in [
        (
            "std/parts/passive.nsm",
            include_str!("../../../std/parts/passive.nsm"),
        ),
        (
            "std/parts/regulator.nsm",
            include_str!("../../../std/parts/regulator.nsm"),
        ),
        (
            "std/mfg/stackup.nsm",
            include_str!("../../../std/mfg/stackup.nsm"),
        ),
        (
            "std/mfg/jlcpcb.nsm",
            include_str!("../../../std/mfg/jlcpcb.nsm"),
        ),
        ("std/env.nsm", include_str!("../../../std/env.nsm")),
        ("std/sources.nsm", include_str!("../../../std/sources.nsm")),
        ("std/logic.nsm", include_str!("../../../std/logic.nsm")),
    ] {
        let c = check(name, text, &loader);
        assert!(c.is_ok(), "{name}:\n{}", c.render());
    }
}

#[test]
fn a_board_using_every_standard_part_checks_clean() {
    let src = "\
import std::parts::passive::{Resistor, Capacitor, Led}
import std::sources::Rail
import std::logic::Logic

board All {
    .r: Resistor
    .c: Capacitor
    .l: Led
    .p: Rail
    .x: Logic
}
";
    let c = check("all.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
}

#[test]
fn imports_resolve_through_a_library_that_imports_another() {
    // `std::logic` imports `Rail` from `std::sources`; its `Logic` carries that type.
    let c = check(
        "t.nsm",
        "import std::logic::Logic\nboard B {\n    .x: Logic\n}\n",
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("B").unwrap().model;
    let reference = m.inst_at("x.reference").unwrap();
    assert_eq!(m.instance(reference).type_name, "Rail");
    assert!(m.inst_at("x.reference.pos").is_some());
}

#[test]
fn import_cycles_do_not_hang() {
    let mut loader = Loader::empty();
    loader.insert("a", "a.nsm", "import b::B\npart A {\n    .p: Pin\n}\n");
    loader.insert("b", "b.nsm", "import a::A\npart B {\n    .q: Pin\n}\n");
    let c = check(
        "t.nsm",
        "import a::A\nimport b::B\nboard X {\n    .a: A\n    .b: B\n}\n",
        &loader,
    );
    assert!(c.is_ok(), "{}", c.render());
}
