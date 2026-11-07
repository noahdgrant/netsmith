//! The clock of a bus and the mode of an SPI bus (ADR 0064, stage 3): one value along the bus, a
//! bound for each part, a message that names the part that does not allow the clock, and a note
//! for a bus that has limits and no clock.
//!
//! The parts are invented for these tests.

use netsmith_check::{Loader, check};
use netsmith_solve::{Catalogs, Solution, solve};

/// A part with an SPI port bounded by its datasheet, and one with a UART whose baud is.
fn part(name: &str, statements: &str) -> String {
    format!(
        "part {name} {{\n .vdd: Rail\n .spi: Spi\n{statements}\n pins {{\n \
         .vdd.pos power \"VDD\" \"1\"\n .vdd.neg power \"VSS\" \"2\"\n \
         .spi.sclk bidirectional \"SCK\" \"3\"\n .spi.mosi bidirectional \"MOSI\" \"4\"\n \
         .spi.miso bidirectional \"MISO\" \"5\"\n .spi.cs bidirectional \"CS\" \"6\"\n }}\n}}\n"
    )
}

fn solved(parts: &str, body: &str) -> Solution {
    let src = format!(
        "import std::bus::{{Spi, SpiMode, Uart}}\nimport std::sources::Rail\n{parts}\nboard B {{\n \
         .supply: Rail {{\n .voltage &= 3.3V +/- 5%\n .pos.net &= \"3V3\"\n .neg.net &= \"GND\"\n }}\n{body}}}\n"
    );
    let c = check("test.nsm", &src, &Loader::std());
    assert!(c.is_ok(), "{}", c.render());
    solve(&c.boards[0], &Catalogs::jlcpcb())
}

fn faults(s: &Solution) -> Vec<String> {
    s.diagnostics.iter().map(|d| d.message.clone()).collect()
}

fn notes(s: &Solution) -> Vec<String> {
    s.not_evaluated.iter().map(|n| n.message.clone()).collect()
}

fn three_parts() -> String {
    format!(
        "{}{}{}",
        part("Zcpu", " .spi.frequency <= 32MHz"),
        part("Zflash", " .spi.frequency <= 33MHz"),
        part("Zimu", " .spi.frequency <= 10MHz"),
    )
}

/// The board of the three, joined with `~`, and `extra`.
fn board(extra: &str) -> String {
    format!(
        " .cpu: Zcpu\n .flash: Zflash\n .imu: Zimu\n .supply ~ .cpu.vdd\n .supply ~ .flash.vdd\n .supply ~ .imu.vdd\n \
         .cpu.spi ~ .flash.spi\n .cpu.spi ~ .imu.spi\n{extra}\n"
    )
}

#[test]
fn a_clock_above_a_limit_names_the_bus_the_value_the_part_and_what_the_others_allow() {
    let s = solved(&three_parts(), &board(" .cpu.spi.frequency &= 20MHz"));
    assert_eq!(
        faults(&s),
        [
            "bus `cpu.spi` is joined at 20MHz, which `imu` (`Zimu`) does not allow: its limit `imu.spi.frequency` is at most 10MHz; `cpu` (`Zcpu`) allows 32MHz and `flash` (`Zflash`) 33MHz, so the bus runs at 10MHz at most"
        ]
    );
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn the_message_points_at_the_part_that_does_not_allow_the_clock() {
    let src = format!(
        "import std::bus::Spi\nimport std::sources::Rail\n{}\nboard B {{\n .supply: Rail {{\n .voltage &= 3.3V +/- 5%\n .pos.net &= \"3V3\"\n .neg.net &= \"GND\"\n }}\n{}}}\n",
        three_parts(),
        board(" .cpu.spi.frequency &= 20MHz")
    );
    let c = check("test.nsm", &src, &Loader::std());
    let s = solve(&c.boards[0], &Catalogs::jlcpcb());
    let d = &s.diagnostics[0];
    let at = &src[d.span.start..d.span.end];
    assert_eq!(at, ".imu: Zimu");
}

#[test]
fn two_parts_that_do_not_allow_the_clock_are_two_messages() {
    let s = solved(&three_parts(), &board(" .cpu.spi.frequency &= 40MHz"));
    let f = faults(&s);
    assert_eq!(f.len(), 3, "{f:#?}");
    for who in ["`cpu` (`Zcpu`)", "`flash` (`Zflash`)", "`imu` (`Zimu`)"] {
        assert!(
            f.iter()
                .any(|m| m.contains(&format!("which {who} does not allow"))),
            "{f:#?}"
        );
    }
}

#[test]
fn a_clock_inside_every_limit_passes_and_says_nothing() {
    let s = solved(&three_parts(), &board(" .cpu.spi.frequency &= 8MHz"));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(notes(&s), Vec::<String>::new());
}

#[test]
fn a_bus_with_limits_and_no_clock_is_a_note_and_never_a_pass() {
    let s = solved(&three_parts(), &board(""));
    assert_eq!(faults(&s), Vec::<String>::new());
    assert_eq!(
        notes(&s),
        [
            "not evaluated: the clock of bus `cpu.spi` is not stated, so the limits on it are not checked (`cpu` (`Zcpu`) at most 32MHz, `flash` (`Zflash`) 33MHz and `imu` (`Zimu`) 10MHz); state the clock the board runs, `.cpu.spi.frequency &= <the clock>`"
        ]
    );
}

#[test]
fn an_interface_joined_pin_by_pin_holds_the_clock_of_its_bus_all_the_same() {
    // No `~` between the interfaces: the nets show the three are one bus, so the board's clock
    // reaches the IMU and its limit is judged.
    let body = " .cpu: Zcpu\n .imu: Zimu\n .supply ~ .cpu.vdd\n .supply ~ .imu.vdd\n \
         .cpu.spi.sclk ~ .imu.spi.sclk\n .cpu.spi.mosi ~ .imu.spi.mosi\n .cpu.spi.miso ~ .imu.spi.miso\n \
         .cpu.spi.cs ~ .imu.spi.cs\n .cpu.spi.frequency &= 20MHz\n";
    let parts = format!(
        "{}{}",
        part("Zcpu", " .spi.frequency <= 32MHz"),
        part("Zimu", " .spi.frequency <= 10MHz")
    );
    let s = solved(&parts, body);
    assert_eq!(
        faults(&s),
        [
            "bus `cpu.spi` is joined at 20MHz, which `imu` (`Zimu`) does not allow: its limit `imu.spi.frequency` is at most 10MHz; `cpu` (`Zcpu`) allows 32MHz, so the bus runs at 10MHz at most"
        ]
    );
}

#[test]
fn the_order_of_the_statements_changes_nothing() {
    let forward = solved(&three_parts(), &board(" .cpu.spi.frequency &= 20MHz"));
    let first = " .cpu.spi.frequency &= 20MHz\n";
    let backward = solved(&three_parts(), &format!("{first}{}", board("")));
    assert_eq!(faults(&forward), faults(&backward));
}

#[test]
fn a_minimum_clock_is_said_the_same_way() {
    let parts = format!(
        "{}{}",
        part("Zcpu", " .spi.frequency <= 32MHz"),
        part("Zadc", " .spi.frequency >= 4MHz"),
    );
    let body = " .cpu: Zcpu\n .adc: Zadc\n .supply ~ .cpu.vdd\n .supply ~ .adc.vdd\n .cpu.spi ~ .adc.spi\n .cpu.spi.frequency &= 1MHz\n";
    let s = solved(&parts, body);
    assert_eq!(
        faults(&s),
        [
            "bus `cpu.spi` is joined at 1MHz, which `adc` (`Zadc`) does not allow: its limit `adc.spi.frequency` is at least 4MHz"
        ]
    );
}

#[test]
fn the_baud_of_a_uart_is_a_clock_too() {
    let uart = |name: &str, limit: &str| {
        format!(
            "part {name} {{\n .vdd: Rail\n .uart: Uart\n .uart.baud <= {limit}\n pins {{\n \
             .vdd.pos power \"VDD\" \"1\"\n .vdd.neg power \"VSS\" \"2\"\n \
             .uart.tx output \"TX\" \"3\"\n .uart.rx input \"RX\" \"4\"\n }}\n}}\n"
        )
    };
    let parts = format!("{}{}", uart("Za", "1MHz"), uart("Zb", "115kHz"));
    let body = " .a: Za\n .b: Zb\n .supply ~ .a.vdd\n .supply ~ .b.vdd\n .a.uart ~ .b.uart\n .a.uart.baud &= 500kHz\n";
    let s = solved(&parts, body);
    let f = faults(&s);
    assert_eq!(f.len(), 1, "{f:#?}");
    assert!(
        f[0].starts_with("bus `a.uart` is joined at 500kHz, which `b` (`Zb`) does not allow: its limit `b.uart.baud` is at most 115kHz"),
        "{}",
        f[0]
    );
}

// ---- the mode ---------------------------------------------------------------------------

#[test]
fn two_parts_that_support_only_different_modes_are_refused() {
    let parts = format!(
        "{}{}",
        part("Zone", " .spi.mode &= SpiMode::Mode1"),
        part("Zthree", " .spi.mode &= SpiMode::Mode3"),
    );
    let body = " .a: Zone\n .b: Zthree\n .supply ~ .a.vdd\n .supply ~ .b.vdd\n .a.spi ~ .b.spi\n";
    let f = faults(&solved(&parts, body));
    assert_eq!(f.len(), 1, "{f:#?}");
    assert!(f[0].starts_with("connected values disagree: `a.spi.mode` is SpiMode::Mode1 but `b.spi.mode` is SpiMode::Mode3"), "{}", f[0]);
}

#[test]
fn a_pin_by_pin_join_equates_the_mode_all_the_same() {
    let parts = format!(
        "{}{}",
        part("Zone", " .spi.mode &= SpiMode::Mode1"),
        part("Zthree", " .spi.mode &= SpiMode::Mode3"),
    );
    let body = " .a: Zone\n .b: Zthree\n .supply ~ .a.vdd\n .supply ~ .b.vdd\n \
         .a.spi.sclk ~ .b.spi.sclk\n .a.spi.mosi ~ .b.spi.mosi\n .a.spi.miso ~ .b.spi.miso\n .a.spi.cs ~ .b.spi.cs\n";
    let f = faults(&solved(&parts, body));
    assert_eq!(f.len(), 1, "{f:#?}");
    assert!(f[0].starts_with("connected values disagree"), "{}", f[0]);
}

#[test]
fn a_part_with_one_mode_and_a_part_with_several_are_fine() {
    // The part that supports modes 0 and 3 states nothing, and the pair works in the mode of the other.
    let parts = format!(
        "{}{}",
        part("Zone", " .spi.mode &= SpiMode::Mode3"),
        part("Zmany", " // Page 12: modes 0 and 3."),
    );
    let body = " .a: Zone\n .b: Zmany\n .supply ~ .a.vdd\n .supply ~ .b.vdd\n .a.spi ~ .b.spi\n";
    let s = solved(&parts, body);
    assert_eq!(faults(&s), Vec::<String>::new());
    assert!(
        format!("{:?}", s.model.domain_at("b.spi.mode")).contains("Mode3"),
        "the mode reaches the second part"
    );
}

#[test]
fn two_parts_with_the_same_mode_are_fine() {
    let parts = format!(
        "{}{}",
        part("Za", " .spi.mode &= SpiMode::Mode0"),
        part("Zb", " .spi.mode &= SpiMode::Mode0"),
    );
    let body = " .a: Za\n .b: Zb\n .supply ~ .a.vdd\n .supply ~ .b.vdd\n .a.spi ~ .b.spi\n";
    assert_eq!(faults(&solved(&parts, body)), Vec::<String>::new());
}
