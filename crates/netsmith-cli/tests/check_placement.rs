//! `board check` and `board drc` make the placement check `board fab` makes, with its words
//! (issue #1654): a part on the bottom under a through-hole part's pins, which come out under
//! it, is refused by all three; a clean placement by none.
//!
//! The board is invented: a plug-in module whose two rows of pins each have their own courtyard,
//! locked on the top in the layout file, and twelve resistors in a 40 by 30 mm outline. `r1` is
//! locked on the bottom under the module's left row.

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

/// The module locked on the top with its left row at x 7.8 to 10.2, and `r1` locked at `r1`.
fn layout(r1: &str) -> String {
    format!(
        "version = 1

[Widget]
seed = 0

[Widget.parts.module]
at = [9.0, 14.95]
rotation = 0.0
side = \"top\"
lock = true
source = \"hand\"

[Widget.parts.r1]
{r1}
lock = true
source = \"hand\"
"
    )
}

/// `r1` on the bottom, under the module's left row.
const UNDER: &str = "at = [9.0, 16.0]\nrotation = 0.0\nside = \"bottom\"";
/// `r1` on the top, clear of the module.
const CLEAR: &str = "at = [16.0, 8.0]\nrotation = 0.0\nside = \"top\"";

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

/// What a run said, stdout then stderr, and its exit code.
fn said(dir: &Path, args: &[&str]) -> (i32, String) {
    let run = netsmith(dir, args);
    (
        run.status.code().unwrap(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        ),
    )
}

/// The messages of the `place` findings of a command's JSON report.
fn placement_findings(dir: &Path, args: &[&str]) -> Vec<String> {
    let run = netsmith(dir, args);
    let doc: Value = serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "{args:?}: {e}: {}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        )
    });
    doc["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "place" && f["severity"] == "error")
        .map(|f| f["message"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn check_and_drc_refuse_a_bottom_part_under_a_through_hole_part_with_fabs_words() {
    let dir = project("check-placement-under", &layout(UNDER));
    let (code, fab) = said(&dir, &["board", "fab", "main.nsm", "--out", "fab"]);
    assert_eq!(code, 1, "{fab}");
    let line = fab
        .lines()
        .find(|l| l.starts_with("error: Widget: the courtyards of"))
        .unwrap_or_else(|| panic!("{fab}"))
        .to_string();
    assert!(
        line.contains("the pins of the through-hole part `module` come out under the other"),
        "{line}"
    );
    let (code, check) = said(&dir, &["board", "check"]);
    assert_eq!(code, 1, "{check}");
    assert!(check.contains(&line), "{check}");
    let (code, drc) = said(&dir, &["board", "drc"]);
    assert_eq!(code, 1, "{drc}");
    assert!(drc.contains(&line), "{drc}");
    assert!(drc.contains("Widget: placement check: 1 fault(s)"), "{drc}");
    let message = line.strip_prefix("error: Widget: ").unwrap();
    for args in [
        &["board", "check", "--format", "json"][..],
        &["board", "drc", "--format", "json"],
    ] {
        assert_eq!(placement_findings(&dir, args), [message], "{args:?}");
    }
}

#[test]
fn a_clean_placement_gets_no_placement_finding_and_check_says_what_it_did_not_judge() {
    let dir = project("check-placement-clear", &layout(CLEAR));
    let (code, check) = said(&dir, &["board", "check"]);
    assert_eq!(code, 0, "{check}");
    assert!(!check.contains("error"), "{check}");
    // Nothing is placed: the eleven resistors the placer places are not judged, and it says so.
    assert!(
        check.contains(
            "note: Widget: the placement check judged the 2 part(s) the layout file places; the 11 part(s) the placer places are not judged"
        ),
        "{check}"
    );
    let (_, drc) = said(&dir, &["board", "drc"]);
    assert!(!drc.contains("placement check:"), "{drc}");
    for args in [
        &["board", "check", "--format", "json"][..],
        &["board", "drc", "--format", "json"],
    ] {
        assert!(placement_findings(&dir, args).is_empty(), "{args:?}");
    }
}
