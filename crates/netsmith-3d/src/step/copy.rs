//! The entities of a part's STEP model copied into the export (ADR 0108: "The STEP export needs
//! no tessellation"): a solid's instances, everything they refer to and the styles that colour
//! them, renumbered in the order of their ids in the model's file and written with the values
//! the file has, so that the export holds the model exactly.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use super::read::Exchange;
use super::read::part21::{Instance, Param};
use super::writer::{Id, Writer, string};

/// Every instance `root` refers to, directly or not, and `root`; then every style of one of
/// them (`STYLED_ITEM` and `OVER_RIDING_STYLED_ITEM`) with what it refers to. The styles are
/// the second list.
pub(super) fn closure(ex: &Exchange, root: u64) -> (BTreeSet<u64>, Vec<u64>) {
    let mut seen = BTreeSet::new();
    reach(ex, root, &mut seen);
    let mut styles = Vec::new();
    for (&id, inst) in &ex.data {
        let item = ["STYLED_ITEM", "OVER_RIDING_STYLED_ITEM"]
            .iter()
            .find_map(|name| inst.part(name))
            .and_then(|e| e.params.get(2))
            .and_then(Param::as_ref);
        if item.is_some_and(|i| seen.contains(&i)) {
            styles.push(id);
        }
    }
    for &s in &styles {
        reach(ex, s, &mut seen);
    }
    (seen, styles)
}

fn reach(ex: &Exchange, id: u64, seen: &mut BTreeSet<u64>) {
    let mut todo = vec![id];
    while let Some(id) = todo.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(inst) = ex.data.get(&id) {
            for e in &inst.parts {
                refs(&e.params, &mut todo);
            }
        }
    }
}

fn refs(params: &[Param], out: &mut Vec<u64>) {
    for p in params {
        match p {
            Param::Ref(r) => out.push(*r),
            Param::List(v) | Param::Typed(_, v) => refs(v, out),
            _ => {}
        }
    }
}

/// Writes the instances `ids` of `ex` into `w`, in their order, each with its references
/// renumbered; returns the new id of each.
pub(super) fn copy(ex: &Exchange, ids: &BTreeSet<u64>, w: &mut Writer) -> BTreeMap<u64, Id> {
    let base = w.next_id().0;
    let map: BTreeMap<u64, Id> = ids
        .iter()
        .enumerate()
        .map(|(k, &id)| (id, Id(base + k)))
        .collect();
    for id in ids {
        // The reader has refused a model that refers to an instance it does not have, so every
        // id is there.
        let text = ex
            .data
            .get(id)
            .map_or_else(String::new, |i| instance(i, &map));
        w.add(text);
    }
    map
}

/// An instance's text with its references renumbered by `map`.
fn instance(inst: &Instance, map: &BTreeMap<u64, Id>) -> String {
    let mut out = String::new();
    let complex = inst.is_complex();
    if complex {
        out.push('(');
    }
    for e in &inst.parts {
        out.push_str(&e.name);
        params(&e.params, map, &mut out);
    }
    if complex {
        out.push(')');
    }
    out
}

fn params(ps: &[Param], map: &BTreeMap<u64, Id>, out: &mut String) {
    out.push('(');
    for (k, p) in ps.iter().enumerate() {
        if k > 0 {
            out.push(',');
        }
        param(p, map, out);
    }
    out.push(')');
}

fn param(p: &Param, map: &BTreeMap<u64, Id>, out: &mut String) {
    match p {
        Param::Null => out.push('$'),
        Param::Derived => out.push('*'),
        Param::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Param::Real(x) => out.push_str(&exact(*x)),
        Param::Str(s) => out.push_str(&string(s)),
        Param::Enum(e) => {
            let _ = write!(out, ".{e}.");
        }
        Param::Binary(b) => {
            let _ = write!(out, "\"{b}\"");
        }
        Param::Ref(r) => match map.get(r) {
            Some(id) => {
                let _ = write!(out, "{id}");
            }
            None => out.push('$'),
        },
        Param::List(v) => params(v, map, out),
        Param::Typed(name, v) => {
            out.push_str(name);
            params(v, map, out);
        }
    }
}

/// A real as STEP writes one, with every digit it needs to be read back as the same `f64`
/// (`0.1`, `1.`, `1.E-07`): the shortest that round-trips, as Rust prints it.
pub(super) fn exact(x: f64) -> String {
    let x = if x == 0.0 { 0.0 } else { x };
    let s = format!("{x:?}");
    let (mantissa, exponent) = match s.split_once('e') {
        Some((m, e)) => (m.to_string(), Some(e.to_string())),
        None => (s, None),
    };
    let mut mantissa = if mantissa.contains('.') {
        mantissa
    } else {
        mantissa + "."
    };
    if mantissa.ends_with(".0") {
        mantissa.pop();
    }
    match exponent {
        Some(e) => format!("{mantissa}E{e}"),
        None => mantissa,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_real_is_written_to_be_read_back_the_same() {
        for (x, s) in [
            (0.0, "0."),
            (-0.0, "0."),
            (1.0, "1."),
            (0.1, "0.1"),
            (-2.5, "-2.5"),
            (1e-7, "1.E-7"),
            (1.5e21, "1.5E21"),
        ] {
            assert_eq!(exact(x), s);
        }
        for x in [0.1 + 0.2, 1.0 / 3.0, 25.4, 1e-300, 123456.789] {
            let text = exact(x).replace("E", "e");
            assert_eq!(text.parse::<f64>().unwrap(), x);
        }
    }
}
