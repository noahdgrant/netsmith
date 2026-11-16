//! The placement requirements in the editor's PCB view (ADR 0045 C4 and C5, issue #815): the
//! scene carries each part's windows, and "Require here" (`PcbChange::Required`) writes a part's
//! place to the board's physical file, refuses one the file states in another shape, and undoes.

use std::fs;

use netsmith_editor::{
    Backend,
    api::{PcbChange, PcbEdit, PcbPart, PcbScene, PcbSide, RequiredPlace},
};
use netsmith_fabricate::project::Target;
use netsmith_pcb::FootprintLibrary;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const BOARD: &str = "Cased";

const SOURCE: &str = "import std::parts::connector::Header1x2\nimport std::mfg::jlcpcb::TwoLayer\n\nboard Cased {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .power: Header1x2\n    .aux: Header1x2\n}\n";

const PHYSICAL: &str = "import std::pcb::Outline\n\nphysical for Cased {\n    case {\n        .outline: Outline {\n            .width &= 40mm\n            .height &= 24mm\n        }\n    }\n\n    // Case drawing, slot A.\n    .power.place.left &= 1.5mm +/- 0.1mm\n    .power.place.y    >= .outline.height / 2 - 0.1mm\n    .power.place.y    <= .outline.height / 2 + 0.1mm\n    .power.place.side &= \"top\"\n}\n";

fn footprints_here() -> bool {
    let here = FootprintLibrary::installed().root().exists();
    if !here {
        eprintln!("skipped: no KiCad footprint libraries");
    }
    here
}

fn project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-required-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"cased\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), SOURCE).unwrap();
    fs::write(root.join("src/main.physical.nsm"), PHYSICAL).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

fn part<'a>(scene: &'a PcbScene, path: &str) -> &'a PcbPart {
    scene.parts.iter().find(|p| p.path == path).unwrap()
}

fn edit(scene: &PcbScene, path: &str, before: RequiredPlace, after: RequiredPlace) -> PcbEdit {
    PcbEdit {
        board: BOARD.into(),
        stamp: scene.layout_stamp.clone(),
        changes: vec![PcbChange::Required {
            path: path.into(),
            before,
            after,
        }],
    }
}

/// Where the part is, as the page states it: the centre of its body from the outline's corner.
fn here(scene: &PcbScene, path: &str) -> RequiredPlace {
    let p = part(scene, path);
    let r = |v: f64| (v * 1e4).round() / 1e4;
    RequiredPlace {
        x: Some(r(p.body.x + p.body.width / 2.0 - scene.origin_x)),
        y: Some(r(p.body.y + p.body.height / 2.0 - scene.origin_y)),
        rotation: Some(p.rotation.rem_euclid(360.0)),
        side: Some(p.side),
    }
}

#[test]
fn the_scene_carries_each_parts_windows_and_whether_the_editor_may_write_its_place() {
    if !footprints_here() {
        return;
    }
    let (_root, editor) = project("scene");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let power = part(&scene, "power");
    let fields: Vec<(&str, Option<f64>, Option<f64>)> = power
        .requirements
        .iter()
        .map(|r| (r.field.as_str(), r.lo, r.hi))
        .collect();
    let close = |a: Option<f64>, b: f64| a.is_some_and(|a| (a - b).abs() < 1e-9);
    assert_eq!(fields.len(), 2, "{fields:?}");
    let left = power.requirements.iter().find(|r| r.field == "left").unwrap();
    assert!(close(left.lo, 1.4) && close(left.hi, 1.6), "{left:?}");
    assert_eq!(left.id, "power.place.left");
    assert_eq!(left.source, "src/main.physical.nsm:12");
    let y = power.requirements.iter().find(|r| r.field == "y").unwrap();
    assert!(close(y.lo, 11.9) && close(y.hi, 12.1), "{y:?}");
    // A window from a drawing is the file's to change.
    let why = power.required_elsewhere.as_deref().unwrap_or_default();
    assert!(
        why.contains("`.power.place.left &= 1.5mm +/- 0.1mm` (main.physical.nsm:12)"),
        "{why}"
    );
    let aux = part(&scene, "aux");
    assert!(aux.requirements.is_empty() && aux.required_elsewhere.is_none());
    assert_eq!(aux.required, RequiredPlace::default());
}

#[test]
fn require_here_writes_the_place_to_the_physical_file_and_undo_takes_it_out() {
    if !footprints_here() {
        return;
    }
    let (root, editor) = project("write");
    let file = root.join("src/main.physical.nsm");
    let scene = editor.pcb_scene(BOARD).unwrap();
    let at = here(&scene, "aux");
    let done = editor
        .pcb_edit(&edit(&scene, "aux", RequiredPlace::default(), at.clone()))
        .unwrap_or_else(|e| panic!("{e:?}"))
        .scene;
    let text = fs::read_to_string(&file).unwrap();
    let x = format!(".aux.place.x &= {}mm\n", at.x.unwrap());
    assert!(text.starts_with(PHYSICAL.trim_end_matches("}\n")), "{text}");
    assert!(text.contains(&x) && text.contains(".aux.place.side &= \"top\"\n"), "{text}");
    // The part stays where it was, now held there by its requirements.
    let aux = part(&done, "aux");
    assert_eq!(aux.required, at);
    assert_eq!(here(&done, "aux"), at);
    assert_eq!(aux.side, PcbSide::Top);
    let fields: Vec<&str> = aux.requirements.iter().map(|r| r.field.as_str()).collect();
    assert_eq!(fields, ["x", "y", "rotation"]);
    assert!(aux.requirements.iter().all(|r| r.lo == r.hi));

    // The same change again is not what the file has now.
    let stale = editor
        .pcb_edit(&edit(&done, "aux", RequiredPlace::default(), at.clone()))
        .unwrap_err();
    assert_eq!(stale.status, 409, "{stale:?}");

    // A part whose place the file states as a window is refused, and nothing is written.
    let refused = editor
        .pcb_edit(&edit(
            &done,
            "power",
            RequiredPlace::default(),
            here(&done, "power"),
        ))
        .unwrap_err();
    assert_eq!(refused.status, 400, "{refused:?}");
    assert!(refused.message.contains("change it there"), "{refused:?}");
    assert_eq!(fs::read_to_string(&file).unwrap(), text);

    // Undo: the change turned round leaves the file as it was.
    let undone = editor
        .pcb_edit(&edit(&done, "aux", at, RequiredPlace::default()))
        .unwrap_or_else(|e| panic!("{e:?}"))
        .scene;
    assert_eq!(fs::read_to_string(&file).unwrap(), PHYSICAL);
    assert!(part(&undone, "aux").requirements.is_empty());
}
