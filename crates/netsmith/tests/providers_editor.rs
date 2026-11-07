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
