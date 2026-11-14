//! The reference rail of a bus's levels is no rail of the board's power (`std::logic::Levels`
//! states `.reference.current &= 0A`): a part whose `.i2c.levels.reference` nothing joins gets no
//! "has no source" note. A rail with nothing to supply it still has one (an unused port's `vbus`,
//! `crates/netsmith-workspace/tests/unused.rs`). The part is invented.

use netsmith_check::Loader;
use netsmith_solve::Catalogs;

const SRC: &str = r#"
import std::bus::{I2c, I2cPullups, I2cRole}
import std::mfg::jlcpcb::TwoLayer
import std::sources::Rail

/// Zrom, an invented I2C target that states no levels.
part Zrom {
    .vdd: Rail
    .vdd.current &= 0mA..1mA
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    pins {
        .vdd.pos power         "VCC" "8"
        .vdd.neg power         "GND" "4"
        .i2c.sda bidirectional "SDA" "5"
        .i2c.scl input         "SCL" "6"
    }
}

board Bench {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
    }
    .supply: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .rom: Zrom
    .supply ~ .rom.vdd
    .pullups: I2cPullups { .resistance &= 4.7kohm +/- 5% }
    .pullups.supply ~ .supply
    .pullups.bus.levels.reference ~ .supply
    .pullups.bus ~ .rom.i2c
}
"#;

#[test]
fn a_levels_reference_that_nothing_joins_is_no_rail_of_the_power_tree() {
    let outcome = netsmith_pipeline::outcome_with(
        "bench.nsm",
        SRC,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        None,
    )
    .unwrap_or_else(|p| panic!("{}", netsmith_pipeline::render_problems(&p)));
    let notes = outcome.boards[0].power_notes();
    assert!(
        notes.iter().all(|n| !n.contains("levels.reference")),
        "{notes:?}"
    );
}
