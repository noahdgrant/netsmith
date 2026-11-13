//! Names with letters outside ASCII, through every part of the editor that turns a span (a byte
//! range) into lines, columns or a slice of the text. A span's end can fall inside a character of
//! two or more bytes; the server must answer, not panic (review of night 3, finding 1).

use std::fs;

use netsmith_workspace::Workspace;
use netsmith_fabricate::project::Target;
use netsmith_editor::Backend;
use netsmith_testkit::TempDir;

/// A project with `text` as its one top-level file.
fn project(name: &str, text: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-unicode-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), text).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

/// A declaration without a block ends at its type name, which ends in a two-byte letter, as the
/// last line of the file, of a definition and of a board.
const SOURCE: &str = "/// Une interface.\ninterface Pé {}\n\nmodule Mé {\n    .aux: Pé\n}\n\nboard Bé {\n    .aux: Pé\n    .mod: Mé\n}\n";

fn utf16_offset(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

#[test]
fn a_declaration_ending_in_a_two_byte_letter_does_not_break_the_project_listing() {
    let (_root, editor) = project("outline", SOURCE);
    let tree = editor.project();
    let file = tree
        .files
        .iter()
        .find(|f| f.path == "src/main.nsm")
        .expect("in the tree");
    let def = |name: &str| file.defs.iter().find(|d| d.name == name).unwrap();
    assert_eq!(def("Pé").line, 2);
    assert_eq!(def("Mé").line, 4);
    assert_eq!(def("Mé").end_line, Some(6));
    let children = def("Bé").children.as_ref().expect("the board's members");
    assert_eq!(
        children
            .iter()
            .map(|c| (c.name.as_str(), c.type_name.as_str(), c.line, c.end_line))
            .collect::<Vec<_>>(),
        [("aux", "Pé", 9, 9), ("mod", "Mé", 10, 10)]
    );
    // The same file as the last thing in the text, with no newline after the type name.
    let (_root, editor) = project("outline-end", "interface Pé {}\nmodule M {\n    .aux: Pé");
    let _ = editor.project();
}

#[test]
fn a_file_that_ends_in_a_two_byte_letter_does_not_break_the_project_listing() {
    for text in ["interface Pé {}\nboard B {\n    .aux: Pé", "board Bé", "é"] {
        let (_root, editor) = project("tail", text);
        let _ = editor.project();
        let _ = editor.analysis();
    }
}

#[test]
fn the_hover_on_a_name_in_two_byte_letters_finds_its_definition_and_says_the_columns_in_characters()
{
    let (_root, editor) = project("hover", SOURCE);
    // Their uses in the board: the hover answers with where the definition's name is.
    for (needle, line, column) in [(".aux: Pé", 2, 11), (".mod: Mé", 4, 8)] {
        let at = SOURCE.rfind(needle).unwrap() + needle.len() - "é".len();
        let found = editor
            .definition("src/main.nsm", SOURCE, utf16_offset(SOURCE, at))
            .unwrap()
            .unwrap_or_else(|| panic!("nothing at {needle}"));
        assert_eq!((found.at.line, found.at.column), (line, column), "{needle}");
        assert_eq!(found.at.end_line, line);
    }
    // On the use of `Pé` that ends the declaration (the span's last byte is half a letter).
    let use_at = SOURCE.find(".aux: Pé\n}").unwrap() + ".aux: ".len();
    let found = editor
        .definition("src/main.nsm", SOURCE, utf16_offset(SOURCE, use_at))
        .unwrap()
        .expect("the definition of Pé");
    assert_eq!(found.at.line, 2);
    assert_eq!(found.signature.as_deref(), Some("interface Pé {}"));
    assert_eq!(found.doc.as_deref(), Some("Une interface."));
    // An offset past the end, or between the halves of a letter, answers with nothing.
    for offset in [SOURCE.len() + 10, utf16_offset(SOURCE, SOURCE.len())] {
        let _ = editor.definition("src/main.nsm", SOURCE, offset).unwrap();
    }
}

#[test]
fn a_problem_in_a_name_of_two_byte_letters_has_character_columns() {
    // The check does not know `Résistance`: the problem is on the type name, which ends the line.
    let text = "board Bé {\n    .r: Résistance\n}\n";
    let (_root, editor) = project("problem", text);
    let analysis = editor.analysis();
    assert!(analysis.errors >= 1, "{:?}", analysis.problems);
    let problem = analysis
        .problems
        .iter()
        .find(|p| p.message.contains("Résistance"))
        .unwrap_or_else(|| panic!("{:?}", analysis.problems));
    let at = problem.at.as_ref().expect("a place");
    assert_eq!(at.line, 2, "{at:?}");
    assert_eq!(at.column, 9, "{at:?}");
    assert_eq!(at.end_line, 2, "{at:?}");
    // The rendered message underlines the name under the right columns.
    assert!(
        problem.rendered.contains("Résistance"),
        "{}",
        problem.rendered
    );
}

#[test]
fn a_text_that_stops_inside_a_name_is_a_problem_with_a_place_and_not_a_panic() {
    for text in ["board Bé {\n    .r: é", "board é", "board B {\n    .é"] {
        let (_root, editor) = project("stop", text);
        let analysis = editor.analysis();
        assert!(analysis.errors >= 1, "{text:?}: {:?}", analysis.problems);
        for problem in &analysis.problems {
            if let Some(at) = &problem.at {
                assert!(at.line >= 1 && at.column >= 1, "{at:?}");
            }
        }
    }
}
