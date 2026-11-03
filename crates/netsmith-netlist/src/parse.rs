//! Reading a netlist from KiCad's files.

use std::path::Path;

use netsmith_fab::sexp::{self, Sexp};
use netsmith_fab::{CliError, KicadCli};

use crate::model::{Netlist, NetlistError};

impl Netlist {
    /// Read the XML `kicad-cli sch export netlist --format kicadxml` writes.
    pub fn from_kicad_xml(text: &str) -> Result<Netlist, NetlistError> {
        let format = |m: String| NetlistError::Format(m);
        let doc = roxmltree::Document::parse(text)
            .map_err(|e| format(format!("the netlist is not XML: {e}")))?;
        let root = doc.root_element();
        if root.tag_name().name() != "export" {
            return Err(format(format!(
                "the XML is not a KiCad netlist: its root is <{}>, not <export>",
                root.tag_name().name()
            )));
        }
        let child = |n: roxmltree::Node<'_, '_>, name: &str| {
            n.children()
                .find(|c| c.has_tag_name(name))
                .map(|c| c.text().unwrap_or("").trim().to_string())
        };
        let mut components = Vec::new();
        for comp in root
            .children()
            .filter(|n| n.has_tag_name("components"))
            .flat_map(|c| c.children().filter(|n| n.has_tag_name("comp")))
        {
            let refdes = comp
                .attribute("ref")
                .ok_or_else(|| format("a <comp> in the netlist has no ref".into()))?;
            components.push((
                refdes.to_string(),
                child(comp, "value").unwrap_or_default(),
                child(comp, "footprint").unwrap_or_default(),
            ));
        }
        let mut nets = Vec::new();
        for net in root
            .children()
            .filter(|n| n.has_tag_name("nets"))
            .flat_map(|c| c.children().filter(|n| n.has_tag_name("net")))
        {
            let name = net.attribute("name").unwrap_or_default().to_string();
            let mut pins = Vec::new();
            for node in net.children().filter(|n| n.has_tag_name("node")) {
                let (Some(r), Some(p)) = (node.attribute("ref"), node.attribute("pin")) else {
                    return Err(format(format!("a <node> of net {name} has no ref or pin")));
                };
                pins.push((r.to_string(), p.to_string()));
            }
            nets.push((name, pins));
        }
        Netlist::from_parts(components, nets)
    }

    /// Read the footprints and pad nets of a `.kicad_pcb`. A footprint that is only on the
    /// board (`board_only`), and a pad with no number, are not part of the circuit.
    pub fn from_kicad_pcb(text: &str) -> Result<Netlist, NetlistError> {
        let root = sexp::parse(text)
            .map_err(|e| NetlistError::Format(format!("cannot read the .kicad_pcb file: {e}")))?;
        if root.head() != Some("kicad_pcb") {
            return Err(NetlistError::Format(
                "the file is not a KiCad board: it has no `(kicad_pcb` list".into(),
            ));
        }
        let mut components = Vec::new();
        let mut nets: Vec<(String, Vec<(String, String)>)> = Vec::new();
        let mut net_index = std::collections::HashMap::new();
        for fp in root.children("footprint") {
            let mut reference = None;
            let mut value = String::new();
            for p in fp.children("property") {
                let text = |i: usize| p.items().get(i).and_then(Sexp::text).unwrap_or("");
                match text(1) {
                    "Reference" => reference = Some(text(2).to_string()),
                    "Value" => value = text(2).to_string(),
                    _ => {}
                }
            }
            let board_only = fp.child("attr").is_some_and(|a| {
                a.items()
                    .iter()
                    .skip(1)
                    .any(|i| i.text() == Some("board_only"))
            });
            let Some(refdes) = reference.filter(|r| !r.is_empty() && !board_only) else {
                continue;
            };
            let footprint = fp.items().get(1).and_then(Sexp::text).unwrap_or("");
            components.push((refdes.clone(), value, footprint.to_string()));
            for pad in fp.children("pad") {
                let number = pad.items().get(1).and_then(Sexp::text).unwrap_or("");
                // `(net 3 "GND")`, or `(net "GND")` in later versions: the name is last.
                let net = pad
                    .child("net")
                    .and_then(|n| n.items().last())
                    .and_then(Sexp::text)
                    .filter(|n| !n.is_empty());
                if let (false, Some(net)) = (number.is_empty(), net) {
                    let i = *net_index.entry(net.to_string()).or_insert_with(|| {
                        nets.push((net.to_string(), Vec::new()));
                        nets.len() - 1
                    });
                    nets[i].1.push((refdes.clone(), number.to_string()));
                }
            }
        }
        Netlist::from_parts(components, nets)
    }

    /// Read the netlist of a schematic, which KiCad exports.
    pub fn from_kicad_schematic(sch: &Path, kicad: &dyn KicadCli) -> Result<Netlist, String> {
        let xml = kicad.netlist(sch).map_err(|e: CliError| e.to_string())?;
        Netlist::from_kicad_xml(&xml).map_err(|e| format!("{}: {e}", sch.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<export version="E">
  <design><source>x.kicad_sch</source></design>
  <components>
    <comp ref="R1"><value>330</value><footprint>Resistor_SMD:R_0402_1005Metric</footprint></comp>
    <comp ref="D1"><value>LED</value><footprint>LED_SMD:LED_0603_1608Metric</footprint></comp>
    <comp ref="#PWR01"><value>GND</value></comp>
  </components>
  <nets>
    <net code="1" name="GND"><node ref="D1" pin="2"/><node ref="#PWR01" pin="1"/></net>
    <net code="2" name="Net-(D1-A)"><node ref="D1" pin="1"/><node ref="R1" pin="2"/></net>
    <net code="3" name="unconnected-(R1-Pad1)"><node ref="R1" pin="1"/></net>
  </nets>
</export>"##;

    #[test]
    fn xml_gives_components_nets_and_pins() {
        let n = Netlist::from_kicad_xml(XML).unwrap();
        assert_eq!(n.components.len(), 2, "the power port is not a part");
        assert_eq!(n.components[0].value, "330");
        assert_eq!(n.nets.len(), 2, "an unconnected pin is on no net");
        let gnd = &n.nets[0];
        assert_eq!(gnd.name, "GND");
        assert_eq!(gnd.pins, [(1, "2".to_string())], "#PWR01.1 is dropped");
        assert_eq!(n.net_of(0, "1"), None);
        assert_eq!(n.net_of(0, "2"), Some(1));
    }

    #[test]
    fn xml_that_is_not_a_netlist_is_refused_by_name() {
        let e = Netlist::from_kicad_xml("<foo/>").unwrap_err().to_string();
        assert!(e.contains("<foo>") && e.contains("<export>"), "{e}");
        let e = Netlist::from_kicad_xml("not xml").unwrap_err().to_string();
        assert!(e.contains("not XML"), "{e}");
    }

    #[test]
    fn a_net_naming_a_missing_component_or_a_pin_on_two_nets_is_refused() {
        let e = Netlist::from_kicad_xml(
            r#"<export><components><comp ref="R1"/></components>
               <nets><net name="A"><node ref="R9" pin="1"/></net></nets></export>"#,
        )
        .unwrap_err();
        assert_eq!(
            e,
            NetlistError::UnknownRefdes {
                net: "A".into(),
                refdes: "R9".into()
            }
        );
        let e = Netlist::from_kicad_xml(
            r#"<export><components><comp ref="R1"/></components>
               <nets><net name="A"><node ref="R1" pin="1"/></net>
                     <net name="B"><node ref="R1" pin="1"/></net></nets></export>"#,
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("R1.1") && e.contains("A and B"), "{e}");
    }

    #[test]
    fn a_board_gives_footprints_and_the_nets_of_their_pads() {
        let pcb = r#"(kicad_pcb (version 20241229)
  (net 0 "") (net 1 "GND") (net 2 "N1")
  (footprint "Resistor_SMD:R_0402_1005Metric" (layer "F.Cu")
    (property "Reference" "R1" (at 0 0 0))
    (property "Value" "330" (at 0 0 0))
    (pad "1" smd roundrect (at 0 0) (net 2 "N1"))
    (pad "2" smd roundrect (at 1 0) (net 1 "GND")))
  (footprint "MountingHole:MountingHole_3.2mm" (layer "F.Cu")
    (property "Reference" "H1" (at 0 0 0))
    (property "Value" "hole" (at 0 0 0))
    (attr board_only)
    (pad "" np_thru_hole circle (at 0 0)))
  (footprint "Resistor_SMD:R_0402_1005Metric" (layer "F.Cu")
    (property "Reference" "R2" (at 0 0 0))
    (property "Value" "1k" (at 0 0 0))
    (pad "1" smd roundrect (at 0 0) (net 2 "N1"))
    (pad "2" smd roundrect (at 1 0))))"#;
        let n = Netlist::from_kicad_pcb(pcb).unwrap();
        assert_eq!(
            n.components
                .iter()
                .map(|c| c.refdes.as_str())
                .collect::<Vec<_>>(),
            ["R1", "R2"]
        );
        assert_eq!(n.nets.len(), 2);
        assert_eq!(n.nets[0].name, "N1");
        assert_eq!(n.nets[0].pins.len(), 2, "R1.1 and R2.1");
        assert_eq!(n.net_of(1, "2"), None, "a pad with no net is unconnected");
        assert!(Netlist::from_kicad_pcb("(kicad_sch)").is_err());
    }
}
