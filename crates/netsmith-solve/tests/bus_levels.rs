//! The logic levels of a bus (ADR 0064, stage 2): `.levels` of a bus interface, judged as the
//! levels of a `Logic` line are (ADR 0046), with a translator as a part that has two buses.
//!
//! The parts are invented for these tests; none has a datasheet.

use netsmith_check::{Domain, Loader, check};
use netsmith_quantity::Quantity;
use netsmith_solve::{Catalogs, Solution, solve};

const PARTS: &str = r#"
import std::bus::{Spi, SpiRole}
import std::sources::Rail

/// A 3.3 V SPI master whose levels follow its supply.
part Zm {
    .vdd: Rail
    .spi: Spi
    .spi.role &= SpiRole::Master
    .spi.levels.reference ~ .vdd
    .spi.levels.vih &= 0.7 * .vdd.voltage
    .spi.levels.vil &= 0.3 * .vdd.voltage
    .spi.levels.voh &= .vdd.voltage - 0.4V
    .spi.levels.vol &= 0.4V
    pins {
        .vdd.pos  power         "VDD"  "1"
        .vdd.neg  power         "VSS"  "2"
        .spi.sclk output        "SCK"  "3"
        .spi.mosi output        "MOSI" "4"
        .spi.miso input         "MISO" "5"
        .spi.cs   output        "CS"   "6"
        .sel1     bidirectional "IO7"  "7"
    }
}

/// A slave that joins its reference rail and states none of the four levels.
part Zbare {
    .vdd: Rail
    .spi: Spi
    .spi.role &= SpiRole::Slave
    .spi.levels.reference ~ .vdd
    pins {
        .vdd.pos   power     "VDD"   "8"
        .vdd.neg   power     "GND"   "6"
        .spi.sclk  input     "SCL"   "1"
        .spi.mosi  input     "SDI"   "2"
        .spi.miso  tri_state "SDO"   "3"
        .spi.cs    input     "CS"    "4"
    }
}

/// A part from before roles and levels: no role, no levels.
part Zplain {
    .vdd: Rail
    .spi: Spi
    pins {
        .vdd.pos   power         "VDD"   "8"
        .vdd.neg   power         "GND"   "6"
        .spi.sclk  bidirectional "SCL"   "1"
        .spi.mosi  bidirectional "SDI"   "2"
        .spi.miso  bidirectional "SDO"   "3"
        .spi.cs    bidirectional "CS"    "4"
    }
}

/// A translator for SPI: the master side `a` on `vcca` and the sensor side `b` on `vccb`.
part Zshift {
    .vcca: Rail
    .vccb: Rail
    .vccb.neg ~ .vcca.neg
    .a: Spi
    .b: Spi
    .a.role &= SpiRole::Slave
    .b.role &= SpiRole::Master
    .a.levels.reference ~ .vcca
    .a.levels.vih &= 0.7 * .vcca.voltage
    .a.levels.vil &= 0.3 * .vcca.voltage
    .a.levels.voh &= .vcca.voltage - 0.2V
    .a.levels.vol &= 0.2V
    .b.levels.reference ~ .vccb
    .b.levels.vih &= 0.7 * .vccb.voltage
    .b.levels.vil &= 0.3 * .vccb.voltage
    .b.levels.voh &= .vccb.voltage - 0.2V
    .b.levels.vol &= 0.2V
    .a.sclk.abs_max &= 3.6V
    .a.mosi.abs_max &= 3.6V
    .a.cs.abs_max &= 3.6V
    .b.miso.abs_max &= 2.1V
    pins {
        .vcca.pos  power     "VCCA" "1"
        .vccb.pos  power     "VCCB" "2"
        .vcca.neg  power     "GND"  "3"
        .a.sclk    input     "A1"   "4"
        .a.mosi    input     "A2"   "5"
        .a.miso    tri_state "A3"   "6"
        .a.cs      input     "A4"   "7"
        .b.sclk    output    "B1"   "8"
        .b.mosi    output    "B2"   "9"
        .b.miso    input     "B3"   "10"
        .b.cs      output    "B4"   "11"
    }
}
"#;

/// A slave with its core on `vdd` and its pins on `vddio`; an input of it survives `abs`.
fn slave(name: &str, abs: &str) -> String {
    format!(
        "part {name} {{\n .vdd: Rail\n .vddio: Rail\n .vddio.neg ~ .vdd.neg\n .spi: Spi\n \
         .spi.role &= SpiRole::Slave\n .spi.levels.reference ~ .vddio\n \
         .spi.levels.vih &= 0.7 * .vddio.voltage\n .spi.levels.vil &= 0.3 * .vddio.voltage\n \
         .spi.levels.voh &= .vddio.voltage - 0.2V\n .spi.levels.vol &= 0.2V\n \
         .spi.sclk.abs_max &= {abs}\n .spi.mosi.abs_max &= {abs}\n .spi.cs.abs_max &= {abs}\n \
         pins {{\n .vdd.pos power \"VDD\" \"8\"\n .vddio.pos power \"VDDIO\" \"5\"\n \
         .vdd.neg power \"GND\" \"6\" \"7\"\n .spi.sclk input \"SCL\" \"1\"\n \
         .spi.mosi input \"SDI\" \"2\"\n .spi.miso tri_state \"SDO\" \"3\"\n \
         .spi.cs input \"CS\" \"4\"\n }}\n}}\n"
    )
}

const S3: &str = "    .s3: Rail {\n        .voltage &= 3.3V +/- 5%\n        .pos.net &= \"3V3\"\n        .neg.net &= \"GND\"\n    }\n";
const S18: &str = "    .s18: Rail {\n        .voltage &= 1.8V +/- 5%\n        .pos.net &= \"1V8\"\n        .neg.net &= \"GND\"\n    }\n";

fn solved(body: &str) -> Solution {
    // `Zs` is a 1.8 V part (an input survives 2.1 V) and `Zs33` the same part on 3.3 V (3.6 V).
    let c = check(
        "test.nsm",
        &format!(
            "{PARTS}\n{}{}\nboard B {{\n{body}}}\n",
            slave("Zs", "2.1V"),
            slave("Zs33", "3.6V")
        ),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn faults(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

/// The processor on 3.3 V, the slave's pins on the rail `io`, the slave's core on 3.3 V, the
/// bus joined as the guide joins it, and the slave's select on a pin of the processor.
fn board(io: &str) -> String {
    let ty = if io == "s3" { "Zs33" } else { "Zs" };
    format!(
        "{S3}{S18}    .m: Zm\n    .s: {ty}\n    .s3 ~ .m.vdd\n    .s3 ~ .s.vdd\n    .{io} ~ .s.vddio\n    .m.spi.sclk ~ .s.spi.sclk\n    .m.spi.mosi ~ .s.spi.mosi\n    .m.spi.miso ~ .s.spi.miso\n    .m.spi.cs ~ .s.spi.cs\n"
    )
}

#[test]
fn a_bus_whose_parts_share_a_level_passes_with_nothing_said() {
    let s = solved(&board("s3"));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_3v3_line_onto_a_1v8_pin_is_refused_with_the_numbers() {
    let s = solved(&board("s18"));
    let f = faults(&s);
    // `sclk`, `mosi` and `cs` are inputs that state an absolute maximum; each is a fault.
    for line in ["sclk", "mosi", "cs"] {
        let want = format!(
            "net `net-m.spi.{line}` reaches 3.465V above its ground `GND` (`m.spi.{line}`, a line of `m.spi.levels.reference`) but `s.spi.{line}.abs_max` is 2.1V: it must not exceed 2.1V"
        );
        assert!(f.contains(&want), "{want}\nnot in {f:#?}");
    }
}

#[test]
fn the_way_back_is_judged_too_a_1v8_high_is_not_a_3v3_high() {
    let s = solved(&board("s18"));
    let f = faults(&s);
    let want = "net `net-m.spi.miso`: `s.spi.levels.voh` is 1.51V..1.69V but `m.spi.levels.vih` needs 2.194V..2.426V: a high of 1.51V is not always read as a high, which takes 2.426V";
    assert!(f.contains(&want.to_string()), "{f:#?}");
}

#[test]
fn a_translator_is_a_part_with_a_bus_on_each_supply_and_neither_message_appears() {
    let body = format!(
        "{S3}{S18}    .m: Zm\n    .t: Zshift\n    .s: Zs\n    .s3 ~ .m.vdd\n    .s3 ~ .t.vcca\n    .s18 ~ .t.vccb\n    .s3 ~ .s.vdd\n    .s18 ~ .s.vddio\n    .m.spi.sclk ~ .t.a.sclk\n    .m.spi.mosi ~ .t.a.mosi\n    .m.spi.miso ~ .t.a.miso\n    .m.spi.cs ~ .t.a.cs\n    .t.b.sclk ~ .s.spi.sclk\n    .t.b.mosi ~ .s.spi.mosi\n    .t.b.miso ~ .s.spi.miso\n    .t.b.cs ~ .s.spi.cs\n"
    );
    let s = solved(&body);
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_part_with_two_supplies_for_two_groups_of_pins_states_two_interfaces() {
    // The translator above is that part: `a` on `vcca` and `b` on `vccb`. Join `a` to a slave
    // that is on 1.8 V, and it is `a`'s levels that are judged: the 3.3 V line is its.
    let body = format!(
        "{S3}{S18}    .m: Zm\n    .t: Zshift\n    .s3 ~ .m.vdd\n    .s3 ~ .t.vcca\n    .s3 ~ .t.vccb\n    .m.spi.sclk ~ .t.a.sclk\n    .m.spi.mosi ~ .t.a.mosi\n    .m.spi.miso ~ .t.a.miso\n    .m.spi.cs ~ .t.a.cs\n"
    );
    let s = solved(&body);
    assert_eq!(faults(&s), Vec::<String>::new());
}

#[test]
fn a_pin_of_a_part_that_states_no_levels_is_a_note_and_never_a_pass() {
    let body = format!(
        "{S3}    .m: Zm\n    .b: Zbare\n    .s3 ~ .m.vdd\n    .s3 ~ .b.vdd\n    .m.spi.sclk ~ .b.spi.sclk\n    .m.spi.mosi ~ .b.spi.mosi\n    .m.spi.miso ~ .b.spi.miso\n    .m.spi.cs ~ .b.spi.cs\n"
    );
    let s = solved(&body);
    assert_eq!(faults(&s), Vec::<String>::new());
    let n = notes(&s);
    assert!(
        n.iter().any(|n| n.starts_with(
            "not evaluated: the high level from `m.spi.levels` to `b.spi.levels` on net"
        ) && n.ends_with("`b.spi.levels.vih` has no value")),
        "{n:#?}"
    );
}

#[test]
fn two_parts_that_state_no_level_at_all_are_not_looked_at() {
    let body = format!(
        "{S3}    .a: Zplain\n    .b: Zplain\n    .s3 ~ .a.vdd\n    .s3 ~ .b.vdd\n    .a.spi ~ .b.spi\n"
    );
    let s = solved(&body);
    let n = notes(&s);
    assert!(n.iter().all(|n| !n.contains("level")), "{n:#?}");
}

#[test]
fn a_plain_pin_that_drives_a_bus_net_is_a_note_that_says_it_is_not_a_bus_pin() {
    // The IMU's select on the processor's `sel1`, a pin that is not the line of anything.
    let body = format!(
        "{S3}{S18}    .m: Zm\n    .s: Zs\n    .s3 ~ .m.vdd\n    .s3 ~ .s.vdd\n    .s18 ~ .s.vddio\n    .m.spi.sclk ~ .s.spi.sclk\n    .m.spi.mosi ~ .s.spi.mosi\n    .m.spi.miso ~ .s.spi.miso\n    .m.sel1 ~ .s.spi.cs\n"
    );
    let s = solved(&body);
    let n = notes(&s);
    assert!(
        n.contains(&"not evaluated: the absolute maximum of `s.spi.cs` against net `net-m.sel1`: `m.sel1` can drive it and is not the `line` of a `Logic` or a pin of an interface with `levels`, so how far it swings is not known".to_string()),
        "{n:#?}"
    );
}

#[test]
fn joining_two_bus_interfaces_with_a_tilde_joins_no_levels_and_no_rails() {
    // A master on 3.3 V and a slave whose pins are on 1.8 V, joined as whole interfaces: the
    // reference rails stay two rails on two nets (a join of the levels would short them).
    let body = format!(
        "{S3}{S18}    .m: Zm\n    .s: Zs\n    .s3 ~ .m.vdd\n    .s3 ~ .s.vdd\n    .s18 ~ .s.vddio\n    .m.spi ~ .s.spi\n"
    );
    let s = solved(&body);
    let rail = |path: &str| {
        s.model
            .domain_at(path)
            .map(|d| format!("{d:?}"))
            .unwrap_or_default()
    };
    assert!(
        rail("m.spi.levels.reference.voltage").contains("3.3")
            || rail("m.spi.levels.reference.voltage").contains("3.1"),
        "{}",
        rail("m.spi.levels.reference.voltage")
    );
    assert!(
        rail("s.spi.levels.reference.voltage").contains("1.8")
            || rail("s.spi.levels.reference.voltage").contains("1.7"),
        "{}",
        rail("s.spi.levels.reference.voltage")
    );
    // And the levels are each part's own, so the slave's `vih` is not the master's.
    assert_ne!(rail("m.spi.levels.vih"), rail("s.spi.levels.vih"));
}

#[test]
fn a_tri_state_bus_line_is_judged_as_a_driver() {
    // It is `miso`, the slave's `tri_state` line, that fails on the way back (above), so the
    // level rule reads the kind `tri_state` as a driver for a bus pin as for a `Logic` line.
    let s = solved(&board("s18"));
    assert!(
        faults(&s).iter().any(|f| f.contains("`s.spi.levels.voh`")),
        "{:#?}",
        faults(&s)
    );
}

/// A listener that joins its reference rail to its supply and states no current for either
/// (the draw is `.vdd.current`, the board's to state), on a 3.3 V rail.
const LISTENER: &str = r#"
import std::bus::Spi
import std::sources::Rail

part Zlisten {
    .designator &= "U"
    .vdd: Rail
    .spi: Spi
    .spi.levels.reference ~ .vdd
    .spi.levels.vih &= 0.7 * .vdd.voltage
    pins {
        .vdd.pos  power "VDD"  "1"
        .vdd.neg  power "VSS"  "2"
        .spi.sclk no_connect "SCK"  "3"
        .spi.mosi no_connect "MOSI" "4"
        .spi.miso no_connect "MISO" "5"
        .spi.cs   no_connect "CS"   "6"
    }
}

board Bench {
    .s3: Rail {
        .voltage &= 3.3V +/- 5%
        .pos.net &= "3V3"
        .neg.net &= "GND"
    }
    .l: Zlisten
    .s3 ~ .l.vdd
    .l.vdd.current &= 1mA..2mA
}
"#;

fn current(s: &Solution, path: &str) -> Quantity {
    match s.model.domain_at(path) {
        Some(Domain::Quantity(q)) => *q,
        other => panic!("`{path}` is {other:?}"),
    }
}

fn listener(src: &str) -> Solution {
    let c = check("test.nsm", src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

#[test]
fn a_levels_reference_joined_to_a_supply_is_a_reference_and_draws_nothing() {
    // The part states no current for the reference: `Levels` does (ADR 0064 amendment), so
    // the net of the supply has one unknown term, the supply's, and it is worked out.
    let s = listener(LISTENER);
    assert_eq!(faults(&s), Vec::<String>::new());
    let reference = current(&s, "l.spi.levels.reference.current");
    assert!(
        reference.lo() == 0.0 && reference.hi() == 0.0,
        "{reference}"
    );
    let supply = current(&s, "s3.current");
    assert!(
        (supply.lo() + 2e-3).abs() < 1e-12 && (supply.hi() + 1e-3).abs() < 1e-12,
        "{supply}"
    );
}

#[test]
fn a_part_that_states_the_reference_current_itself_still_does() {
    // The statement every part had to make before is the same value and agrees with `Levels`.
    let src = LISTENER.replace(
        "    .spi.levels.vih &=",
        "    .spi.levels.reference.current &= 0A\n    .spi.levels.vih &=",
    );
    let s = listener(&src);
    assert_eq!(faults(&s), Vec::<String>::new());
    let supply = current(&s, "s3.current");
    assert!(
        (supply.lo() + 2e-3).abs() < 1e-12 && (supply.hi() + 1e-3).abs() < 1e-12,
        "{supply}"
    );
}

#[test]
fn a_current_stated_for_a_reference_is_refused_nothing_is_drawn_through_it() {
    let src = LISTENER.replace(
        "    .spi.levels.vih &=",
        "    .spi.levels.reference.current &= 5mA\n    .spi.levels.vih &=",
    );
    let c = check("test.nsm", &src, &Loader::std());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    let said = format!("{}\n{}", c.render(), faults(&s).join("\n"));
    assert!(said.contains("reference.current"), "{said}");
}

// ---- an open-drain line: the pull-up's rail is the high level (ADR 0075) ------------------

/// Invented I2C parts: pins that are open-drain by their datasheet, which states a low level
/// (VOL) and the thresholds of a reader (VIH, VIL) and no VOH. `Zpush` states a VOH too.
const I2C_PARTS: &str = r#"
import std::bus::{I2c, I2cRole, I2cPullups}
import std::sources::Rail

/// A controller on its supply: VOL and the thresholds, no VOH.
part Zhost {
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Controller
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "VSS" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  bidirectional "SCL" "4"
    }
}

/// A target whose datasheet gives VOL and no VOH, because the pull-up sets the high.
part Ztarget {
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.vol &= 0.4V
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}

/// A target that states no VOL: it says nothing about its low.
part Znolow {
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}

/// A target whose line is push-pull to its own supply: it states VOH.
part Zpush {
    .vdd: Rail
    .i2c: I2c
    .i2c.role &= I2cRole::Target
    .i2c.levels.reference ~ .vdd
    .i2c.levels.vih &= 0.7 * .vdd.voltage
    .i2c.levels.vil &= 0.3 * .vdd.voltage
    .i2c.levels.voh &= .vdd.voltage - 0.4V
    .i2c.levels.vol &= 0.4V
    pins {
        .vdd.pos  power         "VDD" "1"
        .vdd.neg  power         "GND" "2"
        .i2c.sda  bidirectional "SDA" "3"
        .i2c.scl  input         "SCL" "4"
    }
}
"#;

fn solved_i2c(body: &str) -> Solution {
    let c = check(
        "test.nsm",
        &format!("{I2C_PARTS}\nboard B {{\n{body}}}\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

/// A controller and a target on 3.3 V, the bus pulled up to `pull` (`s3` or `s18`).
fn i2c_board(target: &str, pull: &str) -> String {
    format!(
        "{S3}{S18}    .m: Zhost\n    .t: {target}\n    .s3 ~ .m.vdd\n    .s3 ~ .t.vdd\n    .m.i2c ~ .t.i2c\n    .pullups: I2cPullups {{ .resistance &= 4.7kohm +/- 5% }}\n    .{pull} ~ .pullups.supply\n    .m.i2c ~ .pullups.bus\n"
    )
}

fn high_notes(s: &Solution) -> Vec<String> {
    notes(s)
        .into_iter()
        .filter(|n| n.contains("the high level"))
        .collect()
}

#[test]
fn an_open_drain_bus_with_a_pull_up_and_parts_that_state_vol_and_vih_has_no_note() {
    // Before ADR 0075 each ordered pair of devices on each line was a note, `voh` has no value.
    let s = solved_i2c(&i2c_board("Ztarget", "s3"));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_pull_up_to_a_rail_that_sets_a_low_high_is_refused_with_the_numbers() {
    // The bus is pulled up to 1.8 V +/- 5 % and the target reads a high from 70 % of 3.3 V.
    let s = solved_i2c(&i2c_board("Ztarget", "s18"));
    let f = faults(&s);
    let want = "net `net-m.i2c.sda`: the pull-up `pullups.sda_pullup` to rail `s18` holds the line at 1.71V..1.89V (a driver that states `vol` and no `voh` leaves the high to the pull-up) but `t.i2c.levels.vih` needs 2.194V..2.426V: a high of 1.71V is not always read as a high, which takes 2.426V";
    assert!(f.contains(&want.to_string()), "{want}\nnot in {f:#?}");
    // Only the pull-up is at fault: no `voh` is blamed, and nothing is left unevaluated.
    assert!(f.iter().all(|m| !m.contains(".voh` is")), "{f:#?}");
    assert!(high_notes(&s).is_empty(), "{:#?}", notes(&s));
}

#[test]
fn a_driver_that_states_no_vol_still_gets_its_note() {
    let s = solved_i2c(&i2c_board("Znolow", "s3"));
    assert_eq!(faults(&s), Vec::<String>::new());
    let n = notes(&s);
    // Its high and its low are both unknown: the pull-up does not stand in for a part that says
    // nothing about its output.
    for want in [
        "not evaluated: the high level from `t.i2c.levels` to `m.i2c.levels` on net `net-m.i2c.sda`: `t.i2c.levels.voh` has no value",
        "not evaluated: the low level from `t.i2c.levels` to `m.i2c.levels` on net `net-m.i2c.sda`: `t.i2c.levels.vol` has no value",
    ] {
        assert!(n.contains(&want.to_string()), "{want}\nnot in {n:#?}");
    }
    // The controller states VOL, so its high is the pull-up's and has no note.
    assert!(
        n.iter().all(|m| !m.contains("from `m.i2c.levels`")),
        "{n:#?}"
    );
}

#[test]
fn a_driver_that_states_voh_is_checked_as_before_whatever_pulls_the_line_up() {
    // The target's own supply is 1.8 V and its line is push-pull to it: its VOH of 1.4 V is not
    // a high for the controller (VIH 70 % of 3.3 V), though the pull-up is at 3.3 V.
    let body = format!(
        "{S3}{S18}    .m: Zhost\n    .t: Zpush\n    .s3 ~ .m.vdd\n    .s18 ~ .t.vdd\n    .m.i2c ~ .t.i2c\n    .pullups: I2cPullups {{ .resistance &= 4.7kohm +/- 5% }}\n    .s3 ~ .pullups.supply\n    .m.i2c ~ .pullups.bus\n"
    );
    let s = solved_i2c(&body);
    let f = faults(&s);
    let want = "net `net-m.i2c.sda`: `t.i2c.levels.voh` is 1.31V..1.49V but `m.i2c.levels.vih` needs 2.194V..2.426V: a high of 1.31V is not always read as a high, which takes 2.426V";
    assert!(f.contains(&want.to_string()), "{f:#?}");
    // The controller states no `voh` and the 3.3 V pull-up is the high it gives: that way passes.
    assert!(f.iter().all(|m| !m.contains("the pull-up")), "{f:#?}");
}

#[test]
fn a_plain_resistor_to_a_rail_is_a_pull_up_too() {
    let body = format!(
        "{S3}    .m: Zhost\n    .t: Ztarget\n    .rs: Resistor\n    .rc: Resistor\n    .rs.resistance &= 4.7kohm +/- 5%\n    .rc.resistance &= 4.7kohm +/- 5%\n    .s3 ~ .m.vdd\n    .s3 ~ .t.vdd\n    .m.i2c ~ .t.i2c\n    .s3.pos ~> .rs ~> .m.i2c.sda\n    .s3.pos ~> .rc ~> .m.i2c.scl\n"
    );
    let c = check(
        "test.nsm",
        &format!("import std::parts::passive::Resistor\n{I2C_PARTS}\nboard B {{\n{body}}}\n"),
        &Loader::std(),
    );
    assert!(c.is_ok(), "{}", c.render());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    assert_eq!(faults(&s), Vec::<String>::new());
    assert!(high_notes(&s).is_empty(), "{:#?}", notes(&s));
}
