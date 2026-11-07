//! The bus clause of a peripheral (ADR 0064, stage 4): `peripheral NAME on CLASS as Spi
//! SpiRole::Master { F:kind as line }`.

use netsmith_lang::ast::*;
use netsmith_lang::parse;

const SPI: &str = "\
part Zmcu {
    peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master {
        SPIM0_SCK:output as sclk
        SPIM0_MOSI:output as mosi
        SPIM0_MISO:input as miso
        SPIM0_DCX
    }
    peripheral TX0 as Uart { TX0_TX:output as tx TX0_RX:input as rx }
    peripheral OTHER on APP_GPIO { OTHER_A:output }
}
";

fn err(src: &str) -> String {
    parse(src).unwrap_err().message
}

fn peripherals(src: &str) -> Vec<Peripheral> {
    parse(src).unwrap().definitions[0]
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Peripheral(p) => Some(p.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_peripheral_may_be_an_end_of_a_bus_with_a_role() {
    let p = peripherals(SPI);
    assert_eq!(p.len(), 3);
    let bus = p[0].bus.as_ref().expect("a bus");
    assert_eq!(bus.interface.segments[0].name, "Spi");
    assert_eq!(bus.role.as_ref().unwrap().to_source(), "SpiRole::Master");
    assert_eq!(p[0].on.as_ref().unwrap().name, "APP_GPIO");
    let lines: Vec<Option<&str>> = p[0]
        .functions
        .iter()
        .map(|f| f.line.as_ref().map(|l| l.name.as_str()))
        .collect();
    assert_eq!(lines, [Some("sclk"), Some("mosi"), Some("miso"), None]);
    assert_eq!(
        p[0].functions[0].kind.map(|(k, _)| k),
        Some(PinKind::Output)
    );
}

#[test]
fn a_bus_with_one_role_has_no_role_and_no_on_is_fine() {
    let p = peripherals(SPI);
    let bus = p[1].bus.as_ref().expect("a bus");
    assert_eq!(bus.interface.segments[0].name, "Uart");
    assert!(bus.role.is_none() && p[1].on.is_none());
    assert!(p[2].bus.is_none());
}

#[test]
fn the_bus_clause_prints_back() {
    let file = parse(SPI).unwrap();
    let printed = file.to_source();
    assert!(
        printed.contains("peripheral SPIM0 on APP_GPIO as Spi SpiRole::Master { SPIM0_SCK:output as sclk SPIM0_MOSI:output as mosi SPIM0_MISO:input as miso SPIM0_DCX }"),
        "{printed}"
    );
    assert!(
        printed.contains("peripheral TX0 as Uart { TX0_TX:output as tx TX0_RX:input as rx }"),
        "{printed}"
    );
    assert_eq!(parse(&printed).unwrap().to_source(), printed);
}

#[test]
fn a_qualified_interface_and_role_parse() {
    let p = peripherals(
        "part P {\n    peripheral X as bus::Spi bus::SpiRole::Slave { X_A:input as sclk }\n}\n",
    );
    let bus = p[0].bus.as_ref().unwrap();
    assert_eq!(bus.interface.segments.len(), 2);
    assert_eq!(
        bus.role.as_ref().unwrap().to_source(),
        "bus::SpiRole::Slave"
    );
}

#[test]
fn as_is_a_name_where_a_name_is_meant() {
    // A function called `as` is not one the clause reads, and a member may be called `as`.
    let file = parse("board B {\n    .as: Rail\n    as.pos ~ x.neg\n}\n").unwrap();
    assert!(matches!(&file.definitions[0].items[0], Item::Decl(_)));
}

#[test]
fn the_clause_is_checked_by_the_parser() {
    assert!(err("part P {\n    peripheral X as { X_A:input }\n}\n").contains("a type name"));
    assert!(err("part P {\n    peripheral X as Spi\n}\n").contains("`{`"));
    assert!(
        err("part P {\n    peripheral X { X_A:input as }\n}\n").contains("the line of the bus")
    );
}
