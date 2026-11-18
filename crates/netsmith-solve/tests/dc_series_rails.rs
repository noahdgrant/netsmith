//! A series element between two rails (issue #1268): a ferrite bead from a regulator's output to
//! a second rail, and a pull-up to an open-drain line fed from either rail. Fed from the far
//! rail, the pull-up's draw reads that rail's voltage, which only the network gives, and the
//! analysis left the regulator's network alone. Every part and number here is invented, but the
//! standard bead, pull-up and regulator.

use netsmith_check::{Domain, Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

fn run(src: &str) -> Solution {
    let checked = check("test.nsm", src, &Loader::std());
    assert!(checked.is_ok(), "{}", checked.render());
    let board = checked.standalone_boards().next().expect("one board");
    solve(board, &Catalogs::jlcpcb())
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

fn messages(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn range(s: &Solution, path: &str) -> (f64, f64) {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => (q.lo(), q.hi()),
        other => panic!("`{path}` is {other:?}"),
    }
}

/// A 5 V input, a regulator that makes 3.3 V on `3V3`, a bead from `3V3` to `3V3F`, an invented
/// gate on each rail (100 uA to 1 mA each), and a pull-up on the far gate's open-drain line, fed
/// from `pull_from`. The gate's line has no model, so the pull-up's resistor ends on a net the
/// analysis does not model.
fn board(pull_from: &str) -> String {
    format!(
        r#"import std::bus::PullUp
import std::parts::passive::FerriteBead
import std::parts::regulator::LinearRegulator
import std::sources::Rail

part Zgate {{
    .designator &= "U"
    .vcc: Rail
    .vcc.current &= 100uA..1mA
    pins {{
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
        .a input "A" "3"
        .b open_drain "B" "4"
    }}
}}

board Bead {{
    .input: Rail {{
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }}
    .ldo: LinearRegulator {{ .vout.voltage &= 3.3V +/- 5% }}
    .v33: Rail {{
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0A
    }}
    .v33f: Rail {{
        .pos.net &= "3V3F"
        .neg.net &= "GND"
        .current &= 0A
    }}
    .bead: FerriteBead {{ .impedance &= 600ohm +/- 25%; .impedance_frequency &= 100MHz }}
    .input ~ .ldo.vin
    .ldo.vout ~ .v33
    .v33.pos ~> .bead ~> .v33f.pos
    .near: Zgate
    .near.vcc ~ .v33
    .far: Zgate
    .far.vcc ~ .v33f
    .pull: PullUp {{ .resistor.resistance &= 10kohm +/- 5% }}
    .pull.supply ~ {pull_from}
    .pull.line ~ .far.b
    .far.a ~ .far.b
    .near.a ~ .far.b
}}
"#
    )
}

/// Fed from the near rail, as before: the regulator supplies both gates, the bead's current and
/// the pull-up.
#[test]
fn a_pullup_fed_in_front_of_the_bead_is_solved() {
    let s = run(&board(".v33"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(notes(&s).is_empty(), "{:?}", notes(&s));
    let bead = range(&s, "bead.current");
    assert!(
        (bead.0 - 100e-6).abs() < 1e-9 && (bead.1 - 1e-3).abs() < 1e-9,
        "{bead:?}"
    );
}

/// Reproduced: fed from the far rail, the analysis left the regulator's network alone (`the DC
/// analysis left the network of `ldo` alone`), and nothing in front of the bead was judged. The
/// far rail now has the near side's voltage less the bead's drop, and the pull-up's draw is in
/// the bead's current and the regulator's load.
#[test]
fn a_pullup_fed_behind_the_bead_is_solved() {
    let s = run(&board(".v33f"));
    assert!(messages(&s).is_empty(), "{:?}", messages(&s));
    assert!(notes(&s).is_empty(), "{:?}", notes(&s));

    // The far rail: below the near one by what the bead drops, never above it.
    let near = range(&s, "v33.voltage");
    let far = range(&s, "v33f.voltage");
    let drop = range(&s, "bead.drop");
    assert!(drop.0 >= -1e-9 && drop.1 > 0.0, "{drop:?}");
    assert!(
        far.1 <= near.1 + 1e-6 && far.0 < near.0 && far.0 >= near.0 - drop.1 - 1e-6,
        "far {far:?}, near {near:?}, drop {drop:?}"
    );

    // The bead carries the far gate and the pull-up, at most what the pull-up can draw at the
    // top of the far rail (and no more than at the top of the near one).
    let pull = range(&s, "pull.supply.current");
    let resistance = range(&s, "pull.resistor.resistance");
    assert!(
        pull.0.abs() < 1e-12 && pull.1 > 0.0 && pull.1 <= far.1 / resistance.0 * (1.0 + 1e-6),
        "{pull:?} against {far:?} over {resistance:?}"
    );
    let bead = range(&s, "bead.current");
    let most = 1e-3 + near.1 / resistance.0;
    assert!(
        (bead.0 - 100e-6).abs() < 1e-9 && bead.1 >= 1e-3 + pull.1 - 1e-9 && bead.1 <= most + 1e-9,
        "bead {bead:?}, pull {pull:?}"
    );

    // The regulator supplies the near gate and all the bead carries.
    let ldo = range(&s, "ldo.vout.current");
    assert!(
        (ldo.1 + 200e-6).abs() < 1e-9 && ldo.0 <= -(1e-3 + bead.1) + 1e-9,
        "regulator {ldo:?}, bead {bead:?}"
    );
}
