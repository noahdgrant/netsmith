//! The three example boards as the lowering produces them, so these tests do not need the
//! whole pipeline.

#![allow(dead_code)]

pub mod bench;

use netsmith_core::{Design, Net, Part, PinRef};

pub fn design(
    name: &str,
    parts: &[(&str, &str, &str, Option<&str>)],
    nets: &[(&str, &[&str])],
) -> Design {
    Design {
        name: name.into(),
        process: None,
        features: Default::default(),
        parts: parts
            .iter()
            .map(|&(refdes, kind, value, package)| Part {
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

pub fn regulator() -> Design {
    design(
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
