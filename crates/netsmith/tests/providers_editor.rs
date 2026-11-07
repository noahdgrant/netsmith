//! A picked provider in the editor (ADR 0068): go to definition from a use and from a
//! provider's header, and the bill of materials saying what a part stands for. Invented parts.

use std::fs;

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith_editor::Backend;
use netsmith_kicad::testlib::TempDir;

const GENERIC: &str = "\
generic CoaxReceptacle {
    .designator &= \"J\"
    pins {
        .signal passive
        .shield passive
    }
    .max_frequency: Frequency
    .mpn: String
}
";

fn provider(name: &str, fmax: &str) -> String {
    format!(
        "import src::rf::CoaxReceptacle\n\npart {name} provides CoaxReceptacle {{\n    pins {{\n        .signal passive \"S\" \"1\"\n        .shield passive \"G\" \"2\"\n    }}\n    .max_frequency &= {fmax}\n    .package: Package\n    catalog {} {{ .mpn, .package }}\n}}\n",
        name.to_lowercase()
    )
}

const MAIN: &str = "\
import src::rf::CoaxReceptacle
import src::alpha::AlphaCoax
import src::beta::BetaCoax

board Feed {
    .rf: CoaxReceptacle { .max_frequency >= 2GHz }
}
";

fn project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("providers-editor-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\ncatalogs = [\"catalogs/alphacoax.csv\", \"catalogs/betacoax.csv\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/rf.nsm"), GENERIC).unwrap();
    fs::write(root.join("src/alpha.nsm"), provider("AlphaCoax", "1GHz")).unwrap();
    fs::write(root.join("src/beta.nsm"), provider("BetaCoax", "3GHz")).unwrap();
    for (name, mpn) in [("alphacoax", "ALPHA-1"), ("betacoax", "BETA-2")] {
        fs::write(
            root.join(format!("catalogs/{name}.csv")),
            format!(
                "mpn,package,library,source\n{mpn},PKG,Extended,https://example.invalid/{mpn}\n"
            ),
        )
        .unwrap();
    }
    fs::write(root.join("src/main.nsm"), MAIN).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn go_to_definition_on_a_use_and_a_providers_header_opens_the_generic() {
    let (_root, editor) = project("definition");
    let at = |text: &str, needle: &str, nth: usize| text.match_indices(needle).nth(nth).unwrap().0;
    let rf_line = GENERIC
        .lines()
        .position(|l| l.contains("generic CoaxReceptacle"))
        .unwrap() as u32
        + 1;

    // The type of a use.
    let used = editor
        .definition("src/main.nsm", MAIN, at(MAIN, "CoaxReceptacle", 1) + 3)
        .unwrap()
        .expect("a definition");
    assert_eq!(
        (used.at.file.as_str(), used.at.line),
        ("src/rf.nsm", rf_line)
    );

    // A member through the use goes to the generic's declaration of it.
    let member = editor
        .definition("src/main.nsm", MAIN, at(MAIN, ".max_frequency", 0) + 3)
        .unwrap()
        .expect("a definition");
    assert_eq!(member.at.file, "src/rf.nsm");
    assert_eq!(
        member.at.line,
        GENERIC
            .lines()
            .position(|l| l.contains(".max_frequency: Frequency"))
            .unwrap() as u32
            + 1
    );

    // The name after `provides` in a provider's header.
    let alpha = provider("AlphaCoax", "1GHz");
    let header = editor
        .definition(
            "src/alpha.nsm",
            &alpha,
            at(&alpha, "provides CoaxReceptacle", 0) + 12,
        )
        .unwrap()
        .expect("a definition");
    assert_eq!(
        (header.at.file.as_str(), header.at.line),
        ("src/rf.nsm", rf_line)
    );
}

#[test]
fn the_bill_of_materials_says_which_generic_a_picked_part_stands_for() {
    let (_root, editor) = project("bom");
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let bom = editor.bom(&analysis.boards[0].name).unwrap();
    let coax = bom
        .lines
        .iter()
        .find(|l| l.kind == "BetaCoax")
        .expect("the picked part");
    assert_eq!(coax.via.as_deref(), Some("CoaxReceptacle"));
    assert_eq!(coax.mpn.as_deref(), Some("BETA-2"));
    assert!(bom.lines.iter().all(|l| l.kind != "AlphaCoax"));
}

#[test]
fn a_part_written_by_its_own_type_has_no_via() {
    let (root, _editor) = project("by-type");
    fs::write(
        root.join("src/main.nsm"),
        MAIN.replace(".rf: CoaxReceptacle {", ".rf: BetaCoax {"),
    )
    .unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let bom = editor.bom(&analysis.boards[0].name).unwrap();
    let coax = bom.lines.iter().find(|l| l.kind == "BetaCoax").unwrap();
    assert_eq!(coax.via, None);
}

/// A project of the files given, `src/main.nsm` its only top-level file.
fn project_of(
    name: &str,
    files: &[(&str, &str)],
    catalogs: &[(&str, &str)],
) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("providers-editor-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("catalogs")).unwrap();
    let listed: Vec<String> = catalogs
        .iter()
        .map(|(file, _)| format!("\"catalogs/{file}\""))
        .collect();
    fs::write(
        root.join("netsmith.toml"),
        format!(
            "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\ncatalogs = [{}]\n",
            listed.join(", ")
        ),
    )
    .unwrap();
    for (file, text) in files {
        fs::write(root.join(file), text).unwrap();
    }
    for (file, text) in catalogs {
        fs::write(root.join("catalogs").join(file), text).unwrap();
    }
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn every_capacitor_of_a_picked_bank_carries_the_via_and_the_bank_is_one_line() {
    // A provider of `Capacitor` is a bank when the design says how many. The first capacitor is
    // at the use's path and the others at `bulk#2` and `bulk#3`; all three are one line.
    let poly = "\
import std::parts::passive::Capacitor

/// An invented polymer capacitor.
part PolyCap provides Capacitor {
    pins {
        .a passive \"1\" \"1\"
        .b passive \"2\" \"2\"
    }
    catalog poly_cap { .capacitance, .max_voltage, .package }
}
";
    let main = "\
import std::parts::passive::Capacitor
import std::sources::Rail
import src::poly::PolyCap

board Bulk {
    .supply: Rail {
        .voltage &= 5V
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }
    .bulk: Capacitor {
        .package &= \"CASE_B\"
        .count &= 3
    }
    .supply.pos ~> .bulk ~> .supply.neg
}
";
    let csv = "mpn,name,capacitance,max_voltage,package,library,source\nPC-100U,PC 100uF 10V,100uF +/- 20%,10V,CASE_B,Extended,https://example.invalid/pc-100u\n";
    let (_root, editor) = project_of(
        "bank-via",
        &[("src/poly.nsm", poly), ("src/main.nsm", main)],
        &[("poly_cap.csv", csv)],
    );
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let bom = editor.bom(&analysis.boards[0].name).unwrap();
    let caps: Vec<_> = bom.lines.iter().filter(|l| l.kind == "PolyCap").collect();
    assert_eq!(caps.len(), 1, "{:?}", bom.lines);
    assert_eq!(caps[0].quantity, 3);
    assert_eq!(caps[0].designators, ["C1", "C2", "C3"]);
    assert_eq!(caps[0].via.as_deref(), Some("Capacitor"));
}

#[test]
fn a_provider_that_wraps_its_own_generic_is_an_error_in_the_editor_and_not_a_crash() {
    let buck = "import std::sources::Rail\n\ngeneric BuckConverter {\n    .vin: Rail\n    .vout: Rail\n}\n";
    let wrapper = "import src::buck::BuckConverter\n\nmodule AFilteredBuck provides BuckConverter {\n    .inner: BuckConverter\n    .inner.vin ~ .vin\n    .inner.vout ~ .vout\n}\n";
    let real = "import src::buck::BuckConverter\n\nmodule Zbk9Buck provides BuckConverter {\n}\n";
    let main = "import src::buck::BuckConverter\nimport src::a_filtered::AFilteredBuck\nimport src::zbk9::Zbk9Buck\n\nboard Feed {\n    .buck: BuckConverter\n}\n";
    let (_root, editor) = project_of(
        "wrapper",
        &[
            ("src/buck.nsm", buck),
            ("src/a_filtered.nsm", wrapper),
            ("src/zbk9.nsm", real),
            ("src/main.nsm", main),
        ],
        &[],
    );
    // The main file, and the wrapper's own file as the root of a check (it is an importable
    // module of the project, so the editor checks it by itself too).
    let analysis = editor.analysis();
    assert!(analysis.errors >= 1, "{:?}", analysis.problems);
    assert!(
        analysis.problems.iter().any(|p| p
            .message
            .contains("`AFilteredBuck` would contain itself without end")),
        "{:?}",
        analysis.problems
    );
    let alone = editor
        .check("src/a_filtered.nsm", wrapper)
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert!(
        alone
            .problems
            .iter()
            .any(|p| p.message.contains("would contain itself without end")),
        "{:?}",
        alone.problems
    );
}
