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
fn a_rail_with_no_min_track_is_drawn_at_the_width_its_current_needs() {
    // ADR 0036, amendment: the router works the width out, so the check has nothing to say.
    let l = lowered_of(&board(2.0, "", ""));
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
    assert!(l.notes.is_empty(), "{:?}", l.notes);
    assert_eq!(
        l.design.features.net_widths,
        [("5V".to_string(), 0.79), ("GND".to_string(), 0.79)]
    );
    // 0.4 A is what a 0.1 mm track carries (450 mA at 10 K): the process's least, no entry.
    assert!(
        lowered_of(&board(0.4, "", ""))
            .design
            .features
            .net_widths
            .is_empty()
    );
    // 0.5 A needs 0.12 mm (0.1148 rounded up).
    let half = lowered_of(&board(0.5, "", ""));
    assert_eq!(width_of(&half, "5V"), Some(0.12));
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
    let l = lowered_of(src);
    // The net between the resistors is not named by the board, and is on neither rail. It
    // takes the chain's 1 A: 0.3004 mm rounded up to 0.31 mm, where it was 0.1 mm and a finding.
    let middle: Vec<&(String, f64)> = l
        .design
        .features
        .net_widths
        .iter()
        .filter(|(n, _)| n != "5V" && n != "GND")
        .collect();
    assert_eq!(middle, [&("net-r1.b".to_string(), 0.31)]);
    // The rails' own nets state 0.35 mm, which is wide enough for 1 A.
    assert_eq!(width_of(&l, "5V"), Some(0.35));
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
    let l = lowered_of(&one_plane);
    assert_eq!(width_of(&l, "5V"), Some(0.79));
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

fn lowered_of(src: &str) -> netsmith_lower::Lowered {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    lower(&c.boards[0], &s).unwrap()
}

fn width_of(l: &netsmith_lower::Lowered, net: &str) -> Option<f64> {
    l.design
        .features
        .net_widths
        .iter()
        .find(|(n, _)| n == net)
        .map(|(_, w)| *w)
}

#[test]
fn the_node_between_a_fuse_and_a_diode_is_drawn_at_the_width_its_current_needs() {
    let l = lowered_of(&fuse_diode("        .current &= 1A..2A\n", "", ""));
    let node = l
        .design
        .nets
        .iter()
        .find(|n| n.name.contains("diode") || n.name.contains("fuse"))
        .expect("the node has a generated name");
    // 2 A needs 0.7814 mm in 1 oz at 10 K; rounded up to 0.01 mm.
    assert_eq!(
        width_of(&l, &node.name),
        Some(0.79),
        "{:?}",
        l.design.features.net_widths
    );
    // The router and the check agree: nothing to warn about, nothing stated.
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
    assert!(l.notes.is_empty(), "{:?}", l.notes);
}

#[test]
fn the_rails_on_such_a_chain_are_sized_too() {
    let l = lowered_of(&fuse_diode("        .current &= 1A..2A\n", "", ""));
    assert_eq!(width_of(&l, "VIN"), Some(0.79));
    assert_eq!(width_of(&l, "VOUT"), Some(0.79));
    assert_eq!(width_of(&l, "GND"), Some(0.79));
}

#[test]
fn a_small_current_leaves_the_net_at_the_process_least() {
    let l = lowered_of(&fuse_diode("        .current &= 1mA..20mA\n", "", ""));
    assert!(
        l.design.features.net_widths.is_empty(),
        "{:?}",
        l.design.features.net_widths
    );
    assert!(l.guidelines.is_empty() && l.notes.is_empty());
}

#[test]
fn the_width_follows_the_temperature_rise() {
    // 2 A at 20 K needs 0.5131 mm (hand-worked above): 0.52 rounded up.
    let src = fuse_diode("        .current &= 1A..2A\n", "", "").replace(
        ".stackup.thickness &= 1.6mm",
        ".stackup.thickness &= 1.6mm\n        .max_temperature_rise &= 20K",
    );
    let l = lowered_of(&src);
    assert_eq!(width_of(&l, "VOUT"), Some(0.52));
}

#[test]
fn a_stated_min_track_is_an_override_and_is_still_checked() {
    let src = fuse_diode(
        "        .current &= 1A..2A\n        .min_track &= 0.3mm..1m\n",
        "",
        "",
    );
    let l = lowered_of(&src);
    assert_eq!(width_of(&l, "VOUT"), Some(0.3));
    let found = about(&l.guidelines, "VOUT");
    assert_eq!(found.len(), 1, "{:?}", l.guidelines);
    assert!(found[0].contains("(a rail's `.min_track`)") && found[0].contains("needs 0.79 mm"));
    // The node between the parts has no rail: it is sized from the current.
    let node: Vec<_> = l
        .design
        .nets
        .iter()
        .filter(|n| n.name.starts_with("net-"))
        .collect();
    assert_eq!(node.len(), 1);
    assert_eq!(width_of(&l, &node[0].name), Some(0.79));
}

#[test]
fn a_net_whose_current_is_not_known_is_not_sized_and_is_named() {
    // Two rails that draw an unstated current on one net: the solver can work out neither.
    let src = fuse_diode("", "", "");
    let l = lowered_of(&src);
    assert!(
        l.design.features.net_widths.is_empty(),
        "{:?}",
        l.design.features.net_widths
    );
    let said = |net: &str, who: &str| {
        l.notes.iter().any(|n| {
            n.starts_with(&format!("the current of {who} is not known"))
                && n.contains(&format!("so the tracks of net `{net}` are not sized"))
                && n.contains("the process's least, 0.1 mm")
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
    let l = lowered_of(&shared_ground("2A", "1A", ""));
    // Each supply net carries its own load: 2 A needs 0.7814 mm, 1 A needs 0.3004 mm.
    assert_eq!(width_of(&l, "5V"), Some(0.79));
    assert_eq!(width_of(&l, "3V3"), Some(0.31));
    // The ground carries both returns at once: 3 A, not the larger 2 A.
    // 3 A: A^0.725 = 3 / 0.13220 = 22.69, A = 74.2 mil^2, /1.378 = 53.8 mil = 1.368 mm.
    let three = ipc2221_width(3.0, 0.035, 10.0, Layer::External);
    assert!((three - 1.3670).abs() < 1e-3, "{three}");
    assert_eq!(width_of(&l, "GND"), Some(1.37));
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
    assert!(l.notes.is_empty(), "{:?}", l.notes);
}

#[test]
fn a_stated_min_track_on_a_shared_ground_is_checked_against_the_sum() {
    // 1 mm carries 2.39 A at 10 K: more than the larger load's 2 A, less than the sum's 3 A.
    // The rail states the width, so it is the user's and it is checked, not raised.
    let l = lowered_of(&shared_ground(
        "2A",
        "1A",
        "        .min_track &= 1mm..1m\n",
    ));
    let ground = about(&l.guidelines, "GND");
    assert_eq!(ground.len(), 1, "{:?}", l.guidelines);
    assert!(
        ground[0].starts_with("net `GND` carries up to 3 A (the sum of the currents on it with every load on at once: rail `load_a` 2 A + rail `load_b` 1 A)")
            && ground[0].contains("tracks are 1 mm wide (a rail's `.min_track`)")
            && ground[0].contains("IPC-2221 needs 1.37 mm"),
        "{}",
        ground[0]
    );
    // The supply nets are not too narrow: 1 mm carries the 2 A of 5V.
    assert!(about(&l.guidelines, "5V").is_empty());
    assert_eq!(width_of(&l, "GND"), Some(1.0));
    assert_eq!(
        width_of(&l, "3V3"),
        Some(0.31),
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
    let l = lowered_of(src);
    // 1.5 A: A^0.725 = 1.5 / 0.13220 = 11.347, A = 28.5 mil^2, /1.378 = 20.7 mil = 0.5255 mm.
    let needs = ipc2221_width(1.5, 0.035, 10.0, Layer::External);
    assert!((needs - 0.5255).abs() < 1e-3, "{needs}");
    assert_eq!(width_of(&l, "5V"), Some(0.53));
    assert_eq!(width_of(&l, "GND"), Some(0.53));
}

/// The buck converter of the guide, with the IC's efficiency relation unless `no_efficiency`.
fn converter(no_efficiency: bool) -> String {
    let relation = ".vin.current == -.buck.sw.current * .buck.sw.voltage / (.vin.voltage * .buck.efficiency)\n";
    let ic = format!(
        "import std::parts::regulator::{{Buck, BuckStage}}
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

part Zbk2 {{
    .vin: Rail
    .buck: Buck {{
        .vref &= 0.8V +/- 1.5%
        .fsw &= 1.2MHz +/- 15%
        .min_input_voltage &= 4.5V
        .max_input_voltage &= 6V
        .max_output_current &= 2A
        .efficiency &= 0.85..0.95
        .min_inductance &= 1.5uH
        .min_input_capacitance &= 2uF
        .min_output_capacitance &= 10uF
    }}
    .buck.sw.neg ~ .vin.neg
    {}
    pins {{
        .vin.pos power \"VIN\" \"1\"
        .vin.neg power \"GND\" \"2\"
        .buck.sw.pos power \"SW\" \"3\"
        .buck.fb input \"FB\" \"4\"
        .en input \"EN\" \"5\"
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
        .current &= 0mA..1A
    }}
    .u: Zbk2
    .stage: BuckStage {{ .vout.voltage &= 1.8V +/- 3% }}
    .input ~ .stage.vin
    .input ~ .u.vin
    .stage.vout ~ .output
    .stage.buck ~ .u.buck
    .u.en ~ .input.pos
}}
"
    )
}

#[test]
fn a_switching_regulator_draws_its_input_from_the_power_balance_not_the_output_current() {
    let l = lowered_of(&converter(false));
    // 1 A at 1.746..1.854 V out, from 4.75 V in at 85 % efficiency: 1.854 / (4.75 * 0.85)
    // = 0.4592 A in. The input net carries that, and not the 1 A of the output.
    let into = 1.854 / (4.75 * 0.85);
    let width_in =
        (ipc2221_width(into, 0.035, 10.0, Layer::External) * 100.0 - 1e-9).ceil() / 100.0;
    assert_eq!(width_of(&l, "VIN"), Some(width_in));
    assert!(width_in < 0.2, "{width_in}");
    // The output net carries the 1 A: 0.3004 mm.
    assert_eq!(width_of(&l, "1V8"), Some(0.31));
    // The ground takes the return of the load and of the input: their sum, and each of the
    // converter's ports once, not again for what is inside the module.
    let sum = 1.0 + into;
    let width_gnd =
        (ipc2221_width(sum, 0.035, 10.0, Layer::External) * 100.0 - 1e-9).ceil() / 100.0;
    assert_eq!(
        width_of(&l, "GND"),
        Some(width_gnd),
        "{:?}",
        l.design.features.net_widths
    );
    assert!(l.guidelines.is_empty(), "{:?}", l.guidelines);
    assert!(l.notes.is_empty(), "{:?}", l.notes);
}

#[test]
fn a_regulator_whose_input_current_is_not_modelled_leaves_its_nets_unsized_and_named() {
    // The IC states no efficiency relation, so nothing says what it draws at its input and the
    // solver has two unknowns on the input net. Nothing is guessed from the output current.
    let l = lowered_of(&converter(true));
    assert_eq!(width_of(&l, "VIN"), None);
    assert_eq!(width_of(&l, "GND"), None);
    let named = |net: &str| {
        l.notes
            .iter()
            .find(|n| n.contains(&format!("so the tracks of net `{net}` are not sized")))
    };
    let vin = named("VIN").unwrap_or_else(|| panic!("{:?}", l.notes));
    assert!(
        vin.contains("rail `u.vin`") && vin.contains("is not known"),
        "{vin}"
    );
    assert!(named("GND").is_some(), "{:?}", l.notes);
    // The output net does not depend on it.
    assert_eq!(width_of(&l, "1V8"), Some(0.31));
    assert!(named("1V8").is_none(), "{:?}", l.notes);
}
