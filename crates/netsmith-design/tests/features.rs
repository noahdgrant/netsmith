//! Board features in the design: rail track widths, the outline, cutouts, pours and
//! keepouts (ADR 0020, ADR 0029).

use netsmith_check::{Loader, check};
use netsmith_core::Design;
use netsmith_design::design_board;
use netsmith_solve::{Catalogs, solve};

/// `check` of a source that states its board's physical intent inside the board, as the
/// language took it before ADR 0048: split into the circuit source and the physical file
/// that the language takes now (`netsmith_testkit::split_physical`).
fn check_split(src: &str) -> netsmith_check::Checked {
    let (circuit, physical) = netsmith_testkit::split_physical(src);
    let mut loader = Loader::std();
    if let Some(physical) = physical {
        loader.attach_physical("t.nsm", "t.physical.nsm", &physical);
    }
    check("t.nsm", &circuit, &loader)
}

fn designed(src: &str) -> Result<Design, Vec<String>> {
    let c = check_split(src);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s).map(|l| l.design)
}

/// A four-layer board with one resistor between `VIN` and `GND`, and `extra` members.
fn board(extra: &str) -> String {
    format!(
        "import std::mfg::jlcpcb::FourLayer\nimport std::parts::passive::Resistor\nimport std::pcb::{{Cutout, Keepout, Outline}}\nimport std::sources::Rail\nboard B {{\n    .process: FourLayer {{\n        .inner1.net &= \"GND\"\n        .inner2.net &= \"VIN\"\n        .stackup.thickness &= 1.6mm\n{process}    }}\n    .r: Resistor {{\n        .a.net &= \"VIN\"\n        .b.net &= \"GND\"\n    }}\n{extra}}}\n",
        process = ""
    )
}

fn board_with_process(process: &str, extra: &str) -> String {
    board(extra).replace(
        "        .stackup.thickness &= 1.6mm\n",
        &format!("        .stackup.thickness &= 1.6mm\n{process}"),
    )
}

fn errors_of(src: &str) -> String {
    designed(src).unwrap_err().join("\n")
}

#[test]
fn a_board_with_none_of_it_has_no_features() {
    let d = designed(&board("")).unwrap();
    assert_eq!(d.features, netsmith_core::Features::default());
}

#[test]
fn a_part_knows_its_path() {
    let d = designed(&board("")).unwrap();
    assert_eq!(d.parts[0].path, "r");
}

#[test]
fn a_rail_gives_its_nets_the_bottom_of_its_min_track() {
    let d = designed(&board(
        "    .input: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n        .min_track &= 0.8mm..\n    }\n",
    ))
    .unwrap();
    assert_eq!(
        d.features.net_widths,
        [("GND".to_string(), 0.8), ("VIN".to_string(), 0.8)]
    );
}

#[test]
fn a_net_takes_the_widest_of_its_rails() {
    let d = designed(&board(
        "    .input: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n        .min_track &= 0.8mm..\n    }\n    .other: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n        .min_track &= 0.5mm..2mm\n    }\n",
    ))
    .unwrap();
    assert_eq!(
        d.features.net_widths,
        [("GND".to_string(), 0.8), ("VIN".to_string(), 0.8)]
    );
}

#[test]
fn a_rail_gives_its_nets_the_bottom_of_its_min_clearance_and_a_net_takes_the_largest() {
    let d = designed(&board(
        "    .input: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n        .min_clearance &= 0.5mm..\n    }\n    .other: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n        .min_clearance &= 0.3mm..2mm\n    }\n",
    ))
    .unwrap();
    assert_eq!(
        d.features.net_clearances,
        [("GND".to_string(), 0.5), ("VIN".to_string(), 0.5)]
    );
    // A clearance is not a width.
    assert!(d.features.net_widths.is_empty());
}

#[test]
fn a_rail_with_no_min_clearance_gives_no_clearance() {
    let d = designed(&board(
        "    .input: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }\n",
    ))
    .unwrap();
    assert!(d.features.net_clearances.is_empty());
}

#[test]
fn the_nets_a_rail_has_a_pin_on_are_the_boards_rail_nets() {
    let d = designed(&board(
        "    .input: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }\n    .other: Rail {\n        .pos.net &= \"V3\"\n        .neg.net &= \"GND\"\n    }\n",
    ))
    .unwrap();
    // Sorted by name, a net once, whatever its rails.
    assert_eq!(d.features.rail_nets, ["GND", "V3", "VIN"]);
    // A board with no rail has none.
    let d = designed(&board("")).unwrap();
    assert!(d.features.rail_nets.is_empty());
}

#[test]
fn a_rail_with_no_min_track_gives_no_width() {
    let d = designed(&board(
        "    .input: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }\n",
    ))
    .unwrap();
    assert!(d.features.net_widths.is_empty());
}

#[test]
fn an_outline_has_its_size_as_a_range_and_a_corner_radius() {
    let d = designed(&board(
        "    .outline: Outline {\n        .width &= 60mm\n        .height &= 0mm..40mm\n        .corner_radius &= 2mm\n    }\n",
    ))
    .unwrap();
    let o = d.features.outline.unwrap();
    assert_eq!(o.width, (60.0, 60.0));
    assert_eq!(o.height, (0.0, 40.0));
    assert_eq!(o.corner_radius, 2.0);
}

#[test]
fn an_outline_not_named_outline_is_refused() {
    let text = errors_of(&board(
        "    .edge: Outline {\n        .width &= 60mm\n    }\n",
    ));
    assert!(
        text.contains("`edge` is an `Outline`, and the board's outline is its member `.outline`"),
        "{text}"
    );
}

#[test]
fn a_corner_radius_that_cannot_fit_the_outline_is_refused_with_the_numbers() {
    let text = errors_of(&board(
        "    .outline: Outline {\n        .width &= 3mm\n        .height &= 40mm\n        .corner_radius &= 2mm\n    }\n",
    ));
    assert!(
        text.contains(
            "`outline.corner_radius` is 2 mm, more than half of the shape's shorter side (3 mm)"
        ),
        "{text}"
    );
}

#[test]
fn a_cutout_has_a_size_a_radius_and_a_place_or_none() {
    let d = designed(&board(
        "    .slot: Cutout {\n        .x &= 10mm\n        .y &= 12.5mm\n        .width &= 8mm\n        .height &= 4mm\n        .corner_radius &= 1mm\n    }\n    .notch: Cutout {\n        .width &= 2mm\n        .height &= 2mm\n    }\n",
    ))
    .unwrap();
    let cut = &d.features.cutouts;
    assert_eq!(cut.len(), 2);
    assert_eq!(cut[0].name, "notch");
    assert_eq!(cut[0].at, None);
    assert_eq!(cut[1].name, "slot");
    assert_eq!(cut[1].at, Some((10.0, 12.5)));
    assert_eq!(cut[1].size, (8.0, 4.0));
    assert_eq!(cut[1].corner_radius, 1.0);
}

#[test]
fn a_cutout_without_a_size_or_with_half_a_place_says_so() {
    let text = errors_of(&board(
        "    .slot: Cutout {\n        .x &= 10mm\n        .width &= 8mm\n    }\n",
    ));
    assert!(
        text.contains("`slot` is a `Cutout` and has no height: say how big it is"),
        "{text}"
    );
    let text = errors_of(&board(
        "    .slot: Cutout {\n        .x &= 10mm\n        .width &= 8mm\n        .height &= 4mm\n    }\n",
    ));
    assert!(
        text.contains("`slot` states only one of `.x` and `.y`"),
        "{text}"
    );
}

#[test]
fn a_position_that_is_a_range_is_refused() {
    let text = errors_of(&board(
        "    .slot: Cutout {\n        .x &= 10mm..12mm\n        .y &= 1mm\n        .width &= 8mm\n        .height &= 4mm\n    }\n",
    ));
    assert!(
        text.contains(
            "`slot.x` is 10mm..12mm, a range, and a position, a size or a height is one value"
        ),
        "{text}"
    );
}

#[test]
fn outer_pours_are_the_layers_whose_pour_has_a_net() {
    let d = designed(&board_with_process(
        "        .top.net &= \"GND\"\n        .top.stitch_pitch &= 0mm..8mm\n",
        "",
    ))
    .unwrap();
    assert_eq!(d.features.pours.len(), 1);
    let pour = &d.features.pours[0];
    assert_eq!(
        (pour.layer.as_str(), pour.net.as_str(), pour.stitch_pitch),
        ("top", "GND", Some(8.0))
    );
}

#[test]
fn a_pours_thermal_relief_is_the_bottom_of_its_ranges() {
    let d = designed(&board_with_process(
        "        .top.net &= \"GND\"\n        .top.thermal_gap &= 0.3mm..0.6mm\n        .top.spoke_width &= 0.25mm..\n        .bottom.net &= \"GND\"\n        .bottom.thermal_gap &= ..0.6mm\n",
        "",
    ))
    .unwrap();
    let relief = |layer: &str| {
        let p = d.features.pours.iter().find(|p| p.layer == layer).unwrap();
        (p.thermal_gap, p.spoke_width)
    };
    assert_eq!(relief("top"), (Some(0.3), Some(0.25)));
    // A range with no bottom states no least, and KiCad's default stays.
    assert_eq!(relief("bottom"), (None, None));
}

#[test]
fn a_thermal_relief_that_is_not_positive_is_refused_with_the_field() {
    let text = errors_of(&board_with_process(
        "        .top.net &= \"GND\"\n        .top.thermal_gap &= 0mm..1mm\n        .top.spoke_width &= 0mm..\n",
        "",
    ));
    assert!(
        text.contains("`process.top.thermal_gap` has a bottom of 0 mm, and the gap of a thermal relief must be positive"),
        "{text}"
    );
    assert!(
        text.contains("`process.top.spoke_width` has a bottom of 0 mm, and the spoke width of a thermal relief must be positive"),
        "{text}"
    );
}

#[test]
fn a_pours_clearance_min_width_and_pads_are_read_as_stated() {
    let src = board_with_process(
        "        .top.net &= \"GND\"\n        .top.clearance &= 0.4mm..1mm\n        .top.min_width &= 0.3mm..\n        .top.pads &= PadConnection::ThruHoleOnly\n        .bottom.net &= \"GND\"\n        .bottom.clearance &= ..1mm\n",
        "",
    )
    .replacen("import ", "import std::mfg::stackup::PadConnection\nimport ", 1);
    let d = designed(&src).unwrap();
    let pour = |layer: &str| {
        let p = d.features.pours.iter().find(|p| p.layer == layer).unwrap();
        (p.clearance, p.min_width, p.pads)
    };
    assert_eq!(
        pour("top"),
        (
            Some(0.4),
            Some(0.3),
            Some(netsmith_core::PadConnection::ThruHoleOnly)
        )
    );
    // Unset, or a range with no bottom, states nothing: the defaults stay.
    assert_eq!(pour("bottom"), (None, None, None));
}

#[test]
fn a_pours_clearance_or_min_width_under_the_process_is_refused_with_the_field() {
    // JLCPCB's four-layer process keeps 0.1 mm between copper and makes a 0.09 mm track.
    let text = errors_of(&board_with_process(
        "        .top.net &= \"GND\"\n        .top.clearance &= 0.05mm..\n        .top.min_width &= 0.05mm..1mm\n",
        "",
    ));
    assert!(
        text.contains("`process.top.clearance` has a bottom of 0.05 mm, under the process's least space between copper (`.min_clearance`) of 0.1 mm"),
        "{text}"
    );
    assert!(
        text.contains("`process.top.min_width` has a bottom of 0.05 mm, under the process's narrowest track (`.min_track`) of 0.09 mm"),
        "{text}"
    );
    // The least the process makes is allowed.
    assert!(
        designed(&board_with_process(
            "        .top.net &= \"GND\"\n        .top.clearance &= 0.1mm..\n        .top.min_width &= 0.09mm..\n",
            "",
        ))
        .is_ok()
    );
}

#[test]
fn a_pour_on_a_net_the_board_lacks_is_refused_with_the_nets() {
    let text = errors_of(&board_with_process("        .bottom.net &= \"5V\"\n", ""));
    assert!(
        text.contains("`process.bottom` is a pour on the net `5V`, but the board has no net of that name (its nets: `GND`, `VIN`)"),
        "{text}"
    );
}

#[test]
fn a_keepout_forbids_everything_everywhere_unless_told_otherwise() {
    let d = designed(&board(
        "    .antenna: Keepout {\n        .x &= 40mm\n        .y &= 0mm\n        .width &= 20mm\n        .height &= 12mm\n    }\n    .rf: Keepout {\n        .width &= 5mm\n        .height &= 5mm\n        .layers &= \"top, inner1\"\n        .vias &= \"allow\"\n        .parts &= \"allow\"\n    }\n",
    ))
    .unwrap();
    let k = &d.features.keepouts;
    assert_eq!(k.len(), 2);
    assert_eq!(k[0].name, "antenna");
    assert_eq!(k[0].at, Some((40.0, 0.0)));
    assert_eq!(k[0].size, (20.0, 12.0));
    assert_eq!(k[0].layers, ["top", "inner1", "inner2", "bottom"]);
    assert!(k[0].tracks && k[0].vias && k[0].copper && k[0].parts);
    assert_eq!(k[1].at, None);
    assert_eq!(k[1].layers, ["top", "inner1"]);
    assert!(k[1].tracks && k[1].copper);
    assert!(!k[1].vias && !k[1].parts);
}

#[test]
fn a_keepout_with_a_layer_the_board_lacks_names_the_layers_it_has() {
    let text = errors_of(&board(
        "    .k: Keepout {\n        .width &= 5mm\n        .height &= 5mm\n        .layers &= \"inner3\"\n    }\n",
    ));
    assert!(
        text.contains("`k.layers` names \"inner3\", and this 4-layer board has \"top\", \"inner1\", \"inner2\", \"bottom\""),
        "{text}"
    );
}

#[test]
fn a_keepout_with_a_word_that_is_not_forbid_or_allow_is_refused() {
    let text = errors_of(&board(
        "    .k: Keepout {\n        .width &= 5mm\n        .height &= 5mm\n        .vias &= \"no\"\n    }\n",
    ));
    assert!(
        text.contains("`k.vias` is \"no\", which is not \"forbid\" or \"allow\""),
        "{text}"
    );
}

#[test]
fn a_keepout_that_forbids_parts_on_faces_that_are_all_inner_layers_is_refused() {
    let text = errors_of(&board(
        "    .k: Keepout {\n        .width &= 5mm\n        .height &= 5mm\n        .layers &= \"inner1\"\n        .parts &= \"forbid\"\n    }\n",
    ));
    assert!(
        text.contains("`k.parts` is \"forbid\" and its `.layers` name neither \"top\" nor \"bottom\", the faces a part stands on, so it keeps no part out"),
        "{text}"
    );
    // A face is enough, and an unstated `.parts` is no statement to refuse.
    for fine in [
        "        .layers &= \"bottom\"\n        .parts &= \"forbid\"\n",
        "        .layers &= \"inner1\"\n",
    ] {
        let src = format!(
            "    .k: Keepout {{\n        .width &= 5mm\n        .height &= 5mm\n{fine}    }}\n"
        );
        designed(&board(&src)).unwrap_or_else(|e| panic!("{e:?}"));
    }
}

#[test]
fn a_keepout_that_allows_everything_is_refused() {
    let text = errors_of(&board(
        "    .k: Keepout {\n        .width &= 5mm\n        .height &= 5mm\n        .tracks &= \"allow\"\n        .vias &= \"allow\"\n        .copper &= \"allow\"\n        .parts &= \"allow\"\n    }\n",
    ));
    assert!(
        text.contains("`k` allows tracks, vias, copper and parts, and forbids nothing"),
        "{text}"
    );
}

#[test]
fn a_part_carries_a_keepout_in_its_own_frame() {
    let src = "import std::mfg::jlcpcb::TwoLayer\nimport std::pcb::Keepout\npart Radio {\n    .package: Package\n    .antenna: Keepout {\n        .x &= -1mm\n        .y &= -9mm\n        .width &= 12mm\n        .height &= 8mm\n        .tracks &= \"forbid\"\n        .parts &= \"allow\"\n    }\n}\nboard B {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .radio: Radio\n}\n";
    let d = designed(src).unwrap();
    let k = &d.features.keepouts;
    assert_eq!(k.len(), 1);
    assert_eq!(k[0].name, "radio.antenna");
    assert_eq!(k[0].part.as_deref(), Some("radio"));
    assert_eq!(k[0].at, Some((-1.0, -9.0)));
    assert!(!k[0].parts);
}

#[test]
fn a_keepout_of_a_part_without_a_place_is_refused() {
    let src = "import std::mfg::jlcpcb::TwoLayer\nimport std::pcb::Keepout\npart Radio {\n    .package: Package\n    .antenna: Keepout {\n        .width &= 12mm\n        .height &= 8mm\n    }\n}\nboard B {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .radio: Radio\n}\n";
    let text = errors_of(src);
    assert!(
        text.contains("`radio.antenna` is a keepout of a part and has no `.x` and `.y`"),
        "{text}"
    );
}

#[test]
fn a_keepout_in_a_module_is_refused() {
    let src = "import std::mfg::jlcpcb::TwoLayer\nimport std::pcb::Keepout\nmodule M {\n    .k: Keepout {\n        .width &= 5mm\n        .height &= 5mm\n    }\n}\nboard B {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .m: M\n}\n";
    let text = errors_of(src);
    assert!(
        text.contains("`m.k` is a `Keepout`, and it belongs to a board or to a part"),
        "{text}"
    );
}

#[test]
fn a_module_carries_a_width_to_its_rail_through_a_connection() {
    let src = "import std::mfg::jlcpcb::TwoLayer\nimport std::parts::passive::Resistor\nimport std::sources::Rail\nmodule Stage {\n    .out: Rail\n    .r: Resistor\n    .out.pos ~ .r.a\n    .out.neg ~ .r.b\n}\nboard B {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .stage: Stage\n    .output: Rail {\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n        .min_track &= 0.5mm..\n    }\n    .stage.out ~ .output\n}\n";
    let d = designed(src).unwrap();
    assert_eq!(
        d.features.net_widths,
        [("3V3".to_string(), 0.5), ("GND".to_string(), 0.5)]
    );
}

fn label_board(fields: &str) -> String {
    format!(
        "import std::mfg::jlcpcb::TwoLayer\nimport std::parts::mechanical::Label\nboard B {{\n    .process: TwoLayer {{\n        .stackup.thickness &= 1.6mm\n    }}\n    .rev: Label {{\n{fields}    }}\n}}\n"
    )
}

#[test]
fn a_label_is_a_part_whose_value_is_its_text() {
    let d = designed(&label_board(
        "        .text &= \"REV A\"\n        .text_height &= 1.5mm\n",
    ))
    .unwrap();
    assert_eq!(d.parts.len(), 1);
    assert_eq!(
        (
            d.parts[0].refdes.as_str(),
            d.parts[0].kind.as_str(),
            d.parts[0].value.as_str()
        ),
        ("LBL1", "Label", "REV A")
    );
    assert_eq!(d.features.labels.len(), 1);
    assert_eq!(d.features.labels[0].text_height, Some(1.5));
}

#[test]
fn a_label_with_no_text_or_text_too_small_is_refused_with_the_numbers() {
    let text = errors_of(&label_board(""));
    assert!(
        text.contains("`rev` is a `Label` and has no text: say what it prints"),
        "{text}"
    );
    let text = errors_of(&label_board(
        "        .text &= \"x\"\n        .text_height &= 0.8mm\n",
    ));
    assert!(
        text.contains("`rev.text_height` is 0.8 mm, under the process's least text height of 1 mm"),
        "{text}"
    );
}

#[test]
fn a_part_carries_its_step_model_with_its_offset_and_rotation() {
    let d = designed(&board(
        "    .r.model.file &= \"models/r.step\"\n    .r.model.z &= -0.2mm\n    .r.model.rx &= -90\n    .r.model.rz &= 90\n",
    ))
    .unwrap();
    assert_eq!(
        d.features.models,
        [netsmith_core::StepModel {
            path: "r".into(),
            file: "models/r.step".into(),
            offset: [0.0, 0.0, -0.2],
            rotation: [-90.0, 0.0, 90.0],
            source: String::new(),
            stated_in: Vec::new(),
            resolved: None,
            footprint: None,
            placement_only: None,
        }]
    );
    // The model is drawn, never measured: it gives the part no height.
    assert!(d.features.heights.is_empty(), "{:?}", d.features.heights);
}

#[test]
fn a_placement_of_a_model_with_no_file_is_kept_for_the_model_table() {
    // The design stage holds the placement; `locate` gives it the table's model, or says there
    // is nothing to place (issue #1005).
    let d = designed(&board("    .r.model.z &= 0.1mm\n    .r.model.rz &= 90\n")).unwrap();
    assert_eq!(d.features.models.len(), 1, "{:?}", d.features.models);
    let m = &d.features.models[0];
    assert_eq!(m.path, "r");
    assert_eq!(m.file, "");
    assert_eq!(m.offset, [0.0, 0.0, 0.1]);
    assert_eq!(m.rotation, [0.0, 0.0, 90.0]);
    assert_eq!(
        m.placement_only,
        Some([false, false, true, false, false, true])
    );
}

#[test]
fn a_rotation_of_a_model_that_is_a_range_is_refused() {
    let e = errors_of(&board(
        "    .r.model.file &= \"models/r.step\"\n    .r.model.rz &= 80..100\n",
    ));
    assert!(
        e.contains("`r.model.rz` is") && e.contains("a rotation is one value in degrees"),
        "{e}"
    );
}

#[test]
fn an_empty_model_file_says_the_part_has_none() {
    // The part is listed with no file, so the project's model table gives it none (#763).
    let d = designed(&board("    .r.model.file &= \"\"\n")).unwrap();
    assert_eq!(d.features.models.len(), 1, "{:?}", d.features.models);
    assert_eq!(d.features.models[0].path, "r");
    assert_eq!(d.features.models[0].file, "");
}

#[test]
fn a_placement_of_an_empty_model_file_is_refused() {
    assert_eq!(
        errors_of(&board("    .r.model.file &= \"\"\n    .r.model.rz &= 90\n")),
        "`r.model.file` is empty, which says `r` has no model, and `r.model.rz` is stated, so there is nothing to place: name the STEP file, or remove the placement"
    );
}

/// The design of the four-layer board with a physical file of `case` in its `case { }`
/// section, and `circuit` added to the board.
fn cased(circuit: &str, case: &str) -> Result<Design, Vec<String>> {
    let physical =
        format!("import std::pcb::CaseModel\n\nphysical for B {{\n    case {{\n{case}    }}\n}}\n");
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", &physical);
    let c = check("t.nsm", &board(circuit), &loader);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s).map(|l| l.design)
}

#[test]
fn a_case_model_is_carried_with_its_offset_and_rotation() {
    let d = cased(
        "",
        "        .shell: CaseModel {\n            .file &= \"case/box.step\"\n            .x &= -2mm\n            .z &= -1.6mm\n            .rx &= 180\n            .clearance &= 0.3mm\n        }\n        .lid: CaseModel {\n            .file &= \"case/lid.step\"\n            .rz &= 90\n        }\n",
    )
    .unwrap();
    assert_eq!(
        d.features.case_models,
        [
            netsmith_core::CaseModel {
                name: "lid".into(),
                file: "case/lid.step".into(),
                offset: [0.0, 0.0, 0.0],
                rotation: [0.0, 0.0, 90.0],
                // std's figures, as `CaseModel` offers them with `?=`.
                figures: netsmith_core::CaseFigures {
                    clearance: 0.5,
                    reach: 1.5,
                    boss_gap: 0.5,
                },
                checked: true,
                color: None,
                source: String::new(),
                stated_in: Vec::new(),
                resolved: None,
            },
            netsmith_core::CaseModel {
                name: "shell".into(),
                file: "case/box.step".into(),
                offset: [-2.0, 0.0, -1.6],
                rotation: [180.0, 0.0, 0.0],
                figures: netsmith_core::CaseFigures {
                    clearance: 0.3,
                    ..netsmith_core::CaseFigures::default()
                },
                checked: true,
                color: None,
                source: String::new(),
                stated_in: Vec::new(),
                resolved: None,
            },
        ]
    );
    // A case model is a drawing and a check: it gives the board no outline, hole or limit.
    assert!(d.features.outline.is_none());
    assert!(d.features.cutouts.is_empty() && d.features.keepouts.is_empty());
}

#[test]
fn a_case_model_carries_the_colour_the_board_states() {
    // Issue #756: the variant's name, for the model that states it, and none for the rest.
    let physical = "import std::pcb::CaseModel\nimport std::parts::passive::Color\n\nphysical for B {\n    case {\n        .lid: CaseModel {\n            .file &= \"case/lid.step\"\n            .color &= Color::Black\n        }\n        .shell: CaseModel {\n            .file &= \"case/box.step\"\n        }\n    }\n}\n";
    let loader = Loader::std().with_physical("t.nsm", "t.physical.nsm", physical);
    let c = check("t.nsm", &board(""), &loader);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    let d = design_board(&c.boards[0], &s).map(|l| l.design).unwrap();
    let colours: Vec<(&str, Option<&str>)> = d
        .features
        .case_models
        .iter()
        .map(|c| (c.name.as_str(), c.color.as_deref()))
        .collect();
    assert_eq!(colours, [("lid", Some("Black")), ("shell", None)]);
}

#[test]
fn a_case_models_figure_is_one_length_and_not_negative() {
    let errors = cased(
        "",
        "        .shell: CaseModel {\n            .file &= \"case/box.step\"\n            .clearance &= -1mm\n            .reach &= 1mm..2mm\n        }\n",
    )
    .unwrap_err()
    .join("\n");
    assert!(
        errors.contains("`shell.clearance` is -1mm, and a gap is not negative"),
        "{errors}"
    );
    assert!(errors.contains("`shell.reach` is"), "{errors}");
    assert!(errors.contains("a range"), "{errors}");
}

#[test]
fn a_case_model_that_is_only_a_drawing_is_not_checked() {
    let d = cased(
        "",
        "        .shell: CaseModel {\n            .file &= \"case/box.step\"\n        }\n        .plug: CaseModel {\n            .file &= \"case/plug.step\"\n            .check &= \"none\"\n        }\n",
    )
    .unwrap();
    let checked: Vec<(&str, bool)> = d
        .features
        .case_models
        .iter()
        .map(|c| (c.name.as_str(), c.checked))
        .collect();
    assert_eq!(checked, [("plug", false), ("shell", true)]);
}

#[test]
fn a_case_models_check_is_fit_or_none() {
    let e = cased(
        "",
        "        .shell: CaseModel {\n            .file &= \"case/box.step\"\n            .check &= \"off\"\n        }\n",
    )
    .unwrap_err()
    .join("\n");
    assert_eq!(
        e,
        "`shell.check` is \"off\", which is not \"fit\" or \"none\": \"fit\" measures the board against the model, \"none\" only draws it"
    );
}

#[test]
fn a_case_model_with_no_file_is_refused() {
    assert_eq!(
        cased(
            "",
            "        .shell: CaseModel {\n            .rx &= 180\n        }\n"
        )
        .unwrap_err()
        .join("\n"),
        "`shell` is a `CaseModel` and names no file, so there is no case to place: name it with `.file &= \"case/box.step\"`, relative to the physical file"
    );
    assert_eq!(
        cased(
            "",
            "        .shell: CaseModel {\n            .file &= \" \"\n        }\n"
        )
        .unwrap_err()
        .join("\n"),
        "`shell.file` is empty: name the case's STEP file, relative to the physical file that states it"
    );
}

#[test]
fn a_rotation_or_an_offset_of_a_case_model_that_is_a_range_is_refused() {
    let e = cased(
        "",
        "        .shell: CaseModel {\n            .file &= \"case/box.step\"\n            .rx &= 170..190\n            .z &= -2mm..-1mm\n        }\n",
    )
    .unwrap_err()
    .join("\n");
    assert!(
        e.contains("`shell.rx` is") && e.contains("a rotation is one value in degrees"),
        "{e}"
    );
    assert!(e.contains("shell.z"), "{e}");
}

#[test]
fn a_case_model_of_a_module_is_refused() {
    let src = "import std::mfg::jlcpcb::TwoLayer\nimport std::pcb::CaseModel\nmodule M {\n    .shell: CaseModel {\n        .file &= \"case/box.step\"\n    }\n}\nboard B {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .m: M\n}\n";
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    let e = design_board(&c.boards[0], &s).unwrap_err().join("\n");
    assert_eq!(
        e,
        "`m.shell` is a `CaseModel`, and a case is the board's: declare it in the `case { }` section of the board's physical file"
    );
}

fn hole_board(fields: &str) -> String {
    format!(
        "import std::mfg::jlcpcb::TwoLayer\nimport std::parts::mechanical::MountingHole\nboard B {{\n    .process: TwoLayer {{\n        .stackup.thickness &= 1.6mm\n    }}\n    .hole: MountingHole {{\n{fields}    }}\n}}\n"
    )
}

#[test]
fn a_mounting_hole_with_a_head_carries_a_keepout_round_its_centre_on_every_layer() {
    let d = designed(&hole_board("        .head_diameter &= 6mm\n")).unwrap();
    let k = &d.features.keepouts;
    assert_eq!(k.len(), 1, "{k:?}");
    assert_eq!(k[0].name, "screw head of hole");
    assert_eq!(k[0].part.as_deref(), Some("hole"));
    assert_eq!(k[0].at, Some((-3.0, -3.0)));
    assert_eq!(k[0].size, (6.0, 6.0));
    assert_eq!(k[0].layers, ["top", "bottom"]);
    assert!(k[0].tracks && k[0].vias && k[0].copper && k[0].parts);
    assert_eq!(k[0].max_height, None);
}

#[test]
fn a_mounting_hole_with_no_head_carries_nothing() {
    let d = designed(&hole_board("")).unwrap();
    assert!(d.features.keepouts.is_empty(), "{:?}", d.features.keepouts);
}

#[test]
fn a_head_that_is_a_range_or_not_above_nothing_is_refused() {
    let text = errors_of(&hole_board("        .head_diameter &= 5mm..6mm\n"));
    assert!(
        text.contains("`hole.head_diameter` is") && text.contains("a range"),
        "{text}"
    );
    let text = errors_of(&hole_board("        .head_diameter &= 0mm\n"));
    assert!(
        text.contains("`hole.head_diameter` is 0 mm, and a screw head is wider than nothing"),
        "{text}"
    );
}

// ---------------------------------------------------------------------------------------
// The impedance a design states for tracks (issue #1007).

#[test]
fn a_track_impedance_gives_the_net_its_pin_is_on_a_target() {
    let d = designed(&board(
        "    .z: TrackImpedance {\n        .line.net &= \"VIN\"\n        .impedance &= 50ohm\n    }\n",
    )
    .replace("import std::sources::Rail", "import std::sources::Rail\nimport std::sources::TrackImpedance"))
    .unwrap();
    assert_eq!(
        d.features.impedance_targets,
        [netsmith_core::ImpedanceTarget {
            path: "z".to_string(),
            nets: vec!["VIN".to_string()],
            ohms: 50.0,
            spacing: None,
            skew: None,
        }]
    );
}

#[test]
fn a_pair_impedance_gives_two_nets_a_differential_target_and_a_spacing() {
    let d = designed(&board(
        "    .pz: PairImpedance {\n        .p.net &= \"VIN\"\n        .n.net &= \"GND\"\n        .impedance &= 90ohm\n        .spacing &= 0.15mm\n    }\n",
    )
    .replace("import std::sources::Rail", "import std::sources::Rail\nimport std::sources::PairImpedance"))
    .unwrap();
    assert_eq!(
        d.features.impedance_targets,
        [netsmith_core::ImpedanceTarget {
            path: "pz".to_string(),
            nets: vec!["VIN".to_string(), "GND".to_string()],
            ohms: 90.0,
            spacing: Some(0.15),
            skew: Some(0.1),
        }]
    );
}

#[test]
fn a_target_that_is_not_one_value_above_zero_is_an_error_that_says_what_to_write() {
    let with = |extra: &str| {
        board(extra).replace(
            "import std::sources::Rail",
            "import std::sources::Rail\nimport std::sources::{PairImpedance, TrackImpedance}",
        )
    };
    let missing = errors_of(&with(
        "    .z: TrackImpedance {\n        .line.net &= \"VIN\"\n    }\n",
    ));
    assert!(
        missing.contains("`z.impedance` is not stated: say it with `&= 50ohm`"),
        "{missing}"
    );
    let range = errors_of(&with(
        "    .z: TrackImpedance {\n        .line.net &= \"VIN\"\n        .impedance &= 45ohm..55ohm\n    }\n",
    ));
    assert!(range.contains("a target impedance is one value"), "{range}");
    let no_spacing = errors_of(&with(
        "    .pz: PairImpedance {\n        .p.net &= \"VIN\"\n        .n.net &= \"GND\"\n        .impedance &= 90ohm\n    }\n",
    ));
    assert!(
        no_spacing.contains("`pz.spacing` is not stated: say the space between the two tracks"),
        "{no_spacing}"
    );
    let one_net = errors_of(&with(
        "    .pz: PairImpedance {\n        .p.net &= \"VIN\"\n        .n.net &= \"VIN\"\n        .impedance &= 90ohm\n        .spacing &= 0.15mm\n    }\n",
    ));
    assert!(one_net.contains("a pair is two nets"), "{one_net}");
}

#[test]
fn a_diff_pair_and_a_can_bus_state_their_two_nets_a_pair_once() {
    // Issue #1193: a `DiffPair` and the lines of a `Can` are a pair whatever the nets' names; the
    // two state the same pair, which is listed once, by the first path.
    let d = designed(&board(
        "    .dp: DiffPair {\n        .p.net &= \"VIN\"\n        .n.net &= \"GND\"\n    }\n    .bus: Can {\n        .canh.net &= \"VIN\"\n        .canl.net &= \"GND\"\n    }\n",
    )
    .replace(
        "import std::sources::Rail",
        "import std::bus::Can\nimport std::sources::{DiffPair, Rail}",
    ))
    .unwrap();
    assert_eq!(
        d.features.pairs,
        [netsmith_core::NetPair {
            path: "bus".to_string(),
            p: "VIN".to_string(),
            n: "GND".to_string(),
            // The least of the two limits: the `DiffPair`'s 0.1 mm under the bus's 2.5 mm.
            skew: Some(0.1),
        }]
    );
    assert!(d.features.impedance_targets.is_empty());
}

#[test]
fn a_pair_carries_its_skew_limit_the_top_of_its_skew() {
    // Issue #1234: a `DiffPair` offers `0mm..0.1mm`, a board narrows it, a `Can` offers its
    // standard's 2.5 mm and the lines of a `Usb2` are a pair too.
    let with = |extra: &str| {
        designed(&board(extra).replace(
            "import std::sources::Rail",
            "import std::bus::{Can, Usb2}\nimport std::sources::{DiffPair, Rail}",
        ))
        .unwrap()
        .features
        .pairs
    };
    let skews = |pairs: Vec<netsmith_core::NetPair>| {
        pairs
            .into_iter()
            .map(|p| (p.path, p.skew))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        skews(with(
            "    .dp: DiffPair {\n        .p.net &= \"VIN\"\n        .n.net &= \"GND\"\n    }\n"
        )),
        [("dp".to_string(), Some(0.1))]
    );
    assert_eq!(
        skews(with(
            "    .dp: DiffPair {\n        .p.net &= \"VIN\"\n        .n.net &= \"GND\"\n        .skew &= 0mm..0.05mm\n    }\n"
        )),
        [("dp".to_string(), Some(0.05))]
    );
    assert_eq!(
        skews(with(
            "    .bus: Can {\n        .canh.net &= \"VIN\"\n        .canl.net &= \"GND\"\n    }\n"
        )),
        [("bus".to_string(), Some(2.5))]
    );
    assert_eq!(
        skews(with(
            "    .usb: Usb2 {\n        .dp.net &= \"VIN\"\n        .dm.net &= \"GND\"\n    }\n"
        )),
        [("usb".to_string(), Some(0.1))]
    );
}

#[test]
fn a_skew_with_a_top_of_nothing_is_an_error_that_says_what_to_write() {
    let with = |skew: &str| {
        errors_of(&board(&format!(
            "    .dp: DiffPair {{\n        .p.net &= \"VIN\"\n        .n.net &= \"GND\"\n        .skew &= {skew}\n    }}\n"
        ))
        .replace(
            "import std::sources::Rail",
            "import std::sources::{DiffPair, Rail}",
        ))
    };
    let zero = with("0mm");
    assert!(
        zero.contains("`dp.skew` is 0") && zero.contains("give it a top above 0, `&= 0mm..0.05mm`"),
        "{zero}"
    );
}

#[test]
fn a_diff_pair_on_one_net_or_on_a_net_of_another_pair_is_an_error() {
    let with = |extra: &str| {
        board(extra).replace(
            "import std::sources::Rail",
            "import std::sources::{DiffPair, Rail}",
        )
    };
    let one_net = errors_of(&with(
        "    .dp: DiffPair {\n        .p.net &= \"VIN\"\n        .n.net &= \"VIN\"\n    }\n",
    ));
    assert!(
        one_net.contains("`dp` joins both of its pins to the net `VIN`: a pair is two nets"),
        "{one_net}"
    );
    let two = errors_of(&with(
        "    .a: DiffPair {\n        .p.net &= \"VIN\"\n        .n.net &= \"GND\"\n    }\n    .b: DiffPair {\n        .p.net &= \"VIN\"\n        .n.net &= \"OTHER\"\n    }\n",
    ));
    assert!(
        two.contains("`b` makes `VIN` and `OTHER` a diff pair, but `a` already makes `VIN` and `GND` one: a net is one track of one pair"),
        "{two}"
    );
}

#[test]
fn a_parts_pads_block_gives_the_contacts_of_its_package_and_a_package_row_wins() {
    // Issue 1321: the rows of `pads "SMD"` are kept only for a part in that package, and win over
    // the block for every package.
    let part = "part Row4 {\n    pins \"SMD\" \"THT\" {\n        .p1 passive \"P1\" \"1\"\n        .p2 passive \"P2\" \"2\"\n    }\n    .package: Package\n    pads {\n        \"2\" .contact_offset = (0mm, 0.5mm)\n    }\n    pads \"SMD\" {\n        \"1\" .contact_offset = (-1.175mm, 0mm)\n        \"2\" .contact_offset = (1.175mm, 0mm)\n    }\n}\n";
    let src = board(
        "    .smd: Row4 { .package &= \"SMD\" }\n    .tht: Row4 { .package &= \"THT\" }\n",
    )
    .replace("board B {", &format!("{part}board B {{"));
    let d = designed(&src).unwrap();
    let found: Vec<(&str, &str, (f64, f64))> = d
        .features
        .pad_contacts
        .iter()
        .map(|c| (c.path.as_str(), c.pad.as_str(), c.offset))
        .collect();
    assert_eq!(
        found,
        [
            ("smd", "1", (-1.175, 0.0)),
            ("smd", "2", (1.175, 0.0)),
            ("tht", "2", (0.0, 0.5)),
        ]
    );
}

// ---- a pin's own pour connection (issue #1276) ----------------------------------------------

/// The board of [`board`], with a part of its own whose two ground pins state `ground` and
/// `shield` inside the part, and `extra` members on the board.
fn pad_pour_board(ground: &str, shield: &str, extra: &str) -> String {
    let part = format!(
        "/// A module with an antenna pin and its grounds. Illustrative; not a real part.\npart Radio {{\n    .designator &= \"U\"\n    pins {{\n        .rf passive \"RF\" \"1\"\n        .gnd passive \"GND\" \"2\" \"4\"\n        .shield passive \"SH\" \"3\"\n    }}\n{ground}{shield}}}\n"
    );
    let text = board(&format!(
        "    .u: Radio\n    .u.gnd.net &= \"GND\"\n    .u.shield.net &= \"GND\"\n    .u.rf.net &= \"VIN\"\n{extra}"
    ));
    let text = text.replacen("board B", &format!("{part}board B"), 1);
    text.replacen("import ", "import std::mfg::stackup::PadConnection\nimport ", 1)
}

#[test]
fn a_pin_states_how_a_pour_joins_each_of_its_pads() {
    let d = designed(&pad_pour_board(
        "    .gnd.pour_connection &= PadConnection::Solid\n",
        "    .shield.spoke_width &= 0.3mm..\n",
        "",
    ))
    .unwrap();
    let pours: Vec<_> = d
        .features
        .pad_pours
        .iter()
        .map(|p| {
            (
                p.path.as_str(),
                p.pins
                    .iter()
                    .map(|r| format!("{}.{}", r.refdes, r.pin))
                    .collect::<Vec<_>>(),
                p.connection,
                p.spoke_width,
            )
        })
        .collect();
    let refdes = &d.parts.iter().find(|p| p.path == "u").unwrap().refdes;
    assert_eq!(
        pours,
        vec![
            (
                "u.gnd",
                vec![format!("{refdes}.2"), format!("{refdes}.4")],
                Some(netsmith_core::PadConnection::Solid),
                None
            ),
            ("u.shield", vec![format!("{refdes}.3")], None, Some(0.3)),
        ]
    );
}

#[test]
fn a_board_narrows_the_pour_connection_of_a_pin_of_its_part() {
    let d = designed(&pad_pour_board(
        "",
        "",
        "    .u.shield.pour_connection &= PadConnection::Thermal\n    .u.shield.spoke_width &= 0.2mm\n",
    ))
    .unwrap();
    assert_eq!(d.features.pad_pours.len(), 1);
    let p = &d.features.pad_pours[0];
    assert_eq!(
        (p.path.as_str(), p.connection, p.spoke_width),
        (
            "u.shield",
            Some(netsmith_core::PadConnection::Thermal),
            Some(0.2)
        )
    );
}

#[test]
fn a_pin_with_no_pour_connection_states_none() {
    let d = designed(&pad_pour_board("", "", "")).unwrap();
    assert!(d.features.pad_pours.is_empty());
}

#[test]
fn a_pins_pour_connection_none_or_thru_hole_only_is_refused_with_the_pin() {
    for variant in ["None", "ThruHoleOnly"] {
        let text = errors_of(&pad_pour_board(
            &format!("    .gnd.pour_connection &= PadConnection::{variant}\n"),
            "",
            "",
        ));
        assert!(
            text.contains(&format!(
                "`u.gnd.pour_connection` is `PadConnection::{variant}`, and a pin states only `Solid` or `Thermal` for its pads"
            )),
            "{text}"
        );
    }
}

#[test]
fn a_spoke_width_of_a_solid_pin_or_under_the_least_track_is_refused_with_the_numbers() {
    let text = errors_of(&pad_pour_board(
        "    .gnd.pour_connection &= PadConnection::Solid\n    .gnd.spoke_width &= 0.3mm\n",
        "    .shield.spoke_width &= 0.05mm..\n",
        "",
    ));
    assert!(
        text.contains("`u.gnd.spoke_width` is 0.3 mm, but `u.gnd.pour_connection` is `PadConnection::Solid`, which has no spokes"),
        "{text}"
    );
    assert!(
        text.contains("`u.shield.spoke_width` has a bottom of 0.05 mm, under the process's narrowest track (`.min_track`) of"),
        "{text}"
    );
}
