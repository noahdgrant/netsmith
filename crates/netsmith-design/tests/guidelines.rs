//! Track width against current, by IPC-2221 (ADR 0036).

use netsmith_check::{Assignment, Loader, check, check_with};
use netsmith_design::design_board;
use netsmith_design::guidelines::{Guideline, Layer, ipc2221_current, ipc2221_width, via_current};
use netsmith_solve::{Catalogs, solve};

fn findings_in(loader: &Loader, src: &str) -> Vec<Guideline> {
    let c = check("t.nsm", src, loader);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s).unwrap().guidelines
}

/// A load of `current` on a rail `5V` / `GND`, joined to a supply header and a load header,
/// with `rail` more statements on the rail and `process` more in the process.
fn board(amps: f64, rail: &str, process: &str) -> String {
    format!(
        "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail
board B {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
{process}    }}
    .load: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= 0A..{amps}A
{rail}    }}
    .source: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= -{amps}A..0A
    }}
    .supply: Header1x2
    .out: Header1x2
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
}}
"
    )
}

fn findings(src: &str) -> Vec<Guideline> {
    findings_in(&Loader::std(), src)
}

/// The messages that are about net `net`.
fn about<'a>(found: &'a [Guideline], net: &str) -> Vec<&'a str> {
    let key = format!("net `{net}` ");
    found
        .iter()
        .map(|g| g.message.as_str())
        .filter(|m| m.starts_with(&key))
        .collect()
}

/// `JLCPCB`'s processes with the outer copper of `TwoLayer` made `weight` ounces, as a
/// process that offers that copper would be. The real one is the 1 oz process.
fn loader_with_copper(weight: &str) -> Loader {
    let mut loader = Loader::std();
    let text = include_str!("../../../std/mfg/jlcpcb.nsm");
    let heavy = text.replacen(
        ".stackup.copper_weight &= 1",
        &format!(".stackup.copper_weight &= {weight}"),
        1,
    );
    assert!(
        text.contains(".stackup.copper_weight &= 1"),
        "the 1 oz line is there"
    );
    loader.insert("std::mfg::jlcpcb", "std/mfg/jlcpcb.nsm", &heavy);
    loader
}

// IPC-2221 reference values, worked by hand from the published formula (IPC-2221B, section
// 6.2, "Conductor current carrying capacity": I = k * dT^0.44 * A^0.725, I in amperes, dT
// in degrees Celsius above ambient, A the cross-section in square mils, k = 0.048 for
// outer layers and 0.024 for inner layers; widths are read off the area and the copper's
// thickness). 1 mil is 0.0254 mm, and 1 oz of copper is 0.035 mm = 1.378 mil.
//
// 1 A, dT = 10 K, outer layer:
//   10^0.44 = 2.7542; k * that = 0.048 * 2.7542 = 0.13220
//   A^0.725 = 1 / 0.13220 = 7.5641  =>  A = 7.5641^(1/0.725) = 7.5641^1.3793 = 16.30 mil^2
//   1 oz: w = 16.30 / 1.378 = 11.83 mil = 0.3004 mm
// 2 A, same: A^0.725 = 15.128 => A = 42.39 mil^2, 1 oz: w = 30.77 mil = 0.7814 mm.
// Half the copper doubles the width, twice the copper halves it (the area is what counts):
//   1 A at 0.5 oz: 0.6008 mm; at 2 oz: 0.1502 mm.
// An inner layer has half the k, so it needs the area an outer layer needs for twice the
// current: 1 A inner = 42.39 mil^2 = 0.7814 mm in 1 oz.
#[test]
fn the_formula_gives_the_values_worked_by_hand() {
    let close = |a: f64, b: f64| assert!((a - b).abs() < 5e-4, "{a} is not {b}");
    close(ipc2221_width(1.0, 0.035, 10.0, Layer::External), 0.3004);
    close(ipc2221_width(2.0, 0.035, 10.0, Layer::External), 0.7814);
    close(ipc2221_width(1.0, 0.0175, 10.0, Layer::External), 0.6008);
    close(ipc2221_width(1.0, 0.07, 10.0, Layer::External), 0.1502);
    close(ipc2221_width(1.0, 0.035, 10.0, Layer::Internal), 0.7814);
    // A hotter track carries more: 2 A at 20 K needs A = 27.84 mil^2, 0.5131 mm in 1 oz.
    close(ipc2221_width(2.0, 0.035, 20.0, Layer::External), 0.5131);
}

#[test]
fn the_capacity_of_a_width_is_the_formula_the_other_way_round() {
    for layer in [Layer::External, Layer::Internal] {
        for current in [0.1, 1.0, 3.5] {
            let width = ipc2221_width(current, 0.035, 15.0, layer);
            let back = ipc2221_current(width, 0.035, 15.0, layer);
            assert!(
                (back - current).abs() < 1e-9 * current,
                "{layer:?} {current}"
            );
        }
    }
    // The track of the message below: 0.2 mm in 1 oz at 10 K carries 745 mA, and at its
    // narrowest, 0.16 mm (20% under, the fabricator's tolerance), 633 mA.
    let carries = ipc2221_current(0.2, 0.035, 10.0, Layer::External);
    assert!((carries - 0.7446).abs() < 1e-4, "{carries}");
    let narrowest = ipc2221_current(0.2 * 0.8, 0.035, 10.0, Layer::External);
    assert!((narrowest - 0.6330).abs() < 1e-3, "{narrowest}");
}

#[test]
fn a_2_a_rail_drawn_at_0_2_mm_is_told_the_numbers() {
    let found = findings(&board(2.0, "        .min_track &= 0.2mm..1m\n", ""));
    let five = about(&found, "5V");
    assert_eq!(five.len(), 1, "{found:?}");
    assert_eq!(
        five[0],
        "net `5V` carries up to 2 A (rail `load`: 0 mA to 2 A) but its tracks are 0.2 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 633 mA (at its narrowest, 20% under, the fabricator's tolerance) and IPC-2221 needs 0.98 mm: give a rail on the net `.min_track &= 0.98mm..1m`, or allow a higher `.process.max_temperature_rise`"
    );
    // The return is the same current on the same width.
    assert_eq!(about(&found, "GND").len(), 1);
    assert!(found.iter().all(|g| g.rule == "IPC-2221" && g.at.is_some()));
}

#[test]
fn a_wide_enough_rail_passes_and_the_width_the_message_asks_for_passes() {
    assert!(findings(&board(2.0, "        .min_track &= 1mm..1m\n", "")).is_empty());
    // 0.7814 mm is needed by the narrowest finished track, so a drawn width of 0.7814 / 0.8 =
    // 0.9768 mm: 0.97 fails and the 0.98 the message asks for passes.
    assert_eq!(
        about(
            &findings(&board(2.0, "        .min_track &= 0.97mm..1m\n", "")),
            "5V"
        )
        .len(),
        1
    );
    assert!(findings(&board(2.0, "        .min_track &= 0.98mm..1m\n", "")).is_empty());
}

#[test]
fn a_rail_with_no_min_track_is_drawn_at_the_width_its_current_needs() {
    // ADR 0036, amendment: the router works the width out, so the check has nothing to say.
    let l = designed_of(&board(2.0, "", ""));
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
    assert!(l.notes.is_empty(), "{:?}", l.notes);
    assert_eq!(
        l.design.features.net_widths,
        [("5V".to_string(), 0.98), ("GND".to_string(), 0.98)]
    );
    // 0.3 A needs 0.0713 mm drawn (0.0571 mm over 0.8): under the process's least of 0.1 mm,
    // so the least, no entry. (0.4 A needs 0.106 mm: more than the least, so it is sized.)
    assert!(
        designed_of(&board(0.3, "", ""))
            .design
            .features
            .net_widths
            .is_empty()
    );
    // 0.5 A needs 0.15 mm (0.1148 / 0.8 = 0.1435 rounded up).
    let half = designed_of(&board(0.5, "", ""));
    assert_eq!(width_of(&half, "5V"), Some(0.15));
    assert!(half.guidelines.is_empty());
}

#[test]
fn the_process_least_track_is_the_floor_of_a_narrower_min_track() {
    // The process narrowed to 0.3 mm: a rail's 0.1 mm is drawn at 0.3.
    let found = findings(&board(
        2.0,
        "        .min_track &= 0.1mm..1m\n",
        "        .min_track &= 0.3mm..1m\n",
    ));
    assert!(
        about(&found, "5V")[0].contains("tracks are 0.3 mm wide"),
        "{found:?}"
    );
}

#[test]
fn the_temperature_rise_is_the_process_s_and_a_stated_one_is_not_called_the_default() {
    // 2 A needs 0.7814 mm at 10 K. At 30 K: 30^0.44 = 4.4668, k * that = 0.21441,
    // 2 / 0.21441 = 9.328, A = 9.328^1.3793 = 21.76 mil^2, and in 1 oz (1.378 mil) that is
    // 15.79 mil = 0.401 mm, drawn at 0.401 / 0.8 = 0.5015 mm for the narrowest finished track:
    // 0.3 mm is not enough, 0.5 mm is not either, 0.51 mm is.
    let at = |rise: &str, width: &str| {
        findings(&board(
            2.0,
            &format!("        .min_track &= {width}..1m\n"),
            &format!("        .max_temperature_rise &= {rise}\n"),
        ))
    };
    let found = at("30K", "0.3mm");
    let five = about(&found, "5V");
    assert!(
        five[0].contains("at a temperature rise of 30 K that width carries")
            && !five[0].contains("(the default)"),
        "{}",
        five[0]
    );
    let needs = ipc2221_width(2.0, 0.035, 30.0, Layer::External);
    assert!(needs > 0.3 && needs < 0.5, "{needs}");
    assert!(at("30K", "0.51mm").is_empty());
    assert!(!at("10K", "0.51mm").is_empty());
    // A range is an upper bound, like the process's other limits: its top is the rise.
    assert!(at("0K..30K", "0.51mm").is_empty());
}

#[test]
fn copper_weight_comes_from_the_process() {
    // 1 A: 1 oz needs 0.3004 mm, 0.5 oz 0.6008 mm, 2 oz 0.1502 mm; drawn, over 0.8, 0.3755 mm,
    // 0.7510 mm and 0.1877 mm. A 0.38 mm track is enough for 1 oz and 2 oz.
    let one_amp = board(1.0, "        .min_track &= 0.38mm..1m\n", "");
    assert!(findings_in(&loader_with_copper("1"), &one_amp).is_empty());
    assert!(findings_in(&loader_with_copper("2"), &one_amp).is_empty());
    let half = findings_in(&loader_with_copper("0.5"), &one_amp);
    let five = about(&half, "5V");
    assert!(
        five[0].contains("in 0.5 oz (0.018 mm) outer copper")
            && five[0].contains("IPC-2221 needs 0.76 mm"),
        "{}",
        five[0]
    );
    // And 2 oz is why the thinner track suffices: 0.19 mm is enough for it.
    let thin = board(1.0, "        .min_track &= 0.19mm..1m\n", "");
    assert!(findings_in(&loader_with_copper("2"), &thin).is_empty());
    assert!(!findings_in(&loader_with_copper("1"), &thin).is_empty());
}

#[test]
fn the_2_oz_provider_of_two_layer_rates_a_track_by_its_copper() {
    // The real provider (ADR 0113), as the build's pick of providers takes it for a board that
    // narrows the weight: 2 oz needs 0.1502 mm for 1 A, 0.1877 mm drawn over the tolerance,
    // so 0.19 mm is enough, as it is not in 1 oz.
    let thin = board(
        1.0,
        "        .min_track &= 0.19mm..1m\n",
        "        .stackup.copper_weight &= 2\n",
    );
    let mut assign = Assignment::new();
    assign.insert(
        "B".to_string(),
        [(
            "process".to_string(),
            "std::mfg::jlcpcb::JlcTwoLayer2oz".to_string(),
        )]
        .into_iter()
        .collect(),
    );
    let c = check_with("t.nsm", &thin, &Loader::std(), &assign);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    let designed = design_board(&c.boards[0], &s).unwrap();
    assert!(designed.guidelines.is_empty(), "{:?}", designed.guidelines);
    let p = designed.design.process.expect("a process");
    assert_eq!(p.name, "TwoLayer");
    assert_eq!(p.stackup.copper_weight, 2.0);
    assert_eq!(
        (
            p.min_track,
            p.min_clearance,
            p.min_annular_ring,
            p.min_mask_bridge
        ),
        (0.16, 0.16, 0.254, 0.2)
    );
    let one_oz = board(1.0, "        .min_track &= 0.19mm..1m\n", "");
    assert!(!findings(&one_oz).is_empty());
}

#[test]
fn the_nets_between_series_parts_take_the_chain_s_current() {
    let src = "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail
/// A two-terminal part that passes a current, with no catalog to choose from.
part Link {
    pins {
        .a passive \"A\" \"1\"
        .b passive \"B\" \"2\"
    }
    #[role(current)]
    .current: Current
    bridge .a ~> .b
}
board Chain {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= -1A..0A
        .min_track &= 0.38mm..1m
    }
    .r1: Link
    .r2: Link
    .r1.current &= 0A..1A
    .supply.pos ~> .r1 ~> .r2 ~> .supply.neg
    .j: Header1x2
    .j.p1 ~ .supply.pos
    .j.p2 ~ .supply.neg
}
";
    let l = designed_of(src);
    // The net between the resistors is not named by the board, and is on neither rail. It
    // takes the chain's 1 A: 0.3004 / 0.8 = 0.3755 mm rounded up to 0.38 mm, where it was 0.1 mm and a finding.
    let middle: Vec<&(String, f64)> = l
        .design
        .features
        .net_widths
        .iter()
        .filter(|(n, _)| n != "5V" && n != "GND")
        .collect();
    assert_eq!(middle, [&("net-r1.b".to_string(), 0.38)]);
    // The rails' own nets state 0.38 mm, which is wide enough for 1 A.
    assert_eq!(width_of(&l, "5V"), Some(0.38));
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
}

#[test]
fn a_plane_is_not_a_track_and_a_board_with_no_process_has_no_copper_to_check() {
    let four = "import std::mfg::jlcpcb::FourLayer
import std::parts::connector::Header1x2
import std::sources::Rail
board Planes {
    .process: FourLayer {
        .stackup.thickness &= 1.6mm
        .inner1.net &= \"GND\"
        .inner2.net &= \"5V\"
    }
    .load: Rail {
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= 0A..2A
    }
    .source: Rail {
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= -2A..0A
    }
    .a: Header1x2
    .b: Header1x2
    .a.p1 ~ .source.pos
    .a.p2 ~ .source.neg
    .b.p1 ~ .load.pos
    .b.p2 ~ .load.neg
}
";
    assert!(findings(four).is_empty(), "{:?}", findings(four));
    // With only `GND` a plane, `5V` is a track again.
    let one_plane = four.replace(".inner2.net &= \"5V\"", ".inner2.net &= \"GND\"");
    let l = designed_of(&one_plane);
    assert_eq!(width_of(&l, "5V"), Some(0.98));
    assert_eq!(width_of(&l, "GND"), None, "a plane is an area");
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
    let without = board(2.0, "", "").replace(
        "    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n",
        "",
    );
    assert!(findings(&without).is_empty());
}

// ---- the router sizes every net (issue #4, ADR 0036 amendment) -----------------------

/// A fuse and a diode in series between a 12 V input and a load that draws `load`: the
/// node between them belongs to no rail.
fn fuse_diode(load: &str, supply: &str, fuse: &str) -> String {
    format!(
        "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::parts::diode::Diode
import std::parts::passive::Fuse
import std::sources::Rail
board B {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .supply: Rail {{
        .voltage &= 12V
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
{supply}    }}
    .load: Rail {{
        .pos.net &= \"VOUT\"
        .neg.net &= \"GND\"
{load}    }}
    .fuse: Fuse{fuse}
    .diode: Diode
    .inlet: Header1x2
    .outlet: Header1x2
    .supply.pos ~> .fuse ~> .diode ~> .load.pos
    .supply.neg ~ .load.neg
    .inlet.p1 ~ .supply.pos
    .inlet.p2 ~ .supply.neg
    .outlet.p1 ~ .load.pos
    .outlet.p2 ~ .load.neg
}}
"
    )
}

fn designed_of(src: &str) -> netsmith_design::Designed {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s).unwrap()
}

fn width_of(l: &netsmith_design::Designed, net: &str) -> Option<f64> {
    l.design
        .features
        .net_widths
        .iter()
        .find(|(n, _)| n == net)
        .map(|(_, w)| *w)
}

#[test]
fn the_node_between_a_fuse_and_a_diode_is_drawn_at_the_width_its_current_needs() {
    let l = designed_of(&fuse_diode("        .current &= 1A..2A\n", "", ""));
    let node = l
        .design
        .nets
        .iter()
        .find(|n| n.name.contains("diode") || n.name.contains("fuse"))
        .expect("the node has a generated name");
    // 2 A needs 0.7814 mm in 1 oz at 10 K, 0.9768 mm for the narrowest finished track;
    // rounded up to 0.01 mm.
    assert_eq!(
        width_of(&l, &node.name),
        Some(0.98),
        "{:?}",
        l.design.features.net_widths
    );
    // The router and the check agree: nothing to warn about, nothing stated.
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
    assert!(l.notes.is_empty(), "{:?}", l.notes);
}

#[test]
fn a_via_is_rated_by_its_plated_barrel_and_a_net_gets_as_many_as_its_current_needs() {
    // The process's via: a 0.2 mm hole with 0.018 mm of plating round it is 0.01233 mm², 19.1
    // square mils of copper; at the inner-layer constant and 10 K it carries 0.561 A.
    let each = via_current(0.2, 0.018, 10.0);
    assert!((each - 0.561).abs() < 0.002, "{each}");
    assert!(
        via_current(0.3, 0.018, 10.0) > each,
        "a wider hole carries more"
    );
    assert_eq!(via_current(0.0, 0.018, 10.0), 0.0);
    // So the 2 A node between the fuse and the diode needs four of them side by side.
    let l = designed_of(&fuse_diode("        .current &= 1A..2A\n", "", ""));
    let node = l
        .design
        .features
        .net_currents
        .iter()
        .find(|c| c.net.contains("diode") || c.net.contains("fuse"))
        .expect("the node carries a current");
    assert!((node.current - 2.0).abs() < 1e-9, "{node:?}");
    assert_eq!(node.vias, 4, "{node:?}");
    assert!(
        (node.least_width - ipc2221_width(2.0, 0.035, 10.0, Layer::External) / 0.8).abs() < 1e-9
    );
}

#[test]
fn the_rails_on_such_a_chain_are_sized_too() {
    let l = designed_of(&fuse_diode("        .current &= 1A..2A\n", "", ""));
    assert_eq!(width_of(&l, "VIN"), Some(0.98));
    assert_eq!(width_of(&l, "VOUT"), Some(0.98));
    assert_eq!(width_of(&l, "GND"), Some(0.98));
}

#[test]
fn a_small_current_leaves_the_net_at_the_process_least() {
    let l = designed_of(&fuse_diode("        .current &= 1mA..20mA\n", "", ""));
    assert!(
        l.design.features.net_widths.is_empty(),
        "{:?}",
        l.design.features.net_widths
    );
    assert!(l.guidelines.is_empty() && l.notes.is_empty());
}

#[test]
fn the_width_follows_the_temperature_rise() {
    // 2 A at 20 K needs 0.5131 mm (hand-worked above), 0.6414 mm over 0.8: 0.65 rounded up.
    let src = fuse_diode("        .current &= 1A..2A\n", "", "").replace(
        ".stackup.thickness &= 1.6mm",
        ".stackup.thickness &= 1.6mm\n        .max_temperature_rise &= 20K",
    );
    let l = designed_of(&src);
    assert_eq!(width_of(&l, "VOUT"), Some(0.65));
}

#[test]
fn a_stated_min_track_is_an_override_and_is_still_checked() {
    let src = fuse_diode(
        "        .current &= 1A..2A\n        .min_track &= 0.3mm..1m\n",
        "",
        "",
    );
    let l = designed_of(&src);
    assert_eq!(width_of(&l, "VOUT"), Some(0.3));
    let found = about(&l.guidelines, "VOUT");
    assert_eq!(found.len(), 1, "{:?}", l.guidelines);
    assert!(found[0].contains("(a rail's `.min_track`)") && found[0].contains("needs 0.98 mm"));
    // The node between the parts has no rail: it is sized from the current.
    let node: Vec<_> = l
        .design
        .nets
        .iter()
        .filter(|n| n.name.starts_with("net-"))
        .collect();
    assert_eq!(node.len(), 1);
    assert_eq!(width_of(&l, &node[0].name), Some(0.98));
}

#[test]
fn a_net_with_no_current_known_stays_at_the_process_least_and_is_named() {
    // Two rails that draw an unstated current on one net: the solver can work out neither,
    // and nothing on the net is known to size it from.
    let src = fuse_diode("", "", "");
    let l = designed_of(&src);
    assert!(
        l.design.features.net_widths.is_empty(),
        "{:?}",
        l.design.features.net_widths
    );
    let said = |net: &str, who: &str| {
        l.notes.iter().any(|n| {
            n.starts_with(&format!("the current of {who} is not known"))
                && n.contains(&format!(
                    "so the tracks of net `{net}` are the process's least, 0.1 mm, a lower bound: no current on the net is known"
                ))
        })
    };
    assert!(
        said("net-diode.anode", "part `fuse`, part `diode`"),
        "{:?}",
        l.notes
    );
    assert!(said("VOUT", "rail `load`, part `diode`"), "{:?}", l.notes);
    assert!(l.guidelines.is_empty(), "nothing is known to be too narrow");
}

#[test]
fn the_note_of_a_net_whose_width_is_a_lower_bound_says_it_is_a_track_width_not_sized() {
    // Every note of the design stage that names a net with a current not known has the cause `Track`,
    // one cause for each note, in the order of the notes.
    let l = designed_of(&fuse_diode("", "", ""));
    assert!(!l.notes.is_empty());
    assert_eq!(l.note_causes.len(), l.notes.len());
    assert!(
        l.note_causes
            .iter()
            .all(|&c| c == netsmith_core::Cause::Track),
        "{:?}",
        l.note_causes
    );
}

// ---- Kirchhoff's current law on every net (issue #4, ADR 0036 amendment 2) ------------

/// Two supplies, 5 V and 3.3 V, that return through one ground, with loads of `a` and `b`.
fn shared_ground(a: &str, b: &str, extra: &str) -> String {
    format!(
        "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail
board Shared {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .source_a: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= -{a}..0A
    }}
    .source_b: Rail {{
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
        .current &= -{b}..0A
    }}
    .load_a: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= 0A..{a}
{extra}    }}
    .load_b: Rail {{
        .pos.net &= \"3V3\"
        .neg.net &= \"GND\"
        .current &= 0A..{b}
    }}
    .ha: Header1x2
    .hb: Header1x2
    .hc: Header1x2
    .hd: Header1x2
    .ha.p1 ~ .source_a.pos
    .ha.p2 ~ .source_a.neg
    .hb.p1 ~ .source_b.pos
    .hb.p2 ~ .source_b.neg
    .hc.p1 ~ .load_a.pos
    .hc.p2 ~ .load_a.neg
    .hd.p1 ~ .load_b.pos
    .hd.p2 ~ .load_b.neg
}}
"
    )
}

#[test]
fn a_ground_shared_by_a_2_a_and_a_1_a_rail_is_sized_for_3_a() {
    let l = designed_of(&shared_ground("2A", "1A", ""));
    // Each supply net carries its own load: 2 A needs 0.7814 mm, 1 A needs 0.3004 mm; drawn,
    // over 0.8, 0.9768 mm and 0.3755 mm.
    assert_eq!(width_of(&l, "5V"), Some(0.98));
    assert_eq!(width_of(&l, "3V3"), Some(0.38));
    // The ground carries both returns at once: 3 A, not the larger 2 A.
    // 3 A: A^0.725 = 3 / 0.13220 = 22.69, A = 74.2 mil^2, /1.378 = 53.8 mil = 1.368 mm.
    let three = ipc2221_width(3.0, 0.035, 10.0, Layer::External);
    assert!((three - 1.3670).abs() < 1e-3, "{three}");
    // Drawn for the narrowest finished track: 1.3670 / 0.8 = 1.7088 mm.
    assert_eq!(width_of(&l, "GND"), Some(1.71));
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
    assert!(l.notes.is_empty(), "{:?}", l.notes);
}

#[test]
fn a_stated_min_track_on_a_shared_ground_is_checked_against_the_sum() {
    // 1 mm drawn is 0.8 mm at its narrowest, which carries 2.03 A at 10 K (1 mm itself 2.39 A): more than the larger load's 2 A, less than the sum's 3 A.
    // The rail states the width, so it is the user's and it is checked, not raised.
    let l = designed_of(&shared_ground(
        "2A",
        "1A",
        "        .min_track &= 1mm..1m\n",
    ));
    let ground = about(&l.guidelines, "GND");
    assert_eq!(ground.len(), 1, "{:?}", l.guidelines);
    assert!(
        ground[0].starts_with("net `GND` carries up to 3 A (the sum of the currents on it with every load on at once: rail `load_a` 2 A + rail `load_b` 1 A)")
            && ground[0].contains("tracks are 1 mm wide (a rail's `.min_track`)")
            && ground[0].contains("IPC-2221 needs 1.71 mm"),
        "{}",
        ground[0]
    );
    // The supply nets are not too narrow: 1 mm carries the 2 A of 5V.
    assert!(about(&l.guidelines, "5V").is_empty());
    assert_eq!(width_of(&l, "GND"), Some(1.0));
    assert_eq!(
        width_of(&l, "3V3"),
        Some(0.38),
        "the unstated net is still sized"
    );
}

#[test]
fn loads_on_one_supply_net_add_up() {
    // Three loads of 0.5 A on one rail: the supply net and the ground carry 1.5 A.
    let src = "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail
board Three {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .source: Rail {
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= -1.5A..0A
    }
    .a: Rail {
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= 0A..0.5A
    }
    .b: Rail {
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= 0A..0.5A
    }
    .c: Rail {
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= 0A..0.5A
    }
    .h0: Header1x2
    .h1: Header1x2
    .h2: Header1x2
    .h3: Header1x2
    .h0.p1 ~ .source.pos
    .h0.p2 ~ .source.neg
    .h1.p1 ~ .a.pos
    .h1.p2 ~ .a.neg
    .h2.p1 ~ .b.pos
    .h2.p2 ~ .b.neg
    .h3.p1 ~ .c.pos
    .h3.p2 ~ .c.neg
}
";
    let l = designed_of(src);
    // 1.5 A: A^0.725 = 1.5 / 0.13220 = 11.347, A = 28.5 mil^2, /1.378 = 20.7 mil = 0.5255 mm,
    // drawn at 0.5255 / 0.8 = 0.6569 mm.
    let needs = ipc2221_width(1.5, 0.035, 10.0, Layer::External);
    assert!((needs - 0.5255).abs() < 1e-3, "{needs}");
    assert_eq!(width_of(&l, "5V"), Some(0.66));
    assert_eq!(width_of(&l, "GND"), Some(0.66));
}

/// The buck converter of the guide, with the IC's efficiency relation unless `no_efficiency`.
fn converter(no_efficiency: bool) -> String {
    converter_of(no_efficiency, "1A", "")
}

/// The same with a load of up to `load` on the output and `extra` more statements on the
/// output rail.
fn converter_of(no_efficiency: bool, load: &str, extra: &str) -> String {
    let relation = ".vin.current == -.sw.current * .sw.voltage / (.vin.voltage * .efficiency)\n";
    let ic = format!(
        "import std::parts::regulator::{{Buck, BuckIc}}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

part Zbk2 provides BuckIc {{
    .vref &= 0.8V +/- 1.5%
    .fsw &= 1.2MHz +/- 15%
    .min_input_voltage &= 4.5V
    .max_input_voltage &= 6V
    .max_output_current &= 2A
    .efficiency &= 0.85..0.95
    .min_inductance &= 1.5uH
    .min_input_capacitance &= 2uF
    .min_output_capacitance &= 10uF
    {}
    pins {{
        .vin.pos power \"VIN\" \"1\"
        .vin.neg power \"GND\" \"2\"
        .sw.pos power \"SW\" \"3\"
        .fb input \"FB\" \"4\"
        .en input \"EN\" \"5\"
    }}
    support {{
        .en ~ .vin.pos
    }}
}}
",
        if no_efficiency { "" } else { relation }
    );
    format!(
        "{ic}
board Converter {{
    .process: TwoLayer {{
        .stackup.thickness &= 1.6mm
    }}
    .input: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= \"VIN\"
        .neg.net &= \"GND\"
    }}
    .output: Rail {{
        .pos.net &= \"1V8\"
        .neg.net &= \"GND\"
        .current &= 0mA..{load}
{extra}    }}
    .stage: Buck<Zbk2> {{ .vout.voltage &= 1.8V +/- 3% }}
    .input ~ .stage.vin
    .stage.vout ~ .output
}}
"
    )
}

#[test]
fn a_switching_regulator_draws_its_input_from_the_power_balance_not_the_output_current() {
    let l = designed_of(&converter(false));
    // 1 A at 1.746..1.854 V out, from 4.75 V in at 85 % efficiency: 1.854 / (4.75 * 0.85)
    // = 0.4592 A in. The input net carries that, and not the 1 A of the output.
    let into = 1.854 / (4.75 * 0.85);
    let width_in =
        (ipc2221_width(into, 0.035, 10.0, Layer::External) / 0.8 * 100.0 - 1e-9).ceil() / 100.0;
    assert_eq!(width_of(&l, "VIN"), Some(width_in));
    assert!(width_in < 0.2, "{width_in}");
    // The output net carries the 1 A: 0.3004 mm, drawn at 0.3755 mm.
    assert_eq!(width_of(&l, "1V8"), Some(0.38));
    // The ground takes the return of the load and of the input: their sum, and each of the
    // converter's ports once, not again for what is inside the module.
    let sum = 1.0 + into;
    let width_gnd =
        (ipc2221_width(sum, 0.035, 10.0, Layer::External) / 0.8 * 100.0 - 1e-9).ceil() / 100.0;
    assert_eq!(
        width_of(&l, "GND"),
        Some(width_gnd),
        "{:?}",
        l.design.features.net_widths
    );
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
    assert!(l.notes.is_empty(), "{:?}", l.notes);
}

/// The width in mm that `amps` needs in 1 oz copper at 10 K, for a track finished 20% narrower
/// than drawn (the fabricator's tolerance), rounded up to 0.01 mm.
fn width_for(amps: f64) -> f64 {
    (ipc2221_width(amps, 0.035, 10.0, Layer::External) / 0.8 * 100.0 - 1e-9).ceil() / 100.0
}

/// The note about net `net`, if there is one.
fn note_of<'a>(l: &'a netsmith_design::Designed, net: &str) -> Option<&'a String> {
    l.notes
        .iter()
        .find(|n| n.contains(&format!("so the tracks of net `{net}` are")))
}

#[test]
fn a_known_current_beside_an_unknown_one_sizes_the_net_for_the_known_part() {
    // The IC states no efficiency relation, so nothing says what it draws at its input and the
    // solver has two unknowns on the input net. Nothing is guessed from the output current.
    // The ground returns the load's 1.2 A, which is known, plus the input's current, which is
    // not: the known part is a lower bound and is what the ground is drawn for.
    let l = designed_of(&converter_of(true, "1.2A", ""));
    let known = width_for(1.2);
    assert_eq!(
        known, 0.49,
        "1.2 A needs 0.3863 mm, 0.4828 mm over 0.8: 0.49 rounded up"
    );
    assert_eq!(width_of(&l, "GND"), Some(known));
    let gnd = note_of(&l, "GND").unwrap_or_else(|| panic!("{:?}", l.notes));
    assert!(
        gnd.starts_with("the current of rail `input`, rail `stage.ic.vin` is not known")
            && gnd.contains(
                "so the tracks of net `GND` are 0.49 mm, a lower bound: sized for the 1.2 A that is known, not for the whole current"
            ),
        "{gnd}"
    );
    // The input net has no known current at all: the process's least, and the note says so.
    assert_eq!(width_of(&l, "VIN"), None);
    let vin = note_of(&l, "VIN").unwrap_or_else(|| panic!("{:?}", l.notes));
    assert!(
        vin.contains("rail `stage.ic.vin`")
            && vin.contains("is not known")
            && vin.contains(
                "are the process's least, 0.1 mm, a lower bound: no current on the net is known"
            ),
        "{vin}"
    );
    // The output net does not depend on it.
    assert_eq!(width_of(&l, "1V8"), Some(known));
    assert!(note_of(&l, "1V8").is_none(), "{:?}", l.notes);
    // Drawn for what is known, so a width the tool chose is never a finding.
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
}

#[test]
fn a_net_whose_known_current_needs_less_than_the_least_track_keeps_the_least() {
    // 5 mA is known beside an unknown current: the least track carries it, so the net is at
    // the process's least, and the note says what the least track carries (the load and the
    // feedback divider's few microamps, which `Buck`'s bound on it sizes, #702).
    let l = designed_of(&converter_of(true, "5mA", ""));
    assert_eq!(width_of(&l, "GND"), None);
    let gnd = note_of(&l, "GND").unwrap_or_else(|| panic!("{:?}", l.notes));
    assert!(
        gnd.contains(
            "are the process's least, 0.1 mm, a lower bound: it carries the 5.02 mA that is known, not the whole current"
        ),
        "{gnd}"
    );
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
}

#[test]
fn a_net_whose_every_current_is_known_is_sized_as_before_and_has_no_note() {
    let l = designed_of(&converter_of(false, "1.2A", ""));
    let into = 1.2 * 1.854 / (4.75 * 0.85);
    assert_eq!(width_of(&l, "1V8"), Some(0.49));
    assert_eq!(width_of(&l, "VIN"), Some(width_for(into)));
    assert_eq!(width_of(&l, "GND"), Some(width_for(1.2 + into)));
    assert!(l.notes.is_empty(), "{:?}", l.notes);
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
}

#[test]
fn a_stated_min_track_wider_than_the_known_current_needs_is_respected() {
    let l = designed_of(&converter_of(
        true,
        "1.2A",
        "        .min_track &= 0.9mm..1m\n",
    ));
    // The rail decided the width of its nets: neither raised nor reduced, no finding, and no
    // note on a net whose width the board stated.
    assert_eq!(width_of(&l, "GND"), Some(0.9));
    assert_eq!(width_of(&l, "1V8"), Some(0.9));
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
    assert!(note_of(&l, "GND").is_none(), "{:?}", l.notes);
}

#[test]
fn a_stated_min_track_narrower_than_the_known_current_needs_is_still_a_finding() {
    let l = designed_of(&converter_of(
        true,
        "1.2A",
        "        .min_track &= 0.2mm..1m\n",
    ));
    assert_eq!(width_of(&l, "GND"), Some(0.2));
    let found = about(&l.guidelines, "GND");
    assert_eq!(found.len(), 1, "{:?}", l.guidelines);
    // The current is the known part, and the message says there may be more.
    assert!(
        found[0].starts_with("net `GND` carries at least 1.2 A (")
            && found[0].contains(
                "the current of rail `input`, rail `stage.ic.vin` is not known, so it may be more"
            )
            && found[0].contains("tracks are 0.2 mm wide (a rail's `.min_track`)")
            && found[0].contains("IPC-2221 needs 0.49 mm"),
        "{}",
        found[0]
    );
    // The net whose currents are all known keeps the words it had.
    let out = about(&l.guidelines, "1V8");
    assert!(
        out.len() == 1 && out[0].starts_with("net `1V8` carries up to 1.2 A ("),
        "{out:?}"
    );
}

// ---- the width from the DC analysis' current into a net (ADR 0076, amendment 8) -----------

/// A 9 V to 16 V supply behind a 0.5 ohm link feeds a switching regulator (5 W out at 90 %) and
/// a resistor of 10 ohm. The regulator draws most at 9 V and the resistor at 16 V, so the node
/// between the link and them carries 1.92 A at the most, and not the 2.24 A that each load at its
/// largest adds up to. The node is drawn at the width 1.92 A needs.
#[test]
fn a_net_the_dc_analysis_solved_is_drawn_for_the_most_it_carries_over_the_corners() {
    let src = r#"import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

part Link {
    .designator &= "R"
    pins {
        .a passive "A" "1"
        .b passive "B" "2"
    }
    .resistance: Resistance
    .rated_current: Current
    #[role(current)]
    .current: Current
    .drop: Voltage
    .drop == .current * .resistance
    .current <= .rated_current
    bridge .a ~> .b
    dc resistor .a .b .resistance
}

part Sw {
    .vin: Rail
    .vout: Rail
    .efficiency: Number
    .min_input: Voltage
    .vin.neg ~ .vout.neg
    .vin.voltage >= .min_input
    .designator &= "U"
    pins {
        .vin.pos  power "VIN"  "1"
        .vin.neg  power "GND"  "2"
        .vout.pos power "VOUT" "3"
        .vout.neg power "GND2" "4"
    }
    dc switching_regulator .vin .vout .efficiency
}

board B {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 9V..16V
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .out: Rail {
        .pos.net &= "V5"
        .neg.net &= "GND"
        .current &= 1A
    }
    .link: Link { .resistance &= 0.5ohm; .rated_current &= 5A }
    .rl: Link { .resistance &= 10ohm; .rated_current &= 5A }
    .reg: Sw { .efficiency &= 0.9; .min_input &= 4.5V; .vout.voltage &= 5V }
    .supply.pos ~> .link ~> .reg.vin.pos
    .supply.neg ~ .reg.vin.neg
    .reg.vin.pos ~> .rl ~> .supply.neg
    .reg.vout ~ .out
}
"#;
    let l = designed_of(src);
    let node = l
        .design
        .nets
        .iter()
        .find(|n| n.name.contains("link"))
        .expect("the node after the link has a generated name");
    let total = |v: f64| {
        let mut i = 0.0;
        for _ in 0..200 {
            let vx = v - 0.5 * i;
            i = 5.0 / (0.9 * vx) + vx / 10.0;
        }
        i
    };
    let most = total(16.0).max(total(9.0));
    let width = width_of(&l, &node.name).expect("the node is sized");
    let for_most = ipc2221_width(most, 0.035, 10.0, Layer::External) / 0.8;
    let for_sum = ipc2221_width(2.24, 0.035, 10.0, Layer::External) / 0.8;
    assert!(
        width >= for_most && width <= for_most + 0.011 && width < for_sum - 0.02,
        "{width} mm for {most} A: {for_most} mm; the sum would need {for_sum} mm"
    );
    assert!(l.notes.is_empty(), "{:?}", l.notes);
}
