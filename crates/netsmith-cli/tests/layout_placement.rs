//! One placement check (issue #1224): what `board fab`'s placement refuses, `layout check` refuses
//! with the same finding, and `layout fit` (placed, and `--as-laid-out`) neither offers nor calls
//! free, on a board with a part of several courtyard shapes.
//!
//! The board is invented: a plug-in module whose two rows of pins, 22 mm apart, each have their
//! own courtyard (so a part may stand between them, and not on either), locked in the layout
//! file, and twelve resistors in a 40 by 30 mm outline. `r1` is locked over the module's left
//! row.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use netsmith_testkit::TempDir;
use serde_json::Value;

const BOARD: &str = "\
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

/// An invented plug-in module: two rows of pins, each with its own courtyard.
part Module {
    .designator &= \"J\"
    pins {
        .p1 passive \"P1\" \"1\"
        .p2 passive \"P2\" \"2\"
        .p3 passive \"P3\" \"3\"
        .p4 passive \"P4\" \"4\"
    }
}

board Widget {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .rail: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= \"VCC\"
        .neg.net &= \"GND\"
    }
    .module: Module {
        .assembly &= AssemblyMode::Hand
    }
    .rail.pos ~ .module.p1
    .rail.neg ~ .module.p2
    .rail.pos ~ .module.p3
    .rail.neg ~ .module.p4
RESISTORS}
";

const PHYSICAL: &str = "\
import std::pcb::Outline

physical for Widget {
    case {
        .outline: Outline {
            .width &= 40mm
            .height &= 30mm
        }
    }
}
";

/// The module's footprint: a row of two pins at x = 0 and another at x = 22, each in a
/// courtyard of its own (x -1.2 to 1.2 and 20.8 to 23.2), and nothing drawn between them.
const FOOTPRINT: &str = r#"(footprint "Module-4" (layer "F.Cu")
  (property "Reference" "REF**" (at 11 -3 0) (layer "F.SilkS") (effects (font (size 1 1))))
  (attr through_hole)
  (pad "1" thru_hole circle (at 0 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask"))
  (pad "2" thru_hole circle (at 0 2.54) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask"))
  (pad "3" thru_hole circle (at 22 0) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask"))
  (pad "4" thru_hole circle (at 22 2.54) (size 1.7 1.7) (drill 1) (layers "*.Cu" "*.Mask"))
  (fp_rect (start -1.2 -1.2) (end 1.2 3.74) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd"))
  (fp_rect (start 20.8 -1.2) (end 23.2 3.74) (stroke (width 0.05) (type solid)) (fill none) (layer "F.CrtYd")))
"#;

const KICAD_TOML: &str = "\
[[library]]
name = \"mod\"
symbols = \"libs/mod.kicad_sym\"
footprints = \"libs/mod.pretty\"

[[part]]
kind = \"Module\"
package = \"mod\"
default = true
symbol = \"mod:Module\"
footprint = \"mod:Module-4\"
";

/// The module locked with its rows at x 7.8 to 10.2 and 29.8 to 32.2, and `r1` locked over the
/// left row.
const OVER: &str = "\
version = 1

[Widget]
seed = 0

[Widget.parts.module]
at = [9.0, 14.95]
rotation = 0.0
side = \"top\"
lock = true
source = \"hand\"

[Widget.parts.r1]
at = [10.5, 16.0]
rotation = 0.0
side = \"top\"
lock = true
source = \"hand\"
";

fn symbol() -> String {
    let font = "(effects (font (size 1.27 1.27)))";
    let prop = |name: &str, value: &str, y: f64| {
        format!("(property \"{name}\" \"{value}\" (at 0 {y} 0) {font})")
    };
    let pins: String = (1..=4)
        .map(|n| {
            format!(
                "(pin passive line (at -7.62 {} 0) (length 2.54) (name \"P{n}\" {font}) \
                 (number \"{n}\" {font})) ",
                5.08 - 2.54 * f64::from(n)
            )
        })
        .collect();
    format!(
        "(kicad_symbol_lib (version 20241209) (generator \"netsmith-test\") \
         (symbol \"Module\" (exclude_from_sim no) (in_bom yes) (on_board yes) {} {} {} {} {} \
         (symbol \"Module_0_1\" (rectangle (start -5.08 7.62) (end 5.08 -7.62) \
         (stroke (width 0.254) (type default)) (fill (type background)))) \
         (symbol \"Module_1_1\" {pins})))\n",
        prop("Reference", "J", 8.89),
        prop("Value", "Module", -8.89),
        prop("Footprint", "", 0.0),
        prop("Datasheet", "~", 0.0),
        prop("Description", "", 0.0),
    )
}

/// The project, with `layout` as its layout file.
fn project(tag: &str, layout: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let resistors: String = (1..=12)
        .map(|n| {
            format!(
                "    .r{n}: Resistor {{ .resistance &= 15kohm +/- 1% }}\n    .rail.pos ~> .r{n} ~> .rail.neg\n"
            )
        })
        .collect();
    fs::write(
        dir.join("netsmith.toml"),
        "[project]\nname = \"widget\"\ntop_level = [\"main.nsm\"]\n",
    )
    .unwrap();
    fs::write(dir.join("main.nsm"), BOARD.replace("RESISTORS", &resistors)).unwrap();
    fs::write(dir.join("main.physical.nsm"), PHYSICAL).unwrap();
    fs::write(dir.join("main.pcb.toml"), layout).unwrap();
    fs::write(dir.join("kicad.toml"), KICAD_TOML).unwrap();
    fs::create_dir_all(dir.join("libs/mod.pretty")).unwrap();
    fs::write(dir.join("libs/mod.kicad_sym"), symbol()).unwrap();
    fs::write(dir.join("libs/mod.pretty/Module-4.kicad_mod"), FOOTPRINT).unwrap();
    dir
}

fn netsmith(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .args(args)
        .current_dir(dir)
        .env("NETSMITH_NO_KICAD", "1")
        .env_remove("KICAD_CLI")
        .output()
        .unwrap()
}

fn json(dir: &Path, args: &[&str]) -> Value {
    let run = netsmith(dir, args);
    serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "{args:?}: {e}: {}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        )
    })
}

/// `board fab` of the project: its exit code and what it said.
fn fab(dir: &Path) -> (i32, String) {
    let run = netsmith(dir, &["board", "fab", "main.nsm", "--out", "fab"]);
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    (run.status.code().unwrap(), said)
}

/// The `placement` findings of `layout check --format json`, and whether it is `ok`.
fn check(dir: &Path) -> (bool, Vec<Value>) {
    let doc = json(dir, &["layout", "check", "--format", "json"]);
    let placement = doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["kind"] == "placement")
        .cloned()
        .collect();
    (doc["ok"].as_bool().unwrap(), placement)
}

/// The designators of a fault's parts.
fn refdes(fault: &Value) -> Vec<&str> {
    fault["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["refdes"].as_str().unwrap())
        .collect()
}

/// The overlap of `r1` and the module's left row, as `layout check` gives it: one finding, with
/// `fab`'s data.
fn the_overlap(dir: &Path) -> Value {
    let (ok, placement) = check(dir);
    assert!(!ok, "{placement:?}");
    assert_eq!(placement.len(), 1, "{placement:?}");
    let finding = &placement[0];
    assert_eq!(finding["path"], "module", "{finding}");
    assert_eq!(finding["fault"]["kind"], "overlap", "{finding}");
    assert_eq!(refdes(&finding["fault"]), ["J1", "R1"], "{finding}");
    // The box the two share is where `r1` meets the row, not a piece of the box round both rows.
    let shared = &finding["fault"]["shared"];
    assert!(shared["x_max"].as_f64().unwrap() <= 10.2 + 1e-6, "{finding}");
    finding.clone()
}

/// Each place `layout fit` offers `r1` (`args` after `layout fit r1`), written as it prints it
/// (locked): `layout check` finds nothing about it. The first is also built by `board fab`, which
/// accepts it. Gives how many places were offered.
fn every_offered_place_is_accepted(dir: &Path, args: &[&str]) -> usize {
    let mut full = vec!["layout", "fit", "r1", "--format", "json"];
    full.extend_from_slice(args);
    let doc = json(dir, &full);
    let before = fs::read_to_string(dir.join("main.pcb.toml")).unwrap();
    let offered: Vec<&Value> = doc["poses"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["fits"] == true)
        .collect();
    assert!(!offered.is_empty(), "{doc}");
    for (k, pose) in offered.iter().enumerate() {
        fs::write(dir.join("main.pcb.toml"), &before).unwrap();
        let at = format!(
            "{},{}",
            pose["at"][0].as_f64().unwrap(),
            pose["at"][1].as_f64().unwrap()
        );
        let turn = pose["turn"].as_f64().unwrap().to_string();
        let side = pose["side"].as_str().unwrap();
        let set = netsmith(
            dir,
            &[
                "layout", "set", "r1", "--at", &at, "--turn", &turn, "--side", side, "--lock",
            ],
        );
        assert!(set.status.success(), "{}", String::from_utf8_lossy(&set.stderr));
        let (ok, placement) = check(dir);
        assert!(ok && placement.is_empty(), "{pose}: {placement:?}");
        if k == 0 {
            let (code, said) = fab(dir);
            assert_eq!(code, 0, "{pose}: {said}");
        }
    }
    fs::write(dir.join("main.pcb.toml"), &before).unwrap();
    offered.len()
}

#[test]
fn layout_check_refuses_with_fabs_own_finding_the_part_over_a_row_of_the_module() {
    let dir = project("one-check-refused", OVER);
    let finding = the_overlap(&dir);
    let message = finding["message"].as_str().unwrap();
    assert!(
        message.contains("the courtyards of `module` (J1) and `r1` (R1) overlap"),
        "{message}"
    );
    // The same sentence stops `fab`.
    let (code, said) = fab(&dir);
    assert_ne!(code, 0, "{said}");
    assert!(said.contains(&format!("error: Widget: {message}")), "{said}");
    // The text names the line of the module's entry.
    let run = netsmith(&dir, &["layout", "check"]);
    assert_eq!(run.status.code(), Some(1));
    let text = String::from_utf8_lossy(&run.stdout);
    assert!(
        text.contains(&format!("main.pcb.toml:6: error: {message}")),
        "{text}"
    );
}

#[test]
fn layout_fit_offers_only_places_layout_check_and_fab_accept() {
    let dir = project("one-check-fit", OVER);
    let finding = the_overlap(&dir);
    // Where it is, `fit` finds the fault `layout check` finds: the same data, the same pair.
    let doc = json(&dir, &["layout", "fit", "r1", "--format", "json"]);
    assert_eq!(doc["now"]["fits"], false, "{doc}");
    assert_eq!(
        doc["now"]["placement"],
        Value::Array(vec![finding["fault"].clone()]),
        "{doc}"
    );
    // The line to write a place locks it, so that `fab`'s placer places the rest round it.
    let text = String::from_utf8_lossy(&netsmith(&dir, &["layout", "fit", "r1"]).stdout).into_owned();
    assert!(
        text.lines()
            .filter(|l| l.contains("netsmith layout set"))
            .all(|l| l.ends_with("--lock")),
        "{text}"
    );
    assert!(every_offered_place_is_accepted(&dir, &[]) > 0);
}

#[test]
fn layout_fit_as_laid_out_offers_only_places_layout_check_and_fab_accept() {
    // Every part with an entry: the module locked, the placer's places of the rest captured,
    // and `r1` locked over the left row again.
    let dir = project(
        "one-check-as-laid-out",
        OVER.split("[Widget.parts.r1]").next().unwrap(),
    );
    let capture = netsmith(&dir, &["layout", "capture"]);
    assert!(
        capture.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let (ok, placement) = check(&dir);
    assert!(ok && placement.is_empty(), "{placement:?}");
    let set = netsmith(
        &dir,
        &[
            "layout", "set", "r1", "--at", "10.5,16", "--turn", "0", "--side", "top", "--lock",
        ],
    );
    assert!(set.status.success(), "{}", String::from_utf8_lossy(&set.stderr));
    let finding = the_overlap(&dir);
    let doc = json(&dir, &["layout", "fit", "r1", "--as-laid-out", "--format", "json"]);
    assert_eq!(
        doc["now"]["placement"],
        Value::Array(vec![finding["fault"].clone()]),
        "{doc}"
    );
    assert!(every_offered_place_is_accepted(&dir, &["--as-laid-out"]) > 0);
}
