//! The crate's seam with KiCad's files: a netlist committed in `fixtures/`, in the XML that KiCad's
//! netlist export writes, read and compared, with no KiCad installed.

use netsmith_netlist::{Netlist, Options, Outcome, compare};

fn led() -> Netlist {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/led.net.xml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    Netlist::from_kicad_xml(&text).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_kicad_netlist_file_is_read_with_its_components_and_nets() {
    let netlist = led();
    let refs: Vec<&str> = netlist
        .components
        .iter()
        .map(|c| c.refdes.as_str())
        .collect();
    assert_eq!(refs, ["J1", "R1", "D1"]);
    let r1 = &netlist.components[netlist.find("R1").unwrap()];
    assert_eq!(r1.value, "330");
    assert_eq!(r1.footprint, "Resistor_SMD:R_0402_1005Metric");
    let names: Vec<&str> = netlist.nets.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["GND", "Net-(D1-A)", "VCC"]);
    // R1's pin 1 is on VCC, with J1's pin 1.
    let vcc = netlist.net_of(netlist.find("R1").unwrap(), "1").unwrap();
    assert_eq!(netlist.nets[vcc].name, "VCC");
    assert_eq!(netlist.nets[vcc].pins.len(), 2);
}

#[test]
fn the_netlist_written_back_as_xml_is_the_same_circuit() {
    let netlist = led();
    let again = Netlist::from_kicad_xml(&netlist.to_kicad_xml("led.kicad_sch")).unwrap();
    match compare(&netlist, &again, &Options::default()).unwrap() {
        Outcome::Match(_) => {}
        other => panic!("not the same circuit: {other:?}"),
    }
}

#[test]
fn a_file_that_is_not_a_kicad_netlist_is_refused_with_the_reason() {
    let e = Netlist::from_kicad_xml("<project/>")
        .unwrap_err()
        .to_string();
    assert!(e.contains("not a KiCad netlist"), "{e}");
    let e = Netlist::from_kicad_xml("not xml").unwrap_err().to_string();
    assert!(e.contains("not XML"), "{e}");
}
