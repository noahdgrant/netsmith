//! How the fill of the board's pours joins each pad, in the editor's PCB summary tab (issue
//! #1187): `GET /api/pcb/fill` answers what `netsmith board fill` prints, of the board as the PCB
//! view has it placed (and routed), and its starved thermal reliefs are the design rules check's
//! findings of that board (which the editor runs once the board is routed).

use std::fs;
use std::sync::Arc;

use netsmith_editor::{Backend, Say};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

const POURS: &str = "OutlineAndPours";

/// The four-layer example (ground pours on both outer layers, ground and 3.3 V planes), its top
/// pour's clearance raised to 1.5 mm so that it cuts the spokes of ground pads beside other nets'.
fn starving_project(name: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-fill-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"fill\"\n").unwrap();
    let source = include_str!("../../../examples/outline_and_pours.nsm").replace(
        "        .top.stitch_pitch &= 0mm..8mm\n",
        "        .top.stitch_pitch &= 0mm..8mm\n        .top.clearance &= 1.5mm\n",
    );
    assert!(source.contains(".top.clearance"), "the example's pour moved");
    for (file, text) in [
        ("main.nsm", source.as_str()),
        (
            "main.physical.nsm",
            include_str!("../../../examples/outline_and_pours.physical.nsm"),
        ),
        (
            "main.pcb.toml",
            include_str!("../../../examples/outline_and_pours.pcb.toml"),
        ),
        (
            "main.lock.toml",
            include_str!("../../../examples/outline_and_pours.lock.toml"),
        ),
    ] {
        fs::write(root.join("src").join(file), text).unwrap();
    }
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

#[test]
fn the_fill_of_the_placed_board_names_the_pads_the_check_finds_starved() {
    let (_root, editor) = starving_project("starved");
    let fill = editor.pcb_fill(POURS).unwrap();
    assert_eq!(fill.board, POURS);
    assert!(!fill.routed);
    let fills: Vec<(&str, &str, &str)> = fill
        .fills
        .iter()
        .map(|f| (f.layer.as_str(), f.source.as_str(), f.net.as_str()))
        .collect();
    assert_eq!(
        fills,
        [
            ("top", "pour", "GND"),
            ("inner1", "plane", "GND"),
            ("inner2", "plane", "3V3"),
            ("bottom", "pour", "GND"),
        ]
    );
    let mut starved: Vec<String> = fill.starved.iter().map(|s| s.part.clone()).collect();
    starved.sort();
    assert!(!starved.is_empty(), "{fill:?}");
    assert_eq!(fill.counts.starved as usize, starved.len());

    // Routed here, the board is checked, and its fill asked again.
    let say: Say = Arc::new(|_: &str| {});
    editor.route(POURS, &say).unwrap();
    let fill = editor.pcb_fill(POURS).unwrap();
    assert!(fill.routed);
    let mut starved: Vec<String> = fill.starved.iter().map(|s| s.part.clone()).collect();
    starved.sort();
    let findings = editor.pcb_findings(POURS).unwrap();
    assert!(findings.routed);
    let mut found: Vec<String> = findings
        .findings
        .iter()
        .filter(|f| f.rule == "starved_thermal")
        .flat_map(|f| f.parts.clone())
        .collect();
    found.sort();
    assert_eq!(starved, found, "{fill:?}");
    assert_eq!(editor.pcb_fill("Nope").unwrap_err().status, 404);
}

/// A buck converter on a two-layer board poured with ground on both faces: the IC's one ground
/// pad hangs on a thermal relief of the top pour.
#[test]
fn the_fill_warns_of_a_switching_regulators_ground_pad_on_a_relief() {
    let root = TempDir::new("editor-fill-regulator");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"fill\"\n").unwrap();
    fs::write(
        root.join("src/main.nsm"),
        include_str!("../../../scripts/editor-verify-js/fixtures/buck_pours.nsm"),
    )
    .unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));

    let fill = editor.pcb_fill("BuckPours").unwrap();
    let [ground] = fill.regulator_grounds.as_slice() else {
        panic!("one ground pad of the regulator is on a relief: {fill:?}");
    };
    assert_eq!(
        (
            ground.path.as_str(),
            ground.layer.as_str(),
            ground.net.as_str(),
            ground.pad.as_str(),
            ground.join.as_str(),
        ),
        ("reg.ic", "top", "GND", "2", "thermal")
    );
    assert_eq!(ground.stated_by, None);
    // It is the pad of that part in the top pour, with the spokes the pour's list says.
    let pad = fill
        .fills
        .iter()
        .filter(|f| f.layer == ground.layer && f.net == ground.net)
        .flat_map(|f| &f.pads)
        .find(|p| p.part == ground.part && p.pad == ground.pad)
        .unwrap();
    assert_eq!(
        (ground.spokes, ground.joined_spokes, ground.at, ground.starved),
        (pad.spokes, pad.joined_spokes, pad.at, pad.starved)
    );
    // The page reads it by its camelCase names.
    let json = serde_json::to_value(&fill).unwrap();
    assert_eq!(
        json["regulatorGrounds"][0]["joinedSpokes"].as_u64(),
        pad.joined_spokes.map(u64::from),
        "{json}"
    );
}

/// A project of the buck converter's board with `extra` added to the board's body, and the
/// routes file `routes` beside it, if any.
fn buck_project(name: &str, extra: &str, routes: Option<&str>) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-fill-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"fill\"\n").unwrap();
    let source = include_str!("../../../scripts/editor-verify-js/fixtures/buck_pours.nsm")
        .replace(
            "import std::mfg::jlcpcb::TwoLayer\n",
            "import std::mfg::jlcpcb::TwoLayer\nimport std::mfg::stackup::PadConnection\n",
        )
        .replace(
            "    .reg.vout ~ .output\n",
            &format!("    .reg.vout ~ .output\n{extra}"),
        );
    assert!(
        source.contains("PadConnection\n") && source.contains(extra),
        "the fixture moved"
    );
    fs::write(root.join("src/main.nsm"), source).unwrap();
    if let Some(routes) = routes {
        fs::write(root.join("src/main.routes.toml"), routes).unwrap();
    }
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

/// A pad whose own pin says how a pour joins it names the pin and its spoke width (issue #1636).
#[test]
fn the_fill_names_the_pin_that_states_a_pads_connection() {
    let (_root, editor) = buck_project(
        "stated",
        "    .reg.ic.vin.neg.pour_connection &= PadConnection::Thermal\n    .reg.ic.vin.neg.spoke_width &= 0.5mm..\n",
        None,
    );
    let json = serde_json::to_value(editor.pcb_fill("BuckPours").unwrap()).unwrap();
    let (stated, others): (Vec<&serde_json::Value>, Vec<&serde_json::Value>) = json["fills"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["pads"].as_array().unwrap())
        .partition(|p| !p["statedBy"].is_null());
    assert!(!stated.is_empty(), "{json}");
    for p in stated {
        assert_eq!(
            (p["statedBy"].as_str(), p["spokeWidth"].as_f64()),
            (Some("reg.ic.vin.neg"), Some(0.5)),
            "{p}"
        );
    }
    assert!(
        others.iter().all(|p| p["spokeWidth"].is_null()),
        "no other pin states one: {json}"
    );
}

/// A thermal relief that one zone joins by nothing and a zone of a higher priority over it joins
/// solid says it is joined elsewhere (issue #1636).
#[test]
fn the_fill_says_a_relief_another_fill_of_its_net_joins() {
    let (_root, editor) = buck_project("elsewhere", "", None);
    let fill = editor.pcb_fill("BuckPours").unwrap();
    let ground = &fill.regulator_grounds[0];
    // In the frame of the routes file: from the outline's top left corner.
    let (x, y) = (
        ground.at.x - fill.outline_at.x,
        ground.at.y - fill.outline_at.y,
    );
    let zone = |priority: u32, pads: &str, from: (f64, f64), to: (f64, f64)| {
        format!(
            "[[BuckPours.nets.GND.zones]]\nlayers = [\"top\"]\npriority = {priority}\nclearance = 0.2\nmin_width = 0.2\npads = \"{pads}\"\nthermal_gap = 0.3\nspoke_width = 0.3\noutline = [[{0}, {1}], [{2}, {1}], [{2}, {3}], [{0}, {3}]]\n\n",
            from.0, from.1, to.0, to.1
        )
    };
    // A thermal zone round the pad, and over the pad a solid one of a higher priority. The
    // record's stamp of the pads is made up, which makes it a stale one, whose zones are filled.
    let routes = format!(
        "version = 1\n\n[BuckPours.nets.GND]\npads = \"x\"\n\n{}{}",
        zone(0, "thermal", (x - 4.0, y - 4.0), (x + 4.0, y + 4.0)),
        zone(1, "solid", (x - 1.5, y - 1.5), (x + 1.5, y + 1.5)),
    );
    let (_root, editor) = buck_project("elsewhere-zones", "", Some(&routes));
    let fill = editor.pcb_fill("BuckPours").unwrap();
    let joins: Vec<(&str, bool, bool)> = fill
        .fills
        .iter()
        .filter(|f| f.layer == "top")
        .flat_map(|f| &f.pads)
        .filter(|p| p.part == ground.part && p.pad == ground.pad)
        .map(|p| (p.join.as_str(), p.joined, p.joined_elsewhere))
        .collect();
    assert!(
        joins.contains(&("solid", true, false)) && joins.contains(&("thermal", false, true)),
        "{joins:?}: {fill:?}"
    );
}

/// Two buck stages on one ground, each IC (an invented one) with its ground on two pins, `GND`
/// (`.vin.neg`, pad 6) and `GND2` (`.gnd2`, pad 7), on a board poured with ground on both faces.
const TWO_GROUND: &str = r#"//! Two buck stages on one ground, each IC with its ground on two pins, on a two-layer board poured
//! with ground on both faces: for the warning of a switching regulator's ground pad on a relief
//! and the pin it names (issue #1667).

import std::mfg::jlcpcb::TwoLayer
import std::mfg::stackup::PadConnection
import std::parts::connector::Header1x2
import std::parts::passive::Capacitor
import std::parts::regulator::{Buck, BuckIc}
import std::sources::Rail

/// An invented buck IC in an SOIC-8 with its ground on two pins: `GND` (pad 6) and `GND2`
/// (pad 7). Not a real part.
part TwoGroundBuck provides BuckIc {
    .vref &= 0.8V +/- 1%
    .fsw &= 1.2MHz +/- 15%
    .min_input_voltage &= 4.5V
    .max_input_voltage &= 60V
    .max_output_current &= 3A
    .efficiency &= 0.85..0.95
    .min_inductance &= 1.5uH
    .min_inductor_saturation &= 3A
    .max_inductor_dcr &= 0.1ohm
    .min_input_capacitance &= 2uF
    .min_output_capacitance &= 10uF
    .vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)
    dc switching_regulator .vin .sw .efficiency

    .gnd2 ~ .vin.neg

    pins {
        //  pin     kind     name   pads
        .bst     passive "BST"  "1" required
        .vin.pos power   "VIN"  "2"
        .en      input   "EN"   "3"
        .pg      output  "PG"   "4"
        .fb      input   "FB"   "5"
        .vin.neg power   "GND"  "6"
        .gnd2    power   "GND2" "7"
        .sw.pos  power   "SW"   "8"
    }

    .package: Package
    .package &= "SOIC-8"

    support {
        .boost: Capacitor { .capacitance &= 100nF +/- 20% }
        .bst ~> .boost ~> .sw.pos
        .en ~ .vin.pos
    }
}

board TwoGround {
    /// JLCPCB's two-layer process, both faces poured with ground.
    .process: TwoLayer { .stackup.thickness &= 1.6mm; .top.net &= "GND"; .bottom.net &= "GND" }

    .input: Rail { .voltage &= 12V +/- 5%; .pos.net &= "VIN"; .neg.net &= "GND" }
    .out_a: Rail { .pos.net &= "5V"; .current &= 0mA..500mA }
    .out_b: Rail { .pos.net &= "3V3"; .current &= 0mA..500mA }

    .a: Buck<TwoGroundBuck> { .vout.voltage &= 5V +/- 5% }
    .b: Buck<TwoGroundBuck> { .vout.voltage &= 3.3V +/- 5% }
    .input ~ .a.vin
    .input ~ .b.vin
    .a.vout ~ .out_a
    .b.vout ~ .out_b

    .power_in: Header1x2
    .power_a: Header1x2
    .power_b: Header1x2
    .input.pos ~ .power_in.p1
    .input.neg ~ .power_in.p2
    .out_a.pos ~ .power_a.p1
    .out_a.neg ~ .power_a.p2
    .out_b.pos ~ .power_b.p1
    .out_b.neg ~ .power_b.p2
}
"#;

/// A project of [`TWO_GROUND`] with `extra` added to the board's body.
fn two_ground_project(name: &str, extra: &str) -> (TempDir, Workspace) {
    let root = TempDir::new(&format!("editor-fill-{name}"));
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"fill\"\n").unwrap();
    // The IC is drawn from its pin table, on a footprint a generator answers.
    fs::write(
        root.join("kicad.toml"),
        "[[part]]\nkind = \"TwoGroundBuck\"\npackage = \"SOIC-8\"\ndefault = true\nfootprint = \"Package_SO:SOIC-8_3.9x4.9mm_P1.27mm\"\n",
    )
    .unwrap();
    let source = TWO_GROUND.replace(
        "    .b.vout ~ .out_b\n",
        &format!("    .b.vout ~ .out_b\n{extra}"),
    );
    assert!(source.contains(extra), "the source moved");
    fs::write(root.join("src/main.nsm"), source).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    (root, editor)
}

/// Issue #1667: each warning of a regulator's ground pad names the IC's pin that has the pad,
/// and stating `Solid` on that pin, as the warning says, clears it.
#[test]
fn following_each_regulator_ground_warning_clears_it() {
    let (_root, editor) = two_ground_project("two-ground", "");
    let fill = editor.pcb_fill("TwoGround").unwrap();
    let mut pins: Vec<(String, String, String)> = fill
        .regulator_grounds
        .iter()
        .map(|g| (g.path.clone(), g.pad.clone(), g.pin.clone()))
        .collect();
    pins.sort();
    assert_eq!(
        pins,
        [
            ("a.ic".into(), "6".into(), "a.ic.vin.neg".into()),
            ("a.ic".into(), "7".into(), "a.ic.gnd2".into()),
            ("b.ic".into(), "6".into(), "b.ic.vin.neg".into()),
            ("b.ic".into(), "7".into(), "b.ic.gnd2".into()),
        ],
        "{fill:?}"
    );
    for g in &fill.regulator_grounds {
        assert_eq!(g.pin_pads, std::slice::from_ref(&g.pad), "{g:?}");
        assert!(
            g.warning.contains(&format!(
                "join it solid with `.pour_connection &= PadConnection::Solid` on its pin `{}`",
                g.pin
            )),
            "{}",
            g.warning
        );
        let (_root, followed) = two_ground_project(
            "two-ground-followed",
            &format!(
                "    .{}.pour_connection &= PadConnection::Solid\n",
                g.pin
            ),
        );
        let after = followed.pcb_fill("TwoGround").unwrap();
        assert!(
            !after
                .regulator_grounds
                .iter()
                .any(|a| (&a.part, &a.pad, &a.layer) == (&g.part, &g.pad, &g.layer)),
            "stating `Solid` on `{}` clears the warning of {}.{}: {:?}",
            g.pin,
            g.part,
            g.pad,
            after.regulator_grounds
        );
        // And it reaches that pad alone of the four.
        assert_eq!(after.regulator_grounds.len(), 3, "{:?}", after.regulator_grounds);
    }
}
