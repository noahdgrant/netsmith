//! Input levels in rows by the supply, end to end (issues #1305 and #1307): a board whose
//! receiver states its thresholds in rows builds through the design stage with the level of the
//! row its rail reaches, and the same board on the fixed row's supply is refused with the
//! numbers. `Receiver` and `Host` are invented parts.

use netsmith_check::{Domain, Loader, Member};
use netsmith_solve::Catalogs;

const SRC: &str = r#"
import std::logic::Logic
import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::Rail

/// Invented: an input whose thresholds are a fraction of its supply from 1.1 V to 1.95 V and
/// fixed from 3.0 V to 3.6 V.
part Receiver {
    .designator &= "U"
    .vcc: Rail
    .a: Logic
    .a.reference ~ .vcc
    .a.reference.current &= 0A
    .a.vih &= 0.65 * .vcc.voltage at 1.1V..1.95V, 2.0V at 3.0V..3.6V
    .a.vil &= 0.35 * .vcc.voltage at 1.1V..1.95V, 0.8V at 3.0V..3.6V
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
        .a.line  input "A"   "3"
    }
}

/// Invented: drives within 0.1 V of its supply.
part Host {
    .designator &= "U"
    .vdd: Rail
    .out: Logic
    .out.reference ~ .vdd
    .out.reference.current &= 0A
    .out.voh &= .vdd.voltage - 0.1V
    .out.vol &= 0.1V
    pins {
        .vdd.pos  power  "VDD" "1"
        .vdd.neg  power  "GND" "2"
        .out.line output "OUT" "3"
    }
}

board Link {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .core: Rail {
        .voltage &= 1.8V +/- 5%
        .pos.net &= "1V8"
        .neg.net &= "GND"
    }
    .io: Rail {
        .voltage &= RX_SUPPLY
        .pos.net &= "IO"
        .neg.net &= "GND"
    }
    .power_in: Header1x2
    .core.pos ~ .power_in.p1
    .core.neg ~ .power_in.p2
    .io_in: Header1x2
    .io.pos ~ .io_in.p1
    .io.neg ~ .io_in.p2
    .host: Host
    .rx: Receiver
    .core ~ .host.vdd
    .io ~ .rx.vcc
    .host.out.line ~ .rx.a.line
}
"#;

fn source(supply: &str) -> String {
    SRC.replace("RX_SUPPLY", supply)
}

#[test]
fn a_receiver_in_rows_builds_with_the_level_of_its_row() {
    let src = source("1.8V +/- 5%");
    let outcome = netsmith_pipeline::outcome_with(
        "link.nsm",
        &src,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
    .unwrap_or_else(|p| panic!("{}", netsmith_pipeline::render_problems(&p)));
    let model = &outcome.boards[0].trace.model;
    let Some(Member::Field(f)) = model.lookup("rx.a.vih") else {
        panic!("no level");
    };
    let Domain::Quantity(q) = model.field(f).domain else {
        panic!("the level has no value");
    };
    assert!((q.lo() - 1.1115).abs() < 1e-9 && (q.hi() - 1.2285).abs() < 1e-9, "{q}");
}

#[test]
fn a_receiver_on_the_fixed_rows_supply_is_refused_with_the_numbers() {
    let src = source("3.3V +/- 5%");
    let problems = netsmith_pipeline::outcome_with(
        "link.nsm",
        &src,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
    .err()
    .expect("a 1.8 V high does not reach 2 V");
    let text = netsmith_pipeline::render_problems(&problems);
    assert!(
        text.contains("`host.out.voh` is 1.61V..1.79V but `rx.a.vih` needs 2V"),
        "{text}"
    );
}
