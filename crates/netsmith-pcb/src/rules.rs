//! Design rules: what the board's manufacturing process lets copper, holes and silkscreen
//! be, in the form the router and the writers use.
//!
//! The numbers are not written here. They are the `process` member of the board, from a
//! library such as `std::mfg::jlcpcb`, after the board has narrowed them, and arrive in the
//! [`Design`](netsmith_core::Design). The router reads them to keep copper where a
//! fabricator can make it, the `.kicad_pcb` writer uses the via and track sizes, and the
//! `.kicad_pro` writer hands every number to KiCad's design rule check, so a board that
//! passes it meets the process.

use netsmith_core::Process;
use netsmith_kicad::sexp::fmt_num;

/// What a fabricator can make, and the sizes the router draws with. All in mm.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rules {
    /// The least space between copper of different nets.
    pub clearance: f64,
    /// The narrowest track the fabricator makes.
    pub min_track_width: f64,
    /// The width the router draws tracks at.
    pub track_width: f64,
    /// The copper of the vias the router places, and their drill.
    pub via_diameter: f64,
    pub via_drill: f64,
    /// The smallest copper disc of a via the process makes, which the check holds every via to.
    pub min_via_diameter: f64,
    /// The smallest drilled hole.
    pub min_through_hole: f64,
    /// The least space between the edges of two holes when one is a pad's, and between two
    /// vias' holes.
    pub hole_to_hole: f64,
    pub via_hole_to_hole: f64,
    /// The least space between copper and the board edge.
    pub copper_to_edge: f64,
    /// The least copper ring left round a plated pad's hole, and round a via's hole.
    pub min_annular_ring: f64,
    pub via_ring: f64,
    /// The least space from the edge of a via's or a non-plated hole to a track of another net,
    /// and from a plated pad's hole (issue #431).
    pub hole_clearance: f64,
    pub plated_hole_clearance: f64,
    /// The least space between surface-mount pads of different nets.
    pub pad_to_pad: f64,
    /// The least space between a solder mask opening and a track of another net.
    pub mask_to_track: f64,
    /// The smallest non-plated hole, and the narrowest plated and non-plated slot.
    pub npth_drill: f64,
    pub plated_slot: f64,
    pub npth_slot: f64,
    /// The smallest silkscreen text, and its thinnest stroke.
    pub min_text_height: f64,
    pub min_text_thickness: f64,
    /// The least space between silkscreen and a pad.
    pub silk_to_pad: f64,
    /// The least solder mask left between two openings of different nets.
    pub mask_bridge: f64,
    /// The space a plane keeps from copper that is not on its net: the process's space
    /// between copper, or its space to a hole in an inner layer if that is more. It is the
    /// space between copper and copper, so it keeps the hole clear too.
    pub plane_clearance: f64,
    /// Whether a via may sit in a pad: the process fills and caps vias as wide as the
    /// router's, and the board has not refused them. Such a via needs a process step the
    /// fabricator charges for, so the router uses one only where a pad cannot be reached
    /// any other way.
    pub via_in_pad: bool,
}

impl Rules {
    /// What the placer keeps between a courtyard and the board edge when the process states
    /// nothing to derive it from.
    pub const DEFAULT_EDGE_WALL: f64 = 2.0;

    /// The space the placer keeps between a courtyard and the board edge, from the process:
    /// the copper to edge space, and one track with its space on the courtyard's side, so a
    /// track can still pass between the part and the edge (`advised_edge_clearance`, the
    /// fabricator's advice for a V-cut, stays a note). It was a fixed 2.0 mm, which nothing
    /// needed: the pads, tracks and vias keep `copper_to_edge` from the edge by their own
    /// rules (the router, and the annealer for a pad outside its courtyard), and boards with
    /// parts this close to the edge route completely and pass the design rules check and
    /// KiCad's (ADR 0033). For the JLCPCB process (0.2 mm, 0.1 mm, 0.1 mm) it is 0.4 mm.
    pub fn edge_wall(&self) -> f64 {
        self.copper_to_edge + self.clearance + self.min_track_width
    }

    /// The rules a process asks for. The router draws at the narrowest track and the
    /// smallest via the process allows, so a board that wants more margin narrows the
    /// process (`.process.min_track &= 0.2mm..1m`). A via's copper is its drill with the
    /// ring on both sides, and at least the process's smallest via.
    ///
    /// Fails if the process is not one this router can draw: an even number of layers, with
    /// what each inner layer is (a plane or a signal layer, ADR 0111). The router draws on the
    /// outer layers only.
    pub fn for_process(process: &Process) -> Result<Rules, String> {
        let layers = process.stackup.layers;
        if layers < 2 || !layers.is_multiple_of(2) {
            return Err(format!(
                "the process `{}` has {layers} copper layers, but a board has an even number of them, two or more",
                process.name
            ));
        }
        let inner = process.stackup.inner.as_ref().map_or(0, |i| i.layers.len());
        if inner != layers as usize - 2 {
            return Err(format!(
                "the process `{}` has {layers} copper layers and says what {inner} of its {} inner layers are",
                process.name,
                layers - 2
            ));
        }
        let plane_hole = process
            .stackup
            .inner
            .as_ref()
            .map_or(0.0, |i| i.hole_clearance);
        // The via is the process's (a default the board may replace, issue #431), and must meet
        // its own rules.
        let (via_drill, via_diameter) = (process.via_drill, process.via_diameter);
        let ring = (via_diameter - via_drill) / 2.0;
        if via_drill < process.min_drill - 1e-9
            || via_diameter < process.min_via_diameter - 1e-9
            || ring < process.min_via_ring - 1e-9
        {
            return Err(format!(
                "the via of `{}` is {via_diameter} mm with a {via_drill} mm hole, but the process makes a hole of at least {} mm, a via of at least {} mm and {} mm of copper round its hole: widen `.process.via_diameter` or `.process.via_drill`",
                process.name, process.min_drill, process.min_via_diameter, process.min_via_ring
            ));
        }
        Ok(Rules {
            clearance: process.min_clearance,
            min_track_width: process.min_track,
            track_width: process.min_track,
            via_diameter,
            min_via_diameter: process.min_via_diameter,
            via_drill,
            min_through_hole: process.min_drill,
            hole_to_hole: process.min_hole_to_hole,
            via_hole_to_hole: process.min_via_hole_to_hole,
            copper_to_edge: process.min_edge_clearance,
            min_annular_ring: process.min_annular_ring,
            via_ring: process.min_via_ring,
            hole_clearance: process.min_hole_clearance,
            plated_hole_clearance: process.min_plated_hole_clearance,
            pad_to_pad: process.min_pad_to_pad,
            mask_to_track: process.min_mask_to_track,
            npth_drill: process.min_npth_drill,
            plated_slot: process.min_plated_slot,
            npth_slot: process.min_npth_slot,
            min_text_height: process.min_text_height,
            min_text_thickness: process.min_silk_width,
            silk_to_pad: process.min_silk_to_pad,
            mask_bridge: process.min_mask_bridge,
            plane_clearance: process.min_clearance.max(plane_hole),
            via_in_pad: process.via_in_pad >= via_diameter,
        })
    }

    /// The text of a `.kicad_pro` for a board with these rules, which KiCad's DRC reads when
    /// the file sits beside the `.kicad_pcb`. `wide` are the nets routed wider than the
    /// rules' track width, grouped by width: each group is a net class of its own, named for
    /// the width (`Track_0.8mm`), that KiCad draws new tracks of those nets in.
    pub fn kicad_pro(&self, name: &str, wide: &[(f64, Vec<String>)]) -> String {
        use Json::{Array, Number as N, Object, Text};
        let o = |fields: Vec<(&str, Json)>| {
            Object(
                fields
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            )
        };
        let severities: Vec<(&str, Json)> = [
            ("annular_width", "error"),
            ("clearance", "error"),
            ("copper_edge_clearance", "error"),
            ("courtyards_overlap", "error"),
            ("drill_out_of_range", "error"),
            ("hole_clearance", "error"),
            ("hole_to_hole", "error"),
            ("shorting_items", "error"),
            // The fabricator clips silkscreen over a mask opening and keeps a web only
            // where it can: neither makes the board unmakeable, so they are warnings.
            ("silk_edge_clearance", "warning"),
            ("silk_over_copper", "warning"),
            ("silk_overlap", "warning"),
            ("solder_mask_bridge", "warning"),
            ("text_height", "error"),
            ("text_thickness", "error"),
            ("track_dangling", "error"),
            ("track_width", "error"),
            ("unconnected_items", "error"),
            ("via_dangling", "error"),
        ]
        .iter()
        .map(|(k, v)| (*k, Text((*v).to_string())))
        .collect();
        let default = o(vec![
            ("clearance", N(self.clearance)),
            ("name", Text("Default".to_string())),
            // Without a priority KiCad 9 ignores the class's clearance and checks the board
            // against its own default of 0.2 mm (measured with KiCad 9.0's DRC: 0.2 mm
            // reported on a 0.1 mm project).
            ("priority", N(2147483647.0)),
            ("track_width", N(self.track_width)),
            ("via_diameter", N(self.via_diameter)),
            ("via_drill", N(self.via_drill)),
        ]);
        // A class KiCad reads needs a `priority`, and states no clearance, so the nets in it
        // keep the space the board is checked to. Lower numbers win.
        let wide_class = |priority: usize, width: f64| {
            o(vec![
                ("name", Text(format!("Track_{}mm", fmt_num(width)))),
                ("priority", N(priority as f64)),
                ("track_width", N(width)),
                ("via_diameter", N(self.via_diameter)),
                ("via_drill", N(self.via_drill)),
            ])
        };
        let mut classes = vec![default];
        classes.extend(
            wide.iter()
                .enumerate()
                .map(|(k, (width, _))| wide_class(k, *width)),
        );
        // KiCad assigns a net to a class by a pattern on its name.
        let patterns: Vec<Json> = wide
            .iter()
            .flat_map(|(width, nets)| {
                nets.iter().map(move |net| {
                    o(vec![
                        ("netclass", Text(format!("Track_{}mm", fmt_num(*width)))),
                        ("pattern", Text(net.clone())),
                    ])
                })
            })
            .collect();
        let mut net_settings = vec![
            ("classes", Array(classes)),
            ("meta", o(vec![("version", N(4.0))])),
        ];
        if !patterns.is_empty() {
            net_settings.push(("netclass_patterns", Array(patterns)));
        }
        let file = o(vec![
            (
                "board",
                o(vec![(
                    "design_settings",
                    o(vec![
                        (
                            "defaults",
                            o(vec![
                                ("board_outline_line_width", N(0.05)),
                                ("copper_line_width", N(self.track_width)),
                                ("courtyard_line_width", N(0.05)),
                                ("silk_line_width", N(self.min_text_thickness)),
                                ("silk_text_size_h", N(self.min_text_height)),
                                ("silk_text_size_v", N(self.min_text_height)),
                                ("silk_text_thickness", N(self.min_text_thickness)),
                            ]),
                        ),
                        (
                            "rules",
                            o(vec![
                                ("min_clearance", N(self.clearance)),
                                ("min_copper_edge_clearance", N(self.copper_to_edge)),
                                ("min_hole_clearance", N(self.clearance)),
                                ("min_hole_to_hole", N(self.via_hole_to_hole)),
                                ("min_silk_clearance", N(self.silk_to_pad)),
                                ("min_text_height", N(self.min_text_height)),
                                ("min_text_thickness", N(self.min_text_thickness)),
                                ("min_through_hole_diameter", N(self.min_through_hole)),
                                ("min_track_width", N(self.min_track_width)),
                                ("min_via_annular_width", N(self.via_ring)),
                                ("min_via_diameter", N(self.min_via_diameter)),
                            ]),
                        ),
                        ("rule_severities", o(severities)),
                        ("track_widths", Array(vec![N(0.0), N(self.track_width)])),
                        (
                            "via_dimensions",
                            Array(vec![
                                o(vec![("diameter", N(0.0)), ("drill", N(0.0))]),
                                o(vec![
                                    ("diameter", N(self.via_diameter)),
                                    ("drill", N(self.via_drill)),
                                ]),
                            ]),
                        ),
                    ]),
                )]),
            ),
            (
                "meta",
                o(vec![
                    ("filename", Text(format!("{name}.kicad_pro"))),
                    ("version", N(3.0)),
                ]),
            ),
            ("net_settings", o(net_settings)),
        ]);
        let mut out = String::new();
        file.write(&mut out, 0);
        out.push('\n');
        out
    }
}

/// Just enough JSON to write a project file.
enum Json {
    Number(f64),
    Text(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn write(&self, out: &mut String, depth: usize) {
        let indent = |out: &mut String, depth: usize| {
            for _ in 0..depth {
                out.push_str("  ");
            }
        };
        match self {
            Json::Number(n) => out.push_str(&fmt_num(*n)),
            Json::Text(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Json::Array(items) => {
                out.push_str("[\n");
                for (i, item) in items.iter().enumerate() {
                    indent(out, depth + 1);
                    item.write(out, depth + 1);
                    out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
                }
                indent(out, depth);
                out.push(']');
            }
            Json::Object(fields) => {
                out.push_str("{\n");
                for (i, (key, value)) in fields.iter().enumerate() {
                    indent(out, depth + 1);
                    let _ = std::fmt::Write::write_fmt(out, format_args!("\"{key}\": "));
                    value.write(out, depth + 1);
                    out.push_str(if i + 1 < fields.len() { ",\n" } else { "\n" });
                }
                indent(out, depth);
                out.push('}');
            }
        }
    }
}

/// The `TwoLayer` process as the whole pipeline resolves it, for tests in this crate.
#[cfg(test)]
pub(crate) fn test_process() -> Process {
    process_from(&[])
}

/// The process of the one board of `source`, as the check, the solver and the design stage
/// resolve it: what `netsmith::build_with` gives. This crate's tests do not depend on the netsmith
/// crate, so that they are not among the dependents of every crate it links (issue #518).
#[cfg(test)]
fn process_of(source: &str) -> netsmith_core::Process {
    let checked = netsmith_check::check("t.nsm", source, &netsmith_check::Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = &checked.boards[0];
    let solution = netsmith_solve::solve(board, &netsmith_solve::Catalogs::jlcpcb());
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution.diagnostics
    );
    netsmith_design::design_board(board, &solution)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .design
        .process
        .expect("the board has a process")
}

/// `TwoLayer` after the board has written each of `narrowings` inside `.process { }`.
#[cfg(test)]
pub(crate) fn process_from(narrowings: &[&str]) -> Process {
    let body: String = narrowings
        .iter()
        .chain(&[".stackup.thickness &= 1.6mm"])
        .map(|n| format!("        {n}\n"))
        .collect();
    let source = format!(
        "import std::mfg::jlcpcb::TwoLayer\nimport std::parts::passive::Resistor\nboard T {{\n    .process: TwoLayer {{\n{body}    }}\n    .r: Resistor\n}}\n"
    );
    process_of(&source)
}

/// `FourLayer` after the board has made GND the first plane and 3V3 the second and written
/// each of `narrowings` inside `.process { }`.
#[cfg(test)]
pub(crate) fn four_layer_process(narrowings: &[&str]) -> Process {
    let body: String = narrowings
        .iter()
        .chain(&[".inner1.net &= \"GND\"", ".inner2.net &= \"3V3\""])
        .map(|n| format!("        {n}\n"))
        .collect();
    let source = format!(
        "import std::mfg::jlcpcb::FourLayer\nimport std::parts::passive::Resistor\nboard T {{\n    .process: FourLayer {{\n{body}    }}\n    .r: Resistor {{\n        .a.net &= \"GND\"\n        .b.net &= \"3V3\"\n    }}\n}}\n"
    );
    process_of(&source)
}

/// `SixLayer` with GND on its first inner layer, 3V3 on its last and the two between them
/// signal layers.
#[cfg(test)]
pub(crate) fn six_layer_process() -> Process {
    let body = [
        ".inner1.net &= \"GND\"",
        ".inner2.use &= \"signal\"",
        ".inner3.use &= \"signal\"",
        ".inner4.net &= \"3V3\"",
    ]
    .map(|n| format!("        {n}\n"))
    .concat();
    let source = format!(
        "import std::mfg::jlcpcb::SixLayer\nimport std::parts::passive::Resistor\nboard T {{\n    .process: SixLayer {{\n{body}    }}\n    .r: Resistor {{\n        .a.net &= \"GND\"\n        .b.net &= \"3V3\"\n    }}\n}}\n"
    );
    process_of(&source)
}

/// ACME's `EightLayer` or `TenLayer` (`layers` 8 or 10, ADR 0112) with GND on its first inner
/// layer, 3V3 on its last and signal layers between them.
#[cfg(test)]
pub(crate) fn acme_process(layers: u32) -> Process {
    let inner = layers - 2;
    let body: String = (1..=inner)
        .map(|n| match n {
            1 => format!("        .inner{n}.net &= \"GND\"\n"),
            n if n == inner => format!("        .inner{n}.net &= \"3V3\"\n"),
            _ => format!("        .inner{n}.use &= \"signal\"\n"),
        })
        .collect();
    let name = if layers == 8 {
        "EightLayer"
    } else {
        "TenLayer"
    };
    let source = format!(
        "import std::mfg::acme::{name}\nimport std::parts::passive::Resistor\nboard T {{\n    .process: {name} {{\n{body}    }}\n    .r: Resistor {{\n        .a.net &= \"GND\"\n        .b.net &= \"3V3\"\n    }}\n}}\n"
    );
    process_of(&source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eight_and_ten_layer_processes_are_routed_on_the_outer_layers() {
        for layers in [8, 10] {
            let p = acme_process(layers);
            assert_eq!(p.stackup.layers, layers);
            let r = Rules::for_process(&p).unwrap_or_else(|e| panic!("{layers}: {e}"));
            assert_eq!(r.plane_clearance, 0.3);
            assert_eq!(r.min_track_width, 0.09);
        }
    }

    #[test]
    fn a_six_layer_process_with_signal_layers_is_routed_on_the_outer_layers() {
        let r = Rules::for_process(&six_layer_process()).unwrap();
        // The planes keep the inner-layer hole space; the signal layers change nothing.
        assert_eq!(r.plane_clearance, 0.3);
        assert_eq!(r.min_track_width, 0.09);
    }

    #[test]
    fn a_process_whose_inner_layers_are_not_all_said_is_refused() {
        let mut p = six_layer_process();
        p.stackup.inner.as_mut().unwrap().layers.pop();
        let err = Rules::for_process(&p).unwrap_err();
        assert!(
            err.contains("has 6 copper layers and says what 3 of its 4 inner layers are"),
            "{err}"
        );
    }

    fn rules() -> Rules {
        Rules::for_process(&test_process()).unwrap()
    }

    #[test]
    fn the_rules_are_what_jlcpcb_publishes_for_a_two_layer_board() {
        let r = rules();
        assert_eq!(
            (r.clearance, r.min_track_width, r.track_width),
            (0.1, 0.1, 0.1)
        );
        assert_eq!((r.via_drill, r.min_through_hole), (0.2, 0.15));
        assert_eq!((r.hole_to_hole, r.copper_to_edge), (0.45, 0.2));
        assert_eq!((r.min_annular_ring, r.via_ring), (0.18, 0.05));
        assert_eq!((r.hole_clearance, r.plated_hole_clearance), (0.2, 0.28));
        assert_eq!((r.pad_to_pad, r.mask_to_track), (0.15, 0.09));
        assert_eq!((r.npth_drill, r.plated_slot, r.npth_slot), (0.5, 0.5, 1.0));
        assert_eq!((r.min_text_height, r.min_text_thickness), (1.0, 0.15));
        assert_eq!(r.silk_to_pad, 0.15);
    }

    #[test]
    fn a_via_may_sit_in_a_pad_if_the_process_fills_vias_as_wide_as_the_router_draws() {
        assert!(rules().via_in_pad);
        let no = Rules::for_process(&process_from(&[".via_in_pad &= 0mm"])).unwrap();
        assert!(!no.via_in_pad);
        // The router's via is 0.45 mm wide on this process: a limit under it is none.
        let narrow = Rules::for_process(&process_from(&[".via_in_pad &= 0mm..0.4mm"])).unwrap();
        assert!(!narrow.via_in_pad);
    }

    #[test]
    fn the_via_is_the_smallest_at_the_standard_price_and_keeps_its_hole_clear() {
        let r = rules();
        // 0.2 mm in 0.45 mm: JLCPCB charges more for a smaller hole or a narrower via.
        assert_eq!((r.via_drill, r.via_diameter), (0.2, 0.45));
        // Its copper and the clearance keep a track the page's 0.2 mm from its hole.
        assert!((r.via_diameter - r.via_drill) / 2.0 + r.clearance >= r.hole_clearance - 1e-9);
        assert!(r.track_width >= r.min_track_width);
    }

    #[test]
    fn a_board_may_choose_another_via_and_the_process_refuses_one_it_cannot_make() {
        let wide = Rules::for_process(&process_from(&[
            ".via_drill &= 0.3mm",
            ".via_diameter &= 0.6mm",
        ]))
        .unwrap();
        assert_eq!((wide.via_drill, wide.via_diameter), (0.3, 0.6));
        let thin = process_from(&[".via_drill &= 0.3mm", ".via_diameter &= 0.35mm"]);
        let err = Rules::for_process(&thin).unwrap_err();
        assert!(err.contains("0.05 mm of copper round its hole"), "{err}");
    }

    #[test]
    fn narrowing_the_process_changes_the_rules() {
        let tight = Rules::for_process(&process_from(&[
            ".min_track &= 0.15mm..1m",
            ".min_clearance &= 0.2mm..1m",
        ]))
        .unwrap();
        assert_eq!((tight.min_track_width, tight.track_width), (0.15, 0.15));
        assert_eq!(tight.clearance, 0.2);
        assert_eq!(
            tight.copper_to_edge, 0.2,
            "what was not narrowed is unchanged"
        );
    }

    #[test]
    fn a_board_of_an_odd_number_of_layers_is_refused() {
        let mut p = test_process();
        p.stackup.layers = 3;
        let err = Rules::for_process(&p).unwrap_err();
        assert!(
            err.contains("TwoLayer") && err.contains("3 copper layers"),
            "{err}"
        );
        assert!(err.contains("an even number of them"), "{err}");
    }

    #[test]
    fn four_layers_that_do_not_say_what_their_inner_layers_are_are_refused() {
        let mut p = test_process();
        p.stackup.layers = 4;
        let err = Rules::for_process(&p).unwrap_err();
        assert!(
            err.contains("4 copper layers and says what 0 of its 2 inner layers are"),
            "{err}"
        );
    }

    #[test]
    fn a_four_layer_process_is_what_jlcpcb_publishes_for_multilayer() {
        let r = Rules::for_process(&four_layer_process(&[])).unwrap();
        assert_eq!((r.min_track_width, r.track_width), (0.09, 0.09));
        assert_eq!(r.clearance, 0.1);
        assert_eq!((r.via_drill, r.min_annular_ring), (0.2, 0.15));
        assert_eq!((r.via_diameter, r.plated_slot), (0.45, 0.35));
        assert_eq!(
            r.plane_clearance, 0.3,
            "the inner layer hole space is the larger"
        );
    }

    #[test]
    fn a_two_layer_process_has_no_plane_space_beyond_its_clearance() {
        let r = rules();
        assert_eq!(r.plane_clearance, r.clearance);
    }

    #[test]
    fn the_project_file_holds_every_rule() {
        let text = rules().kicad_pro("Led", &[]);
        for line in [
            r#""min_clearance": 0.1"#,
            r#""min_copper_edge_clearance": 0.2"#,
            r#""min_hole_to_hole": 0.2"#,
            r#""min_through_hole_diameter": 0.15"#,
            r#""min_track_width": 0.1"#,
            r#""min_via_annular_width": 0.05"#,
            r#""min_via_diameter": 0.25"#,
            r#""min_text_height": 1"#,
            r#""min_text_thickness": 0.15"#,
            r#""silk_over_copper": "warning""#,
            r#""solder_mask_bridge": "warning""#,
            r#""clearance": "error""#,
            r#""filename": "Led.kicad_pro""#,
        ] {
            assert!(text.contains(line), "missing {line}\n{text}");
        }
        assert!(text.ends_with("}\n"));
    }

    #[test]
    fn a_narrowed_rule_reaches_the_project_file() {
        let process = process_from(&[".min_clearance &= 0.25mm..1m"]);
        let text = Rules::for_process(&process).unwrap().kicad_pro("Led", &[]);
        assert!(text.contains(r#""min_clearance": 0.25"#), "{text}");
        assert!(!text.contains(r#""min_clearance": 0.1"#));
    }

    #[test]
    fn the_project_file_is_the_same_every_time() {
        let r = rules();
        assert_eq!(r.kicad_pro("A", &[]), r.kicad_pro("A", &[]));
    }

    #[test]
    fn wide_nets_are_net_classes_by_width_in_the_project_file() {
        let wide = [
            (0.5, vec!["3V3".to_string()]),
            (0.8, vec!["GND".to_string(), "VIN".to_string()]),
        ];
        let text = rules().kicad_pro("Led", &wide);
        for line in [
            r#""name": "Default""#,
            r#""name": "Track_0.5mm""#,
            r#""track_width": 0.8"#,
            r#""priority": 1"#,
            r#""netclass": "Track_0.8mm""#,
            r#""pattern": "VIN""#,
            r#""pattern": "3V3""#,
        ] {
            assert!(text.contains(line), "missing {line}\n{text}");
        }
        let none = rules().kicad_pro("Led", &[]);
        assert!(!none.contains("netclass_patterns"), "{none}");
    }
}
