//! A module's physical file in a project (ADR 0048, the module layer, issue #813): the file
//! beside the module's source in `src/` is paired with it by stem, and every board that uses
//! the module gets it; a module that names its file and has none is an error.

use std::fs;

use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;

const DOCK: &str = "\
import std::parts::connector::Header1x2

module Dock {
    physical \"dock.physical.nsm\"
    .row_a: Header1x2
    .row_b: Header1x2
}
";

const DOCK_PHYSICAL: &str = "\
physical for Dock {
    .row_b.place.x - .row_a.place.x == 17.5mm +/- 0.05mm
}
";

const MAIN: &str = "\
import src::dock::Dock

board B {
    .d: Dock
}
";

fn project(with_file: bool) -> TempDir {
    let root = TempDir::new("module-physical");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/dock.nsm"), DOCK).unwrap();
    fs::write(root.join("src/main.nsm"), MAIN).unwrap();
    if with_file {
        fs::write(root.join("src/dock.physical.nsm"), DOCK_PHYSICAL).unwrap();
    }
    root
}

fn checked(root: &TempDir) -> netsmith_check::Checked {
    let main = format!("{}/src/main.nsm", root.display());
    let target = Target::resolve_from(root, Some(&main)).unwrap();
    let file = &target.files[0];
    let text = fs::read_to_string(&file.path).unwrap();
    netsmith_check::check(&file.name, &text, &target.loader().unwrap())
}

#[test]
fn a_board_gets_the_physical_file_of_a_module_of_its_project() {
    let root = project(true);
    let c = checked(&root);
    assert!(c.is_ok(), "{}", c.render());
    let from_module = c.boards[0]
        .model
        .relations()
        .iter()
        .filter(|r| c.sources.get(r.source).name == "src/dock.physical.nsm")
        .count();
    assert_eq!(from_module, 1);
}

#[test]
fn a_module_whose_file_is_missing_says_so() {
    let root = project(false);
    let c = checked(&root);
    let messages: Vec<&str> = c.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        [
            "`module Dock` names its physical file `dock.physical.nsm`, and there is no such file beside `src/dock.nsm`; add the file with `physical for Dock { ... }`, or remove this line"
        ]
    );
}
