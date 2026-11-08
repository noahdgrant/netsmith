//! A plain-text netlist, one element a line, for islands that are data and not code.
//!
//! ```text
//! # a comment; names are words; GND and 0 are the reference node
//! V  rail   VIN GND  9..16            a rail: a range
//! R  r1     VIN MID  10k+-1%          a resistor: a value, a range or a tolerance
//! I  load   OUT GND  0..1.2           a current drawn from the first node into the second
//! D  d1     A   K    0..0.8           a diode: its forward drop
//! L  u1     PIN VDD GND states=HLZ roh=0..200 rol=0..100 rz=3.3M   a logic pin on a rail
//! M  q1     G D S    n vth=1..2.5 ron=2..5     (n or p channel)
//! REG reg   VIN VOUT GND vout=5+-2% eff=0.9..0.95 [iq=20u] [linear] [vin_min=7]
//! P  load   NODE GND 0..2             a load of constant power
//! ```
//!
//! A number takes an SI prefix (`p n u m k M G`) and may end in its unit (`10kohm`, `3.3V`).
//! A range is `lo..hi`, `value+-5%` or one value.

use crate::{
    netlist::{Kind, Logic, LogicState, Netlist, Regulator},
    range::Range,
};

#[derive(Debug, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// A number with an optional SI prefix and unit: `4.7k`, `50m`, `10kohm`, `1.2V`, `-3u`.
pub fn parse_number(text: &str) -> Result<f64, String> {
    let mut t = text.trim();
    for unit in ["ohm", "Ohm", "V", "A", "W"] {
        if let Some(rest) = t.strip_suffix(unit) {
            t = rest;
            break;
        }
    }
    let (digits, scale) = match t.chars().last() {
        Some('p') => (&t[..t.len() - 1], 1e-12),
        Some('n') => (&t[..t.len() - 1], 1e-9),
        Some('u') => (&t[..t.len() - 1], 1e-6),
        Some('m') => (&t[..t.len() - 1], 1e-3),
        Some('k') => (&t[..t.len() - 1], 1e3),
        Some('M') => (&t[..t.len() - 1], 1e6),
        Some('G') => (&t[..t.len() - 1], 1e9),
        _ => (t, 1.0),
    };
    digits
        .parse::<f64>()
        .map(|v| v * scale)
        .map_err(|_| format!("`{text}` is not a number"))
}

/// `lo..hi`, `value+-5%`, or one value.
pub fn parse_range(text: &str) -> Result<Range, String> {
    if let Some((lo, hi)) = text.split_once("..") {
        return Ok(Range::new(parse_number(lo)?, parse_number(hi)?));
    }
    if let Some((value, tol)) = text.split_once("+-") {
        let tol = tol
            .strip_suffix('%')
            .ok_or_else(|| format!("a tolerance ends in %: `{text}`"))?;
        let fraction = tol
            .parse::<f64>()
            .map_err(|_| format!("`{tol}` is not a percentage"))?
            / 100.0;
        return Ok(Range::tol(parse_number(value)?, fraction));
    }
    parse_number(text).map(Range::point)
}

fn key_values(words: &[&str]) -> (Vec<String>, std::collections::HashMap<String, String>) {
    let mut flags = Vec::new();
    let mut map = std::collections::HashMap::new();
    for w in words {
        match w.split_once('=') {
            Some((k, v)) => {
                map.insert(k.to_string(), v.to_string());
            }
            None => flags.push(w.to_string()),
        }
    }
    (flags, map)
}

pub fn parse(text: &str) -> Result<Netlist, ParseError> {
    let mut nl = Netlist::new();
    for (n, raw) in text.lines().enumerate() {
        let line = n + 1;
        let src = raw.split('#').next().unwrap_or("").trim();
        if src.is_empty() {
            continue;
        }
        let words: Vec<&str> = src.split_whitespace().collect();
        let err = |message: String| ParseError { line, message };
        let need = |count: usize| -> Result<(), ParseError> {
            if words.len() < count {
                Err(err(format!(
                    "`{}` needs {} words, there are {}",
                    words[0],
                    count,
                    words.len()
                )))
            } else {
                Ok(())
            }
        };
        let range = |i: usize| parse_range(words[i]).map_err(err);
        match words[0] {
            "R" => {
                need(5)?;
                let r = range(4)?;
                nl.resistor(words[1], words[2], words[3], r);
            }
            "V" => {
                need(5)?;
                let v = range(4)?;
                nl.vsource(words[1], words[2], words[3], v);
            }
            "I" => {
                need(5)?;
                let i = range(4)?;
                nl.isource(words[1], words[2], words[3], i);
            }
            "D" => {
                need(5)?;
                let vf = range(4)?;
                nl.diode(words[1], words[2], words[3], vf);
            }
            "P" => {
                need(5)?;
                let p = range(4)?;
                nl.power_load(words[1], words[2], words[3], p);
            }
            "L" => {
                need(5)?;
                let (_, kv) = key_values(&words[5..]);
                let get = |k: &str| kv.get(k).map(String::as_str);
                let states: Vec<LogicState> = get("states")
                    .unwrap_or("HLZ")
                    .chars()
                    .map(|c| match c {
                        'H' => Ok(LogicState::High),
                        'L' => Ok(LogicState::Low),
                        'Z' => Ok(LogicState::Z),
                        other => Err(err(format!("a state is H, L or Z, not `{other}`"))),
                    })
                    .collect::<Result<_, _>>()?;
                let roh = get("roh").map(parse_range).transpose().map_err(err)?;
                let rol = get("rol").map(parse_range).transpose().map_err(err)?;
                let rz = get("rz").map(parse_number).transpose().map_err(err)?;
                let node = nl.node(words[2]);
                let vdd = nl.node(words[3]);
                let gnd = nl.node(words[4]);
                nl.add(
                    words[1],
                    Kind::Logic(Logic {
                        node,
                        vdd,
                        gnd,
                        states,
                        roh: roh.unwrap_or(Range::point(0.0)),
                        rol: rol.unwrap_or(Range::point(0.0)),
                        rz: rz.unwrap_or(0.0),
                    }),
                );
            }
            "M" => {
                need(6)?;
                let (flags, kv) = key_values(&words[5..]);
                let get = |k: &str| {
                    kv.get(k)
                        .ok_or_else(|| err(format!("`{}` needs `{k}=`", words[0])))
                        .and_then(|v| parse_range(v).map_err(err))
                };
                let (gate, drain, source) =
                    (nl.node(words[2]), nl.node(words[3]), nl.node(words[4]));
                let channel = flags.first().map(String::as_str).unwrap_or("n");
                nl.add(
                    words[1],
                    Kind::Mosfet {
                        gate,
                        drain,
                        source,
                        p_channel: channel == "p",
                        vth: get("vth")?,
                        ron: get("ron")?,
                    },
                );
            }
            "REG" => {
                need(5)?;
                let (flags, kv) = key_values(&words[5..]);
                let get = |k: &str| kv.get(k).map(|v| parse_range(v).map_err(err)).transpose();
                let linear = flags.iter().any(|f| f == "linear");
                let v = get("vout")?.ok_or_else(|| err("`REG` needs `vout=`".to_string()))?;
                let eff = get("eff")?.unwrap_or(Range::point(1.0));
                if !linear && !kv.contains_key("eff") {
                    return Err(err(
                        "a switching `REG` needs `eff=` (or `linear`)".to_string()
                    ));
                }
                let iq = get("iq")?.map_or(0.0, |r| r.hi);
                let vin_min = get("vin_min")?.map_or(0.0, |r| r.lo);
                let (vin, vout, gnd) = (nl.node(words[2]), nl.node(words[3]), nl.node(words[4]));
                nl.add(
                    words[1],
                    Kind::Regulator(Regulator {
                        vin,
                        vout,
                        gnd,
                        v,
                        eff,
                        iq,
                        linear,
                        vin_min,
                    }),
                );
            }
            other => return Err(err(format!("`{other}` is not an element letter"))),
        }
    }
    Ok(nl)
}
