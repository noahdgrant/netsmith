//! The text of a comparison.

use crate::graph::{Correspondence, Difference, Outcome, Pin, Side};

/// What to tell a person: that the netlists match, with what corresponds to what where the
/// names differ, or each difference.
pub fn report(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Match(c) => matched(c),
        Outcome::Different(d) => different(d),
        Outcome::Undecided(why) => format!("undecided: {why}\n"),
    }
}

fn matched(c: &Correspondence) -> String {
    let mut out = format!(
        "match: {} components and {} nets correspond\n",
        c.components.len(),
        c.nets.len()
    );
    let renumbered: Vec<_> = c.components.iter().filter(|(a, b)| a != b).collect();
    if renumbered.is_empty() {
        out.push_str("  every component has the same designator on both sides\n");
    } else {
        out.push_str("  designators (A -> B):\n");
        for (a, b) in renumbered {
            out.push_str(&format!("    {a} -> {b}\n"));
        }
    }
    let renamed: Vec<_> = c.nets.iter().filter(|(a, b)| a != b).collect();
    if renamed.is_empty() {
        out.push_str("  every net has the same name on both sides\n");
    } else {
        out.push_str("  nets (A -> B):\n");
        for (a, b) in renamed {
            out.push_str(&format!("    `{a}` -> `{b}`\n"));
        }
    }
    if c.interchangeable > 0 {
        out.push_str(&format!(
            "  note: {} choices were made between interchangeable parts (equal parts wired alike), so those designators could be paired the other way round\n",
            c.interchangeable
        ));
    }
    out
}

fn pins(pins: &[Pin]) -> String {
    pins.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn different(differences: &[Difference]) -> String {
    let mut out = format!(
        "different: {} difference{}\n",
        differences.len(),
        if differences.len() == 1 { "" } else { "s" }
    );
    for d in differences {
        match d {
            Difference::ComponentOnly {
                side,
                refdes,
                value,
                footprint,
                pins,
            } => {
                let other = if *side == Side::A { Side::B } else { Side::A };
                out.push_str(&format!(
                    "component only in {side}: {refdes} ({value}, {footprint}) has no counterpart in {other}"
                ));
                if pins.is_empty() {
                    out.push_str("; no pin is connected\n");
                } else {
                    let list: Vec<String> = pins
                        .iter()
                        .map(|(p, n)| format!("pin {p} on `{n}`"))
                        .collect();
                    out.push_str(&format!("; {}\n", list.join(", ")));
                }
            }
            Difference::ComponentClass { a, b } => {
                out.push_str(&format!(
                    "component {} (A) and {} (B) correspond but differ: A is {} in {}, B is {} in {}\n",
                    a.0, b.0, a.1, a.2, b.1, b.2
                ));
            }
            Difference::NetPins {
                a,
                b,
                only_a,
                only_b,
            } => {
                out.push_str(&format!(
                    "net `{a}` (A) and `{b}` (B) correspond but their pins differ\n"
                ));
                for (side, list) in [(Side::A, only_a), (Side::B, only_b)] {
                    for (pin, elsewhere) in list {
                        let note = if elsewhere.is_empty() {
                            String::new()
                        } else {
                            format!(" ({elsewhere})")
                        };
                        out.push_str(&format!("  only in {side}: {pin}{note}\n"));
                    }
                }
            }
            Difference::NetOnly {
                side,
                name,
                pins: p,
            } => {
                let other = if *side == Side::A { Side::B } else { Side::A };
                out.push_str(&format!(
                    "net only in {side}: `{name}` ({}) has no counterpart in {other}\n",
                    pins(p)
                ));
            }
            Difference::Unlocated(m) => out.push_str(&format!("{m}\n")),
        }
    }
    out
}
