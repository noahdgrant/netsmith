//! The three example boards as the design stage produces them, so these tests do not need the
//! whole pipeline.

#![allow(dead_code)]

pub mod bench;
pub mod bus;
pub mod corpus;
pub mod mcu;
pub mod rowcol;

use netsmith_core::{Design, Net, Part, PartPin, PinKind, PinRef, PowerFlow};

/// What the design stage records as the generics a standard kind provides: a fixture that
/// names a kind of the standard library gives it the same, so that the layout finds the
/// connectors the way it does on a real board (`Connector`, ADR 0068).
pub fn provides_of(kind: &str) -> Vec<String> {
    let connector = kind.starts_with("Header")
        || kind.starts_with("Socket")
        || kind.starts_with("UsbC")
        || matches!(kind, "Coax" | "Obd2M" | "ProgrammingPads6");
    if connector {
        vec![netsmith_core::CONNECTOR_GENERIC.to_string()]
    } else {
        Vec::new()
    }
}

pub fn design(
    name: &str,
    parts: &[(&str, &str, &str, Option<&str>)],
    nets: &[(&str, &[&str])],
) -> Design {
    Design {
        modules: Vec::new(),
        name: name.into(),
        process: None,
        features: Default::default(),
        interfaces: Vec::new(),
        notes: Vec::new(),
        parts: parts
            .iter()
            .map(|&(refdes, kind, value, package)| Part {
                provides: provides_of(kind),
                refdes: refdes.into(),
                kind: kind.into(),
                value: value.into(),
                package: package.map(str::to_string),
                path: String::new(),
                pins: Vec::new(),
            })
            .collect(),
        nets: nets
            .iter()
            .map(|&(name, pins)| Net {
                name: name.into(),
                pins: pins
                    .iter()
                    .map(|p| {
                        let (refdes, pin) = p.split_once('.').unwrap();
                        PinRef {
                            refdes: refdes.into(),
                            pin: pin.into(),
                        }
                    })
                    .collect(),
            })
            .collect(),
    }
}

pub fn led() -> Design {
    design(
        "LedExample",
        &[
            ("D1", "Led", "LED", None),
            ("J1", "Header1x2", "Conn_01x02", None),
            ("R1", "Resistor", "300", Some("0402")),
        ],
        &[
            ("GND", &["D1.1", "J1.2"]),
            ("VCC", &["J1.1", "R1.1"]),
            ("net-led.anode", &["D1.2", "R1.2"]),
        ],
    )
}

pub fn status_indicator() -> Design {
    design(
        "IndicatorExample",
        &[
            ("J1", "Header1x2", "Conn_01x02", None),
            ("J2", "Header1x2", "Conn_01x02", None),
            ("C1", "Capacitor", "100n", Some("0402")),
            ("R1", "Resistor", "1k", Some("0402")),
            ("D1", "Led", "LED", None),
        ],
        &[
            ("net-enable_in.p1", &["J1.1", "R1.1"]),
            ("net-enable_in.p2", &["C1.2", "D1.1", "J1.2", "J2.2"]),
            ("net-power_in.p1", &["C1.1", "J2.1"]),
            ("net-status_indicator.current_limit.b", &["D1.2", "R1.2"]),
        ],
    )
}

/// The pin table of a `LinearRegulator` as the design stage produces it for a solved board:
/// the datasheet names of `std/parts/regulator.nsm` (pad 3 the input, 2 the output, 1 the
/// ground), the input taking power and the output giving it. A part with a row of the standard
/// mapping that names no symbol is drawn from this table (ADR 0097).
pub fn linear_regulator_pins() -> Vec<PartPin> {
    let pin = |name: &str, datasheet: &str, pad: &str, flow| PartPin {
        name: name.into(),
        datasheet: datasheet.into(),
        kind: PinKind::Power,
        pads: vec![pad.into()],
        function: None,
        power_flow: flow,
    };
    vec![
        pin("vin.pos", "IN", "3", Some(PowerFlow::Takes)),
        pin("vin.neg", "GND", "1", None),
        pin("vout.pos", "OUT", "2", Some(PowerFlow::Gives)),
    ]
}

pub fn regulator() -> Design {
    let mut d = design(
        "RegulatorExample",
        &[
            ("J1", "Header1x2", "Conn_01x02", None),
            ("J2", "Header1x2", "Conn_01x02", None),
            ("C1", "Capacitor", "1u", Some("0402")),
            ("C2", "Capacitor", "4.7u", Some("0402")),
            ("U1", "LinearRegulator", "LDO-3.3V", Some("SOT-223")),
        ],
        &[
            ("3V3", &["C2.1", "J2.1", "U1.2"]),
            ("GND", &["C1.2", "C2.2", "J1.2", "J2.2", "U1.1"]),
            ("VIN", &["C1.1", "J1.1", "U1.3"]),
        ],
    );
    for part in &mut d.parts {
        if part.kind == "LinearRegulator" {
            part.pins = linear_regulator_pins();
        }
    }
    d
}

/// `modules` copies of a ten-part supply: a regulator, four capacitors, four resistors and a
/// light, on a shared `GND` and `VIN`, each with its own nets besides.
pub fn supplies(modules: usize) -> (Design, Vec<netsmith_view::Group>) {
    let mut parts = Vec::new();
    let mut nets: Vec<Net> = Vec::new();
    let mut groups = Vec::new();
    let pin = |r: &str, p: &str| PinRef {
        refdes: r.into(),
        pin: p.into(),
    };
    let mut gnd = Vec::new();
    let mut vin = Vec::new();
    for m in 1..=modules {
        let first = parts.len();
        let name = |k: &str| format!("{k}{m}");
        for (refdes, kind, value, package) in [
            (name("U"), "LinearRegulator", "LDO", Some("SOT-223")),
            (name("C"), "Capacitor", "1u", Some("0402")),
            (name("CA"), "Capacitor", "4.7u", Some("0402")),
            (name("CB"), "Capacitor", "100n", Some("0402")),
            (name("CC"), "Capacitor", "100n", Some("0402")),
            (name("R"), "Resistor", "1k", Some("0402")),
            (name("RA"), "Resistor", "10k", Some("0402")),
            (name("RB"), "Resistor", "10k", Some("0402")),
            (name("RC"), "Resistor", "2k", Some("0402")),
            (name("D"), "Led", "LED", None),
        ] {
            parts.push(Part {
                provides: provides_of(kind),
                refdes,
                kind: kind.into(),
                value: value.into(),
                package: package.map(str::to_string),
                path: String::new(),
                pins: Vec::new(),
            });
        }
        groups.push(netsmith_view::Group {
            name: format!("supply{m}"),
            detail: "Supply".into(),
            parts: (first..first + 10).collect(),
        });
        gnd.extend([
            pin(&name("U"), "1"),
            pin(&name("C"), "2"),
            pin(&name("CA"), "2"),
        ]);
        gnd.extend([
            pin(&name("CB"), "2"),
            pin(&name("CC"), "2"),
            pin(&name("D"), "1"),
        ]);
        vin.extend([pin(&name("U"), "3"), pin(&name("C"), "1")]);
        nets.push(Net {
            name: format!("out{m}"),
            pins: vec![
                pin(&name("U"), "2"),
                pin(&name("CA"), "1"),
                pin(&name("R"), "1"),
                pin(&name("CB"), "1"),
            ],
        });
        nets.push(Net {
            name: format!("sense{m}"),
            pins: vec![
                pin(&name("R"), "2"),
                pin(&name("RA"), "1"),
                pin(&name("RB"), "1"),
                pin(&name("CC"), "1"),
            ],
        });
        nets.push(Net {
            name: format!("led{m}"),
            pins: vec![pin(&name("RA"), "2"), pin(&name("RC"), "1")],
        });
        nets.push(Net {
            name: format!("anode{m}"),
            pins: vec![pin(&name("RC"), "2"), pin(&name("D"), "2")],
        });
        nets.push(Net {
            name: format!("tap{m}"),
            pins: vec![pin(&name("RB"), "2")],
        });
    }
    // Each supply's output also feeds the next supply's resistor: a net between sheets.
    for m in 1..modules {
        nets.iter_mut()
            .find(|n| n.name == format!("out{m}"))
            .unwrap()
            .pins
            .push(pin(&format!("RB{}", m + 1), "2"));
        nets.retain(|n| n.name != format!("tap{}", m + 1));
    }
    nets.push(Net {
        name: "GND".into(),
        pins: gnd,
    });
    nets.push(Net {
        name: "VIN".into(),
        pins: vin,
    });
    (
        Design {
            modules: Vec::new(),
            name: "Big".into(),
            parts,
            nets,
            process: None,
            features: Default::default(),
            interfaces: Vec::new(),
            notes: Vec::new(),
        },
        groups,
    )
}

pub fn all() -> Vec<Design> {
    vec![led(), status_indicator(), regulator()]
}

/// Whether tags open and close in matching pairs. Enough of an XML reader for the SVG
/// and HTML this crate writes: no CDATA, no comments inside attribute values.
pub fn balanced(doc: &str) -> Result<(), String> {
    let mut stack: Vec<String> = Vec::new();
    let mut rest = doc;
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let end = rest.find('>').ok_or("a tag is never closed")?;
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        if tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            match stack.pop() {
                Some(open) if open == name.trim() => {}
                other => return Err(format!("</{name}> closes {other:?}")),
            }
        } else if !tag.ends_with('/') {
            let name = tag.split_whitespace().next().unwrap_or("");
            if matches!(name, "meta" | "link" | "br" | "hr" | "img" | "input") {
                continue;
            }
            // `<style>` and `<script>` hold text that may contain `<`; skip it whole.
            if name == "style" || name == "script" {
                let close = format!("</{name}>");
                let at = rest.find(&close).ok_or(format!("<{name}> never closes"))?;
                rest = &rest[at + close.len()..];
                continue;
            }
            stack.push(name.to_string());
        }
    }
    if stack.is_empty() {
        Ok(())
    } else {
        Err(format!("unclosed: {stack:?}"))
    }
}

fn on_segment(
    p: netsmith_view::geom::Pt,
    a: netsmith_view::geom::Pt,
    b: netsmith_view::geom::Pt,
) -> bool {
    let (x0, x1) = (a.x.min(b.x) - 1e-6, a.x.max(b.x) + 1e-6);
    let (y0, y1) = (a.y.min(b.y) - 1e-6, a.y.max(b.y) + 1e-6);
    p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1
}

/// Whether the wires of `net` join all of `points` into one piece (a wire's end on another
/// wire is a join).
pub fn wired_together(
    l: &netsmith_view::Layout,
    net: usize,
    points: &[netsmith_view::geom::Pt],
) -> bool {
    use netsmith_view::geom::Pt;
    let segments: Vec<(Pt, Pt)> = l
        .wires
        .iter()
        .filter(|w| w.net == net)
        .flat_map(|w| {
            w.points
                .windows(2)
                .map(|s| (s[0], s[1]))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut group: Vec<usize> = (0..segments.len()).collect();
    fn find(g: &mut [usize], i: usize) -> usize {
        if g[i] != i {
            let r = find(g, g[i]);
            g[i] = r;
        }
        g[i]
    }
    for i in 0..segments.len() {
        for j in 0..i {
            let (a, b) = (segments[i], segments[j]);
            if [a.0, a.1].iter().any(|&p| on_segment(p, b.0, b.1))
                || [b.0, b.1].iter().any(|&p| on_segment(p, a.0, a.1))
            {
                let (ri, rj) = (find(&mut group, i), find(&mut group, j));
                group[ri] = rj;
            }
        }
    }
    let Some(first) = points
        .first()
        .and_then(|p| segments.iter().position(|&(a, b)| on_segment(*p, a, b)))
    else {
        return false;
    };
    let root = find(&mut group, first);
    points.iter().all(|p| {
        segments
            .iter()
            .enumerate()
            .any(|(i, &(a, b))| on_segment(*p, a, b) && find(&mut group, i) == root)
    })
}
