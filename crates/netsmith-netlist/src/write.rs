//! Writing a netlist as KiCad's netlist XML, the format [`Netlist::from_kicad_xml`] reads (#560):
//! a port of a board too large to commit holds its netlist in this form.

use std::fmt::Write as _;

use crate::model::Netlist;

/// `text` with the five characters XML gives a meaning written as entities.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

impl Netlist {
    /// The netlist as the XML `kicad-cli sch export netlist --format kicadxml` writes, with what
    /// [`Netlist::from_kicad_xml`] reads: each component with its value and footprint, in order,
    /// and each net with its nodes, in order. `source` is the file the netlist was read from, in
    /// `<design><source>`. Reading the text gives this netlist back, except that the reader trims
    /// the spaces around a value or a footprint.
    pub fn to_kicad_xml(&self, source: &str) -> String {
        let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        out.push_str("<export version=\"E\">\n");
        let _ = writeln!(
            out,
            "  <design><source>{}</source></design>",
            escaped(source)
        );
        out.push_str("  <components>\n");
        for c in &self.components {
            let _ = writeln!(
                out,
                "    <comp ref=\"{}\"><value>{}</value><footprint>{}</footprint></comp>",
                escaped(&c.refdes),
                escaped(&c.value),
                escaped(&c.footprint)
            );
        }
        out.push_str("  </components>\n  <nets>\n");
        for (i, net) in self.nets.iter().enumerate() {
            let _ = writeln!(
                out,
                "    <net code=\"{}\" name=\"{}\">",
                i + 1,
                escaped(&net.name)
            );
            for (c, pin) in &net.pins {
                let _ = writeln!(
                    out,
                    "      <node ref=\"{}\" pin=\"{}\"/>",
                    escaped(&self.components[*c].refdes),
                    escaped(pin)
                );
            }
            out.push_str("    </net>\n");
        }
        out.push_str("  </nets>\n</export>\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Netlist;

    fn netlist() -> Netlist {
        let s = |t: &str| t.to_string();
        Netlist::from_parts(
            vec![
                (s("R1"), s("4k7"), s("Resistor_SMD:R_0402_1005Metric")),
                (s("U1"), s("A&B <\"x\"> 'y'"), s("Lib:SOT-23")),
                (s("H1"), s("hole"), s("MountingHole:MountingHole_3.2mm")),
                (s("D1"), s(""), s("")),
            ],
            vec![
                (s("+3V3"), vec![(s("U1"), s("3")), (s("R1"), s("1"))]),
                (
                    s("/sheet/Net-(R1-Pad2)&<>"),
                    vec![(s("R1"), s("2")), (s("U1"), s("A1")), (s("D1"), s("1"))],
                ),
                (s("GND"), vec![(s("U1"), s("2"))]),
            ],
        )
        .unwrap()
    }

    #[test]
    fn reading_what_is_written_gives_the_same_netlist() {
        let n = netlist();
        let xml = n.to_kicad_xml("board & co.kicad_pcb");
        assert_eq!(Netlist::from_kicad_xml(&xml).unwrap(), n, "{xml}");
        assert!(
            xml.contains("<source>board &amp; co.kicad_pcb</source>"),
            "{xml}"
        );
        // A part with no pin on a net (a hole) is a component of the XML too.
        assert!(xml.contains("<comp ref=\"H1\">"), "{xml}");
    }

    #[test]
    fn a_board_read_and_written_reads_back_the_same() {
        let pcb = r#"(kicad_pcb (version 20241229)
  (footprint "Resistor_SMD:R_0402_1005Metric" (layer "F.Cu")
    (property "Reference" "R1") (property "Value" "330")
    (pad "1" smd roundrect (net 2 "N1")) (pad "2" smd roundrect (net 1 "GND")))
  (footprint "Package_TO_SOT_SMD:SOT-23" (layer "F.Cu")
    (property "Reference" "U1") (property "Value" "TLV<1>")
    (pad "1" smd rect (net 2 "N1")) (pad "2" smd rect (net 1 "GND")) (pad "3" smd rect)))"#;
        let n = Netlist::from_kicad_pcb(pcb).unwrap();
        let back = Netlist::from_kicad_xml(&n.to_kicad_xml("x.kicad_pcb")).unwrap();
        assert_eq!(back, n);
    }

    #[test]
    fn an_empty_netlist_is_written_and_read() {
        let n = Netlist::default();
        assert_eq!(Netlist::from_kicad_xml(&n.to_kicad_xml("")).unwrap(), n);
    }
}
