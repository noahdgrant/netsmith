//! A shared `enclosure` (ADR 0132): the contents of a board's `case { }` section as a
//! definition of its own, which a board's physical unit names with `case Name`.

use netsmith_check::*;

const CIRCUIT: &str = "\
import std::parts::connector::Header1x2

board A {
    .j: Header1x2
}

board B {
    .j: Header1x2
}
";

/// The case of both boards, as a file of the project: `src::case`.
const CASE: &str = "\
import std::parts::mechanical::MountingHole
import std::pcb::{CaseModel, Cutout, Outline}

enclosure Box {
    .outline: Outline {
        .width &= 40mm
        .height &= 24mm
    }
    .window: Cutout {
        .x &= 30mm
        .y &= 2mm
        .width &= 6mm
        .height &= 3mm
    }
    .hole: MountingHole
    .hole.place.x &= 4mm
    .shell: CaseModel {
        .file &= \"box.step\"
    }
}
";

fn run(circuit: &str, physical: &str, case: &str) -> Checked {
    let mut loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", physical);
    loader.insert("src::case", "src/case.nsm", case);
    check("t.nsm", circuit, &loader)
}

fn messages(c: &Checked) -> Vec<String> {
    c.diagnostics.iter().map(|d| d.message.clone()).collect()
}

#[test]
fn two_boards_share_one_enclosure() {
    let c = run(
        CIRCUIT,
        "import src::case::Box\n\nphysical for A {\n    case Box\n    .j.place.left &= .outline.width - 10mm\n}\n\nphysical for B {\n    case Box\n}\n",
        CASE,
    );
    assert!(c.is_ok(), "{}", c.render());
    for board in ["A", "B"] {
        let m = &c.board(board).unwrap().model;
        assert!(matches!(m.domain_at("outline.width"), Some(Domain::Quantity(q)) if (q.lo() - 0.040).abs() < 1e-9), "{board}: {:?}", m.domain_at("outline.width"));
        assert!(m.inst_at("window").is_some(), "{board}");
        assert!(m.inst_at("hole").is_some(), "{board}");
        assert_eq!(
            m.domain_at("shell.file"),
            Some(&Domain::Text("box.step".into()))
        );
        // A statement of the enclosure keeps its own file and line.
        let at = &m.field(m.field_at("outline.width").unwrap()).narrowed_at;
        assert_eq!(c.sources.get(at[0].0).name, "src/case.nsm", "{board}");
    }
}

#[test]
fn an_enclosure_in_the_physical_file_serves_the_units_of_that_file() {
    let c = run(
        CIRCUIT,
        "import std::pcb::Outline\n\nenclosure Frame {\n    .outline: Outline {\n        .width &= 30mm\n        .height &= 20mm\n    }\n}\n\nphysical for A {\n    case Frame\n}\n\nphysical for B {\n    case Frame\n}\n",
        "",
    );
    assert!(c.is_ok(), "{}", c.render());
    assert!(c.board("B").unwrap().model.inst_at("outline").is_some());
}

#[test]
fn a_unit_adds_its_own_case_section_to_the_enclosure() {
    let c = run(
        CIRCUIT,
        "import src::case::Box\nimport std::parts::mechanical::Fiducial\n\nphysical for A {\n    case Box\n    case {\n        .fid: Fiducial\n    }\n}\n",
        CASE,
    );
    assert!(c.is_ok(), "{}", c.render());
    let m = &c.board("A").unwrap().model;
    assert!(m.inst_at("fid").is_some() && m.inst_at("outline").is_some());
    // A name both declare is declared twice.
    let c = run(
        CIRCUIT,
        "import src::case::Box\nimport std::pcb::Outline\n\nphysical for A {\n    case Box\n    case {\n        .outline: Outline {}\n    }\n}\n",
        CASE,
    );
    assert_eq!(messages(&c).len(), 1, "{}", c.render());
    assert!(messages(&c)[0].contains("outline"), "{}", c.render());
}

#[test]
fn a_name_that_is_not_an_enclosure_is_refused() {
    let c = run(CIRCUIT, "physical for A {\n    case Nope\n}\n", CASE);
    assert_eq!(
        messages(&c),
        [
            "`case Nope` names no enclosure: this file neither defines nor imports `Nope`; import it (`import src::case::Nope`), or define `enclosure Nope { ... }` in this physical file"
        ]
    );
    let c = run(
        CIRCUIT,
        "import std::parts::connector::Header1x2\n\nphysical for A {\n    case Header1x2\n}\n",
        CASE,
    );
    assert_eq!(
        messages(&c),
        [
            "`case Header1x2` names a part, and `case` names an enclosure (`enclosure Header1x2 { ... }`, ADR 0132)"
        ]
    );
}

#[test]
fn a_board_is_in_one_case() {
    let c = run(
        CIRCUIT,
        "import src::case::Box\n\nenclosure Other {\n}\n\nphysical for A {\n    case Box\n    case Other\n}\n",
        CASE,
    );
    assert_eq!(
        messages(&c),
        [
            "a board is in one case, and this unit already names `case Box` (line 7); put what `Other` adds to it in `Box`, or in the unit's own `case { }` section"
        ]
    );
}

#[test]
fn an_enclosure_names_no_other_enclosure() {
    let c = run(
        CIRCUIT,
        "enclosure Inner {\n}\n\nenclosure Outer {\n    case Inner\n}\n\nenclosure Me {\n    case Me\n}\n\nphysical for A {\n    case Outer\n}\n\nphysical for B {\n    case Me\n}\n",
        "",
    );
    let m = messages(&c);
    assert_eq!(m.len(), 2, "{}", c.render());
    assert!(
        m.iter()
            .all(|m| m.contains("is in a case already: an enclosure")),
        "{}",
        c.render()
    );
}

#[test]
fn case_of_an_enclosure_belongs_in_a_boards_physical_unit() {
    let c = run(
        "import src::case::Box\n\nboard A {\n    case Box\n}\n",
        "",
        CASE,
    );
    assert_eq!(
        messages(&c),
        ["`case Box` belongs in a board's physical file (`physical for Board { case Box }`, ADR 0132)"]
    );
}

#[test]
fn an_enclosure_is_no_member() {
    let c = run(
        "import src::case::Box\n\nboard A {\n    .case: Box\n}\n",
        "",
        CASE,
    );
    assert_eq!(
        messages(&c),
        [
            "`Box` is an enclosure, and an enclosure is no member: a board's physical file puts the board in it with `case Box` in `physical for Board { ... }` (ADR 0132)"
        ]
    );
}

#[test]
fn a_boards_circuit_source_holds_no_enclosure() {
    let c = run(
        "import std::pcb::Outline\n\nenclosure Box {\n    .outline: Outline {}\n}\n\nboard A {\n}\n",
        "",
        "",
    );
    assert_eq!(
        messages(&c),
        [
            "`enclosure Box` is physical intent, and a file that defines a board (`A`) is circuit source, which does not hold it (ADR 0048); put it in `t.physical.nsm`, or in a file of its own under `src/` that the physical files of several boards import"
        ]
    );
}

#[test]
fn an_enclosure_checked_on_its_own_holds_only_what_a_case_section_holds() {
    let c = check("src/case.nsm", CASE, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let c = check(
        "src/case.nsm",
        "import std::parts::connector::Header1x2\n\nenclosure Bad {\n    .j: Header1x2\n}\n",
        &Loader::std(),
    );
    assert_eq!(messages(&c).len(), 1, "{}", c.render());
    assert!(
        messages(&c)[0].starts_with("a physical file declares the outline"),
        "{}",
        c.render()
    );
}

#[test]
fn an_error_in_an_enclosure_points_at_its_own_file() {
    let c = run(
        CIRCUIT,
        "import src::case::Box\n\nphysical for A {\n    case Box\n}\n",
        "import std::pcb::Outline\n\nenclosure Box {\n    .outline: Outline {\n        .nope &= 1mm\n    }\n}\n",
    );
    assert!(!c.is_ok());
    assert_eq!(
        c.sources.get(c.diagnostics[0].source).name,
        "src/case.nsm",
        "{}",
        c.render()
    );
}
