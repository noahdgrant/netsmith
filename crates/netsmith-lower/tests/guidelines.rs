//! Track width against current, by IPC-2221 (ADR 0036).

use netsmith_check::{Loader, check};
use netsmith_lower::guidelines::{Guideline, Layer, ipc2221_current, ipc2221_width};
use netsmith_lower::lower;
use netsmith_solve::{Catalogs, solve};

fn findings_in(loader: &Loader, src: &str) -> Vec<Guideline> {
    let c = check("t.nsm", src, loader);
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    lower(&c.boards[0], &s).unwrap().guidelines
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
    // The track of the message below: 0.2 mm in 1 oz at 10 K carries 745 mA.
    let carries = ipc2221_current(0.2, 0.035, 10.0, Layer::External);
    assert!((carries - 0.7446).abs() < 1e-4, "{carries}");
}

#[test]
fn a_2_a_rail_drawn_at_0_2_mm_is_told_the_numbers() {
    let found = findings(&board(2.0, "        .min_track &= 0.2mm..1m\n", ""));
    let five = about(&found, "5V");
    assert_eq!(five.len(), 1, "{found:?}");
    assert_eq!(
        five[0],
        "net `5V` carries up to 2 A (rail `load`: 0 mA to 2 A) but its tracks are 0.2 mm wide (a rail's `.min_track`), in 1 oz (0.035 mm) outer copper; at a temperature rise of 10 K (the default) that width carries 745 mA and IPC-2221 needs 0.79 mm: give a rail on the net `.min_track &= 0.79mm..1m`, or allow a higher `.process.max_temperature_rise`"
    );
    // The return is the same current on the same width.
    assert_eq!(about(&found, "GND").len(), 1);
    assert!(found.iter().all(|g| g.rule == "IPC-2221" && g.at.is_some()));
}

#[test]
fn a_wide_enough_rail_passes_and_the_width_the_message_asks_for_passes() {
    assert!(findings(&board(2.0, "        .min_track &= 1mm..1m\n", "")).is_empty());
    // 0.7814 mm is needed, so 0.78 fails and the 0.79 the message asks for passes.
    assert_eq!(
        about(
            &findings(&board(2.0, "        .min_track &= 0.78mm..1m\n", "")),
            "5V"
        )
        .len(),
        1
    );
    assert!(findings(&board(2.0, "        .min_track &= 0.79mm..1m\n", "")).is_empty());
}

#[test]
fn a_rail_with_no_min_track_is_checked_at_the_process_least_and_says_so() {
    let found = findings(&board(2.0, "", ""));
    let five = about(&found, "5V");
    assert_eq!(five.len(), 1, "{found:?}");
    assert!(
        five[0].contains("its tracks are 0.1 mm wide (the process's least, because no rail on it states a `.min_track`)")
            && five[0].contains("that width carries 450 mA and IPC-2221 needs 0.79 mm"),
        "{}",
        five[0]
    );
    // A current a 0.1 mm track carries is no finding: 450 mA at 10 K.
    assert!(findings(&board(0.4, "", "")).is_empty());
    assert_eq!(findings(&board(0.5, "", "")).len(), 2);
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
    // 15.79 mil = 0.401 mm: 0.3 mm is not enough, 0.5 mm is.
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
    assert!(at("30K", "0.5mm").is_empty());
    assert!(!at("10K", "0.5mm").is_empty());
    // A range is an upper bound, like the process's other limits: its top is the rise.
    assert!(at("0K..30K", "0.5mm").is_empty());
}

#[test]
fn copper_weight_comes_from_the_process() {
    // 1 A with a 0.35 mm track: 1 oz needs 0.3004 mm, 0.5 oz 0.6008 mm, 2 oz 0.1502 mm.
    let one_amp = board(1.0, "        .min_track &= 0.35mm..1m\n", "");
    assert!(findings_in(&loader_with_copper("1"), &one_amp).is_empty());
    assert!(findings_in(&loader_with_copper("2"), &one_amp).is_empty());
    let half = findings_in(&loader_with_copper("0.5"), &one_amp);
    let five = about(&half, "5V");
    assert!(
        five[0].contains("in 0.5 oz (0.018 mm) outer copper")
            && five[0].contains("IPC-2221 needs 0.61 mm"),
        "{}",
        five[0]
    );
    // And 2 oz is why the thinner track suffices: 0.16 mm is enough for it.
    let thin = board(1.0, "        .min_track &= 0.16mm..1m\n", "");
    assert!(findings_in(&loader_with_copper("2"), &thin).is_empty());
    assert!(!findings_in(&loader_with_copper("1"), &thin).is_empty());
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
        .min_track &= 0.35mm..1m
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
    let found = findings(src);
    // The net between the resistors is not named by the board, and is on neither rail.
    let middle: Vec<&Guideline> = found
        .iter()
        .filter(|g| !g.message.starts_with("net `5V`") && !g.message.starts_with("net `GND`"))
        .collect();
    assert_eq!(middle.len(), 1, "{found:?}");
    assert!(
        middle[0]
            .message
            .contains("carries up to 1 A (part `r1`: 0 mA to 1 A)")
            && middle[0]
                .message
                .contains("tracks are 0.1 mm wide (the process's least"),
        "{}",
        middle[0].message
    );
    // The rails' own nets are wide enough for 1 A at 0.35 mm.
    assert!(about(&found, "5V").is_empty() && about(&found, "GND").is_empty());
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
    let found = findings(&one_plane);
    assert_eq!(about(&found, "5V").len(), 1, "{found:?}");
    assert!(about(&found, "GND").is_empty());
    let without = board(2.0, "", "").replace(
        "    .process: TwoLayer {\n        .stackup.thickness &= 1.6mm\n    }\n",
        "",
    );
    assert!(findings(&without).is_empty());
}
