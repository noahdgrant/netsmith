//! The impedance a design states for tracks (issue #1007) reaches the board's fabrication
//! notes: the six-layer example with a `TrackImpedance` on one net and a `PairImpedance` on two,
//! placed (no KiCad needed).

use netsmith_check::Loader;
use netsmith_pcb::place_options;
use netsmith_pipeline::{Libraries, build_with};
use netsmith_solve::Catalogs;

const SIX_LAYER: &str = include_str!("../../../examples/six_layer.nsm");

/// The six-layer example with a 50 ohm target on its output net and a 90 ohm pair on its input
/// and output nets (a pair that no router would couple, since the names do not say it is one:
/// the target is stated either way).
fn source() -> String {
    SIX_LAYER
        .replace(
            "import std::sources::Rail\n",
            "import std::sources::{PairImpedance, Rail, TrackImpedance}\n",
        )
        .replacen(
            "    .power_in: Header1x2\n",
            "    .z: TrackImpedance {\n        .line.net &= \"3V3\"\n        .impedance &= 50ohm\n    }\n    .pz: PairImpedance {\n        .p.net &= \"VIN\"\n        .n.net &= \"3V3\"\n        .impedance &= 90ohm\n        .spacing &= 0.15mm\n    }\n    .power_in: Header1x2\n",
            1,
        )
}

#[test]
fn a_stated_target_is_in_the_design_and_in_the_fab_notes_with_the_routers_width() {
    let loader = Loader::std();
    let built = build_with(
        "six.nsm",
        &source(),
        &loader,
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0);
    assert_eq!(
        built
            .design
            .features
            .impedance_targets
            .iter()
            .map(|t| (t.path.as_str(), t.nets.clone(), t.ohms, t.spacing))
            .collect::<Vec<_>>(),
        [
            ("pz", vec!["VIN".to_string(), "3V3".to_string()], 90.0, Some(0.15)),
            ("z", vec!["3V3".to_string()], 50.0, None),
        ]
    );
    let libraries = Libraries::sample();
    let board = netsmith_placer::board_from_design_placed(
        &built.design,
        &libraries.mapping,
        &libraries.footprints,
        &place_options(&built.design, None),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let notes = board.process_extras().join("\n");
    assert!(
        notes.contains("track impedance target: nets `VIN` and `3V3` (pz), 90 ohm differential, the tracks 0.15 mm apart (edge to edge)"),
        "{notes}"
    );
    assert!(
        notes.contains("track impedance target: net `3V3` (z), 50 ohm single-ended"),
        "{notes}"
    );
    // Each target has a line for each layer a track may be on, with the width that gives it
    // and what the router draws; the net's own width is the router's.
    let targets: Vec<&str> = notes
        .lines()
        .skip_while(|l| !l.starts_with("track impedance target: net `3V3`"))
        .skip(1)
        .take(4)
        .collect();
    assert_eq!(targets.len(), 4, "{notes}");
    for (line, layer) in targets.iter().zip(["F.Cu", "In2.Cu", "In3.Cu", "B.Cu"]) {
        assert!(
            line.trim_start().starts_with(&format!("{layer}: "))
                && line.contains("mm wide tracks give 50 ohm")
                && line.contains(&format!(
                    "the {} mm tracks the router draws give ",
                    board.track_width("3V3")
                )),
            "{line}"
        );
    }
}

#[test]
fn a_design_with_no_target_has_no_target_lines() {
    let loader = Loader::std();
    let built = build_with(
        "six.nsm",
        SIX_LAYER,
        &loader,
        &Catalogs::jlcpcb(),
        &Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .remove(0);
    assert!(built.design.features.impedance_targets.is_empty());
}
