//! The current of a branch (issue #1113): a pin of a part that states `.current` has the copper
//! from its pads to the rest of its net sized for that current, and not for the net's.

use netsmith_check::{Loader, check};
use netsmith_design::design_board;
use netsmith_solve::{Catalogs, solve};

/// A 3 A load on `5V`, a supply header and a load header, a part that draws up to 20 mA through
/// its rail `vdd` on the same net, and a probe header whose `p1` is on `5V` too; `more` adds
/// statements to the board.
fn board(more: &str) -> String {
    format!(
        "import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

/// An invented part that draws up to 20 mA.
part Sink {{
    .designator &= \"U\"
    .vdd: Rail
    .vdd.current &= 0mA..20mA
    pins {{
        .vdd.pos power \"VDD\" \"1\"
        .vdd.neg power \"GND\" \"2\"
    }}
}}

board B {{
    .process: TwoLayer {{ .stackup.thickness &= 1.6mm }}
    .load: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
        .current &= 0A..3A
    }}
    .source: Rail {{
        .pos.net &= \"5V\"
        .neg.net &= \"GND\"
    }}
    .supply: Header1x2
    .out: Header1x2
    .probe: Header1x2
    .u: Sink
    .supply.p1 ~ .source.pos
    .supply.p2 ~ .source.neg
    .out.p1 ~ .load.pos
    .out.p2 ~ .load.neg
    .probe.p1 ~ .load.pos
    .probe.p2 ~ .load.neg
    .u.vdd ~ .load
{more}}}
"
    )
}

fn design(src: &str) -> Result<netsmith_design::Designed, Vec<String>> {
    let c = check("t.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert!(s.diagnostics.is_empty(), "{:?}", s.diagnostics);
    design_board(&c.boards[0], &s)
}

#[test]
fn a_pin_that_states_its_current_is_a_branch_sized_for_it_and_not_for_its_net() {
    let d = design(&board("    .probe.p1.current &= 1mA\n    .u.vdd.pos.current &= 0mA..20mA\n"))
        .unwrap_or_else(|e| panic!("{e:?}"));
    let f = &d.design.features;
    let net = f.net_currents.iter().find(|c| c.net == "5V").unwrap();
    assert!(net.current >= 3.0, "{net:?}");
    assert!(net.vias > 1, "the net needs vias side by side: {net:?}");
    assert!(net.least_width > 1.0, "{net:?}");
    let process = d.design.process.as_ref().unwrap();
    let paths: Vec<&str> = f.branch_currents.iter().map(|b| b.path.as_str()).collect();
    assert_eq!(paths, ["probe.p1", "u.vdd.pos"]);
    for b in &f.branch_currents {
        assert_eq!(b.net, "5V");
        assert_eq!(b.vias, 1, "{b:?}");
        assert!(b.least_width < process.min_track, "{b:?}");
        assert_eq!(b.width, process.min_track, "drawn at the process's least: {b:?}");
        assert_eq!(b.pins.len(), 1, "{b:?}");
    }
    assert_eq!(f.branch_currents[0].current, 0.001);
    assert_eq!(f.branch_currents[1].current, 0.02);
    assert_eq!(f.branch_currents[0].pins[0].pin, "1");
}

#[test]
fn a_pin_that_states_nothing_is_no_branch_and_the_design_is_as_before() {
    let d = design(&board("")).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(d.design.features.branch_currents.is_empty());
}

#[test]
fn a_branch_stated_below_what_flows_through_its_pin_is_refused_with_the_numbers() {
    let errors = design(&board("    .u.vdd.pos.current &= 1mA\n")).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("`u.vdd.pos.current` says its branch carries at most 1 mA")
            && errors[0].contains("rail `u.vdd` carries up to 20 mA"),
        "{}",
        errors[0]
    );
}

#[test]
fn a_rail_s_min_track_is_the_least_a_branch_is_drawn_at() {
    let d = design(&board(
        "    .probe.p1.current &= 1mA\n    .load.min_track &= 0.3mm..\n",
    ))
    .unwrap_or_else(|e| panic!("{e:?}"));
    let b = &d.design.features.branch_currents[0];
    assert!((b.width - 0.3).abs() < 1e-9, "{b:?}");
}
