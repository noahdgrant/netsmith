//! Board features in the lowered design: rail track widths, the outline, cutouts, pours and
//! keepouts (ADR 0020, ADR 0029).

use netsmith_check::{Loader, check};
use netsmith_core::Design;
use netsmith_lower::lower;
use netsmith_solve::{Catalogs, solve};

/// `check` of a source that states its board's physical intent inside the board, as the
/// language took it before ADR 0048: split into the circuit source and the physical file
/// that the language takes now (`netsmith_kicad::testlib::split_physical`).
fn check_split(src: &str) -> netsmith_check::Checked {
    let (circuit, physical) = netsmith_kicad::testlib::split_physical(src);
    let mut loader = Loader::std();
    if let Some(physical) = physical {
        loader.attach_physical("t.nsm", "t.physical.nsm", &physical);
    }
    check("t.nsm", &circuit, &loader)
}

fn lowered(src: &str) -> Result<Design, Vec<String>> {
    let c = check_split(src);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    lower(&c.boards[0], &s).map(|l| l.design)
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
    lowered(src).unwrap_err().join("\n")
}

#[test]
fn a_board_with_none_of_it_has_no_features() {
    let d = lowered(&board("")).unwrap();
    assert_eq!(d.features, netsmith_core::Features::default());
}

#[test]
fn a_part_knows_its_path() {
    let d = lowered(&board("")).unwrap();
    assert_eq!(d.parts[0].path, "r");
}

#[test]
fn a_rail_gives_its_nets_the_bottom_of_its_min_track() {
    let d = lowered(&board(
        "    .input: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n        .min_track &= 0.8mm..1m\n    }\n",
    ))
    .unwrap();
    assert_eq!(
        d.features.net_widths,
        [("GND".to_string(), 0.8), ("VIN".to_string(), 0.8)]
    );
}

#[test]
fn a_net_takes_the_widest_of_its_rails() {
    let d = lowered(&board(
        "    .input: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n        .min_track &= 0.8mm..1m\n    }\n    .other: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n        .min_track &= 0.5mm..2mm\n    }\n",
    ))
    .unwrap();
    assert_eq!(
        d.features.net_widths,
        [("GND".to_string(), 0.8), ("VIN".to_string(), 0.8)]
    );
}

#[test]
fn a_rail_with_no_min_track_gives_no_width() {
    let d = lowered(&board(
        "    .input: Rail {\n        .pos.net &= \"VIN\"\n        .neg.net &= \"GND\"\n    }\n",
    ))
    .unwrap();
    assert!(d.features.net_widths.is_empty());
}

#[test]
fn an_outline_has_its_size_as_a_range_and_a_corner_radius() {
    let d = lowered(&board(
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
    let d = lowered(&board(
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
    let d = lowered(&board_with_process(
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
fn a_pour_on_a_net_the_board_lacks_is_refused_with_the_nets() {
    let text = errors_of(&board_with_process("        .bottom.net &= \"5V\"\n", ""));
    assert!(
        text.contains("`process.bottom` is a pour on the net `5V`, but the board has no net of that name (its nets: `GND`, `VIN`)"),
        "{text}"
    );
}

#[test]
fn a_keepout_forbids_everything_everywhere_unless_told_otherwise() {
    let d = lowered(&board(
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
    let d = lowered(src).unwrap();
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
    let src = "import std::mfg::jlcpcb::TwoLayer\nimport std::parts::passive::Resistor\nimport std::sources::Rail\nmodule Stage {\n    .out: Rail\n    .r: Resistor\n    .out.pos ~ .r.a\n    .out.neg ~ .r.b\n}\nboard B {\n    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n    .stage: Stage\n    .output: Rail {\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n        .min_track &= 0.5mm..1m\n    }\n    .stage.out ~ .output\n}\n";
    let d = lowered(src).unwrap();
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
    let d = lowered(&label_board(
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
