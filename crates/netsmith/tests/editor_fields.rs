//! The fields the editor's page asked for in its UI rounds (ADR 0019, "Fields added for the
//! UI"). Every one is optional and left out of the JSON when empty.

use std::fs;

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith_editor::Backend;
use netsmith_testkit::TempDir;

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

/// A project: `two_boards.nsm` as its one top-level file.
fn project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-fields-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), TWO_BOARDS).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn the_project_says_which_netsmith_serves_it_and_where() {
    let (root, editor) = project("project");
    let tree = editor.project();
    assert_eq!(tree.version.as_deref(), Some(env!("CARGO_PKG_VERSION")));
    let shown = tree.root.expect("the project folder");
    assert!(std::path::Path::new(&shown).is_absolute(), "{shown}");
    assert_eq!(
        std::fs::canonicalize(&shown).unwrap(),
        std::fs::canonicalize(&*root).unwrap()
    );
    // Where kicad-cli was found is there exactly when it was found.
    assert_eq!(
        tree.kicad_path,
        netsmith_fab::KicadCliExe::locate().map(|k| k.exe().display().to_string())
    );
    assert_eq!(tree.kicad, tree.kicad_path.is_some());
    // The backend leaves the facts of the binary's page and checkout to the server.
    assert_eq!(tree.bundle_hash, None);
    assert_eq!(tree.git_describe, None);
}

#[test]
fn a_single_file_names_its_folder_as_the_root() {
    let dir = TempDir::new("editor-fields-single");
    fs::write(dir.join("one.nsm"), TWO_BOARDS).unwrap();
    let editor = Workspace::open(Target::resolve_from(&dir, dir.join("one.nsm").to_str()).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    let shown = editor.project().root.expect("a folder");
    assert_eq!(
        std::fs::canonicalize(&shown).unwrap(),
        std::fs::canonicalize(&*dir).unwrap()
    );
}

#[test]
fn the_new_project_fields_are_left_out_of_the_json_when_empty() {
    let project = netsmith_editor::api::Project {
        title: "t".into(),
        has_manifest: false,
        kicad: false,
        files: Vec::new(),
        folders: Vec::new(),
        version: None,
        git_describe: None,
        root: None,
        kicad_path: None,
        bundle_hash: None,
    };
    let json = serde_json::to_value(&project).unwrap();
    for key in ["version", "gitDescribe", "root", "kicadPath", "bundleHash"] {
        assert!(json.get(key).is_none(), "{key}: {json}");
    }
    let full = netsmith_editor::api::Project {
        kicad_path: Some("/usr/bin/kicad-cli".into()),
        bundle_hash: Some("8bfd81b2f58bf920".into()),
        git_describe: Some("v0.1.0-3-gabc".into()),
        ..project
    };
    let json = serde_json::to_value(&full).unwrap();
    assert_eq!(json["kicadPath"], "/usr/bin/kicad-cli");
    assert_eq!(json["bundleHash"], "8bfd81b2f58bf920");
    assert_eq!(json["gitDescribe"], "v0.1.0-3-gabc");
}

fn line_of(text: &str, needle: &str) -> u32 {
    text.lines().position(|l| l.contains(needle)).unwrap() as u32 + 1
}

/// The line of the first line that is exactly `needle`, from line `from`.
fn line_after(text: &str, from: u32, needle: &str) -> u32 {
    text.lines()
        .enumerate()
        .skip(from as usize)
        .find(|(_, l)| *l == needle)
        .map(|(i, _)| i as u32 + 1)
        .unwrap()
}

#[test]
fn a_definition_says_where_it_ends_and_what_is_declared_in_it() {
    let (_root, editor) = project("outline");
    let tree = editor.project();
    let main = tree
        .files
        .iter()
        .find(|f| f.path == "src/main.nsm")
        .unwrap();
    let ldo = main.defs.iter().find(|d| d.name == "LdoStage").unwrap();
    let start = line_of(TWO_BOARDS, "module LdoStage");
    assert_eq!(ldo.line, start);
    assert_eq!(ldo.end_line, Some(line_after(TWO_BOARDS, start, "}")));
    let children: Vec<(&str, &str, Option<&str>)> = ldo
        .children
        .as_ref()
        .unwrap()
        .iter()
        .map(|c| (c.name.as_str(), c.type_name.as_str(), c.kind.as_deref()))
        .collect();
    // `Rail` is imported from the standard library: its kind is read from there.
    assert_eq!(children[0], ("vin", "Rail", Some("interface")));
    assert_eq!(children[1], ("vout", "Rail", Some("interface")));
    assert_eq!(children[2].0, "regulator");
    assert_eq!(children[2].1, "LinearRegulator");
    assert!(children[2].2.is_some(), "an imported definition has a kind");
    assert_eq!(
        children.iter().map(|c| c.0).collect::<Vec<_>>(),
        ["vin", "vout", "regulator", "input_cap", "output_cap"]
    );
    let vin = &ldo.children.as_ref().unwrap()[0];
    assert_eq!(vin.line, line_of(TWO_BOARDS, ".vin:  Rail"));
    assert_eq!(vin.end_line, vin.line);

    // A declaration of a definition of the same file takes its kind from there; a
    // declaration with a block ends on its closing line.
    let board = main.defs.iter().find(|d| d.name == "PowerBoard").unwrap();
    let kids = board.children.as_ref().unwrap();
    let stage = kids.iter().find(|c| c.name == "stage").unwrap();
    assert_eq!(
        (stage.type_name.as_str(), stage.kind.as_deref()),
        ("LdoStage", Some("module"))
    );
    let first = line_of(TWO_BOARDS, ".stage: LdoStage {");
    assert_eq!(stage.line, first);
    assert_eq!(stage.end_line, first + 2);
    // Only the declarations written directly in the body are children.
    assert!(kids.iter().all(|c| !c.name.contains("thickness")));
    // The system's members are boards.
    let system = main.defs.iter().find(|d| d.name == "TwoBoards").unwrap();
    assert_eq!(
        system.children.as_ref().unwrap()[0].kind.as_deref(),
        Some("board")
    );
}

#[test]
fn a_definition_with_no_declaration_has_no_children_key_and_the_new_keys_are_camel_case() {
    let (_root, editor) = project("outline-json");
    let tree = editor.project();
    let json = serde_json::to_value(&tree).unwrap();
    let defs = json["files"][0]["defs"].as_array().unwrap();
    assert!(defs.iter().all(|d| d.get("endLine").is_some()));
    let child = &defs[0]["children"][0];
    assert!(
        child.get("endLine").is_some() && child.get("typeName").is_some(),
        "{child}"
    );
    let bare = netsmith_editor::api::Def {
        kind: "module".into(),
        name: "M".into(),
        line: 1,
        end_line: Some(1),
        children: None,
    };
    assert!(
        serde_json::to_value(&bare)
            .unwrap()
            .get("children")
            .is_none()
    );
    // Standard library files are listed with their definitions too.
    let std_file = tree
        .files
        .iter()
        .find(|f| f.path == "std/sources.nsm")
        .unwrap();
    assert!(
        std_file
            .defs
            .iter()
            .all(|d| d.end_line.is_some_and(|e| e >= d.line))
    );
}

#[test]
fn a_definition_answer_carries_a_signature_and_the_doc_comment() {
    let (_root, editor) = project("hover");
    let find = |needle: &str, nth: usize, plus: usize| {
        let offset = TWO_BOARDS.match_indices(needle).nth(nth).unwrap().0 + plus;
        editor
            .definition("src/main.nsm", TWO_BOARDS, offset)
            .unwrap()
            .unwrap_or_else(|| panic!("nothing at {needle}"))
    };
    // A type of this file: the ports are the declarations of an interface type, then `...`
    // for the parts inside; the doc is the `///` line above the module.
    let ldo = find("LdoStage", 1, 1);
    assert_eq!(
        ldo.signature.as_deref(),
        Some("module LdoStage { .vin: Rail .vout: Rail ... }")
    );
    assert_eq!(
        ldo.doc.as_deref(),
        Some("A linear regulator with a capacitor on its input and on its output.")
    );
    // A board: its rails and its process are the declarations of interfaces.
    let board = find("PowerBoard", 1, 1);
    assert_eq!(
        board.signature.as_deref(),
        Some("board PowerBoard { .process: TwoLayer .input: Rail .output: Rail ... }")
    );
    assert_eq!(
        board.doc.as_deref(),
        Some("Turns the 5 V it is given into 3.3 V for the other board.")
    );
    // A definition of the library, from its import: an interface lists its own members, six
    // at most; the whole doc comment is there, one line to a line, and the file is not needed.
    let rail = find("Rail", 0, 1);
    let signature = rail.signature.unwrap();
    assert_eq!(
        signature,
        "interface Rail { .pos: Pin .neg: Pin .voltage: Voltage .current: Current .peak: Voltage .reverse_peak: Voltage ... }"
    );
    // A generic's other statements (its pin table, catalog) are the `...`.
    let capacitor = find("Capacitor", 1, 1).signature.unwrap();
    assert!(
        capacitor.starts_with("generic Capacitor { .capacitance: Capacitance")
            && capacitor.ends_with(" ... }"),
        "{capacitor}"
    );
    let doc = rail.doc.unwrap();
    assert!(doc.lines().count() > 3 && !doc.starts_with(' '), "{doc}");
    assert!(doc.contains("nothing on the board drives"), "{doc}");
    // A member is its declaration, with its own doc comment if it has one.
    let vout = find(".stage.vout", 0, 8);
    assert_eq!(vout.signature.as_deref(), Some(".vout: Rail"));
    assert_eq!(vout.doc, None);
    let input = find(".stage.vout", 0, 2);
    assert_eq!(input.signature.as_deref(), Some(".stage: LdoStage"));
    // An imported module has no signature; its file's `//!` lines are its doc, if it has any.
    let module = find("passive", 0, 1);
    assert_eq!(module.signature, None);
    assert_eq!(
        module.doc.as_deref().map(|d| d.lines().next()),
        Some(Some("Two-terminal parts."))
    );
}

#[test]
fn the_signature_and_doc_are_left_out_of_the_json_when_there_is_nothing_to_say() {
    let location = netsmith_editor::api::Location {
        file: "a.nsm".into(),
        line: 1,
        column: 1,
        end_line: 1,
        end_column: 2,
    };
    let bare = netsmith_editor::api::Definition {
        at: location,
        signature: None,
        doc: None,
    };
    let json = serde_json::to_value(&bare).unwrap();
    assert!(
        json.get("signature").is_none() && json.get("doc").is_none(),
        "{json}"
    );
}

#[test]
fn a_signature_names_six_declarations_at_most_and_an_empty_body_is_braces() {
    let (_root, editor) = project("hover-six");
    let text = "interface Wide {\n    .a: Pin\n    .b: Pin\n    .c: Pin\n    .d: Pin\n    .e: Pin\n    .f: Pin\n    .g: Pin\n}\n\nmodule Empty {}\n\nboard B {\n    .w: Wide\n    .e: Empty\n}\n";
    let hover = |needle: &str| {
        let offset = text.rfind(needle).unwrap() + 1;
        editor
            .definition("src/main.nsm", text, offset)
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        hover("Wide").signature.as_deref(),
        Some("interface Wide { .a: Pin .b: Pin .c: Pin .d: Pin .e: Pin .f: Pin ... }")
    );
    assert_eq!(hover("Empty").signature.as_deref(), Some("module Empty {}"));
    assert_eq!(hover("Empty").doc, None);
}

const MCU_MODULE: &str = r#"import std::sources::Rail

/// A part with a pin that has several functions. Illustrative; not a real part.
part Mcu {
    .vdd: Rail
    pins {
        .vdd.pos power         "VDD" "1"
        .vdd.neg power         "VSS" "2"
        .pa2     bidirectional "PA2" "3" fn GPIO UART1_TX
        .pa3     bidirectional "PA3" "4" fn GPIO UART1_RX
    }
}
"#;

const MCU_BOARD: &str = r#"import src::mcu::Mcu
import std::parts::passive::Resistor
import std::sources::Rail

board Uart {
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .u: Mcu
    .load: Resistor
    .supply ~ .u.vdd
    .u.pa2 ~ .load.a
    .u.pa2.function &= "UART1_TX"
    .u.pa3 ~ .load.b
    .u.pa3.function &= "GPIO"
}
"#;

/// A project of the files named, the first being the top-level one.
fn project_of(name: &str, files: &[(&str, &str)]) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-fields-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    let top: Vec<String> = files
        .iter()
        .take(1)
        .map(|(f, _)| format!("\"{f}\""))
        .collect();
    fs::write(
        root.join("netsmith.toml"),
        format!(
            "[project]\nname = \"demo\"\ntop_level = [{}]\n",
            top.join(", ")
        ),
    )
    .unwrap();
    for (file, text) in files {
        fs::write(root.join(file), text).unwrap();
    }
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn a_net_names_the_datasheet_pin_of_each_pad_and_the_function_the_board_chose() {
    let (_root, editor) = project("pins");
    let board = editor.board("TwoBoards/power").unwrap();
    let pins = |net: &str| {
        board
            .nets
            .iter()
            .find(|n| n.name == net)
            .unwrap()
            .pins
            .iter()
            .map(|p| {
                format!(
                    "{}.{}={}",
                    p.refdes,
                    p.pin,
                    p.name.as_deref().unwrap_or("-")
                )
            })
            .collect::<Vec<_>>()
    };
    // Pad 2 of the regulator is its output, pad 3 its input, pad 1 its ground.
    assert!(
        pins("3V3").contains(&"U1.2=OUT".to_string()),
        "{:?}",
        pins("3V3")
    );
    assert!(pins("VIN").contains(&"U1.3=IN".to_string()));
    assert!(pins("GND").contains(&"U1.1=GND".to_string()));
    // A pin with no function chosen has no `function`.
    assert!(
        board
            .nets
            .iter()
            .flat_map(|n| &n.pins)
            .all(|p| p.function.is_none())
    );

    // A part with several functions per pin: the board's choice is named beside the pad.
    let (_root, editor) = project_of(
        "functions",
        &[("src/main.nsm", MCU_BOARD), ("src/mcu.nsm", MCU_MODULE)],
    );
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let board = editor.board("Uart").unwrap();
    let pin = |refdes: &str, pad: &str| {
        board
            .nets
            .iter()
            .flat_map(|n| &n.pins)
            .find(|p| p.refdes == refdes && p.pin == pad)
            .unwrap_or_else(|| panic!("{refdes}.{pad}"))
            .clone()
    };
    let u = board
        .parts
        .iter()
        .find(|p| p.kind == "Mcu")
        .unwrap()
        .refdes
        .clone();
    let tx = pin(&u, "3");
    assert_eq!(
        (tx.name.as_deref(), tx.function.as_deref()),
        (Some("PA2"), Some("UART1_TX"))
    );
    let gpio = pin(&u, "4");
    assert_eq!(
        (gpio.name.as_deref(), gpio.function.as_deref()),
        (Some("PA3"), Some("GPIO"))
    );
    let vdd = pin(&u, "1");
    assert_eq!((vdd.name.as_deref(), vdd.function), (Some("VDD"), None));
}

#[test]
fn a_pin_with_no_name_or_function_leaves_them_out_of_the_json() {
    let bare = netsmith_editor::api::PinInfo {
        refdes: "R1".into(),
        pin: "1".into(),
        name: None,
        function: None,
    };
    assert_eq!(
        serde_json::to_string(&bare).unwrap(),
        r#"{"refdes":"R1","pin":"1"}"#
    );
}

#[test]
fn a_part_says_what_the_solver_worked_out_for_it() {
    let (_root, editor) = project("solved");
    let power = editor.board("TwoBoards/power").unwrap();
    let part = |board: &netsmith_editor::api::BoardDetail, refdes: &str| {
        board
            .parts
            .iter()
            .find(|p| p.refdes == refdes)
            .unwrap()
            .clone()
    };

    // A capacitor's capacitance in use after the DC bias across it, which the rail gave.
    let c1 = part(&power, "C1").solved.expect("C1 was solved");
    let effective = c1.effective_capacitance.expect("derated");
    assert_eq!(effective.text, "1.72uF..2.916uF");
    assert!((effective.lo - 1.72e-6).abs() < 1e-8 && (effective.hi - 2.916e-6).abs() < 1e-8);
    // The 5 V rail with its 5 % tolerance is the bias.
    assert_eq!(c1.bias.expect("the bias").text, "4.75V..5.25V");
    // The tolerance of the part the solver chose: 8 uF..12 uF round 10 uF.
    let tolerance = c1.tolerance.expect("a tolerance");
    assert_eq!(
        (tolerance.percent, tolerance.text.as_str()),
        (20.0, "+/- 20%")
    );
    let ratings: Vec<(&str, &str)> = c1
        .ratings
        .as_ref()
        .unwrap()
        .iter()
        .map(|r| (r.name.as_str(), r.range.text.as_str()))
        .collect();
    assert_eq!(ratings, [("max_voltage", "10V")]);
    let row = c1.row.expect("a catalog row");
    assert_eq!(row.catalog.as_deref(), Some("capacitor"));
    assert_eq!(row.lcsc.as_deref(), Some("C315248"));
    assert_eq!(row.mpn.as_deref(), Some("CL05A106MP5NUNC"));
    assert_eq!(row.library.as_deref(), Some("Extended"));
    assert!(!row.generic);
    // The row's number is also the part's own `lcsc`.
    assert_eq!(part(&power, "C1").lcsc, row.lcsc);

    // A regulator is not derated; it has ratings and a row.
    let u1 = part(&power, "U1").solved.unwrap();
    assert!(u1.effective_capacitance.is_none() && u1.bias.is_none() && u1.tolerance.is_none());
    let ratings: Vec<(&str, &str)> = u1
        .ratings
        .as_ref()
        .unwrap()
        .iter()
        .map(|r| (r.name.as_str(), r.range.text.as_str()))
        .collect();
    assert_eq!(ratings, [("max_current", "100mA"), ("max_power", "250mW")]);
    assert_eq!(u1.row.unwrap().mpn.as_deref(), Some("XC6206P332MR-G"));

    // A resistor has a tolerance and a power rating, no derating.
    let led = editor.board("TwoBoards/led").unwrap();
    let r1 = part(&led, "R1").solved.unwrap();
    assert_eq!(r1.tolerance.unwrap().text, "+/- 1%");
    assert_eq!(r1.ratings.unwrap()[0].range.text, "62.5mW");
    assert!(r1.effective_capacitance.is_none());

    // A connector has no derived value, only its row.
    let j1 = part(&led, "J1").solved.unwrap();
    assert!(j1.tolerance.is_none() && j1.ratings.is_none() && j1.effective_capacitance.is_none());
    assert_eq!(j1.row.unwrap().lcsc.as_deref(), Some("C42431841"));
}

#[test]
fn what_the_source_states_is_not_reported_as_solved() {
    // A capacitor whose range the board states keeps no solved tolerance; the solver
    // still derives what it worked out.
    let text = TWO_BOARDS.replace(
        ".input_cap:  Capacitor { .effective_capacitance >= 1uF }",
        ".input_cap:  Capacitor { .effective_capacitance >= 1uF; .capacitance &= 10uF +/- 20% }",
    );
    assert_ne!(text, TWO_BOARDS);
    let (_root, editor) = project_of("stated", &[("src/main.nsm", &text)]);
    assert_eq!(
        editor.analysis().errors,
        0,
        "{:?}",
        editor.analysis().problems
    );
    let power = editor.board("TwoBoards/power").unwrap();
    let c1 = power.parts.iter().find(|p| p.refdes == "C1").unwrap();
    let solved = c1.solved.as_ref().unwrap();
    assert!(solved.tolerance.is_none(), "{solved:?}");
    assert!(solved.effective_capacitance.is_some());
}

#[test]
fn a_part_with_nothing_solved_leaves_the_key_out_of_the_json() {
    let bare = netsmith_editor::api::Solved {
        effective_capacitance: None,
        bias: None,
        tolerance: None,
        ratings: None,
        row: None,
    };
    assert_eq!(serde_json::to_string(&bare).unwrap(), "{}");
    let row = netsmith_editor::api::SolvedRow {
        catalog: None,
        mpn: None,
        lcsc: Some("C1".into()),
        library: None,
        generic: true,
    };
    assert_eq!(
        serde_json::to_string(&row).unwrap(),
        r#"{"lcsc":"C1","generic":true}"#
    );
}

#[test]
fn a_module_says_which_boards_use_it() {
    // `LdoStage` in a module file of its own, used by two boards, one of which is a member of a
    // system (a board a system places is built as its member only), and by nothing else.
    let start = TWO_BOARDS.find("/// A linear regulator").unwrap();
    let end = TWO_BOARDS.find("/// Turns the 5 V").unwrap();
    let ldo = format!(
        "import std::parts::passive::Capacitor\nimport std::parts::regulator::LinearRegulator\nimport std::sources::Rail\n\n{}",
        &TWO_BOARDS[start..end]
    );
    let power_start = end;
    let power_end = TWO_BOARDS.find("/// A status LED").unwrap();
    let power = &TWO_BOARDS[power_start..power_end];
    let main = format!(
        "import src::ldo::LdoStage\nimport std::parts::connector::Header1x2\nimport std::sources::Rail\nimport std::mfg::jlcpcb::TwoLayer\n\n{power}\n{}\nsystem Pair {{\n    .a: PowerBoard\n}}\n",
        power.replace("PowerBoard", "OtherBoard")
    );
    let (_root, editor) = project_of("uses", &[("src/main.nsm", &main), ("src/ldo.nsm", &ldo)]);
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let uses = analysis.module_uses.expect("a module is used");
    assert_eq!(uses.len(), 1, "{uses:?}");
    let ldo_use = &uses[0];
    assert_eq!(
        (ldo_use.name.as_str(), ldo_use.file.as_str()),
        ("LdoStage", "src/ldo.nsm")
    );
    assert_eq!(ldo_use.line, line_of(&ldo, "module LdoStage"));
    // In the order of `boards`: a board of a system is named as the system's member.
    let boards: Vec<&str> = analysis.boards.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(boards, ["OtherBoard", "Pair/a"]);
    assert_eq!(ldo_use.used_by, boards);
    // It is the definition the tree lists, by file and line.
    let tree = editor.project();
    let file = tree.files.iter().find(|f| f.path == "src/ldo.nsm").unwrap();
    let def = file.defs.iter().find(|d| d.name == "LdoStage").unwrap();
    assert_eq!(def.line, ldo_use.line);
}

#[test]
fn a_project_with_no_module_in_use_leaves_module_uses_out() {
    let (_root, editor) = project_of(
        "empty",
        &[(
            "src/main.nsm",
            "import std::sources::Rail\n\nboard Empty {\n    .r: Rail\n}\n",
        )],
    );
    let analysis = editor.analysis();
    assert_eq!(analysis.module_uses, None);
    assert!(
        serde_json::to_value(&analysis)
            .unwrap()
            .get("moduleUses")
            .is_none()
    );
    // And a project that uses one says it in camel case.
    let (_root, editor) = project("uses-json");
    let json = serde_json::to_value(editor.analysis()).unwrap();
    assert_eq!(json["moduleUses"][0]["name"], "LdoStage");
    assert_eq!(json["moduleUses"][0]["usedBy"][0], "TwoBoards/power");
}
